#![allow(clippy::disallowed_methods)]

//! グローバル環境変数設定
//!
//! アプリケーション全体で使用する環境変数を一元管理する唯一の入口。
//! 他のモジュールでは環境変数を直接読まず、このモジュール経由で扱う。
//! プロセス起動時に一度だけ初期化し、以降はどこからでもアクセス可能。

use once_cell::sync::OnceCell;
use std::path::PathBuf;
use std::sync::Arc;

/// グローバル環境変数設定
static ENV_CONFIG: OnceCell<Arc<EnvConfig>> = OnceCell::new();

/// 最大録音時間のデフォルト値（秒）
pub const DEFAULT_MAX_RECORDING_DURATION_SECS: u64 = 30;

/// 最大録音時間を指定する環境変数名
pub const MAX_RECORDING_DURATION_SECS_ENV: &str = "VOICE_INPUT_MAX_SECS";

/// 録音開始時に先頭へ付与するローカル pre-roll のデフォルト値（ミリ秒）
pub const DEFAULT_AUDIO_PRE_ROLL_MS: u64 = 500;

/// 録音開始時 pre-roll の最大値（ミリ秒）
pub const MAX_AUDIO_PRE_ROLL_MS: u64 = 5_000;

/// 録音開始時の pre-roll 長を指定する環境変数名
pub const AUDIO_PRE_ROLL_MS_ENV: &str = "VOICE_INPUT_PRE_ROLL_MS";

/// 録音開始・停止サウンドの有効化を指定する環境変数名
pub const RECORDING_SOUNDS_ENABLED_ENV: &str = "VOICE_INPUT_RECORDING_SOUNDS";

/// 録音状態HUDの有効化を指定する環境変数名
pub const RECORDING_HUD_ENABLED_ENV: &str = "VOICE_INPUT_RECORDING_HUD";

/// 録音状態HUDヘルパーのパスを指定する環境変数名
pub const RECORDING_HUD_HELPER_PATH_ENV: &str = "VOICE_INPUT_RECORDING_HUD_HELPER_PATH";

/// 録音状態HUDが受け取った状態を検証用に書き出すパスを指定する環境変数名
pub const RECORDING_HUD_LOG_PATH_ENV: &str = "VOICE_INPUT_RECORDING_HUD_LOG_PATH";

/// キー押下中だけ録音する push-to-talk を有効にする環境変数名
pub const PUSH_TO_TALK_ENABLED_ENV: &str = "VOICE_INPUT_PUSH_TO_TALK";

/// push-to-talk のトリガーキーを指定する環境変数名
pub const PUSH_TO_TALK_HOTKEY_ENV: &str = "VOICE_INPUT_PUSH_TO_TALK_HOTKEY";

/// push-to-talk のデフォルトホットキー
pub const DEFAULT_PUSH_TO_TALK_HOTKEY: &str = "opt+8";

/// CLI で未指定のときに使う既定の転写バックエンドを指定する環境変数名
pub const DEFAULT_TRANSCRIPTION_PROVIDER_ENV: &str = "VOICE_INPUT_DEFAULT_TRANSCRIPTION_PROVIDER";

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
        "transcription provider '{value}' is unsupported. Supported providers: gpt-transcribe, realtime-whisper, mlx-qwen3-asr"
    )]
    UnsupportedTranscriptionProvider { value: String },
    #[error("VOICE_INPUT_MAX_SECS must be a positive integer: {value}")]
    InvalidMaxDurationSecs { value: String },
    #[error("VOICE_INPUT_PRE_ROLL_MS must be an integer between 0 and 5000: {value}")]
    InvalidAudioPreRollMs { value: String },
    #[error("{name} must be either 'true' or 'false': {value}")]
    InvalidBooleanEnv { name: &'static str, value: String },
}

/// 転写バックエンド種別
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TranscriptionProvider {
    #[serde(rename = "gpt-transcribe")]
    GptTranscribe,
    #[serde(rename = "realtime-whisper")]
    RealtimeWhisper,
    #[serde(rename = "mlx-qwen3-asr")]
    MlxQwen3Asr,
}

impl TranscriptionProvider {
    pub const DEFAULT: Self = Self::GptTranscribe;

    /// 文字列から転写バックエンド設定を生成
    pub fn parse(value: &str) -> Result<Self, ConfigError> {
        match value {
            "gpt-transcribe" => Ok(Self::GptTranscribe),
            "realtime-whisper" => Ok(Self::RealtimeWhisper),
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
            Self::RealtimeWhisper => "realtime-whisper",
            Self::MlxQwen3Asr => "mlx-qwen3-asr",
        }
    }

    /// OpenAI API キーを利用する provider かどうかを返す
    pub fn uses_openai_api(&self) -> bool {
        matches!(self, Self::GptTranscribe | Self::RealtimeWhisper)
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

impl EnvConfig {
    /// 環境変数から設定を構築し、妥当性を検証する
    pub(crate) fn from_env() -> Result<Self, ConfigError> {
        Self::from_env_with_recording_max_duration_secs(None)
    }

    /// 環境変数から設定を構築し、録音最大秒数だけ明示値で上書きする
    pub(crate) fn from_env_with_recording_max_duration_secs(
        max_duration_secs_override: Option<u64>,
    ) -> Result<Self, ConfigError> {
        let provider = match non_empty_env(DEFAULT_TRANSCRIPTION_PROVIDER_ENV) {
            Some(value) => TranscriptionProvider::parse(&value)?,
            None => TranscriptionProvider::DEFAULT,
        };
        let mlx_qwen3_asr_model = DEFAULT_MLX_QWEN3_ASR_MODEL.to_string();
        let streaming_enabled = parse_bool_env("OPENAI_TRANSCRIBE_STREAMING")?;
        let mlx_qwen3_asr_command = "mlx-qwen3-asr".to_string();
        let audio_pre_roll_ms = match std::env::var(AUDIO_PRE_ROLL_MS_ENV) {
            Ok(value) => parse_audio_pre_roll_ms(&value)?,
            Err(_) => DEFAULT_AUDIO_PRE_ROLL_MS,
        };
        let max_duration_secs = match max_duration_secs_override {
            Some(value) if value > 0 => value,
            Some(value) => {
                return Err(ConfigError::InvalidMaxDurationSecs {
                    value: value.to_string(),
                });
            }
            None => match std::env::var(MAX_RECORDING_DURATION_SECS_ENV) {
                Ok(value) => parse_max_duration_secs(&value)?,
                Err(_) => DEFAULT_MAX_RECORDING_DURATION_SECS,
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
                input_device_priorities: csv_env("INPUT_DEVICE_PRIORITY"),
                preferred_format: PreferredAudioFormat::Flac,
                pre_roll_ms: audio_pre_roll_ms,
                recording_sounds_enabled: parse_bool_env_with_default(
                    RECORDING_SOUNDS_ENABLED_ENV,
                    true,
                )?,
            },
            recording: RecordingConfig { max_duration_secs },
            ui: UiConfig {
                recording_hud_enabled: parse_bool_env_with_default(
                    RECORDING_HUD_ENABLED_ENV,
                    true,
                )?,
                recording_hud_helper_path: non_empty_env(RECORDING_HUD_HELPER_PATH_ENV)
                    .map(PathBuf::from),
                recording_hud_log_path: non_empty_env(RECORDING_HUD_LOG_PATH_ENV)
                    .map(PathBuf::from),
            },
            push_to_talk: PushToTalkConfig {
                enabled: parse_bool_env(PUSH_TO_TALK_ENABLED_ENV)?,
                hotkey: non_empty_env(PUSH_TO_TALK_HOTKEY_ENV)
                    .unwrap_or_else(|| DEFAULT_PUSH_TO_TALK_HOTKEY.to_string()),
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
        Self::from_env_with_recording_max_duration_secs(max_duration_secs_override)
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
        if ENV_CONFIG.get().is_some() {
            return Ok(());
        }

        let config =
            EnvConfig::from_env_with_recording_max_duration_secs(max_duration_secs_override)?;

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

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

fn non_empty_env_with_lowercase_fallback(name: &str) -> Option<String> {
    non_empty_env(name).or_else(|| non_empty_env(&name.to_ascii_lowercase()))
}

fn csv_env(name: &str) -> Vec<String> {
    non_empty_env(name)
        .map(|value| {
            value
                .split(',')
                .map(|entry| entry.trim().to_string())
                .filter(|entry| !entry.is_empty())
                .collect()
        })
        .unwrap_or_default()
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
        AudioConfig, ConfigError, DEFAULT_AUDIO_PRE_ROLL_MS, DEFAULT_MAX_RECORDING_DURATION_SECS,
        DEFAULT_PUSH_TO_TALK_HOTKEY, DEFAULT_TRANSCRIPTION_PROVIDER_ENV, EnvConfig,
        MAX_AUDIO_PRE_ROLL_MS, PathConfig, PreferredAudioFormat, ProfilingConfig, ProxyConfig,
        PushToTalkConfig, RECORDING_HUD_ENABLED_ENV, RECORDING_HUD_HELPER_PATH_ENV,
        RECORDING_HUD_LOG_PATH_ENV, RECORDING_SOUNDS_ENABLED_ENV, RecordingConfig,
        TranscriptionConfig, TranscriptionProvider, UiConfig, lock_test_env,
    };
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
                pre_roll_ms: DEFAULT_AUDIO_PRE_ROLL_MS,
                recording_sounds_enabled: true,
            },
            recording: RecordingConfig {
                max_duration_secs: DEFAULT_MAX_RECORDING_DURATION_SECS,
            },
            ui: UiConfig {
                recording_hud_enabled: true,
                recording_hud_helper_path: None,
                recording_hud_log_path: None,
            },
            push_to_talk: PushToTalkConfig {
                enabled: false,
                hotkey: DEFAULT_PUSH_TO_TALK_HOTKEY.to_string(),
            },
            profiling: ProfilingConfig { enabled: false },
        }
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
            TranscriptionProvider::parse("realtime-whisper").unwrap(),
            TranscriptionProvider::RealtimeWhisper
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

    /// ストリーミング設定は環境変数から有効化状態を読み取れる
    #[test]
    fn streaming_flag_is_loaded_from_environment() {
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
        let _lock = lock_test_env();
        unsafe {
            std::env::remove_var("VOICE_INPUT_PUSH_TO_TALK");
            std::env::remove_var("VOICE_INPUT_PUSH_TO_TALK_HOTKEY");
        }

        let config = EnvConfig::from_env().unwrap();

        assert!(!config.push_to_talk.enabled);
        assert_eq!(config.push_to_talk.hotkey, DEFAULT_PUSH_TO_TALK_HOTKEY);
    }

    /// push-to-talk は環境変数から有効化とホットキー指定ができる
    #[test]
    fn push_to_talk_settings_are_loaded_from_environment() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("VOICE_INPUT_PUSH_TO_TALK", "true");
            std::env::set_var("VOICE_INPUT_PUSH_TO_TALK_HOTKEY", "cmd+space");
        }

        let config = EnvConfig::from_env().unwrap();

        assert!(config.push_to_talk.enabled);
        assert_eq!(config.push_to_talk.hotkey, "cmd+space");

        unsafe {
            std::env::remove_var("VOICE_INPUT_PUSH_TO_TALK");
            std::env::remove_var("VOICE_INPUT_PUSH_TO_TALK_HOTKEY");
        }
    }

    /// push-to-talk の有効化フラグは true/false 以外を許可しない
    #[test]
    fn try_from_env_rejects_invalid_push_to_talk_flag() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("VOICE_INPUT_PUSH_TO_TALK", "enabled");
        }

        let result = EnvConfig::try_from_env();

        assert_eq!(
            result.unwrap_err(),
            ConfigError::InvalidBooleanEnv {
                name: "VOICE_INPUT_PUSH_TO_TALK",
                value: "enabled".to_string(),
            }
        );

        unsafe {
            std::env::remove_var("VOICE_INPUT_PUSH_TO_TALK");
        }
    }

    /// 録音最大秒数は環境変数から読み込める
    #[test]
    fn max_duration_secs_is_loaded_from_environment() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("VOICE_INPUT_MAX_SECS", "45");
        }

        let config = EnvConfig::from_env().unwrap();

        assert_eq!(config.recording.max_duration_secs, 45);

        unsafe {
            std::env::remove_var("VOICE_INPUT_MAX_SECS");
        }
    }

    /// 環境変数がなければ既定の実行バックエンドを使う
    #[test]
    fn default_transcription_provider_is_used_without_provider_env() {
        let _lock = lock_test_env();
        unsafe {
            std::env::remove_var(DEFAULT_TRANSCRIPTION_PROVIDER_ENV);
        }

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

    /// CLI 未指定時の既定転写バックエンドは環境変数から読み込める
    #[test]
    fn default_transcription_provider_is_loaded_from_environment() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var(DEFAULT_TRANSCRIPTION_PROVIDER_ENV, "realtime-whisper");
        }

        let config = EnvConfig::from_env().unwrap();

        assert_eq!(
            config.transcription.provider,
            TranscriptionProvider::RealtimeWhisper
        );

        unsafe {
            std::env::remove_var(DEFAULT_TRANSCRIPTION_PROVIDER_ENV);
        }
    }

    /// CLI 未指定時の既定転写バックエンドは未対応値を拒否する
    #[test]
    fn try_from_env_rejects_invalid_default_transcription_provider() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var(DEFAULT_TRANSCRIPTION_PROVIDER_ENV, "openai");
        }

        let result = EnvConfig::try_from_env();

        assert_eq!(
            result.unwrap_err(),
            ConfigError::UnsupportedTranscriptionProvider {
                value: "openai".to_string(),
            }
        );

        unsafe {
            std::env::remove_var(DEFAULT_TRANSCRIPTION_PROVIDER_ENV);
        }
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
        let _lock = lock_test_env();
        unsafe {
            std::env::remove_var("VOICE_INPUT_PRE_ROLL_MS");
        }

        let config = EnvConfig::from_env().unwrap();

        assert_eq!(config.audio.pre_roll_ms, DEFAULT_AUDIO_PRE_ROLL_MS);
    }

    /// 録音開始時の pre-roll は環境変数から読み込める
    #[test]
    fn audio_pre_roll_ms_is_loaded_from_environment() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("VOICE_INPUT_PRE_ROLL_MS", "250");
        }

        let config = EnvConfig::from_env().unwrap();

        assert_eq!(config.audio.pre_roll_ms, 250);

        unsafe {
            std::env::remove_var("VOICE_INPUT_PRE_ROLL_MS");
        }
    }

    /// 録音開始時の pre-roll は0msで無効化できる
    #[test]
    fn audio_pre_roll_ms_accepts_zero() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("VOICE_INPUT_PRE_ROLL_MS", "0");
        }

        let config = EnvConfig::from_env().unwrap();

        assert_eq!(config.audio.pre_roll_ms, 0);

        unsafe {
            std::env::remove_var("VOICE_INPUT_PRE_ROLL_MS");
        }
    }

    /// 録音開始時の pre-roll は上限値を受け入れる
    #[test]
    fn audio_pre_roll_ms_accepts_maximum() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("VOICE_INPUT_PRE_ROLL_MS", MAX_AUDIO_PRE_ROLL_MS.to_string());
        }

        let config = EnvConfig::from_env().unwrap();

        assert_eq!(config.audio.pre_roll_ms, MAX_AUDIO_PRE_ROLL_MS);

        unsafe {
            std::env::remove_var("VOICE_INPUT_PRE_ROLL_MS");
        }
    }

    /// 録音開始・停止サウンドは既定で有効になる
    #[test]
    fn recording_sounds_are_enabled_by_default() {
        let _lock = lock_test_env();
        unsafe {
            std::env::remove_var(RECORDING_SOUNDS_ENABLED_ENV);
        }

        let config = EnvConfig::from_env().unwrap();

        assert!(config.audio.recording_sounds_enabled);
    }

    /// 録音状態HUDは既定で有効になる
    #[test]
    fn recording_hud_is_enabled_by_default() {
        let _lock = lock_test_env();
        unsafe {
            std::env::remove_var(RECORDING_HUD_ENABLED_ENV);
        }

        let config = EnvConfig::from_env().unwrap();

        assert!(config.ui.recording_hud_enabled);
    }

    /// 録音状態HUDは環境変数で無効化できる
    #[test]
    fn recording_hud_can_be_disabled_from_environment() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var(RECORDING_HUD_ENABLED_ENV, "false");
        }

        let config = EnvConfig::from_env().unwrap();

        assert!(!config.ui.recording_hud_enabled);

        unsafe {
            std::env::remove_var(RECORDING_HUD_ENABLED_ENV);
        }
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

    /// 録音開始・停止サウンドは環境変数で無効化できる
    #[test]
    fn recording_sounds_can_be_disabled_from_environment() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var(RECORDING_SOUNDS_ENABLED_ENV, "false");
        }

        let config = EnvConfig::from_env().unwrap();

        assert!(!config.audio.recording_sounds_enabled);

        unsafe {
            std::env::remove_var(RECORDING_SOUNDS_ENABLED_ENV);
        }
    }

    /// 録音開始・停止サウンド設定はtrue/false以外を許可しない
    #[test]
    fn try_from_env_rejects_invalid_recording_sounds_flag() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var(RECORDING_SOUNDS_ENABLED_ENV, "off");
        }

        let result = EnvConfig::try_from_env();

        assert_eq!(
            result,
            Err(ConfigError::InvalidBooleanEnv {
                name: RECORDING_SOUNDS_ENABLED_ENV,
                value: "off".to_string(),
            })
        );

        unsafe {
            std::env::remove_var(RECORDING_SOUNDS_ENABLED_ENV);
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

    /// 録音最大秒数が整数でない場合は設定エラーになる
    #[test]
    fn try_from_env_rejects_invalid_max_duration_secs() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("VOICE_INPUT_MAX_SECS", "abc");
        }

        let result = EnvConfig::try_from_env();

        assert_eq!(
            result,
            Err(ConfigError::InvalidMaxDurationSecs {
                value: "abc".to_string(),
            })
        );

        unsafe {
            std::env::remove_var("VOICE_INPUT_MAX_SECS");
        }
    }

    /// 録音開始時の pre-roll が整数でない場合は設定エラーになる
    #[test]
    fn try_from_env_rejects_invalid_audio_pre_roll_ms() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("VOICE_INPUT_PRE_ROLL_MS", "abc");
        }

        let result = EnvConfig::try_from_env();

        assert_eq!(
            result,
            Err(ConfigError::InvalidAudioPreRollMs {
                value: "abc".to_string(),
            })
        );

        unsafe {
            std::env::remove_var("VOICE_INPUT_PRE_ROLL_MS");
        }
    }

    /// 録音開始時の pre-roll が上限を超える場合は設定エラーになる
    #[test]
    fn try_from_env_rejects_too_large_audio_pre_roll_ms() {
        let _lock = lock_test_env();
        let value = (MAX_AUDIO_PRE_ROLL_MS + 1).to_string();
        unsafe {
            std::env::set_var("VOICE_INPUT_PRE_ROLL_MS", &value);
        }

        let result = EnvConfig::try_from_env();

        assert_eq!(result, Err(ConfigError::InvalidAudioPreRollMs { value }));

        unsafe {
            std::env::remove_var("VOICE_INPUT_PRE_ROLL_MS");
        }
    }

    /// 録音最大秒数が0の場合は設定エラーになる
    #[test]
    fn try_from_env_rejects_zero_max_duration_secs() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("VOICE_INPUT_MAX_SECS", "0");
        }

        let result = EnvConfig::try_from_env();

        assert_eq!(
            result,
            Err(ConfigError::InvalidMaxDurationSecs {
                value: "0".to_string(),
            })
        );

        unsafe {
            std::env::remove_var("VOICE_INPUT_MAX_SECS");
        }
    }

    /// 録音最大秒数が負数の場合は設定エラーになる
    #[test]
    fn try_from_env_rejects_negative_max_duration_secs() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("VOICE_INPUT_MAX_SECS", "-1");
        }

        let result = EnvConfig::try_from_env();

        assert_eq!(
            result,
            Err(ConfigError::InvalidMaxDurationSecs {
                value: "-1".to_string(),
            })
        );

        unsafe {
            std::env::remove_var("VOICE_INPUT_MAX_SECS");
        }
    }

    /// 明示指定された録音最大秒数は不正な環境変数より優先される
    #[test]
    fn explicit_max_duration_secs_overrides_invalid_environment() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("VOICE_INPUT_MAX_SECS", "abc");
        }

        let config = EnvConfig::try_from_env_with_recording_max_duration_secs(Some(120))
            .expect("explicit max duration should override env");

        assert_eq!(config.recording.max_duration_secs, 120);

        unsafe {
            std::env::remove_var("VOICE_INPUT_MAX_SECS");
        }
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

    /// ストリーミング設定はtrue/false以外を許可しない
    #[test]
    fn try_from_env_rejects_invalid_streaming_flag() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("OPENAI_TRANSCRIBE_STREAMING", "TRUE");
        }

        let result = EnvConfig::try_from_env();

        assert_eq!(
            result,
            Err(ConfigError::InvalidBooleanEnv {
                name: "OPENAI_TRANSCRIBE_STREAMING",
                value: "TRUE".to_string(),
            })
        );

        unsafe {
            std::env::remove_var("OPENAI_TRANSCRIBE_STREAMING");
        }
    }

    /// ストリーミング設定はfalseを明示しても正常に無効化できる
    #[test]
    fn try_from_env_accepts_explicit_false_streaming_flag() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("OPENAI_TRANSCRIBE_STREAMING", "false");
        }

        let result = EnvConfig::try_from_env().expect("streaming=false should be valid");

        assert!(!result.transcription.streaming_enabled);

        unsafe {
            std::env::remove_var("OPENAI_TRANSCRIBE_STREAMING");
        }
    }

    /// test_initが利用する検証経路は未初期化時に無効な環境変数を拒否する
    #[test]
    fn test_init_loader_rejects_invalid_env_when_uninitialized() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("OPENAI_TRANSCRIBE_STREAMING", "TRUE");
        }

        let result = EnvConfig::load_for_test_init();

        assert_eq!(
            result,
            Err(ConfigError::InvalidBooleanEnv {
                name: "OPENAI_TRANSCRIBE_STREAMING",
                value: "TRUE".to_string(),
            })
        );

        unsafe {
            std::env::remove_var("OPENAI_TRANSCRIBE_STREAMING");
        }
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

    /// 入力デバイス優先順はカンマ区切り環境変数から読み込める
    #[test]
    fn input_device_priorities_are_loaded_from_environment() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var(
                "INPUT_DEVICE_PRIORITY",
                "Built-in Microphone, Yeti X ,  ,External Mic",
            );
        }

        let config = EnvConfig::from_env().unwrap();

        assert_eq!(
            config.audio.input_device_priorities,
            vec![
                "Built-in Microphone".to_string(),
                "Yeti X".to_string(),
                "External Mic".to_string()
            ]
        );

        unsafe {
            std::env::remove_var("INPUT_DEVICE_PRIORITY");
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
