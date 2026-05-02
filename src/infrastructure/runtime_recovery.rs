use std::time::{Duration, SystemTime};

/// wake 復旧の再試行方針
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WakeRecoveryRetryPolicy {
    pub max_attempts: usize,
    pub retry_interval: Duration,
}

impl WakeRecoveryRetryPolicy {
    /// macOS の wake 直後に音声デバイスが stream 作成可能になるまで待つ既定値を返す
    pub fn after_wake() -> Self {
        Self {
            max_attempts: 30,
            retry_interval: Duration::from_secs(2),
        }
    }
}

/// スリープ復帰を検知するための tick 遅延検出器
#[derive(Debug)]
pub struct SleepWakeDetector {
    last_tick_at: SystemTime,
    wake_threshold: Duration,
}

impl SleepWakeDetector {
    /// 新しい検出器を作成する
    pub fn new(last_tick_at: SystemTime, wake_threshold: Duration) -> Self {
        Self {
            last_tick_at,
            wake_threshold,
        }
    }

    /// 次回 tick を記録し、閾値超過なら wake と判定する
    pub fn record_tick(&mut self, now: SystemTime) -> bool {
        let elapsed = now
            .duration_since(self.last_tick_at)
            .unwrap_or(Duration::ZERO);
        self.last_tick_at = now;
        elapsed >= self.wake_threshold
    }
}

#[cfg(test)]
mod tests {
    use super::{SleepWakeDetector, WakeRecoveryRetryPolicy};
    use std::time::{Duration, SystemTime};

    /// 間隔内の tick では wake を検知しない
    #[test]
    fn detector_ignores_normal_tick_interval() {
        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let mut detector = SleepWakeDetector::new(start, Duration::from_secs(30));

        assert!(!detector.record_tick(start + Duration::from_secs(20)));
    }

    /// 大きく遅延した tick を wake とみなす
    #[test]
    fn detector_flags_large_tick_delay_as_wake() {
        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let mut detector = SleepWakeDetector::new(start, Duration::from_secs(30));

        assert!(detector.record_tick(start + Duration::from_secs(95)));
    }

    /// システム時刻が戻った場合は wake とみなさず基準時刻を更新する
    #[test]
    fn detector_ignores_backward_wall_clock_jump() {
        let start = SystemTime::UNIX_EPOCH + Duration::from_secs(100);
        let mut detector = SleepWakeDetector::new(start, Duration::from_secs(30));

        assert!(!detector.record_tick(start - Duration::from_secs(10)));
    }

    /// wake 復帰直後の音声デバイス再接続を待てる再試行幅を持つ
    #[test]
    fn wake_recovery_retry_policy_waits_for_audio_device_reconnection() {
        let policy = WakeRecoveryRetryPolicy::after_wake();

        assert_eq!(policy.max_attempts, 30);
        assert_eq!(policy.retry_interval, Duration::from_secs(2));
    }
}
