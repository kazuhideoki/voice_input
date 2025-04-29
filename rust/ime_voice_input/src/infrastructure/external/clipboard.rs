//! 選択テキスト取得ユーティリティ。
//! AppleScript 経由でフロントアプリに Cmd+C を送り、pbpaste で取得します。
use std::process::Command;

/// 現在フォーカスされているアプリケーションの選択テキストを取得します。
///
/// 戻り値:
/// - Ok(text)  … 成功
/// - Err(msg) … AppleScript 実行失敗または pbpaste 失敗
///
/// TODO: macOSのシステムAPIを試す。Cのバインディングとかでできる？
pub fn get_selected_text() -> Result<String, String> {
    let script = r#"
        tell application "System Events"
            set frontApp to name of first application process whose frontmost is true

            -- 現在の選択テキストをクリップボードにコピー
            keystroke "c" using {command down}
            delay 0.1

            -- クリップボードから取得
            do shell script "pbpaste"
        end tell
    "#;

    let output = Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output()
        .map_err(|e| format!("Failed to execute AppleScript: {}", e))?;

    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        Err(format!(
            "AppleScript error: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ))
    }
}
