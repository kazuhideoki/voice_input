# Voice Input 使用ガイド

## 🎯 クイックスタート

### 1. 初回セットアップ（5分）

```bash
# リポジトリのクローン
git clone https://github.com/yourusername/voice_input.git
cd voice_input

# 環境変数の設定
cp .env.example .env
# .envファイルを編集してOPENAI_API_KEYを設定

# ビルド
cargo build --release

# 開発環境セットアップ（権限設定を含む）
./scripts/setup-dev-env.sh
```

### 2. 基本的な使い方

#### 音声入力の開始
```bash
# 録音開始 → 停止 → 文字起こし → 入力
voice_input toggle

# または個別に操作
voice_input start  # 録音開始
voice_input stop   # 録音停止して文字起こし
```

#### スタック機能（複数テキスト管理）
```bash
# スタックモードを有効化（ショートカットキーも自動有効化）
voice_input stack on

# これで以下が使用可能に：
# - Cmd+R: 録音開始/停止
# - Cmd+1〜9: 保存したテキストをペースト
# - Cmd+C: 全スタッククリア
# - ESC: スタックモード終了
```

## 📚 詳細な使用方法

### 音声入力モード

#### 1. 直接入力モード（デフォルト・推奨）
```bash
voice_input toggle
# または
voice_input toggle --direct-input
```
- ✅ クリップボードを汚染しない
- ✅ 高速（平均22ms）
- ✅ 日本語・絵文字完全対応

#### 2. クリップボード経由モード
```bash
voice_input toggle --copy-and-paste
```
- 互換性重視の場合に使用
- 一部のアプリケーションで必要

#### 3. コピーのみモード
```bash
voice_input toggle --copy-only
```
- クリップボードにコピーのみ
- 手動でペーストする場合

### スタック機能の活用

#### 基本操作
```bash
# スタックモード有効化
voice_input stack on

# 現在のスタック確認
voice_input stack list

# 特定のスタックをペースト（CLIから）
voice_input paste 3

# 全スタッククリア
voice_input stack clear

# スタックモード無効化
voice_input stack off
```

#### ショートカットキー（スタックモード時）
| キー | 動作 |
|------|------|
| Cmd+R | 録音開始/停止（トグル） |
| Cmd+1〜9 | スタック1〜9の内容をペースト |
| Cmd+C | 全スタッククリア |
| ESC | スタックモード終了 |

### 辞書機能（自動置換）

```bash
# 単語登録
voice_input dict add "まちがい" "正しい"
voice_input dict add "VSC" "Visual Studio Code"

# 登録内容確認
voice_input dict list

# 単語削除
voice_input dict remove "まちがい"

# 辞書ファイルの場所変更
voice_input config set dict-path ~/Dropbox/voice_input_dict.json
```

### 詳細設定

#### 環境変数
```bash
# .env または export で設定

# OpenAI API設定
OPENAI_API_KEY=your_api_key_here
OPENAI_TRANSCRIBE_MODEL=whisper-1  # またはgpt-4o-mini-transcribe

# 録音設定
VOICE_INPUT_MAX_SECS=30  # 最大録音時間（秒）
INPUT_DEVICE_PRIORITY="MacBook Pro Microphone,External Microphone"

# 動作モード（移行期間のみ）
VOICE_INPUT_USE_SUBPROCESS=false  # true で旧実装使用
```

#### デバイス優先順位
```bash
# 利用可能なデバイス一覧
voice_input --list-devices

# 出力例：
# 0: MacBook Pro Microphone
# 1: AirPods Pro
# 2: External Microphone

# .envで優先順位を設定
INPUT_DEVICE_PRIORITY="AirPods Pro,MacBook Pro Microphone"
```

## 🎮 実践的な使用例

### プログラミングでの活用

```bash
# 1. エディタでコードを開く
# 2. スタックモードを有効化
voice_input stack on

# 3. よく使うコードスニペットを音声で登録
# Cmd+R → "console.log debug statement" → Cmd+R
# Cmd+R → "try catch block" → Cmd+R
# Cmd+R → "async await function" → Cmd+R

# 4. 必要な時にCmd+1, Cmd+2, Cmd+3でペースト
```

### ドキュメント作成での活用

```bash
# 長文入力に最適
voice_input toggle

# 話した内容：
# "本日の会議では、プロジェクトの進捗状況について話し合いました。
#  主な議題は以下の通りです。
#  1つ目、開発スケジュールの見直し
#  2つ目、リソース配分の最適化
#  3つ目、品質保証プロセスの改善"
```

### チャット・メッセージングでの活用

```bash
# Slack/Discord での素早い返信
voice_input stack on

# よく使う返信を事前登録
# スタック1: "確認しました、ありがとうございます"
# スタック2: "少々お待ちください"
# スタック3: "了解です！"

# Cmd+数字で瞬時に返信
```

## 🛠️ 高度な使い方

### カスタムショートカット（Raycast/Alfred連携）

```bash
#!/bin/bash
# Raycast script example
# Required parameters: None
# Optional parameters: None

# @raycast.title Voice Input Toggle
# @raycast.packageName Voice Input
# @raycast.schemaVersion 1

/usr/local/bin/voice_input toggle
```

### シェル関数の定義

```bash
# ~/.zshrc or ~/.bashrc に追加

# 音声でコミットメッセージ
vcommit() {
    voice_input toggle --copy-only
    git commit -m "$(pbpaste)"
}

# 音声でファイル名変更
vrename() {
    local oldname="$1"
    echo "新しいファイル名を話してください"
    voice_input toggle --copy-only
    mv "$oldname" "$(pbpaste)"
}

# 音声メモ
vmemo() {
    voice_input toggle --copy-only
    echo "$(date): $(pbpaste)" >> ~/voice_memos.txt
}
```

### API経由での利用（開発中）

```bash
# HTTPサーバーモード（将来実装予定）
voice_inputd --http-server --port 8080

# curl での利用例
curl -X POST http://localhost:8080/transcribe \
  -F "audio=@recording.wav" \
  -H "Authorization: Bearer YOUR_TOKEN"
```

## ❓ よくある質問（FAQ）

### 基本的な質問

**Q: 音声が認識されません**
A: 以下を確認してください：
1. マイクの接続と権限設定
2. `voice_input --list-devices`でデバイスが表示されるか
3. 環境が静かか（ノイズキャンセリング推奨）
4. OpenAI APIキーが正しく設定されているか

**Q: 日本語が文字化けします**
A: ターミナルとアプリケーションのエンコーディングをUTF-8に設定してください。

**Q: ショートカットキーが効きません**
A: アクセシビリティ権限を確認してください：
```bash
# 権限状態確認
voice_input health

# システム設定を開く
open "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility"
```

### スタック機能

**Q: スタックの保存数に制限はありますか？**
A: 現在は1-9の9個まで。古いものから上書きされます。

**Q: スタックの内容は再起動後も保持されますか？**
A: いいえ、メモリ上のみで管理されるため、デーモン再起動で消去されます。

**Q: スタックモード中に通常のCmd+数字を使いたい**
A: ESCキーまたは`voice_input stack off`でスタックモードを一時的に無効化してください。

### パフォーマンス

**Q: 入力が遅い/ラグがあります**
A: 以下を試してください：
1. 他の重いアプリケーションを終了
2. `VOICE_INPUT_USE_SUBPROCESS=false`を確認（新方式を使用）
3. より短いテキストに分割して入力

**Q: 長時間使用でメモリ使用量が増えます**
A: 定期的にデーモンを再起動してください：
```bash
pkill -f voice_inputd
nohup voice_inputd > /tmp/voice_inputd.out 2>&1 &
```

### トラブルシューティング

**Q: "Socket already in use"エラー**
A: 
```bash
rm /tmp/voice_input.sock
pkill -f voice_inputd
```

**Q: 特定のアプリでのみ動作しない**
A: APP_COMPATIBILITY.mdを確認し、そのアプリ用の回避策を試してください。

**Q: Cmd+Vとの違いは？**
A: voice_inputは直接テキストを挿入するため：
- クリップボードを汚染しない
- より高速（平均85%高速）
- 日本語入力で安定

### 開発者向け

**Q: カスタムモデルを使いたい**
A: 環境変数で指定可能：
```bash
export OPENAI_TRANSCRIBE_MODEL="custom-model-name"
```

**Q: ログを詳細に見たい**
A: 
```bash
export RUST_LOG=debug
tail -f /tmp/voice_inputd.err
```

**Q: プラグイン/拡張機能を作りたい**
A: 現在はCLI経由のみ。将来的にはgRPC APIを提供予定です。

## 📞 サポート

### 問題報告
- GitHub Issues: https://github.com/yourusername/voice_input/issues
- 必要な情報：
  - macOSバージョン
  - `voice_input health`の出力
  - エラーログ（`/tmp/voice_inputd.err`）

### コミュニティ
- Discord: [近日公開予定]
- 日本語対応: 可

### 商用利用
- ライセンス: MIT
- 商用利用: 可（ただしOpenAI API利用料は各自負担）