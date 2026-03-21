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
}
