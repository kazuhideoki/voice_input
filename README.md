# Voice Input（音声入力ツール）

キーボード操作ひとつで **録音開始 → 録音停止 → 音声文字起こし → クリップボード貼付** までを自動化する、
Rust 製 CLI & Raycast スクリプトのセットです。
OpenAI® Speech-to-Text API を利用し、⽇常のメモ取りやコーディング補助を爆速化します。

---

## 全体の仕組み

```
[ユーザー] → [Raycast/CLI]
                  ↓
[voice_input] → [voice_inputd デーモン]
                  ↓
    [録音] → [OpenAI API] → [クリップボード]
```

> **補足**: CLI をターミナルから直接使う場合は、録音停止に `Ctrl+C` を送って同じフロー（SIGINT）を発火できます。

- **1 回目の呼び出し**: `record` が録音を開始
- **2 回目の呼び出し**: Raycast 側は既存プロセスに **SIGINT** を送り、録音を停止 → `transcribe` を子プロセスで実行
- 転写結果はクリップボードに入り、`--paste` 指定時は即貼り付け
- Apple Music 再生中の場合は自動で一時停止→復帰

### システムアーキテクチャ

本ツールは２つの主要コンポーネントで構成されています：

1. **voice_input** (クライアント CLI)
   - ユーザーからのコマンドを受け付け、デーモンへ IPC で転送
   - Record/Stop/Toggle/Status コマンドを提供

2. **voice_inputd** (バックグラウンドデーモン)
   - Unix Domain Socket でクライアントからの要求を待機
   - 録音、文字起こし、クリップボード操作を担当
   - Apple Music の自動一時停止と再開を管理

クライアントとデーモン間は `/tmp/voice_input.sock` を介して通信し、JSON シリアライズされたコマンドとレスポンスをやり取りします。

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
| **常駐デーモン**             | LaunchAgent によるバックグラウンド実行で常時待機                                                   |

---

## インストール

### 1. リポジトリをクローン & ビルド

```bash
git clone https://github.com/yourname/voice_input.git
cd voice_input
cargo build --release
```

生成されたバイナリ：
- クライアント: `target/release/voice_input`
- デーモン: `target/release/voice_inputd`

### 2. デーモンの LaunchAgent 設定

デーモンは macOS の LaunchAgent として実行する必要があります。以下の手順で設定します：

1. 以下の内容で LaunchAgent plist ファイルを **手動で作成** します

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>com.user.voiceinputd</string>
    <key>ProgramArguments</key>
    <array>
        <string>/Users/あなたのユーザー名/voice_input/target/release/voice_inputd</string>
    </array>
    <key>EnvironmentVariables</key>
    <dict>
        <key>OPENAI_API_KEY</key>
        <string>sk-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx</string>
    </dict>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>StandardOutPath</key>
    <string>/tmp/voiceinputd.log</string>
    <key>StandardErrorPath</key>
    <string>/tmp/voiceinputd.error.log</string>
</dict>
</plist>
```

2. ファイルを適切な場所に保存し、権限を設定

```bash
# ~/.env から OPENAI_API_KEY を取得して plist に設定
OPENAI_KEY=$(grep OPENAI_API_KEY ~/.env | cut -d= -f2)
mkdir -p ~/Library/LaunchAgents/
sed "s|あなたのユーザー名|$USER|g; s|sk-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx|$OPENAI_KEY|g" \
    voice_input/Library/LaunchAgents/com.user.voiceinputd.plist.sample \
    > ~/Library/LaunchAgents/com.user.voiceinputd.plist

# LaunchAgent を読み込む
launchctl load -w ~/Library/LaunchAgents/com.user.voiceinputd.plist
```

> **注意**: パスの `/Users/あなたのユーザー名/` 部分は、必ず自分のホームディレクトリに置き換えてください。

### 3. Raycast 連携スクリプトの設定

Raycast と連携するためのスクリプトを設定します：

1. run_voice_input.sh スクリプトを取得または作成

```bash
mkdir -p "$HOME/Library/Mobile Documents/com~apple~CloudDocs/Raycast Script/"
cat > "$HOME/Library/Mobile Documents/com~apple~CloudDocs/Raycast Script/run_voice_input.sh" << 'EOF'
#!/bin/bash

# voice_input を呼び出すための Raycast スクリプト
# 実行権限 chmod +x が必要

# 環境変数設定（.env から読み込む場合）
if [ -f "$HOME/.env" ]; then
  export $(grep -v '^#' "$HOME/.env" | xargs)
fi

# パスを環境に合わせて変更
VOICE_INPUT="$HOME/voice_input/target/release/voice_input"

# 現在選択しているテキストを取得（選択がある場合）
SELECTION=$(pbpaste)

# voice_input を toggle モードで実行（録音開始/停止の切り替え）
$VOICE_INPUT toggle --paste ${SELECTION:+--prompt "$SELECTION"}

exit 0
EOF

# 実行権限を付与
chmod +x "$HOME/Library/Mobile Documents/com~apple~CloudDocs/Raycast Script/run_voice_input.sh"
```

2. Raycast の設定

- Raycast を開き、「スクリプトコマンド」を追加
- スクリプトパスに `$HOME/Library/Mobile Documents/com~apple~CloudDocs/Raycast Script/run_voice_input.sh` を指定
- ショートカットキーを設定（例: `Option+V`）

> **ヒント**: iCloud Drive を使用していない場合は、スクリプトの保存先を適宜変更してください。

---

## 使い方

### ① 環境変数を設定

`.env` またはシェルの環境変数に以下を追加してください。
`.env.example` をコピーして編集すると便利です。

```dotenv
OPENAI_API_KEY=sk-xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx
# 任意：使用モデル（省略時は gpt-4o-mini-transcribe）
OPENAI_TRANSCRIBE_MODEL=gpt-4o-mini-transcribe
# 任意：入力デバイスの優先順位
INPUT_DEVICE_PRIORITY="device1,device2,device3"
```

### ② Raycast ショートカットを実行

1. **1 回目** … 録音開始（効果音「Ping」）
2. **2 回目** … 録音停止 → 音声文字起こし → 📋 コピー → ペースト
