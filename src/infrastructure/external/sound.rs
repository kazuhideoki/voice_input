use std::process::Command;

/// Play the start recording sound
pub fn play_start_sound() {
    // Use afplay on macOS to play a system sound
    Command::new("afplay")
        .arg("/System/Library/Sounds/Ping.aiff")
        .spawn()
        .ok();
}

/// Play the stop recording sound
pub fn play_stop_sound() {
    // Use afplay on macOS to play a system sound
    Command::new("afplay")
        .arg("/System/Library/Sounds/Purr.aiff")
        .spawn()
        .ok();
}

/// Play the transcription complete sound
pub fn play_transcription_complete_sound() {
    // Use afplay on macOS to play a system sound
    Command::new("afplay")
        .arg("/System/Library/Sounds/Glass.aiff")
        .spawn()
        .ok();
}

// 例: Apple Music を一時停止させる
pub fn pause_apple_music() -> bool {
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

    // Apple Musicが起動していて、再生中か確認
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
