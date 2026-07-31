use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::infrastructure::push_to_talk::validate_hotkey;
use crate::utils::config::{
    TranscriptionProvider, parse_audio_pre_roll_ms, parse_max_duration_secs,
};

#[derive(Parser)]
#[command(author, version, about = "Voice Input client (daemon control + dict)")]
pub struct Cli {
    /// 利用可能な入力デバイスを一覧表示
    #[arg(long)]
    pub list_devices: bool,

    #[command(subcommand)]
    pub cmd: Option<Cmd>,
}

#[derive(Subcommand)]
pub enum Cmd {
    /// 録音開始
    Start {
        /// 録音後の音声データを指定パスへ保存
        #[arg(long, value_name = "PATH")]
        save_audio: Option<PathBuf>,
        /// 最大録音時間（秒）
        #[arg(long, value_name = "SECS", value_parser = parse_max_duration_secs_arg)]
        max_secs: Option<u64>,
        /// デバッグ用にマイク入力の代わりへ流すWAVファイル
        #[arg(long, value_name = "PATH")]
        input_file: Option<PathBuf>,
        /// 転写バックエンド（gpt-transcribe/gpt-live-transcribe/mlx-qwen3-asr）
        #[arg(long, value_name = "PROVIDER", value_parser = parse_transcription_provider)]
        transcription_provider: Option<TranscriptionProvider>,
    },
    /// 録音停止
    Stop,
    /// 録音開始 / 停止トグル
    Toggle {
        /// 録音後の音声データを指定パスへ保存
        #[arg(long, value_name = "PATH")]
        save_audio: Option<PathBuf>,
        /// 最大録音時間（秒）
        #[arg(long, value_name = "SECS", value_parser = parse_max_duration_secs_arg)]
        max_secs: Option<u64>,
        /// デバッグ用にマイク入力の代わりへ流すWAVファイル
        #[arg(long, value_name = "PATH")]
        input_file: Option<PathBuf>,
        /// 転写バックエンド（gpt-transcribe/gpt-live-transcribe/mlx-qwen3-asr）
        #[arg(long, value_name = "PROVIDER", value_parser = parse_transcription_provider)]
        transcription_provider: Option<TranscriptionProvider>,
    },
    /// デーモン状態取得
    Status,
    /// 最新の転写履歴を一覧表示
    History,
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
pub enum DictCmd {
    /// 対象語句へ変換する候補を追加
    #[command(name = "add")]
    Add {
        term: String,
        #[arg(required = true)]
        surfaces: Vec<String>,
    },
    /// 対象語句を削除
    #[command(name = "remove-term")]
    RemoveTerm { term: String },
    /// 対象語句から候補を削除
    #[command(name = "remove-variant")]
    RemoveVariant { term: String, surface: String },
    /// 一覧表示
    List,
}

#[derive(Subcommand)]
pub enum ConfigCmd {
    /// 設定値を永続化
    Set {
        #[command(subcommand)]
        field: ConfigField,
    },
    /// 現在有効な設定値を表示
    Get {
        #[arg(value_enum)]
        field: RuntimeConfigField,
    },
    /// 永続設定を削除して `.env` の `VOICE_INPUT_DEFAULT_*` へ戻す
    Unset {
        #[arg(value_enum)]
        field: RuntimeConfigField,
    },
    /// 現在有効な設定値を一覧表示
    Show,
}

#[derive(Subcommand)]
pub enum ConfigField {
    /// 辞書ファイルの保存先を指定
    #[command(name = "dict-path")]
    DictPath { path: String },
    /// 既定の転写バックエンドを指定
    #[command(name = "transcription-provider")]
    TranscriptionProvider {
        #[arg(value_parser = parse_transcription_provider)]
        provider: TranscriptionProvider,
    },
    /// 既定の最大録音時間を指定
    #[command(name = "max-secs")]
    MaxSecs {
        #[arg(value_parser = parse_max_duration_secs_arg)]
        secs: u64,
    },
    /// 録音開始時のpre-roll長を指定
    #[command(name = "pre-roll-ms")]
    PreRollMs {
        #[arg(value_parser = parse_audio_pre_roll_ms_arg)]
        millis: u64,
    },
    /// 入力デバイスの優先順位を指定
    #[command(name = "input-device-priorities")]
    InputDevicePriorities {
        #[arg(required = true, num_args = 1..)]
        priorities: Vec<String>,
    },
    /// 録音開始・停止サウンドの有効・無効を指定
    #[command(name = "recording-sounds-enabled")]
    RecordingSoundsEnabled {
        #[arg(action = clap::ArgAction::Set)]
        enabled: bool,
    },
    /// 録音状態HUDの有効・無効を指定
    #[command(name = "recording-hud-enabled")]
    RecordingHudEnabled {
        #[arg(action = clap::ArgAction::Set)]
        enabled: bool,
    },
    /// push-to-talkの有効・無効を指定
    #[command(name = "push-to-talk-enabled")]
    PushToTalkEnabled {
        #[arg(action = clap::ArgAction::Set)]
        enabled: bool,
    },
    /// push-to-talkのホットキーを指定
    #[command(name = "push-to-talk-hotkey")]
    PushToTalkHotkey {
        #[arg(value_parser = parse_push_to_talk_hotkey_arg)]
        hotkey: String,
    },
    /// GPT Transcribeのストリーミング直接入力を指定
    #[command(name = "transcribe-streaming")]
    TranscribeStreaming {
        #[arg(action = clap::ArgAction::Set)]
        enabled: bool,
    },
}

#[derive(Clone, Copy, clap::ValueEnum)]
pub enum RuntimeConfigField {
    TranscriptionProvider,
    MaxSecs,
    PreRollMs,
    InputDevicePriorities,
    RecordingSoundsEnabled,
    RecordingHudEnabled,
    PushToTalkEnabled,
    PushToTalkHotkey,
    TranscribeStreaming,
}

fn parse_transcription_provider(value: &str) -> Result<TranscriptionProvider, String> {
    TranscriptionProvider::parse(value).map_err(|error| error.to_string())
}

fn parse_max_duration_secs_arg(value: &str) -> Result<u64, String> {
    parse_max_duration_secs(value).map_err(|error| error.to_string())
}

fn parse_audio_pre_roll_ms_arg(value: &str) -> Result<u64, String> {
    parse_audio_pre_roll_ms(value).map_err(|error| error.to_string())
}

fn parse_push_to_talk_hotkey_arg(value: &str) -> Result<String, String> {
    validate_hotkey(value)?;
    Ok(value.to_string())
}

#[cfg(test)]
mod tests {
    use super::{Cli, Cmd, ConfigCmd, ConfigField, RuntimeConfigField, TranscriptionProvider};
    use clap::Parser;

    /// providerの永続設定コマンドを解釈できる
    #[test]
    fn parses_persistent_transcription_provider() {
        let cli = Cli::try_parse_from([
            "voice_input",
            "config",
            "set",
            "transcription-provider",
            "gpt-live-transcribe",
        ])
        .unwrap();

        assert!(matches!(
            cli.cmd,
            Some(Cmd::Config {
                action: ConfigCmd::Set {
                    field: ConfigField::TranscriptionProvider {
                        provider: TranscriptionProvider::GptLiveTranscribe
                    }
                }
            })
        ));
    }

    /// 最大録音時間の永続設定は0を拒否する
    #[test]
    fn rejects_zero_persistent_max_secs() {
        let result = Cli::try_parse_from(["voice_input", "config", "set", "max-secs", "0"]);

        assert!(result.is_err());
    }

    /// pre-rollの永続設定は上限超過を拒否する
    #[test]
    fn rejects_excessive_persistent_pre_roll() {
        let result = Cli::try_parse_from(["voice_input", "config", "set", "pre-roll-ms", "5001"]);

        assert!(result.is_err());
    }

    /// push-to-talkの永続設定は不正なホットキーを拒否する
    #[test]
    fn rejects_invalid_persistent_push_to_talk_hotkey() {
        let result = Cli::try_parse_from([
            "voice_input",
            "config",
            "set",
            "push-to-talk-hotkey",
            "opt+unknown-key",
        ]);

        assert!(result.is_err());
    }

    /// 実行時設定の解除対象を解釈できる
    #[test]
    fn parses_runtime_config_unset() {
        let cli = Cli::try_parse_from(["voice_input", "config", "unset", "transcription-provider"])
            .unwrap();

        assert!(matches!(
            cli.cmd,
            Some(Cmd::Config {
                action: ConfigCmd::Unset {
                    field: RuntimeConfigField::TranscriptionProvider
                }
            })
        ));
    }

    /// 実行時設定の一覧表示コマンドを解釈できる
    #[test]
    fn parses_runtime_config_show() {
        let cli = Cli::try_parse_from(["voice_input", "config", "show"]).unwrap();

        assert!(matches!(
            cli.cmd,
            Some(Cmd::Config {
                action: ConfigCmd::Show
            })
        ));
    }
}
