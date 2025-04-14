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
pub fn pause_apple_music() {
    let script = r#"
        tell application "Music"
            if player state is playing then
                pause
            end if
        end tell
    "#;

    let _ = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output();
}

pub fn resume_apple_music() {
    let script = r#"
        tell application "Music"
            play
        end tell
    "#;

    let _ = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output();
}
