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
| **直接テキスト入力（デフォルト）** | クリップボードを汚染せずにカーソル位置に直接入力 |
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

## macOS での権限設定

### 初回セットアップ

1. **開発環境セットアップ（ラッパースクリプト方式）**
   ```sh
   ./scripts/setup-dev-env.sh
   ```
   このスクリプトは以下を自動で行います：
   - `/usr/local/bin/voice_inputd_wrapper` にラッパースクリプトを作成
   - LaunchAgentをラッパー経由で起動するよう設定
   - デーモンを再起動

2. **権限の付与**
   - システム設定 → プライバシーとセキュリティ → アクセシビリティ
   - 以下を追加して有効化：
     - **使用中のターミナル**（Terminal.app、iTerm2など）
     - `/usr/local/bin/voice_inputd_wrapper`

### 開発時の再ビルド

ラッパースクリプト方式により、再ビルド時の権限再設定が不要になりました：

```sh
./scripts/dev-build.sh
```

これだけで：
- リリースビルドを実行
- デーモンを自動的に再起動
- **権限の再設定は不要**

### 仕組み

macOSのTCCシステムは実行ファイルのハッシュ値で権限を管理するため、再ビルドすると権限が失われます。
ラッパースクリプト方式では：

1. 変更されないラッパースクリプト（`/usr/local/bin/voice_inputd_wrapper`）に権限を付与
2. ラッパーが実際のバイナリ（`target/release/voice_inputd`）を実行
3. 再ビルドしてもラッパーのハッシュ値は変わらないため、権限が維持される

### トラブルシューティング

権限関連のエラーが発生した場合：

```sh
# エラーログを確認
tail -f /tmp/voice_inputd.err

# 手動でデーモンを再起動
pkill -f voice_inputd
nohup /usr/local/bin/voice_inputd_wrapper > /tmp/voice_inputd.out 2> /tmp/voice_inputd.err &
```

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

録音開始,停止の切り替え+直接入力。

```sh
voice_input toggle
```

## テキスト入力方式

voice_inputは2つのテキスト入力方式をサポートしています。デフォルトは直接入力方式です。

### 直接入力（デフォルト）

クリップボードを汚染せずにカーソル位置に直接テキストを入力します。

```sh
# デフォルト動作（直接入力）
voice_input start
voice_input toggle
```

**直接入力の特徴:**
- ✅ クリップボードの内容を保持
- ✅ 日本語・絵文字を含むすべての文字に対応
- ✅ 既存のアクセシビリティ権限で動作
- ✅ ペースト方式より約85%高速（平均: 0.02秒 vs 0.15秒）

### クリップボード方式（オプション）

従来の⌘Vによるペースト方式を使用したい場合：

```sh
# クリップボード経由でペースト
voice_input start --copy-and-paste
voice_input toggle --copy-and-paste

# クリップボードにコピーのみ（ペーストしない）
voice_input start --copy-only
voice_input toggle --copy-only
```

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
`voice_input toggle` 1 回で録音開始→停止→文字起こし→直接入力まで
完結します。デフォルトではカーソル位置に直接テキストが入力されます。

