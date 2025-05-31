/// Integration Test: Daemon IPC Stack Command Processing
///
/// このテストは、デーモンのIPC処理ロジックをテストするために作成されています。
///
/// ## なぜプロダクションコードを直接使わないのか？
///
/// プロダクションコード（voice_inputd.rs:handle_client）には以下の問題があります：
///
/// 1. **外部依存の混在**: `text_input::type_text()` が実際のキーボード入力を実行
///    - GUI権限が必要（macOS Accessibility）
///    - CI環境では実行不可能
///    - テスト時に予期しない入力が発生
///
/// 2. **複雑な初期化**: UnixSocket、Recorder、複数のArc<Mutex<>>が必要
///
/// 3. **テスト対象の不明確さ**: ネットワークI/O + ビジネスロジックが混在
///
/// ## このテストの目的
///
/// **テストしたいもの**：
/// - IPC処理ロジック（StackServiceとの統合）
/// - エラーハンドリング（mutex失敗時の適切な処理）
/// - レスポンス形式の正確性
/// - 状態変更の正しさ
///
/// **テストしたくないもの**：
/// - 実際のテキスト入力（外部依存）
/// - ネットワークI/O
/// - プロセス間通信
///
/// ## アプローチ
///
/// プロダクションコードのIPC処理ロジックを抽出し、外部依存を排除した
/// `simulate_ipc_processing` 関数でビジネスロジックのみをテストします。
/// これにより、安定したCI実行と高速なテスト実行を実現しています。
use std::sync::{Arc, Mutex};
use voice_input::{
    application::StackService,
    ipc::{IpcCmd, IpcResp},
};

/// プロダクションコードのIPC処理ロジックをシミュレート
///
/// 外部依存（text_input、UnixSocket等）を排除し、StackServiceとの統合部分のみを抽出。
/// プロダクションコード（voice_inputd.rs:211-311行目）と同等のロジックを実装。
async fn simulate_ipc_processing(
    cmd: IpcCmd,
    stack_service: &Arc<Mutex<StackService>>,
) -> Result<IpcResp, String> {
    match cmd {
        IpcCmd::EnableStackMode => match stack_service.lock() {
            Ok(mut service) => {
                service.enable_stack_mode();
                Ok(IpcResp {
                    ok: true,
                    msg: "Stack mode enabled".to_string(),
                })
            }
            Err(e) => Ok(IpcResp {
                ok: false,
                msg: format!("Failed to enable stack mode: {}", e),
            }),
        },
        IpcCmd::DisableStackMode => match stack_service.lock() {
            Ok(mut service) => {
                service.disable_stack_mode();
                Ok(IpcResp {
                    ok: true,
                    msg: "Stack mode disabled".to_string(),
                })
            }
            Err(e) => Ok(IpcResp {
                ok: false,
                msg: format!("Failed to disable stack mode: {}", e),
            }),
        },
        IpcCmd::ListStacks => match stack_service.lock() {
            Ok(service) => {
                let stacks = service.list_stacks();
                let mode_enabled = service.is_stack_mode_enabled();

                if stacks.is_empty() {
                    Ok(IpcResp {
                        ok: true,
                        msg: format!(
                            "Stack mode: {} | No stacks",
                            if mode_enabled { "enabled" } else { "disabled" }
                        ),
                    })
                } else {
                    let mut lines = vec![format!(
                        "Stack mode: {}",
                        if mode_enabled { "enabled" } else { "disabled" }
                    )];
                    for stack in stacks {
                        lines.push(format!(
                            "[{}] {} ({})",
                            stack.number, stack.preview, stack.created_at
                        ));
                    }
                    Ok(IpcResp {
                        ok: true,
                        msg: lines.join("\n"),
                    })
                }
            }
            Err(e) => Ok(IpcResp {
                ok: false,
                msg: format!("Failed to list stacks: {}", e),
            }),
        },
        IpcCmd::ClearStacks => match stack_service.lock() {
            Ok(mut service) => {
                service.clear_stacks();
                Ok(IpcResp {
                    ok: true,
                    msg: "All stacks cleared".to_string(),
                })
            }
            Err(e) => Ok(IpcResp {
                ok: false,
                msg: format!("Failed to clear stacks: {}", e),
            }),
        },
        IpcCmd::PasteStack { number } => {
            match stack_service.lock() {
                Ok(service) => {
                    if let Some(_stack) = service.get_stack(number) {
                        // Simulate successful paste (skip actual text input in test)
                        Ok(IpcResp {
                            ok: true,
                            msg: format!("Pasted stack {}", number),
                        })
                    } else {
                        Ok(IpcResp {
                            ok: false,
                            msg: format!("Stack {} not found", number),
                        })
                    }
                }
                Err(e) => Ok(IpcResp {
                    ok: false,
                    msg: format!("Failed to access stack service: {}", e),
                }),
            }
        }
        _ => Err("Unsupported command for this test".to_string()),
    }
}

#[tokio::test]
async fn test_enable_stack_mode_ipc() {
    let stack_service = Arc::new(Mutex::new(StackService::new()));

    let resp = simulate_ipc_processing(IpcCmd::EnableStackMode, &stack_service)
        .await
        .unwrap();
    assert!(resp.ok);
    assert_eq!(resp.msg, "Stack mode enabled");

    // Verify the service state changed
    let service = stack_service.lock().unwrap();
    assert!(service.is_stack_mode_enabled());
}

#[tokio::test]
async fn test_disable_stack_mode_ipc() {
    let stack_service = Arc::new(Mutex::new(StackService::new()));

    // First enable it
    let mut service = stack_service.lock().unwrap();
    service.enable_stack_mode();
    drop(service);

    let resp = simulate_ipc_processing(IpcCmd::DisableStackMode, &stack_service)
        .await
        .unwrap();
    assert!(resp.ok);
    assert_eq!(resp.msg, "Stack mode disabled");

    // Verify the service state changed
    let service = stack_service.lock().unwrap();
    assert!(!service.is_stack_mode_enabled());
}

#[tokio::test]
async fn test_list_stacks_empty_ipc() {
    let stack_service = Arc::new(Mutex::new(StackService::new()));

    let resp = simulate_ipc_processing(IpcCmd::ListStacks, &stack_service)
        .await
        .unwrap();
    assert!(resp.ok);
    assert!(resp.msg.contains("Stack mode: disabled"));
    assert!(resp.msg.contains("No stacks"));
}

#[tokio::test]
async fn test_list_stacks_with_data_ipc() {
    let stack_service = Arc::new(Mutex::new(StackService::new()));

    // Add some test stacks
    {
        let mut service = stack_service.lock().unwrap();
        service.enable_stack_mode();
        service.save_stack("First test stack".to_string());
        service.save_stack("Second test stack".to_string());
    }

    let resp = simulate_ipc_processing(IpcCmd::ListStacks, &stack_service)
        .await
        .unwrap();
    assert!(resp.ok);
    assert!(resp.msg.contains("Stack mode: enabled"));
    assert!(resp.msg.contains("[1]"));
    assert!(resp.msg.contains("[2]"));
    assert!(resp.msg.contains("First test stack"));
    assert!(resp.msg.contains("Second test stack"));
}

#[tokio::test]
async fn test_clear_stacks_ipc() {
    let stack_service = Arc::new(Mutex::new(StackService::new()));

    // Add some test stacks
    {
        let mut service = stack_service.lock().unwrap();
        service.save_stack("Test stack to be cleared".to_string());
    }

    let resp = simulate_ipc_processing(IpcCmd::ClearStacks, &stack_service)
        .await
        .unwrap();
    assert!(resp.ok);
    assert_eq!(resp.msg, "All stacks cleared");

    // Verify stacks were cleared
    let service = stack_service.lock().unwrap();
    assert_eq!(service.list_stacks().len(), 0);
}

#[tokio::test]
async fn test_paste_stack_success_ipc() {
    let stack_service = Arc::new(Mutex::new(StackService::new()));

    // Add a test stack
    {
        let mut service = stack_service.lock().unwrap();
        service.save_stack("Test stack for pasting".to_string());
    }

    let resp = simulate_ipc_processing(IpcCmd::PasteStack { number: 1 }, &stack_service)
        .await
        .unwrap();
    assert!(resp.ok);
    assert_eq!(resp.msg, "Pasted stack 1");
}

#[tokio::test]
async fn test_paste_stack_not_found_ipc() {
    let stack_service = Arc::new(Mutex::new(StackService::new()));

    let resp = simulate_ipc_processing(IpcCmd::PasteStack { number: 999 }, &stack_service)
        .await
        .unwrap();
    assert!(!resp.ok);
    assert_eq!(resp.msg, "Stack 999 not found");
}
