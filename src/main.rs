mod audio_recoder;
mod text_selection;
mod transcribe_audio;

use audio_recoder::record;
use dotenv::dotenv;
use tokio::runtime::Runtime;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // .env ファイルの読み込み
    dotenv().ok();
    println!("環境変数を読み込みました");

    // tokio ランタイムを作成して非同期処理を実行
    let rt = Runtime::new()?;

    // 録音して文字起こし
    let transcription = rt.block_on(record())?;

    // 結果をコンソールに表示
    println!("文字起こし結果: {}", transcription);
    Ok(())
}
