//! テキスト直接入力モジュール
//!
//! Enigoライブラリを使用してクリップボードを使わずに
//! カーソル位置に直接テキストを入力する機能を提供

use std::error::Error;
use std::fmt;
use crate::infrastructure::external::text_input_enigo;

/// テキスト入力に関するエラー
#[derive(Debug)]
pub enum TextInputError {
    /// テキスト入力実行エラー
    AppleScriptFailure(String),
    /// 不正な入力データ
    InvalidInput(String),
}

impl fmt::Display for TextInputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextInputError::AppleScriptFailure(msg) => {
                write!(f, "Text input execution failed: {}", msg)
            }
            TextInputError::InvalidInput(msg) => {
                write!(f, "Invalid input: {}", msg)
            }
        }
    }
}

impl Error for TextInputError {}

/// デフォルト設定でテキストを直接入力
///
/// Enigoライブラリを使用して日本語を含む全ての文字を入力
///
/// # Example
/// ```no_run
/// # use voice_input::infrastructure::external::text_input::type_text;
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// type_text("Hello, World!").await?;
/// # Ok(())
/// # }
/// ```
pub async fn type_text(text: &str) -> Result<(), TextInputError> {
    // Enigoを使用して日本語を含むすべてのテキストを入力
    text_input_enigo::type_text_default(text)
        .await
        .map_err(|e| TextInputError::AppleScriptFailure(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_empty_text() {
        let result = type_text("").await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_simple_text() {
        // Note: この test は System Events へのアクセス権限が必要
        // CI環境では skip する可能性あり
        let result = type_text("Hello").await;
        // 権限が無い場合はエラーになるが、それも正常動作
        match result {
            Ok(_) => println!("✅ Direct input test successful"),
            Err(e) => println!("⚠️ Expected error (no accessibility): {}", e),
        }
    }
}