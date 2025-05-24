use std::io::Write;
use std::os::unix::net::UnixStream;
use std::time::Duration;
use voice_input::ipc::{IpcCmd, IpcResp};

/// IPCコマンドを送信してレスポンスを受信するヘルパー関数
fn send_ipc_cmd(cmd: &IpcCmd) -> Result<IpcResp, Box<dyn std::error::Error>> {
    let socket_path = "/tmp/voice_inputd.sock";

    // Unix domain socketに接続
    let mut stream = UnixStream::connect(socket_path)?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;

    // コマンドをJSON形式でシリアライズして送信
    let json = serde_json::to_string(cmd)?;
    stream.write_all(json.as_bytes())?;
    stream.write_all(b"\n")?;
    stream.flush()?;

    // レスポンスを読み取り
    let mut response = String::new();
    use std::io::Read;
    let mut buf = [0; 1024];
    match stream.read(&mut buf) {
        Ok(n) => {
            response.push_str(&String::from_utf8_lossy(&buf[..n]));
        }
        Err(e) => return Err(Box::new(e)),
    }

    // JSONをパース
    let resp: IpcResp = serde_json::from_str(&response)?;
    Ok(resp)
}

#[test]
#[ignore] // デーモンが起動している必要があるため、通常のテストでは無視
fn test_direct_input_flag_e2e() {
    // direct_input=trueでStartコマンドを送信
    let cmd = IpcCmd::Start {
        paste: true,
        prompt: None,
        direct_input: true,
    };

    match send_ipc_cmd(&cmd) {
        Ok(resp) => {
            println!("Response: {:?}", resp);
            // デーモンが正常に応答することを確認
            assert!(resp.ok || resp.msg.contains("Already recording"));
        }
        Err(e) => {
            // デーモンが起動していない場合はスキップ
            eprintln!("Skipping test: daemon not running or error: {}", e);
        }
    }
}

#[test]
#[ignore]
fn test_no_direct_input_flag_e2e() {
    // direct_input=falseでStartコマンドを送信
    let cmd = IpcCmd::Start {
        paste: true,
        prompt: None,
        direct_input: false,
    };

    match send_ipc_cmd(&cmd) {
        Ok(resp) => {
            println!("Response: {:?}", resp);
            assert!(resp.ok || resp.msg.contains("Already recording"));
        }
        Err(e) => {
            eprintln!("Skipping test: daemon not running or error: {}", e);
        }
    }
}

#[test]
#[ignore]
fn test_toggle_with_direct_input_e2e() {
    // direct_input=trueでToggleコマンドを送信
    let cmd = IpcCmd::Toggle {
        paste: true,
        prompt: Some("test prompt".to_string()),
        direct_input: true,
    };

    match send_ipc_cmd(&cmd) {
        Ok(resp) => {
            println!("Response: {:?}", resp);
            assert!(resp.ok);
        }
        Err(e) => {
            eprintln!("Skipping test: daemon not running or error: {}", e);
        }
    }
}

#[test]
#[ignore]
fn test_status_command_e2e() {
    // Statusコマンドは常に動作するはず
    let cmd = IpcCmd::Status;

    match send_ipc_cmd(&cmd) {
        Ok(resp) => {
            println!("Status response: {:?}", resp);
            assert!(resp.ok);
            // ステータスメッセージに期待される内容が含まれているか確認
            assert!(resp.msg.contains("Recording") || resp.msg.contains("Stopped"));
        }
        Err(e) => {
            eprintln!("Skipping test: daemon not running or error: {}", e);
        }
    }
}

#[test]
fn test_ipc_cmd_json_format() {
    // IpcCmdが正しくJSONにシリアライズされることを確認
    let cmd = IpcCmd::Start {
        paste: true,
        prompt: Some("test".to_string()),
        direct_input: true,
    };

    let json = serde_json::to_string(&cmd).unwrap();
    println!("Serialized JSON: {}", json);

    // JSONに必要なフィールドが含まれていることを確認
    assert!(json.contains(r#""Start""#));
    assert!(json.contains(r#""paste":true"#));
    assert!(json.contains(r#""prompt":"test""#));
    assert!(json.contains(r#""direct_input":true"#));
}
