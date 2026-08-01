//! ユーザーが変更できる設定のアプリケーション既定値。
//!
//! `config.json` で未指定の項目は、すべてこのファイルの値へ戻る。

/// 転写バックエンド種別。
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TranscriptionProvider {
    #[serde(rename = "gpt-transcribe")]
    GptTranscribe,
    #[serde(rename = "gpt-live-transcribe")]
    GptLiveTranscribe,
    #[serde(rename = "mlx-qwen3-asr")]
    MlxQwen3Asr,
}

pub const TRANSCRIPTION_PROVIDER: TranscriptionProvider = TranscriptionProvider::GptTranscribe;
pub const MAX_SECS: u64 = 30;
pub const PRE_ROLL_MS: u64 = 500;
pub const INPUT_DEVICE_PRIORITIES: &[&str] = &[];
pub const RECORDING_SOUNDS_ENABLED: bool = true;
pub const RECORDING_HUD_ENABLED: bool = true;
pub const PUSH_TO_TALK_ENABLED: bool = false;
pub const PUSH_TO_TALK_HOTKEY: &str = "opt+8";
pub const TRANSCRIBE_STREAMING: bool = false;
