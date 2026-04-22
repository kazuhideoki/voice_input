# Voice Input

Rust 製の **音声録音・文字起こし CLI / デーモン** です。
`voice_input` はクライアント CLI、`voice_inputd` はバックグラウンド常駐デーモンとして動作します。

[CLI] → [/tmp/voice_input.sock] → [voice_inputd] → (録音 / 転写 / 直接入力)

## 特徴

| 機能                               | 説明                                             |
| ---------------------------------- | ------------------------------------------------ |
| **高速録音トグル**                 | 1 コマンドで録音開始 / 停止を切替                |
| **複数転写バックエンド**           | OpenAI API または `mlx-qwen3-asr` を利用可能     |
| **Apple Music 自動ポーズ/再開**    | 録音中は BGM を一時停止、終了後に自動再生        |
| **単語リスト置換**                 | 転写テキストを辞書で自動置換                     |
| **録音→転写まで自動**              | 1 コマンドで録音開始から文字起こしまで           |
| **直接テキスト入力（デフォルト）** | カーソル位置に直接入力 |
| **IPC Unix Socket**                | CLI ↔ デーモン間通信は JSON over UDS            |
| **高速メモリ処理**                 | 一時ファイルを作成せず、メモリ上で音声処理       |

## 環境変数準備

```sh
cp .env.example .env
```

- TRANSCRIPTION_PROVIDER=openai # または mlx-qwen3-asr
- TRANSCRIPTION_API_KEY=your_openai_api_key_here # OpenAI 利用時のみ
- TRANSCRIPTION_MODEL=gpt-4o-mini-transcribe # OpenAI: gpt-4o-mini-transcribe / gpt-4o-transcribe, mlx: 例 Qwen/Qwen3-ASR-1.7B
- OPENAI_TRANSCRIBE_STREAMING=false
- MLX_QWEN3_ASR_COMMAND=mlx-qwen3-asr
- INPUT_DEVICE_PRIORITY="device1,device2,device3"
- VOICE_INPUT_ENV_PATH=/path/to/.env
- VOICE_INPUT_SOCKET_PATH=/custom/path/voice_input.sock
- VOICE_INPUT_SOCKET_DIR=/custom/socket/dir # `VOICE_INPUT_SOCKET_PATH` 未設定時のみ有効
- XDG_DATA_HOME=/custom/xdg/data

`.env` はデフォルトでカレントディレクトリから読み込まれ、`VOICE_INPUT_ENV_PATH` が設定されている場合はそのパスが優先されます。
環境変数は `src/utils/config.rs` の `EnvConfig` で起動時に一度だけ読み込まれます。
`TRANSCRIPTION_PROVIDER=openai` のときに `TRANSCRIPTION_MODEL` へ `whisper-1` など未対応モデルを指定した場合は、起動時にエラーになります。
`TRANSCRIPTION_PROVIDER=mlx-qwen3-asr` のときは `mlx-qwen3-asr` コマンドが必要で、録音データは CLI 連携のため一時ファイル経由で渡します。

## 音声処理

Voice Inputは音声データをメモリ上で直接処理し、一時ファイルを作成しません。

**利点:**
- ✅ 高速処理（ファイルI/Oの削除）
- ✅ ディスク容量を消費しない
- ✅ セキュリティ向上（一時ファイルが残らない）
- ✅ SSDの書き込み回数を削減

**メモリ使用量の目安:**
- 1分間の録音: 約10MB
- 5分間の録音: 約50MB
- 10分間の録音: 約100MB

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

### デプロイ方式

現在は `app bundle` 方式のみをサポートします。

- `VoiceInput.app` を構築し、その bundle 内の `voice_inputd` を LaunchAgent で起動します。
- 権限の付与対象を `VoiceInput.app` に固定し、再ビルド後も権限の再設定が発生しにくい構成です。

### 初回セットアップ

1. **app bundle 配置**

   ```sh
   ./scripts/setup-app-bundle.sh
   ./scripts/build-app-bundle.sh
   ```

   このスクリプトは個人用の開発環境を前提に絶対パスを書き込むため、リポジトリ配置先が異なる場合は中身を調整してから実行してください。
   実行すると以下を自動で行います：

   - `~/Library/LaunchAgents/com.user.voiceinputd.plist` を作成
   - LaunchAgent が `~/Applications/VoiceInput.app/Contents/MacOS/voice_inputd` を起動するよう設定
   - `.env` の読み込み先を `VOICE_INPUT_ENV_PATH` で固定

2. **権限の付与**
   - システム設定 → プライバシーとセキュリティ → マイク
   - `VoiceInput.app` を有効化
   - システム設定 → プライバシーとセキュリティ → アクセシビリティ
   - `VoiceInput.app` を有効化

3. **権限反映後の再起動**

```sh
./scripts/restart-app-bundle.sh
```

この方式では `~/Applications/VoiceInput.app` を構築し、LaunchAgent は bundle 内の `voice_inputd` を起動します。
初回は `build-app-bundle.sh` で app bundle を配置したあと、システム設定で `VoiceInput.app` に `Microphone` / `Accessibility` 権限を付与し、最後に `restart-app-bundle.sh` で LaunchAgent を再起動してください。
`restart-app-bundle.sh` は再ビルドや再署名を行わず、権限付与の反映に必要な再起動だけを実行します。
`cleanup-app-bundle.sh` は bundle を削除したうえで、bundle identifier に対して `Microphone` / `Accessibility` の TCC 設定を reset します。

### 開発時の再ビルド

app bundle を LaunchAgent で起動するため、再ビルド時の権限再設定は不要です：

```sh
./scripts/build-app-bundle.sh
```

通常はこのコマンドだけで十分です。以下をまとめて行います：

- リリースビルドを実行
- `~/Applications/VoiceInput.app` を更新
- `com.user.voiceinputd` を再起動
- **権限の再設定は不要**
- ログイン後は LaunchAgent が自動起動するため、通常は再実行不要

### 自動復旧

- macOS に再ログインした後は LaunchAgent が自動で `voice_inputd` を起動します
- `voice_inputd` が異常終了した場合は `KeepAlive` により自動で再起動されます
- 長時間スリープ後は daemon が wake を検知して音声入力ストリームとテキスト入力ワーカーの再初期化を試みます
- wake 復旧が連続で失敗した場合は daemon が終了し、LaunchAgent が再起動します

### 仕組み

macOS の TCC システムは実行ファイルや bundle identity を基準に権限を管理するため、起動対象がぶれると再ビルド後に権限が不安定になりやすくなります。
この開発環境では：

1. `VoiceInput.app` を固定の bundle identifier (`com.user.voiceinput`) で生成
2. LaunchAgent が常に bundle 内の `voice_inputd` を起動
3. wake 復帰時は内部リソースを再初期化し、回復不能ならプロセスを落として LaunchAgent に再起動させる

### トラブルシューティング

権限関連のエラーが発生した場合：

```sh
# エラーログを確認
tail -f /tmp/voice_inputd.err

# まず通常の再ビルド兼再起動を試す
./scripts/build-app-bundle.sh

# LaunchAgent を明示的に再起動
launchctl kickstart -k gui/$(id -u)/com.user.voiceinputd
```

開発環境自体を解除したい場合は、以下を実行してください。

```sh
./scripts/cleanup-app-bundle.sh
```

ビルド生成物まで消したい場合は、別途 `cargo clean` を実行してください。

## 使い方（基本）

録音開始,停止

```sh
voice_input start
voice_input stop
voice_input toggle --prompt "固有名詞の補助プロンプト"
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

現在のvoice_inputは**直接入力方式のみ**を提供しています。

```sh
# デフォルト動作（直接入力）
voice_input start
voice_input toggle
voice_input start --prompt "会議メモ。人名は英字優先"
```

**直接入力の特徴:**

- ✅ クリップボードの内容を保持
- ✅ 日本語・絵文字を含むすべての文字に対応
- ✅ 既存のアクセシビリティ権限で動作
- ✅ 直接入力のため手動ペーストが不要

デーモンと外部依存の状態をまとめて確認:

```sh
voice_input health
```

ソケット接続先を切り替えたい場合は、CLI とデーモンの両方に同じ `VOICE_INPUT_SOCKET_PATH` または
`VOICE_INPUT_SOCKET_DIR` を設定してください。

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

## 開発

### ビルドとテスト

```bash
# 開発ビルド
cargo build

# リリースビルド
cargo build --release

# すべてのテストを実行（ローカル環境）
cargo test

# 環境依存を避けるテスト（音声デバイスが不要なテストのみ）
cargo test --features ci-test

# フォーマットチェック
cargo fmt -- --check

# Lintチェック
cargo clippy --all-targets -- -D warnings
```

### パフォーマンス

メモリ処理のパフォーマンス測定はベンチマークで行えます：

```bash
# ベンチマーク実行（詳細な性能測定）
cargo bench
```

#### メモリ処理の利点

- ディスクI/Oの完全排除による高速化
- 一時ファイル作成・削除のオーバーヘッド排除
- システムコールの削減

#### ローカル品質チェック

ローカルで品質チェックを実行できます：

```bash
# 基本的な品質チェック
./scripts/quality-check.sh

# ベンチマークを含む完全チェック
./scripts/quality-check.sh --bench
```

`scripts/quality-check.sh` は `cargo fmt -- --check`、
`cargo clippy --all-targets -- -D warnings`、`cargo test` を順に実行したあと、
補助的なE2E確認をベストエフォートで流します。

### Rustバージョン管理

プロジェクトルートの `rust-toolchain.toml` により、このリポジトリで使用するRustバージョンと補助コンポーネントを固定しています：

```toml
[toolchain]
channel = "1.86.0"
components = ["rustfmt", "clippy"]
profile = "minimal"
targets = ["aarch64-apple-darwin", "x86_64-apple-darwin"]
```

### テスト戦略

- **ローカル環境**: `cargo test` ですべてのテストを実行
- **環境依存テストを避けたい場合**: `cargo test --features ci-test` で環境依存のテストをスキップ
- **無視されるテスト**: 音声デバイス、デーモンプロセス、GUI操作が必要なテスト
