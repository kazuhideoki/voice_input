//! voice_input CLI: `voice_inputd` デーモンの簡易コントローラ。
//! `Start` / `Stop` / `Toggle` / `Status` の各コマンドを `ipc::send_cmd` で送信します。
use clap::Parser;
use voice_input::{
    cli::{Cli, Cmd, ConfigCmd, ConfigField, DictCmd, InputMode, resolve_input_mode},
    domain::dict::{DictRepository, EntryStatus, WordEntry},
    infrastructure::config::AppConfig,
    infrastructure::dict::JsonFileDictRepo,
    ipc::{IpcCmd, send_cmd},
    load_env,
    utils::config::EnvConfig,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_env();

    // 環境変数設定を初期化
    EnvConfig::init()?;

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
