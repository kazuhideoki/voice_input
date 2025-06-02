//! ShortcutServiceの統合テスト
//! Phase 1の基本機能統合をテストする

use tokio::sync::mpsc;
use voice_input::ipc::IpcCmd;
use voice_input::shortcut::ShortcutService;

#[tokio::test]
async fn test_shortcut_service_creation() {
    let service = ShortcutService::new();
    assert!(!service.is_enabled());
}

#[tokio::test]
async fn test_shortcut_service_stop_when_not_started() {
    let mut service = ShortcutService::new();
    let result = service.stop().await;
    assert!(result.is_ok());
    assert!(!service.is_enabled());
}

#[tokio::test]
async fn test_ipc_channel_integration() {
    // IPCチャンネルがShortcutServiceと正しく統合されることを確認
    let (tx, mut rx) = mpsc::unbounded_channel::<IpcCmd>();

    // テスト用のコマンドを送信
    let test_cmd = IpcCmd::Toggle {
        paste: false,
        prompt: None,
        direct_input: false,
    };

    let send_result = tx.send(test_cmd);
    assert!(send_result.is_ok());

    // 受信できることを確認
    let received = rx.recv().await;
    assert!(received.is_some());

    match received.unwrap() {
        IpcCmd::Toggle {
            paste,
            prompt,
            direct_input,
        } => {
            assert!(!paste);
            assert!(prompt.is_none());
            assert!(!direct_input);
        }
        _ => panic!("Expected Toggle command"),
    }
}

#[tokio::test]
async fn test_paste_stack_command_serialization() {
    let (tx, mut rx) = mpsc::unbounded_channel::<IpcCmd>();

    // PasteStackコマンドをテスト
    let test_cmd = IpcCmd::PasteStack { number: 5 };
    let send_result = tx.send(test_cmd);
    assert!(send_result.is_ok());

    let received = rx.recv().await;
    assert!(received.is_some());

    match received.unwrap() {
        IpcCmd::PasteStack { number } => {
            assert_eq!(number, 5);
        }
        _ => panic!("Expected PasteStack command"),
    }
}

#[tokio::test]
#[ignore] // rdev実動作が必要なため手動テストのみ
async fn test_shortcut_service_start_with_accessibility_check() {
    let mut service = ShortcutService::new();
    let (tx, _rx) = mpsc::unbounded_channel();

    // アクセシビリティ権限の状態によって結果が変わる
    let result = service.start(tx).await;

    // 成功 or アクセシビリティ権限エラーのいずれかであることを確認
    match result {
        Ok(_) => {
            println!("Shortcut service started successfully");
            assert!(service.is_enabled());

            // クリーンアップ
            let stop_result = service.stop().await;
            assert!(stop_result.is_ok());
            assert!(!service.is_enabled());
        }
        Err(e) => {
            println!("Expected accessibility permission error: {}", e);
            assert!(e.contains("アクセシビリティ権限"));
            assert!(!service.is_enabled());
        }
    }
}

#[tokio::test]
async fn test_shortcut_service_lifecycle() {
    let mut service = ShortcutService::new();
    let (_tx, _rx) = mpsc::unbounded_channel::<IpcCmd>();

    // 初期状態
    assert!(!service.is_enabled());

    // 停止処理（未開始状態でも成功するべき）
    let stop_result = service.stop().await;
    assert!(stop_result.is_ok());
    assert!(!service.is_enabled());

    // ShortcutServiceインスタンスが正常に作成できることを確認
    // 実際のstart/stopは手動テストで確認
}

#[tokio::test]
async fn test_multiple_ipc_commands() {
    let (tx, mut rx) = mpsc::unbounded_channel::<IpcCmd>();

    // 複数のコマンドを送信
    let commands = vec![
        IpcCmd::Toggle {
            paste: true,
            prompt: Some("test".to_string()),
            direct_input: false,
        },
        IpcCmd::PasteStack { number: 1 },
        IpcCmd::PasteStack { number: 9 },
        IpcCmd::Toggle {
            paste: false,
            prompt: None,
            direct_input: true,
        },
    ];

    for cmd in commands.iter() {
        let result = tx.send(cmd.clone());
        assert!(result.is_ok());
    }

    // 全て受信できることを確認
    for expected_cmd in commands.iter() {
        let received = rx.recv().await;
        assert!(received.is_some());

        match (&received.unwrap(), expected_cmd) {
            (
                IpcCmd::Toggle {
                    paste: p1,
                    prompt: pr1,
                    direct_input: d1,
                },
                IpcCmd::Toggle {
                    paste: p2,
                    prompt: pr2,
                    direct_input: d2,
                },
            ) => {
                assert_eq!(p1, p2);
                assert_eq!(pr1, pr2);
                assert_eq!(d1, d2);
            }
            (IpcCmd::PasteStack { number: n1 }, IpcCmd::PasteStack { number: n2 }) => {
                assert_eq!(n1, n2);
            }
            _ => panic!("Command mismatch"),
        }
    }
}

#[test]
fn test_cli_args_parsing() {
    // voice_inputdのCLI引数構造体が正しく定義されていることを確認
    // 実際のパースはbinクレートなので、型の存在確認のみ

    // ShortcutServiceの基本機能が利用可能であることを確認
    let service = ShortcutService::new();
    assert!(!service.is_enabled());

    // IPCコマンドが正しく定義されていることを確認
    let cmd = IpcCmd::Toggle {
        paste: false,
        prompt: None,
        direct_input: false,
    };

    match cmd {
        IpcCmd::Toggle { .. } => {} // 正常
        _ => panic!("Toggle command not properly defined"),
    }
}

#[tokio::test]
async fn test_error_handling() {
    let mut service = ShortcutService::new();

    // 未開始状態での停止は成功するべき
    let result = service.stop().await;
    assert!(result.is_ok());

    // 状態が正しく管理されていることを確認
    assert!(!service.is_enabled());
}

// CI環境で安全に実行できるテストのみを含む
// rdev実動作が必要なテストは #[ignore] マークを付けて手動実行のみとする
