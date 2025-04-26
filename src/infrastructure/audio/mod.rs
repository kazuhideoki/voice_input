// src/infrastructure/audio/mod.rs
pub mod cpal_backend;

/// Trait for audio recording backends
pub trait AudioBackend {
    /// Start recording audio
    fn start_recording(&self) -> Result<(), Box<dyn std::error::Error>>;
    
    /// Stop recording and save to the specified WAV file path
    fn stop_recording(&self, output_path: &str) -> Result<(), Box<dyn std::error::Error>>;
    
    /// Check if recording is in progress
    fn is_recording(&self) -> bool;
}

