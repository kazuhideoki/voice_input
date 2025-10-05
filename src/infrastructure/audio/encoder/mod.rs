use thiserror::Error;

pub mod flac;

/// 対応する音声フォーマット
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AudioFormat {
    Wav,
    Flac,
}

#[derive(Debug, Error)]
pub enum AudioEncodeError {
    #[error("FLAC encode failed: {0}")]
    Flac(String),
}
