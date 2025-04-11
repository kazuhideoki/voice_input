// src/text_selection.rs
use std::process::Command;

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
        .map_err(|e| format!("AppleScriptの実行に失敗: {}", e))?;

    if output.status.success() {
        let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(text)
    } else {
        let error = String::from_utf8_lossy(&output.stderr).trim().to_string();
        Err(format!("AppleScriptエラー: {}", error))
    }
}
