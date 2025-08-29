# Voice Input — プロジェクト概要 (Onboarding)

- 目的: Rust 製の「音声録音 → 転写（OpenAI） → テキスト入力」を行う CLI/デーモン。`voice_input`(CLI) と `voice_inputd`(常駐) が IPC(Unix Domain Socket) で連携し、録音・転写と結果の直接入力（デフォルト）を提供します。
- 主要構成: `[CLI] → [/tmp/voice_input.sock] → [voice_inputd] → (録音/転写/テキスト入力)`
- 対象OS: Darwin(macOS)

## 技術スタック
- 言語/Edition: Rust (edition = 2024)
- 非同期: `tokio`, `futures`
- オーディオ: `cpal`
- HTTP/API: `reqwest`, 転写は OpenAI API（モデル例: `gpt-4o-mini-transcribe`）
- シリアライズ: `serde`, `serde_json`
- 入力・UI: `enigo`(キーボード入力), `egui`/`eframe`(UI), `rdev`(グローバルショートカット)
- エラー/ユーティリティ: `thiserror`, `async-trait`, `once_cell`, `directories`, `chrono`

## エントリポイント（バイナリ）
- `voice_input`: クライアント CLI（`src/main.rs`）
- `voice_inputd`: 常駐デーモン（`src/bin/voice_inputd.rs`）
- `voice_input_ui`: UI プロセス（`src/bin/voice_input_ui.rs`）
- `migrate_dict`: 辞書マイグレーション
- `enigo_helper`: Enigo 入力ヘルパー

## 主なディレクトリ
- `src/application`: サービス層（録音/転写/スタック/ワーカー等）
- `src/domain`: ドメイン層（録音・辞書・スタック等のモデル）
- `src/infrastructure`: 外部I/O（音声、OpenAI、UI、辞書Repo、設定、IPC 等）
- `src/monitoring`: メモリ監視・メトリクス
- `src/shortcut`: ショートカット処理
- `src/utils`: 環境変数/設定ユーティリティ
- `tests/`, `benches/`, `examples/`

## 設定（.env）
- 手順: `cp .env.example .env`
- 代表変数:
  - `OPENAI_API_KEY`
  - `OPENAI_TRANSCRIBE_MODEL`（デフォルト: `gpt-4o-mini-transcribe`）
  - `INPUT_DEVICE_PRIORITY`（優先デバイスのカンマ区切り）
  - `VOICE_INPUT_USE_SUBPROCESS`（旧実装の暫定利用フラグ・非推奨）

## テキスト入力方式
- デフォルト: 直接入力（カーソル位置へ直接タイプ; クリップボード非汚染・高速）
- 代替: クリップボード方式（`--copy-and-paste` / `--copy-only`）

## macOS 権限/サービス
- ラッパー: `/usr/local/bin/voice_inputd_wrapper` が `target/release/voice_inputd` を実行
- LaunchAgent: `com.user.voiceinputd`（標準出力/エラーは `/tmp/voice_inputd.{out,err}`）
- アクセシビリティ: ターミナル/ラッパーに権限付与が必要
- 補助スクリプト: `./scripts/setup-dev-env.sh`, `./scripts/dev-build.sh`

## 辞書機能
- 置換辞書: `~/Library/Application Support/voice_input/dictionary.json`
- CLI で追加/削除/参照、保存先は `voice_input config set dict-path <PATH>` で変更可（旧ファイルは `.bak`）

## テスト/品質/CI 概要
- ローカル: `cargo test`（全テスト）
- CI 向け: `cargo test --features ci-test`（環境依存テストは無効化）
- フォーマット: `cargo fmt -- --check`
- Lint: `cargo clippy -- -D warnings`（CI では `--all-targets --features ci-test` 併用）
- 品質スクリプト: `./scripts/quality-check.sh [--bench|--memory]`

## IPC/ログ/トラブルシュート
- IPC: `/tmp/voice_input.sock`
- デーモン再起動: `launchctl kickstart -k user/$(id -u)/com.user.voiceinputd`（失敗時はラッパーを直接起動）
- ログ確認: `tail -f /tmp/voice_inputd.err`
