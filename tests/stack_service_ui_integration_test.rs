//! StackServiceとUIシステムの統合テスト
//!
//! StackServiceのUI通知機能、スタック操作時のUI更新、
//! モックUIハンドラーを使用した統合テストを実行します。

use std::sync::{Arc, Mutex};
use voice_input::application::stack_service::{StackService, UiNotificationHandler};
use voice_input::infrastructure::ui::UiNotification;

#[derive(Debug, Default)]
struct MockUiHandler {
    notifications: Arc<Mutex<Vec<UiNotification>>>,
}

impl MockUiHandler {
    fn new() -> Self {
        Self {
            notifications: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn get_notifications(&self) -> Vec<UiNotification> {
        self.notifications.lock().unwrap().clone()
    }

    fn clear_notifications(&self) {
        self.notifications.lock().unwrap().clear();
    }
}

impl UiNotificationHandler for MockUiHandler {
    fn notify(&self, notification: UiNotification) -> Result<(), String> {
        self.notifications.lock().unwrap().push(notification);
        Ok(())
    }
}

#[test]
fn test_stack_service_with_ui_notifications() {
    let mut service = StackService::new();
    let handler = Arc::new(MockUiHandler::new());

    // UI通知ハンドラーを設定
    let handler_trait: Arc<dyn UiNotificationHandler> = handler.clone();
    service.set_ui_handler(Arc::downgrade(&handler_trait));

    // スタックモード有効化
    service.enable_stack_mode();

    let notifications = handler.get_notifications();
    assert_eq!(notifications.len(), 1);
    if let UiNotification::ModeChanged(enabled) = &notifications[0] {
        assert!(enabled);
    } else {
        panic!("Expected ModeChanged notification");
    }

    handler.clear_notifications();

    // スタック保存
    let id = service.save_stack("Test text".to_string());
    assert_eq!(id, 1);

    let notifications = handler.get_notifications();
    assert_eq!(notifications.len(), 1);
    if let UiNotification::StackAdded(stack_info) = &notifications[0] {
        assert_eq!(stack_info.number, 1);
        assert_eq!(stack_info.preview, "Test text");
        assert!(!stack_info.is_active);
        assert_eq!(stack_info.char_count, 9);
    } else {
        panic!("Expected StackAdded notification");
    }

    handler.clear_notifications();

    // スタックアクセス
    let stack = service.get_stack_with_context(1).unwrap();
    assert_eq!(stack.text, "Test text");

    let notifications = handler.get_notifications();
    assert_eq!(notifications.len(), 1);
    if let UiNotification::StackAccessed(accessed_id) = &notifications[0] {
        assert_eq!(*accessed_id, 1);
    } else {
        panic!("Expected StackAccessed notification");
    }

    handler.clear_notifications();

    // スタッククリア
    service.clear_stacks();

    let notifications = handler.get_notifications();
    assert_eq!(notifications.len(), 1);
    if let UiNotification::StacksCleared = &notifications[0] {
        // 正常
    } else {
        panic!("Expected StacksCleared notification");
    }

    handler.clear_notifications();

    // スタックモード無効化
    service.disable_stack_mode();

    let notifications = handler.get_notifications();
    assert_eq!(notifications.len(), 1);
    if let UiNotification::ModeChanged(enabled) = &notifications[0] {
        assert!(!enabled);
    } else {
        panic!("Expected ModeChanged notification");
    }
}

#[test]
fn test_stack_service_without_ui_handler() {
    let mut service = StackService::new();

    // UI通知ハンドラーを設定せずに操作
    service.enable_stack_mode();
    let _id = service.save_stack("Test".to_string());
    service.clear_stacks();
    service.disable_stack_mode();

    // エラーが発生しないことを確認
}

#[test]
fn test_stack_preview_truncation() {
    let mut service = StackService::new();
    let handler = Arc::new(MockUiHandler::new());
    let handler_trait: Arc<dyn UiNotificationHandler> = handler.clone();
    service.set_ui_handler(Arc::downgrade(&handler_trait));
    service.enable_stack_mode();

    handler.clear_notifications();

    // 長いテキストを保存
    let long_text = "This is a very long text that should be truncated for display purposes in the UI component.";
    let _id = service.save_stack(long_text.to_string());

    let notifications = handler.get_notifications();
    assert_eq!(notifications.len(), 1);

    if let UiNotification::StackAdded(stack_info) = &notifications[0] {
        // プレビューは40文字 + "..." の最大43文字以内になる
        assert!(stack_info.preview.chars().count() <= StackService::PREVIEW_LENGTH + 3);
        assert!(stack_info.preview.ends_with("..."));
        assert_eq!(stack_info.char_count, long_text.len());
    } else {
        panic!("Expected StackAdded notification");
    }
}
