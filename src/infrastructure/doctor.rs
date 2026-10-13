//! 利用条件を読み取り専用で診断する。権限の要求・録音・入力・設定変更は行わない。

use super::{
    audio::CpalAudioBackend, config::AppConfig, config::diagnostic_data_dir, dict, push_to_talk,
};
use crate::ipc::{IpcCmd, send_cmd_with_timeout};
use crate::utils::config::{EnvConfig, PathConfig, TranscriptionProvider};
use serde::{Deserialize, Serialize};
use std::{path::Path, time::Duration};

const DEPENDENCY_TIMEOUT: Duration = Duration::from_secs(8);
const IPC_TIMEOUT: Duration = Duration::from_secs(12);

/// 診断項目の状態。未確認は成功扱いしない。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckStatus {
    /// 利用条件を満たす。
    Ok,
    /// 必須条件を満たさない。
    Fail,
    /// 補助機能などに注意が必要。
    Warn,
    /// 確認できない。
    Unknown,
    /// 現在の設定では不要。
    Skip,
}

/// 状態と、必要な場合の対処方法。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorCheck {
    /// 確認対象。
    pub name: String,
    /// 確認結果。
    pub status: CheckStatus,
    /// 観測した状態。
    pub detail: String,
    /// 問題解決の手順。不要な場合は空文字列。
    pub action: String,
}

/// CLIとデーモンの診断結果。
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DoctorReport {
    /// 全項目の結果。
    pub checks: Vec<DoctorCheck>,
}

impl DoctorReport {
    fn add(&mut self, name: &str, status: CheckStatus, detail: impl Into<String>, action: &str) {
        self.checks.push(DoctorCheck {
            name: name.to_string(),
            status,
            detail: detail.into(),
            action: action.to_string(),
        });
    }

    /// すべての必須条件を確認できたかを返す。
    pub fn is_ready(&self) -> bool {
        !self.checks.is_empty()
            && self
                .checks
                .iter()
                .all(|check| !matches!(check.status, CheckStatus::Fail | CheckStatus::Unknown))
    }

    /// 一覧と対処方法を端末表示用に整形する。
    pub fn render(&self) -> String {
        let summary = if self
            .checks
            .iter()
            .any(|check| check.status == CheckStatus::Fail)
        {
            "利用できません（要対応）"
        } else if !self.is_ready() {
            "利用可否を確認できません"
        } else if self
            .checks
            .iter()
            .any(|check| check.status == CheckStatus::Warn)
        {
            "利用可能（事前確認・注意あり）"
        } else {
            "利用可能（事前確認）"
        };
        let mut lines = vec![format!("Voice Input doctor: {summary}"), String::new()];
        for check in &self.checks {
            let label = match check.status {
                CheckStatus::Ok => "OK",
                CheckStatus::Fail => "FAIL",
                CheckStatus::Warn => "WARN",
                CheckStatus::Unknown => "未確認",
                CheckStatus::Skip => "対象外",
            };
            lines.push(format!("[{label}] {}: {}", check.name, check.detail));
            if !check.action.is_empty() {
                lines.push(format!("  対処: {}", check.action));
            }
        }
        lines.push(String::new());
        lines.push("診断は設定・権限・依存先の事前確認です。実録音、文字起こし、カーソルへの入力は行いません。".to_string());
        lines.join("\n")
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub(crate) struct DaemonDiagnosis {
    pub report: DoctorReport,
    pub config: AppConfig,
    pub config_path: std::path::PathBuf,
}

/// デーモン停止時や設定不備のときも診断を続けるCLIの入口。
pub fn run() -> DoctorReport {
    let mut report = DoctorReport::default();
    let config_path = diagnostic_data_dir().map(|dir| dir.join("config.json"));
    let config = config_path
        .as_ref()
        .map_err(|error| error.to_string())
        .and_then(|path| AppConfig::load_for_diagnosis(path).map_err(|error| error.to_string()));
    match &config {
        Ok(config) => {
            let path = config_path.as_ref().expect("resolved config path");
            let detail = if path.exists() {
                path.display().to_string()
            } else {
                format!("{}（未作成・既定値を使用）", path.display())
            };
            match EnvConfig::try_with_user_settings(&config.user_setting_overrides()) {
                Ok(_) => report.add("設定", CheckStatus::Ok, detail, ""),
                Err(error) => report.add(
                    "設定",
                    CheckStatus::Fail,
                    format!("{detail}: {error}"),
                    "config.jsonと実行時環境変数の不正な値を修正してください。",
                ),
            }
            check_hotkey(&mut report, config);
            if let Some(parent) = path.parent() {
                let dictionary_path = config
                    .dict_path
                    .as_ref()
                    .map(std::path::PathBuf::from)
                    .unwrap_or_else(|| parent.join("dictionary.json"));
                check_dictionary(&mut report, &dictionary_path);
            }
        }
        Err(error) => report.add(
            "設定",
            CheckStatus::Fail,
            format!("config.jsonを読み込めません: {error}"),
            "config.jsonのJSON形式、ファイル権限、シンボリックリンク先を確認してください。",
        ),
    }

    check_installation(&mut report);
    let socket = PathConfig::from_env().ipc_socket_path();
    match send_cmd_with_timeout(&IpcCmd::Doctor, &socket, IPC_TIMEOUT) {
        Ok(response) => {
            match serde_json::from_str::<DaemonDiagnosis>(&response.msg) {
                Ok(diagnosis) => {
                    report.add(
                        "デーモン",
                        CheckStatus::Ok,
                        format!("接続済み / {}", socket.display()),
                        "",
                    );
                    if let (Ok(config), Ok(path)) = (&config, &config_path) {
                        let same_path = paths_equal(path, &diagnosis.config_path);
                        let same_settings = effective_settings_equal(config, &diagnosis.config);
                        report.add("設定の反映", if same_path && same_settings { CheckStatus::Ok } else { CheckStatus::Fail },
                        format!("daemon設定: {} / {}", diagnosis.config_path.display(),
                            if same_path && same_settings { "反映済み" } else { "CLIの設定と不一致" }),
                        if same_path && same_settings { "" } else {
                            "CLIとdaemonのXDG_DATA_HOMEを揃え、処理完了後にdaemonを再起動してください。"
                        });
                    }
                    if diagnosis.report.checks.is_empty()
                        || (!response.ok && diagnosis.report.is_ready())
                    {
                        report.add(
                            "デーモン診断",
                            CheckStatus::Fail,
                            "daemonの診断結果が空、または診断失敗を返しました",
                            "daemonログを確認してください。",
                        );
                    }
                    report.checks.extend(diagnosis.report.checks);
                }
                Err(_) => {
                    report.add("デーモン診断", CheckStatus::Fail, "診断応答を解釈できません",
                    "doctor対応版のCLIとdaemonを同時にビルド・配置し、daemonを再起動してください。");
                    add_unconfirmed(&mut report);
                }
            }
        }
        Err(error) => {
            report.add("デーモン", CheckStatus::Fail, format!("{} / {error}", socket.display()),
                "VoiceInput.appを未配置なら scripts/setup-app-bundle.sh と scripts/build-app-bundle.sh を実行してください。旧版daemonも scripts/build-app-bundle.sh で更新が必要です。更新済みなら scripts/restart-app-bundle.sh で再起動し、daemonログとVOICE_INPUT_SOCKET_PATH / VOICE_INPUT_SOCKET_DIRの一致を確認してください。");
            add_unconfirmed(&mut report);
        }
    }
    report
}

fn add_unconfirmed(report: &mut DoctorReport) {
    for name in [
        "音声入力",
        "入力ストリーム",
        "直接入力ワーカー",
        "マイク権限",
        "アクセシビリティ権限",
        "入力監視権限",
        "転写先",
    ] {
        report.add(name, CheckStatus::Unknown, "daemonから確認できません", "daemon起動後にvoice_input doctorを再実行してください。権限はシステム設定 → プライバシーとセキュリティでVoiceInput.appを確認できます。");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    /// ローカル転写先だけを確認し、コマンドの標準出力・標準エラーにある情報は表示しない
    #[tokio::test]
    async fn local_provider_does_not_probe_openai_or_print_command_output() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("asr");
        std::fs::write(&path, "#!/bin/sh\necho PRIVATE_TEST_TOKEN >&2\nexit 0\n").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        let mut config = {
            let _guard = crate::utils::config::lock_test_env();
            EnvConfig::try_from_env().unwrap()
        };
        config.transcription.provider = TranscriptionProvider::MlxQwen3Asr;
        config.transcription.mlx_qwen3_asr_command = path.to_string_lossy().into_owned();
        let mut report = DoctorReport::default();
        check_transcription(&mut report, &config).await;
        assert!(report.is_ready());
        assert!(!report.render().contains("OpenAI"));
        assert!(!report.render().contains("PRIVATE_TEST_TOKEN"));
    }

    /// 設定値を明示的に既定値へ保存しただけではdaemonを未反映と判定しない
    #[test]
    fn explicit_default_settings_match_implicit_defaults() {
        let implicit = AppConfig::default();
        let explicit = AppConfig {
            max_secs: Some(implicit.effective_max_secs()),
            ..AppConfig::default()
        };
        assert!(effective_settings_equal(&implicit, &explicit));
    }
}

fn check_dictionary(report: &mut DoctorReport, path: &Path) {
    match dict::validate_read_only(path) {
        Ok(()) => report.add(
            "辞書",
            CheckStatus::Ok,
            format!(
                "{}{}",
                path.display(),
                if path.exists() {
                    ""
                } else {
                    "（未作成・空の辞書）"
                }
            ),
            "",
        ),
        Err(error) => report.add(
            "辞書",
            CheckStatus::Fail,
            format!("{} / {error}", path.display()),
            "辞書のJSON形式・version・読込権限・リンク先を修正してください。",
        ),
    }
}

fn check_hotkey(report: &mut DoctorReport, config: &AppConfig) {
    if !config.effective_push_to_talk_enabled() {
        report.add("Push-to-talk", CheckStatus::Skip, "無効", "");
    } else {
        let hotkey = config.effective_push_to_talk_hotkey();
        match push_to_talk::validate_hotkey(&hotkey) {
            Ok(()) => report.add("Push-to-talk", CheckStatus::Ok, hotkey, ""),
            Err(error) => report.add(
                "Push-to-talk",
                CheckStatus::Fail,
                error,
                "voice_input config set push_to_talk_hotkey opt+8 などで修正してください。",
            ),
        }
    }
}

fn paths_equal(a: &Path, b: &Path) -> bool {
    a == b || matches!((a.canonicalize(), b.canonicalize()), (Ok(a), Ok(b)) if a == b)
}

fn effective_settings_equal(a: &AppConfig, b: &AppConfig) -> bool {
    a.dict_path == b.dict_path
        && a.effective_transcription_provider() == b.effective_transcription_provider()
        && a.effective_max_secs() == b.effective_max_secs()
        && a.effective_pre_roll_ms() == b.effective_pre_roll_ms()
        && a.effective_input_device_priorities() == b.effective_input_device_priorities()
        && a.effective_recording_sounds_enabled() == b.effective_recording_sounds_enabled()
        && a.effective_recording_hud_enabled() == b.effective_recording_hud_enabled()
        && a.effective_push_to_talk_enabled() == b.effective_push_to_talk_enabled()
        && a.effective_push_to_talk_hotkey() == b.effective_push_to_talk_hotkey()
        && a.effective_transcribe_streaming() == b.effective_transcribe_streaming()
}

fn check_installation(report: &mut DoctorReport) {
    #[cfg(target_os = "macos")]
    if let Some((app, agent)) = crate::utils::config::doctor_installation_paths() {
        for (name, path) in [
            (
                "app bundleのdaemon",
                app.join("Contents/MacOS/voice_inputd"),
            ),
            ("LaunchAgent定義", agent),
        ] {
            report.add(
                name,
                if path.is_file() {
                    CheckStatus::Ok
                } else {
                    CheckStatus::Warn
                },
                format!(
                    "{} / {}",
                    path.display(),
                    if path.is_file() {
                        "配置済み"
                    } else {
                        "未配置"
                    }
                ),
                if path.is_file() {
                    ""
                } else {
                    "ログイン時の自動起動にはapp bundleのセットアップが必要です。"
                },
            );
        }
    }
}

/// デーモン自身の設定・権限・依存先を診断する。
pub(crate) async fn diagnose_daemon(
    state: &str,
    input_status: Result<String, crate::application::AudioBackendError>,
) -> DaemonDiagnosis {
    let mut report = DoctorReport::default();
    let config = AppConfig::load_runtime();
    let env = EnvConfig::get();
    report.add(
        "実行状態",
        CheckStatus::Ok,
        format!(
            "pid={} / {state} / provider={} / max_secs={} / pre_roll_ms={}",
            std::process::id(),
            config.effective_transcription_provider().as_str(),
            config.effective_max_secs(),
            config.effective_pre_roll_ms()
        ),
        "",
    );
    check_permissions(&mut report, config.effective_push_to_talk_enabled());
    match input_status {
        Ok(detail) => report.add("入力ストリーム", CheckStatus::Ok, detail, ""),
        Err(error) => report.add(
            "入力ストリーム",
            CheckStatus::Fail,
            error.to_string(),
            "マイクの接続・権限を確認し、daemonを再起動してください。",
        ),
    }
    let worker_running = super::external::text_input::worker_is_running();
    report.add(
        "直接入力ワーカー",
        if worker_running {
            CheckStatus::Ok
        } else {
            CheckStatus::Fail
        },
        if worker_running {
            "稼働中"
        } else {
            "停止しています"
        },
        if worker_running {
            ""
        } else {
            "アクセシビリティ権限を確認してdaemonを再起動してください。"
        },
    );
    match CpalAudioBackend::diagnose_input() {
        Ok(detail) => report.add("音声入力", CheckStatus::Ok, detail, ""),
        Err(error) => report.add("音声入力", CheckStatus::Fail, error.to_string(),
            "マイクの接続とmacOSのサウンド入力を確認し、input_device_prioritiesを見直してください。"),
    }
    if let Ok(dir) = diagnostic_data_dir() {
        let path = config
            .dict_path
            .as_ref()
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| dir.join("dictionary.json"));
        check_dictionary(&mut report, &path);
    }
    check_transcription(&mut report, &env).await;
    DaemonDiagnosis {
        report,
        config,
        config_path: diagnostic_data_dir()
            .expect("daemon has a data directory")
            .join("config.json"),
    }
}

async fn check_transcription(report: &mut DoctorReport, config: &EnvConfig) {
    match config.transcription.provider {
        TranscriptionProvider::GptTranscribe | TranscriptionProvider::GptLiveTranscribe => {
            report.add(
                "APIキー",
                CheckStatus::Ok,
                "daemonバイナリに埋込済み（値は表示しません）",
                "",
            );
            let result = reqwest::Client::new()
                .get("https://api.openai.com/v1/models")
                .bearer_auth(&config.transcription.api_key)
                .timeout(DEPENDENCY_TIMEOUT)
                .send()
                .await;
            match result {
                Ok(response) if response.status().is_success() => report.add("OpenAI API", CheckStatus::Ok,
                    "接続・認証成功（/v1/models）。転写モデルの権限・残高・WebSocket接続は未検証", ""),
                Ok(response) if response.status().as_u16() == 403 => report.add("OpenAI API", CheckStatus::Unknown,
                    "HTTP 403: 診断用の/v1/modelsへのアクセスが拒否されました。転写APIの利用可否は未確認",
                    "APIキーのモデル一覧取得権限とOpenAIプロジェクトへのアクセスを確認してください。"),
                Ok(response) => report.add("OpenAI API", CheckStatus::Fail, format!("HTTP {}", response.status()),
                    match response.status().as_u16() {
                        401 => "埋込キーが無効です。.envのTRANSCRIPTION_API_KEYを修正して再ビルド・配置してください。",
                        429 => "OpenAIの利用枠・残高・レート制限を確認してください。",
                        _ => "OpenAIの稼働状況とネットワーク・プロキシ設定を確認してください。",
                    }),
                Err(error) => report.add("OpenAI API", CheckStatus::Fail,
                    if error.is_timeout() { "8秒以内に応答がありません" } else { "接続できません（ネットワーク・TLS・プロキシ）" },
                    "ネットワーク、HTTPS_PROXY / HTTP_PROXY / ALL_PROXYとOpenAIへの接続を確認してください。"),
            }
        }
        TranscriptionProvider::MlxQwen3Asr => {
            let mut command =
                tokio::process::Command::new(&config.transcription.mlx_qwen3_asr_command);
            command.arg("--help").kill_on_drop(true);
            let result = tokio::time::timeout(DEPENDENCY_TIMEOUT, command.output()).await;
            let (status, detail, action) = match result {
                Ok(Ok(output)) if output.status.success() => (
                    CheckStatus::Ok,
                    "コマンドを実行可能（モデル読込・転写は未検証）",
                    "",
                ),
                Ok(Ok(_)) => (
                    CheckStatus::Fail,
                    "--helpが失敗しました",
                    "daemonのPATHとmlx-qwen3-asrの依存パッケージを確認してください。",
                ),
                Ok(Err(_)) => (
                    CheckStatus::Fail,
                    "コマンドを起動できません",
                    "mlx-qwen3-asrをインストールし、LaunchAgentのPATHから実行可能にしてください。",
                ),
                Err(_) => (
                    CheckStatus::Fail,
                    "8秒以内に応答がありません",
                    "mlx-qwen3-asr --helpが完了するか確認してください。",
                ),
            };
            report.add(
                "mlx-qwen3-asr",
                status,
                format!(
                    "{} / model={} / {detail}",
                    config.transcription.mlx_qwen3_asr_command,
                    config.transcription.mlx_qwen3_asr_model
                ),
                action,
            );
        }
    }
}

#[cfg(target_os = "macos")]
fn check_permissions(report: &mut DoctorReport, push_to_talk_enabled: bool) {
    use objc2::{
        msg_send,
        runtime::{AnyClass, AnyObject},
    };
    #[link(name = "ApplicationServices", kind = "framework")]
    unsafe extern "C" {
        fn AXIsProcessTrusted() -> bool;
    }
    #[link(name = "CoreGraphics", kind = "framework")]
    unsafe extern "C" {
        fn CGPreflightListenEventAccess() -> bool;
    }
    #[link(name = "AVFoundation", kind = "framework")]
    unsafe extern "C" {
        static AVMediaTypeAudio: *const AnyObject;
    }

    // AVFoundationのクラスメソッドを呼ぶ。NSString定数はframeworkが所有する。
    let mic = AnyClass::get(c"AVCaptureDevice").map(|class| unsafe {
        let status: isize = msg_send![class, authorizationStatusForMediaType: AVMediaTypeAudio];
        status
    });
    let (status, detail) = match mic {
        Some(3) => (CheckStatus::Ok, "許可済み"),
        Some(0) => (CheckStatus::Fail, "未許可（許可要求前）"),
        Some(1) => (CheckStatus::Fail, "システムにより制限されています"),
        Some(2) => (CheckStatus::Fail, "拒否されています"),
        _ => (CheckStatus::Unknown, "OSから権限状態を取得できません"),
    };
    report.add("マイク権限", status, detail, if status == CheckStatus::Ok { "" } else {
        "システム設定 → プライバシーとセキュリティ → マイクでVoiceInput.appを許可し、daemonを再起動してください。"
    });
    // どちらも確認のみで、権限ダイアログは出さない。
    let accessibility = unsafe { AXIsProcessTrusted() };
    report.add("アクセシビリティ権限", if accessibility { CheckStatus::Ok } else { CheckStatus::Fail },
        if accessibility { "許可済み（直接入力）" } else { "直接入力の許可がありません" },
        if accessibility { "" } else { "システム設定 → プライバシーとセキュリティ → アクセシビリティでVoiceInput.appを許可し、daemonを再起動してください。" });
    if push_to_talk_enabled {
        let input_monitoring = unsafe { CGPreflightListenEventAccess() };
        report.add("入力監視権限", if input_monitoring { CheckStatus::Ok } else { CheckStatus::Fail },
            if input_monitoring { "許可済み" } else { "Push-to-talkの許可がありません" },
            if input_monitoring { "" } else { "システム設定 → プライバシーとセキュリティ → 入力監視でVoiceInput.appを許可し、daemonを再起動してください。" });
    } else {
        report.add(
            "入力監視権限",
            CheckStatus::Skip,
            "Push-to-talk無効のため不要",
            "",
        );
    }
}

#[cfg(not(target_os = "macos"))]
fn check_permissions(report: &mut DoctorReport, _push_to_talk_enabled: bool) {
    report.add(
        "権限",
        CheckStatus::Unknown,
        "macOS以外では権限を診断できません",
        "OS側でマイクと直接入力の権限を確認してください。",
    );
}
