use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryMetrics {
    pub current_mb: f64,
    pub peak_mb: f64,
    pub threshold_mb: usize,
    pub usage_percent: f64,
}

impl MemoryMetrics {
    pub fn log_summary(&self) {
        println!(
            "[INFO] Memory usage: {:.1} MB / {} MB ({:.1}%), Peak: {:.1} MB",
            self.current_mb, self.threshold_mb, self.usage_percent, self.peak_mb
        );
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecordingMetrics {
    pub recording_duration: Duration,
    pub processing_duration: Duration,
    pub total_duration: Duration,
    pub audio_bytes: usize,
    pub memory_metrics: MemoryMetrics,
    pub mode: RecordingMode,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub enum RecordingMode {
    Memory,
    File,
}

impl RecordingMetrics {
    pub fn new(mode: RecordingMode) -> Self {
        Self {
            recording_duration: Duration::from_secs(0),
            processing_duration: Duration::from_secs(0),
            total_duration: Duration::from_secs(0),
            audio_bytes: 0,
            memory_metrics: MemoryMetrics {
                current_mb: 0.0,
                peak_mb: 0.0,
                threshold_mb: 0,
                usage_percent: 0.0,
            },
            mode,
        }
    }

    pub fn log_summary(&self) {
        println!("[INFO] Recording metrics ({:?} mode):", self.mode);
        println!(
            "[INFO]   Duration: {:.1}s recording, {:.1}s processing, {:.1}s total",
            self.recording_duration.as_secs_f64(),
            self.processing_duration.as_secs_f64(),
            self.total_duration.as_secs_f64()
        );
        println!(
            "[INFO]   Audio size: {:.1} MB",
            self.audio_bytes as f64 / 1024.0 / 1024.0
        );
        self.memory_metrics.log_summary();
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}

pub struct MetricsCollector {
    start_time: Instant,
    recording_start: Option<Instant>,
    processing_start: Option<Instant>,
    mode: RecordingMode,
}

impl MetricsCollector {
    pub fn new(mode: RecordingMode) -> Self {
        Self {
            start_time: Instant::now(),
            recording_start: None,
            processing_start: None,
            mode,
        }
    }

    pub fn start_recording(&mut self) {
        self.recording_start = Some(Instant::now());
    }

    pub fn start_processing(&mut self) {
        self.processing_start = Some(Instant::now());
    }

    pub fn finish(self, audio_bytes: usize, memory_metrics: MemoryMetrics) -> RecordingMetrics {
        let total_duration = self.start_time.elapsed();

        let recording_duration = self
            .recording_start
            .and_then(|start| self.processing_start.map(|end| end.duration_since(start)))
            .unwrap_or(Duration::from_secs(0));

        let processing_duration = self
            .processing_start
            .map(|start| start.elapsed())
            .unwrap_or(Duration::from_secs(0));

        RecordingMetrics {
            recording_duration,
            processing_duration,
            total_duration,
            audio_bytes,
            memory_metrics,
            mode: self.mode,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_metrics_collector() {
        let mut collector = MetricsCollector::new(RecordingMode::Memory);

        collector.start_recording();
        thread::sleep(Duration::from_millis(100));

        collector.start_processing();
        thread::sleep(Duration::from_millis(50));

        let memory_metrics = MemoryMetrics {
            current_mb: 10.0,
            peak_mb: 15.0,
            threshold_mb: 100,
            usage_percent: 10.0,
        };

        let metrics = collector.finish(1024 * 1024, memory_metrics);

        assert!(metrics.recording_duration.as_millis() >= 100);
        assert!(metrics.processing_duration.as_millis() >= 50);
        assert!(metrics.total_duration.as_millis() >= 150);
        assert_eq!(metrics.audio_bytes, 1024 * 1024);
        assert_eq!(metrics.mode, RecordingMode::Memory);
    }

    #[test]
    fn test_recording_metrics_json() {
        let metrics = RecordingMetrics {
            recording_duration: Duration::from_secs(5),
            processing_duration: Duration::from_secs(2),
            total_duration: Duration::from_secs(7),
            audio_bytes: 10 * 1024 * 1024,
            memory_metrics: MemoryMetrics {
                current_mb: 20.0,
                peak_mb: 25.0,
                threshold_mb: 100,
                usage_percent: 20.0,
            },
            mode: RecordingMode::Memory,
        };

        let json = metrics.to_json().unwrap();
        assert!(json.contains("\"mode\": \"Memory\""));
        assert!(json.contains("\"audio_bytes\": 10485760"));
        assert!(json.contains("\"current_mb\": 20.0"));
    }
}
