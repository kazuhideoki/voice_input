//! Application層の抽象化トレイト定義
//! 外部依存を抽象化し、テスト可能な構造を提供します

use crate::error::Result;
use crate::infrastructure::audio::cpal_backend::AudioData;
use async_trait::async_trait;

/// 音声録音機能の抽象化
#[async_trait]
pub trait AudioRecorder: Send + Sync {
    /// 録音を開始
    async fn start(&mut self) -> Result<()>;

    /// 録音を停止し、音声データを返す
    async fn stop(&mut self) -> Result<AudioData>;

    /// 録音中かどうかを返す
    fn is_recording(&self) -> bool;
}

/// 音声文字起こし機能の抽象化
#[async_trait]
pub trait TranscriptionClient: Send + Sync {
    /// 音声データを文字起こし
    async fn transcribe(&self, audio: AudioData, language: &str) -> Result<String>;
}

/// テキスト入力機能の抽象化
#[async_trait]
pub trait TextInputClient: Send + Sync {
    /// テキストを直接入力
    async fn input_text(&self, text: &str) -> Result<()>;
}

/// クリップボード操作の抽象化
#[async_trait]
pub trait ClipboardClient: Send + Sync {
    /// 選択されたテキストを取得
    async fn get_selected_text(&self) -> Result<Option<String>>;

    /// クリップボードにテキストを設定
    async fn set_clipboard(&self, text: &str) -> Result<()>;
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

/// サウンド再生の抽象化
#[async_trait]
pub trait SoundPlayer: Send + Sync {
    /// 開始音を再生
    async fn play_start_sound(&self) -> Result<()>;

    /// 停止音を再生
    async fn play_stop_sound(&self) -> Result<()>;
}

/// 辞書機能の抽象化
#[async_trait]
pub trait DictionaryService: Send + Sync {
    /// テキストに辞書変換を適用
    async fn apply_replacements(&self, text: &str) -> Result<String>;
}
