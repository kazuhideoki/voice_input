use device_query::{DeviceQuery, DeviceState, Keycode};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

pub fn start_key_monitor() -> (Arc<Mutex<bool>>, JoinHandle<()>) {
    // 停止トリガーを共有するためのフラグ
    let stop_trigger = Arc::new(Mutex::new(false));
    let stop_trigger_clone = stop_trigger.clone();

    // device_query を使ってグローバルなキー入力監視をバックグラウンドスレッドで実行
    let monitor_handle = thread::spawn(move || {
        let device_state = DeviceState::new();

        loop {
            let keys = device_state.get_keys();

            // Altキーが押されているか確認
            let alt_pressed = keys.contains(&Keycode::LAlt) || keys.contains(&Keycode::RAlt);

            // Alt+8の組み合わせをチェック
            if alt_pressed && keys.contains(&Keycode::Key8) {
                let mut trigger = stop_trigger_clone.lock().unwrap();
                *trigger = true;
                println!("Alt+8 キー検知！録音停止トリガー発動！");
                return; // ループを抜けてスレッド終了
            }

            thread::sleep(Duration::from_millis(10));
        }
    });

    (stop_trigger, monitor_handle)
}

pub fn wait_for_stop_trigger(stop_trigger: &Arc<Mutex<bool>>) {
    // メインスレッドは停止トリガーになるまで待つ
    loop {
        {
            if *stop_trigger.lock().unwrap() {
                break;
            }
        }
        thread::sleep(Duration::from_millis(100));
    }
}
