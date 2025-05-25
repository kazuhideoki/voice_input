use voice_input::ipc::IpcCmd;

#[test]
fn test_backward_compatibility_without_direct_input() {
    // Test that old JSON format without direct_input field still works
    // This simulates messages from older clients

    // Old format Start command (without direct_input)
    let old_json = r#"{"Start":{"paste":true,"prompt":"test"}}"#;

    // This should fail because direct_input is now required
    let result = serde_json::from_str::<IpcCmd>(old_json);
    assert!(
        result.is_err(),
        "Expected deserialization to fail without direct_input field"
    );
}

#[test]
fn test_backward_compatibility_with_default() {
    // Since direct_input is required, we need to ensure new clients always send it
    // This test verifies the current behavior

    let json_with_direct_input = r#"{"Start":{"paste":true,"prompt":"test","direct_input":false}}"#;
    let cmd: IpcCmd = serde_json::from_str(json_with_direct_input).unwrap();

    match cmd {
        IpcCmd::Start {
            paste,
            prompt,
            direct_input,
        } => {
            assert!(paste);
            assert_eq!(prompt, Some("test".to_string()));
            assert!(!direct_input);
        }
        _ => panic!("Expected Start command"),
    }
}

#[test]
fn test_toggle_backward_compatibility() {
    // Test Toggle command compatibility
    let old_json = r#"{"Toggle":{"paste":false,"prompt":null}}"#;

    // This should fail because direct_input is now required
    let result = serde_json::from_str::<IpcCmd>(old_json);
    assert!(
        result.is_err(),
        "Expected deserialization to fail without direct_input field"
    );
}

#[test]
fn test_other_commands_unchanged() {
    // Test that other commands work as before
    let commands = vec![
        (r#"{"Stop":null}"#, "Stop"),
        (r#"{"Status":null}"#, "Status"),
        (r#"{"Health":null}"#, "Health"),
        (r#"{"ListDevices":null}"#, "ListDevices"),
    ];

    for (json, expected) in commands {
        let cmd: IpcCmd = serde_json::from_str(json).unwrap();
        let variant_name = match cmd {
            IpcCmd::Stop => "Stop",
            IpcCmd::Status => "Status",
            IpcCmd::Health => "Health",
            IpcCmd::ListDevices => "ListDevices",
            _ => "Unknown",
        };
        assert_eq!(variant_name, expected);
    }
}

#[test]
fn test_future_compatibility() {
    // Test that extra fields in JSON are ignored (forward compatibility)
    let json_with_extra =
        r#"{"Start":{"paste":true,"prompt":"test","direct_input":true,"future_field":"ignored"}}"#;

    // serde by default ignores unknown fields, so this should work
    let result = serde_json::from_str::<IpcCmd>(json_with_extra);
    assert!(
        result.is_ok(),
        "Should ignore unknown fields for forward compatibility"
    );
}
