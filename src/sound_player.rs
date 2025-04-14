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