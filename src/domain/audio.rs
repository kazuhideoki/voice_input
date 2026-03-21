use std::error::Error;

/// 音声データの返却形式
#[derive(Debug, Clone)]
pub struct AudioData {
    pub bytes: Vec<u8>,
    pub mime_type: &'static str,
    pub file_name: String,
}

/// 録音デバイス抽象。
/// 実装は `start_recording`→`stop_recording` が 1 対で呼ばれることを前提とする。
pub trait AudioBackend {
    /// 録音を開始。
    fn start_recording(&self) -> Result<(), Box<dyn Error>>;

    /// 録音を停止し、音声データを返す。
    fn stop_recording(&self) -> Result<AudioData, Box<dyn Error>>;

    /// 現在録音中であれば `true`。
    fn is_recording(&self) -> bool;
}
