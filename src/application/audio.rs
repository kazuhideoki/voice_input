use thiserror::Error;

/// 音声データの返却形式
#[derive(Debug, Clone)]
pub struct AudioData {
    pub bytes: Vec<u8>,
    pub mime_type: &'static str,
    pub file_name: String,
}

#[derive(Debug, Error)]
pub enum AudioBackendError {
    #[error("audio backend state error: {message}")]
    State { message: String },
    #[error("audio data error: {message}")]
    AudioData { message: String },
    #[error("audio encode error: {message}")]
    Encode { message: String },
    #[error("audio stream operation failed: {message}")]
    StreamOperation { message: String },
    #[error("audio processing failed: {message}")]
    Processing { message: String },
}

/// 録音デバイス抽象。
/// 実装は `start_recording`→`stop_recording` が 1 対で呼ばれることを前提とする。
pub trait AudioBackend {
    /// 録音を開始。
    fn start_recording(&self) -> Result<(), AudioBackendError>;

    /// 録音を停止し、音声データを返す。
    fn stop_recording(&self) -> Result<AudioData, AudioBackendError>;

    /// 現在録音中であれば `true`。
    fn is_recording(&self) -> bool;

    /// スリープ復帰後に録音デバイスやストリームを回復する。
    fn recover_after_wake(&self) -> Result<(), AudioBackendError> {
        Ok(())
    }
}

/// `AudioBackend` の薄いラッパ。録音 port をアプリケーション層へ提供する。
pub struct Recorder<T: AudioBackend> {
    backend: T,
}

impl<T: AudioBackend> Recorder<T> {
    /// バックエンドを注入して新しい `Recorder` を作成。
    pub fn new(backend: T) -> Self {
        Self { backend }
    }

    /// 録音を開始します。
    pub fn start(&mut self) -> Result<(), AudioBackendError> {
        self.backend.start_recording()
    }

    /// 録音を停止し、音声データを返します。
    pub fn stop(&mut self) -> Result<AudioData, AudioBackendError> {
        self.backend.stop_recording()
    }

    /// 録音中かどうかを返します。
    pub fn is_recording(&self) -> bool {
        self.backend.is_recording()
    }

    /// スリープ復帰後にバックエンド回復を行います。
    pub fn recover_after_wake(&self) -> Result<(), AudioBackendError> {
        self.backend.recover_after_wake()
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
        fn new() -> Self {
            Self {
                recording: Arc::new(AtomicBool::new(false)),
                test_data: vec![1, 2, 3, 4, 5],
            }
        }
    }

    impl AudioBackend for MockAudioBackend {
        fn start_recording(&self) -> Result<(), AudioBackendError> {
            self.recording.store(true, Ordering::SeqCst);
            Ok(())
        }

        fn stop_recording(&self) -> Result<AudioData, AudioBackendError> {
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
        let backend = MockAudioBackend::new();
        let mut recorder = Recorder::new(backend);

        recorder.start().unwrap();

        let result = recorder.stop().unwrap();
        assert_eq!(result.bytes, vec![1, 2, 3, 4, 5]);
    }
}
