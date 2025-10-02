use std::cell::RefCell;
use std::rc::Rc;
use voice_input::application::StackService;
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
    assert!(matches!(cmd, IpcCmd::EnableStackMode));
}

#[test]
fn test_stack_mode_prevents_auto_paste() {
    // スタックモードが有効な場合、転写結果がスタックに保存されるのみで
    // 自動ペーストが実行されないことをテストするロジックを検証

    // StackServiceインスタンスを作成
    let mut stack_service = StackService::new();

    // 通常モード（スタックモード無効）のテスト
    assert!(!stack_service.is_stack_mode_enabled());

    // paste=true, stack_service=None（通常モード）の場合
    // should_paste = true && (true || false) = true
    let paste = true;
    let stack_service_none: Option<Rc<RefCell<StackService>>> = None;
    let should_paste_normal = paste
        && (stack_service_none.is_none()
            || !stack_service_none
                .as_ref()
                .unwrap()
                .borrow()
                .is_stack_mode_enabled());
    assert!(should_paste_normal, "Normal mode should allow paste");

    // スタックモードを有効化
    stack_service.enable_stack_mode();
    assert!(stack_service.is_stack_mode_enabled());

    // paste=true, stack_service=Some（スタックモード有効）の場合
    // should_paste = true && (false || !true) = true && false = false
    let stack_service_some = Some(Rc::new(RefCell::new(stack_service)));
    let should_paste_stack = paste
        && (stack_service_some.is_none()
            || !stack_service_some
                .as_ref()
                .unwrap()
                .borrow()
                .is_stack_mode_enabled());
    assert!(!should_paste_stack, "Stack mode should prevent auto-paste");
}

#[test]
fn test_stack_mode_paste_logic_comprehensive() {
    // より包括的なテストケース

    // ケース1: paste=false（ペーストしない設定）
    let paste = false;
    let stack_service1 = StackService::new();
    let stack_service_ref = Some(Rc::new(RefCell::new(stack_service1)));
    let should_paste = paste
        && (stack_service_ref.is_none()
            || !stack_service_ref
                .as_ref()
                .unwrap()
                .borrow()
                .is_stack_mode_enabled());
    assert!(
        !should_paste,
        "paste=false should never paste regardless of stack mode"
    );

    // ケース2: paste=true, スタックモードOFF
    let mut stack_service2 = StackService::new();
    stack_service2.disable_stack_mode();
    let paste = true;
    let stack_service_ref = Some(Rc::new(RefCell::new(stack_service2)));
    let should_paste = paste
        && (stack_service_ref.is_none()
            || !stack_service_ref
                .as_ref()
                .unwrap()
                .borrow()
                .is_stack_mode_enabled());
    assert!(should_paste, "paste=true with stack mode OFF should paste");

    // ケース3: paste=true, スタックモードON
    let mut stack_service3 = StackService::new();
    stack_service3.enable_stack_mode();
    let stack_service_ref = Some(Rc::new(RefCell::new(stack_service3)));
    let should_paste = paste
        && (stack_service_ref.is_none()
            || !stack_service_ref
                .as_ref()
                .unwrap()
                .borrow()
                .is_stack_mode_enabled());
    assert!(
        !should_paste,
        "paste=true with stack mode ON should NOT paste"
    );

    // ケース4: paste=true, stack_service=None（レガシーモード）
    let stack_service_ref: Option<Rc<RefCell<StackService>>> = None;
    let should_paste = paste
        && (stack_service_ref.is_none()
            || !stack_service_ref
                .as_ref()
                .unwrap()
                .borrow()
                .is_stack_mode_enabled());
    assert!(
        should_paste,
        "paste=true with no stack service should paste (legacy behavior)"
    );
}
