//! voice_input CLI: `voice_inputd` デーモンの簡易コントローラ。
//! `Start` / `Stop` / `Toggle` / `Status` の各コマンドを `ipc::send_cmd` で送信します。
use clap::{Parser, Subcommand};
use ime_voice_input::{
    domain::dict::{DictRepository, WordEntry},
    infrastructure::dict::JsonFileDictRepo,
    ipc::{IpcCmd, send_cmd},
};

#[derive(Parser)]
#[command(author, version, about = "Voice Input client (daemon control + dict)")]
struct Cli {
    /// 利用可能な入力デバイスを一覧表示
    #[arg(long)]
    list_devices: bool,

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
    /// 🔤 辞書操作
    Dict {
        #[command(subcommand)]
        action: DictCmd,
    },
}

#[derive(Subcommand)]
enum DictCmd {
    /// 登録 or 置換
    Add {
        surface: String,
        replacement: String,
    },
    /// 削除
    Remove { surface: String },
    /// 一覧表示
    List,
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

    /* ── 追加: デバイス一覧フラグ ── */
    if cli.list_devices {
        match send_cmd(&IpcCmd::ListDevices) {
            Ok(resp) if resp.ok => println!("{}", resp.msg),
            Ok(resp) => eprintln!("Error: {}", resp.msg),
            Err(e) => eprintln!("Error: {}", e),
        }
        return Ok(());
    }

    /* ───── コマンド解析 ──────────── */
    match cli.cmd.unwrap_or(Cmd::Toggle {
        paste: false,
        prompt: None,
    }) {
        /* 録音系 → IPC */
        Cmd::Start { paste, prompt } => relay(IpcCmd::Start { paste, prompt })?,
        Cmd::Stop => relay(IpcCmd::Stop)?,
        Cmd::Toggle { paste, prompt } => relay(IpcCmd::Toggle { paste, prompt })?,
        Cmd::Status => relay(IpcCmd::Status)?,

        /* 辞書操作 → ローカル JSON */
        Cmd::Dict { action } => {
            let repo = JsonFileDictRepo::new();
            match action {
                DictCmd::Add {
                    surface,
                    replacement,
                } => {
                    repo.upsert(WordEntry {
                        surface: surface.clone(),
                        replacement,
                        hit: 0,
                    })?;
                    println!("✅ Added/updated entry for “{surface}”");
                }
                DictCmd::Remove { surface } => {
                    if repo.delete(&surface)? {
                        println!("🗑️  Removed “{surface}”");
                    } else {
                        println!("ℹ️  No entry found for “{surface}”");
                    }
                }
                DictCmd::List => {
                    let list = repo.load()?;
                    if list.is_empty() {
                        println!("(no entries)");
                    } else {
                        println!("─ Dictionary ───────────────");
                        for e in list {
                            println!("• {:<20} → {}", e.surface, e.replacement);
                        }
                    }
                }
            }
        }
    }
    Ok(())
}

fn relay(cmd: IpcCmd) -> Result<(), Box<dyn std::error::Error>> {
    let resp = send_cmd(&cmd)?;
    if resp.ok {
        println!("{}", resp.msg);
    } else {
        eprintln!("Error: {}", resp.msg);
    }
    Ok(())
}
