//! キーイベント抑制テストプログラム
//! 
//! rdev unstable_grab機能を使用してキーイベントを抑制するプロトタイプ実装

use rdev::{grab, Event, EventType, Key};
use std::io::{self, Write};
use std::sync::{Arc, Mutex};

// グローバルな修飾キー状態を追跡
static CMD_PRESSED: std::sync::OnceLock<Arc<Mutex<bool>>> = std::sync::OnceLock::new();

fn main() {
    println!("キーイベント抑制テストプログラム（rdev unstable_grab）");
    println!("==============================================");
    println!("このプログラムはキーイベントの検出と抑制を行います。");
    println!("Cmd+R、Cmd+1-9キーの組み合わせを抑制対象とします。");
    println!("抑制されたキーは他のアプリケーションに送信されません。");
    println!("終了するには Ctrl+C を押してください。");
    println!();

    // 修飾キー状態の初期化
    CMD_PRESSED.set(Arc::new(Mutex::new(false))).unwrap();

    // アクセシビリティ権限の確認
    check_accessibility_permission();

    println!("キーイベント抑制を開始中...");
    println!();

    // unstable_grab機能でキーイベントを処理
    if let Err(error) = grab(handle_key_event) {
        println!("キーイベント抑制の開始に失敗しました: {:?}", error);
        print_error_guidance();
    }
}

fn handle_key_event(event: Event) -> Option<Event> {
    let cmd_state = CMD_PRESSED.get().unwrap();
    
    match event.event_type {
        EventType::KeyPress(key) => {
            // Cmdキーの状態を更新
            if is_cmd_key(&key) {
                if let Ok(mut pressed) = cmd_state.lock() {
                    *pressed = true;
                }
            }
            
            // Cmdキーが押されているかチェックしてイベント抑制判定
            if is_cmd_pressed(cmd_state) {
                match key {
                    Key::KeyR => {
                        println!("[抑制] Cmd+R が検出されました - イベントを抑制します");
                        io::stdout().flush().unwrap();
                        trigger_voice_recording_simulation();
                        return None; // イベント抑制（ブラウザリロードを防ぐ）
                    }
                    Key::Num1 => {
                        println!("[抑制] Cmd+1 が検出されました - イベントを抑制します");
                        io::stdout().flush().unwrap();
                        trigger_stack_access_simulation(1);
                        return None; // イベント抑制（タブ切り替えを防ぐ）
                    }
                    Key::Num2 => {
                        println!("[抑制] Cmd+2 が検出されました - イベントを抑制します");
                        io::stdout().flush().unwrap();
                        trigger_stack_access_simulation(2);
                        return None;
                    }
                    Key::Num3 => {
                        println!("[抑制] Cmd+3 が検出されました - イベントを抑制します");
                        io::stdout().flush().unwrap();
                        trigger_stack_access_simulation(3);
                        return None;
                    }
                    Key::Num4 => {
                        println!("[抑制] Cmd+4 が検出されました - イベントを抑制します");
                        io::stdout().flush().unwrap();
                        trigger_stack_access_simulation(4);
                        return None;
                    }
                    Key::Num5 => {
                        println!("[抑制] Cmd+5 が検出されました - イベントを抑制します");
                        io::stdout().flush().unwrap();
                        trigger_stack_access_simulation(5);
                        return None;
                    }
                    Key::Num6 => {
                        println!("[抑制] Cmd+6 が検出されました - イベントを抑制します");
                        io::stdout().flush().unwrap();
                        trigger_stack_access_simulation(6);
                        return None;
                    }
                    Key::Num7 => {
                        println!("[抑制] Cmd+7 が検出されました - イベントを抑制します");
                        io::stdout().flush().unwrap();
                        trigger_stack_access_simulation(7);
                        return None;
                    }
                    Key::Num8 => {
                        println!("[抑制] Cmd+8 が検出されました - イベントを抑制します");
                        io::stdout().flush().unwrap();
                        trigger_stack_access_simulation(8);
                        return None;
                    }
                    Key::Num9 => {
                        println!("[抑制] Cmd+9 が検出されました - イベントを抑制します");
                        io::stdout().flush().unwrap();
                        trigger_stack_access_simulation(9);
                        return None;
                    }
                    _ => {
                        // その他のCmd+キーはパススルー
                    }
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
    
    // デフォルトはパススルー（イベントを他のアプリケーションに送信）
    Some(event)
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

// 音声録音機能のシミュレーション
fn trigger_voice_recording_simulation() {
    println!("  → 音声録音トグル機能を実行（シミュレーション）");
}

// スタックアクセス機能のシミュレーション
fn trigger_stack_access_simulation(number: u8) {
    println!("  → スタック {} アクセス機能を実行（シミュレーション）", number);
}

fn check_accessibility_permission() {
    println!("アクセシビリティ権限の確認...");
    println!("注意: このプログラムを実行するには、アクセシビリティ権限が必要です。");
    println!("権限が必要な場合は、システムダイアログが表示されます。");
    println!("システム環境設定 > セキュリティとプライバシー > アクセシビリティ");
    println!("で、このアプリケーションにアクセシビリティ権限を付与してください。");
    println!();
}

fn print_error_guidance() {
    println!();
    println!("トラブルシューティング:");
    println!("1. アクセシビリティ権限が正しく設定されているか確認");
    println!("2. macOS Monterey以降ではInput Monitoring権限も必要な場合があります");
    println!("3. Terminal.appまたはターミナルアプリに権限を付与してください");
    println!("4. 権限設定後はターミナルを再起動してください");
    println!();
}