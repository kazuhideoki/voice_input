# カーソル位置直接テキスト挿入：Enigoアプローチ

## 重要：実装方針変更

当初はAppleScript keystrokeアプローチで実装を進めていましたが、テスト中にAppleScriptの`keystroke`命令が**非ASCII文字（日本語、絵文字など）をサポートしていない**ことが判明しました。

そのため、**Enigoライブラリ（CGEventPostベース）**に切り替え、日本語を含むすべての文字を正しく入力できるようにしました。

## 概要

音声認識結果をコピー&ペーストではなく、カーソル位置に直接入力する方法の調査・設計・実装。

## 現在の問題点

現在の実装（`src/bin/voice_inputd.rs:372-376`）：

```rust
let _ = tokio::process::Command::new("osascript")
    .arg("-e")
    .arg(r#"tell app "System Events" to keystroke "v" using {command down}"#)
    .output()
    .await;
```

**問題：**

- クリップボードの汚染（元の内容が失われる）
- ⌘V操作はクリップボード全体を対象とする

## 解決策：Enigoライブラリを使用した直接入力

### 実装方針

Enigoライブラリ（macOSではCGEventPost APIを使用）を使用してテキストを直接入力します。

**メリット：**

- ✅ クリップボードを使わない
- ✅ 日本語・特殊文字・絵文字完全対応
- ✅ アプリケーション非依存
- ✅ クロスプラットフォーム対応可能
- ✅ アクティブに開発されている

**デメリット：**

- ⚠️ 外部依存の追加
- ⚠️ バイナリサイズの若干の増加

### 技術実装

#### 1. Enigoライブラリの統合

```rust
// src/infrastructure/external/text_input_enigo.rs
use enigo::{Enigo, Settings, Keyboard};

pub async fn type_text_with_enigo(text: &str) -> Result<(), EnigoInputError> {
    let text_owned = text.to_string();
    
    tokio::task::spawn_blocking(move || {
        let mut enigo = Enigo::new(&Settings::default())?;
        enigo.text(&text_owned)?;
        Ok(())
    }).await?
}
```

#### 2. シンプルなAPI

```rust
// src/infrastructure/external/text_input.rs
pub async fn type_text(text: &str) -> Result<(), TextInputError> {
    // Enigoを使用して日本語を含むすべてのテキストを入力
    text_input_enigo::type_text_default(text)
        .await
        .map_err(|e| TextInputError::AppleScriptFailure(e.to_string()))
}
```

#### 3. voice_inputd.rs での統合

```rust
// handle_transcription関数内の修正
if paste {
    tokio::time::sleep(Duration::from_millis(80)).await;

    if direct_input {
        // 新しい直接入力方式
        if let Err(e) = type_text_directly(&replaced).await {
            eprintln!("Direct input failed: {}, falling back to paste", e);
            // フォールバック: 既存のペースト方式
            let _ = tokio::process::Command::new("osascript")
                .arg("-e")
                .arg(r#"tell app "System Events" to keystroke "v" using {command down}"#)
                .output()
                .await;
        }
    } else {
        // 既存のペースト方式
        let _ = tokio::process::Command::new("osascript")
            .arg("-e")
            .arg(r#"tell app "System Events" to keystroke "v" using {command down}"#)
            .output()
            .await;
    }
}
```

### CLI拡張

#### IpcCmd拡張

```rust
// src/ipc.rs
#[derive(Serialize, Deserialize, Debug)]
pub enum IpcCmd {
    Start {
        paste: bool,
        prompt: Option<String>,
        direct_input: bool, // 新しいフラグ
    },
    // ... 他のコマンドも同様に拡張
}
```

#### CLI引数拡張

```rust
// src/main.rs
#[derive(Subcommand)]
enum Cmd {
    Start {
        #[arg(long, default_value_t = false)]
        paste: bool,
        #[arg(long)]
        prompt: Option<String>,
        #[arg(long, default_value_t = false)]
        direct_input: bool, // 新しいフラグ
    },
    // ... 他のコマンドも同様
}
```

## 段階的実装計画

### Phase 1: 基本実装

1. ✅ 設計文書作成
2. ✅ AppleScript keystroke関数実装 (P1-1完了)
3. ✅ voice_inputd.rsへの統合 (P1-3完了)
4. ✅ 基本テスト (P1-1完了)

### Phase 2: CLI拡張

1. ✅ IpcCmd構造体拡張 (P1-2完了)
2. ✅ voice_inputd統合 (P1-3完了)
3. ✅ CLI引数追加 (P1-4完了)
4. ✅ エンドツーエンドテスト (P1-5完了)

### Phase 3: 最適化

1. ⏳ パフォーマンステスト
2. ⏳ エラーハンドリング改善
3. ⏳ 長文分割最適化

## テスト計画

### 基本動作テスト

- [x] 短いテキスト（1-5語）(P1-1完了)
- [x] 中程度のテキスト（1-3文）(P1-1完了)
- [x] 長いテキスト（段落レベル）(P1-1完了)
- [x] 特殊文字（記号、絵文字）(P1-1完了)
- [x] 改行を含むテキスト (P1-1完了)

### アプリケーション互換性テスト

- [ ] VS Code
- [ ] TextEdit
- [ ] Safari（フォーム入力）
- [ ] Chrome（フォーム入力）
- [ ] Terminal
- [ ] Messages
- [ ] Notes

### パフォーマンステスト

- [x] 入力遅延測定 (P1-1完了)
- [x] 長文入力時間測定 (P1-1完了)
- [x] リソース使用量確認 (P1-1完了)

## 設定オプション

将来的にAppConfigで制御可能にする設定：

```rust
pub struct AppConfig {
    // 既存設定...

    /// デフォルトで直接入力を使用するか
    pub use_direct_input_by_default: bool,

    /// 直接入力失敗時にペーストにフォールバックするか
    pub fallback_to_paste: bool,
}
```

## 既知の制限事項

1. **アプリケーション固有の挙動**

   - 一部のアプリで直接入力が期待通りに動作しない可能性
   - フォールバック機能で対応

2. **アクセシビリティ権限**
   - System Eventsの使用にはアクセシビリティ権限が必要（既存と同じ）


## 実装状況

### P1-1: テキスト直接入力コアモジュール (✅ 完了)

**実装ファイル:**
- `src/infrastructure/external/text_input.rs` - コアモジュール実装
- `examples/text_input_demo.rs` - 動作デモ
- `examples/text_input_performance.rs` - パフォーマンステスト
- `examples/security_test.rs` - セキュリティテスト

**実装内容:**
- エスケープ関数 (`escape_for_applescript`)
- 直接入力関数 (`type_text_directly`, `type_text`)
- 設定バリデーション (`validate_config`)
- エラー型定義 (`TextInputError`)
- 包括的なテストスイート

## 段階的実装計画（プルリクエスト最適化）

### P1-1: テキスト直接入力コアモジュール ✅ 完了

**範囲:** 基本的なkeystroke機能実装
**ファイル:** `src/infrastructure/external/text_input.rs`（新規）

**実装内容:**

```rust
// 実装済みのAPI
pub async fn type_text_directly(text: &str, config: &TextInputConfig) -> Result<(), TextInputError>
pub async fn type_text(text: &str) -> Result<(), TextInputError>
pub fn validate_config(config: &TextInputConfig) -> Result<(), TextInputError>
fn escape_for_applescript(text: &str) -> Result<String, TextInputError>
```

**PR要件:**

- [x] 単体テスト実装
- [x] 文字数制限対応（200文字でチャンク分割）
- [x] エラーハンドリング（TextInputError型定義）
- [x] ドキュメントコメント

### P1-2: IPC拡張（direct_inputフラグ）✅ 完了

**範囲:** 内部通信にdirect_inputオプション追加
**ファイル:** `src/ipc.rs`

**変更内容:**

```rust
#[derive(Serialize, Deserialize, Debug)]
pub enum IpcCmd {
    Start {
        paste: bool,
        prompt: Option<String>,
        direct_input: bool,  // 追加
    },
    Toggle {
        paste: bool,
        prompt: Option<String>,
        direct_input: bool,  // 追加
    },
    // 他は変更なし
}
```

**PR要件:**

- [x] シリアライゼーションテスト（tests/ipc_serialization_test.rs）
- [x] 後方互換性確認（tests/ipc_compatibility_test.rs）

### P1-3: voice_inputd統合 ✅ 完了

**範囲:** デーモンプロセスでの直接入力実装
**ファイル:** `src/bin/voice_inputd.rs`

**変更内容:**

- `handle_transcription`関数にdirect_inputパラメータ追加
- 直接入力とペーストの分岐処理
- フォールバック機能

**実装例:**
```rust
use voice_input::infrastructure::external::text_input;

// handle_transcription関数内
if paste {
    if direct_input {
        match text_input::type_text(&replaced).await {
            Ok(_) => {},
            Err(e) => {
                eprintln!("Direct input failed: {}, falling back to paste", e);
                // 既存のペースト処理へフォールバック
            }
        }
    } else {
        // 既存のペースト処理
    }
}
```

**PR要件:**

- [x] 既存ペースト機能の保持
- [x] エラー時の適切なフォールバック
- [x] 統合テスト

### P1-4: CLI引数拡張 ✅ 完了

**範囲:** ユーザーインターフェース拡張
**ファイル:** `src/main.rs`

**実装内容:**

- `--direct-input`: 直接入力使用（将来的にデフォルト化を検討）
- `--no-direct-input`: 明示的にペースト方式使用
- `resolve_direct_input_flag`関数でフラグ競合チェック

**動作:**

```bash
# デフォルト（現在はペースト方式）
voice_input start --paste

# 明示的に直接入力
voice_input start --paste --direct-input

# 従来のペースト方式を明示的に使用
voice_input start --paste --no-direct-input

# 競合時はエラー
voice_input start --paste --direct-input --no-direct-input  
# Error: "Cannot specify both --direct-input and --no-direct-input"
```

**PR要件:**

- [x] 引数競合チェック
- [x] ヘルプテキスト更新
- [x] CLIテスト（tests/cli_args_test.rs）
- [x] エンドツーエンドテスト（tests/e2e_direct_input_test.rs）

### P1-5: モジュール統合・テスト ✅ 完了

**範囲:** 全体統合とテスト強化
**ファイル:** `tests/integration_test.rs`, `tests/voice_inputd_direct_input_test.rs`, `tests/performance_test.rs`

**実装内容:**

- text_inputモジュールのexport（既に完了済みを確認）
- 統合テスト実装（4個）
- voice_inputd統合テスト実装（6個）
- パフォーマンステスト実装（2個）

**PR要件:**

- [x] モジュール公開設定
- [x] 統合テスト実装
- [x] パフォーマンス比較機能
- [x] ドキュメント作成（p1-5-handover.md）

## 各PRの依存関係

```
P1-1 (コアモジュール)
  ↓
P1-2 (IPC拡張) ← P1-3 (voice_inputd統合)
  ↓                ↓
P1-4 (CLI拡張) ←----┘
  ↓
P1-5 (統合テスト)
```

**並行作業可能:** P1-2とP1-3は同時作業可能

## エラーハンドリング方針

プロジェクトではanyhowクレートを使用せず、以下のパターンでエラーハンドリングを行います：

- **外部ライブラリとの境界**: `Result<T, Box<dyn std::error::Error>>`
- **内部API**: 必要に応じて独自のエラー型を定義
- **文字列エラー**: 簡単なケースでは`&'static str`や`String`

**参考実装**: `src/infrastructure/external/openai.rs:32`

## 次のステップ

1. ✅ 段階的実装計画完成
2. ✅ エラーハンドリング方針確認
3. 🔄 keystroke制限テスト実行（推奨）
4. ⏳ P1-1から順次実装開始

このアプローチにより、適切なPRサイズで段階的に機能を実装できます。
