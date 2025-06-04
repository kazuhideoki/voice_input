//! Cmdキーのリリース検出モジュール
//! 
//! Cmdキーがリリースされたことを確実に検出し、
//! ペースト処理の適切なタイミングを提供

use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;
use std::time::{Duration, Instant};

/// Cmdキーの状態とリリース通知を管理
pub struct CmdReleaseDetector {
    /// Cmdキーが押されているか
    cmd_pressed: Arc<AtomicBool>,
    /// リリース通知用
    release_notify: Arc<Notify>,
    /// 最後のCmdキー押下時刻
    last_cmd_press: Arc<Mutex<Option<Instant>>>,
}

impl CmdReleaseDetector {
    /// 新しいCmdReleaseDetectorを作成
    pub fn new() -> Self {
        Self {
            cmd_pressed: Arc::new(AtomicBool::new(false)),
            release_notify: Arc::new(Notify::new()),
            last_cmd_press: Arc::new(Mutex::new(None)),
        }
    }

    /// Cmdキーが押されたことを記録
    pub fn on_cmd_press(&self) {
        self.cmd_pressed.store(true, Ordering::SeqCst);
        if let Ok(mut last_press) = self.last_cmd_press.lock() {
            *last_press = Some(Instant::now());
        }
    }

    /// Cmdキーがリリースされたことを記録
    pub fn on_cmd_release(&self) {
        let was_pressed = self.cmd_pressed.swap(false, Ordering::SeqCst);
        if was_pressed {
            // リリースを通知
            self.release_notify.notify_waiters();
        }
    }

    /// Cmdキーが押されているか確認
    pub fn is_cmd_pressed(&self) -> bool {
        self.cmd_pressed.load(Ordering::SeqCst)
    }

    /// Cmdキーのリリースを待機
    /// 
    /// # Arguments
    /// * `timeout` - タイムアウト時間
    /// 
    /// # Returns
    /// * `Ok(())` - Cmdキーがリリースされた
    /// * `Err(())` - タイムアウト
    pub async fn wait_for_release(&self, timeout: Duration) -> Result<(), ()> {
        // 既にリリースされている場合は即座に返す
        if !self.is_cmd_pressed() {
            return Ok(());
        }

        // タイムアウト付きで待機
        tokio::select! {
            _ = self.release_notify.notified() => {
                // 追加の安定待機時間（キーイベントが完全に処理されるまで）
                tokio::time::sleep(Duration::from_millis(30)).await;
                Ok(())
            }
            _ = tokio::time::sleep(timeout) => {
                // タイムアウトした場合もCmdキーをリリース状態にリセット
                self.cmd_pressed.store(false, Ordering::SeqCst);
                Err(())
            }
        }
    }

    /// リリース通知をクリア（次の検出サイクルの準備）
    pub fn clear_notification(&self) {
        // Notifyは自動的にクリアされるため、特別な処理は不要
    }

    /// Cmdキーが最近押されたか確認
    /// 
    /// # Arguments
    /// * `within` - この時間内に押されたかを確認
    /// 
    /// # Returns
    /// * `true` - 指定時間内に押された
    /// * `false` - 押されていない、または指定時間より前
    pub fn was_pressed_recently(&self, within: Duration) -> bool {
        if let Ok(last_press) = self.last_cmd_press.lock() {
            if let Some(press_time) = *last_press {
                return press_time.elapsed() <= within;
            }
        }
        false
    }
}

impl Clone for CmdReleaseDetector {
    fn clone(&self) -> Self {
        Self {
            cmd_pressed: Arc::clone(&self.cmd_pressed),
            release_notify: Arc::clone(&self.release_notify),
            last_cmd_press: Arc::clone(&self.last_cmd_press),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_cmd_release_detection() {
        let detector = CmdReleaseDetector::new();
        
        // 初期状態
        assert!(!detector.is_cmd_pressed());
        
        // Cmd押下
        detector.on_cmd_press();
        assert!(detector.is_cmd_pressed());
        
        // 別タスクからリリース
        let detector_clone = detector.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(50)).await;
            detector_clone.on_cmd_release();
        });
        
        // リリース待機
        let result = detector.wait_for_release(Duration::from_millis(100)).await;
        assert!(result.is_ok());
        assert!(!detector.is_cmd_pressed());
    }

    #[tokio::test]
    async fn test_timeout() {
        let detector = CmdReleaseDetector::new();
        
        // Cmd押下
        detector.on_cmd_press();
        
        // タイムアウトテスト
        let result = detector.wait_for_release(Duration::from_millis(50)).await;
        assert!(result.is_err());
        // タイムアウト後はリリース状態になる
        assert!(!detector.is_cmd_pressed());
    }

    #[tokio::test]
    async fn test_already_released() {
        let detector = CmdReleaseDetector::new();
        
        // 既にリリースされている場合は即座に返る
        let start = Instant::now();
        let result = detector.wait_for_release(Duration::from_secs(1)).await;
        let elapsed = start.elapsed();
        
        assert!(result.is_ok());
        assert!(elapsed < Duration::from_millis(100));
    }

    #[test]
    fn test_recent_press() {
        let detector = CmdReleaseDetector::new();
        
        // 押下前
        assert!(!detector.was_pressed_recently(Duration::from_secs(1)));
        
        // 押下
        detector.on_cmd_press();
        assert!(detector.was_pressed_recently(Duration::from_millis(100)));
        
        std::thread::sleep(Duration::from_millis(150));
        assert!(!detector.was_pressed_recently(Duration::from_millis(100)));
    }
}