<<<<<<< HEAD
pub mod cpal_backend;
pub mod encoder;
pub use crate::domain::audio::{AudioBackend, AudioData};
pub use cpal_backend::CpalAudioBackend;
=======
use thiserror::Error;

pub mod cpal_backend;
pub mod encoder;
use self::cpal_backend::{AudioError, CpalBackendError};
use self::encoder::AudioEncodeError;
pub use cpal_backend::{AudioData, CpalAudioBackend};

#[derive(Debug, Error)]
pub enum AudioBackendError {
    #[error(transparent)]
    State(#[from] CpalBackendError),
    #[error(transparent)]
    AudioData(#[from] AudioError),
    #[error(transparent)]
    Encode(#[from] AudioEncodeError),
    #[error("audio stream operation failed: {message}")]
    StreamOperation { message: String },
    #[error("audio processing failed: {message}")]
    Processing { message: String },
}

/// 録音デバイス抽象。
/// 実装は `start_recording`→`stop_recording` が 1 対で呼ばれることを前提とする。
pub trait AudioBackend {
    /// 録音を開始。
    fn start_recording(&self) -> Result<(), AudioBackendError>;

    /// 録音を停止し、音声データを返します。
    /// AudioData にはバイト列と mime_type（既定: FLAC、失敗時にWAVへフォールバック）
    /// および file_name が含まれます。
    fn stop_recording(&self) -> Result<AudioData, AudioBackendError>;

    /// 現在録音中であれば `true`。
    fn is_recording(&self) -> bool;
}
>>>>>>> main
