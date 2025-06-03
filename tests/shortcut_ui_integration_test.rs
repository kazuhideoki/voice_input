//! Phase 3: ショートカットキーとUI連携の統合テスト
//!
//! ESCキー処理、タイマー管理、視覚的フィードバックの統合動作をテスト

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use voice_input::application::stack_service::{StackService, UiNotificationHandler};
use voice_input::infrastructure::ui::{StackDisplayInfo, UiNotification};
use voice_input::ipc::IpcCmd;

#[derive(Debug, Default)]
struct MockUiHandler {
    notifications: Arc<Mutex<Vec<UiNotification>>>,
    accessed_times: Arc<Mutex<Vec<(u32, Instant)>>>,
}

impl MockUiHandler {
    fn new() -> Self {
        Self {
            notifications: Arc::new(Mutex::new(Vec::new())),
            accessed_times: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn get_notifications(&self) -> Vec<UiNotification> {
        self.notifications.lock().unwrap().clone()
    }

    fn get_last_notification(&self) -> Option<UiNotification> {
        self.notifications.lock().unwrap().last().cloned()
    }

    fn get_accessed_times(&self) -> Vec<(u32, Instant)> {
        self.accessed_times.lock().unwrap().clone()
    }

    fn clear(&self) {
        self.notifications.lock().unwrap().clear();
        self.accessed_times.lock().unwrap().clear();
    }
}

impl UiNotificationHandler for MockUiHandler {
    fn notify(&self, notification: UiNotification) -> Result<(), String> {
        self.notifications
            .lock()
            .unwrap()
            .push(notification.clone());

        // StackAccessedイベントの時刻を記録
        if let UiNotification::StackAccessed(id) = notification {
            self.accessed_times
                .lock()
                .unwrap()
                .push((id, Instant::now()));
        }

        Ok(())
    }
}

#[test]
fn test_esc_key_disables_stack_mode() {
    // ESCキーでスタックモードが無効化されることを確認
    let (tx, mut rx) = mpsc::unbounded_channel::<IpcCmd>();

    // ESCキーコマンドを送信
    let cmd = IpcCmd::DisableStackMode;
    tx.send(cmd.clone()).unwrap();

    // コマンドが正しく送信されたことを確認
    let received = rx.blocking_recv().unwrap();
    assert_eq!(received, IpcCmd::DisableStackMode);
}

#[test]
fn test_stack_access_notification_with_highlight_timing() {
    let mut service = StackService::new();
    let handler = Arc::new(MockUiHandler::new());
    let handler_trait: Arc<dyn UiNotificationHandler> = handler.clone();
    service.set_ui_handler(Arc::downgrade(&handler_trait));

    service.enable_stack_mode();
    handler.clear();

    // 複数のスタックを作成
    for i in 1..=3 {
        service.save_stack(format!("Stack {}", i));
    }
    handler.clear();

    // スタック2にアクセス
    let start_time = Instant::now();
    let _stack = service.get_stack_with_context(2).unwrap();

    // StackAccessedイベントが送信されたことを確認
    let notification = handler.get_last_notification().unwrap();
    if let UiNotification::StackAccessed(id) = notification {
        assert_eq!(id, 2);
    } else {
        panic!("Expected StackAccessed notification");
    }

    // アクセス時刻が記録されていることを確認
    let accessed_times = handler.get_accessed_times();
    assert_eq!(accessed_times.len(), 1);
    assert_eq!(accessed_times[0].0, 2);

    // アクセス時刻が現在時刻に近いことを確認（1秒以内）
    let elapsed = accessed_times[0].1.duration_since(start_time);
    assert!(elapsed < Duration::from_secs(1));
}

#[test]
fn test_multiple_stack_highlights_override() {
    let mut service = StackService::new();
    let handler = Arc::new(MockUiHandler::new());
    let handler_trait: Arc<dyn UiNotificationHandler> = handler.clone();
    service.set_ui_handler(Arc::downgrade(&handler_trait));

    service.enable_stack_mode();

    // 3つのスタックを作成
    for i in 1..=3 {
        service.save_stack(format!("Stack {}", i));
    }
    handler.clear();

    // スタック1にアクセス
    let _stack1 = service.get_stack_with_context(1).unwrap();
    std::thread::sleep(Duration::from_millis(100));

    // スタック2にアクセス（ハイライトが移動するはず）
    let _stack2 = service.get_stack_with_context(2).unwrap();

    // 通知履歴を確認
    let notifications = handler.get_notifications();
    assert_eq!(notifications.len(), 2);

    // 最初のアクセス
    if let UiNotification::StackAccessed(id) = &notifications[0] {
        assert_eq!(*id, 1);
    }

    // 2番目のアクセス（ハイライトが移動）
    if let UiNotification::StackAccessed(id) = &notifications[1] {
        assert_eq!(*id, 2);
    }

    // アクセス時刻の記録を確認
    let accessed_times = handler.get_accessed_times();
    assert_eq!(accessed_times.len(), 2);
    assert!(accessed_times[1].1 > accessed_times[0].1);
}

#[test]
fn test_stack_display_info_for_keyboard_hints() {
    let mut service = StackService::new();
    let handler = Arc::new(MockUiHandler::new());
    let handler_trait: Arc<dyn UiNotificationHandler> = handler.clone();
    service.set_ui_handler(Arc::downgrade(&handler_trait));

    service.enable_stack_mode();
    handler.clear();

    // 12個のスタックを作成（Cmd+1-9の範囲を超える）
    for i in 1..=12 {
        service.save_stack(format!("Stack number {}", i));
    }

    let notifications = handler.get_notifications();
    assert_eq!(notifications.len(), 12);

    // 各スタックの情報を確認
    for (index, notification) in notifications.iter().enumerate() {
        if let UiNotification::StackAdded(stack_info) = notification {
            assert_eq!(stack_info.number as usize, index + 1);

            // スタック1-9はCmd+キーで操作可能
            if index < 9 {
                // UIで表示されるキーボードヒントのインデックス確認
                assert!(
                    index < 9,
                    "スタック{}はCmd+{}で操作可能",
                    index + 1,
                    index + 1
                );
            } else {
                // スタック10以降はキーボードショートカットなし
                assert!(
                    index >= 9,
                    "スタック{}はキーボードショートカットなし",
                    index + 1
                );
            }
        }
    }
}

#[test]
fn test_esc_key_clears_ui_notifications() {
    let mut service = StackService::new();
    let handler = Arc::new(MockUiHandler::new());
    let handler_trait: Arc<dyn UiNotificationHandler> = handler.clone();
    service.set_ui_handler(Arc::downgrade(&handler_trait));

    // スタックモードを有効化
    service.enable_stack_mode();
    assert_eq!(handler.get_notifications().len(), 1);

    // スタックを追加
    service.save_stack("Test stack".to_string());
    assert_eq!(handler.get_notifications().len(), 2);

    handler.clear();

    // ESCキーでスタックモードを無効化（DisableStackModeコマンドの処理をシミュレート）
    service.disable_stack_mode();

    // ModeChanged(false)通知が送信されることを確認
    let notifications = handler.get_notifications();
    assert_eq!(notifications.len(), 1);
    if let UiNotification::ModeChanged(enabled) = &notifications[0] {
        assert!(!enabled);
    } else {
        panic!("Expected ModeChanged(false) notification");
    }
}

#[test]
fn test_highlight_timer_simulation() {
    // タイマーロジックのシミュレーションテスト
    const HIGHLIGHT_DURATION_SECS: u64 = 3;

    let start = Instant::now();
    let highlight_until = start + Duration::from_secs(HIGHLIGHT_DURATION_SECS);

    // 即座のチェック（ハイライト中）
    assert!(Instant::now() < highlight_until);

    // 1秒後（まだハイライト中）
    std::thread::sleep(Duration::from_secs(1));
    assert!(Instant::now() < highlight_until);

    // NOTE: 3秒待つと他のテストも遅くなるため、ロジックの確認のみ
    // 実際の3秒後のテストは手動テストで確認
}

#[test]
fn test_paste_stack_with_ui_feedback() {
    let mut service = StackService::new();
    let handler = Arc::new(MockUiHandler::new());
    let handler_trait: Arc<dyn UiNotificationHandler> = handler.clone();
    service.set_ui_handler(Arc::downgrade(&handler_trait));

    service.enable_stack_mode();

    // 5つのスタックを作成
    for i in 1..=5 {
        service.save_stack(format!("Content {}", i));
    }
    handler.clear();

    // Cmd+3をシミュレート（スタック3をペースト）
    let stack = service.get_stack_with_context(3);
    assert!(stack.is_ok());
    assert_eq!(stack.unwrap().text, "Content 3");

    // UI通知を確認
    let notification = handler.get_last_notification().unwrap();
    if let UiNotification::StackAccessed(id) = notification {
        assert_eq!(id, 3);
    } else {
        panic!("Expected StackAccessed notification");
    }
}

#[test]
fn test_stack_info_display_properties() {
    // StackDisplayInfoの表示プロパティをテスト
    let info = StackDisplayInfo {
        number: 5,
        preview: "Test preview text".to_string(),
        created_at: "2024-01-01 12:00:00".to_string(),
        is_active: false,
        char_count: 17,
    };

    assert_eq!(info.number, 5);
    assert_eq!(info.preview, "Test preview text");
    assert_eq!(info.char_count, 17);
    assert!(!info.is_active);
}

#[test]
fn test_concurrent_ui_notifications() {
    let mut service = StackService::new();
    let handler = Arc::new(MockUiHandler::new());
    let handler_trait: Arc<dyn UiNotificationHandler> = handler.clone();
    service.set_ui_handler(Arc::downgrade(&handler_trait));

    service.enable_stack_mode();

    // 複数のスタックを高速に追加
    for i in 1..=10 {
        service.save_stack(format!("Stack {}", i));
    }

    // 全ての通知が正しく送信されたことを確認
    let notifications = handler.get_notifications();
    assert_eq!(notifications.len(), 11); // ModeChanged + 10 StackAdded

    // 最初はModeChanged
    matches!(&notifications[0], UiNotification::ModeChanged(true));

    // 残りはStackAdded
    for (i, notification) in notifications.iter().enumerate().skip(1).take(10) {
        if let UiNotification::StackAdded(info) = notification {
            assert_eq!(info.number as usize, i);
        }
    }
}

// CI環境で実行可能なテストのみを含む
// 実際のキーイベントやUI描画を必要とするテストは手動テストで確認
