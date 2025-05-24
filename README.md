# Voice Input

Rust 製の **音声録音・文字起こし CLI / デーモン** です。
`voice_input` はクライアント CLI、`voice_inputd` はバックグラウンド常駐デーモンとして動作します。

[CLI] → [/tmp/voice_input.sock] → [voice_inputd] → (録音 / 転写 / クリップボード)

## 特徴

| 機能                       | 説明                                  |
| -------------------------- | ------------------------------------- |
| **高速録音トグル**         | 1 コマンドで録音開始 / 停止を切替     |
| **OpenAI API 対応**        | 日本語・英語を自動認識                |
| **Apple Music 自動ポーズ/再開** | 録音中は BGM を一時停止、終了後に自動再生 |
| **単語リスト置換**         | 転写テキストを辞書で自動置換            |
| **録音→転写まで自動**      | 1 コマンドで録音開始から文字起こしまで |
| **直接テキスト入力**       | クリップボードを汚染せずにカーソル位置に直接入力 |
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

## 直接テキスト入力

従来の⌘Vによるペースト方式に加えて、クリップボードを汚染せずにカーソル位置に直接テキストを入力するモードが利用できます。

```sh
# 直接入力モード（クリップボードを汚染しない）
voice_input start --paste --direct-input
voice_input toggle --paste --direct-input

# 明示的にペースト方式を使用
voice_input start --paste --no-direct-input

# デフォルト（ペースト方式）
voice_input start --paste
```

**直接入力の特徴:**
- ✅ クリップボードの内容を保持
- ✅ 日本語・絵文字を含むすべての文字に対応
- ✅ 既存のアクセシビリティ権限で動作
- ✅ ペースト方式より約85%高速（平均: 0.02秒 vs 0.15秒）

デーモンと外部依存の状態をまとめて確認:

```sh
voice_input health
```

## 辞書による結果置換

転写されたテキストは、ユーザー定義の辞書を通して自動的に置換されます。
辞書は JSON 形式で `~/Library/Application Support/voice_input/dictionary.json` に保存され、
CLI から編集できます。

保存先を変更したい場合は次のコマンドを実行してください。設定は同ディレクトリの
`config.json` に記録され、変更時には旧ファイルが `<旧パス>.bak` として残ります。

```sh
voice_input config set dict-path /path/to/shared/dictionary.json
```

```sh
# 単語登録または更新
voice_input dict add "誤変換" "正しい語"

# 単語削除
voice_input dict remove "誤変換"

# 登録一覧表示
voice_input dict list
```

## 録音から転写までの一括実行

`voice_input start` / `stop` を明示的に使わなくても、
`voice_input toggle` 1 回で録音開始→停止→文字起こし→クリップボード保存まで
完結します。`--paste` を付ければ自動で ⌘V が送信されます。

