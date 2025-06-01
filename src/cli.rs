use clap::{Parser, Subcommand};

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
    /// スタックモード制御
    #[command(name = "stack-mode")]
    StackMode {
        #[command(subcommand)]
        action: StackModeCmd,
    },
    /// スタックをペースト
    Paste {
        /// ペーストするスタック番号 (1-based)
        number: u32,
    },
    /// スタック一覧表示
    #[command(name = "list-stacks")]
    ListStacks,
    /// 全スタックをクリア
    #[command(name = "clear-stacks")]
    ClearStacks,
}

#[derive(Subcommand)]
pub enum StackModeCmd {
    /// スタックモードを有効化
    On,
    /// スタックモードを無効化
    Off,
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

/// フラグの競合をチェックし、入力モードを決定
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputMode {
    Direct,       // デフォルト: 直接入力
    CopyAndPaste, // クリップボード経由でペースト
    CopyOnly,     // クリップボードにコピーのみ
}

pub fn resolve_input_mode(
    copy_and_paste: bool,
    copy_only: bool,
) -> Result<InputMode, &'static str> {
    match (copy_and_paste, copy_only) {
        (true, true) => Err("Cannot specify both --copy-and-paste and --copy-only"),
        (true, false) => Ok(InputMode::CopyAndPaste),
        (false, true) => Ok(InputMode::CopyOnly),
        (false, false) => Ok(InputMode::Direct), // デフォルトは直接入力
    }
}
