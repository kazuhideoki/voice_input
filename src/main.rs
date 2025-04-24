use std::path::Path;
use std::process::Command;

use arboard::Clipboard;
use clap::{Parser, Subcommand};
use tokio::{runtime::Runtime, sync::mpsc};

mod audio_recoder;
mod key_monitor;
mod request_speech_to_text; // ← start_recording だけ使う
mod sound_player;
mod text_selection;
mod transcribe_audio;

use audio_recoder::RECORDING_STATUS_FILE;
use voice_input::spawn_detached;

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
    /// 録音開始 / 停止トグル（デフォルト）
    Toggle,
    /// WAV ファイルをバックグラウンドで文字起こし
    Transcribe {
        wav: String,
        #[arg(long)]
        prompt: Option<String>,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();
    let cli = Cli::parse();

    match cli.cmd.unwrap_or(Cmd::Toggle) {
        Cmd::Toggle => toggle_flow(),
        Cmd::Transcribe { wav, prompt } => transcribe_flow(&wav, prompt.as_deref()),
    }
}

/// ---------------------------------------------------
/// トグル処理
/// ---------------------------------------------------
fn toggle_flow() -> Result<(), Box<dyn std::error::Error>> {
    // Tokio ランタイム（同期/非同期混在のため都度生成）
    let rt = Runtime::new()?;

    // 録音中かどうか判定
    if !Path::new(RECORDING_STATUS_FILE).exists() {
        // -------------------- 録音開始 --------------------
        println!("Starting recording...");
        let (notify_tx, _notify_rx) = mpsc::channel::<()>(1);
        rt.block_on(request_speech_to_text::start_recording(notify_tx))?;
        sound_player::play_start_sound();
        println!("Recording… もう一度 Alt+8 で停止");
        return Ok(());
    }

    // -------------------- 録音停止 --------------------
    println!("Stopping recording…");
    let wav_path = rt.block_on(audio_recoder::stop_recording())?;
    sound_player::play_stop_sound();

    // detach で転写サブプロセスを起動
    let exe = std::env::current_exe()?;
    spawn_detached(Command::new(exe), ["transcribe", &wav_path])?;
    println!("Spawned transcribe process for {wav_path}");

    Ok(())
}

/// ---------------------------------------------------
/// 転写処理（バックグラウンドで実行）
/// ---------------------------------------------------
fn transcribe_flow(wav: &str, prompt: Option<&str>) -> Result<(), Box<dyn std::error::Error>> {
    println!("Transcribing {wav} …");

    let rt = Runtime::new()?;
    let txt = rt.block_on(transcribe_audio::transcribe_audio(wav, prompt))?;

    // クリップボードに貼り付け
    let mut clipboard = Clipboard::new()?;
    clipboard.set_text(&txt)?;

    sound_player::play_transcription_complete_sound();
    println!("Transcription done:\n{txt}");

    Ok(())
}
