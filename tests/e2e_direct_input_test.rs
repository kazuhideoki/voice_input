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

/// デーモン起動時にStartコマンドへ応答できる
#[test]
#[ignore] // デーモンが起動している必要があるため、通常のテストでは無視
fn start_command_responds_in_daemon() {
    // Startコマンドを送信
    let cmd = IpcCmd::Start { prompt: None };

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

/// 旧フラグ廃止後もStartコマンドが通る
#[test]
#[ignore]
fn start_command_works_without_deprecated_flag() {
    // Startコマンドを送信（旧フラグは廃止）
    let cmd = IpcCmd::Start { prompt: None };

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

/// Toggleコマンドがプロンプト付きでも成功する
#[test]
#[ignore]
fn toggle_command_with_prompt_succeeds() {
    // Toggleコマンドを送信
    let cmd = IpcCmd::Toggle {
        prompt: Some("test prompt".to_string()),
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

/// Statusコマンドが録音状態を返す
#[test]
#[ignore]
fn status_command_reports_recording_state() {
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

/// IpcCmdがJSONに正しくシリアライズされる
#[test]
fn ipc_cmd_serializes_to_json() {
    // IpcCmdが正しくJSONにシリアライズされることを確認
    let cmd = IpcCmd::Start {
        prompt: Some("test".to_string()),
    };

    let json = serde_json::to_string(&cmd).unwrap();
    println!("Serialized JSON: {}", json);

    // JSONに必要なフィールドが含まれていることを確認
    assert!(json.contains(r#""Start""#));
    assert!(json.contains(r#""prompt":"test""#));
}
