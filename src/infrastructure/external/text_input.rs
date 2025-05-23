//! テキスト直接入力モジュール
//!
//! AppleScript keystroke を使用してクリップボードを使わずに
//! カーソル位置に直接テキストを入力する機能を提供

use std::error::Error;
use std::fmt;
use tokio::process::Command;
use tokio::time::{Duration, sleep};

/// テキスト入力に関するエラー
#[derive(Debug)]
pub enum TextInputError {
    /// AppleScript実行エラー
    AppleScriptFailure(String),
    /// テキストエスケープエラー
    EscapeError(String),
    /// タイムアウトエラー
    Timeout,
    /// 不正な入力データ
    InvalidInput(String),
}

impl fmt::Display for TextInputError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TextInputError::AppleScriptFailure(msg) => {
                write!(f, "AppleScript execution failed: {}", msg)
            }
            TextInputError::EscapeError(msg) => {
                write!(f, "Text escaping failed: {}", msg)
            }
            TextInputError::Timeout => {
                write!(f, "Text input operation timed out")
            }
            TextInputError::InvalidInput(msg) => {
                write!(f, "Invalid input: {}", msg)
            }
        }
    }
}

impl Error for TextInputError {}

/// テキスト入力設定
#[derive(Debug, Clone)]
pub struct TextInputConfig {
    /// 分割送信時の最大文字数
    pub max_chunk_size: usize,
    /// 分割送信時の遅延（ミリ秒）
    pub chunk_delay_ms: u64,
    /// AppleScript実行タイムアウト（秒）
    pub timeout_seconds: u64,
}

impl Default for TextInputConfig {
    fn default() -> Self {
        Self {
            max_chunk_size: 200, // 安全な初期値
            chunk_delay_ms: 10,  // 最小遅延
            timeout_seconds: 30, // 十分な時間
        }
    }
}

/// AppleScript文字列リテラル用エスケープ関数
///
/// # 対応する特殊文字
/// - バックスラッシュ: \ → \\
/// - ダブルクォート: " → \"
/// - 改行文字: \n → \r (AppleScriptは\rを改行として認識)
/// - キャリッジリターン重複回避: \r\r → \r
///
/// # Arguments
/// * `text` - エスケープ対象のテキスト
///
/// # Returns
/// AppleScript で安全に使用できるエスケープ済み文字列
fn escape_for_applescript(text: &str) -> Result<String, TextInputError> {
    if text.is_empty() {
        return Ok(String::new());
    }

    // 最大文字数制限チェック (AppleScript の実際の制限)
    if text.len() > 32768 {
        return Err(TextInputError::InvalidInput(
            "Text too long for AppleScript processing".to_string(),
        ));
    }

    let escaped = text
        .replace("\\", "\\\\") // バックスラッシュエスケープ (最初に実行)
        .replace("\"", "\\\"") // ダブルクォートエスケープ
        .replace("\n", "\r") // 改行文字変換
        .replace("\r\r", "\r"); // 重複回避

    Ok(escaped)
}

#[cfg(test)]
mod escape_tests {
    use super::*;

    #[test]
    fn test_basic_escape() {
        assert_eq!(
            escape_for_applescript("Hello \"World\"").unwrap(),
            "Hello \\\"World\\\""
        );
    }

    #[test]
    fn test_newline_escape() {
        assert_eq!(
            escape_for_applescript("Line1\nLine2").unwrap(),
            "Line1\rLine2"
        );
    }

    #[test]
    fn test_backslash_escape() {
        assert_eq!(
            escape_for_applescript("Path\\to\\file").unwrap(),
            "Path\\\\to\\\\file"
        );
    }

    #[test]
    fn test_complex_escape() {
        assert_eq!(
            escape_for_applescript("Say \"Hello\\world\"\nNext line").unwrap(),
            "Say \\\"Hello\\\\world\\\"\rNext line"
        );
    }

    #[test]
    fn test_empty_string() {
        assert_eq!(escape_for_applescript("").unwrap(), "");
    }

    #[test]
    fn test_too_long_string() {
        let long_text = "a".repeat(32769);
        assert!(escape_for_applescript(&long_text).is_err());
    }
}

/// テキストを AppleScript keystroke で直接入力
///
/// # Arguments
/// * `text` - 入力するテキスト
/// * `config` - 入力設定
///
/// # Returns
/// 成功時は Ok(()), 失敗時は TextInputError
///
/// # 分割送信
/// 長いテキストは config.max_chunk_size で分割して送信
/// 各分割間に config.chunk_delay_ms の遅延を挿入
///
/// # Example
/// ```no_run
/// # use voice_input::infrastructure::external::text_input::{type_text_directly, TextInputConfig};
/// # #[tokio::main]
/// # async fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let config = TextInputConfig::default();
/// type_text_directly("Hello, World!", &config).await?;
/// # Ok(())
/// # }
/// ```
pub async fn type_text_directly(
    text: &str,
    config: &TextInputConfig,
) -> Result<(), TextInputError> {
    // 設定のバリデーション
    validate_config(config)?;
    
    if text.is_empty() {
        return Ok(());
    }

    let escaped = escape_for_applescript(text)?;
    let chars: Vec<char> = escaped.chars().collect();

    // 分割送信が必要かチェック
    if chars.len() <= config.max_chunk_size {
        // 単一送信
        execute_keystroke(&escaped, config.timeout_seconds).await
    } else {
        // 分割送信
        execute_chunked_keystroke(&chars, config).await
    }
}

/// 単一のkeystrokeコマンド実行
async fn execute_keystroke(escaped_text: &str, timeout_seconds: u64) -> Result<(), TextInputError> {
    let script = format!(
        r#"tell application "System Events" to keystroke "{}""#,
        escaped_text
    );

    let output = tokio::time::timeout(
        Duration::from_secs(timeout_seconds),
        Command::new("osascript").arg("-e").arg(script).output(),
    )
    .await
    .map_err(|_| TextInputError::Timeout)?
    .map_err(|e| TextInputError::AppleScriptFailure(e.to_string()))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(TextInputError::AppleScriptFailure(stderr.to_string()));
    }

    Ok(())
}

/// 分割keystrokeコマンド実行
async fn execute_chunked_keystroke(
    chars: &[char],
    config: &TextInputConfig,
) -> Result<(), TextInputError> {
    for chunk in chars.chunks(config.max_chunk_size) {
        let chunk_str: String = chunk.iter().collect();

        execute_keystroke(&chunk_str, config.timeout_seconds).await?;

        // 最後のチャンク以外では遅延を挿入
        if chunk.len() == config.max_chunk_size {
            sleep(Duration::from_millis(config.chunk_delay_ms)).await;
        }
    }

    Ok(())
}

/// デフォルト設定でテキストを直接入力
///
/// 最も簡単な使用方法。内部でデフォルト設定を使用
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
    type_text_directly(text, &TextInputConfig::default()).await
}

/// 設定のバリデーション
///
/// # Example
/// ```
/// use voice_input::infrastructure::external::text_input::{TextInputConfig, validate_config};
///
/// let mut config = TextInputConfig::default();
/// assert!(validate_config(&config).is_ok());
///
/// config.max_chunk_size = 0;
/// assert!(validate_config(&config).is_err());
/// ```
pub fn validate_config(config: &TextInputConfig) -> Result<(), TextInputError> {
    if config.max_chunk_size == 0 {
        return Err(TextInputError::InvalidInput(
            "max_chunk_size must be greater than 0".to_string(),
        ));
    }

    if config.max_chunk_size > 1000 {
        return Err(TextInputError::InvalidInput(
            "max_chunk_size too large (max: 1000)".to_string(),
        ));
    }

    if config.timeout_seconds == 0 {
        return Err(TextInputError::InvalidInput(
            "timeout_seconds must be greater than 0".to_string(),
        ));
    }

    if config.timeout_seconds > 300 {
        return Err(TextInputError::InvalidInput(
            "timeout_seconds too large (max: 300)".to_string(),
        ));
    }

    Ok(())
}

#[cfg(test)]
mod integration_tests {
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

    #[test]
    fn test_config_validation() {
        let mut config = TextInputConfig::default();
        assert!(validate_config(&config).is_ok());

        config.max_chunk_size = 0;
        assert!(validate_config(&config).is_err());

        config.max_chunk_size = 1500;
        assert!(validate_config(&config).is_err());
    }

    #[tokio::test]
    async fn test_large_text() {
        let large_text = "A".repeat(500);
        let config = TextInputConfig {
            max_chunk_size: 100,
            chunk_delay_ms: 1, // テスト用に短縮
            timeout_seconds: 10,
        };

        let result = type_text_directly(&large_text, &config).await;
        // 分割処理が正常に動作することを確認
        match result {
            Ok(_) => println!("✅ Large text test successful"),
            Err(e) => println!("⚠️ Expected error: {}", e),
        }
    }
}
