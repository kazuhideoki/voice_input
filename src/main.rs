mod audio_recoder;
mod text_selection;
mod transcribe_audio;
mod request_speech_to_text;

use clap::{Parser, Subcommand};
use dotenv::dotenv;
use request_speech_to_text::{start_recording, stop_recording_and_transcribe};
use tokio::runtime::Runtime;

#[derive(Parser)]
#[command(name = "voice_input")]
#[command(about = "音声入力ツール", version, long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// 録音を開始する
    Start,
    /// 録音を停止して文字起こしを行う
    Stop,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // .env ファイルの読み込み
    dotenv().ok();
    println!("環境変数を読み込みました");

    // コマンドライン引数をパース
    let cli = Cli::parse();

    // tokio ランタイムを作成して非同期処理を実行
    let rt = Runtime::new()?;

    match cli.command {
        Some(Commands::Start) => {
            println!("録音を開始します...");
            rt.block_on(start_recording())?;
            println!("録音を開始しました。録音を停止するには Enter キーを押してください。");
            
            // 入力を待機し、Enterが押されたら録音を停止する
            let mut input = String::new();
            std::io::stdin().read_line(&mut input)?;
            
            println!("録音を停止します...");
            let transcription = rt.block_on(stop_recording_and_transcribe())?;
            println!("録音を停止しました。");
            println!("文字起こし結果: {}", transcription);
        }
        Some(Commands::Stop) => {
            println!("録音を停止します...");
            let transcription = rt.block_on(stop_recording_and_transcribe())?;
            println!("録音を停止しました。");
            println!("文字起こし結果: {}", transcription);
        }
        None => {
            println!("コマンドが指定されていません。'start' または 'stop' を指定してください。");
            println!("例: cargo run -- start");
            println!("例: cargo run -- stop");
        }
    }

    Ok(())
}
