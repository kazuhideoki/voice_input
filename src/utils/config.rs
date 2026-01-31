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

/// 環境変数設定
#[derive(Debug, Clone)]
pub struct EnvConfig {
    /// OpenAI APIキー
    pub openai_api_key: Option<String>,
    /// XDG Data Home ディレクトリ
    pub xdg_data_home: Option<String>,
    /// 環境変数ファイルのパス
    pub env_path: Option<String>,
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
            };
            ENV_CONFIG.set(Arc::new(config)).ok();
        }
    }
}
