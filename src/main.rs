//! voice_input CLI: `voice_inputd` デーモンの簡易コントローラ。
//! 録音操作（Start/Stop/Toggle/Status）のほか、ヘルスチェック、デバイス一覧、
//! 辞書操作、設定操作の各コマンドを `ipc::send_cmd` で送信します。
use clap::Parser;
use std::path::PathBuf;
use voice_input::{
    application::DictionaryService,
    cli::{Cli, Cmd, ConfigCmd, ConfigField, DictCmd, RuntimeConfigField, format_dictionary},
    infrastructure::{config::AppConfig, dict::JsonFileDictRepo},
    ipc::{IpcCmd, send_cmd},
    load_env,
    utils::config::EnvConfig,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    load_env();

    let cli = Cli::parse();
    let max_secs_override = cli.cmd.as_ref().and_then(command_max_secs);
    let persisted_config = AppConfig::load();

    // 永続設定とコマンド指定をアプリケーション既定値より優先して、起動中不変の設定を構築する。
    EnvConfig::init_with_user_setting_overrides(
        &persisted_config.user_setting_overrides(),
        max_secs_override,
    )?;
    AppConfig::init_runtime(persisted_config);

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
        save_audio: None,
        max_secs: None,
        input_file: None,
        transcription_provider: None,
    }) {
        /* 録音系 → IPC */
        Cmd::Start {
            save_audio,
            max_secs,
            input_file,
            transcription_provider,
        } => {
            let cmd = match canonicalize_input_file(input_file)? {
                Some(input_file_path) => IpcCmd::StartWithInputFile {
                    save_audio_path: save_audio,
                    max_duration_secs: max_secs,
                    input_file_path,
                    transcription_provider,
                },
                None => IpcCmd::Start {
                    save_audio_path: save_audio,
                    max_duration_secs: max_secs,
                    transcription_provider,
                },
            };
            relay(cmd)?;
        }
        Cmd::Stop => relay(IpcCmd::Stop)?,
        Cmd::Toggle {
            save_audio,
            max_secs,
            input_file,
            transcription_provider,
        } => {
            let cmd = match canonicalize_input_file(input_file)? {
                Some(input_file_path) => IpcCmd::ToggleWithInputFile {
                    save_audio_path: save_audio,
                    max_duration_secs: max_secs,
                    input_file_path,
                    transcription_provider,
                },
                None => IpcCmd::Toggle {
                    save_audio_path: save_audio,
                    max_duration_secs: max_secs,
                    transcription_provider,
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
                DictCmd::Add { term, surfaces } => {
                    let result = service.add_variants(&term, &surfaces)?;
                    let variants = surfaces.join(", ");
                    if result.term_created {
                        println!(
                            "✅ Created term “{term}” and added/updated variants “{variants}”"
                        );
                    } else {
                        println!(
                            "✅ Added/updated variants “{variants}” to existing term “{term}”"
                        );
                    }
                }
                DictCmd::RemoveTerm { term } => {
                    if service.delete_term(&term)? {
                        println!("🗑️  Removed term “{term}”");
                    } else {
                        println!("ℹ️  No term found for “{term}”");
                    }
                }
                DictCmd::RemoveVariant { term, surface } => {
                    if service.delete_variant(&term, &surface)? {
                        println!("🗑️  Removed variant “{surface}” from “{term}”");
                    } else {
                        println!("ℹ️  No variant found for “{surface}” in “{term}”");
                    }
                }
                DictCmd::List => {
                    let document = service.list()?;
                    println!("{}", format_dictionary(&document));
                }
            }
        }
        Cmd::Config { action } => handle_config_command(action)?,
    }
    Ok(())
}

fn handle_config_command(action: ConfigCmd) -> Result<(), Box<dyn std::error::Error>> {
    let mut config = AppConfig::load();
    match action {
        ConfigCmd::Set { field } => match field {
            ConfigField::DictPath { path } => {
                config.set_dict_path(std::path::PathBuf::from(&path))?;
                println!("✅ dict_path set to {path}");
            }
            ConfigField::TranscriptionProvider { provider } => {
                config.set_transcription_provider(provider)?;
                println!("✅ transcription_provider set to {}", provider.as_str());
                notify_daemon_config_changed();
            }
            ConfigField::MaxSecs { secs } => {
                config.set_max_secs(secs)?;
                println!("✅ max_secs set to {secs}");
                notify_daemon_config_changed();
            }
            ConfigField::PreRollMs { millis } => {
                config.set_pre_roll_ms(millis)?;
                println!("✅ pre_roll_ms set to {millis}");
                notify_daemon_config_changed();
            }
            ConfigField::InputDevicePriorities { priorities } => {
                config.set_input_device_priorities(priorities.clone())?;
                println!(
                    "✅ input_device_priorities set to {}",
                    priorities.join(", ")
                );
                notify_daemon_config_changed();
            }
            ConfigField::RecordingSoundsEnabled { enabled } => {
                config.set_recording_sounds_enabled(enabled)?;
                println!("✅ recording_sounds_enabled set to {enabled}");
                notify_daemon_config_changed();
            }
            ConfigField::RecordingHudEnabled { enabled } => {
                config.set_recording_hud_enabled(enabled)?;
                println!("✅ recording_hud_enabled set to {enabled}");
                notify_daemon_config_changed();
            }
            ConfigField::PushToTalkEnabled { enabled } => {
                config.set_push_to_talk_enabled(enabled)?;
                println!("✅ push_to_talk_enabled set to {enabled}");
                notify_daemon_config_changed();
            }
            ConfigField::PushToTalkHotkey { hotkey } => {
                config.set_push_to_talk_hotkey(hotkey.clone())?;
                println!("✅ push_to_talk_hotkey set to {hotkey}");
                notify_daemon_config_changed();
            }
            ConfigField::TranscribeStreaming { enabled } => {
                config.set_transcribe_streaming(enabled)?;
                println!("✅ transcribe_streaming set to {enabled}");
                notify_daemon_config_changed();
            }
        },
        ConfigCmd::Get { field } => print_runtime_config(field, &config),
        ConfigCmd::Show => print_all_config(&config),
        ConfigCmd::Unset { field } => {
            match field {
                RuntimeConfigField::TranscriptionProvider => {
                    config.unset_transcription_provider()?;
                }
                RuntimeConfigField::MaxSecs => config.unset_max_secs()?,
                RuntimeConfigField::PreRollMs => config.unset_pre_roll_ms()?,
                RuntimeConfigField::InputDevicePriorities => {
                    config.unset_input_device_priorities()?
                }
                RuntimeConfigField::RecordingSoundsEnabled => {
                    config.unset_recording_sounds_enabled()?
                }
                RuntimeConfigField::RecordingHudEnabled => config.unset_recording_hud_enabled()?,
                RuntimeConfigField::PushToTalkEnabled => config.unset_push_to_talk_enabled()?,
                RuntimeConfigField::PushToTalkHotkey => config.unset_push_to_talk_hotkey()?,
                RuntimeConfigField::TranscribeStreaming => config.unset_transcribe_streaming()?,
            }
            println!("✅ {} unset", runtime_config_field_name(field));
            notify_daemon_config_changed();
        }
    }
    Ok(())
}

fn print_all_config(config: &AppConfig) {
    println!("dict_path={}", config.dict_path().display());
    println!(
        "transcription_provider={}",
        config.effective_transcription_provider().as_str()
    );
    println!("max_secs={}", config.effective_max_secs());
    println!("pre_roll_ms={}", config.effective_pre_roll_ms());
    println!(
        "input_device_priorities={}",
        config.effective_input_device_priorities().join(",")
    );
    println!(
        "recording_sounds_enabled={}",
        config.effective_recording_sounds_enabled()
    );
    println!(
        "recording_hud_enabled={}",
        config.effective_recording_hud_enabled()
    );
    println!(
        "push_to_talk_enabled={}",
        config.effective_push_to_talk_enabled()
    );
    println!(
        "push_to_talk_hotkey={}",
        config.effective_push_to_talk_hotkey()
    );
    println!(
        "transcribe_streaming={}",
        config.effective_transcribe_streaming()
    );
}

fn print_runtime_config(field: RuntimeConfigField, config: &AppConfig) {
    match field {
        RuntimeConfigField::TranscriptionProvider => {
            println!("{}", config.effective_transcription_provider().as_str())
        }
        RuntimeConfigField::MaxSecs => println!("{}", config.effective_max_secs()),
        RuntimeConfigField::PreRollMs => println!("{}", config.effective_pre_roll_ms()),
        RuntimeConfigField::InputDevicePriorities => {
            println!("{}", config.effective_input_device_priorities().join(","))
        }
        RuntimeConfigField::RecordingSoundsEnabled => {
            println!("{}", config.effective_recording_sounds_enabled())
        }
        RuntimeConfigField::RecordingHudEnabled => {
            println!("{}", config.effective_recording_hud_enabled())
        }
        RuntimeConfigField::PushToTalkEnabled => {
            println!("{}", config.effective_push_to_talk_enabled())
        }
        RuntimeConfigField::PushToTalkHotkey => {
            println!("{}", config.effective_push_to_talk_hotkey())
        }
        RuntimeConfigField::TranscribeStreaming => {
            println!("{}", config.effective_transcribe_streaming())
        }
    }
}

fn runtime_config_field_name(field: RuntimeConfigField) -> &'static str {
    match field {
        RuntimeConfigField::TranscriptionProvider => "transcription_provider",
        RuntimeConfigField::MaxSecs => "max_secs",
        RuntimeConfigField::PreRollMs => "pre_roll_ms",
        RuntimeConfigField::InputDevicePriorities => "input_device_priorities",
        RuntimeConfigField::RecordingSoundsEnabled => "recording_sounds_enabled",
        RuntimeConfigField::RecordingHudEnabled => "recording_hud_enabled",
        RuntimeConfigField::PushToTalkEnabled => "push_to_talk_enabled",
        RuntimeConfigField::PushToTalkHotkey => "push_to_talk_hotkey",
        RuntimeConfigField::TranscribeStreaming => "transcribe_streaming",
    }
}

fn notify_daemon_config_changed() {
    match send_cmd(&IpcCmd::ReloadConfig) {
        Ok(response) if !response.ok => {
            eprintln!(
                "⚠️  Setting was saved, but the daemon could not apply it: {}",
                response.msg
            );
        }
        Err(error) => {
            eprintln!("ℹ️  Setting was saved and will apply when the daemon starts: {error}");
        }
        Ok(_) => {}
    }
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
