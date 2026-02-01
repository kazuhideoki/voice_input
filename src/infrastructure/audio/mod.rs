use std::error::Error;

pub mod cpal_backend;
pub mod encoder;
pub use cpal_backend::{AudioData, CpalAudioBackend};

/// 録音デバイス抽象。
/// 実装は `start_recording`→`stop_recording` が 1 対で呼ばれることを前提とする。
pub trait AudioBackend {
    /// 録音を開始。
    fn start_recording(&self) -> Result<(), Box<dyn Error>>;

    /// 録音を停止し、音声データを返します。
    /// AudioData にはバイト列と mime_type（既定: FLAC、失敗時にWAVへフォールバック）
    /// および file_name が含まれます。
    fn stop_recording(&self) -> Result<AudioData, Box<dyn Error>>;

    /// 現在録音中であれば `true`。
    fn is_recording(&self) -> bool;
}
