use crate::monitoring::metrics::MemoryMetrics;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

#[derive(Clone)]
pub struct MemoryMonitor {
    threshold_mb: usize,
    current_usage: Arc<AtomicUsize>,
    peak_usage: Arc<AtomicUsize>,
    alert_callback: Option<Arc<dyn Fn(usize) + Send + Sync>>,
}

impl std::fmt::Debug for MemoryMonitor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemoryMonitor")
            .field("threshold_mb", &self.threshold_mb)
            .field("current_usage", &self.current_usage.load(Ordering::SeqCst))
            .field("peak_usage", &self.peak_usage.load(Ordering::SeqCst))
            .field("alert_callback", &self.alert_callback.is_some())
            .finish()
    }
}

impl MemoryMonitor {
    pub fn new(threshold_mb: usize) -> Self {
        Self {
            threshold_mb,
            current_usage: Arc::new(AtomicUsize::new(0)),
            peak_usage: Arc::new(AtomicUsize::new(0)),
            alert_callback: None,
        }
    }

    pub fn with_alert_callback<F>(mut self, callback: F) -> Self
    where
        F: Fn(usize) + Send + Sync + 'static,
    {
        self.alert_callback = Some(Arc::new(callback));
        self
    }

    pub fn update_usage(&self, bytes: usize) {
        self.current_usage.store(bytes, Ordering::SeqCst);

        // ピーク使用量の更新
        let mut peak = self.peak_usage.load(Ordering::SeqCst);
        while bytes > peak {
            match self.peak_usage.compare_exchange_weak(
                peak,
                bytes,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(x) => peak = x,
            }
        }

        let mb = bytes / 1024 / 1024;
        #[cfg(debug_assertions)]
        println!("[DEBUG] Memory usage updated: {} MB", mb);

        if bytes > self.threshold_mb * 1024 * 1024 {
            self.trigger_alert(bytes);
        }
    }

    pub fn add_usage(&self, additional_bytes: usize) {
        let current = self
            .current_usage
            .fetch_add(additional_bytes, Ordering::SeqCst);
        let new_usage = current + additional_bytes;

        // ピーク使用量の更新
        let mut peak = self.peak_usage.load(Ordering::SeqCst);
        while new_usage > peak {
            match self.peak_usage.compare_exchange_weak(
                peak,
                new_usage,
                Ordering::SeqCst,
                Ordering::SeqCst,
            ) {
                Ok(_) => break,
                Err(x) => peak = x,
            }
        }

        let mb = new_usage / 1024 / 1024;
        #[cfg(debug_assertions)]
        println!(
            "[DEBUG] Memory usage increased by {} bytes, total: {} MB",
            additional_bytes, mb
        );

        if new_usage > self.threshold_mb * 1024 * 1024 {
            self.trigger_alert(new_usage);
        }
    }

    pub fn subtract_usage(&self, freed_bytes: usize) {
        let current = self.current_usage.load(Ordering::SeqCst);
        let new_usage = current.saturating_sub(freed_bytes);
        self.current_usage.store(new_usage, Ordering::SeqCst);

        let mb = new_usage / 1024 / 1024;
        #[cfg(debug_assertions)]
        println!(
            "[DEBUG] Memory usage decreased by {} bytes, total: {} MB",
            freed_bytes, mb
        );
    }

    pub fn reset(&self) {
        self.current_usage.store(0, Ordering::SeqCst);
        println!("[INFO] Memory monitor reset");
    }

    pub fn get_metrics(&self) -> MemoryMetrics {
        let current_bytes = self.current_usage.load(Ordering::SeqCst);
        let peak_bytes = self.peak_usage.load(Ordering::SeqCst);

        MemoryMetrics {
            current_mb: current_bytes as f64 / 1024.0 / 1024.0,
            peak_mb: peak_bytes as f64 / 1024.0 / 1024.0,
            threshold_mb: self.threshold_mb,
            usage_percent: self.calculate_usage_percent(),
        }
    }

    pub fn is_above_threshold(&self) -> bool {
        self.current_usage.load(Ordering::SeqCst) > self.threshold_mb * 1024 * 1024
    }

    fn calculate_usage_percent(&self) -> f64 {
        let current = self.current_usage.load(Ordering::SeqCst) as f64;
        let threshold = (self.threshold_mb * 1024 * 1024) as f64;
        (current / threshold) * 100.0
    }

    fn trigger_alert(&self, bytes: usize) {
        let mb = bytes / 1024 / 1024;
        let percent = self.calculate_usage_percent();

        eprintln!(
            "[WARN] Memory usage alert: {} MB ({:.1}% of {} MB threshold)",
            mb, percent, self.threshold_mb
        );

        if let Some(ref callback) = self.alert_callback {
            callback(bytes);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[test]
    fn test_memory_monitor_basic() {
        let monitor = MemoryMonitor::new(100); // 100MB threshold

        // 初期状態
        let metrics = monitor.get_metrics();
        assert_eq!(metrics.current_mb, 0.0);
        assert_eq!(metrics.peak_mb, 0.0);
        assert_eq!(metrics.usage_percent, 0.0);

        // 使用量を更新
        monitor.update_usage(50 * 1024 * 1024); // 50MB
        let metrics = monitor.get_metrics();
        assert_eq!(metrics.current_mb, 50.0);
        assert_eq!(metrics.peak_mb, 50.0);
        assert_eq!(metrics.usage_percent, 50.0);
        assert!(!monitor.is_above_threshold());
    }

    #[test]
    fn test_memory_monitor_threshold_alert() {
        let alert_triggered = Arc::new(Mutex::new(false));
        let alert_triggered_clone = alert_triggered.clone();

        let monitor = MemoryMonitor::new(100).with_alert_callback(move |_bytes| {
            *alert_triggered_clone.lock().unwrap() = true;
        });

        // 閾値以下
        monitor.update_usage(50 * 1024 * 1024);
        assert!(!*alert_triggered.lock().unwrap());

        // 閾値超過
        monitor.update_usage(150 * 1024 * 1024);
        assert!(*alert_triggered.lock().unwrap());
        assert!(monitor.is_above_threshold());
    }

    #[test]
    fn test_memory_monitor_add_subtract() {
        let monitor = MemoryMonitor::new(100);

        // 追加
        monitor.add_usage(30 * 1024 * 1024); // +30MB
        assert_eq!(monitor.get_metrics().current_mb, 30.0);

        monitor.add_usage(20 * 1024 * 1024); // +20MB
        assert_eq!(monitor.get_metrics().current_mb, 50.0);
        assert_eq!(monitor.get_metrics().peak_mb, 50.0);

        // 減算
        monitor.subtract_usage(10 * 1024 * 1024); // -10MB
        assert_eq!(monitor.get_metrics().current_mb, 40.0);
        assert_eq!(monitor.get_metrics().peak_mb, 50.0); // ピークは変わらない

        // リセット
        monitor.reset();
        assert_eq!(monitor.get_metrics().current_mb, 0.0);
    }

    #[test]
    fn test_memory_monitor_peak_tracking() {
        let monitor = MemoryMonitor::new(100);

        monitor.update_usage(50 * 1024 * 1024); // 50MB
        assert_eq!(monitor.get_metrics().peak_mb, 50.0);

        monitor.update_usage(80 * 1024 * 1024); // 80MB
        assert_eq!(monitor.get_metrics().peak_mb, 80.0);

        monitor.update_usage(30 * 1024 * 1024); // 30MB
        assert_eq!(monitor.get_metrics().current_mb, 30.0);
        assert_eq!(monitor.get_metrics().peak_mb, 80.0); // ピークは80MBのまま
    }
}
