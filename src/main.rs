use std::fs;
use std::process::Command;

use arboard::Clipboard;
use clap::{Parser, Subcommand};
use ctrlc;
use tokio::{runtime::Runtime, sync::mpsc};

mod audio_recoder;
mod request_speech_to_text;
mod sound_player;
mod text_selection;
mod transcribe_audio;

use voice_input::spawn_detached;

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
    },
    /// 録音 + 停止トグル
    Record,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();
    let cli = Cli::parse();

    match cli.cmd.unwrap_or(Cmd::Record) {
        Cmd::Transcribe { wav, prompt } => run_transcription(&wav, prompt.as_deref()),
        Cmd::Record => run_record_cycle(),
    }
}

/// ---------------------------------------------------
/// 転写処理（バックグラウンド実行）
fn run_transcription(wav: &str, prompt: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    println!("Transcribing {wav} …");

    let rt = Runtime::new()?;
    let txt = rt.block_on(transcribe_audio::transcribe_audio(wav, prompt))?;

    // クリップボードに貼り付け
    let mut clipboard = Clipboard::new()?;
    clipboard.set_text(&txt)?;

    sound_player::play_transcription_complete_sound();
    println!("Transcription done:\n{txt}");

    // --- Apple Music を再開 ---
    if std::path::Path::new(MUSIC_MARKER_FILE).exists() {
        sound_player::resume_apple_music();
        let _ = fs::remove_file(MUSIC_MARKER_FILE);
    }

    Ok(())
}

/// ---------------------------------------------------
/// 録音トグル処理
fn run_record_cycle() -> Result<(), Box<dyn std::error::Error>> {
    // ---------------- 録音開始 ----------------
    let rt = Runtime::new()?;
    let (notify_tx, _notify_rx) = mpsc::channel::<()>(1);

    // 1 Apple Music が再生中なら一時停止し、マーカーを作成
    if sound_player::pause_apple_music() {
        let _ = fs::write(MUSIC_MARKER_FILE, "");
    }
    sound_player::play_start_sound();
    // 2 テキスト選択を取得 & 録音開始
    let start_selected_text = rt.block_on(request_speech_to_text::start_recording(notify_tx))?;

    println!("Recording… もう一度 ⌥+8 で停止 (Raycast が SIGINT を送ります)");

    // ---------------- 停止待ち ----------------
    let (sig_tx, sig_rx) = std::sync::mpsc::channel::<()>();
    ctrlc::set_handler(move || {
        let _ = sig_tx.send(());
    })?; // SIGINT / SIGTERM

    sig_rx.recv().ok(); // ブロック

    // ---------------- 録音停止 & 転写起動 ----------------
    println!("Stopping recording…");
    let wav_path = rt.block_on(audio_recoder::stop_recording())?;
    sound_player::play_stop_sound();

    // 3 転写サブプロセスを detach で起動
    let exe = std::env::current_exe()?;
    match start_selected_text {
        Some(ref txt) if !txt.trim().is_empty() => {
            spawn_detached(
                Command::new(exe),
                ["transcribe", &wav_path, "--prompt", txt],
            )?;
        }
        _ => {
            spawn_detached(Command::new(exe), ["transcribe", &wav_path])?;
        }
    }

    println!("Spawned transcribe process for {wav_path}");
    Ok(())
}
