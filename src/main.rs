use arboard::Clipboard;
use tokio::runtime::Runtime;

mod audio_recoder;
mod request_speech_to_text;
mod text_selection;
mod transcribe_audio;

use request_speech_to_text::{start_recording, stop_recording_and_transcribe};

use device_query::{DeviceQuery, DeviceState, Keycode};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

// ここは既存の録音開始・停止処理のモジュールを使う前提

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // .env の読み込みとか、API Keyの読み込み処理
    dotenv::dotenv().ok();
    println!("環境変数を読み込みました");

    // ここは省略、API Keyなどのチェックは同じ

    // Tokio ランタイムの作成
    let rt = Runtime::new()?;

    println!("録音を開始するで...");
    rt.block_on(start_recording())?;
    println!("録音開始完了！どこでも Enter キーが押されたら録音停止するで！");

    // 停止トリガーを共有するためのフラグ
    let stop_trigger = Arc::new(Mutex::new(false));
    let stop_trigger_clone = stop_trigger.clone();

    // device_query を使ってグローバルなキー入力監視をバックグラウンドスレッドで実行
    let monitor_handle = thread::spawn(move || {
        let device_state = DeviceState::new();
        let mut last_keys = Vec::new();

        loop {
            let keys = device_state.get_keys();
            // 直前に押されてなかったキーだけを対象にする
            for key in &keys {
                if !last_keys.contains(key) {
                    println!("キーが押された: {:?}", key);
                    if *key == Keycode::Enter {
                        // Enter キーなら
                        let mut trigger = stop_trigger_clone.lock().unwrap();
                        *trigger = true;
                        println!("Enter キー検知！録音停止トリガー発動！");
                        return; // ループを抜けてスレッド終了
                    }
                }
            }
            last_keys = keys;
            thread::sleep(Duration::from_millis(10));
        }
    });

    // メインスレッドは停止トリガーになるまで待つ
    loop {
        {
            if *stop_trigger.lock().unwrap() {
                break;
            }
        }
        thread::sleep(Duration::from_millis(100));
    }

    // 監視スレッドの終了待ち
    monitor_handle.join().unwrap();

    println!("録音停止処理開始するで...");
    let transcription = rt.block_on(stop_recording_and_transcribe())?;
    println!("録音停止完了！文字起こし結果: {}", transcription);

    let mut clipboard = Clipboard::new().unwrap();
    clipboard.set_text(transcription).unwrap();
    println!("文字起こし結果をクリップボードにコピーしました");

    Ok(())
}
