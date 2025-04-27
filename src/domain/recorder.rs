use crate::infrastructure::audio::AudioBackend;
use std::error::Error;

/// `AudioBackend` の薄いラッパ。バックエンド選択を抽象化し、ドメイン層に録音 I/F を提供する。
pub struct Recorder<T: AudioBackend> {
    backend: T,
}

impl<T: AudioBackend> Recorder<T> {
    /// バックエンドを注入して新しい `Recorder` を作成。
    pub fn new(backend: T) -> Self {
        Self { backend }
    }

    /// 録音を開始します。
    pub fn start(&self) -> Result<(), Box<dyn Error>> {
        self.backend.start_recording()
    }

    /// 録音を停止し、保存された WAV ファイルのパスを返します。
    pub fn stop(&self) -> Result<String, Box<dyn Error>> {
        self.backend.stop_recording()
    }

    /// 録音中かどうかを返します。
    pub fn is_recording(&self) -> bool {
        self.backend.is_recording()
    }
}
