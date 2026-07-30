use voice_input::ipc::IpcCmd;

/// Startの任意フィールドを全て省略しても既定値で復元できる
#[test]
fn start_defaults_omitted_optional_fields() {
    let command: IpcCmd = serde_json::from_str(r#"{"Start":{}}"#).unwrap();

    assert_eq!(
        command,
        IpcCmd::Start {
            save_audio_path: None,
            max_duration_secs: None,
            transcription_provider: None,
        }
    );
}

/// Toggleの任意フィールドを全て省略しても既定値で復元できる
#[test]
fn toggle_defaults_omitted_optional_fields() {
    let command: IpcCmd = serde_json::from_str(r#"{"Toggle":{}}"#).unwrap();

    assert_eq!(
        command,
        IpcCmd::Toggle {
            save_audio_path: None,
            max_duration_secs: None,
            transcription_provider: None,
        }
    );
}

/// 未知フィールドがあっても既知フィールドは復元できる
#[test]
fn unknown_fields_are_ignored() {
    let json = r#"{"Start":{"future_field":"ignored","max_duration_secs":90}}"#;
    let command: IpcCmd = serde_json::from_str(json).unwrap();

    assert_eq!(
        command,
        IpcCmd::Start {
            save_audio_path: None,
            max_duration_secs: Some(90),
            transcription_provider: None,
        }
    );
}

/// 引数を持たないコマンドも従来のJSON表現から復元できる
#[test]
fn unit_commands_deserialize_from_json() {
    let commands = vec![
        (r#"{"Stop":null}"#, IpcCmd::Stop),
        (r#"{"Status":null}"#, IpcCmd::Status),
        (r#"{"History":null}"#, IpcCmd::History),
        (r#"{"Health":null}"#, IpcCmd::Health),
        (r#"{"ListDevices":null}"#, IpcCmd::ListDevices),
    ];

    for (json, expected) in commands {
        assert_eq!(serde_json::from_str::<IpcCmd>(json).unwrap(), expected);
    }
}
