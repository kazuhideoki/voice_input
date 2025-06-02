use rdev::{listen, Event, EventType, Key};
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

// グローバルな修飾キー状態を追跡（boolで簡単に管理）
static CMD_PRESSED: std::sync::OnceLock<Arc<Mutex<bool>>> = std::sync::OnceLock::new();

fn main() {
    println!("ショートカットキー動作確認プログラム（改良版）");
    println!("============================================");
    println!("このプログラムはグローバルキーイベントを検出します。");
    println!("Cmd+R、Cmd+1-9キーの組み合わせを検出対象とします。");
    println!("Cmdキーの状態を正確に追跡します。");
    println!("終了するには Ctrl+C を押してください。");
    println!();

    // 修飾キー状態の初期化
    CMD_PRESSED.set(Arc::new(Mutex::new(false))).unwrap();

    // アクセシビリティ権限の確認
    check_accessibility_permission();

    println!("キーイベント検出を開始中...");
    println!();

    // キーイベントリスナーを開始
    if let Err(error) = listen(callback) {
        println!("キーイベントリスナーの開始に失敗しました: {:?}", error);
        println!("アクセシビリティ権限が必要な可能性があります。");
        println!("システム環境設定 > セキュリティとプライバシー > アクセシビリティ");
        println!("で、このアプリケーションにアクセシビリティ権限を付与してください。");
    }
}

fn callback(event: Event) {
    let cmd_state = CMD_PRESSED.get().unwrap();
    
    match event.event_type {
        EventType::KeyPress(key) => {
            // Cmdキーの状態を更新
            if is_cmd_key(&key) {
                if let Ok(mut pressed) = cmd_state.lock() {
                    *pressed = true;
                }
            }
            
            // Cmdキーが押されているかチェック
            if is_cmd_pressed(cmd_state) {
                match key {
                    Key::KeyR => {
                        println!("[検出] Cmd+R が押されました");
                        io::stdout().flush().unwrap();
                    }
                    Key::Num1 => {
                        println!("[検出] Cmd+1 が押されました");
                        io::stdout().flush().unwrap();
                    }
                    Key::Num2 => {
                        println!("[検出] Cmd+2 が押されました");
                        io::stdout().flush().unwrap();
                    }
                    Key::Num3 => {
                        println!("[検出] Cmd+3 が押されました");
                        io::stdout().flush().unwrap();
                    }
                    Key::Num4 => {
                        println!("[検出] Cmd+4 が押されました");
                        io::stdout().flush().unwrap();
                    }
                    Key::Num5 => {
                        println!("[検出] Cmd+5 が押されました");
                        io::stdout().flush().unwrap();
                    }
                    Key::Num6 => {
                        println!("[検出] Cmd+6 が押されました");
                        io::stdout().flush().unwrap();
                    }
                    Key::Num7 => {
                        println!("[検出] Cmd+7 が押されました");
                        io::stdout().flush().unwrap();
                    }
                    Key::Num8 => {
                        println!("[検出] Cmd+8 が押されました");
                        io::stdout().flush().unwrap();
                    }
                    Key::Num9 => {
                        println!("[検出] Cmd+9 が押されました");
                        io::stdout().flush().unwrap();
                    }
                    _ => {}
                }
            }
        }
        EventType::KeyRelease(key) => {
            // Cmdキーのリリース時に状態を更新
            if is_cmd_key(&key) {
                if let Ok(mut pressed) = cmd_state.lock() {
                    *pressed = false;
                }
            }
        }
        _ => {}
    }
}

// Cmdキーかどうかをチェック
fn is_cmd_key(key: &Key) -> bool {
    matches!(key, Key::MetaLeft | Key::MetaRight)
}

// Cmdキーが押されているかチェック
fn is_cmd_pressed(cmd_state: &Arc<Mutex<bool>>) -> bool {
    if let Ok(pressed) = cmd_state.lock() {
        *pressed
    } else {
        false
    }
}

fn check_accessibility_permission() {
    println!("アクセシビリティ権限の確認...");
    
    // macOSのアクセシビリティ権限確認は通常CoreFoundationを使用しますが、
    // rdevライブラリが内部的に処理するため、ここでは基本的な情報のみ表示
    println!("注意: このプログラムを実行するには、アクセシビリティ権限が必要です。");
    println!("権限が必要な場合は、システムダイアログが表示されます。");
    println!();
}