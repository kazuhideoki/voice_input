// src/main.rs
use std::{fs, process::Command, thread, time::Duration};

use arboard::Clipboard;
use clap::{Parser, Subcommand};
use ctrlc;
use tokio::{runtime::Runtime, sync::mpsc};

// Using the old module names for now
// This will be refactored in later phases
mod audio_recoder;
mod request_speech_to_text;
mod sound_player;
mod text_selection;
mod transcribe_audio;

/// Apple Music の再生状態を一時保存するマーカー
const MUSIC_MARKER_FILE: &str = "/tmp/voice_input_music_was_playing";

/// ===================================================
/// CLI
/// ===================================================
#[derive(Parser)]
#[command(author, version, about = "Voice Input Toggle & Transcribe")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// バックグラウンドで呼ばれる転写サブコマンド
    Transcribe {
        wav: String,
        #[arg(long)]
        prompt: Option<String>,
        /// 転写後すぐに貼り付ける
        #[arg(long, default_value_t = false)]
        paste: bool,
    },
    /// 録音 ↔ 停止トグル
    Record {
        /// 転写後すぐに貼り付ける
        #[arg(long, default_value_t = false)]
        paste: bool,
    },
}

// Make the main function public for Phase 1
pub fn main() {
    println!("Voice Input CLI - Phase 1 restructuring in progress");
    println!("This is a placeholder. Full functionality will be restored in Phase 2-5.");
}

// Placeholder functions to be implemented in future phases
fn run_transcription(_wav: &str, _prompt: Option<&str>, _paste: bool) -> Result<(), Box<dyn std::error::Error>> {
    println!("Transcription placeholder - will be implemented in Phase 3-4");
    Ok(())
}

fn run_record_cycle(_paste: bool) -> Result<(), Box<dyn std::error::Error>> {
    println!("Record cycle placeholder - will be implemented in Phase 2-3");
    Ok(())
}
