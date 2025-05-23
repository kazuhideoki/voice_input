//! voice_inputdの直接入力機能のテスト

use std::error::Error;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::time::timeout;
use voice_input::ipc::{IpcCmd, IpcResp};

/// テスト用のソケットパス
fn test_socket_path() -> String {
    format!("/tmp/voice_input_test_{}.sock", std::process::id())
}

/// voice_inputdプロセスの起動と停止をテスト
#[tokio::test]
async fn test_voice_inputd_startup_shutdown() -> Result<(), Box<dyn Error>> {
    let socket_path = test_socket_path();

    // ソケットが存在しないことを確認
    assert!(!std::path::Path::new(&socket_path).exists());

    // TODO: 実際のvoice_inputdプロセスを起動する処理
    // 現在は単体テストのみ実装

    Ok(())
}

/// direct_input=trueでのIPCコマンド送信をテスト
#[tokio::test]
async fn test_direct_input_ipc_command() -> Result<(), Box<dyn Error>> {
    // IpcCmdの構築とシリアライゼーションのテスト
    let cmd = IpcCmd::Start {
        paste: true,
        prompt: None,
        direct_input: true,
    };

    let json = serde_json::to_string(&cmd)?;
    assert!(json.contains("\"direct_input\":true"));

    // デシリアライゼーションのテスト
    let deserialized: IpcCmd = serde_json::from_str(&json)?;
    match deserialized {
        IpcCmd::Start { direct_input, .. } => {
            assert!(direct_input);
        }
        _ => panic!("Unexpected command type"),
    }

    Ok(())
}

/// direct_input=falseでのIPCコマンド送信をテスト（既存動作の維持）
#[tokio::test]
async fn test_legacy_paste_ipc_command() -> Result<(), Box<dyn Error>> {
    let cmd = IpcCmd::Start {
        paste: true,
        prompt: None,
        direct_input: false,
    };

    let json = serde_json::to_string(&cmd)?;
    assert!(json.contains("\"direct_input\":false"));

    let deserialized: IpcCmd = serde_json::from_str(&json)?;
    match deserialized {
        IpcCmd::Start { direct_input, .. } => {
            assert!(!direct_input);
        }
        _ => panic!("Unexpected command type"),
    }

    Ok(())
}

/// Toggleコマンドでのdirect_inputフラグのテスト
#[tokio::test]
async fn test_toggle_with_direct_input() -> Result<(), Box<dyn Error>> {
    let cmd = IpcCmd::Toggle {
        paste: true,
        prompt: Some("Test prompt".to_string()),
        direct_input: true,
    };

    let json = serde_json::to_string(&cmd)?;
    let deserialized: IpcCmd = serde_json::from_str(&json)?;

    match deserialized {
        IpcCmd::Toggle {
            paste,
            prompt,
            direct_input,
        } => {
            assert!(paste);
            assert_eq!(prompt, Some("Test prompt".to_string()));
            assert!(direct_input);
        }
        _ => panic!("Unexpected command type"),
    }

    Ok(())
}

/// エラー時のフォールバック動作の確認
/// 注：実際のフォールバック動作は手動テストで確認する必要がある
#[test]
fn test_error_handling_scenario() {
    // このテストは概念的なもので、実際のエラーハンドリングは
    // voice_inputdの実行時に確認する必要がある

    // シナリオ1: text_input::type_text()がエラーを返す
    // 期待: エラーログが出力され、osascriptによるペーストが実行される

    // シナリオ2: direct_input=falseの場合
    // 期待: 従来通りosascriptによるペーストが実行される

    assert!(true); // プレースホルダー
}

/// 統合テストのヘルパー関数
async fn send_ipc_command(socket_path: &str, cmd: IpcCmd) -> Result<IpcResp, Box<dyn Error>> {
    let stream = UnixStream::connect(socket_path).await?;
    let (reader, mut writer) = stream.into_split();

    // コマンドを送信
    let cmd_json = serde_json::to_string(&cmd)?;
    writer.write_all(cmd_json.as_bytes()).await?;
    writer.write_all(b"\n").await?;
    writer.flush().await?;

    // レスポンスを受信
    let mut reader = BufReader::new(reader);
    let mut line = String::new();

    timeout(Duration::from_secs(5), reader.read_line(&mut line)).await??;

    let resp: IpcResp = serde_json::from_str(&line)?;
    Ok(resp)
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    /// 実際のvoice_inputdプロセスとの統合テスト
    /// 注：このテストは手動で実行する必要がある
    #[tokio::test]
    #[ignore] // 手動実行用
    async fn test_real_voice_inputd_interaction() -> Result<(), Box<dyn Error>> {
        // 1. voice_inputdが実行されていることを前提とする
        // 2. 実際のソケットパスを使用
        let socket_path = "/tmp/voice_input.sock";

        // direct_input=trueでStartコマンドを送信
        let cmd = IpcCmd::Start {
            paste: true,
            prompt: None,
            direct_input: true,
        };

        match send_ipc_command(socket_path, cmd).await {
            Ok(resp) => {
                println!("Received response: {:?}", resp);
                assert!(resp.ok, "Expected successful response");
                println!("Message: {}", resp.msg);
            }
            Err(e) => {
                eprintln!("Error: {}. Make sure voice_inputd is running.", e);
                return Err(e);
            }
        }

        Ok(())
    }
}
