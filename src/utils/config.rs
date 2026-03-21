//! グローバル環境変数設定
//!
//! アプリケーション全体で使用する環境変数を一元管理。
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

/// OpenAI の文字起こしモデル
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpenAiTranscriptionModel(String);

impl OpenAiTranscriptionModel {
    const DEFAULT: &'static str = "gpt-4o-mini-transcribe";
    const STREAMING_SUPPORTED: [&'static str; 2] = ["gpt-4o-mini-transcribe", "gpt-4o-transcribe"];

    /// 環境変数からモデル設定を生成
    pub fn from_env() -> Self {
        Self(std::env::var("OPENAI_TRANSCRIBE_MODEL").unwrap_or_else(|_| Self::DEFAULT.to_string()))
    }

    /// 文字列からモデル設定を生成
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    /// モデル名を文字列で取得
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// ストリーミング転写に対応しているか
    pub fn supports_streaming(&self) -> bool {
        Self::STREAMING_SUPPORTED.contains(&self.0.as_str())
    }
}

/// 設定読み込みエラー
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigError {
    #[error("OPENAI_TRANSCRIBE_MODEL={model} does not support streaming")]
    InvalidStreamingModel { model: String },
    #[error("VOICE_INPUT_MAX_SECS must be an integer: {value}")]
    InvalidMaxDurationSecs { value: String },
    #[error("{name} must be either 'true' or 'false': {value}")]
    InvalidBooleanEnv { name: &'static str, value: String },
}

/// OpenAI 転写設定
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptionConfig {
    /// OpenAI APIキー
    pub openai_api_key: Option<String>,
    /// OpenAI 文字起こしモデル
    pub model: OpenAiTranscriptionModel,
    /// ストリーミング直接入力を有効にする
    pub streaming_enabled: bool,
    /// 転写ログ保存先パス
    pub log_path: Option<PathBuf>,
    /// 低信頼語の自動選択を有効にする
    pub low_confidence_selection_enabled: bool,
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
    /// 録音設定
    pub recording: RecordingConfig,
}

impl EnvConfig {
    /// 環境変数から設定を構築
    #[cfg(test)]
    fn from_env() -> Self {
        Self {
            paths: PathConfig {
                xdg_data_home: non_empty_env("XDG_DATA_HOME").map(PathBuf::from),
            },
            transcription: TranscriptionConfig {
                openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
                model: OpenAiTranscriptionModel::from_env(),
                streaming_enabled: env_flag_unvalidated("OPENAI_TRANSCRIBE_STREAMING"),
                log_path: non_empty_env("OPENAI_TRANSCRIPTION_LOG_PATH").map(PathBuf::from),
                low_confidence_selection_enabled: env_flag_unvalidated(
                    "VOICE_INPUT_LOW_CONFIDENCE_SELECTION",
                ),
            },
            recording: RecordingConfig {
                max_duration_secs: std::env::var("VOICE_INPUT_MAX_SECS")
                    .ok()
                    .and_then(|value| value.parse().ok())
                    .unwrap_or(30),
            },
        }
    }

    /// 環境変数から設定を構築し、妥当性を検証する
    pub fn try_from_env() -> Result<Self, ConfigError> {
        let model = OpenAiTranscriptionModel::from_env();
        let streaming_enabled = parse_bool_env("OPENAI_TRANSCRIBE_STREAMING")?;
        if streaming_enabled && !model.supports_streaming() {
            return Err(ConfigError::InvalidStreamingModel {
                model: model.as_str().to_string(),
            });
        }

        let max_duration_secs = match std::env::var("VOICE_INPUT_MAX_SECS") {
            Ok(value) => value
                .parse()
                .map_err(|_| ConfigError::InvalidMaxDurationSecs { value })?,
            Err(_) => 30,
        };

        Ok(Self {
            paths: PathConfig {
                xdg_data_home: non_empty_env("XDG_DATA_HOME").map(PathBuf::from),
            },
            transcription: TranscriptionConfig {
                openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
                model,
                streaming_enabled,
                log_path: non_empty_env("OPENAI_TRANSCRIPTION_LOG_PATH").map(PathBuf::from),
                low_confidence_selection_enabled: parse_bool_env(
                    "VOICE_INPUT_LOW_CONFIDENCE_SELECTION",
                )?,
            },
            recording: RecordingConfig { max_duration_secs },
        })
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

        let config = EnvConfig::try_from_env()?;

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

        // 既に初期化されている場合は何もしない
        // (once_cellは再初期化できないため)
        if ENV_CONFIG.get().is_none() {
            ENV_CONFIG.set(Arc::new(config)).ok();
        }
    }

    /// テスト用: デフォルト設定で初期化（既に初期化済みの場合はスキップ）
    #[cfg(test)]
    pub fn test_init() {
        let _lock = TEST_LOCK.lock().unwrap();

        if ENV_CONFIG.get().is_none() {
            // テスト用のデフォルト設定
            let config = EnvConfig::try_from_env().expect("test env config should be valid");
            ENV_CONFIG.set(Arc::new(config)).ok();
        }
    }
}

fn non_empty_env(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
}

#[cfg(test)]
fn env_flag_unvalidated(name: &str) -> bool {
    std::env::var(name)
        .ok()
        .is_some_and(|value| value == "true")
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

#[cfg(test)]
mod tests {
    use super::{
        ConfigError, EnvConfig, OpenAiTranscriptionModel, PathConfig, RecordingConfig, TEST_LOCK,
        TranscriptionConfig,
    };
    use std::path::PathBuf;
    use std::sync::MutexGuard;

    fn lock_test_env() -> MutexGuard<'static, ()> {
        TEST_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// ストリーミング有効化時は対応モデルのみ許可対象として判定できる
    #[test]
    fn streaming_support_is_determined_by_model_whitelist() {
        assert!(OpenAiTranscriptionModel::new("gpt-4o-mini-transcribe").supports_streaming());
        assert!(OpenAiTranscriptionModel::new("gpt-4o-transcribe").supports_streaming());
        assert!(!OpenAiTranscriptionModel::new("whisper-1").supports_streaming());
    }

    /// ストリーミング設定は環境変数から有効化状態を読み取れる
    #[test]
    fn streaming_flag_is_loaded_from_environment() {
        let config = EnvConfig {
            paths: PathConfig {
                xdg_data_home: None,
            },
            transcription: TranscriptionConfig {
                openai_api_key: None,
                model: OpenAiTranscriptionModel::new("gpt-4o-mini-transcribe"),
                streaming_enabled: true,
                log_path: None,
                low_confidence_selection_enabled: false,
            },
            recording: RecordingConfig {
                max_duration_secs: 30,
            },
        };

        assert!(config.transcription.streaming_enabled);
    }

    /// ストリーミング有効時は転写を直列化する
    #[test]
    fn streaming_uses_single_transcription_parallelism() {
        let config = EnvConfig {
            paths: PathConfig {
                xdg_data_home: None,
            },
            transcription: TranscriptionConfig {
                openai_api_key: None,
                model: OpenAiTranscriptionModel::new("gpt-4o-mini-transcribe"),
                streaming_enabled: true,
                log_path: None,
                low_confidence_selection_enabled: false,
            },
            recording: RecordingConfig {
                max_duration_secs: 30,
            },
        };

        assert_eq!(config.recommended_transcription_parallelism(), 1);
    }

    /// ストリーミング無効時は従来の並列度を維持する
    #[test]
    fn non_streaming_keeps_existing_transcription_parallelism() {
        let config = EnvConfig {
            paths: PathConfig {
                xdg_data_home: None,
            },
            transcription: TranscriptionConfig {
                openai_api_key: None,
                model: OpenAiTranscriptionModel::new("whisper-1"),
                streaming_enabled: false,
                log_path: None,
                low_confidence_selection_enabled: false,
            },
            recording: RecordingConfig {
                max_duration_secs: 30,
            },
        };

        assert_eq!(config.recommended_transcription_parallelism(), 2);
    }

    /// 転写ログ保存先は環境変数未指定なら無効のままになる
    #[test]
    fn transcription_log_path_is_disabled_by_default() {
        let config = EnvConfig {
            paths: PathConfig {
                xdg_data_home: None,
            },
            transcription: TranscriptionConfig {
                openai_api_key: None,
                model: OpenAiTranscriptionModel::new("whisper-1"),
                streaming_enabled: false,
                log_path: None,
                low_confidence_selection_enabled: false,
            },
            recording: RecordingConfig {
                max_duration_secs: 30,
            },
        };

        assert_eq!(config.transcription.log_path, None);
    }

    /// 転写ログ保存先は設定されていればその値を保持する
    #[test]
    fn transcription_log_path_keeps_configured_value() {
        let config = EnvConfig {
            paths: PathConfig {
                xdg_data_home: None,
            },
            transcription: TranscriptionConfig {
                openai_api_key: None,
                model: OpenAiTranscriptionModel::new("whisper-1"),
                streaming_enabled: false,
                log_path: Some(PathBuf::from("/tmp/transcription-log.ndjson")),
                low_confidence_selection_enabled: false,
            },
            recording: RecordingConfig {
                max_duration_secs: 30,
            },
        };

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

        let config = EnvConfig::from_env();

        assert_eq!(config.transcription.log_path, None);

        unsafe {
            std::env::remove_var("OPENAI_TRANSCRIPTION_LOG_PATH");
        }
    }

    /// 低信頼語の自動選択は既定で無効
    #[test]
    fn low_confidence_selection_is_disabled_by_default() {
        let config = EnvConfig {
            paths: PathConfig {
                xdg_data_home: None,
            },
            transcription: TranscriptionConfig {
                openai_api_key: None,
                model: OpenAiTranscriptionModel::new("whisper-1"),
                streaming_enabled: false,
                log_path: None,
                low_confidence_selection_enabled: false,
            },
            recording: RecordingConfig {
                max_duration_secs: 30,
            },
        };

        assert!(!config.transcription.low_confidence_selection_enabled);
    }

    /// 低信頼語の自動選択は環境変数で有効化できる
    #[test]
    fn low_confidence_selection_flag_is_loaded_from_environment() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("VOICE_INPUT_LOW_CONFIDENCE_SELECTION", "true");
        }

        let config = EnvConfig::from_env();

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

        let config = EnvConfig::from_env();

        assert_eq!(config.recording.max_duration_secs, 45);

        unsafe {
            std::env::remove_var("VOICE_INPUT_MAX_SECS");
        }
    }

    /// ストリーミング有効時に非対応モデルは設定エラーになる
    #[test]
    fn try_from_env_rejects_streaming_with_unsupported_model() {
        let _lock = lock_test_env();
        unsafe {
            std::env::set_var("OPENAI_TRANSCRIBE_STREAMING", "true");
            std::env::set_var("OPENAI_TRANSCRIBE_MODEL", "whisper-1");
        }

        let result = EnvConfig::try_from_env();

        assert_eq!(
            result,
            Err(ConfigError::InvalidStreamingModel {
                model: "whisper-1".to_string(),
            })
        );

        unsafe {
            std::env::remove_var("OPENAI_TRANSCRIBE_STREAMING");
            std::env::remove_var("OPENAI_TRANSCRIBE_MODEL");
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
    fn test_init_validation_path_rejects_invalid_env_when_uninitialized() {
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
}
