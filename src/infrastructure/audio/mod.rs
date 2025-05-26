use std::error::Error;

pub mod cpal_backend;
pub use cpal_backend::{AudioData, CpalAudioBackend};

/// 録音デバイス抽象。
/// 実装は `start_recording`→`stop_recording` が 1 対で呼ばれることを前提とする。
pub trait AudioBackend {
    /// 録音を開始。
    fn start_recording(&self) -> Result<(), Box<dyn Error>>;

    /// 録音を停止し、音声データを返します。
    /// メモリモードの場合はWAVフォーマットのバイトデータ、
    /// レガシーモードの場合はWAVファイルのパスを返します。
    fn stop_recording(&self) -> Result<AudioData, Box<dyn Error>>;

    /// 現在録音中であれば `true`。
    fn is_recording(&self) -> bool;
}
