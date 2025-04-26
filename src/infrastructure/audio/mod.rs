use std::error::Error;

pub trait AudioBackend {
    fn start_recording(&self) -> Result<(), Box<dyn Error>>;
    /// 録音を停止し、保存した WAV ファイルのパスを返す
    fn stop_recording(&self) -> Result<String, Box<dyn Error>>;
    fn is_recording(&self) -> bool;
}

pub mod cpal_backend;
pub use cpal_backend::CpalAudioBackend;
