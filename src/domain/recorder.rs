use crate::infrastructure::audio::{AudioBackend, AudioData};
use std::error::Error;

/// `AudioBackend` の薄いラッパ。バックエンド選択を抽象化し、ドメイン層に録音 I/F を提供する。
pub struct Recorder<T: AudioBackend> {
    backend: T,
}

impl<T: AudioBackend> Recorder<T> {
    /// バックエンドを注入して新しい `Recorder` を作成。
    pub fn new(backend: T) -> Self {
        Self { backend }
    }

    /// 録音を開始します。
    pub fn start(&mut self) -> Result<(), Box<dyn Error>> {
        self.backend.start_recording()
    }

    /// 録音を停止し、音声データを返します。
    pub fn stop(&mut self) -> Result<AudioData, Box<dyn Error>> {
        let result = self.backend.stop_recording()?;
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
            Ok(AudioData {
                bytes: self.test_data.clone(),
                mime_type: "audio/wav",
                file_name: "audio.wav".to_string(),
            })
        }

        fn is_recording(&self) -> bool {
            self.recording.load(Ordering::SeqCst)
        }
    }

    /// stopがAudioDataを返す
    #[test]
    fn stop_returns_audio_data() {
        let backend = MockAudioBackend::new(true);
        let mut recorder = Recorder::new(backend);

        recorder.start().unwrap();

        // stopは直接AudioDataを返す
        let result = recorder.stop().unwrap();
        assert_eq!(result.bytes, vec![1, 2, 3, 4, 5]);
    }
}
