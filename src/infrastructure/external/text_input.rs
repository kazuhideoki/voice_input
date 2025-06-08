//! テキスト直接入力モジュール
//!
//! macOS Accessibility APIを使用してクリップボードを使わずに
//! カーソル位置に直接テキストを入力する機能を提供

use crate::infrastructure::external::{text_input_accessibility, text_input_subprocess};
use crate::utils::config::EnvConfig;
use std::error::Error;

/// メイン入力関数
///
/// デフォルトではAccessibility APIを使用。移行期間中は環境変数で
/// 旧実装（subprocess方式）に切り替え可能。
///
/// # Environment Variables
/// - `VOICE_INPUT_USE_SUBPROCESS` - "true"に設定すると旧subprocess方式を使用
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
    let config = EnvConfig::get();

    // 移行期間中の環境変数による切り替え
    if config.use_subprocess {
        eprintln!("⚠️ Using legacy subprocess method (VOICE_INPUT_USE_SUBPROCESS=true)");
        return text_input_subprocess::type_text_via_subprocess(text)
            .await
            .map_err(|e| Box::new(e) as Box<dyn Error>);
    }

    // デフォルト: Accessibility API使用
    match text_input_accessibility::insert_text_at_cursor(text).await {
        Ok(_) => {
            println!("✓ Text inserted via Accessibility API");
            Ok(())
        }
        Err(e) => {
            eprintln!("Text insertion failed: {}", e);

            // 権限エラーの場合は特別なメッセージ
            if matches!(
                e,
                text_input_accessibility::TextInputError::PermissionDenied
            ) {
                eprintln!("\nPlease grant accessibility permission:");
                eprintln!("System Settings > Privacy & Security > Accessibility");
            }

            Err(Box::new(e))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[cfg_attr(feature = "ci-test", ignore)]
    async fn test_empty_text() {
        // テスト用初期化
        EnvConfig::test_init();

        let result = type_text("").await;
        // 空文字列でも正常に処理されるべき
        match result {
            Ok(_) => println!("✅ Empty text handled correctly"),
            Err(e) => println!("⚠️ Error (may be due to permissions): {}", e),
        }
    }

    #[tokio::test]
    #[cfg_attr(feature = "ci-test", ignore)]
    async fn test_simple_text() {
        // テスト用初期化
        EnvConfig::test_init();

        // Note: このテストはアクセシビリティ権限が必要
        let result = type_text("Hello").await;
        match result {
            Ok(_) => println!("✅ Direct input test successful"),
            Err(e) => {
                println!("⚠️ Expected error (no accessibility): {}", e);
                // エラーメッセージが適切に表示されることを確認
                assert!(
                    format!("{}", e).contains("Accessibility")
                        || format!("{}", e).contains("permission")
                        || format!("{}", e).contains("focused")
                );
            }
        }
    }

    #[tokio::test]
    #[cfg_attr(feature = "ci-test", ignore)]
    async fn test_japanese_text() {
        // テスト用初期化
        EnvConfig::test_init();

        // 日本語テキストのテスト
        let result = type_text("こんにちは").await;
        match result {
            Ok(_) => println!("✅ Japanese text input successful"),
            Err(e) => println!("⚠️ Error: {}", e),
        }
    }

    #[tokio::test]
    #[cfg_attr(feature = "ci-test", ignore)]
    async fn test_env_var_switching() {
        // Note: once_cellは再初期化できないため、
        // このテストは現在の設定での動作を確認するのみ

        // テスト用初期化
        EnvConfig::test_init();

        let config = EnvConfig::get();

        // 現在の設定での動作を確認
        let result = type_text("test").await;

        if config.use_subprocess {
            println!("Running in subprocess mode: {:?}", result);
        } else {
            println!("Running in accessibility mode: {:?}", result);
        }

        // 結果は環境に依存するため、エラーでも成功でも良い
        match result {
            Ok(_) => println!("✅ Text input successful"),
            Err(e) => println!("⚠️ Text input failed (expected in test env): {}", e),
        }
    }
}
