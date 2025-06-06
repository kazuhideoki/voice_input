//! サブプロセスを使用したテキスト入力モジュール
//!
//! rdevとの競合を避けるため、別プロセスでEnigo操作を実行
//!
//! ⚠️ DEPRECATED: このモジュールは移行期間後に削除予定です。
//! 新規コードではtext_input_accessibility.rsを使用してください。

use std::error::Error;
use std::fmt;
use tokio::process::Command;

/// サブプロセス実行エラー
#[derive(Debug)]
pub enum SubprocessInputError {
    /// プロセス起動エラー
    SpawnError(String),
    /// プロセス実行エラー
    ExecutionError(String),
    /// ヘルパーバイナリが見つからない
    HelperNotFound(String),
}

impl fmt::Display for SubprocessInputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SubprocessInputError::SpawnError(msg) => {
                write!(f, "Failed to spawn subprocess: {}", msg)
            }
            SubprocessInputError::ExecutionError(msg) => {
                write!(f, "Subprocess execution failed: {}", msg)
            }
            SubprocessInputError::HelperNotFound(msg) => {
                write!(f, "Helper binary not found: {}", msg)
            }
        }
    }
}

impl Error for SubprocessInputError {}

/// サブプロセスを使用してテキストを入力
///
/// # Arguments
/// * `text` - 入力するテキスト
///
/// # Returns
/// * `Ok(())` - 成功時
/// * `Err(SubprocessInputError)` - エラー時
pub async fn type_text_via_subprocess(text: &str) -> Result<(), SubprocessInputError> {
    // enigo_helperのパスを取得
    let helper_path = std::env::current_exe()
        .map_err(|e| SubprocessInputError::HelperNotFound(e.to_string()))?
        .parent()
        .ok_or_else(|| {
            SubprocessInputError::HelperNotFound("Parent directory not found".to_string())
        })?
        .join("enigo_helper");

    // サブプロセスを起動してテキストを入力
    let output = Command::new(&helper_path)
        .arg(text)
        .output()
        .await
        .map_err(|e| SubprocessInputError::SpawnError(format!("{}: {:?}", e, helper_path)))?;

    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        match output.status.code() {
            Some(1) => Err(SubprocessInputError::ExecutionError(
                "Invalid arguments".to_string(),
            )),
            Some(2) => Err(SubprocessInputError::ExecutionError(format!(
                "Text input failed: {}",
                stderr
            ))),
            Some(3) => Err(SubprocessInputError::ExecutionError(format!(
                "Enigo initialization failed: {}",
                stderr
            ))),
            _ => Err(SubprocessInputError::ExecutionError(format!(
                "Unknown error: {}",
                stderr
            ))),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    #[ignore] // 手動実行用
    async fn test_subprocess_input() {
        let test_texts = vec![
            "Hello, World!",
            "こんにちは、世界！",
            "Mixed: 英語 and 日本語",
        ];

        println!("Testing subprocess text input...");
        println!("Place cursor in a text field within 3 seconds");
        tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;

        for text in test_texts {
            println!("Inputting: {}", text);
            match type_text_via_subprocess(text).await {
                Ok(_) => println!("✓ Success"),
                Err(e) => println!("✗ Error: {}", e),
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    }
}
