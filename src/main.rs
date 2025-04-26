use std::{fs, process::Command, thread, time::Duration};

use arboard::Clipboard;
use clap::{Parser, Subcommand};
use tokio::runtime::Runtime;

use voice_input::{
    infrastructure::{
        audio::CpalAudioBackend,
        external::{
            clipboard, openai,
            sound::{
                pause_apple_music, play_start_sound, play_stop_sound,
                play_transcription_complete_sound, resume_apple_music,
            },
        },
    },
    spawn_detached,
};

/// Apple Music の再生状態を一時保存するマーカー
const MUSIC_MARKER_FILE: &str = "/tmp/voice_input_music_was_playing";

/// ===================================================
/// CLI 定義
/// ===================================================
#[derive(Parser, Debug)]
#[command(author, version, about = "Voice Input Toggle & Transcribe")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// バックグラウンドで呼ばれる転写サブコマンド
    Transcribe {
        wav: String,
        #[arg(long)]
        prompt: Option<String>,
        /// 転写後すぐに貼り付け
        #[arg(long, default_value_t = false)]
        paste: bool,
    },
    /// 録音 ↔ 停止トグル
    Record {
        /// 転写後すぐに貼り付け
        #[arg(long, default_value_t = false)]
        paste: bool,
    },
}

//
// ────────────────────────────────────────────────────────────
//  本番実行エントリ（テストビルド時は無効化）
// ────────────────────────────────────────────────────────────
//
#[cfg(not(test))]
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
    let txt = rt.block_on(openai::transcribe_audio(wav, prompt))?;

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

    play_transcription_complete_sound();
    println!("Transcription done:\n{txt}");

    // Apple Music を再開
    if std::path::Path::new(MUSIC_MARKER_FILE).exists() {
        resume_apple_music();
        let _ = fs::remove_file(MUSIC_MARKER_FILE);
    }

    Ok(())
}

/// ---------------------------------------------------
/// 録音トグル処理
fn run_record_cycle(paste: bool) -> Result<(), Box<dyn std::error::Error>> {
    // Apple Music 一時停止
    if pause_apple_music() {
        let _ = fs::write(MUSIC_MARKER_FILE, "");
    }
    play_start_sound();

    // ==== 録音開始 ====
    let start_selected_text = clipboard::get_selected_text().ok();
    let recorder = voice_input::domain::recorder::Recorder::new(CpalAudioBackend::default());

    recorder.start()?; // ← block_on 不要
    println!("Recording… もう一度 ⌥+8 で停止");

    // Ctrl-C 待ち
    let (tx, rx) = std::sync::mpsc::channel::<()>();
    ctrlc::set_handler(move || {
        let _ = tx.send(());
    })?;
    rx.recv().ok();

    // ==== 録音停止 ====
    println!("Stopping recording…");
    let wav_path = recorder.stop()?; // path 取得
    play_stop_sound();

    // ==== 転写プロセス detatch ====
    let exe = std::env::current_exe()?;
    let mut args: Vec<String> = vec![
        "transcribe".to_owned(),
        wav_path.clone(), // String のまま保持
    ];

    if paste {
        args.push("--paste".to_owned());
    }

    if let Some(txt) = start_selected_text.filter(|t| !t.trim().is_empty()) {
        args.push("--prompt".to_owned());
        args.push(txt); // 所有権ごと渡すので OK
    }
    spawn_detached(Command::new(exe), args)?;
    println!("Spawned transcribe process for {wav_path}");

    Ok(())
}

/* ===== ここからユニットテスト ===== */
#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn record_with_paste_flag_parses() {
        let cli = Cli::try_parse_from(["prog", "record", "--paste"]).unwrap();
        match cli.cmd.unwrap() {
            Cmd::Record { paste } => assert!(paste),
            _ => panic!("expected Record variant"),
        }
    }

    #[test]
    fn default_command_is_record() {
        let cli = Cli::try_parse_from(["prog"]).unwrap();
        let cmd = cli.cmd.unwrap_or(Cmd::Record { paste: false });
        match cmd {
            Cmd::Record { paste } => assert!(!paste),
            _ => panic!("expected default Record"),
        }
    }
}
