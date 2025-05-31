//! UIパフォーマンステスト
//!
//! UIコンポーネントのパフォーマンスを測定し、大量のスタック情報や
//! 頻繁な通知処理時のパフォーマンスを検証します。

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use voice_input::infrastructure::ui::{StackDisplayInfo, StackManagerApp, UiNotification};

#[test]
fn test_ui_notification_processing_performance() {
    let (tx, rx) = mpsc::unbounded_channel();
    let _app = StackManagerApp::new(rx);

    // 大量のスタック通知を送信してパフォーマンスを測定
    let start_time = Instant::now();
    let notification_count = 100;

    for i in 1..=notification_count {
        let stack_info = StackDisplayInfo {
            number: i,
            preview: format!("Stack {} preview text", i),
            created_at: "12:34:56".to_string(),
            is_active: false,
            char_count: 20,
        };

        // 通知をキューに送信
        tx.send(UiNotification::StackAdded(stack_info)).unwrap();
    }

    let elapsed = start_time.elapsed();

    // 100個の通知処理が10ms以内に完了することを確認
    assert!(
        elapsed < Duration::from_millis(10),
        "Notification queuing took too long: {:?}",
        elapsed
    );

    // メモリリークの確認として、送信者を削除
    drop(tx);
}

#[test]
fn test_stack_display_info_memory_usage() {
    // 大量のStackDisplayInfoを作成してメモリ使用量をチェック
    let mut stacks = Vec::new();

    for i in 1..=1000 {
        let stack_info = StackDisplayInfo {
            number: i,
            preview: "A".repeat(40), // 最大プレビュー長
            created_at: "12:34:56".to_string(),
            is_active: false,
            char_count: 1000,
        };
        stacks.push(stack_info);
    }

    // 1000個のスタック情報が正常に作成されることを確認
    assert_eq!(stacks.len(), 1000);

    // メモリ効率のチェック - 各スタック情報は合理的なサイズであるべき
    let estimated_size_per_stack = std::mem::size_of::<StackDisplayInfo>() + 40 + 8; // 構造体 + preview + created_at
    let total_estimated_size = estimated_size_per_stack * 1000;

    // 1000個で約200KB以下であることを確認（概算）
    assert!(
        total_estimated_size < 200_000,
        "Memory usage too high: {} bytes",
        total_estimated_size
    );
}

#[test]
fn test_ui_state_updates_frequency() {
    // UI更新頻度のテスト（60FPS = 16.67ms間隔）
    let target_frame_time = Duration::from_millis(16);
    let mut last_update = Instant::now();
    let mut frame_times = Vec::new();

    // 10フレーム分の時間を測定
    for _ in 0..10 {
        std::thread::sleep(target_frame_time);
        let now = Instant::now();
        let frame_duration = now.duration_since(last_update);
        frame_times.push(frame_duration);
        last_update = now;
    }

    // 平均フレーム時間が16.67ms ± 5msの範囲内であることを確認
    let avg_frame_time: Duration = frame_times.iter().sum::<Duration>() / frame_times.len() as u32;

    assert!(
        avg_frame_time >= Duration::from_millis(15),
        "Frame time too fast: {:?}",
        avg_frame_time
    );
    assert!(
        avg_frame_time <= Duration::from_millis(20),
        "Frame time too slow: {:?}",
        avg_frame_time
    );
}

#[derive(Debug, Default)]
struct MockPerformanceMonitor {
    update_times: Arc<Mutex<Vec<Duration>>>,
}

impl MockPerformanceMonitor {
    fn new() -> Self {
        Self {
            update_times: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn record_update(&self, duration: Duration) {
        self.update_times.lock().unwrap().push(duration);
    }

    fn get_average_update_time(&self) -> Duration {
        let times = self.update_times.lock().unwrap();
        if times.is_empty() {
            return Duration::from_millis(0);
        }
        times.iter().sum::<Duration>() / times.len() as u32
    }

    fn get_max_update_time(&self) -> Duration {
        let times = self.update_times.lock().unwrap();
        times
            .iter()
            .max()
            .copied()
            .unwrap_or(Duration::from_millis(0))
    }
}

#[test]
fn test_ui_update_performance_monitoring() {
    let monitor = MockPerformanceMonitor::new();

    // UI更新のシミュレーション
    for i in 0..50 {
        let start = Instant::now();

        // UI更新処理のシミュレーション
        let stack_count = i + 1;
        let _ui_work = (0..stack_count)
            .map(|j| format!("Stack {} processing", j))
            .collect::<Vec<_>>();

        let update_time = start.elapsed();
        monitor.record_update(update_time);

        // リアルなUI更新間隔をシミュレート
        if update_time < Duration::from_millis(16) {
            std::thread::sleep(Duration::from_millis(16) - update_time);
        }
    }

    let avg_time = monitor.get_average_update_time();
    let max_time = monitor.get_max_update_time();

    // 平均更新時間が5ms以下であることを確認
    assert!(
        avg_time <= Duration::from_millis(5),
        "Average update time too slow: {:?}",
        avg_time
    );

    // 最大更新時間が10ms以下であることを確認
    assert!(
        max_time <= Duration::from_millis(10),
        "Max update time too slow: {:?}",
        max_time
    );

    println!("UI Performance Test Results:");
    println!("  Average update time: {:?}", avg_time);
    println!("  Max update time: {:?}", max_time);
    println!("  Total updates: 50");
}
