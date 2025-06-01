//! スタックマネージャーUIコンポーネントのテスト
//!
//! StackManagerAppの基本動作、通知処理、状態管理をテストします。
//! GUIコンポーネントのため、実際の描画テストは制限されます。

use tokio::sync::mpsc;
use voice_input::infrastructure::ui::{StackDisplayInfo, StackManagerApp, UiNotification, UiState};

#[test]
fn test_stack_manager_app_creation() {
    let (_tx, rx) = mpsc::unbounded_channel();
    let _app = StackManagerApp::new(rx);

    // 初期状態確認は直接アクセスできないので、正常に作成されることを確認
    // 実際の検証はStackManagerAppの実装に依存するため、基本的なテストのみ実施
}

#[test]
fn test_ui_state_default() {
    let state = UiState::default();

    assert!(!state.stack_mode_enabled);
    assert_eq!(state.total_count, 0);
    assert!(state.stacks.is_empty());
    assert_eq!(state.last_accessed_id, None);
}

#[test]
fn test_ui_notifications() {
    // UiNotificationの各バリアントが正常に作成できることを確認
    let stack_info = StackDisplayInfo {
        number: 1,
        preview: "Test".to_string(),
        created_at: "2024-01-01".to_string(),
        is_active: false,
        char_count: 4,
    };

    let notifications = vec![
        UiNotification::StackAdded(stack_info),
        UiNotification::StackAccessed(1),
        UiNotification::StacksCleared,
        UiNotification::ModeChanged(true),
    ];

    // 各通知が正常に作成されることを確認
    assert_eq!(notifications.len(), 4);
}

#[test]
fn test_stack_display_info_preview_truncation() {
    let long_text = "This is a very long text that should be truncated for display purposes";
    let stack_info = StackDisplayInfo {
        number: 1,
        preview: long_text[..40.min(long_text.len())].to_string(),
        created_at: "2024-01-01 12:00:00".to_string(),
        is_active: false,
        char_count: long_text.len(),
    };

    assert!(stack_info.preview.len() <= 40);
    assert_eq!(stack_info.char_count, long_text.len());
}

#[test]
fn test_ui_error_display() {
    use voice_input::infrastructure::ui::UiError;

    let error1 = UiError::InitializationFailed("Test error".to_string());
    let error2 = UiError::ChannelClosed;
    let error3 = UiError::RenderingError("Render error".to_string());

    assert!(error1.to_string().contains("initialization failed"));
    assert!(error2.to_string().contains("channel closed"));
    assert!(error3.to_string().contains("rendering error"));
}
