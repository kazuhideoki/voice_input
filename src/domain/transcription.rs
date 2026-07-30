use serde::{Deserialize, Serialize};

/// 辞書適用前の転写結果
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranscriptionOutput {
    /// 生の全文
    pub text: String,
}

impl TranscriptionOutput {
    /// 転写結果を生成
    pub fn from_text(text: impl Into<String>) -> Self {
        Self { text: text.into() }
    }
}

/// 最終入力する転写結果
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinalizedTranscription {
    /// 実際に入力する文字列
    pub text: String,
}
