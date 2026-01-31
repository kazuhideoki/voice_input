use std::sync::OnceLock;
use std::time::{Duration, Instant};

const PROFILE_ENV: &str = "VOICE_INPUT_PROFILE";

#[cfg(test)]
use std::sync::atomic::{AtomicI8, AtomicUsize, Ordering};

#[cfg(test)]
static ENABLED_OVERRIDE: AtomicI8 = AtomicI8::new(-1);
#[cfg(test)]
static LOG_COUNT: AtomicUsize = AtomicUsize::new(0);

/// プロファイルログが有効かを返す。
pub fn enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    #[cfg(test)]
    {
        let override_value = ENABLED_OVERRIDE.load(Ordering::SeqCst);
        if override_value >= 0 {
            return override_value == 1;
        }
    }
    *ENABLED.get_or_init(|| {
        std::env::var(PROFILE_ENV)
            .ok()
            .map(|value| value.trim().to_ascii_lowercase())
            .map(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
            .unwrap_or(false)
    })
}

/// 計測開始用タイマー。
pub struct Timer {
    label: &'static str,
    start: Instant,
}

impl Timer {
    /// 計測を開始する。
    pub fn start(label: &'static str) -> Self {
        Self {
            label,
            start: Instant::now(),
        }
    }

    /// 経過時間をログに出力する。
    pub fn log(self) {
        log_duration(self.label, self.start.elapsed(), "");
    }

    /// 追加情報付きで経過時間をログに出力する。
    pub fn log_with(self, extra: &str) {
        log_duration(self.label, self.start.elapsed(), extra);
    }
}

/// 計測済みの経過時間をログに出力する。
pub fn log_duration(label: &str, elapsed: Duration, extra: &str) {
    if !enabled() {
        return;
    }

    #[cfg(test)]
    {
        LOG_COUNT.fetch_add(1, Ordering::SeqCst);
    }

    if extra.is_empty() {
        eprintln!("PROFILE label={} ms={}", label, elapsed.as_millis());
    } else {
        eprintln!(
            "PROFILE label={} ms={} {}",
            label,
            elapsed.as_millis(),
            extra
        );
    }
}

/// 任意タイミングのログを出力する。
pub fn log_point(label: &str, extra: &str) {
    log_duration(label, Duration::ZERO, extra);
}

#[cfg(test)]
pub fn set_enabled_override(value: bool) {
    ENABLED_OVERRIDE.store(if value { 1 } else { 0 }, Ordering::SeqCst);
}

#[cfg(test)]
pub fn clear_enabled_override() {
    ENABLED_OVERRIDE.store(-1, Ordering::SeqCst);
}

#[cfg(test)]
pub fn reset_log_count() {
    LOG_COUNT.store(0, Ordering::SeqCst);
}

#[cfg(test)]
pub fn log_count() -> usize {
    LOG_COUNT.load(Ordering::SeqCst)
}

#[cfg(test)]
mod tests {
    use super::*;
    use scopeguard::guard;

    /// 上書き設定でプロファイル有効/無効を切り替えられる
    #[test]
    fn profile_override_controls_enabled() {
        let _guard = guard((), |_| clear_enabled_override());

        set_enabled_override(true);
        assert!(enabled());

        set_enabled_override(false);
        assert!(!enabled());
    }
}
