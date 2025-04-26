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
    Toggle,
    Transcribe {
        wav: String,
        #[arg(long)]
        prompt: Option<String>,
    },
    Record, // ★ 追加
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();
    let cli = Cli::parse();

    match cli.cmd.unwrap_or(Cmd::Toggle) {
        Cmd::Toggle => toggle_flow(),
        Cmd::Transcribe { wav, prompt } => transcribe_flow(&wav, prompt.as_deref()),
        Cmd::Record => record_flow(), // ★ 追加
    }
}

/// ---------------------------------------------------
/// トグル処理
/// ---------------------------------------------------
fn toggle_flow() -> Result<(), Box<dyn std::error::Error>> {
    // Tokio ランタイム（同期/非同期混在のため都度生成）
    let rt = Runtime::new()?;

    // 録音中かどうか判定
    // ---------- 録音状態を確認（孤児ファイル対策付き） ----------
    // if Path::new(RECORDING_STATUS_FILE).exists() {
    //     if someone_else_is_recording() {
    //         return Err("すでに録音中です".into());
    //     }
    //     fs::remove_file(RECORDING_STATUS_FILE).ok(); // 孤児なら掃除
    // }

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

fn record_flow() -> Result<(), Box<dyn std::error::Error>> {
    // ---------------- 録音開始 ----------------
    let rt = Runtime::new()?;
    let (notify_tx, _notify_rx) = mpsc::channel::<()>(1);
    rt.block_on(request_speech_to_text::start_recording(notify_tx))?;
    sound_player::play_start_sound();
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

    let exe = std::env::current_exe()?;
    spawn_detached(Command::new(exe), ["transcribe", &wav_path])?;
    println!("Spawned transcribe process for {wav_path}");

    Ok(())
}
