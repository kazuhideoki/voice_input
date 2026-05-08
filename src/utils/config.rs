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
        "TRANSCRIPTION_PROVIDER={value} is unsupported. Supported providers: 4o, realtime-whisper, mlx-qwen3-asr"
    )]
    UnsupportedTranscriptionProvider { value: String },
    #[error(
        "TRANSCRIPTION_MODEL={value} is unsupported for provider {provider}. Supported models: gpt-4o-mini-transcribe, gpt-4o-transcribe, gpt-realtime-whisper"
    )]
    UnsupportedTranscriptionModel { provider: String, value: String },
    #[error("VOICE_INPUT_MAX_SECS must be an integer: {value}")]
    InvalidMaxDurationSecs { value: String },
    #[error("{name} must be either 'true' or 'false': {value}")]
    InvalidBooleanEnv { name: &'static str, value: String },
    #[error("VOICE_INPUT_AUDIO_FORMAT must be either 'flac' or 'wav': {value}")]
    InvalidAudioFormat { value: String },
    #[error(
        "VOICE_INPUT_AUDIO_FORMAT={value} is unsupported for provider {provider}. Supported formats: {supported}"
    )]
    UnsupportedAudioFormatForProvider {
        provider: String,
        value: String,
        supported: &'static str,
    },
}

/// 転写バックエンド種別
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TranscriptionProvider {
    #[serde(rename = "4o")]
    OpenAi4o,
    #[serde(rename = "realtime-whisper")]
    OpenAiRealtimeWhisper,
    #[serde(rename = "mlx-qwen3-asr")]
    MlxQwen3Asr,
}

impl TranscriptionProvider {
    pub const DEFAULT: Self = Self::OpenAi4o;

    /// 環境変数から転写バックエンド設定を生成
    pub fn from_env() -> Result<Self, ConfigError> {
        match std::env::var("TRANSCRIPTION_PROVIDER") {
            Ok(value) => Self::parse(&value),
            Err(_) => Ok(Self::DEFAULT),
        }
    }

    /// 文字列から転写バックエンド設定を生成
    pub fn parse(value: &str) -> Result<Self, ConfigError> {
        match value {
            "4o" => Ok(Self::OpenAi4o),
            "realtime-whisper" => Ok(Self::OpenAiRealtimeWhisper),
            "mlx-qwen3-asr" => Ok(Self::MlxQwen3Asr),
            unsupported => Err(ConfigError::UnsupportedTranscriptionProvider {
                value: unsupported.to_string(),
            }),
        }
    }

    /// 環境変数未指定時のモデル名を返す
    pub fn default_model(&self) -> &'static str {
        match self {
            Self::OpenAi4o => "gpt-4o-mini-transcribe",
            Self::OpenAiRealtimeWhisper => "gpt-realtime-whisper",
            Self::MlxQwen3Asr => "Qwen/Qwen3-ASR-1.7B",
        }
    }

    /// モデル名を検証する
    pub fn validate_model(&self, value: &str) -> Result<(), ConfigError> {
        match self {
            Self::OpenAi4o => match value {
                "gpt-4o-mini-transcribe" | "gpt-4o-transcribe" => Ok(()),
                unsupported => Err(ConfigError::UnsupportedTranscriptionModel {
                    provider: self.as_str().to_string(),
                    value: unsupported.to_string(),
                }),
            },
            Self::OpenAiRealtimeWhisper => match value {
                "gpt-realtime-whisper" => Ok(()),
                unsupported => Err(ConfigError::UnsupportedTranscriptionModel {
                    provider: self.as_str().to_string(),
                    value: unsupported.to_string(),
                }),
            },
            Self::MlxQwen3Asr => Ok(()),
        }
    }

    /// バックエンド名を文字列で取得
    pub fn as_str(&self) -> &str {
        match self {
            Self::OpenAi4o => "4o",
            Self::OpenAiRealtimeWhisper => "realtime-whisper",
            Self::MlxQwen3Asr => "mlx-qwen3-asr",
        }
    }

    /// OpenAI API キーを利用する provider かどうかを返す
    pub fn uses_openai_api(&self) -> bool {
        matches!(self, Self::OpenAi4o | Self::OpenAiRealtimeWhisper)
    }
}

/// 転写設定
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptionConfig {
    /// 転写バックエンド
    pub provider: TranscriptionProvider,
    /// 転写サービス APIキー
    pub api_key: Option<String>,
    /// 転写モデル名
    pub model: String,
    /// OpenAI Realtime Whisper のモデル名
    pub realtime_whisper_model: String,
    /// mlx-qwen3-asr のモデル名
    pub mlx_qwen3_asr_model: String,
    /// ストリーミング直接入力を有効にする
    pub streaming_enabled: bool,
    /// 転写ログ保存先パス
    pub log_path: Option<PathBuf>,
    /// 低信頼語の自動選択を有効にする
    pub low_confidence_selection_enabled: bool,
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
    /// プロファイリング設定
    pub profiling: ProfilingConfig,
}

impl EnvConfig {
    /// 環境変数から設定を構築し、妥当性を検証する
    pub(crate) fn from_env() -> Result<Self, ConfigError> {
        let env_provider = TranscriptionProvider::from_env()?;
        let provider = TranscriptionProvider::DEFAULT;
        let model = load_openai_transcription_model(env_provider)?;
        let realtime_whisper_model = load_realtime_whisper_model(env_provider)?;
        let mlx_qwen3_asr_model = load_mlx_qwen3_asr_model(env_provider);
        let streaming_enabled = parse_bool_env("OPENAI_TRANSCRIBE_STREAMING")?;
        let mlx_qwen3_asr_command = load_mlx_qwen3_asr_command();
        let preferred_format = PreferredAudioFormat::from_env(env_provider)?;
        let max_duration_secs = match std::env::var("VOICE_INPUT_MAX_SECS") {
            Ok(value) => value
                .parse()
                .map_err(|_| ConfigError::InvalidMaxDurationSecs { value })?,
            Err(_) => 30,
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
                model,
                realtime_whisper_model,
                mlx_qwen3_asr_model,
                streaming_enabled,
                log_path: non_empty_env("OPENAI_TRANSCRIPTION_LOG_PATH").map(PathBuf::from),
                low_confidence_selection_enabled: parse_bool_env(
                    "VOICE_INPUT_LOW_CONFIDENCE_SELECTION",
                )?,
                mlx_qwen3_asr_command,
            },
            proxy: ProxyConfig {
                all: non_empty_env_with_lowercase_fallback("ALL_PROXY"),
                https: non_empty_env_with_lowercase_fallback("HTTPS_PROXY"),
                http: non_empty_env_with_lowercase_fallback("HTTP_PROXY"),
            },
            audio: AudioConfig {
                input_device_priorities: csv_env("INPUT_DEVICE_PRIORITY"),
                preferred_format,
            },
            recording: RecordingConfig { max_duration_secs },
            profiling: ProfilingConfig {
                enabled: parse_bool_env("VOICE_INPUT_PROFILE")?,
            },
        })
    }

    /// 環境変数から設定を構築し、妥当性を検証する
    pub fn try_from_env() -> Result<Self, ConfigError> {
        Self::from_env()
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
        if ENV_CONFIG.get().is_some() {
            return Ok(());
        }

        let config = EnvConfig::from_env()?;

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

fn load_openai_transcription_model(
    env_provider: TranscriptionProvider,
) -> Result<String, ConfigError> {
    let value = if env_provider == TranscriptionProvider::OpenAi4o {
        non_empty_env("TRANSCRIPTION_MODEL").or_else(|| non_empty_env("OPENAI_TRANSCRIBE_MODEL"))
    } else {
        non_empty_env("OPENAI_TRANSCRIBE_MODEL")
    };

    let model =
        value.unwrap_or_else(|| TranscriptionProvider::OpenAi4o.default_model().to_string());
    TranscriptionProvider::OpenAi4o.validate_model(&model)?;
    Ok(model)
}

fn load_realtime_whisper_model(env_provider: TranscriptionProvider) -> Result<String, ConfigError> {
    let value = if env_provider == TranscriptionProvider::OpenAiRealtimeWhisper {
        non_empty_env("TRANSCRIPTION_MODEL")
    } else {
        None
    };

    let model = value.unwrap_or_else(|| {
        TranscriptionProvider::OpenAiRealtimeWhisper
            .default_model()
            .to_string()
    });
    TranscriptionProvider::OpenAiRealtimeWhisper.validate_model(&model)?;
    Ok(model)
}

fn load_mlx_qwen3_asr_model(env_provider: TranscriptionProvider) -> String {
    non_empty_env("MLX_QWEN3_ASR_MODEL")
        .or_else(|| {
            if env_provider == TranscriptionProvider::MlxQwen3Asr {
                non_empty_env("TRANSCRIPTION_MODEL")
            } else {
                None
            }
        })
        .unwrap_or_else(|| {
            TranscriptionProvider::MlxQwen3Asr
                .default_model()
                .to_string()
        })
}

fn load_mlx_qwen3_asr_command() -> String {
    non_empty_env("MLX_QWEN3_ASR_COMMAND").unwrap_or_else(|| "mlx-qwen3-asr".into())
}

fn parse_bool_env(name: &'static str) -> Result<bool, ConfigError> {
    match std::env::var(name) {
        Ok(value) => match value.as_str() {
            "true" => Ok(true),
            "false" => Ok(false),
            _ => Err(ConfigError::InvalidBooleanEnv { name, value }),
        },
        Err(_) => Ok(false),
    }
}

impl PreferredAudioFormat {
    fn from_env(provider: TranscriptionProvider) -> Result<Self, ConfigError> {
        match non_empty_env("VOICE_INPUT_AUDIO_FORMAT") {
            Some(value) => Self::parse_for_provider(provider, &value),
            None => Ok(match provider {
                TranscriptionProvider::OpenAi4o | TranscriptionProvider::OpenAiRealtimeWhisper => {
                    Self::Flac
                }
                TranscriptionProvider::MlxQwen3Asr => Self::Wav,
            }),
        }
    }

    fn parse(value: &str) -> Result<Self, ConfigError> {
        match value.to_ascii_lowercase().as_str() {
            "flac" => Ok(Self::Flac),
            "wav" => Ok(Self::Wav),
            _ => Err(ConfigError::InvalidAudioFormat {
                value: value.to_string(),
            }),
        }
    }

    fn parse_for_provider(
        provider: TranscriptionProvider,
        value: &str,
    ) -> Result<Self, ConfigError> {
        let format = Self::parse(value)?;
        if provider == TranscriptionProvider::MlxQwen3Asr && format != Self::Wav {
            return Err(ConfigError::UnsupportedAudioFormatForProvider {
                provider: provider.as_str().to_string(),
                value: value.to_string(),
                supported: "wav",
            });
        }

        Ok(format)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AudioConfig, ConfigError, EnvConfig, PathConfig, PreferredAudioFormat, ProfilingConfig,
        ProxyConfig, RecordingConfig, TranscriptionConfig, TranscriptionProvider, lock_test_env,
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
            },
            recording: RecordingConfig {
                max_duration_secs: 30,
            },
            profiling: ProfilingConfig { enabled: false },
        }
    }

    fn openai_transcription_config() -> TranscriptionConfig {
        TranscriptionConfig {
            provider: TranscriptionProvider::OpenAi4o,
            api_key: None,
            model: "gpt-4o-mini-transcribe".to_string(),
            realtime_whisper_model: "gpt-realtime-whisper".to_string(),
            mlx_qwen3_asr_model: "Qwen/Qwen3-ASR-1.7B".to_string(),
            streaming_enabled: false,
            log_path: None,
            low_confidence_selection_enabled: false,
            mlx_qwen3_asr_command: "mlx-qwen3-asr".to_string(),
        }
    }

    /// 対応プロバイダは文字列から列挙型へ変換できる
    #[test]
    fn supported_transcription_providers_are_parsed() {
        assert_eq!(
            TranscriptionProvider::parse("4o").unwrap(),
            TranscriptionProvider::OpenAi4o
        );
        assert_eq!(
            TranscriptionProvider::parse("realtime-whisper").unwrap(),
            TranscriptionProvider::OpenAiRealtimeWhisper
        );
        assert_eq!(
            TranscriptionProvider::parse("mlx-qwen3-asr").unwrap(),
            TranscriptionProvider::MlxQwen3Asr
        );
    }

    /// OpenAI の未対応モデルは設定値として拒否する
    #[test]
    fn unsupported_openai_model_is_rejected() {
        let error = TranscriptionProvider::OpenAi4o
            .validate_model("whisper-1")
            .unwrap_err();
        assert_eq!(
            error,
            ConfigError::UnsupportedTranscriptionModel {
                provider: "4o".to_string(),
                value: "whisper-1".to_string(),
            }
        );
    }

    /// Realtime Whisper は専用モデルを受け入れる
    #[test]
    fn realtime_whisper_accepts_realtime_whisper_model() {
        assert!(
            TranscriptionProvider::OpenAiRealtimeWhisper
                .validate_model("gpt-realtime-whisper")
                .is_ok()
        );
    }

    /// mlx-qwen3-asr は Hugging Face のモデル名をそのまま受け入れる
    #[test]
    fn mlx_qwen3_asr_accepts_hugging_face_model_name() {
        assert!(
            TranscriptionProvider::MlxQwen3Asr
                .validate_model("Qwen/Qwen3-ASR-1.7B")
                .is_ok()
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

    /// 低信頼語の自動選択は既定で無効
    #[test]
    fn low_confidence_selection_is_disabled_by_default() {
        let config = sample_env_config(openai_transcription_config());

        assert!(!config.transcription.low_confidence_selection_enabled);
    }

    /// 低信頼語の自動選択は環境変数で有効化できる
    #[test]
    fn low_confidence_selection_flag_is_loaded_from_environment() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("VOICE_INPUT_LOW_CONFIDENCE_SELECTION", "true");
        }

        let config = EnvConfig::from_env().unwrap();

        assert!(config.transcription.low_confidence_selection_enabled);

        unsafe {
            std::env::remove_var("VOICE_INPUT_LOW_CONFIDENCE_SELECTION");
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

    /// OpenAI の未対応モデルが環境変数に指定されている場合は設定構築に失敗する
    #[test]
    fn unsupported_openai_model_in_env_fails_config_loading() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("TRANSCRIPTION_PROVIDER", "4o");
            std::env::set_var("OPENAI_TRANSCRIBE_MODEL", "whisper-1");
        }

        let result = EnvConfig::from_env();

        assert_eq!(
            result,
            Err(ConfigError::UnsupportedTranscriptionModel {
                provider: "4o".to_string(),
                value: "whisper-1".to_string(),
            })
        );

        unsafe {
            std::env::remove_var("TRANSCRIPTION_PROVIDER");
            std::env::remove_var("OPENAI_TRANSCRIBE_MODEL");
        }
    }

    /// provider環境変数は検証だけ行い既定の実行バックエンドは変更しない
    #[test]
    fn provider_env_does_not_change_default_transcription_provider() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("TRANSCRIPTION_PROVIDER", "mlx-qwen3-asr");
            std::env::remove_var("TRANSCRIPTION_MODEL");
            std::env::remove_var("OPENAI_TRANSCRIBE_MODEL");
            std::env::remove_var("MLX_QWEN3_ASR_MODEL");
        }

        let config = EnvConfig::from_env().unwrap();

        assert_eq!(
            config.transcription.provider,
            TranscriptionProvider::OpenAi4o
        );
        assert_eq!(config.transcription.model, "gpt-4o-mini-transcribe");
        assert_eq!(
            config.transcription.realtime_whisper_model,
            "gpt-realtime-whisper"
        );
        assert_eq!(
            config.transcription.mlx_qwen3_asr_model,
            "Qwen/Qwen3-ASR-1.7B"
        );
        assert_eq!(config.transcription.mlx_qwen3_asr_command, "mlx-qwen3-asr");

        unsafe {
            std::env::remove_var("TRANSCRIPTION_PROVIDER");
        }
    }

    /// realtime-whisper指定時はTRANSCRIPTION_MODELを専用モデルとして読み込む
    #[test]
    fn realtime_whisper_model_is_loaded_from_transcription_model() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("TRANSCRIPTION_PROVIDER", "realtime-whisper");
            std::env::set_var("TRANSCRIPTION_MODEL", "gpt-realtime-whisper");
            std::env::remove_var("OPENAI_TRANSCRIBE_MODEL");
        }

        let config = EnvConfig::from_env().unwrap();

        assert_eq!(
            config.transcription.realtime_whisper_model,
            "gpt-realtime-whisper"
        );
        assert_eq!(config.transcription.model, "gpt-4o-mini-transcribe");

        unsafe {
            std::env::remove_var("TRANSCRIPTION_PROVIDER");
            std::env::remove_var("TRANSCRIPTION_MODEL");
        }
    }

    /// mlx-qwen3-asr コマンドは明示設定された値をそのまま使う
    #[test]
    fn mlx_qwen3_asr_command_uses_configured_value_as_is() {
        let _lock = lock_test_env();
        let original_command = std::env::var("MLX_QWEN3_ASR_COMMAND").ok();

        unsafe {
            std::env::set_var("TRANSCRIPTION_PROVIDER", "mlx-qwen3-asr");
            std::env::remove_var("TRANSCRIPTION_MODEL");
            std::env::set_var("MLX_QWEN3_ASR_COMMAND", "/Users/example/bin/mlx-qwen3-asr");
        }

        let config = EnvConfig::from_env().unwrap();

        assert_eq!(
            config.transcription.mlx_qwen3_asr_command,
            "/Users/example/bin/mlx-qwen3-asr"
        );

        unsafe {
            std::env::remove_var("TRANSCRIPTION_PROVIDER");
            std::env::remove_var("TRANSCRIPTION_MODEL");
        }
        if let Some(value) = original_command {
            unsafe {
                std::env::set_var("MLX_QWEN3_ASR_COMMAND", value);
            }
        } else {
            unsafe {
                std::env::remove_var("MLX_QWEN3_ASR_COMMAND");
            }
        }
    }

    /// mlx-qwen3-asr 利用時は既定で WAV を選ぶ
    #[test]
    fn mlx_qwen3_asr_defaults_to_wav_audio_format() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("TRANSCRIPTION_PROVIDER", "mlx-qwen3-asr");
            std::env::remove_var("VOICE_INPUT_AUDIO_FORMAT");
        }

        let config = EnvConfig::from_env().unwrap();

        assert_eq!(config.audio.preferred_format, PreferredAudioFormat::Wav);

        unsafe {
            std::env::remove_var("TRANSCRIPTION_PROVIDER");
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

    /// 低信頼語選択設定はtrue/false以外を許可しない
    #[test]
    fn try_from_env_rejects_invalid_low_confidence_flag() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("VOICE_INPUT_LOW_CONFIDENCE_SELECTION", "ture");
        }

        let result = EnvConfig::try_from_env();

        assert_eq!(
            result,
            Err(ConfigError::InvalidBooleanEnv {
                name: "VOICE_INPUT_LOW_CONFIDENCE_SELECTION",
                value: "ture".to_string(),
            })
        );

        unsafe {
            std::env::remove_var("VOICE_INPUT_LOW_CONFIDENCE_SELECTION");
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

    /// 低信頼語選択設定はfalseを明示しても正常に無効化できる
    #[test]
    fn try_from_env_accepts_explicit_false_low_confidence_flag() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("VOICE_INPUT_LOW_CONFIDENCE_SELECTION", "false");
        }

        let result =
            EnvConfig::try_from_env().expect("low confidence selection=false should be valid");

        assert!(!result.transcription.low_confidence_selection_enabled);

        unsafe {
            std::env::remove_var("VOICE_INPUT_LOW_CONFIDENCE_SELECTION");
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

    /// 録音フォーマットは環境変数から読み込める
    #[test]
    fn preferred_audio_format_is_loaded_from_environment() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("VOICE_INPUT_AUDIO_FORMAT", "wav");
        }

        let config = EnvConfig::from_env().unwrap();

        assert_eq!(config.audio.preferred_format, PreferredAudioFormat::Wav);

        unsafe {
            std::env::remove_var("VOICE_INPUT_AUDIO_FORMAT");
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

    /// 録音フォーマットは未対応値を拒否する
    #[test]
    fn try_from_env_rejects_invalid_audio_format() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("VOICE_INPUT_AUDIO_FORMAT", "mp3");
        }

        let result = EnvConfig::try_from_env();

        assert_eq!(
            result,
            Err(ConfigError::InvalidAudioFormat {
                value: "mp3".to_string(),
            })
        );

        unsafe {
            std::env::remove_var("VOICE_INPUT_AUDIO_FORMAT");
        }
    }

    /// mlx-qwen3-asr は FLAC 指定を受け付けない
    #[test]
    fn mlx_qwen3_asr_rejects_flac_audio_format() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("TRANSCRIPTION_PROVIDER", "mlx-qwen3-asr");
            std::env::set_var("VOICE_INPUT_AUDIO_FORMAT", "flac");
        }

        let result = EnvConfig::try_from_env();

        assert_eq!(
            result,
            Err(ConfigError::UnsupportedAudioFormatForProvider {
                provider: "mlx-qwen3-asr".to_string(),
                value: "flac".to_string(),
                supported: "wav",
            })
        );

        unsafe {
            std::env::remove_var("TRANSCRIPTION_PROVIDER");
            std::env::remove_var("VOICE_INPUT_AUDIO_FORMAT");
        }
    }
}
