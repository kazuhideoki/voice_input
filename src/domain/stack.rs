use serde::{Deserialize, Serialize};
use std::time::SystemTime;

/// 音声入力結果を保持するスタック
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stack {
    /// スタック番号（1-based）
    pub id: u32,
    /// 転写されたテキスト
    pub text: String,
    /// 作成日時
    pub created_at: SystemTime,
}

/// CLI表示用のスタック情報
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackInfo {
    /// スタック番号（1-based）
    pub number: u32,
    /// テキストのプレビュー（最大30文字）
    pub preview: String,
    /// 作成日時（フォーマット済み）
    pub created_at: String,
}

impl Stack {
    /// 新しいStackインスタンスを作成します。
    ///
    /// # Arguments
    ///
    /// * `id` - スタック番号（1-based）
    /// * `text` - 保存するテキスト
    pub fn new(id: u32, text: String) -> Self {
        Self {
            id,
            text,
            created_at: SystemTime::now(),
        }
    }

    /// StackをCLI表示用のStackInfoに変換します。
    ///
    /// テキストは最大30文字に切り詰められ、それ以上の場合は"..."が追加されます。
    pub fn to_info(&self) -> StackInfo {
        StackInfo {
            number: self.id,
            preview: self.text.chars().take(30).collect::<String>()
                + if self.text.len() > 30 { "..." } else { "" },
            created_at: format!("{:?}", self.created_at), // 簡易実装
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stack_creation() {
        let stack = Stack::new(1, "Hello, world!".to_string());
        assert_eq!(stack.id, 1);
        assert_eq!(stack.text, "Hello, world!");
    }

    #[test]
    fn test_stack_to_info_preview() {
        let stack = Stack::new(
            1,
            "This is a very long text that should be truncated".to_string(),
        );
        let info = stack.to_info();
        assert_eq!(info.preview, "This is a very long text that ...");
    }

    #[test]
    fn test_stack_to_info_short_text() {
        let stack = Stack::new(2, "Short text".to_string());
        let info = stack.to_info();
        assert_eq!(info.preview, "Short text");
        assert_eq!(info.number, 2);
    }

    #[test]
    fn test_stack_serialization() {
        let stack = Stack::new(1, "Test".to_string());
        let json = serde_json::to_string(&stack).unwrap();
        let deserialized: Stack = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.id, stack.id);
        assert_eq!(deserialized.text, stack.text);
    }

    #[test]
    fn test_stack_info_serialization() {
        let stack_info = StackInfo {
            number: 1,
            preview: "Test preview".to_string(),
            created_at: "2024-01-01 00:00:00".to_string(),
        };
        let json = serde_json::to_string(&stack_info).unwrap();
        let deserialized: StackInfo = serde_json::from_str(&json).unwrap();
        assert_eq!(deserialized.number, stack_info.number);
        assert_eq!(deserialized.preview, stack_info.preview);
        assert_eq!(deserialized.created_at, stack_info.created_at);
    }
}
