use thiserror::Error;
use tokio::sync::mpsc;

/// 音声データの返却形式
#[derive(Debug, Clone)]
pub struct AudioData {
    pub bytes: Vec<u8>,
    pub mime_type: &'static str,
    pub file_name: String,
}

/// 録音中に流れる PCM フレーム。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioFrame {
    pub samples: Vec<i16>,
    pub sample_rate: u32,
    pub channels: u16,
}

/// 録音停止直後の raw PCM キャプチャ。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedAudio {
    pub samples: Vec<i16>,
    pub sample_rate: u32,
    pub channels: u16,
}

/// 録音停止時に要求する音声エンコード形式。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioDataFormat {
    Wav,
    Flac,
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
    #[error("{message}")]
    NoAudioCaptured { message: String },
}

/// 録音デバイス抽象。
/// 実装は `start_recording`→`stop_recording` が 1 対で呼ばれることを前提とする。
pub trait AudioBackend {
    /// 録音を開始。
    fn start_recording(&self) -> Result<(), AudioBackendError>;

    /// 録音中 PCM フレームの送信先を指定して録音を開始。
    fn start_recording_with_frame_tx(
        &self,
        _frame_tx: Option<mpsc::UnboundedSender<AudioFrame>>,
    ) -> Result<(), AudioBackendError> {
        self.start_recording()
    }

    /// 録音を停止し、音声データを返す。
    fn stop_recording(&self) -> Result<AudioData, AudioBackendError>;

    /// 指定した形式で録音を停止し、音声データを返す。
    fn stop_recording_as(&self, _format: AudioDataFormat) -> Result<AudioData, AudioBackendError> {
        self.stop_recording()
    }

    /// 録音を停止し、エンコード前の raw PCM を返す。
    fn stop_capture(&self) -> Result<CapturedAudio, AudioBackendError> {
        Err(AudioBackendError::AudioData {
            message: "raw capture is not supported by this audio backend".to_string(),
        })
    }

    /// raw PCM キャプチャを音声データへエンコードする。
    fn encode_capture(
        &self,
        _capture: CapturedAudio,
        _format: Option<AudioDataFormat>,
    ) -> Result<AudioData, AudioBackendError> {
        Err(AudioBackendError::AudioData {
            message: "capture encoding is not supported by this audio backend".to_string(),
        })
    }

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

    /// 録音中 PCM フレームの送信先を指定して録音を開始します。
    pub fn start_with_frame_tx(
        &mut self,
        frame_tx: Option<mpsc::UnboundedSender<AudioFrame>>,
    ) -> Result<(), AudioBackendError> {
        self.backend.start_recording_with_frame_tx(frame_tx)
    }

    /// 録音を停止し、音声データを返します。
    pub fn stop(&mut self) -> Result<AudioData, AudioBackendError> {
        self.backend.stop_recording()
    }

    /// 指定した形式で録音を停止し、音声データを返します。
    pub fn stop_as(&mut self, format: AudioDataFormat) -> Result<AudioData, AudioBackendError> {
        self.backend.stop_recording_as(format)
    }

    /// 録音を停止し、エンコード前の raw PCM を返します。
    pub fn stop_capture(&mut self) -> Result<CapturedAudio, AudioBackendError> {
        self.backend.stop_capture()
    }

    /// raw PCM キャプチャを音声データへエンコードします。
    pub fn encode_capture(
        &self,
        capture: CapturedAudio,
        format: Option<AudioDataFormat>,
    ) -> Result<AudioData, AudioBackendError> {
        self.backend.encode_capture(capture, format)
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
