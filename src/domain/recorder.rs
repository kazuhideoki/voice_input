// src/domain/recorder.rs
use crate::infrastructure::audio::AudioBackend;
use std::error::Error;

/// Recorder domain service
pub struct Recorder<T: AudioBackend> {
    audio_backend: T,
}

impl<T: AudioBackend> Recorder<T> {
    /// Create a new recorder with the specified audio backend
    pub fn new(audio_backend: T) -> Self {
        Self { audio_backend }
    }

    /// Start recording audio
    pub fn start_recording(&self) -> Result<(), Box<dyn Error>> {
        self.audio_backend.start_recording()
    }

    /// Stop recording and save to the specified WAV file path
    pub fn stop_recording(&self, output_path: &str) -> Result<(), Box<dyn Error>> {
        self.audio_backend.stop_recording(output_path)
    }

    /// Check if recording is in progress
    pub fn is_recording(&self) -> bool {
        self.audio_backend.is_recording()
    }
}

