use std::error::Error;

/// 録音デバイス抽象。
/// 実装は `start_recording`→`stop_recording` が 1 対で呼ばれることを前提とする。
pub trait AudioBackend {
    /// 録音を開始。
    fn start_recording(&self) -> Result<(), Box<dyn Error>>;

    /// 録音を停止し、生成された WAV ファイルのパスを返します。
    fn stop_recording(&self) -> Result<String, Box<dyn Error>>;

    /// 現在録音中であれば `true`。
    fn is_recording(&self) -> bool;
}

pub mod cpal_backend;
pub use cpal_backend::CpalAudioBackend;
