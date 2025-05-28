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

    /// メモリモードかどうかを判定
    fn is_memory_mode(&self) -> bool {
        // 環境変数で判定する簡易実装
        std::env::var("LEGACY_TMP_WAV_FILE").unwrap_or_default() != "true"
    }

    /// 録音を開始します。
    pub fn start(&mut self) -> Result<(), Box<dyn Error>> {
        // メトリクス収集開始
        let mode = if self.is_memory_mode() {
            RecordingMode::Memory
        } else {
            RecordingMode::File
        };
        self.metrics_collector = Some(MetricsCollector::new(mode));

        if let Some(ref mut collector) = self.metrics_collector {
            collector.start_recording();
        }

        self.backend.start_recording()
    }

    /// 録音を停止し、保存された WAV ファイルのパスを返します。
    /// 注意: このメソッドは廃止予定です。代わりに stop_raw() を使用してください。
    /// メモリモードでは一時ファイルを作成しません。
    pub fn stop(&mut self) -> Result<String, Box<dyn Error>> {
        let _result = self.backend.stop_recording()?;
        // メモリモードでは一時ファイルを作成しない
        Err("Memory mode is not supported by stop(). Use stop_raw() instead.".into())
    }

    /// 録音を停止し、音声データを返します。
    /// 新しいAPIで、AudioData型を直接返します。
    pub fn stop_raw(&mut self) -> Result<AudioData, Box<dyn Error>> {
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
    fn test_recorder_stop_legacy() {
        let backend = MockAudioBackend::new(false);
        let mut recorder = Recorder::new(backend);

        // 録音開始
        assert!(recorder.start().is_ok());
        assert!(recorder.is_recording());

        // 録音停止（レガシーstop()メソッド）
        let result = recorder.stop();
        assert!(result.is_err()); // メモリモードではエラーが返される
        assert!(!recorder.is_recording());
    }

    #[test]
    fn test_recorder_stop_error() {
        let backend = MockAudioBackend::new(true);
        let mut recorder = Recorder::new(backend);

        // 録音開始
        assert!(recorder.start().is_ok());
        assert!(recorder.is_recording());

        // 録音停止（レガシーstop()メソッド）
        let result = recorder.stop();
        assert!(result.is_err()); // 常にエラーが返される
        assert!(!recorder.is_recording());
    }

    #[test]
    fn test_recorder_stop_raw() {
        let backend = MockAudioBackend::new(true);
        let mut recorder = Recorder::new(backend);

        recorder.start().unwrap();

        // stop_rawは直接AudioDataを返す
        let result = recorder.stop_raw().unwrap();
        assert_eq!(result.0, vec![1, 2, 3, 4, 5]);
    }
}
