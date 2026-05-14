# Voice Input

Rust 製の **音声録音・文字起こし CLI / デーモン** です。
`voice_input` はクライアント CLI、`voice_inputd` はバックグラウンド常駐デーモンとして動作します。

[CLI] → [/tmp/voice_input.sock] → [voice_inputd] → (録音 / 転写 / 直接入力)

## 特徴

| 機能                               | 説明                                                   |
| ---------------------------------- | ------------------------------------------------------ |
| **高速録音トグル**                 | 1 コマンドで録音開始 / 停止を切替                      |
| **Push-to-talk**                   | ホットキーを押している間だけ録音                       |
| **複数転写バックエンド**           | OpenAI 4o / Realtime Whisper / `mlx-qwen3-asr` に対応   |
| **Pre-roll**                       | 録音開始直前の音声を短く先頭へ付与                     |
| **直接テキスト入力（デフォルト）** | カーソル位置へ直接入力                                 |
| **Apple Music 自動ポーズ/再開**    | 録音中だけ再生を止め、終了後に戻す                     |
| **単語リスト置換**                 | 転写テキストを辞書で自動置換                           |
| **履歴表示**                       | daemon 起動中の確定転写を `voice_input history` で確認  |
| **低信頼箇所の選択**               | logprobs から信頼度の低い範囲を選択可能                |
| **デバッグ入力/保存**              | WAV 入力と録音 WAV 保存に対応                           |
| **IPC Unix Socket**                | CLI ↔ デーモン間通信は JSON over UDS                    |
| **高速メモリ処理**                 | OpenAI 系の通常録音処理はメモリ上で完結                |

## 環境変数準備

```sh
cp .env.example .env
```

主な設定:

```sh
TRANSCRIPTION_API_KEY=your_openai_api_key_here # OpenAI 系利用時のみ
VOICE_INPUT_DEFAULT_TRANSCRIPTION_PROVIDER=realtime-whisper
OPENAI_TRANSCRIBE_STREAMING=false
VOICE_INPUT_LOW_CONFIDENCE_SELECTION=false
VOICE_INPUT_MAX_SECS=30
VOICE_INPUT_PRE_ROLL_MS=500
VOICE_INPUT_RECORDING_SOUNDS=true
INPUT_DEVICE_PRIORITY="device1,device2,device3"
VOICE_INPUT_PUSH_TO_TALK=true
VOICE_INPUT_PUSH_TO_TALK_HOTKEY=opt+8
```

必要に応じて `VOICE_INPUT_ENV_PATH`、`VOICE_INPUT_SOCKET_PATH`、`VOICE_INPUT_SOCKET_DIR`、`XDG_DATA_HOME` も指定できます。

`.env` はデフォルトでカレントディレクトリから読み込まれ、`VOICE_INPUT_ENV_PATH` が設定されている場合はそのパスが優先されます。
環境変数は `src/utils/config.rs` の `EnvConfig` で起動時に一度だけ読み込まれます。
転写バックエンドは `4o`、`realtime-whisper`、`mlx-qwen3-asr` から選べます。既定値は `VOICE_INPUT_DEFAULT_TRANSCRIPTION_PROVIDER`、コマンドごとの上書きは `--transcription-provider` です。
`4o` は `--transcription-model gpt-4o-mini-transcribe` も指定できます。`realtime-whisper` は録音中に逐次入力し、`mlx-qwen3-asr` はローカルコマンドを使います。
`VOICE_INPUT_PRE_ROLL_MS` は録音開始直前の音声を先頭へ付与する長さです。既定値は 500ms、0 で無効です。
`VOICE_INPUT_RECORDING_SOUNDS=false` を設定すると、録音開始・停止時の効果音を無効化できます。未指定時は有効です。
`VOICE_INPUT_PUSH_TO_TALK=true` の場合、`VOICE_INPUT_PUSH_TO_TALK_HOTKEY` を押している間だけ録音します。既定は `opt+8` です。
`VOICE_INPUT_LOW_CONFIDENCE_SELECTION=true` にすると、logprobs が得られる転写では低信頼箇所を選択します。

## 音声処理

OpenAI 系の通常録音処理は音声データをメモリ上で直接処理します。
`mlx-qwen3-asr` 連携では CLI へ渡すため一時音声ファイルを作成し、処理後に削除します。

**OpenAI 系での利点:**
- 高速処理（ファイル I/O の削除）
- ディスク容量を消費しない
- セキュリティ向上（一時ファイルが残らない）
- SSD の書き込み回数を削減

**メモリ使用量の目安:**
- 1 分間の録音: 約 10MB
- 5 分間の録音: 約 50MB
- 10 分間の録音: 約 100MB

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
   - `VOICE_INPUT_PUSH_TO_TALK=true` を使う場合は、入力監視でも `VoiceInput.app` を有効化

3. **権限反映後の再起動**

```sh
./scripts/restart-app-bundle.sh
```

`restart-app-bundle.sh` は再ビルドや再署名を行わず、権限付与の反映に必要な再起動だけを実行します。
`cleanup-app-bundle.sh` は bundle を削除したうえで、bundle identifier に対して `Microphone` / `Accessibility` / `Input Monitoring` の TCC 設定を reset します。

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
- push-to-talk の権限不足など起動時に利用者対応が必要な失敗は、再起動ループを避けるためログを出して停止します
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

## 使い方

録音開始 / 停止 / トグル:

```sh
voice_input start
voice_input stop
voice_input toggle --prompt "固有名詞の補助プロンプト"
```

最大録音時間はデフォルト 30 秒です。コマンドごとに変える場合は `--max-secs` を指定します。

```sh
voice_input start --max-secs 120
voice_input toggle --max-secs 90
```

常に変更したい場合は CLI とデーモンの両方に `VOICE_INPUT_MAX_SECS` を設定してください。

転写バックエンドを一時的に変える場合:

```sh
voice_input start --transcription-provider 4o
voice_input toggle --transcription-provider realtime-whisper
voice_input start --transcription-provider 4o --transcription-model gpt-4o-mini-transcribe
```

デバッグ用に WAV を入力したり、録音後の音声を保存できます。

```sh
voice_input start --input-file /path/to/input.wav
voice_input start --save-audio /tmp/voice-input-debug.wav
```

利用可能な入力デバイスを一覧表示:

```sh
voice_input --list-devices
```

入力デバイス名とインデックスを表示します。環境変数 `INPUT_DEVICE_PRIORITY` を
設定する際の参考にしてください。

確定転写の履歴を表示:

```sh
voice_input history
```

キーを押している間だけ録音する場合は daemon 側の環境変数で有効化します。

```sh
VOICE_INPUT_DEFAULT_TRANSCRIPTION_PROVIDER=realtime-whisper
VOICE_INPUT_PUSH_TO_TALK=true
VOICE_INPUT_PUSH_TO_TALK_HOTKEY=opt+8
```

`VOICE_INPUT_PUSH_TO_TALK_HOTKEY` は `opt+8`、`cmd+space`、`ctrl+shift+v`、`fn+f8` のような `modifier+key` 形式を受け付けます。キーボード配列差分で通常表記が合わない場合は `opt+keycode:28` のように raw keycode も指定できます。

## テキスト入力方式

現在の voice_input は **直接入力方式のみ**を提供しています。

```sh
# デフォルト動作（直接入力）
voice_input start
voice_input toggle
voice_input start --prompt "会議メモ。人名は英字優先"
```

**直接入力の特徴:**

- クリップボードの内容を保持
- 日本語・絵文字を含むすべての文字に対応
- 既存のアクセシビリティ権限で動作
- 手動ペーストが不要

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
旧形式の辞書ファイルは読み込み時に自動で現行形式へ移行され、移行前の内容は
`dictionary.json.v1.bak` として残ります。

保存先を変更したい場合は次のコマンドを実行してください。設定は同ディレクトリの
`config.json` に記録され、変更時には旧ファイルが `<旧パス>.bak` として残ります。

```sh
voice_input config set dict-path /path/to/shared/dictionary.json
```

```sh
# 対象語句へ変換する候補を登録
voice_input dict add "OpenAI" "オープンAI"

# 対象語句を削除
voice_input dict remove-term "OpenAI"

# 対象語句から候補を削除
voice_input dict remove-variant "OpenAI" "オープンAI"

# 登録一覧表示
voice_input dict list
```

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
