# Voice Input

Rust 製の **音声録音・文字起こし CLI / デーモン** です。
`voice_input` はクライアント CLI、`voice_inputd` はバックグラウンド常駐デーモンとして動作します。

[CLI] → [/tmp/voice_input.sock] → [voice_inputd] → (録音 / 転写 / 直接入力)

## 特徴

| 機能                               | 説明                                                   |
| ---------------------------------- | ------------------------------------------------------ |
| **高速録音トグル**                 | 1 コマンドで録音開始 / 停止を切替                      |
| **Push-to-talk**                   | ホットキーを押している間だけ録音                       |
| **複数転写バックエンド**           | GPT Transcribe / GPT Live Transcribe / `mlx-qwen3-asr`  |
| **Pre-roll**                       | 録音開始直前の音声を短く先頭へ付与                     |
| **直接テキスト入力（デフォルト）** | カーソル位置へ直接入力                                 |
| **Apple Music 自動ポーズ/再開**    | 録音中だけ再生を止め、終了後に戻す                     |
| **単語リスト置換**                 | 転写テキストを辞書で自動置換                           |
| **履歴表示**                       | daemon 起動中の確定転写を `voice_input history` で確認  |
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
VOICE_INPUT_DEFAULT_TRANSCRIPTION_PROVIDER=gpt-transcribe
VOICE_INPUT_DEFAULT_TRANSCRIBE_STREAMING=false
VOICE_INPUT_DEFAULT_MAX_SECS=30
VOICE_INPUT_DEFAULT_PRE_ROLL_MS=500
VOICE_INPUT_DEFAULT_RECORDING_SOUNDS_ENABLED=true
VOICE_INPUT_DEFAULT_RECORDING_HUD_ENABLED=true
VOICE_INPUT_DEFAULT_INPUT_DEVICE_PRIORITIES="device1,device2,device3"
VOICE_INPUT_DEFAULT_PUSH_TO_TALK_ENABLED=true
VOICE_INPUT_DEFAULT_PUSH_TO_TALK_HOTKEY=opt+8
```

必要に応じて `VOICE_INPUT_ENV_PATH`、`VOICE_INPUT_SOCKET_PATH`、`VOICE_INPUT_SOCKET_DIR`、`XDG_DATA_HOME` も指定できます。

`.env` はデフォルトでカレントディレクトリから読み込まれ、`VOICE_INPUT_ENV_PATH` が設定されている場合はそのパスが優先されます。
環境変数は `src/utils/config.rs` の `EnvConfig` で起動時に一度だけ読み込まれます。APIキー、Proxy、IPC・データ配置などの起動環境と、`VOICE_INPUT_DEFAULT_*` 形式のユーザー設定既定値を保持します。
転写バックエンドは `gpt-transcribe`、`gpt-live-transcribe`、`mlx-qwen3-asr` から選べます。既定値は `VOICE_INPUT_DEFAULT_TRANSCRIPTION_PROVIDER`、コマンドごとの上書きは `--transcription-provider` です。
ユーザーが変更する設定はビルド後に `voice_input config` で `config.json` へ永続化します。優先順位は、コマンドごとの指定、`config.json`、`.env` の `VOICE_INPUT_DEFAULT_*`、プログラム内の既定値の順です。
macOSでは `config.json` を `~/Library/Application Support/com.user.voice_input/config.json` に保存します。このパスがシンボリックリンクの場合はリンクを維持したままリンク先を更新するため、dotfilesで管理できます。
OpenAI 系のモデルは provider ごとに固定されます。`gpt-transcribe` は録音後の音声を GPT Transcribe へ送り、`gpt-live-transcribe` は GPT Live Transcribe で録音中に逐次入力します。`mlx-qwen3-asr` はローカルコマンドを使います。
`VOICE_INPUT_DEFAULT_PRE_ROLL_MS` は録音開始直前の音声を先頭へ付与する長さです。既定値は 500ms、0 で無効です。
`VOICE_INPUT_DEFAULT_RECORDING_SOUNDS_ENABLED=false` を設定すると、録音開始・停止時の効果音を無効化できます。未指定時は有効です。
`VOICE_INPUT_DEFAULT_PUSH_TO_TALK_ENABLED=true` の場合、`VOICE_INPUT_DEFAULT_PUSH_TO_TALK_HOTKEY` を押している間だけ録音します。既定は `opt+8` です。

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
   - `VOICE_INPUT_DEFAULT_PUSH_TO_TALK_ENABLED=true` を使う場合は、入力監視でも `VoiceInput.app` を有効化

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
voice_input toggle
```

最大録音時間はデフォルト 30 秒です。コマンドごとに変える場合は `--max-secs` を指定します。

```sh
voice_input start --max-secs 120
voice_input toggle --max-secs 90
```

既定値を永続変更する場合:

```sh
voice_input config set max-secs 120
voice_input config get max-secs
voice_input config unset max-secs
```

転写バックエンドを一時的に変える場合:

```sh
voice_input start --transcription-provider gpt-transcribe
voice_input toggle --transcription-provider gpt-live-transcribe
voice_input start --transcription-provider mlx-qwen3-asr
```

既定の転写バックエンドを永続変更する場合:

```sh
voice_input config set transcription-provider gpt-live-transcribe
voice_input config get transcription-provider
voice_input config unset transcription-provider
```

pre-roll長を永続変更する場合:

```sh
voice_input config set pre-roll-ms 250
voice_input config get pre-roll-ms
voice_input config unset pre-roll-ms
```

設定可能な全項目の現在値を表示する場合:

```sh
voice_input config show
```

```text
dict-path=/path/to/dictionary.json
transcription-provider=gpt-live-transcribe
max-secs=120
pre-roll-ms=250
input-device-priorities=External Mic,MacBook Microphone
recording-sounds-enabled=false
recording-hud-enabled=true
push-to-talk-enabled=true
push-to-talk-hotkey=opt+8
transcribe-streaming=false
```

そのほかのユーザー設定も同じ形式で変更できます。

```sh
voice_input config set input-device-priorities "External Mic" "MacBook Microphone"
voice_input config set recording-sounds-enabled false
voice_input config set recording-hud-enabled true
voice_input config set push-to-talk-enabled true
voice_input config set push-to-talk-hotkey opt+8
voice_input config set transcribe-streaming false
```

`unset` 後は対応する `.env` の `VOICE_INPUT_DEFAULT_*` へ戻ります。永続設定の変更後、LaunchAgentで動作しているデーモンは自動再起動して全設定を一貫して反映します。録音または転写処理中の場合は、すべて完了するまで再起動を待ちます。デーモンを手動起動している場合は、設定変更後に起動し直してください。

デバッグ用に WAV を入力したり、録音後の音声を保存できます。

```sh
voice_input start --input-file /path/to/input.wav
voice_input start --save-audio /tmp/voice-input-debug.wav
```

利用可能な入力デバイスを一覧表示:

```sh
voice_input --list-devices
```

入力デバイス名とインデックスを表示します。`input-device-priorities` を設定する際の参考にしてください。

確定転写の履歴を表示:

```sh
voice_input history
```

キーを押している間だけ録音する場合は永続設定で有効化します。

```sh
voice_input config set transcription-provider gpt-transcribe
voice_input config set push-to-talk-enabled true
voice_input config set push-to-talk-hotkey opt+8
```

`push-to-talk-hotkey` は `opt+8`、`cmd+space`、`ctrl+shift+v`、`fn+f8` のような `modifier+key` 形式を受け付けます。キーボード配列差分で通常表記が合わない場合は `opt+keycode:28` のように raw keycode も指定できます。

## テキスト入力方式

現在の voice_input は **直接入力方式のみ**を提供しています。

```sh
# デフォルト動作（直接入力）
voice_input start
voice_input toggle
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
辞書は JSON 形式で `~/Library/Application Support/com.user.voice_input/dictionary.json` に保存され、
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

## License

MIT
