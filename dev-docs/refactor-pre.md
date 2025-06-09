# テキスト入力実装のリファクタリング詳細設計書

## 背景と問題分析

### 現状の問題

1. **macOS Accessibility APIの不安定性**

   - 各アプリケーションの実装に依存しており、動作が不安定
   - アプリケーションによってはテキスト挿入が失敗する
   - エラーハンドリングが困難

2. **Subprocess方式のプロセス管理問題**

   - メインプロセス（voice_inputd）が異常終了した場合、enigo_helperプロセスがゾンビ化する可能性
   - 親プロセスの監視機構が存在しない
   - プロセスグループ管理が実装されていない

3. **ショートカットモード（rdev）の残存問題**
   - スタッキングモードをオフにしても、キーボードフックが残る事象が発生
   - `rdev::grab`が別スレッドで動作し続ける可能性
   - デーモンクラッシュ時にシステム全体のキーボード入力に影響

### 技術的制約

1. **rdevとEnigoの競合**

   - 両者がmacOSの同じイベントシステム（CGEventTap/CGEventPost）を使用
   - カーネルレベルでの競合により、連続入力時に失敗が発生
   - 完全な解決には別プロセス実行が必要

2. **macOSのセキュリティ制約**
   - アクセシビリティ権限が必要
   - サンドボックス環境での動作制限
   - システムイベントへのアクセス制限

## 改善案

### 1. プロセス管理の強化

#### 1.1 親プロセス監視機能の実装

```rust
// src/bin/enigo_helper.rs に追加
use std::process;
use nix::unistd::Pid;
use nix::sys::signal;

fn monitor_parent_process() {
    let ppid = nix::unistd::getppid();

    std::thread::spawn(move || {
        loop {
            // kill(pid, 0) はプロセスの存在確認のみ行う
            match nix::sys::signal::kill(ppid, None) {
                Ok(_) => {
                    // 親プロセスは生きている
                }
                Err(_) => {
                    // 親プロセスが存在しない
                    eprintln!("Parent process died, exiting...");
                    process::exit(0);
                }
            }
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
    });
}

fn main() {
    // 親プロセス監視を開始
    monitor_parent_process();

    // 既存の処理...
}
```

#### 1.2 プロセスグループ管理

```rust
// src/infrastructure/external/text_input_subprocess.rs の改善
use std::os::unix::process::CommandExt;

pub async fn type_text_via_subprocess(text: &str) -> Result<(), SubprocessInputError> {
    let helper_path = /* ... */;

    let mut child = Command::new(&helper_path)
        .arg(text)
        .process_group(0)  // 新しいプロセスグループを作成
        .spawn()
        .map_err(|e| SubprocessInputError::SpawnError(format!("{}: {:?}", e, helper_path)))?;

    // タイムアウト付きで完了を待つ
    match tokio::time::timeout(
        tokio::time::Duration::from_secs(5),
        child.wait()
    ).await {
        Ok(Ok(status)) if status.success() => Ok(()),
        Ok(Ok(status)) => {
            // エラーコードに基づく処理
            Err(SubprocessInputError::ExecutionError(/*...*/))
        }
        Ok(Err(e)) => Err(SubprocessInputError::ExecutionError(e.to_string())),
        Err(_) => {
            // タイムアウト時は強制終了
            let _ = child.kill();
            Err(SubprocessInputError::ExecutionError("Process timeout".to_string()))
        }
    }
}
```

### 2. デーモンの安全な終了処理

#### 2.1 シグナルハンドラーの実装

```rust
// src/bin/voice_inputd.rs に追加
use tokio::signal;
use std::sync::atomic::{AtomicBool, Ordering};

static SHUTDOWN: AtomicBool = AtomicBool::new(false);

async fn setup_signal_handlers(
    ui_manager: Rc<RefCell<UiProcessManager>>,
) {
    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

    println!("Received termination signal, cleaning up...");
    SHUTDOWN.store(true, Ordering::SeqCst);


    // UIプロセスの停止
    if let Ok(mut manager) = ui_manager.try_borrow_mut() {
        let _ = manager.stop_ui();
    }

    // ソケットファイルの削除
    let _ = fs::remove_file(socket_path());

    std::process::exit(0);
}
```

### 3. 設定による実装切り替えの改善

```rust
// src/utils/config.rs の拡張
#[derive(Debug, Clone)]
pub struct EnvConfig {
    // 既存のフィールド...
    pub use_subprocess: bool,
    pub subprocess_timeout_ms: u64,  // 新規追加
    pub enable_parent_monitoring: bool,  // 新規追加
}

impl EnvConfig {
    fn from_env() -> Result<Self, Box<dyn Error>> {
        Ok(Self {
            // 既存の設定...
            use_subprocess: env_bool("VOICE_INPUT_USE_SUBPROCESS"),
            subprocess_timeout_ms: env::var("VOICE_INPUT_SUBPROCESS_TIMEOUT")
                .unwrap_or_else(|_| "5000".to_string())
                .parse()
                .unwrap_or(5000),
            enable_parent_monitoring: env_bool("VOICE_INPUT_ENABLE_PARENT_MONITORING"),
        })
    }
}
```

## 実装計画

### Phase 0: Accessibility API の削除（優先度：最高）

- [ ] text_input_accessibility.rs ファイルの削除
- [ ] text_input.rs からAccessibility API関連のコードを削除
- [ ] VOICE_INPUT_USE_SUBPROCESSをデフォルトに変更
- [ ] 関連するテストコードの削除または修正
- [ ] READMEやドキュメントからAccessibility API関連の記述を削除

**完了条件:**

- `cargo build --release` が成功する
- `cargo test` が全て成功する
- text_input_accessibility関連のコードが完全に削除されている
- subprocess方式がデフォルトで動作する(環境変数なし)

### Phase 1: プロセス管理の強化（優先度：高）

- [ ] enigo_helperに親プロセス監視機能を実装
- [ ] subprocess実行時のプロセスグループ管理を追加
- [ ] タイムアウト処理の実装

**完了条件:**

- voice_inputdをkillしても、enigo_helperプロセスが残らない
- タイムアウト時にenigo_helperが確実に終了する
- `ps aux | grep enigo_helper` でゾンビプロセスが存在しない

### Phase 2: 安全な終了処理（優先度：高）

- [ ] シグナルハンドラーの実装
- [ ] リソースクリーンアップの確実な実行
- [ ] ゾンビプロセス防止機構の実装

**完了条件:**

- Ctrl+C、SIGTERM、SIGKILLのいずれでも適切にクリーンアップされる
- /tmp/voice_input.sock が残らない
- 全ての子プロセスが確実に終了する

### Phase 3: 設定とモニタリング（優先度：低）

- [ ] 詳細な設定オプションの追加
- [ ] プロセス状態のモニタリング
- [ ] ログ機能の強化

**完了条件:**

- 環境変数で全ての動作をカスタマイズ可能
- プロセス状態を`voice_input health`で確認可能
- エラー発生時に詳細なログが出力される

## テスト計画

1. **プロセス管理テスト**

   - 親プロセスkill時の子プロセス終了確認
   - タイムアウト動作の確認
   - プロセスグループ管理の動作確認

2. **終了処理テスト**
   - Ctrl+C、SIGTERM、SIGKILLでの動作確認
   - リソースリークの確認
   - ソケットファイルのクリーンアップ確認

## リスクと対策

1. **後方互換性**

   - 環境変数による段階的移行
   - 既存動作をデフォルトとして維持

2. **パフォーマンス影響**

   - 親プロセス監視の軽量化
   - 必要最小限のチェック頻度

3. **システム依存性**
   - macOS固有機能の適切な分離
   - 将来的なクロスプラットフォーム対応への配慮
