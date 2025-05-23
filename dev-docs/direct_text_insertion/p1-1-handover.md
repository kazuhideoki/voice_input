# P1-1 → P1-2 引き継ぎ情報

## 実装完了内容

### 作成ファイル
- `src/infrastructure/external/text_input.rs` - テキスト直接入力コアモジュール
- `examples/text_input_demo.rs` - 基本的な動作デモ
- `examples/text_input_performance.rs` - パフォーマンステスト
- `examples/security_test.rs` - セキュリティテスト
- `examples/integration_test.rs` - 統合テスト

### 実装機能
1. **エスケープ機能**: `escape_for_applescript()` - AppleScript用の文字列エスケープ
2. **直接入力機能**: `type_text_directly()` - 設定可能なテキスト入力
3. **簡易入力機能**: `type_text()` - デフォルト設定での入力
4. **設定検証**: `validate_config()` - 入力設定のバリデーション

### エラー型
- `TextInputError` - 4種類のエラーを定義
  - `AppleScriptFailure` - スクリプト実行エラー
  - `EscapeError` - エスケープ処理エラー
  - `Timeout` - タイムアウトエラー
  - `InvalidInput` - 不正入力エラー

## P1-2で必要な作業

### IpcCmd拡張
```rust
// src/ipc.rs に追加
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

### 利用方法
```rust
// voice_inputd.rs での使用例
use crate::infrastructure::external::text_input;

// handle_transcription関数内で
if paste {
    if direct_input {
        match text_input::type_text(&replaced).await {
            Ok(_) => {},
            Err(e) => {
                eprintln!("Direct input failed: {}, falling back to paste", e);
                // フォールバック処理
            }
        }
    } else {
        // 既存のペースト処理
    }
}
```

## 注意事項
1. アクセシビリティ権限が必要（System Events）
2. 長文は自動的に分割送信される（デフォルト200文字ごと）
3. エラー時は既存のペースト方式へのフォールバックを推奨

## テスト済み項目
- ✅ 基本的なテキスト入力（英語・日本語）
- ✅ 特殊文字のエスケープ
- ✅ 長文の分割送信
- ✅ エラーハンドリング
- ✅ セキュリティ（インジェクション対策）
- ✅ パフォーマンス（目標時間内）