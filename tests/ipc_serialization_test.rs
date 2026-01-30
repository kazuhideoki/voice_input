use voice_input::ipc::IpcCmd;

#[test]
fn test_ipccmd_serialization_start() {
    let start_cmd = IpcCmd::Start {
        prompt: Some("test prompt".to_string()),
    };

    let json = serde_json::to_string(&start_cmd).unwrap();
    let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();

    match deserialized {
        IpcCmd::Start { prompt } => {
            assert_eq!(prompt, Some("test prompt".to_string()));
        }
        _ => panic!("Expected Start command"),
    }
}

#[test]
fn test_ipccmd_serialization_toggle() {
    let toggle_cmd = IpcCmd::Toggle { prompt: None };

    let json = serde_json::to_string(&toggle_cmd).unwrap();
    let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();

    match deserialized {
        IpcCmd::Toggle { prompt } => {
            assert_eq!(prompt, None);
        }
        _ => panic!("Expected Toggle command"),
    }
}

#[test]
fn test_ipccmd_json_roundtrip() {
    // Test various combinations
    let commands = vec![
        IpcCmd::Start { prompt: None },
        IpcCmd::Start {
            prompt: Some("hello".to_string()),
        },
        IpcCmd::Toggle {
            prompt: Some("world".to_string()),
        },
        IpcCmd::Stop,
        IpcCmd::Status,
        IpcCmd::Health,
        IpcCmd::ListDevices,
    ];

    for cmd in commands {
        let json = serde_json::to_string(&cmd).unwrap();
        let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();

        // Verify the JSON round-trip preserves the data
        let json2 = serde_json::to_string(&deserialized).unwrap();
        assert_eq!(json, json2);
    }
}

#[test]
fn test_ipccmd_json_format() {
    // Verify the actual JSON format
    let cmd = IpcCmd::Start {
        prompt: Some("test".to_string()),
    };

    let json = serde_json::to_string(&cmd).unwrap();
    assert!(json.contains("\"Start\""));
    assert!(json.contains("\"prompt\":\"test\""));
}
