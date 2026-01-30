use voice_input::ipc::IpcCmd;

#[test]
fn test_backward_compatibility_without_prompt() {
    // prompt が省略された旧形式でも受け付ける
    let old_json = r#"{"Start":{}}"#;

    let result = serde_json::from_str::<IpcCmd>(old_json);
    assert!(result.is_ok(), "Expected deserialization to succeed");
}

#[test]
fn test_backward_compatibility_with_extra_fields() {
    // 旧クライアントのフィールドが混在しても無視される
    let json_with_extra = r#"{"Start":{"paste":true,"prompt":"test","direct_input":false}}"#;
    let cmd: IpcCmd = serde_json::from_str(json_with_extra).unwrap();

    match cmd {
        IpcCmd::Start { prompt } => {
            assert_eq!(prompt, Some("test".to_string()));
        }
        _ => panic!("Expected Start command"),
    }
}

#[test]
fn test_toggle_backward_compatibility() {
    // Test Toggle command compatibility with extra fields
    let old_json = r#"{"Toggle":{"paste":false,"prompt":null,"direct_input":true}}"#;

    let result = serde_json::from_str::<IpcCmd>(old_json);
    assert!(result.is_ok(), "Expected deserialization to succeed");
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
    let json_with_extra = r#"{"Start":{"prompt":"test","future_field":"ignored"}}"#;

    // serde by default ignores unknown fields, so this should work
    let result = serde_json::from_str::<IpcCmd>(json_with_extra);
    assert!(
        result.is_ok(),
        "Should ignore unknown fields for forward compatibility"
    );
}
