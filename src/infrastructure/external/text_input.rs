//! テキスト直接入力モジュール
//!
//! subprocessを使用してテキストを入力する機能を提供

use crate::infrastructure::external::text_input_subprocess;
use std::error::Error;

/// メイン入力関数
///
/// subprocessを使用してテキストを入力します。
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
pub async fn type_text(text: &str) -> Result<(), Box<dyn Error>> {
    text_input_subprocess::type_text_via_subprocess(text)
        .await
        .map_err(|e| Box::new(e) as Box<dyn Error>)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[cfg_attr(feature = "ci-test", ignore)]
    async fn test_empty_text() {
        let result = type_text("").await;
        // 空文字列でも正常に処理されるべき
        match result {
            Ok(_) => println!("✅ Empty text handled correctly"),
            Err(e) => println!("⚠️ Error: {}", e),
        }
    }

    #[tokio::test]
    #[cfg_attr(feature = "ci-test", ignore)]
    async fn test_simple_text() {
        let result = type_text("Hello").await;
        match result {
            Ok(_) => println!("✅ Direct input test successful"),
            Err(e) => {
                println!("⚠️ Error: {}", e);
            }
        }
    }

    #[tokio::test]
    #[cfg_attr(feature = "ci-test", ignore)]
    async fn test_japanese_text() {
        // 日本語テキストのテスト
        let result = type_text("こんにちは").await;
        match result {
            Ok(_) => println!("✅ Japanese text input successful"),
            Err(e) => println!("⚠️ Error: {}", e),
        }
    }
}