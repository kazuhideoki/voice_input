use crate::infrastructure::audio::{AudioBackend, AudioData};
use crate::monitoring::{
    MemoryMonitor,
    metrics::{MetricsCollector, RecordingMode},
};
use std::error::Error;
use std::sync::Arc;

/// `AudioBackend` の薄いラッパ。バックエンド選択を抽象化し、ドメイン層に録音 I/F を提供する。
pub struct Recorder<T: AudioBackend> {
    backend: T,
    memory_monitor: Option<Arc<MemoryMonitor>>,
    metrics_collector: Option<MetricsCollector>,
}

impl<T: AudioBackend> Recorder<T> {
    /// バックエンドを注入して新しい `Recorder` を作成。
    pub fn new(backend: T) -> Self {
        Self {
            backend,
            memory_monitor: None,
            metrics_collector: None,
        }
    }

    /// メモリモニターを設定する
    pub fn with_memory_monitor(mut self, monitor: Arc<MemoryMonitor>) -> Self {
        self.memory_monitor = Some(monitor);
        self
    }

    /// 録音を開始します。
    pub fn start(&mut self) -> Result<(), Box<dyn Error>> {
        // メトリクス収集開始
        self.metrics_collector = Some(MetricsCollector::new(RecordingMode::Memory));

        if let Some(ref mut collector) = self.metrics_collector {
            collector.start_recording();
        }

        self.backend.start_recording()
    }

    /// 録音を停止し、音声データを返します。
    pub fn stop(&mut self) -> Result<AudioData, Box<dyn Error>> {
        if let Some(ref mut collector) = self.metrics_collector {
            collector.start_processing();
        }

        let result = self.backend.stop_recording()?;

        // メモリ使用量の更新
        if let Some(ref monitor) = self.memory_monitor {
            monitor.update_usage(result.0.len());
        }

        // メトリクスの完了
        if let (Some(collector), Some(monitor)) =
            (self.metrics_collector.take(), &self.memory_monitor)
        {
            let audio_bytes = result.0.len();

            let metrics = collector.finish(audio_bytes, monitor.get_metrics());
            metrics.log_summary();
        }

        Ok(result)
    }

    /// 録音中かどうかを返します。
    pub fn is_recording(&self) -> bool {
        self.backend.is_recording()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    /// テスト用のモックAudioBackend
    struct MockAudioBackend {
        recording: Arc<AtomicBool>,
        test_data: Vec<u8>,
    }

    impl MockAudioBackend {
        fn new(_return_memory: bool) -> Self {
            Self {
                recording: Arc::new(AtomicBool::new(false)),
                test_data: vec![1, 2, 3, 4, 5], // テスト用のダミーデータ
            }
        }
    }

    impl AudioBackend for MockAudioBackend {
        fn start_recording(&self) -> Result<(), Box<dyn Error>> {
            self.recording.store(true, Ordering::SeqCst);
            Ok(())
        }

        fn stop_recording(&self) -> Result<AudioData, Box<dyn Error>> {
            self.recording.store(false, Ordering::SeqCst);
            Ok(AudioData(self.test_data.clone()))
        }

        fn is_recording(&self) -> bool {
            self.recording.load(Ordering::SeqCst)
        }
    }

    #[test]
    fn test_recorder_stop() {
        let backend = MockAudioBackend::new(true);
        let mut recorder = Recorder::new(backend);

        recorder.start().unwrap();

        // stopは直接AudioDataを返す
        let result = recorder.stop().unwrap();
        assert_eq!(result.0, vec![1, 2, 3, 4, 5]);
    }
}
