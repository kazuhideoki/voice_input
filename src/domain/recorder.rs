use crate::infrastructure::audio::AudioBackend;
use std::error::Error;

/// Thin wrapper around an `AudioBackend`
pub struct Recorder<T: AudioBackend> {
    backend: T,
}

impl<T: AudioBackend> Recorder<T> {
    pub fn new(backend: T) -> Self {
        Self { backend }
    }
    pub fn start(&self) -> Result<(), Box<dyn Error>> {
        self.backend.start_recording()
    }
    pub fn stop(&self) -> Result<String, Box<dyn Error>> {
        self.backend.stop_recording()
    }
    pub fn is_recording(&self) -> bool {
        self.backend.is_recording()
    }
}
