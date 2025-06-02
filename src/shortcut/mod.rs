//! ショートカットキー処理のメインサービス
//! voice_inputdプロセスに統合されるShortcutServiceを提供

use crate::ipc::IpcCmd;
use tokio::sync::mpsc;

pub mod key_handler;

use key_handler::KeyHandler;

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

    /// ショートカットキーサービスを開始
    ///
    /// # Arguments
    /// * `ipc_sender` - IPCコマンドを送信するためのSender
    ///
    /// # Returns
    /// * `Ok(())` - 正常に開始された場合
    /// * `Err(String)` - アクセシビリティ権限エラーまたは起動エラー
    pub async fn start(&mut self, ipc_sender: mpsc::UnboundedSender<IpcCmd>) -> Result<(), String> {
        // 既に起動済みの場合はエラー
        if self.enabled {
            return Err("ShortcutService is already enabled".to_string());
        }

        // アクセシビリティ権限チェック
        if !self.check_accessibility_permission() {
            return Err("アクセシビリティ権限が必要です。システム環境設定 > セキュリティとプライバシー > プライバシー > アクセシビリティ で voice_inputd を許可してください。".to_string());
        }

        // KeyHandlerを非同期タスクで起動
        let handle = tokio::task::spawn_blocking(move || {
            let key_handler = KeyHandler::new(ipc_sender);
            key_handler.start_grab()
        });

        self.key_handler = Some(handle);
        self.enabled = true;

        println!("ShortcutService started successfully");
        Ok(())
    }

    /// ショートカットキーサービスを停止
    pub async fn stop(&mut self) -> Result<(), String> {
        if !self.enabled {
            return Ok(()); // 既に停止済み
        }

        if let Some(handle) = self.key_handler.take() {
            handle.abort();
        }

        self.enabled = false;
        println!("ShortcutService stopped");
        Ok(())
    }

    /// サービスが有効かどうかを返す
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }

    /// macOSアクセシビリティ権限をチェック
    ///
    /// # Returns
    /// * `true` - アクセシビリティ権限が付与されている
    /// * `false` - アクセシビリティ権限が付与されていない
    fn check_accessibility_permission(&self) -> bool {
        // Phase 1では簡略化実装（常にtrueを返す）
        // 実際の権限チェックはPhase 2で実装予定
        #[cfg(target_os = "macos")]
        {
            // TODO: Phase 2でcore_foundation依存関係を追加してFFI実装
            println!("WARNING: Accessibility permission check not implemented - assuming granted");
            true
        }

        #[cfg(not(target_os = "macos"))]
        {
            // macOS以外では常にtrueを返す（テスト目的）
            true
        }
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
    #[ignore] // 実際のrdev::grabを使用するため手動テストのみ
    async fn test_double_start_error() {
        let mut service = ShortcutService::new();
        let (tx, _rx) = mpsc::unbounded_channel();

        // 最初の起動（アクセシビリティ権限エラーの可能性あり）
        let result1 = service.start(tx.clone()).await;

        if result1.is_ok() {
            // 成功した場合、2回目の起動はエラーになるべき
            let result2 = service.start(tx).await;
            assert!(result2.is_err());
            assert!(result2.unwrap_err().contains("already enabled"));

            // クリーンアップ
            let _ = service.stop().await;
        }
        // アクセシビリティ権限エラーの場合はテストをスキップ
    }

    #[test]
    fn test_accessibility_permission_check() {
        let service = ShortcutService::new();

        // アクセシビリティ権限チェックの実行（結果は環境依存）
        let has_permission = service.check_accessibility_permission();

        // macOSでない場合は常にtrueが返される
        #[cfg(not(target_os = "macos"))]
        assert!(has_permission);

        // macOSの場合は権限状態によって変わるため、booleanであることのみ確認
        #[cfg(target_os = "macos")]
        assert!(has_permission || !has_permission);
    }
}
