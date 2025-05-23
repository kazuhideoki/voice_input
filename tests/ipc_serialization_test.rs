use voice_input::ipc::IpcCmd;

#[test]
fn test_ipccmd_serialization_with_direct_input() {
    // Test Start command with direct_input
    let start_cmd = IpcCmd::Start {
        paste: true,
        prompt: Some("test prompt".to_string()),
        direct_input: true,
    };

    let json = serde_json::to_string(&start_cmd).unwrap();
    let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();

    match deserialized {
        IpcCmd::Start {
            paste,
            prompt,
            direct_input,
        } => {
            assert_eq!(paste, true);
            assert_eq!(prompt, Some("test prompt".to_string()));
            assert_eq!(direct_input, true);
        }
        _ => panic!("Expected Start command"),
    }
}

#[test]
fn test_ipccmd_serialization_toggle() {
    // Test Toggle command with direct_input
    let toggle_cmd = IpcCmd::Toggle {
        paste: false,
        prompt: None,
        direct_input: false,
    };

    let json = serde_json::to_string(&toggle_cmd).unwrap();
    let deserialized: IpcCmd = serde_json::from_str(&json).unwrap();

    match deserialized {
        IpcCmd::Toggle {
            paste,
            prompt,
            direct_input,
        } => {
            assert_eq!(paste, false);
            assert_eq!(prompt, None);
            assert_eq!(direct_input, false);
        }
        _ => panic!("Expected Toggle command"),
    }
}

#[test]
fn test_ipccmd_json_roundtrip() {
    // Test various combinations
    let commands = vec![
        IpcCmd::Start {
            paste: true,
            prompt: None,
            direct_input: true,
        },
        IpcCmd::Start {
            paste: false,
            prompt: Some("hello".to_string()),
            direct_input: false,
        },
        IpcCmd::Toggle {
            paste: true,
            prompt: Some("world".to_string()),
            direct_input: true,
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
        paste: true,
        prompt: Some("test".to_string()),
        direct_input: true,
    };

    let json = serde_json::to_string(&cmd).unwrap();
    assert!(json.contains("\"Start\""));
    assert!(json.contains("\"paste\":true"));
    assert!(json.contains("\"prompt\":\"test\""));
    assert!(json.contains("\"direct_input\":true"));
}
