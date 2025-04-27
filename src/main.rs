//! voice_input CLI: `voice_inputd` デーモンの簡易コントローラ。
//! `Start` / `Stop` / `Toggle` / `Status` の各コマンドを `ipc::send_cmd` で送信します。
use clap::{Parser, Subcommand};
use voice_input::ipc::{IpcCmd, send_cmd};

#[derive(Parser)]
#[command(author, version, about = "Voice Input client (daemon control)")]
struct Cli {
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// 録音開始
    Start {
        /// 転写後に即ペースト
        #[arg(long, default_value_t = false)]
        paste: bool,
        /// Whisper へ追加のプロンプト
        #[arg(long)]
        prompt: Option<String>,
    },
    /// 録音停止
    Stop,
    /// 録音開始 / 停止トグル
    Toggle {
        #[arg(long, default_value_t = false)]
        paste: bool,
        #[arg(long)]
        prompt: Option<String>,
    },
    /// デーモン状態取得
    Status,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // TODO env の扱いまとめる
    // .env 読み込み
    if let Ok(path) = std::env::var("VOICE_INPUT_ENV_PATH") {
        dotenvy::from_path(path).ok();
    } else {
        dotenvy::dotenv().ok();
    }

    let cli = Cli::parse();
    let resp = match cli.cmd.unwrap_or(Cmd::Toggle {
        paste: false,
        prompt: None,
    }) {
        Cmd::Start { paste, prompt } => send_cmd(&IpcCmd::Start { paste, prompt })?,
        Cmd::Stop => send_cmd(&IpcCmd::Stop)?,
        Cmd::Toggle { paste, prompt } => send_cmd(&IpcCmd::Toggle { paste, prompt })?,
        Cmd::Status => send_cmd(&IpcCmd::Status)?,
    };

    if resp.ok {
        println!("{}", resp.msg);
    } else {
        eprintln!("Error: {}", resp.msg);
    }
    Ok(())
}
