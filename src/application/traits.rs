//! Application層の抽象化トレイト定義
//! 外部依存を抽象化し、テスト可能な構造を提供します

use crate::error::Result;
use crate::infrastructure::audio::cpal_backend::AudioData;
use async_trait::async_trait;

/// 音声文字起こし機能の抽象化
#[async_trait]
pub trait TranscriptionClient: Send + Sync {
    /// 音声データを文字起こし
    async fn transcribe(&self, audio: AudioData, language: &str) -> Result<String>;
}

/// メディア制御の抽象化
#[async_trait]
pub trait MediaController: Send + Sync {
    /// Apple Musicが再生中かチェック
    async fn is_playing(&self) -> Result<bool>;

    /// Apple Musicを一時停止
    async fn pause(&self) -> Result<()>;

    /// Apple Musicを再生再開
    async fn resume(&self) -> Result<()>;
}
