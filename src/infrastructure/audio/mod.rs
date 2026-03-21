pub mod cpal_backend;
pub mod encoder;
use self::cpal_backend::{AudioError, CpalBackendError};
use self::encoder::AudioEncodeError;
pub use crate::domain::audio::{AudioBackend, AudioBackendError, AudioData};
pub use cpal_backend::CpalAudioBackend;

impl From<CpalBackendError> for AudioBackendError {
    fn from(error: CpalBackendError) -> Self {
        Self::State {
            message: error.to_string(),
        }
    }
}

impl From<AudioError> for AudioBackendError {
    fn from(error: AudioError) -> Self {
        Self::AudioData {
            message: error.to_string(),
        }
    }
}

impl From<AudioEncodeError> for AudioBackendError {
    fn from(error: AudioEncodeError) -> Self {
        Self::Encode {
            message: error.to_string(),
        }
    }
}
