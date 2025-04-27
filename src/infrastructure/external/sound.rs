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
    let check_script = r#"
        tell application \"System Events\" to (exists (processes where name is \"Music\"))
    "#;

    let check_output = Command::new("osascript")
        .arg("-e")
        .arg(check_script)
        .output();

    if let Ok(output) = check_output {
        if let Ok(result) = String::from_utf8(output.stdout) {
            if result.trim() == "true" {
                let playing_script = r#"
                    tell application "Music"
                        set was_playing to (player state is playing)
                        if was_playing then
                            pause
                        end if
                        return was_playing
                    end tell
                "#;

                if let Ok(output) = std::process::Command::new("osascript")
                    .arg("-e")
                    .arg(playing_script)
                    .output()
                {
                    if let Ok(result) = String::from_utf8(output.stdout) {
                        return result.trim() == "true";
                    }
                }
            }
        }
    }
    false
}

/// Apple Music を再開します (起動している場合のみ)。
pub fn resume_apple_music() {
    let check_script = r#"
        tell application "System Events"
            set isRunning to (exists (processes where name is "Music"))
        end tell
    "#;

    // まずApple Musicが起動しているか確認
    let check_output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(check_script)
        .output();

    // コマンド実行に成功し、出力が "true" ならば再生を試みる
    if let Ok(output) = check_output {
        if let Ok(result) = String::from_utf8(output.stdout) {
            if result.trim() == "true" {
                let play_script = r#"
                    tell application "Music"
                        play
                    end tell
                "#;

                let _ = std::process::Command::new("osascript")
                    .arg("-e")
                    .arg(play_script)
                    .output();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
