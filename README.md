# Voice Input（音声入力ツール）

キーボード操作ひとつで **録音開始 → 録音停止 → 音声文字起こし → クリップボード貼付** までを自動化する、
Rust 製 CLI & Raycast スクリプトのセットです。
OpenAI® Speech-to-Text API を利用し、⽇常のメモ取りやコーディング補助を爆速化します。

---

## 全体の仕組み

$1

> **補足**: CLI をターミナルから直接使う場合は、録音停止に `Ctrl+C` を送って同じフロー（SIGINT）を発火できます。

- **1 回目の呼び出し**: `record` が録音を開始
- **2 回目の呼び出し**: Raycast 側は既存プロセスに **SIGINT** を送り、録音を停止 → `transcribe` を子プロセスで実行
- 転写結果はクリップボードに入り、`--paste` 指定時は即貼り付け
- Apple Music 再生中の場合は自動で一時停止→復帰

---

## 特徴 (Features)

| 機能                         | 概要                                                                                               |
| ---------------------------- | -------------------------------------------------------------------------------------------------- |
| **録音トリガ**               | Raycast ショートカットを **再度押す** か、CLI 実行時は **`Ctrl+C`** を送ると録音を停止し転写を開始 |
| **選択テキストを文脈に注入** | カーソル位置や選択範囲をプロンプトとして渡し、精度を向上                                           |
| **即ペースト** (`--paste`)   | 転写完了後すぐに貼り付けて手数ゼロ                                                                 |
| **Apple Music 自動ポーズ**   | 録音中は BGM をミュート、終了後に復帰                                                              |
| **高速起動**                 | CPAL 初期化を遅延実行、バイナリをストリップして <1 MB                                              |
| **クロスプラットフォーム**   | CPAL/hound 利用で macOS・Linux 対応（Windows は未検証）                                            |

---

## インストール

```bash
git clone https://github.com/yourname/voice_input.git
cd voice_input
cargo build --release         # バイナリ: target/release/voice_input

# Raycast 連携（例）
ln -s "$(pwd)/scripts/run_voice_input.sh" \
      "$HOME/.config/raycast/scripts/Run Voice Input"
```

---

## 使い方

### ① 環境変数を設定

`.env` またはシェルの環境変数に以下を追加してください。
`.env.example` をコピーして編集すると便利です。

```dotenv
OPENAI_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
# 任意：使用モデル（省略時は gpt-4o-mini-transcribe）
OPENAI_TRANSCRIBE_MODEL=gpt-4o-mini-transcribe
# 任意：WAV 保存ディレクトリ
INPUT_DEVICE_PRIORITY="device1,device2,device3"
```

### ② Raycast ショートカットを実行

1. **1 回目** … 録音開始（効果音「Ping」）
2. **2 回目** … 録音停止 → 音声文字起こし → 📋 コピー → ペースト（`--paste` 指定時）

### ③ CLI 直叩き（例）

```bash
# 録音トグル
target/release/voice_input record --paste

# 既存 WAV を文字起こし
target/release/voice_input transcribe ./foo.wav --prompt "専門用語はそのままカタカナで"
```

---

## 開発・テスト

```bash
# 依存解決 & 静的解析
cargo check
cargo clippy --all-targets --all-features -- -D warnings

# 単体テスト
cargo test
```

---

## TODO

- [ ] CPAL ストリームの常駐ウォームアップデーモン
