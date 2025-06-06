# ショートカットキー抑制アイデア

## 概要

voice_inputプロジェクトにおけるショートカットキーモード実装のためのキーイベント抑制機能の設計アイデアをまとめる。既存アプリケーションとの競合を回避しつつ、Cmd+R（録音トグル）、Cmd+1-9（スタックアクセス）等のグローバルショートカットを実現する。

## 基本方針

### アーキテクチャ選択

**実装場所**: voice_inputd（常駐デーモン）
- **理由**: 既に常駐済み、アクセシビリティ権限の一元管理、IPCによる連携基盤
- **責任境界**: 入力系機能（音声録音 + キーフック）を同一プロセスで管理

### 技術スタック

**ライブラリ**: rdev 0.5 (unstable_grab機能)
- **現状確認済み**: 基本的なキー検出動作
- **採用機能**: unstable_grab - グローバルイベントストリームへのフック
- **抑制方法**: コールバック内でNone返却によるイベント抑制

## 技術的課題と検証項目

### 🔴 最優先検証項目

#### 1. キーイベント抑制機能 (unstable_grab)
```rust
// rdev unstable_grab機能を使用
#[cfg(feature = "unstable_grab")]
use rdev::{grab, Event, EventType, Key};

#[cfg(feature = "unstable_grab")]
let callback = |event: Event| -> Option<Event> {
    match event.event_type {
        EventType::KeyPress(Key::KeyR) if is_cmd_pressed() => {
            // Cmd+R検出 - 音声録音トグル実行
            trigger_voice_recording();
            None // イベント抑制（ブラウザリロードを防ぐ）
        }
        EventType::KeyPress(Key::Num1..=Key::Num9) if is_cmd_pressed() => {
            // Cmd+1-9検出 - スタックアクセス実行
            trigger_stack_access(extract_number(&event));
            None // イベント抑制（タブ切り替えを防ぐ）
        }
        _ => Some(event) // その他はパススルー
    }
};

// グローバルフック開始
if let Err(error) = grab(callback) {
    eprintln!("キーフックの開始に失敗: {:?}", error);
}
```

**検証内容**:
- ✅ unstable_grab機能の存在確認済み
- Cmd+特定キーの選択的抑制動作確認
- アクセシビリティ権限エラー時の動作確認
- 長時間稼働時のシステム安定性確認

#### 2. プロセス異常終了時の復旧
```rust
// rdev unstable_grabでの復旧戦略
struct KeyHookManager {
    _hook_thread: Option<std::thread::JoinHandle<()>>,
}

impl KeyHookManager {
    fn start_hook(&mut self) -> Result<(), Error> {
        let handle = std::thread::spawn(|| {
            #[cfg(feature = "unstable_grab")]
            if let Err(error) = grab(|event| {
                // キーイベント処理ロジック
                handle_key_event(event)
            }) {
                eprintln!("グローバルフック失敗: {:?}", error);
            }
        });
        self._hook_thread = Some(handle);
        Ok(())
    }
}

impl Drop for KeyHookManager {
    fn drop(&mut self) {
        // rdev grab関数はブロッキング処理のため
        // スレッド終了で自動的にフック解除される
        if let Some(handle) = self._hook_thread.take() {
            // 必要に応じてスレッドの適切な終了処理
        }
    }
}
```

**検証内容**:
- grab関数のブロッキング動作とプロセス終了時の自動解除確認
- voice_inputd異常終了（kill -9）時のシステム状態確認
- アクセシビリティ権限失効時のエラーハンドリング確認

### 🟡 実装方針決定項目

#### 3. voice_inputd統合アーキテクチャ
```rust
// 統合アーキテクチャ案
pub struct VoiceInputDaemon {
    audio_service: AudioService,
    stack_service: StackService,
    shortcut_service: ShortcutService, // 新規追加
    ipc_server: IpcServer,
}

pub struct ShortcutService {
    key_hook: KeyHookManager,
    cmd_pressed: Arc<Mutex<bool>>,
}
```

**検証内容**:
- tokio非同期処理とrdev同期処理の共存
- IPCコマンド拡張による制御インターフェース
- 音声録音処理との同時実行時の性能影響

#### 4. レスポンス時間要件
**目標**: キー検出から音声録音開始まで10ms以下

```rust
// 性能測定項目
fn measure_key_to_recording_latency() {
    // 1. キーイベント検出時刻
    // 2. IPCコマンド送信時刻  
    // 3. 音声録音開始時刻
    // 総遅延時間を測定
}
```

### 🟢 ユーザビリティ確認項目

#### 5. 実際の競合パターン
**主要検証対象アプリ**:
- Safari/Chrome: Cmd+R（リロード）、Cmd+1-9（タブ切り替え）
- VSCode: Cmd+R（デバッグ実行）
- Terminal: Cmd+R（履歴検索）

**検証方法**:
```bash
# 競合テストシナリオ
cargo run --example shortcut_key_test
# 1. 各アプリでCmd+Rを実行
# 2. キー検出とアプリ動作の両方発生確認
# 3. 抑制時はキー検出のみ、アプリ動作なし確認
```

## 実装戦略

### Phase 0での検証範囲
1. **✅ キーイベント抑制API調査**: rdev unstable_grab機能採用決定
2. **基本的な抑制機能プロトタイプ**: examples/key_suppression_test.rs
3. **異常終了復旧テスト**: プロセスkill時の動作確認
4. **主要アプリ競合テスト**: 実機での動作確認

### 代替案の準備
**Plan A**: ✅ rdev unstable_grabによる完全抑制（採用）
**Plan B**: パススルー + 重複処理抑制（アプリ側で対処）
**Plan C**: 異なるキーバインド使用（Cmd+Shift+R等）

## 技術的制約と前提条件

### macOS固有制約
- **アクセシビリティ権限**: 必須、grab関数がEventTapErrorで失敗する
- **プロセス要件**: grab実行プロセスが親プロセスである必要（forkなし）
- **システム保護機能**: 一部システムキーは抑制不可の可能性
- **サンドボックス制限**: App Store配布時の制約（将来考慮）

### 既存システムとの共存
- **voice_inputdアーキテクチャ**: 既存のtokio非同期処理基盤
- **IPCインターフェース**: `/tmp/voice_input.sock`経由の制御
- **メモリ管理**: インメモリ処理方針の維持

## 成功基準

### 技術的成功基準
- [ ] 特定キー組み合わせの選択的抑制が可能
- [ ] プロセス異常終了時の自動復旧が機能
- [ ] キー検出から録音開始まで10ms以下
- [ ] 既存機能（音声録音、スタック管理）への影響なし

### ユーザビリティ成功基準  
- [ ] 主要アプリとの競合が回避される
- [ ] 日常的なワークフローが阻害されない
- [ ] 権限設定が明確で理解しやすい
- [ ] エラー時のフォールバック動作が適切

## 次のアクション

1. **✅ rdevキーイベント抑制API調査** - unstable_grab機能採用決定
2. **rdev unstable_grab プロトタイプ作成** - examples/key_suppression_test.rs作成
3. **プロセス復旧機能検証** - 異常終了テストシナリオ実行
4. **統合アーキテクチャ詳細設計** - voice_inputdへの組み込み方針策定
5. **実機競合テスト** - 主要アプリでの実際の使用感確認

## 実装要件更新

### Cargo.toml更新
```toml
[dependencies]
rdev = { version = "0.5", features = ["unstable_grab"] }
```

### 必須検証項目
- [ ] unstable_grab機能でのCmd+R抑制テスト
- [ ] unstable_grab機能でのCmd+1-9抑制テスト  
- [ ] アクセシビリティ権限エラー時の適切なエラーハンドリング
- [ ] 長時間稼働時のメモリリークやパフォーマンス影響確認