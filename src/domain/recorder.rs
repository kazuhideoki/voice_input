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
    pub fn start(&self) -> Result<(), Box<dyn Error>> {
        self.backend.start_recording()
    }

    /// 録音を停止し、保存された WAV ファイルのパスを返します。
    /// 後方互換性のため、メモリモードの場合は一時ファイルに書き出してパスを返します。
    pub fn stop(&self) -> Result<String, Box<dyn Error>> {
        match self.backend.stop_recording()? {
            AudioData::File(path) => Ok(path.to_string_lossy().into_owned()),
            AudioData::Memory(wav_data) => {
                // メモリモードの場合、一時ファイルに書き出す
                use std::io::Write;
                let timestamp = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
                let mut temp_path = std::env::temp_dir();
                temp_path.push(format!("voice_input_{}.wav", timestamp));
                
                let mut file = std::fs::File::create(&temp_path)?;
                file.write_all(&wav_data)?;
                
                Ok(temp_path.to_string_lossy().into_owned())
            }
        }
    }

    /// 録音を停止し、音声データを返します。
    /// 新しいAPIで、AudioData型を直接返します。
    pub fn stop_raw(&self) -> Result<AudioData, Box<dyn Error>> {
        self.backend.stop_recording()
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
        return_memory: bool,
        test_data: Vec<u8>,
    }

    impl MockAudioBackend {
        fn new(return_memory: bool) -> Self {
            Self {
                recording: Arc::new(AtomicBool::new(false)),
                return_memory,
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
            if self.return_memory {
                Ok(AudioData::Memory(self.test_data.clone()))
            } else {
                Ok(AudioData::File("/tmp/test.wav".into()))
            }
        }

        fn is_recording(&self) -> bool {
            self.recording.load(Ordering::SeqCst)
        }
    }

    #[test]
    fn test_recorder_with_file_backend() {
        let backend = MockAudioBackend::new(false);
        let recorder = Recorder::new(backend);
        
        // 録音開始
        assert!(recorder.start().is_ok());
        assert!(recorder.is_recording());
        
        // 録音停止（ファイルモード）
        let result = recorder.stop().unwrap();
        assert_eq!(result, "/tmp/test.wav");
        assert!(!recorder.is_recording());
    }

    #[test]
    fn test_recorder_with_memory_backend() {
        let backend = MockAudioBackend::new(true);
        let recorder = Recorder::new(backend);
        
        // 録音開始
        assert!(recorder.start().is_ok());
        assert!(recorder.is_recording());
        
        // 録音停止（メモリモード）
        let result = recorder.stop().unwrap();
        assert!(result.contains("voice_input_"));
        assert!(result.ends_with(".wav"));
        assert!(!recorder.is_recording());
        
        // ファイルが作成されたことを確認
        assert!(std::path::Path::new(&result).exists());
        
        // クリーンアップ
        let _ = std::fs::remove_file(&result);
    }

    #[test]
    fn test_recorder_stop_raw() {
        let backend = MockAudioBackend::new(true);
        let recorder = Recorder::new(backend);
        
        recorder.start().unwrap();
        
        // stop_rawは直接AudioDataを返す
        let result = recorder.stop_raw().unwrap();
        match result {
            AudioData::Memory(data) => {
                assert_eq!(data, vec![1, 2, 3, 4, 5]);
            }
            _ => panic!("Expected AudioData::Memory"),
        }
    }
}
