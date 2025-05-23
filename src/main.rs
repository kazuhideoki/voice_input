//! voice_input CLI: `voice_inputd` デーモンの簡易コントローラ。
//! `Start` / `Stop` / `Toggle` / `Status` の各コマンドを `ipc::send_cmd` で送信します。
use clap::{Parser, Subcommand};
use voice_input::{
    domain::dict::{DictRepository, EntryStatus, WordEntry},
    infrastructure::config::AppConfig,
    infrastructure::dict::JsonFileDictRepo,
    ipc::{IpcCmd, send_cmd},
    load_env,
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
    /// ヘルスチェック
    Health,
    /// 🔤 辞書操作
    Dict {
        #[command(subcommand)]
        action: DictCmd,
    },
    /// 各種設定操作
    Config {
        #[command(subcommand)]
        action: ConfigCmd,
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

#[derive(Subcommand)]
enum ConfigCmd {
    /// `dict-path` 設定
    Set {
        #[command(subcommand)]
        field: ConfigField,
    },
}

#[derive(Subcommand)]
enum ConfigField {
    /// 辞書ファイルの保存先を指定
    #[command(name = "dict-path")]
    DictPath { path: String },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_env();

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
        Cmd::Start { paste, prompt } => {
            // TODO(P1-4): direct_input引数を追加し、CLIから受け取れるようにする
            relay(IpcCmd::Start {
                paste,
                prompt,
                direct_input: false,
            })?
        }
        Cmd::Stop => relay(IpcCmd::Stop)?,
        Cmd::Toggle { paste, prompt } => {
            // TODO(P1-4): direct_input引数を追加し、CLIから受け取れるようにする
            relay(IpcCmd::Toggle {
                paste,
                prompt,
                direct_input: false,
            })?
        }
        Cmd::Status => relay(IpcCmd::Status)?,
        Cmd::Health => relay(IpcCmd::Health)?,

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
                        status: EntryStatus::Active,
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
                            println!("• {:<20} → {} [{}]", e.surface, e.replacement, e.status);
                        }
                    }
                }
            }
        }
        Cmd::Config { action } => match action {
            ConfigCmd::Set { field } => match field {
                ConfigField::DictPath { path } => {
                    let mut cfg = AppConfig::load();
                    cfg.set_dict_path(std::path::PathBuf::from(&path))?;
                    println!("✅ dict-path set to {path}");
                }
            },
        },
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
