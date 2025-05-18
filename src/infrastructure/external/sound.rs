//! 効果音および Apple Music 制御ユーティリティ。
use std::process::Command;

/// 録音開始を示すサウンドを再生します。
pub fn play_start_sound() {
    let _ = Command::new("afplay")
        .arg("/System/Library/Sounds/Ping.aiff")
        .spawn();
}

/// 録音停止を示すサウンドを再生します。
pub fn play_stop_sound() {
    let _ = Command::new("afplay")
        .arg("/System/Library/Sounds/Purr.aiff")
        .spawn();
}

/// 転写完了を示すサウンドを再生します。
pub fn play_transcription_complete_sound() {
    let _ = Command::new("afplay")
        .arg("/System/Library/Sounds/Glass.aiff")
        .spawn();
}

/// Apple Music を一時停止し、元々再生中だったかを返します。
pub fn pause_apple_music() -> bool {
    // 直接 Music アプリを操作する - プロセスチェックをバイパス
    let playing_script = r#"
        try
            tell application "Music"
                set was_playing to (player state is playing)
                if was_playing then
                    pause
                end if
                return was_playing
            end tell
        on error
            return false
        end try
    "#;

    // エラーハンドリングを強化
    match Command::new("osascript")
        .arg("-e")
        .arg(playing_script)
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                if let Ok(result) = String::from_utf8(output.stdout) {
                    let trimmed = result.trim();
                    // デバッグ用に結果を出力
                    println!("Music pause result: '{}'", trimmed);
                    return trimmed == "true";
                }
            } else {
                // エラー出力がある場合は表示
                if let Ok(err) = String::from_utf8(output.stderr) {
                    if !err.trim().is_empty() {
                        eprintln!("Music pause error: {}", err.trim());
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to execute osascript: {}", e);
        }
    }
    false
}

/// Apple Music を再開します。
pub fn resume_apple_music() {
    // 直接 Music アプリを操作する - プロセスチェックをバイパス
    let play_script = r#"
        try
            tell application "Music"
                play
                return true
            end tell
        on error
            return false
        end try
    "#;

    // エラーハンドリングを強化
    match std::process::Command::new("osascript")
        .arg("-e")
        .arg(play_script)
        .output()
    {
        Ok(output) => {
            if output.status.success() {
                if let Ok(result) = String::from_utf8(output.stdout) {
                    println!("Music resume result: '{}'", result.trim());
                }
            } else {
                // エラー出力がある場合は表示
                if let Ok(err) = String::from_utf8(output.stderr) {
                    if !err.trim().is_empty() {
                        eprintln!("Music resume error: {}", err.trim());
                    }
                }
            }
        }
        Err(e) => {
            eprintln!("Failed to execute osascript: {}", e);
        }
    }
}

#[cfg(test)]
mod tests {

    /// macOS 専用のため CI ではスキップする想定。
    #[test]
    #[cfg(target_os = "macos")]
    fn sound_helpers_do_not_panic() {
        play_start_sound();
        play_stop_sound();
        play_transcription_complete_sound();
        let _ = pause_apple_music();
        resume_apple_music();
    }
}
