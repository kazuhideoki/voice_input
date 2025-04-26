use std::{fs, process::Command, thread, time::Duration};

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

fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenv::dotenv().ok();
    let cli = Cli::parse();

    match cli.cmd.unwrap_or(Cmd::Record { paste: false }) {
        Cmd::Transcribe { wav, prompt, paste } => run_transcription(&wav, prompt.as_deref(), paste),
        Cmd::Record { paste } => run_record_cycle(paste),
    }
}

/// ---------------------------------------------------
/// 転写処理（バックグラウンド実行）
fn run_transcription(
    wav: &str,
    prompt: Option<&str>,
    paste: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    println!("Transcribing {wav} …");

    let rt = Runtime::new()?;
    let txt = rt.block_on(transcribe_audio::transcribe_audio(wav, prompt))?;

    // ① クリップボードへコピー
    let mut clipboard = Clipboard::new()?;
    clipboard.set_text(&txt)?;

    // ② 必要ならペースト
    if paste {
        thread::sleep(Duration::from_millis(5)); // 反映待ち
        Command::new("osascript")
            .arg("-e")
            .arg(r#"tell application "System Events" to keystroke "v" using {command down}"#)
            .status()
            .ok();
    }

    sound_player::play_transcription_complete_sound();
    println!("Transcription done:\n{txt}");

    // Apple Music を再開
    if std::path::Path::new(MUSIC_MARKER_FILE).exists() {
        sound_player::resume_apple_music();
        let _ = fs::remove_file(MUSIC_MARKER_FILE);
    }

    Ok(())
}

/// ---------------------------------------------------
/// 録音トグル処理
fn run_record_cycle(paste: bool) -> Result<(), Box<dyn std::error::Error>> {
    // ---------- 録音開始 ----------
    let rt = Runtime::new()?;
    let (notify_tx, _notify_rx) = mpsc::channel::<()>(1);

    // Apple Music を一時停止
    if sound_player::pause_apple_music() {
        let _ = fs::write(MUSIC_MARKER_FILE, "");
    }
    sound_player::play_start_sound();

    // 選択テキスト取得 & 録音開始
    let start_selected_text = rt.block_on(request_speech_to_text::start_recording(notify_tx))?;
    println!("Recording… もう一度 ⌥+8 で停止 (Raycast が SIGINT を送ります)");

    // ---------- 停止待ち ----------
    let (sig_tx, sig_rx) = std::sync::mpsc::channel::<()>();
    ctrlc::set_handler(move || {
        let _ = sig_tx.send(());
    })?;
    sig_rx.recv().ok();

    // ---------- 録音停止 ----------
    println!("Stopping recording…");
    let wav_path = rt.block_on(audio_recoder::stop_recording())?;
    sound_player::play_stop_sound();

    // ---------- 転写サブプロセス detatch ----------
    let exe = std::env::current_exe()?;
    let mut args = vec!["transcribe", &wav_path];
    if paste {
        args.push("--paste");
    }
    if let Some(ref txt) = start_selected_text {
        if !txt.trim().is_empty() {
            args.extend(["--prompt", txt]);
        }
    }
    spawn_detached(Command::new(exe), args)?;
    println!("Spawned transcribe process for {wav_path}");
    Ok(())
}
