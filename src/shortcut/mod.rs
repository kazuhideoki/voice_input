//! ショートカットキー処理のメインサービス
//! voice_inputdプロセスに統合されるShortcutServiceを提供

use crate::ipc::IpcCmd;
use std::fmt;
use tokio::sync::mpsc;

pub mod key_handler;

use key_handler::KeyHandler;

/// ショートカットサービスエラー型
#[derive(Debug, Clone)]
pub enum ShortcutError {
    /// rdev初期化に失敗
    RdevInitFailed(String),
    /// IPCチャンネルがクローズされている
    IpcChannelClosed,
    /// システム要件が満たされていない
    SystemRequirementNotMet(String),
}

impl fmt::Display for ShortcutError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShortcutError::RdevInitFailed(msg) => {
                write!(f, "❌ キーボードフック初期化エラー: {}", msg)
            }
            ShortcutError::IpcChannelClosed => {
                write!(f, "❌ IPC通信チャンネルが切断されています")
            }
            ShortcutError::SystemRequirementNotMet(msg) => {
                write!(f, "❌ システム要件エラー: {}", msg)
            }
        }
    }
}

impl std::error::Error for ShortcutError {}

/// ショートカットキー機能の管理を行うサービス
pub struct ShortcutService {
    enabled: bool,
    key_handler: Option<tokio::task::JoinHandle<Result<(), String>>>,
}

impl ShortcutService {
    /// 新しいShortcutServiceインスタンスを作成
    pub fn new() -> Self {
        Self {
            enabled: false,
            key_handler: None,
        }
    }

    /// システム要件をチェック
    pub fn check_system_requirements() -> Result<(), ShortcutError> {
        // macOS以外のプラットフォームはサポート対象外
        #[cfg(not(target_os = "macos"))]
        {
            return Err(ShortcutError::SystemRequirementNotMet(
                "ショートカット機能はmacOSでのみサポートされています".to_string(),
            ));
        }

        #[cfg(target_os = "macos")]
        {
            // macOSでは特に追加のチェックなし
            Ok(())
        }
    }

    /// ショートカットキーサービスを開始
    ///
    /// # Arguments
    /// * `ipc_sender` - IPCコマンドを送信するためのSender
    ///
    /// # Returns
    /// * `Ok(())` - 正常に開始された場合
    /// * `Err(ShortcutError)` - 各種エラー（システム要件、初期化失敗等）
    pub async fn start(
        &mut self,
        ipc_sender: mpsc::UnboundedSender<IpcCmd>,
    ) -> Result<(), ShortcutError> {
        // 既に起動済みの場合はエラー
        if self.enabled {
            return Err(ShortcutError::SystemRequirementNotMet(
                "ShortcutService is already enabled".to_string(),
            ));
        }

        // システム要件チェック
        Self::check_system_requirements()?;

        // IPCチャンネルが有効かテスト
        if ipc_sender.is_closed() {
            return Err(ShortcutError::IpcChannelClosed);
        }

        // KeyHandlerを非同期タスクで起動
        let key_handler = KeyHandler::new(ipc_sender);

        let handle = tokio::task::spawn_blocking(move || key_handler.start_grab());

        self.key_handler = Some(handle);
        self.enabled = true;

        println!("ShortcutService started successfully");
        Ok(())
    }

    /// ショートカットキーサービスを停止
    pub async fn stop(&mut self) -> Result<(), ShortcutError> {
        if !self.enabled {
            return Ok(()); // 既に停止済み
        }

        if let Some(handle) = self.key_handler.take() {
            handle.abort();

            // タスクの完了を待機（タイムアウト付き）
            match tokio::time::timeout(tokio::time::Duration::from_millis(1000), handle).await {
                Ok(_) => {
                    println!("KeyHandler task completed gracefully");
                }
                Err(_) => {
                    println!("KeyHandler task terminated (timeout)");
                }
            }
        }

        self.enabled = false;
        println!("ShortcutService stopped");
        Ok(())
    }

    /// サービスが有効かどうかを返す
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
}

impl Default for ShortcutService {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[tokio::test]
    async fn test_shortcut_service_new() {
        let service = ShortcutService::new();
        assert!(!service.is_enabled());
    }

    #[tokio::test]
    async fn test_shortcut_service_default() {
        let service = ShortcutService::default();
        assert!(!service.is_enabled());
    }

    #[tokio::test]
    async fn test_stop_when_not_started() {
        let mut service = ShortcutService::new();
        let result = service.stop().await;
        assert!(result.is_ok());
        assert!(!service.is_enabled());
    }

    #[tokio::test]
    async fn test_ipc_channel_closed_error() {
        let mut service = ShortcutService::new();
        let (tx, rx) = mpsc::unbounded_channel();

        // チャンネルを閉じる
        drop(rx);

        let result = service.start(tx).await;

        // チャンネルエラー
        assert!(result.is_err());

        if let Err(error) = result {
            println!("Expected error occurred: {}", error);
        }
    }

    #[test]
    fn test_system_requirements_check() {
        let result = ShortcutService::check_system_requirements();

        #[cfg(target_os = "macos")]
        {
            assert!(result.is_ok());
        }

        #[cfg(not(target_os = "macos"))]
        {
            assert!(result.is_err());
        }
    }

    #[test]
    fn test_shortcut_error_display() {
        let errors = vec![
            ShortcutError::RdevInitFailed("test".to_string()),
            ShortcutError::IpcChannelClosed,
            ShortcutError::SystemRequirementNotMet("test".to_string()),
        ];

        for error in errors {
            let display = format!("{}", error);
            assert!(display.contains("❌"));
            println!("Error display: {}", display);
        }
    }

    #[tokio::test]
    #[ignore] // 実際のrdev::grabを使用するため手動テストのみ
    async fn test_double_start_error() {
        let mut service = ShortcutService::new();
        let (tx, _rx) = mpsc::unbounded_channel();

        // 最初の起動
        let result1 = service.start(tx.clone()).await;

        if result1.is_ok() {
            // 成功した場合、2回目の起動はエラーになるべき
            let result2 = service.start(tx).await;
            assert!(result2.is_err());

            if let Err(ShortcutError::SystemRequirementNotMet(msg)) = result2 {
                assert!(msg.contains("already enabled"));
            } else {
                panic!("Expected SystemRequirementNotMet error");
            }

            // クリーンアップ
            let _ = service.stop().await;
        }
    }
}