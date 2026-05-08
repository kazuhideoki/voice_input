use clap::{Parser, Subcommand};
use std::path::PathBuf;

use crate::utils::config::TranscriptionProvider;

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
        /// 転写バックエンド（4o/mlx-qwen3-asr）
        #[arg(long, value_name = "PROVIDER", value_parser = parse_transcription_provider)]
        transcription_provider: Option<TranscriptionProvider>,
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
        /// 転写バックエンド（4o/mlx-qwen3-asr）
        #[arg(long, value_name = "PROVIDER", value_parser = parse_transcription_provider)]
        transcription_provider: Option<TranscriptionProvider>,
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
pub enum DictCmd {
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
