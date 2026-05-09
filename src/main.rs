//! voice_input CLI: `voice_inputd` デーモンの簡易コントローラ。
//! 録音操作（Start/Stop/Toggle/Status）のほか、ヘルスチェック、デバイス一覧、
//! 辞書操作、設定操作の各コマンドを `ipc::send_cmd` で送信します。
use clap::Parser;
use std::path::PathBuf;
use voice_input::{
    application::DictionaryService,
    cli::{Cli, Cmd, ConfigCmd, ConfigField, DictCmd},
    domain::dict::{EntryStatus, WordEntry},
    infrastructure::{config::AppConfig, dict::JsonFileDictRepo},
    ipc::{IpcCmd, send_cmd},
    load_env,
    utils::config::EnvConfig,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_env();

    let cli = Cli::parse();
    let max_secs_override = cli.cmd.as_ref().and_then(command_max_secs);

    // 環境変数設定を初期化。--max-secs は VOICE_INPUT_MAX_SECS より優先する。
    EnvConfig::init_with_recording_max_duration_secs(max_secs_override)?;

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
        save_audio: None,
        max_secs: None,
        input_file: None,
        transcription_provider: None,
        transcription_model: None,
    }) {
        /* 録音系 → IPC */
        Cmd::Start {
            prompt,
            save_audio,
            max_secs,
            input_file,
            transcription_provider,
            transcription_model,
        } => {
            let cmd = match canonicalize_input_file(input_file)? {
                Some(input_file_path) => IpcCmd::StartWithInputFile {
                    prompt,
                    save_audio_path: save_audio,
                    max_duration_secs: max_secs,
                    input_file_path,
                    transcription_provider,
                    transcription_model,
                },
                None => IpcCmd::Start {
                    prompt,
                    save_audio_path: save_audio,
                    max_duration_secs: max_secs,
                    transcription_provider,
                    transcription_model,
                },
            };
            relay(cmd)?;
        }
        Cmd::Stop => relay(IpcCmd::Stop)?,
        Cmd::Toggle {
            prompt,
            save_audio,
            max_secs,
            input_file,
            transcription_provider,
            transcription_model,
        } => {
            let cmd = match canonicalize_input_file(input_file)? {
                Some(input_file_path) => IpcCmd::ToggleWithInputFile {
                    prompt,
                    save_audio_path: save_audio,
                    max_duration_secs: max_secs,
                    input_file_path,
                    transcription_provider,
                    transcription_model,
                },
                None => IpcCmd::Toggle {
                    prompt,
                    save_audio_path: save_audio,
                    max_duration_secs: max_secs,
                    transcription_provider,
                    transcription_model,
                },
            };
            relay(cmd)?;
        }
        Cmd::Status => relay(IpcCmd::Status)?,
        Cmd::History => relay(IpcCmd::History)?,
        Cmd::Health => relay(IpcCmd::Health)?,

        /* 辞書操作 → ローカル JSON */
        Cmd::Dict { action } => {
            let service = DictionaryService::new(Box::new(JsonFileDictRepo::new()));
            match action {
                DictCmd::Add {
                    surface,
                    replacement,
                } => {
                    service.upsert(WordEntry {
                        surface: surface.clone(),
                        replacement,
                        hit: 0,
                        status: EntryStatus::Active,
                    })?;
                    println!("✅ Added/updated entry for “{surface}”");
                }
                DictCmd::Remove { surface } => {
                    if service.delete(&surface)? {
                        println!("🗑️  Removed “{surface}”");
                    } else {
                        println!("ℹ️  No entry found for “{surface}”");
                    }
                }
                DictCmd::List => {
                    let list = service.list()?;
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

fn command_max_secs(cmd: &Cmd) -> Option<u64> {
    match cmd {
        Cmd::Start { max_secs, .. } | Cmd::Toggle { max_secs, .. } => *max_secs,
        Cmd::Stop
        | Cmd::Status
        | Cmd::History
        | Cmd::Health
        | Cmd::Dict { .. }
        | Cmd::Config { .. } => None,
    }
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

fn canonicalize_input_file(
    input_file: Option<PathBuf>,
) -> Result<Option<PathBuf>, Box<dyn std::error::Error>> {
    input_file
        .map(|path| {
            std::fs::canonicalize(&path).map_err(|error| {
                format!("failed to resolve input file {}: {}", path.display(), error).into()
            })
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 入力ファイルの相対パスはdaemonへ送る前に絶対パスへ解決される
    #[test]
    fn input_file_path_is_canonicalized_before_ipc() {
        let target_dir = PathBuf::from("target/input-file-canonicalize-test");
        std::fs::create_dir_all(&target_dir).unwrap();
        let path = target_dir.join(format!("sample-{}.wav", std::process::id()));
        std::fs::write(&path, b"RIFF").unwrap();

        let resolved = canonicalize_input_file(Some(path.clone()))
            .unwrap()
            .unwrap();

        assert!(resolved.is_absolute());
        assert_eq!(resolved, std::fs::canonicalize(&path).unwrap());
        std::fs::remove_file(path).unwrap();
    }
}
