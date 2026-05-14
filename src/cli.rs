use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::utils::config::{TranscriptionProvider, parse_max_duration_secs};

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
        /// Whisper へ追加のプロンプト
        #[arg(long)]
        prompt: Option<String>,
        /// 録音後の音声データを指定パスへ保存
        #[arg(long, value_name = "PATH")]
        save_audio: Option<PathBuf>,
        /// 最大録音時間（秒）
        #[arg(long, value_name = "SECS", value_parser = parse_max_duration_secs_arg)]
        max_secs: Option<u64>,
        /// デバッグ用にマイク入力の代わりへ流すWAVファイル
        #[arg(long, value_name = "PATH")]
        input_file: Option<PathBuf>,
        /// 転写バックエンド（4o/realtime-whisper/mlx-qwen3-asr）
        #[arg(long, value_name = "PROVIDER", value_parser = parse_transcription_provider)]
        transcription_provider: Option<TranscriptionProvider>,
        /// 4o 転写モデル（gpt-4o-transcribe/gpt-4o-mini-transcribe）
        #[arg(long, value_name = "MODEL", value_parser = parse_transcription_model)]
        transcription_model: Option<String>,
    },
    /// 録音停止
    Stop,
    /// 録音開始 / 停止トグル
    Toggle {
        #[arg(long)]
        prompt: Option<String>,
        /// 録音後の音声データを指定パスへ保存
        #[arg(long, value_name = "PATH")]
        save_audio: Option<PathBuf>,
        /// 最大録音時間（秒）
        #[arg(long, value_name = "SECS", value_parser = parse_max_duration_secs_arg)]
        max_secs: Option<u64>,
        /// デバッグ用にマイク入力の代わりへ流すWAVファイル
        #[arg(long, value_name = "PATH")]
        input_file: Option<PathBuf>,
        /// 転写バックエンド（4o/realtime-whisper/mlx-qwen3-asr）
        #[arg(long, value_name = "PROVIDER", value_parser = parse_transcription_provider)]
        transcription_provider: Option<TranscriptionProvider>,
        /// 4o 転写モデル（gpt-4o-transcribe/gpt-4o-mini-transcribe）
        #[arg(long, value_name = "MODEL", value_parser = parse_transcription_model)]
        transcription_model: Option<String>,
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
    /// `dict-path` 設定
    Set {
        #[command(subcommand)]
        field: ConfigField,
    },
}

#[derive(Subcommand)]
pub enum ConfigField {
    /// 辞書ファイルの保存先を指定
    #[command(name = "dict-path")]
    DictPath { path: String },
}

fn parse_transcription_provider(value: &str) -> Result<TranscriptionProvider, String> {
    TranscriptionProvider::parse(value).map_err(|error| error.to_string())
}

fn parse_transcription_model(value: &str) -> Result<String, String> {
    TranscriptionProvider::OpenAi4o
        .validate_model(value)
        .map(|()| value.to_string())
        .map_err(|error| error.to_string())
}

fn parse_max_duration_secs_arg(value: &str) -> Result<u64, String> {
    parse_max_duration_secs(value).map_err(|error| error.to_string())
}
