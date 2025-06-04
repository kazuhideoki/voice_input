//! Enigoテキスト入力専用ヘルパープロセス
//! 
//! rdevとの競合を避けるため、別プロセスでEnigo操作を実行

use enigo::{Enigo, Keyboard, Settings, Direction::Release, Key};
use std::env;
use std::process;

fn main() {
    // コマンドライン引数からテキストを取得
    let args: Vec<String> = env::args().collect();
    if args.len() != 2 {
        eprintln!("Usage: enigo_helper <text>");
        process::exit(1);
    }
    
    let text = &args[1];
    
    // Enigoインスタンスを作成して即座に使用
    let mut settings = Settings::default();
    settings.mac_delay = 20;
    
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