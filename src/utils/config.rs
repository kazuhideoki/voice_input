//! グローバル環境変数設定
//!
//! アプリケーション全体で使用する環境変数を一元管理。
//! プロセス起動時に一度だけ初期化し、以降はどこからでもアクセス可能。

use once_cell::sync::OnceCell;
use std::sync::Arc;

/// グローバル環境変数設定
static ENV_CONFIG: OnceCell<Arc<EnvConfig>> = OnceCell::new();

#[cfg(test)]
use std::sync::Mutex;

#[cfg(test)]
static TEST_LOCK: Mutex<()> = Mutex::new(());

/// OpenAI の文字起こしモデル
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OpenAiTranscriptionModel {
    Gpt4oMiniTranscribe,
    Gpt4oTranscribe,
}

impl OpenAiTranscriptionModel {
    const DEFAULT: Self = Self::Gpt4oMiniTranscribe;

    /// 環境変数からモデル設定を生成
    pub fn from_env() -> Result<Self, String> {
        match std::env::var("OPENAI_TRANSCRIBE_MODEL") {
            Ok(value) => Self::parse(&value),
            Err(_) => Ok(Self::DEFAULT),
        }
    }

    /// 文字列からモデル設定を生成
    pub fn parse(value: &str) -> Result<Self, String> {
        match value {
            "gpt-4o-mini-transcribe" => Ok(Self::Gpt4oMiniTranscribe),
            "gpt-4o-transcribe" => Ok(Self::Gpt4oTranscribe),
            unsupported => Err(format!(
                "OPENAI_TRANSCRIBE_MODEL={} is unsupported. Supported models: gpt-4o-mini-transcribe, gpt-4o-transcribe",
                unsupported
            )),
        }
    }

    /// モデル名を文字列で取得
    pub fn as_str(&self) -> &str {
        match self {
            Self::Gpt4oMiniTranscribe => "gpt-4o-mini-transcribe",
            Self::Gpt4oTranscribe => "gpt-4o-transcribe",
        }
    }
}

/// 環境変数設定
#[derive(Debug, Clone)]
pub struct EnvConfig {
    /// OpenAI APIキー
    pub openai_api_key: Option<String>,
    /// XDG Data Home ディレクトリ
    pub xdg_data_home: Option<String>,
    /// 環境変数ファイルのパス
    pub env_path: Option<String>,
    /// OpenAI 文字起こしモデル
    pub openai_transcribe_model: OpenAiTranscriptionModel,
    /// ストリーミング直接入力を有効にする
    pub openai_transcribe_streaming: bool,
    /// 転写ログ保存先パス
    pub openai_transcription_log_path: Option<String>,
    /// 低信頼語の自動選択を有効にする
    pub low_confidence_selection_enabled: bool,
}

impl EnvConfig {
    /// 環境変数から設定を構築
    pub fn from_env() -> Result<Self, String> {
        Ok(Self {
            openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
            xdg_data_home: std::env::var("XDG_DATA_HOME").ok(),
            env_path: std::env::var("VOICE_INPUT_ENV_PATH").ok(),
            openai_transcribe_model: OpenAiTranscriptionModel::from_env()?,
            openai_transcribe_streaming: std::env::var("OPENAI_TRANSCRIBE_STREAMING")
                .ok()
                .is_some_and(|value| value == "true"),
            openai_transcription_log_path: non_empty_env("OPENAI_TRANSCRIPTION_LOG_PATH"),
            low_confidence_selection_enabled: std::env::var("VOICE_INPUT_LOW_CONFIDENCE_SELECTION")
                .ok()
                .is_some_and(|value| value == "true"),
        })
    }

    /// 転写の推奨同時実行数を返す
    pub fn recommended_transcription_parallelism(&self) -> usize {
        if self.openai_transcribe_streaming {
            1
        } else {
            2
        }
    }

    /// 環境変数から設定を初期化
    ///
    /// アプリケーション起動時に呼び出す。
    /// 既に初期化済みの場合は何もせずOkを返す（冪等性を保証）。
    pub fn init() -> Result<(), Box<dyn std::error::Error>> {
        if ENV_CONFIG.get().is_some() {
            return Ok(());
        }

        let config =
            EnvConfig::from_env().map_err(|e| -> Box<dyn std::error::Error> { e.into() })?;

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
            let config = EnvConfig::from_env()
                .expect("test environment must use a supported transcription model");
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
mod tests {
    use super::{EnvConfig, OpenAiTranscriptionModel, TEST_LOCK};

    /// 対応モデルは文字列から列挙型へ変換できる
    #[test]
    fn supported_models_are_parsed() {
        assert_eq!(
            OpenAiTranscriptionModel::parse("gpt-4o-mini-transcribe").unwrap(),
            OpenAiTranscriptionModel::Gpt4oMiniTranscribe
        );
        assert_eq!(
            OpenAiTranscriptionModel::parse("gpt-4o-transcribe").unwrap(),
            OpenAiTranscriptionModel::Gpt4oTranscribe
        );
    }

    /// ストリーミング設定は環境変数から有効化状態を読み取れる
    #[test]
    fn streaming_flag_is_loaded_from_environment() {
        let config = EnvConfig {
            openai_api_key: None,
            xdg_data_home: None,
            env_path: None,
            openai_transcribe_model: OpenAiTranscriptionModel::Gpt4oMiniTranscribe,
            openai_transcribe_streaming: true,
            openai_transcription_log_path: None,
            low_confidence_selection_enabled: false,
        };

        assert!(config.openai_transcribe_streaming);
    }

    /// ストリーミング有効時は転写を直列化する
    #[test]
    fn streaming_uses_single_transcription_parallelism() {
        let config = EnvConfig {
            openai_api_key: None,
            xdg_data_home: None,
            env_path: None,
            openai_transcribe_model: OpenAiTranscriptionModel::Gpt4oMiniTranscribe,
            openai_transcribe_streaming: true,
            openai_transcription_log_path: None,
            low_confidence_selection_enabled: false,
        };

        assert_eq!(config.recommended_transcription_parallelism(), 1);
    }

    /// ストリーミング無効時は従来の並列度を維持する
    #[test]
    fn non_streaming_keeps_existing_transcription_parallelism() {
        let config = EnvConfig {
            openai_api_key: None,
            xdg_data_home: None,
            env_path: None,
            openai_transcribe_model: OpenAiTranscriptionModel::Gpt4oTranscribe,
            openai_transcribe_streaming: false,
            openai_transcription_log_path: None,
            low_confidence_selection_enabled: false,
        };

        assert_eq!(config.recommended_transcription_parallelism(), 2);
    }

    /// 転写ログ保存先は環境変数未指定なら無効のままになる
    #[test]
    fn transcription_log_path_is_disabled_by_default() {
        let config = EnvConfig {
            openai_api_key: None,
            xdg_data_home: None,
            env_path: None,
            openai_transcribe_model: OpenAiTranscriptionModel::Gpt4oTranscribe,
            openai_transcribe_streaming: false,
            openai_transcription_log_path: None,
            low_confidence_selection_enabled: false,
        };

        assert_eq!(config.openai_transcription_log_path, None);
    }

    /// 転写ログ保存先は設定されていればその値を保持する
    #[test]
    fn transcription_log_path_keeps_configured_value() {
        let config = EnvConfig {
            openai_api_key: None,
            xdg_data_home: None,
            env_path: None,
            openai_transcribe_model: OpenAiTranscriptionModel::Gpt4oTranscribe,
            openai_transcribe_streaming: false,
            openai_transcription_log_path: Some("/tmp/transcription-log.ndjson".to_string()),
            low_confidence_selection_enabled: false,
        };

        assert_eq!(
            config.openai_transcription_log_path.as_deref(),
            Some("/tmp/transcription-log.ndjson")
        );
    }

    /// 転写ログ保存先は空文字なら無効扱いになる
    #[test]
    fn transcription_log_path_treats_empty_env_as_disabled() {
        let _lock = TEST_LOCK.lock().unwrap();
        // SAFETY: テストロックで環境変数アクセスを直列化している。
        unsafe {
            std::env::set_var("OPENAI_TRANSCRIPTION_LOG_PATH", "   ");
        }

        let config = EnvConfig::from_env().unwrap();

        assert_eq!(config.openai_transcription_log_path, None);

        // SAFETY: テストロックで環境変数アクセスを直列化している。
        unsafe {
            std::env::remove_var("OPENAI_TRANSCRIPTION_LOG_PATH");
        }
    }

    /// 低信頼語の自動選択は既定で無効
    #[test]
    fn low_confidence_selection_is_disabled_by_default() {
        let config = EnvConfig {
            openai_api_key: None,
            xdg_data_home: None,
            env_path: None,
            openai_transcribe_model: OpenAiTranscriptionModel::Gpt4oTranscribe,
            openai_transcribe_streaming: false,
            openai_transcription_log_path: None,
            low_confidence_selection_enabled: false,
        };

        assert!(!config.low_confidence_selection_enabled);
    }

    /// 低信頼語の自動選択は環境変数で有効化できる
    #[test]
    fn low_confidence_selection_flag_is_loaded_from_environment() {
        let _lock = TEST_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("VOICE_INPUT_LOW_CONFIDENCE_SELECTION", "true");
        }

        let config = EnvConfig::from_env().unwrap();

        assert!(config.low_confidence_selection_enabled);

        unsafe {
            std::env::remove_var("VOICE_INPUT_LOW_CONFIDENCE_SELECTION");
        }
    }

    /// 未対応モデルは設定値として拒否する
    #[test]
    fn unsupported_model_is_rejected() {
        let error = OpenAiTranscriptionModel::parse("whisper-1").unwrap_err();

        assert!(error.contains("whisper-1"));
    }

    /// 未対応モデルが環境変数に指定されている場合は設定構築に失敗する
    #[test]
    fn unsupported_model_in_env_fails_config_loading() {
        let _lock = TEST_LOCK.lock().unwrap();
        unsafe {
            std::env::set_var("OPENAI_TRANSCRIBE_MODEL", "whisper-1");
        }

        let result = EnvConfig::from_env();

        assert!(result.is_err());

        unsafe {
            std::env::remove_var("OPENAI_TRANSCRIBE_MODEL");
        }
    }
}
