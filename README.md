# Voice Input

Rust 製の **音声録音・文字起こし CLI / デーモン** です。
`voice_input` はクライアント CLI、`voice_inputd` はバックグラウンド常駐デーモンとして動作します。

[CLI] → [/tmp/voice_input.sock] → [voice_inputd] → (録音 / 転写 / クリップボード)

## 特徴

| 機能                       | 説明                                  |
| -------------------------- | ------------------------------------- |
| **高速録音トグル**         | 1 コマンドで録音開始 / 停止を切替     |
| **OpenAI API 対応**        | 日本語・英語を自動認識                |
| **Apple Music 自動ポーズ** | 録音中は BGM を一時停止               |
| **IPC Unix Socket**        | CLI ↔ デーモン間通信は JSON over UDS |

## 環境変数準備

```sh
cp .env.example .env
```

- OPENAI_API_KEY=your_openai_api_key_here
- OPENAI_TRANSCRIBE_MODEL=gpt-4o-mini-transcribe # デフォルト
 - INPUT_DEVICE_PRIORITY="device1,device2,device3"

## ビルド

```bash
git clone https://github.com/yourname/voice_input.git
cd voice_input
cargo build --release

# 生成物:
# - target/release/voice_input … CLI
# - target/release/voice_inputd … デーモン
```

## MacOS での権限設定

以下、ペーストできるようにする

- `設定` -> `プライバシーとセキュリティ` -> `アクセシビリティ`
  - **メインで使うターミナル** に許可を与える
  - `/Users/kazuhideoki/voice_input/target/release/voice_inputd` **再ビルド時再設定**

**再ビルド時は `voiceinputd` のデーモンの再起動**

```sh
launchctl unload ~/Library/LaunchAgents/com.user.voiceinputd.plist
launchctl load ~/Library/LaunchAgents/com.user.voiceinputd.plist
```

```sh
osascript -e 'tell app "System Events" to keystroke "v" using {command down}'
```

また、初回実行時にはいくつか権限のリクエストが来る。

## 使い方（基本）

録音開始,停止

```sh
voice_input start
voice_input stop
```

利用可能な入力デバイスを一覧表示

```sh
voice_input --list-devices
```
入力デバイス名とインデックスを表示します。環境変数 `INPUT_DEVICE_PRIORITY` を
設定する際の参考にしてください。

録音開始,停止の切り替え+ペースト。

```sh
voice_input toggle --paste
```

デーモンと外部依存の状態をまとめて確認:

```sh
voice_input health
```
