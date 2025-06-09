//! Enigoテキスト入力専用ヘルパープロセス
//!
//! rdevとの競合を避けるため、別プロセスでEnigo操作を実行

use enigo::{Direction::Release, Enigo, Key, Keyboard, Settings};
use std::env;
use std::process;
use voice_input::utils::config::EnvConfig;

fn main() {
    // 環境変数設定を初期化
    if let Err(e) = EnvConfig::init() {
        eprintln!("Warning: Failed to initialize EnvConfig: {}", e);
        // エラーがあっても続行（このヘルパーは独立して動作する必要があるため）
    }
    // コマンドライン引数からテキストを取得
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: enigo_helper <text>");
        process::exit(1);
    }

    let text = &args[1];

    // Enigoインスタンスを作成して即座に使用
    let settings = Settings {
        mac_delay: 20,
        ..Default::default()
    };

    match Enigo::new(&settings) {
        Ok(mut enigo) => {
            // 少し待機
            std::thread::sleep(std::time::Duration::from_millis(50));

            // Metaキーのリリース（念のため）
            let _ = enigo.key(Key::Meta, Release);

            // さらに待機
            std::thread::sleep(std::time::Duration::from_millis(30));

            // テキスト入力
            if let Err(e) = enigo.text(text) {
                eprintln!("Text input error: {}", e);
                process::exit(2);
            }

            // 完了待機
            std::thread::sleep(std::time::Duration::from_millis(30));

            // 正常終了
            process::exit(0);
        }
        Err(e) => {
            eprintln!("Enigo init error: {}", e);
            process::exit(3);
        }
    }
}
