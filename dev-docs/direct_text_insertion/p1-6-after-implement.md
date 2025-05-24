# P1-6: 直接テキスト入力機能の実装後処理

## Why

### 背景
P1-1〜P1-5で計画されたAppleScript keystrokeベースの実装を進めていたが、途中で以下の問題が発覚した：
- AppleScriptのkeystroke命令は**非ASCII文字（日本語、絵文字など）をサポートしていない**
- 日本語テキストを入力すると、文字化けして「aaa...」のような文字の繰り返しになる

この問題を解決するため、急遽Enigoライブラリ（CGEventPostベース）を導入し、正常に日本語入力が可能になった。

### 目的
- AppleScriptベースの実装コードを削除し、Enigoベースの実装に統一する
- 不要になったコード、設定、ドキュメントを整理する
- コードベースをクリーンな状態に保つ
- 今後のメンテナンス性を向上させる

## What

### 削除対象の特定

#### 1. 不要なコード（モジュール・関数）
- **src/infrastructure/external/text_input.rs**
  - `escape_for_applescript()` - AppleScript文字列エスケープ関数
  - `execute_keystroke()` - AppleScriptでキーストローク実行
  - `execute_chunked_keystroke()` - 分割キーストローク実行
  - `type_text_directly()` - AppleScriptベースの直接入力（使用されていない）
  - `TextInputConfig` - チャンク分割設定（Enigoでは不要）
  - `validate_config()` - 設定検証関数
  - 関連するテスト（escape_tests、一部のintegration_tests）

#### 2. 不要なエラー型
- **TextInputError**
  - `EscapeError` - エスケープエラー（Enigoでは不要）
  - `Timeout` - タイムアウトエラー（Enigoでは不要）
  - `InvalidInput` の一部メッセージ

#### 3. 不要なドキュメント記述
- **dev-docs/direct_text_insertion.md**
  - AppleScript keystrokeアプローチの詳細説明
  - エスケープ関数の説明
  - 分割送信の説明
  - AppleScript文字数制限の説明
  - チャンク設定の説明

#### 4. 不要なテスト
- **examples/text_input_demo.rs** - AppleScript用のデモ（Enigo版に置き換え）
- **examples/text_input_performance.rs** - AppleScript用のパフォーマンステスト

### 保持すべきコード

#### 1. Enigoベースの実装
- **src/infrastructure/external/text_input_enigo.rs** - 全体を保持
- **src/infrastructure/external/text_input.rs**
  - `type_text()` - Enigoを呼び出す関数（保持）
  - `TextInputError` の基本型定義（簡略化して保持）

#### 2. 統合部分
- **src/bin/voice_inputd.rs** - 現在の実装を保持
- **src/main.rs** - CLI引数処理を保持
- **src/ipc.rs** - direct_inputフラグを保持

#### 3. テスト
- **tests/integration_test.rs** - 保持
- **tests/voice_inputd_direct_input_test.rs** - 保持（TextInputConfig削除）
- **tests/performance_test.rs** - 保持（TextInputConfig削除）
- **tests/cli_args_test.rs** - 保持
- **tests/e2e_direct_input_test.rs** - 保持

## How

### 削除・修正手順（インクリメンタル）

#### Step 1: text_input.rsの簡略化
**目的**: AppleScript関連コードを削除し、Enigoラッパーのみに簡略化

1. 以下を削除：
   - `escape_for_applescript()`関数と関連テスト
   - `execute_keystroke()`関数
   - `execute_chunked_keystroke()`関数
   - `type_text_directly()`関数
   - `TextInputConfig`構造体
   - `validate_config()`関数
   - `TextInputError`のうち不要なバリアント（EscapeError, Timeout）

2. `type_text()`関数を保持（現在のEnigo呼び出し版）

3. 検証：
```bash
cargo check
cargo clippy
cargo test
```

#### Step 2: テストファイルの修正
**目的**: TextInputConfigに依存するテストを修正

1. **tests/voice_inputd_direct_input_test.rs**
   - `TextInputConfig`のインポートを削除
   - モック設定部分を削除または簡略化

2. **tests/performance_test.rs**
   - `TextInputConfig`のインポートを削除
   - 直接`type_text()`を呼び出すように修正

3. 検証：
```bash
cargo test --test voice_inputd_direct_input_test
cargo test --test performance_test -- --ignored --nocapture
```

#### Step 3: examplesの削除
**目的**: AppleScript用のサンプルコードを削除

1. 削除：
```bash
rm examples/text_input_demo.rs
rm examples/text_input_performance.rs
rm examples/security_test.rs
```

2. 検証：
```bash
cargo check
```

#### Step 4: ドキュメントの更新
**目的**: AppleScriptアプローチの記述を削除・更新

1. **dev-docs/direct_text_insertion.md**
   - 冒頭に「実装方針変更」セクションを追加
   - AppleScript関連の詳細説明をコメントアウトまたは削除
   - Enigoベースの実装に関する説明を追加

2. **README.md**
   - 直接入力機能の説明を確認（変更不要の可能性）

3. **CLAUDE.md**
   - ビルド・テストコマンドを確認（変更不要の可能性）

#### Step 5: 最終確認
**目的**: 全体が正常に動作することを確認

1. フルビルド：
```bash
cargo clean
cargo build --release
```

2. 全テスト実行：
```bash
cargo test
```

3. Clippy実行：
```bash
cargo clippy -- -D warnings
```

4. 実機動作確認：
```bash
# デーモン起動
pkill voice_inputd
./target/release/voice_inputd

# 別ターミナルで実行
./target/release/voice_input toggle --paste --direct-input
```

### リスク管理

#### 1. 段階的検証
- 各ステップ後に`cargo check`と`cargo test`を実行
- エラーが発生したら即座に原因を特定・修正

#### 2. ロールバック計画
- 問題が発生した場合、Gitで前のコミットに戻す

### 今後の課題

#### Phase 2で実装予定
1. **設定ファイルサポート**
   - デフォルトで直接入力を使用するかの設定
   - エラー時のフォールバック設定

2. **パフォーマンス最適化**
   - Enigoの初期化コスト削減
   - 大量テキスト入力時の最適化

3. **エラーハンドリング改善**
   - より詳細なエラーメッセージ
   - ユーザーフレンドリーなエラー表示

#### 将来的な検討事項
1. **クロスプラットフォーム対応**
   - Linux/Windows版の実装（Enigoは対応済み）

2. **入力メソッド統合**
   - IME（日本語入力）との連携改善
   - 変換候補の処理

3. **セキュリティ強化**
   - 入力内容のログ制御
   - センシティブデータの扱い

## 実装メモ

### Enigoライブラリについて
- **依存関係**: `enigo = "0.2.0"`
- **内部実装**: macOSではCGEventPostを使用
- **メリット**: 
  - Unicode完全対応
  - クロスプラットフォーム
  - アクティブに開発されている
- **デメリット**:
  - 外部依存の追加
  - バイナリサイズの若干の増加

### 削除されるコード量（推定）
- text_input.rs: 約200行削減
- examples: 約300行削減
- 合計: 約500行のコード削減

これにより、コードベースがよりシンプルで保守しやすくなります。