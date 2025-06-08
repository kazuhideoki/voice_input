//! ショートカットキー処理のメインサービス
//! voice_inputdプロセスに統合されるShortcutServiceを提供
//! Phase 2: 権限システム統合とエラーハンドリング強化

use crate::infrastructure::permissions::AccessibilityPermissions;
use crate::ipc::IpcCmd;
use std::fmt;
use tokio::sync::mpsc;

pub mod key_handler;

use key_handler::KeyHandler;

/// ショートカットサービスエラー型
#[derive(Debug, Clone)]
pub enum ShortcutError {
    /// アクセシビリティ権限が拒否されている
    PermissionDenied(String),
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
            ShortcutError::PermissionDenied(msg) => {
                write!(f, "❌ アクセシビリティ権限エラー: {}", msg)
            }
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

    /// システム要件とアクセシビリティ権限をチェック
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
            // アクセシビリティ権限の詳細チェック
            use crate::infrastructure::permissions::{PermissionChecker, PermissionStatus};

            match AccessibilityPermissions::check_status() {
                PermissionStatus::Granted => Ok(()),
                PermissionStatus::Denied => Err(ShortcutError::PermissionDenied(
                    AccessibilityPermissions::get_error_message(),
                )),
                PermissionStatus::NotDetermined => Err(ShortcutError::PermissionDenied(
                    "アクセシビリティ権限が未確定です。初回設定が必要です。".to_string(),
                )),
            }
        }
    }

    /// 権限拒否時のユーザーガイダンス文字列を取得
    pub fn handle_permission_denied() -> String {
        use crate::infrastructure::permissions::PermissionChecker;

        format!(
            "{}\n\n{}",
            AccessibilityPermissions::get_error_message(),
            AccessibilityPermissions::get_permission_description()
        )
    }

    /// ショートカットキーサービスを開始
    ///
    /// # Arguments
    /// * `ipc_sender` - IPCコマンドを送信するためのSender
    ///
    /// # Returns
    /// * `Ok(())` - 正常に開始された場合
    /// * `Err(ShortcutError)` - 各種エラー（権限、システム要件、初期化失敗等）
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

        // システム要件とアクセシビリティ権限チェック
        Self::check_system_requirements()?;

        // IPCチャンネルが有効かテスト
        if ipc_sender.is_closed() {
            return Err(ShortcutError::IpcChannelClosed);
        }

        // KeyHandlerを非同期タスクで起動
        let key_handler = KeyHandler::new(ipc_sender);
        
        let handle = tokio::task::spawn_blocking(move || {
            key_handler.start_grab()
        });

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

    /// 互換性維持用：アクセシビリティ権限チェック（簡易版）
    /// Phase 1コードとの互換性のため残存
    #[deprecated(note = "Use ShortcutService::check_system_requirements() instead")]
    #[allow(dead_code)]
    fn check_accessibility_permission(&self) -> bool {
        AccessibilityPermissions::check()
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

        // CI環境では権限エラー、実環境ではチャンネルエラーまたは権限エラー
        assert!(result.is_err());

        if let Err(error) = result {
            println!("Expected error occurred: {}", error);
        }
    }

    #[test]
    fn test_system_requirements_check() {
        let result = ShortcutService::check_system_requirements();

        #[cfg(feature = "ci-test")]
        {
            // CI環境では常に成功
            assert!(result.is_ok());
        }

        #[cfg(not(feature = "ci-test"))]
        {
            // 実環境では権限状態次第
            match result {
                Ok(_) => println!("System requirements satisfied"),
                Err(e) => println!("System requirements not met: {}", e),
            }
        }
    }

    #[test]
    fn test_permission_denied_handler() {
        let guidance = ShortcutService::handle_permission_denied();
        assert!(guidance.contains("アクセシビリティ権限"));
        assert!(guidance.contains("Voice Input"));
    }

    #[test]
    fn test_shortcut_error_display() {
        let errors = vec![
            ShortcutError::PermissionDenied("test".to_string()),
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

        // 最初の起動（権限エラーの可能性あり）
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
        // 権限エラーの場合はテストをスキップ
    }

    #[test]
    #[allow(deprecated)]
    fn test_legacy_accessibility_permission_check() {
        let service = ShortcutService::new();

        // 互換性チェック（レガシーメソッド）
        let has_permission = service.check_accessibility_permission();

        // CI環境では常にtrue
        #[cfg(feature = "ci-test")]
        assert!(has_permission);

        // 実環境では権限状態によって変わる
        #[cfg(not(feature = "ci-test"))]
        {
            println!("Legacy permission check result: {}", has_permission);
        }
    }
}
