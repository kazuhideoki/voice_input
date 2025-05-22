//! 効果音および Apple Music 制御ユーティリティ。
use std::process::Command;
use crate::infrastructure::audio::NativeSoundPlayer;
use crate::native::NativeMusicController;

/// 音声再生バックエンド
pub enum SoundBackend {
    Native(NativeSoundPlayer),
    Command, // 既存実装（フォールバック用）
}

/// 音声再生管理構造体
pub struct SoundPlayer {
    backend: SoundBackend,
}

impl SoundPlayer {
    /// 新しいSoundPlayerインスタンスを作成します
    pub fn new() -> Self {
        match NativeSoundPlayer::new() {
            Ok(native) => Self { backend: SoundBackend::Native(native) },
            Err(e) => {
                eprintln!("Failed to initialize native sound player, falling back to command: {}", e);
                Self { backend: SoundBackend::Command }
            },
        }
    }

    /// 録音開始を示すサウンドを再生します
    pub fn play_start_sound(&self) {
        match &self.backend {
            SoundBackend::Native(player) => {
                if let Err(e) = player.play_ping() {
                    eprintln!("Native ping sound failed, falling back to command: {}", e);
                    play_start_sound_command();
                }
            },
            SoundBackend::Command => play_start_sound_command(),
        }
    }

    /// 録音停止を示すサウンドを再生します
    pub fn play_stop_sound(&self) {
        match &self.backend {
            SoundBackend::Native(player) => {
                if let Err(e) = player.play_purr() {
                    eprintln!("Native purr sound failed, falling back to command: {}", e);
                    play_stop_sound_command();
                }
            },
            SoundBackend::Command => play_stop_sound_command(),
        }
    }

    /// 転写完了を示すサウンドを再生します
    pub fn play_transcription_complete_sound(&self) {
        match &self.backend {
            SoundBackend::Native(player) => {
                if let Err(e) = player.play_glass() {
                    eprintln!("Native glass sound failed, falling back to command: {}", e);
                    play_transcription_complete_sound_command();
                }
            },
            SoundBackend::Command => play_transcription_complete_sound_command(),
        }
    }
}

use std::sync::{Mutex, OnceLock};

/// グローバル音声プレイヤーインスタンス（安全な初期化）
static SOUND_PLAYER: OnceLock<Mutex<SoundPlayer>> = OnceLock::new();

/// グローバル音声プレイヤーを取得します
fn get_sound_player() -> &'static Mutex<SoundPlayer> {
    SOUND_PLAYER.get_or_init(|| {
        Mutex::new(SoundPlayer::new())
    })
}

/// 録音開始を示すサウンドを再生します（新しいAPI）
pub fn play_start_sound() {
    if let Ok(player) = get_sound_player().lock() {
        player.play_start_sound();
    } else {
        play_start_sound_command();
    }
}

/// 録音停止を示すサウンドを再生します（新しいAPI）
pub fn play_stop_sound() {
    if let Ok(player) = get_sound_player().lock() {
        player.play_stop_sound();
    } else {
        play_stop_sound_command();
    }
}

/// 転写完了を示すサウンドを再生します（新しいAPI）
pub fn play_transcription_complete_sound() {
    if let Ok(player) = get_sound_player().lock() {
        player.play_transcription_complete_sound();
    } else {
        play_transcription_complete_sound_command();
    }
}

/// 録音開始を示すサウンドを再生します（コマンド版・フォールバック用）
fn play_start_sound_command() {
    let _ = Command::new("afplay")
        .arg("/System/Library/Sounds/Ping.aiff")
        .spawn();
}

/// 録音停止を示すサウンドを再生します（コマンド版・フォールバック用）
fn play_stop_sound_command() {
    let _ = Command::new("afplay")
        .arg("/System/Library/Sounds/Purr.aiff")
        .spawn();
}

/// 転写完了を示すサウンドを再生します（コマンド版・フォールバック用）
fn play_transcription_complete_sound_command() {
    let _ = Command::new("afplay")
        .arg("/System/Library/Sounds/Glass.aiff")
        .spawn();
}

/// Apple Music を一時停止し、元々再生中だったかを返します（新しいAPI）
pub fn pause_apple_music() -> bool {
    // ネイティブ実装を試行
    match NativeMusicController::pause_apple_music() {
        Ok(was_playing) => {
            println!("Native music pause result: {}", was_playing);
            was_playing
        },
        Err(e) => {
            eprintln!("Native music pause failed, falling back to osascript: {}", e);
            pause_apple_music_command()
        }
    }
}

/// Apple Music を一時停止します（コマンド版・フォールバック用）
fn pause_apple_music_command() -> bool {
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

/// Apple Music を再開します（新しいAPI）
pub fn resume_apple_music() {
    // ネイティブ実装を試行
    match NativeMusicController::resume_apple_music() {
        Ok(success) => {
            println!("Native music resume result: {}", success);
        },
        Err(e) => {
            eprintln!("Native music resume failed, falling back to osascript: {}", e);
            resume_apple_music_command();
        }
    }
}

/// Apple Music を再開します（コマンド版・フォールバック用）
fn resume_apple_music_command() {
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
