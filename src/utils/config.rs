#![allow(clippy::disallowed_methods)]

//! グローバル環境変数設定
//!
//! アプリケーション全体で使用する環境変数を一元管理する唯一の入口。
//! 他のモジュールでは環境変数を直接読まず、このモジュール経由で扱う。
//! プロセス起動時に一度だけ初期化し、以降はどこからでもアクセス可能。

use once_cell::sync::OnceCell;
use std::path::PathBuf;
use std::sync::Arc;

use crate::application::config_defaults;
pub use crate::application::config_defaults::TranscriptionProvider;

/// グローバル環境変数設定
static ENV_CONFIG: OnceCell<Arc<EnvConfig>> = OnceCell::new();

/// 録音開始時 pre-roll の最大値（ミリ秒）
pub const MAX_AUDIO_PRE_ROLL_MS: u64 = 5_000;

/// 録音状態HUDヘルパーのパスを指定する環境変数名
pub const RECORDING_HUD_HELPER_PATH_ENV: &str = "VOICE_INPUT_RECORDING_HUD_HELPER_PATH";

/// 録音状態HUDが受け取った状態を検証用に書き出すパスを指定する環境変数名
pub const RECORDING_HUD_LOG_PATH_ENV: &str = "VOICE_INPUT_RECORDING_HUD_LOG_PATH";

/// mlx-qwen3-asr で利用する固定モデル
const DEFAULT_MLX_QWEN3_ASR_MODEL: &str = "Qwen/Qwen3-ASR-1.7B";

#[cfg(test)]
use std::sync::Mutex;

#[cfg(test)]
static TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
pub(crate) fn lock_test_env() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 設定読み込みエラー
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error(
        "transcription provider '{value}' is unsupported. Supported providers: gpt-transcribe, gpt-live-transcribe, mlx-qwen3-asr"
    )]
    UnsupportedTranscriptionProvider { value: String },
    #[error("max_secs must be a positive integer: {value}")]
    InvalidMaxDurationSecs { value: String },
    #[error("pre_roll_ms must be an integer between 0 and 5000: {value}")]
    InvalidAudioPreRollMs { value: String },
    #[error("{name} must be either 'true' or 'false': {value}")]
    InvalidBooleanEnv { name: &'static str, value: String },
}

impl TranscriptionProvider {
    /// 文字列から転写バックエンド設定を生成
    pub fn parse(value: &str) -> Result<Self, ConfigError> {
        match value {
            "gpt-transcribe" => Ok(Self::GptTranscribe),
            "gpt-live-transcribe" => Ok(Self::GptLiveTranscribe),
            "mlx-qwen3-asr" => Ok(Self::MlxQwen3Asr),
            unsupported => Err(ConfigError::UnsupportedTranscriptionProvider {
                value: unsupported.to_string(),
            }),
        }
    }

    /// バックエンド名を文字列で取得
    pub fn as_str(&self) -> &str {
        match self {
            Self::GptTranscribe => "gpt-transcribe",
            Self::GptLiveTranscribe => "gpt-live-transcribe",
            Self::MlxQwen3Asr => "mlx-qwen3-asr",
        }
    }

    /// OpenAI API キーを利用する provider かどうかを返す
    pub fn uses_openai_api(&self) -> bool {
        matches!(self, Self::GptTranscribe | Self::GptLiveTranscribe)
    }
}

/// 転写設定
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptionConfig {
    /// 転写バックエンド
    pub provider: TranscriptionProvider,
    /// 転写サービス APIキー
    pub api_key: Option<String>,
    /// mlx-qwen3-asr のモデル名
    pub mlx_qwen3_asr_model: String,
    /// ストリーミング直接入力を有効にする
    pub streaming_enabled: bool,
    /// 転写ログ保存先パス
    pub log_path: Option<PathBuf>,
    /// mlx-qwen3-asr コマンド名
    pub mlx_qwen3_asr_command: String,
}

impl TranscriptionConfig {
    /// 転写の推奨同時実行数を返す
    pub fn recommended_parallelism(&self) -> usize {
        if self.streaming_enabled { 1 } else { 2 }
    }
}

/// パス系の設定
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathConfig {
    /// XDG Data Home ディレクトリ
    pub xdg_data_home: Option<PathBuf>,
    /// IPC ソケットの絶対パス上書き
    pub socket_path: Option<PathBuf>,
    /// IPC ソケット配置ディレクトリ上書き
    pub socket_dir: Option<PathBuf>,
}

impl PathConfig {
    /// IPC ソケットパスを返す
    pub fn ipc_socket_path(&self) -> PathBuf {
        const SOCKET_FILENAME: &str = "voice_input.sock";
        const DEFAULT_SOCKET_PATH: &str = "/tmp/voice_input.sock";

        if let Some(path) = self.socket_path.as_ref() {
            return path.clone();
        }

        if let Some(dir) = self.socket_dir.as_ref() {
            return dir.join(SOCKET_FILENAME);
        }

        PathBuf::from(DEFAULT_SOCKET_PATH)
    }
}

/// HTTP プロキシ設定
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyConfig {
    /// すべてのプロトコルに適用するプロキシ
    pub all: Option<String>,
    /// HTTPS 用プロキシ
    pub https: Option<String>,
    /// HTTP 用プロキシ
    pub http: Option<String>,
}

/// 音声入力設定
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioConfig {
    /// 優先入力デバイスの一覧
    pub input_device_priorities: Vec<String>,
    /// 録音フォーマット
    pub preferred_format: PreferredAudioFormat,
    /// 録音開始時に先頭へ付与するローカル pre-roll 長（ミリ秒）
    pub pre_roll_ms: u64,
    /// 録音開始・停止サウンドを再生する
    pub recording_sounds_enabled: bool,
}

/// 録音フォーマット
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreferredAudioFormat {
    Flac,
    Wav,
}

/// プロファイリング設定
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfilingConfig {
    /// プロファイルログ出力を有効にする
    pub enabled: bool,
}

/// 録音設定
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RecordingConfig {
    /// 最大録音秒数
    pub max_duration_secs: u64,
}

/// UI設定
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UiConfig {
    /// 録音状態HUDを表示する
    pub recording_hud_enabled: bool,
    /// 録音状態HUDヘルパーのパス
    pub recording_hud_helper_path: Option<PathBuf>,
    /// 録音状態HUDが受け取った状態を書き出す検証用ログ
    pub recording_hud_log_path: Option<PathBuf>,
}

/// push-to-talk 設定
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PushToTalkConfig {
    /// キー押下中録音を有効にする
    pub enabled: bool,
    /// トリガーにするホットキー
    pub hotkey: String,
}

/// 環境変数設定
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnvConfig {
    /// パス系の設定
    pub paths: PathConfig,
    /// OpenAI 転写設定
    pub transcription: TranscriptionConfig,
    /// HTTP プロキシ設定
    pub proxy: ProxyConfig,
    /// 音声入力設定
    pub audio: AudioConfig,
    /// 録音設定
    pub recording: RecordingConfig,
    /// UI設定
    pub ui: UiConfig,
    /// push-to-talk 設定
    pub push_to_talk: PushToTalkConfig,
    /// プロファイリング設定
    pub profiling: ProfilingConfig,
}

/// `config.json` またはコマンド指定によりアプリケーション既定値を置き換えるユーザー設定。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserSettingOverrides {
    pub transcription_provider: Option<TranscriptionProvider>,
    pub max_secs: Option<u64>,
    pub pre_roll_ms: Option<u64>,
    pub input_device_priorities: Option<Vec<String>>,
    pub recording_sounds_enabled: Option<bool>,
    pub recording_hud_enabled: Option<bool>,
    pub push_to_talk_enabled: Option<bool>,
    pub push_to_talk_hotkey: Option<String>,
    pub transcribe_streaming: Option<bool>,
}

impl EnvConfig {
    /// 環境変数から設定を構築し、妥当性を検証する
    pub(crate) fn from_env() -> Result<Self, ConfigError> {
        Self::from_env_with_overrides(&UserSettingOverrides::default(), None)
    }

    fn from_env_with_overrides(
        overrides: &UserSettingOverrides,
        max_duration_secs_override: Option<u64>,
    ) -> Result<Self, ConfigError> {
        let provider = overrides
            .transcription_provider
            .unwrap_or(config_defaults::TRANSCRIPTION_PROVIDER);
        let mlx_qwen3_asr_model = DEFAULT_MLX_QWEN3_ASR_MODEL.to_string();
        let streaming_enabled = overrides
            .transcribe_streaming
            .unwrap_or(config_defaults::TRANSCRIBE_STREAMING);
        let mlx_qwen3_asr_command = "mlx-qwen3-asr".to_string();
        let audio_pre_roll_ms = match overrides.pre_roll_ms {
            Some(millis) if millis <= MAX_AUDIO_PRE_ROLL_MS => millis,
            Some(millis) => {
                return Err(ConfigError::InvalidAudioPreRollMs {
                    value: millis.to_string(),
                });
            }
            None => config_defaults::PRE_ROLL_MS,
        };
        let max_duration_secs = match max_duration_secs_override {
            Some(value) if value > 0 => value,
            Some(value) => {
                return Err(ConfigError::InvalidMaxDurationSecs {
                    value: value.to_string(),
                });
            }
            None => match overrides.max_secs {
                Some(value) if value > 0 => value,
                Some(value) => {
                    return Err(ConfigError::InvalidMaxDurationSecs {
                        value: value.to_string(),
                    });
                }
                None => config_defaults::MAX_SECS,
            },
        };

        Ok(Self {
            paths: PathConfig {
                xdg_data_home: non_empty_env("XDG_DATA_HOME").map(PathBuf::from),
                socket_path: non_empty_env("VOICE_INPUT_SOCKET_PATH").map(PathBuf::from),
                socket_dir: non_empty_env("VOICE_INPUT_SOCKET_DIR").map(PathBuf::from),
            },
            transcription: TranscriptionConfig {
                provider,
                api_key: non_empty_env("TRANSCRIPTION_API_KEY")
                    .or_else(|| non_empty_env("OPENAI_API_KEY")),
                mlx_qwen3_asr_model,
                streaming_enabled,
                log_path: non_empty_env("OPENAI_TRANSCRIPTION_LOG_PATH").map(PathBuf::from),
                mlx_qwen3_asr_command,
            },
            proxy: ProxyConfig {
                all: non_empty_env_with_lowercase_fallback("ALL_PROXY"),
                https: non_empty_env_with_lowercase_fallback("HTTPS_PROXY"),
                http: non_empty_env_with_lowercase_fallback("HTTP_PROXY"),
            },
            audio: AudioConfig {
                input_device_priorities: overrides.input_device_priorities.clone().unwrap_or_else(
                    || {
                        config_defaults::INPUT_DEVICE_PRIORITIES
                            .iter()
                            .map(|value| (*value).to_string())
                            .collect()
                    },
                ),
                preferred_format: PreferredAudioFormat::Flac,
                pre_roll_ms: audio_pre_roll_ms,
                recording_sounds_enabled: overrides
                    .recording_sounds_enabled
                    .unwrap_or(config_defaults::RECORDING_SOUNDS_ENABLED),
            },
            recording: RecordingConfig { max_duration_secs },
            ui: UiConfig {
                recording_hud_enabled: overrides
                    .recording_hud_enabled
                    .unwrap_or(config_defaults::RECORDING_HUD_ENABLED),
                recording_hud_helper_path: non_empty_env(RECORDING_HUD_HELPER_PATH_ENV)
                    .map(PathBuf::from),
                recording_hud_log_path: non_empty_env(RECORDING_HUD_LOG_PATH_ENV)
                    .map(PathBuf::from),
            },
            push_to_talk: PushToTalkConfig {
                enabled: overrides
                    .push_to_talk_enabled
                    .unwrap_or(config_defaults::PUSH_TO_TALK_ENABLED),
                hotkey: overrides
                    .push_to_talk_hotkey
                    .clone()
                    .unwrap_or_else(|| config_defaults::PUSH_TO_TALK_HOTKEY.to_string()),
            },
            profiling: ProfilingConfig {
                enabled: parse_bool_env("VOICE_INPUT_PROFILE")?,
            },
        })
    }

    /// 環境変数から設定を構築し、妥当性を検証する
    pub fn try_from_env() -> Result<Self, ConfigError> {
        Self::from_env()
    }

    /// 環境変数から設定を構築し、録音最大秒数だけ明示値で上書きする
    pub fn try_from_env_with_recording_max_duration_secs(
        max_duration_secs_override: Option<u64>,
    ) -> Result<Self, ConfigError> {
        Self::from_env_with_overrides(&UserSettingOverrides::default(), max_duration_secs_override)
    }

    /// 転写の推奨同時実行数を返す
    pub fn recommended_transcription_parallelism(&self) -> usize {
        self.transcription.recommended_parallelism()
    }

    /// 環境変数から設定を初期化
    ///
    /// アプリケーション起動時に呼び出す。
    /// 既に初期化済みの場合は何もせずOkを返す（冪等性を保証）。
    pub fn init() -> Result<(), ConfigError> {
        Self::init_with_recording_max_duration_secs(None)
    }

    /// 環境変数から設定を初期化し、録音最大秒数だけ明示値で上書きする
    pub fn init_with_recording_max_duration_secs(
        max_duration_secs_override: Option<u64>,
    ) -> Result<(), ConfigError> {
        Self::init_with_user_setting_overrides(
            &UserSettingOverrides::default(),
            max_duration_secs_override,
        )
    }

    /// 永続設定をアプリケーション既定値より優先して設定を初期化する。
    pub fn init_with_user_setting_overrides(
        overrides: &UserSettingOverrides,
        max_duration_secs_override: Option<u64>,
    ) -> Result<(), ConfigError> {
        if ENV_CONFIG.get().is_some() {
            return Ok(());
        }

        let config = EnvConfig::from_env_with_overrides(overrides, max_duration_secs_override)?;

        // 並列実行時の競合を考慮：既に他のスレッドが初期化していても成功とする
        let _ = ENV_CONFIG.set(Arc::new(config));
        Ok(())
    }

    /// 設定を取得
    ///
    /// # Panics
    /// `init()`が呼ばれていない場合パニックする
    pub fn get() -> Arc<EnvConfig> {
        ENV_CONFIG
            .get()
            .expect("EnvConfig not initialized. Call EnvConfig::init() first")
            .clone()
    }

    /// テスト用: カスタム設定で初期化
    ///
    /// Note: once_cellはtakeをサポートしていないため、
    /// テストではプロセス全体で一つの設定を共有する必要があります。
    #[cfg(test)]
    pub fn init_for_test(config: EnvConfig) {
        let _lock = TEST_LOCK.lock().unwrap();

        if ENV_CONFIG.get().is_none() {
            ENV_CONFIG.set(Arc::new(config)).ok();
        }
    }

    /// テスト用: デフォルト設定で初期化（既に初期化済みの場合はスキップ）
    #[cfg(test)]
    pub fn test_init() {
        let _lock = TEST_LOCK.lock().unwrap();

        if ENV_CONFIG.get().is_none() {
            let config = Self::load_for_test_init().expect("test env config should be valid");
            ENV_CONFIG.set(Arc::new(config)).ok();
        }
    }

    #[cfg(test)]
    fn load_for_test_init() -> Result<Self, ConfigError> {
        Self::from_env()
    }
}

/// 初期化前に `config.json` の配置を決めるXDGデータディレクトリを返す。
pub fn xdg_data_home_from_env() -> Option<PathBuf> {
    non_empty_env("XDG_DATA_HOME").map(PathBuf::from)
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn non_empty_env_with_lowercase_fallback(name: &str) -> Option<String> {
    non_empty_env(name).or_else(|| non_empty_env(&name.to_ascii_lowercase()))
}

/// 最大録音時間の秒数を検証して返す
pub fn parse_max_duration_secs(value: &str) -> Result<u64, ConfigError> {
    let secs = value
        .parse()
        .map_err(|_| ConfigError::InvalidMaxDurationSecs {
            value: value.to_string(),
        })?;
    if secs == 0 {
        return Err(ConfigError::InvalidMaxDurationSecs {
            value: value.to_string(),
        });
    }
    Ok(secs)
}

/// 録音開始時 pre-roll のミリ秒を検証して返す
pub fn parse_audio_pre_roll_ms(value: &str) -> Result<u64, ConfigError> {
    let millis = value
        .parse()
        .map_err(|_| ConfigError::InvalidAudioPreRollMs {
            value: value.to_string(),
        })?;
    if millis > MAX_AUDIO_PRE_ROLL_MS {
        return Err(ConfigError::InvalidAudioPreRollMs {
            value: value.to_string(),
        });
    }
    Ok(millis)
}

fn parse_bool_env(name: &'static str) -> Result<bool, ConfigError> {
    parse_bool_env_with_default(name, false)
}

fn parse_bool_env_with_default(name: &'static str, default: bool) -> Result<bool, ConfigError> {
    match std::env::var(name) {
        Ok(value) => match value.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(ConfigError::InvalidBooleanEnv { name, value }),
        },
        Err(_) => Ok(default),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AudioConfig, ConfigError, EnvConfig, PathConfig, PreferredAudioFormat, ProfilingConfig,
        ProxyConfig, PushToTalkConfig, RECORDING_HUD_HELPER_PATH_ENV, RECORDING_HUD_LOG_PATH_ENV,
        RecordingConfig, TranscriptionConfig, TranscriptionProvider, UiConfig,
        UserSettingOverrides, lock_test_env,
    };
    use crate::application::config_defaults;
    use crate::infrastructure::config::AppConfig;
    use std::path::PathBuf;

    fn sample_env_config(transcription: TranscriptionConfig) -> EnvConfig {
        EnvConfig {
            paths: PathConfig {
                xdg_data_home: None,
                socket_path: None,
                socket_dir: None,
            },
            transcription,
            proxy: ProxyConfig {
                all: None,
                https: None,
                http: None,
            },
            audio: AudioConfig {
                input_device_priorities: Vec::new(),
                preferred_format: PreferredAudioFormat::Flac,
                pre_roll_ms: config_defaults::PRE_ROLL_MS,
                recording_sounds_enabled: true,
            },
            recording: RecordingConfig {
                max_duration_secs: config_defaults::MAX_SECS,
            },
            ui: UiConfig {
                recording_hud_enabled: true,
                recording_hud_helper_path: None,
                recording_hud_log_path: None,
            },
            push_to_talk: PushToTalkConfig {
                enabled: false,
                hotkey: config_defaults::PUSH_TO_TALK_HOTKEY.to_string(),
            },
            profiling: ProfilingConfig { enabled: false },
        }
    }

    /// 永続設定はアプリケーション既定値を上書きする
    #[test]
    fn user_setting_overrides_application_defaults() {
        let overrides = UserSettingOverrides {
            max_secs: Some(90),
            transcribe_streaming: Some(true),
            ..UserSettingOverrides::default()
        };

        let result = EnvConfig::from_env_with_overrides(&overrides, None);

        let config = result.unwrap();
        assert_eq!(config.recording.max_duration_secs, 90);
        assert!(config.transcription.streaming_enabled);
    }

    /// 永続設定から構築した環境設定は有効設定と同じ値を保持する
    #[test]
    fn environment_config_matches_effective_persisted_settings() {
        let persisted = AppConfig {
            dict_path: None,
            transcription_provider: Some(TranscriptionProvider::MlxQwen3Asr),
            max_secs: Some(90),
            pre_roll_ms: Some(250),
            input_device_priorities: Some(vec!["External Mic".to_string()]),
            recording_sounds_enabled: Some(false),
            recording_hud_enabled: Some(false),
            push_to_talk_enabled: Some(true),
            push_to_talk_hotkey: Some("cmd+space".to_string()),
            transcribe_streaming: Some(true),
        };

        let environment =
            EnvConfig::from_env_with_overrides(&persisted.user_setting_overrides(), None).unwrap();

        assert_eq!(
            environment.transcription.provider,
            persisted.effective_transcription_provider()
        );
        assert_eq!(
            environment.recording.max_duration_secs,
            persisted.effective_max_secs()
        );
        assert_eq!(
            environment.audio.pre_roll_ms,
            persisted.effective_pre_roll_ms()
        );
        assert_eq!(
            environment.audio.input_device_priorities,
            persisted.effective_input_device_priorities()
        );
        assert_eq!(
            environment.audio.recording_sounds_enabled,
            persisted.effective_recording_sounds_enabled()
        );
        assert_eq!(
            environment.ui.recording_hud_enabled,
            persisted.effective_recording_hud_enabled()
        );
        assert_eq!(
            environment.push_to_talk.enabled,
            persisted.effective_push_to_talk_enabled()
        );
        assert_eq!(
            environment.push_to_talk.hotkey,
            persisted.effective_push_to_talk_hotkey()
        );
        assert_eq!(
            environment.transcription.streaming_enabled,
            persisted.effective_transcribe_streaming()
        );
    }

    /// 廃止したユーザー設定用環境変数はアプリケーション既定値を変更しない
    #[test]
    fn removed_user_setting_environment_variables_are_ignored() {
        let _lock = lock_test_env();
        let removed_variables = [
            (
                "VOICE_INPUT_DEFAULT_TRANSCRIPTION_PROVIDER",
                "mlx-qwen3-asr",
            ),
            ("VOICE_INPUT_DEFAULT_TRANSCRIBE_STREAMING", "true"),
            ("VOICE_INPUT_DEFAULT_MAX_SECS", "99"),
            ("VOICE_INPUT_DEFAULT_PRE_ROLL_MS", "0"),
            ("VOICE_INPUT_DEFAULT_INPUT_DEVICE_PRIORITIES", "Ignored Mic"),
            ("VOICE_INPUT_DEFAULT_RECORDING_SOUNDS_ENABLED", "false"),
            ("VOICE_INPUT_DEFAULT_RECORDING_HUD_ENABLED", "false"),
            ("VOICE_INPUT_DEFAULT_PUSH_TO_TALK_ENABLED", "true"),
            ("VOICE_INPUT_DEFAULT_PUSH_TO_TALK_HOTKEY", "cmd+space"),
        ];
        for (name, value) in removed_variables {
            unsafe { std::env::set_var(name, value) };
        }

        let config = EnvConfig::from_env().unwrap();

        for (name, _) in removed_variables {
            unsafe { std::env::remove_var(name) };
        }
        assert_eq!(
            config.transcription.provider,
            config_defaults::TRANSCRIPTION_PROVIDER
        );
        assert_eq!(
            config.transcription.streaming_enabled,
            config_defaults::TRANSCRIBE_STREAMING
        );
        assert_eq!(
            config.recording.max_duration_secs,
            config_defaults::MAX_SECS
        );
        assert_eq!(config.audio.pre_roll_ms, config_defaults::PRE_ROLL_MS);
        assert!(config.audio.input_device_priorities.is_empty());
        assert_eq!(
            config.audio.recording_sounds_enabled,
            config_defaults::RECORDING_SOUNDS_ENABLED
        );
        assert_eq!(
            config.ui.recording_hud_enabled,
            config_defaults::RECORDING_HUD_ENABLED
        );
        assert_eq!(
            config.push_to_talk.enabled,
            config_defaults::PUSH_TO_TALK_ENABLED
        );
        assert_eq!(
            config.push_to_talk.hotkey,
            config_defaults::PUSH_TO_TALK_HOTKEY
        );
    }

    fn openai_transcription_config() -> TranscriptionConfig {
        TranscriptionConfig {
            provider: TranscriptionProvider::GptTranscribe,
            api_key: None,
            mlx_qwen3_asr_model: "Qwen/Qwen3-ASR-1.7B".to_string(),
            streaming_enabled: false,
            log_path: None,
            mlx_qwen3_asr_command: "mlx-qwen3-asr".to_string(),
        }
    }

    /// 対応プロバイダは文字列から列挙型へ変換できる
    #[test]
    fn supported_transcription_providers_are_parsed() {
        assert_eq!(
            TranscriptionProvider::parse("gpt-transcribe").unwrap(),
            TranscriptionProvider::GptTranscribe
        );
        assert_eq!(
            TranscriptionProvider::parse("gpt-live-transcribe").unwrap(),
            TranscriptionProvider::GptLiveTranscribe
        );
        assert_eq!(
            TranscriptionProvider::parse("mlx-qwen3-asr").unwrap(),
            TranscriptionProvider::MlxQwen3Asr
        );
    }

    /// 廃止した4oプロバイダ名は受け付けない
    #[test]
    fn removed_4o_provider_is_rejected() {
        let error = TranscriptionProvider::parse("4o").unwrap_err();
        assert_eq!(
            error,
            ConfigError::UnsupportedTranscriptionProvider {
                value: "4o".to_string(),
            }
        );
    }

    /// 廃止したRealtime Whisperプロバイダ名は受け付けない
    #[test]
    fn removed_realtime_whisper_provider_is_rejected() {
        let error = TranscriptionProvider::parse("realtime-whisper").unwrap_err();
        assert_eq!(
            error,
            ConfigError::UnsupportedTranscriptionProvider {
                value: "realtime-whisper".to_string(),
            }
        );
    }

    /// ストリーミング設定は構築済み設定の有効化状態を保持する
    #[test]
    fn streaming_flag_keeps_configured_value() {
        let mut transcription = openai_transcription_config();
        transcription.streaming_enabled = true;
        let config = sample_env_config(transcription);

        assert!(config.transcription.streaming_enabled);
    }

    /// ストリーミング有効時は転写を直列化する
    #[test]
    fn streaming_uses_single_transcription_parallelism() {
        let mut transcription = openai_transcription_config();
        transcription.streaming_enabled = true;
        let config = sample_env_config(transcription);

        assert_eq!(config.recommended_transcription_parallelism(), 1);
    }

    /// ストリーミング無効時は従来の並列度を維持する
    #[test]
    fn non_streaming_keeps_existing_transcription_parallelism() {
        let config = sample_env_config(openai_transcription_config());

        assert_eq!(config.recommended_transcription_parallelism(), 2);
    }

    /// 転写ログ保存先は環境変数未指定なら無効のままになる
    #[test]
    fn transcription_log_path_is_disabled_by_default() {
        let config = sample_env_config(openai_transcription_config());

        assert_eq!(config.transcription.log_path, None);
    }

    /// 転写ログ保存先は設定されていればその値を保持する
    #[test]
    fn transcription_log_path_keeps_configured_value() {
        let mut transcription = openai_transcription_config();
        transcription.log_path = Some(PathBuf::from("/tmp/transcription-log.ndjson"));
        let config = sample_env_config(transcription);

        assert_eq!(
            config.transcription.log_path.as_deref(),
            Some(PathBuf::from("/tmp/transcription-log.ndjson").as_path())
        );
    }

    /// 転写ログ保存先は空文字なら無効扱いになる
    #[test]
    fn transcription_log_path_treats_empty_env_as_disabled() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("OPENAI_TRANSCRIPTION_LOG_PATH", "   ");
        }

        let config = EnvConfig::from_env().unwrap();

        assert_eq!(config.transcription.log_path, None);

        unsafe {
            std::env::remove_var("OPENAI_TRANSCRIPTION_LOG_PATH");
        }
    }

    /// push-to-talk は既定で無効かつ opt+8 を既定ホットキーにする
    #[test]
    fn push_to_talk_defaults_to_disabled_opt_8() {
        let config = EnvConfig::from_env().unwrap();

        assert!(!config.push_to_talk.enabled);
        assert_eq!(
            config.push_to_talk.hotkey,
            config_defaults::PUSH_TO_TALK_HOTKEY
        );
    }

    /// 未設定なら既定の実行バックエンドを使う
    #[test]
    fn default_transcription_provider_is_used_without_persisted_setting() {
        let config = EnvConfig::from_env().unwrap();

        assert_eq!(
            config.transcription.provider,
            TranscriptionProvider::GptTranscribe
        );
        assert_eq!(
            config.transcription.mlx_qwen3_asr_model,
            "Qwen/Qwen3-ASR-1.7B"
        );
        assert_eq!(config.transcription.mlx_qwen3_asr_command, "mlx-qwen3-asr");
    }

    /// mlx-qwen3-asr コマンドは既定コマンドを使う
    #[test]
    fn mlx_qwen3_asr_command_uses_default_command() {
        let _lock = lock_test_env();

        let config = EnvConfig::from_env().unwrap();

        assert_eq!(config.transcription.mlx_qwen3_asr_command, "mlx-qwen3-asr");
    }

    /// 音声フォーマットは既定で FLAC を選ぶ
    #[test]
    fn audio_format_defaults_to_flac() {
        let config = EnvConfig::from_env().unwrap();

        assert_eq!(config.audio.preferred_format, PreferredAudioFormat::Flac);
    }

    /// 録音開始時の pre-roll は既定で 500ms になる
    #[test]
    fn audio_pre_roll_defaults_to_500ms() {
        let config = EnvConfig::from_env().unwrap();

        assert_eq!(config.audio.pre_roll_ms, config_defaults::PRE_ROLL_MS);
    }

    /// 録音開始・停止サウンドは既定で有効になる
    #[test]
    fn recording_sounds_are_enabled_by_default() {
        let config = EnvConfig::from_env().unwrap();

        assert!(config.audio.recording_sounds_enabled);
    }

    /// 録音状態HUDは既定で有効になる
    #[test]
    fn recording_hud_is_enabled_by_default() {
        let config = EnvConfig::from_env().unwrap();

        assert!(config.ui.recording_hud_enabled);
    }

    /// 録音状態HUDのヘルパーパスと検証ログパスは環境変数から読み込める
    #[test]
    fn recording_hud_paths_are_loaded_from_environment() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var(RECORDING_HUD_HELPER_PATH_ENV, "/tmp/voice_input_hud");
            std::env::set_var(RECORDING_HUD_LOG_PATH_ENV, "/tmp/voice_input_hud.log");
        }

        let config = EnvConfig::from_env().unwrap();

        assert_eq!(
            config.ui.recording_hud_helper_path.as_deref(),
            Some(PathBuf::from("/tmp/voice_input_hud").as_path())
        );
        assert_eq!(
            config.ui.recording_hud_log_path.as_deref(),
            Some(PathBuf::from("/tmp/voice_input_hud.log").as_path())
        );

        unsafe {
            std::env::remove_var(RECORDING_HUD_HELPER_PATH_ENV);
            std::env::remove_var(RECORDING_HUD_LOG_PATH_ENV);
        }
    }

    /// OpenAI APIキーは新旧環境変数の後方互換を保つ
    #[test]
    fn transcription_api_key_falls_back_to_openai_api_key() {
        let _lock = lock_test_env();
        unsafe {
            std::env::remove_var("TRANSCRIPTION_API_KEY");
            std::env::set_var("OPENAI_API_KEY", "legacy-openai-key");
        }

        let config = EnvConfig::from_env().unwrap();

        assert_eq!(
            config.transcription.api_key.as_deref(),
            Some("legacy-openai-key")
        );

        unsafe {
            std::env::remove_var("OPENAI_API_KEY");
        }
    }

    /// 明示指定された録音最大秒数はアプリケーション既定値より優先される
    #[test]
    fn explicit_max_duration_secs_overrides_application_default() {
        let config = EnvConfig::try_from_env_with_recording_max_duration_secs(Some(120))
            .expect("explicit max duration should override the default");

        assert_eq!(config.recording.max_duration_secs, 120);
    }

    /// 明示指定された録音最大秒数でも0は拒否する
    #[test]
    fn explicit_max_duration_secs_rejects_zero() {
        let result = EnvConfig::try_from_env_with_recording_max_duration_secs(Some(0));

        assert_eq!(
            result,
            Err(ConfigError::InvalidMaxDurationSecs {
                value: "0".to_string(),
            })
        );
    }

    /// IPCソケット設定は環境変数から優先順に解決される
    #[test]
    fn ipc_socket_path_is_loaded_from_environment() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("VOICE_INPUT_SOCKET_PATH", "/tmp/voice_input-test.sock");
            std::env::set_var("VOICE_INPUT_SOCKET_DIR", "/tmp/ignored-dir");
        }

        let config = EnvConfig::from_env().unwrap();

        assert_eq!(
            config.paths.socket_path,
            Some(PathBuf::from("/tmp/voice_input-test.sock"))
        );
        assert_eq!(
            config.paths.ipc_socket_path(),
            PathBuf::from("/tmp/voice_input-test.sock")
        );

        unsafe {
            std::env::remove_var("VOICE_INPUT_SOCKET_PATH");
            std::env::remove_var("VOICE_INPUT_SOCKET_DIR");
        }
    }

    /// IPCソケットディレクトリ設定はパス未指定時の配置先として使われる
    #[test]
    fn ipc_socket_dir_is_loaded_from_environment() {
        let _lock = lock_test_env();
        unsafe {
            std::env::remove_var("VOICE_INPUT_SOCKET_PATH");
            std::env::set_var("VOICE_INPUT_SOCKET_DIR", "/var/tmp");
        }

        let config = EnvConfig::from_env().unwrap();

        assert_eq!(config.paths.socket_path, None);
        assert_eq!(config.paths.socket_dir, Some(PathBuf::from("/var/tmp")));
        assert_eq!(
            config.paths.ipc_socket_path(),
            PathBuf::from("/var/tmp/voice_input.sock")
        );

        unsafe {
            std::env::remove_var("VOICE_INPUT_SOCKET_DIR");
        }
    }

    /// HTTPプロキシ設定は大文字環境変数から読み込める
    #[test]
    fn proxy_settings_are_loaded_from_uppercase_environment() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("ALL_PROXY", "socks5://127.0.0.1:1080");
            std::env::set_var("HTTPS_PROXY", "http://127.0.0.1:8443");
            std::env::set_var("HTTP_PROXY", "http://127.0.0.1:8080");
        }

        let config = EnvConfig::from_env().unwrap();

        assert_eq!(config.proxy.all.as_deref(), Some("socks5://127.0.0.1:1080"));
        assert_eq!(config.proxy.https.as_deref(), Some("http://127.0.0.1:8443"));
        assert_eq!(config.proxy.http.as_deref(), Some("http://127.0.0.1:8080"));

        unsafe {
            std::env::remove_var("ALL_PROXY");
            std::env::remove_var("HTTPS_PROXY");
            std::env::remove_var("HTTP_PROXY");
        }
    }

    /// HTTPプロキシ設定は小文字環境変数も受け入れる
    #[test]
    fn proxy_settings_accept_lowercase_environment_names() {
        let _lock = lock_test_env();
        unsafe {
            std::env::remove_var("ALL_PROXY");
            std::env::remove_var("HTTPS_PROXY");
            std::env::remove_var("HTTP_PROXY");
            std::env::set_var("all_proxy", "socks5://127.0.0.1:1081");
            std::env::set_var("https_proxy", "http://127.0.0.1:8444");
            std::env::set_var("http_proxy", "http://127.0.0.1:8081");
        }

        let config = EnvConfig::from_env().unwrap();

        assert_eq!(config.proxy.all.as_deref(), Some("socks5://127.0.0.1:1081"));
        assert_eq!(config.proxy.https.as_deref(), Some("http://127.0.0.1:8444"));
        assert_eq!(config.proxy.http.as_deref(), Some("http://127.0.0.1:8081"));

        unsafe {
            std::env::remove_var("all_proxy");
            std::env::remove_var("https_proxy");
            std::env::remove_var("http_proxy");
        }
    }

    /// プロファイル設定は環境変数から読み込める
    #[test]
    fn profiling_flag_is_loaded_from_environment() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("VOICE_INPUT_PROFILE", "true");
        }

        let config = EnvConfig::from_env().unwrap();

        assert!(config.profiling.enabled);

        unsafe {
            std::env::remove_var("VOICE_INPUT_PROFILE");
        }
    }

    /// プロファイル設定はtrue/false以外を許可しない
    #[test]
    fn try_from_env_rejects_invalid_profile_flag() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("VOICE_INPUT_PROFILE", "ture");
        }

        let result = EnvConfig::try_from_env();

        assert_eq!(
            result,
            Err(ConfigError::InvalidBooleanEnv {
                name: "VOICE_INPUT_PROFILE",
                value: "ture".to_string(),
            })
        );

        unsafe {
            std::env::remove_var("VOICE_INPUT_PROFILE");
        }
    }
}
