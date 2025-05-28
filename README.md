# Voice Input

Rust 製の **音声録音・文字起こし CLI / デーモン** です。
`voice_input` はクライアント CLI、`voice_inputd` はバックグラウンド常駐デーモンとして動作します。

[CLI] → [/tmp/voice_input.sock] → [voice_inputd] → (録音 / 転写 / クリップボード)

## 特徴

| 機能                               | 説明                                             |
| ---------------------------------- | ------------------------------------------------ |
| **高速録音トグル**                 | 1 コマンドで録音開始 / 停止を切替                |
| **OpenAI API 対応**                | 日本語・英語を自動認識                           |
| **Apple Music 自動ポーズ/再開**    | 録音中は BGM を一時停止、終了後に自動再生        |
| **単語リスト置換**                 | 転写テキストを辞書で自動置換                     |
| **録音→転写まで自動**              | 1 コマンドで録音開始から文字起こしまで           |
| **直接テキスト入力（デフォルト）** | クリップボードを汚染せずにカーソル位置に直接入力 |
| **IPC Unix Socket**                | CLI ↔ デーモン間通信は JSON over UDS            |
| **高速メモリ処理**                 | 一時ファイルを作成せず、メモリ上で音声処理       |
| **メモリ使用量監視**               | リアルタイムメモリ監視とアラート機能             |

## 環境変数準備

```sh
cp .env.example .env
```

- OPENAI_API_KEY=your_openai_api_key_here
- OPENAI_TRANSCRIBE_MODEL=gpt-4o-mini-transcribe # デフォルト
- INPUT_DEVICE_PRIORITY="device1,device2,device3"

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

## 開発

### ビルドとテスト

```bash
# 開発ビルド
cargo build

# リリースビルド
cargo build --release

# すべてのテストを実行（ローカル環境）
cargo test

# CI環境向けテスト（音声デバイスが不要なテストのみ）
cargo test --features ci-test

# フォーマットチェック
cargo fmt -- --check

# Lintチェック
cargo clippy -- -D warnings
```

### パフォーマンス

メモリ処理による高速パフォーマンスを測定できます：

```bash
# パフォーマンステストの実行
# 1. OpenAI APIキーを設定
export OPENAI_API_KEY="your_api_key_here"
export INPUT_DEVICE_PRIORITY="device1,device2,device3"

# 2. 音声デバイスの確認
cargo run --bin voice_inputd &
cargo run --bin voice_input -- --list-devices
pkill voice_inputd

# 3. テスト実行
cargo test --test performance_test -- --ignored --nocapture

# 4. ベンチマーク実行（詳細な性能測定）
cargo bench
```

#### メモリ処理の利点

- ディスクI/Oの完全排除による高速化
- 一時ファイル作成・削除のオーバーヘッド排除
- システムコールの削減
- メモリ監視によるオーバーヘッド: 1%未満

#### メモリ使用量の監視

録音中のメモリ使用量はリアルタイムで監視され、設定した閾値を超えると警告が表示されます。

```bash
# メモリ監視付きベンチマークの実行
cargo test --test benchmarks::recording_bench -- benchmark_memory_monitor_overhead --nocapture
```

### CI/CD

GitHub Actionsで自動テストが実行されます。CIでは以下が実行されます：

1. **コードフォーマットチェック** - `cargo fmt`
2. **Clippy静的解析** - すべての警告をエラーとして扱う
3. **テスト実行** - 音声デバイスやデーモンが不要なテストのみ
4. **E2Eテスト** - モック環境での統合テスト
5. **パフォーマンスベンチマーク** - 性能劣化の自動検出

#### ローカル品質チェック

CI実行前にローカルで品質チェックを実行できます：

```bash
# 基本的な品質チェック
./scripts/quality-check.sh

# ベンチマークを含む完全チェック
./scripts/quality-check.sh --bench

# メモリ監視テストを含む
./scripts/quality-check.sh --memory
```

### Rustバージョン管理

プロジェクトルートの `rust-toolchain.toml` により、ローカル環境とCI環境で同じRustバージョンが使用されます：

```toml
[toolchain]
channel = "1.86.0"
components = ["rustfmt", "clippy"]
```

これにより、開発者間およびCI環境でのビルド再現性が保証されます。

### テスト戦略

- **ローカル環境**: `cargo test` ですべてのテストを実行
- **CI環境**: `cargo test --features ci-test` で環境依存のテストをスキップ
- **無視されるテスト**: 音声デバイス、デーモンプロセス、GUI操作が必要なテスト

### エージェント向けドキュメント連携

- [CLAUDE.md](./CLAUDE.md)
- [AGENTS.md](./AGENTS.md) を参照してください。
