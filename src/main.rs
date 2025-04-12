mod audio_recorder;
mod key_monitor;
mod text_selection;
mod websocket_client;
mod websocket_server;

use device_query::Keycode;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;

fn main() {
    println!("Rust多機能ツールを起動しています...");

    // 選択テキストを保持する共有変数
    let selected_text = Arc::new(Mutex::new(String::new()));

    // キーモニター設定
    let selected_text_clone = Arc::clone(&selected_text);
    let key_monitor = key_monitor::KeyMonitor::new(move |key| {
        match key {
            Keycode::F2 => {
                println!("F2キーが押されました - 選択テキストを取得します");
                match text_selection::get_selected_text() {
                    Ok(text) => {
                        println!("選択されたテキスト: {}", text);
                        if let Ok(mut selected) = selected_text_clone.lock() {
                            *selected = text;
                        }
                    }
                    Err(e) => println!("テキスト取得エラー: {}", e),
                }
            }
            Keycode::F3 => {
                println!("F3キーが押されました - WebSocketサーバーを起動します");
                // WebSocketサーバーを別スレッドで起動
                std::thread::spawn(|| {
                    let rt = Runtime::new().unwrap();
                    rt.block_on(async {
                        let server = websocket_server::WebsocketServer::new("127.0.0.1:8080");
                        if let Err(e) = server.run().await {
                            println!("WebSocketサーバーエラー: {}", e);
                        }
                    });
                });
            }
            Keycode::F4 => {
                println!("F4キーが押されました - WebSocketクライアントを起動します");
                // WebSocketクライアントを別スレッドで起動
                std::thread::spawn(|| {
                    let rt = Runtime::new().unwrap();
                    rt.block_on(async {
                        let client = websocket_client::WebsocketClient::new("ws://127.0.0.1:8080");
                        if let Err(e) = client.connect().await {
                            println!("WebSocketクライアントエラー: {}", e);
                        }
                    });
                });
            }
            _ => {}
        }
    });

    // キー監視を開始
    let _monitor_handle = key_monitor.start_monitoring();

    println!("キー監視を開始しました。以下のキーが利用可能です：");
    println!("  F2: 選択テキストを取得");
    println!("  F3: WebSocketサーバーを起動");
    println!("  F4: WebSocketクライアントを起動");
    println!("  F5: 音声録音を開始");
    println!("  F6: 音声録音を停止");

    // 音声録音の設定
    let audio_recorder = Arc::new(audio_recorder::AudioRecorder::new());
    let audio_recorder_clone = Arc::clone(&audio_recorder);
    
    // F5/F6キーのハンドラを追加
    let key_monitor_recorder = key_monitor::KeyMonitor::new(move |key| {
        match key {
            Keycode::F5 => {
                println!("F5キーが押されました - 音声録音を開始します");
                if let Err(e) = audio_recorder_clone.start_recording() {
                    println!("録音開始エラー: {}", e);
                }
            }
            Keycode::F6 => {
                println!("F6キーが押されました - 音声録音を停止します");
                audio_recorder_clone.stop_recording();
                let samples = audio_recorder_clone.get_samples();
                println!("録音サンプル数: {}", samples.len());
            }
            _ => {}
        }
    });
    
    let _recorder_monitor_handle = key_monitor_recorder.start_monitoring();

    // メインスレッドを動作させ続ける
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
