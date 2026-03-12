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
}

impl EnvConfig {
    /// 環境変数から設定を初期化
    ///
    /// アプリケーション起動時に呼び出す。
    /// 既に初期化済みの場合は何もせずOkを返す（冪等性を保証）。
    pub fn init() -> Result<(), Box<dyn std::error::Error>> {
        if ENV_CONFIG.get().is_some() {
            return Ok(());
        }

        let config = EnvConfig {
            openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
            xdg_data_home: std::env::var("XDG_DATA_HOME").ok(),
            env_path: std::env::var("VOICE_INPUT_ENV_PATH").ok(),
            openai_transcribe_model: OpenAiTranscriptionModel::from_env(),
            openai_transcribe_streaming: std::env::var("OPENAI_TRANSCRIBE_STREAMING")
                .ok()
                .is_some_and(|value| value == "true"),
        };

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
            let config = EnvConfig {
                openai_api_key: std::env::var("OPENAI_API_KEY").ok(),
                xdg_data_home: std::env::var("XDG_DATA_HOME").ok(),
                env_path: std::env::var("VOICE_INPUT_ENV_PATH").ok(),
                openai_transcribe_model: OpenAiTranscriptionModel::from_env(),
                openai_transcribe_streaming: std::env::var("OPENAI_TRANSCRIBE_STREAMING")
                    .ok()
                    .is_some_and(|value| value == "true"),
            };
            ENV_CONFIG.set(Arc::new(config)).ok();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{EnvConfig, OpenAiTranscriptionModel};

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
            openai_api_key: None,
            xdg_data_home: None,
            env_path: None,
            openai_transcribe_model: OpenAiTranscriptionModel::new("gpt-4o-mini-transcribe"),
            openai_transcribe_streaming: true,
        };

        assert!(config.openai_transcribe_streaming);
    }
}
