use voice_input::domain::{Stack, StackInfo};
use voice_input::ipc::{IpcCmd, IpcStackResp};

#[test]
fn test_stack_module_exports() {
    // Stack構造体が正しくエクスポートされていることを確認
    let stack = Stack::new(1, "Test".to_string());
    assert_eq!(stack.id, 1);

    // StackInfo構造体が正しくエクスポートされていることを確認
    let stack_info = StackInfo {
        number: 1,
        preview: "Test".to_string(),
        created_at: "2024-01-01".to_string(),
    };
    assert_eq!(stack_info.number, 1);
}

#[test]
fn test_ipc_stack_types_available() {
    // IPCモジュールでStack関連の型が利用可能であることを確認
    let stack_resp = IpcStackResp {
        stacks: vec![StackInfo {
            number: 1,
            preview: "Test".to_string(),
            created_at: "2024-01-01".to_string(),
        }],
        mode_enabled: true,
    };
    assert_eq!(stack_resp.stacks.len(), 1);
    assert!(stack_resp.mode_enabled);

    // 新しいIPCコマンドが利用可能であることを確認
    let cmd = IpcCmd::EnableStackMode;
    match cmd {
        IpcCmd::EnableStackMode => assert!(true),
        _ => panic!("Expected EnableStackMode"),
    }
}
