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
        /// Whisper へ追加のプロンプト
        #[arg(long)]
        prompt: Option<String>,
        /// クリップボード経由でペースト（デフォルトの直接入力を無効化）
        #[arg(
            long,
            help = "Use clipboard copy-and-paste method instead of direct input"
        )]
        copy_and_paste: bool,
        /// クリップボードにコピーのみ（ペーストしない）
        #[arg(
            long,
            help = "Only copy to clipboard without pasting (conflicts with --copy-and-paste)"
        )]
        copy_only: bool,
    },
    /// 録音停止
    Stop,
    /// 録音開始 / 停止トグル
    Toggle {
        #[arg(long)]
        prompt: Option<String>,
        /// クリップボード経由でペースト（デフォルトの直接入力を無効化）
        #[arg(
            long,
            help = "Use clipboard copy-and-paste method instead of direct input"
        )]
        copy_and_paste: bool,
        /// クリップボードにコピーのみ（ペーストしない）
        #[arg(
            long,
            help = "Only copy to clipboard without pasting (conflicts with --copy-and-paste)"
        )]
        copy_only: bool,
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

/// フラグの競合をチェックし、入力モードを決定
#[derive(Debug, Clone, Copy, PartialEq)]
enum InputMode {
    Direct,       // デフォルト: 直接入力
    CopyAndPaste, // クリップボード経由でペースト
    CopyOnly,     // クリップボードにコピーのみ
}

fn resolve_input_mode(copy_and_paste: bool, copy_only: bool) -> Result<InputMode, &'static str> {
    match (copy_and_paste, copy_only) {
        (true, true) => Err("Cannot specify both --copy-and-paste and --copy-only"),
        (true, false) => Ok(InputMode::CopyAndPaste),
        (false, true) => Ok(InputMode::CopyOnly),
        (false, false) => Ok(InputMode::Direct), // デフォルトは直接入力
    }
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
        prompt: None,
        copy_and_paste: false,
        copy_only: false,
    }) {
        /* 録音系 → IPC */
        Cmd::Start {
            prompt,
            copy_and_paste,
            copy_only,
        } => {
            let input_mode = resolve_input_mode(copy_and_paste, copy_only)?;
            let direct_input = input_mode == InputMode::Direct;
            let paste = match input_mode {
                InputMode::Direct => true,       // 直接入力の場合は常にペースト
                InputMode::CopyAndPaste => true, // copy-and-pasteの場合も常にペースト
                InputMode::CopyOnly => false,    // copy_onlyの場合はペーストしない
            };
            relay(IpcCmd::Start {
                paste,
                prompt,
                direct_input,
            })?
        }
        Cmd::Stop => relay(IpcCmd::Stop)?,
        Cmd::Toggle {
            prompt,
            copy_and_paste,
            copy_only,
        } => {
            let input_mode = resolve_input_mode(copy_and_paste, copy_only)?;
            let direct_input = input_mode == InputMode::Direct;
            let paste = match input_mode {
                InputMode::Direct => true,       // 直接入力の場合は常にペースト
                InputMode::CopyAndPaste => true, // copy-and-pasteの場合も常にペースト
                InputMode::CopyOnly => false,    // copy_onlyの場合はペーストしない
            };
            relay(IpcCmd::Toggle {
                paste,
                prompt,
                direct_input,
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
