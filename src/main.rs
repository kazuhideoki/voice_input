use arboard::Clipboard;
use tokio::runtime::Runtime;

mod audio_recoder;
mod key_monitor;
mod request_speech_to_text;
mod text_selection;
mod transcribe_audio;

use key_monitor::{start_key_monitor, wait_for_stop_trigger};
use request_speech_to_text::{start_recording, stop_recording_and_transcribe};

// ここは既存の録音開始・停止処理のモジュールを使う前提

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // .env の読み込みとか、API Keyの読み込み処理
    dotenv::dotenv().ok();
    println!("環境変数を読み込みました");

    // Tokio ランタイムの作成
    let rt = Runtime::new()?;

    println!("録音を開始するで...");
    rt.block_on(start_recording())?;
    println!("録音開始完了！どこでも Alt+8 キーが押されたら録音停止するで！");

    // キー監視の開始
    let (stop_trigger, monitor_handle) = start_key_monitor();
    
    // 停止トリガーを待つ
    wait_for_stop_trigger(&stop_trigger);

    // 監視スレッドの終了待ち
    monitor_handle.join().unwrap();

    println!("録音停止処理開始するで...");
    let transcription = rt.block_on(stop_recording_and_transcribe())?;
    println!("{}", transcription);

    let mut clipboard = Clipboard::new().unwrap();
    clipboard.set_text(transcription).unwrap();

    Ok(())
}
