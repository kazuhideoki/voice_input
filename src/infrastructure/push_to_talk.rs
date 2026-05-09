//! macOS のグローバルキーイベントを使った push-to-talk 入力。

use std::sync::mpsc as std_mpsc;
use std::thread;
use std::time::Duration;

use tokio::sync::mpsc;

use crate::error::VoiceInputError;
use crate::utils::config::PushToTalkConfig;

const MOD_SHIFT: u64 = 0x0002_0000;
const MOD_CONTROL: u64 = 0x0004_0000;
const MOD_OPTION: u64 = 0x0008_0000;
const MOD_COMMAND: u64 = 0x0010_0000;
const MOD_FN: u64 = 0x0080_0000;
const ALL_MODIFIERS: u64 = MOD_SHIFT | MOD_CONTROL | MOD_OPTION | MOD_COMMAND | MOD_FN;

/// push-to-talk のキーイベント。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PushToTalkEvent {
    /// トリガーキーが押された。
    KeyDown,
    /// トリガーキーが離された。
    KeyUp,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Hotkey {
    key_code: u16,
    required_modifiers: u64,
}

impl Hotkey {
    fn matches_key_down(self, key_code: u16, flags: u64) -> bool {
        self.key_code == key_code && (flags & ALL_MODIFIERS) == self.required_modifiers
    }

    fn matches_active_key_up(self, key_code: u16) -> bool {
        self.key_code == key_code
    }
}

/// push-to-talk 監視スレッド。
pub struct PushToTalkMonitor {
    _thread: thread::JoinHandle<()>,
}

/// push-to-talk が有効な場合だけグローバルキーモニタを起動する。
pub fn spawn_monitor(
    config: &PushToTalkConfig,
    tx: mpsc::UnboundedSender<PushToTalkEvent>,
) -> Result<Option<PushToTalkMonitor>, VoiceInputError> {
    if !config.enabled {
        return Ok(None);
    }

    let hotkey = parse_hotkey(&config.hotkey).map_err(VoiceInputError::ConfigInitError)?;
    let hotkey_label = config.hotkey.clone();
    let (startup_tx, startup_rx) = std_mpsc::channel();
    let thread = thread::Builder::new()
        .name("voice-input-push-to-talk".to_string())
        .spawn(move || {
            if let Err(error) = platform::run_event_tap(hotkey, tx, startup_tx) {
                eprintln!("Push-to-talk monitor stopped for {hotkey_label}: {error}");
            }
        })
        .map_err(|error| {
            VoiceInputError::SystemError(format!("failed to spawn push-to-talk monitor: {error}"))
        })?;

    match startup_rx.recv_timeout(Duration::from_secs(2)) {
        Ok(Ok(())) => {}
        Ok(Err(error)) => return Err(VoiceInputError::SystemError(error)),
        Err(error) => {
            return Err(VoiceInputError::SystemError(format!(
                "push-to-talk monitor did not report startup: {error}"
            )));
        }
    }

    Ok(Some(PushToTalkMonitor { _thread: thread }))
}

fn parse_hotkey(value: &str) -> Result<Hotkey, String> {
    let mut key_code = None;
    let mut required_modifiers = 0;

    for raw_part in value.split('+') {
        let part = raw_part.trim().to_ascii_lowercase();
        if part.is_empty() {
            return Err(format!("invalid hotkey '{value}'"));
        }

        match part.as_str() {
            "shift" => required_modifiers |= MOD_SHIFT,
            "ctrl" | "control" => required_modifiers |= MOD_CONTROL,
            "opt" | "option" | "alt" => required_modifiers |= MOD_OPTION,
            "cmd" | "command" | "super" | "meta" => required_modifiers |= MOD_COMMAND,
            "fn" | "function" => required_modifiers |= MOD_FN,
            key => {
                if key_code.replace(parse_key_code(key)?).is_some() {
                    return Err(format!(
                        "hotkey '{value}' contains multiple non-modifier keys"
                    ));
                }
            }
        }
    }

    let key_code =
        key_code.ok_or_else(|| format!("hotkey '{value}' must include a non-modifier key"))?;

    Ok(Hotkey {
        key_code,
        required_modifiers,
    })
}

fn parse_key_code(value: &str) -> Result<u16, String> {
    if let Some(raw_keycode) = value.strip_prefix("keycode:") {
        return raw_keycode
            .parse::<u16>()
            .map_err(|_| format!("invalid raw keycode '{raw_keycode}'"));
    }

    key_code_for_name(value).ok_or_else(|| format!("unsupported hotkey key '{value}'"))
}

fn key_code_for_name(value: &str) -> Option<u16> {
    Some(match value {
        "a" => 0x00,
        "s" => 0x01,
        "d" => 0x02,
        "f" => 0x03,
        "h" => 0x04,
        "g" => 0x05,
        "z" => 0x06,
        "x" => 0x07,
        "c" => 0x08,
        "v" => 0x09,
        "b" => 0x0B,
        "q" => 0x0C,
        "w" => 0x0D,
        "e" => 0x0E,
        "r" => 0x0F,
        "y" => 0x10,
        "t" => 0x11,
        "1" => 0x12,
        "2" => 0x13,
        "3" => 0x14,
        "4" => 0x15,
        "6" => 0x16,
        "5" => 0x17,
        "=" | "equal" => 0x18,
        "9" => 0x19,
        "7" => 0x1A,
        "-" | "minus" => 0x1B,
        "8" => 0x1C,
        "0" => 0x1D,
        "]" | "rightbracket" | "right_bracket" => 0x1E,
        "o" => 0x1F,
        "u" => 0x20,
        "[" | "leftbracket" | "left_bracket" => 0x21,
        "i" => 0x22,
        "p" => 0x23,
        "return" | "enter" => 0x24,
        "l" => 0x25,
        "j" => 0x26,
        "'" | "quote" => 0x27,
        "k" => 0x28,
        ";" | "semicolon" => 0x29,
        "\\" | "backslash" => 0x2A,
        "," | "comma" => 0x2B,
        "/" | "slash" => 0x2C,
        "n" => 0x2D,
        "m" => 0x2E,
        "." | "period" => 0x2F,
        "tab" => 0x30,
        "space" => 0x31,
        "`" | "grave" | "backtick" => 0x32,
        "delete" | "backspace" => 0x33,
        "esc" | "escape" => 0x35,
        "f17" => 0x40,
        "volumeup" | "volume_up" => 0x48,
        "volumedown" | "volume_down" => 0x49,
        "mute" => 0x4A,
        "f18" => 0x4F,
        "f19" => 0x50,
        "f20" => 0x5A,
        "f5" => 0x60,
        "f6" => 0x61,
        "f7" => 0x62,
        "f3" => 0x63,
        "f8" => 0x64,
        "f9" => 0x65,
        "eisu" => 0x66,
        "f11" => 0x67,
        "kana" => 0x68,
        "f13" => 0x69,
        "f16" => 0x6A,
        "f14" => 0x6B,
        "f10" => 0x6D,
        "f12" => 0x6F,
        "f15" => 0x71,
        "help" => 0x72,
        "home" => 0x73,
        "pageup" | "page_up" => 0x74,
        "forwarddelete" | "forward_delete" => 0x75,
        "f4" => 0x76,
        "end" => 0x77,
        "f2" => 0x78,
        "pagedown" | "page_down" => 0x79,
        "f1" => 0x7A,
        "left" | "leftarrow" | "left_arrow" => 0x7B,
        "right" | "rightarrow" | "right_arrow" => 0x7C,
        "down" | "downarrow" | "down_arrow" => 0x7D,
        "up" | "uparrow" | "up_arrow" => 0x7E,
        _ => return None,
    })
}

#[cfg(target_os = "macos")]
mod platform {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    use core_foundation::runloop::CFRunLoop;
    use core_graphics::event::{
        CGEvent, CGEventFlags, CGEventTap, CGEventTapLocation, CGEventTapOptions,
        CGEventTapPlacement, CGEventType, CallbackResult, EventField,
    };
    use tokio::sync::mpsc;

    use super::{ALL_MODIFIERS, Hotkey, PushToTalkEvent};

    pub fn run_event_tap(
        hotkey: Hotkey,
        tx: mpsc::UnboundedSender<PushToTalkEvent>,
        startup_tx: std::sync::mpsc::Sender<Result<(), String>>,
    ) -> Result<(), String> {
        let active = Arc::new(AtomicBool::new(false));
        let callback_active = active.clone();
        let mut startup_tx = Some(startup_tx);

        let result = CGEventTap::with_enabled(
            CGEventTapLocation::HID,
            CGEventTapPlacement::HeadInsertEventTap,
            CGEventTapOptions::Default,
            vec![CGEventType::KeyDown, CGEventType::KeyUp],
            move |_proxy, event_type, event| {
                handle_event(hotkey, &tx, callback_active.as_ref(), event_type, event)
            },
            || {
                if let Some(startup_tx) = startup_tx.take() {
                    let _ = startup_tx.send(Ok(()));
                }
                CFRunLoop::run_current()
            },
        );

        if result.is_err() {
            let error = "failed to install macOS event tap; grant Accessibility/Input Monitoring permission to VoiceInput.app".to_string();
            if let Some(startup_tx) = startup_tx.take() {
                let _ = startup_tx.send(Err(error.clone()));
            }
            return Err(error);
        }

        Ok(())
    }

    fn handle_event(
        hotkey: Hotkey,
        tx: &mpsc::UnboundedSender<PushToTalkEvent>,
        active: &AtomicBool,
        event_type: CGEventType,
        event: &CGEvent,
    ) -> CallbackResult {
        let key_code = event.get_integer_value_field(EventField::KEYBOARD_EVENT_KEYCODE) as u16;

        match event_type {
            CGEventType::KeyDown => {
                let flags = relevant_flags(event.get_flags());
                if !hotkey.matches_key_down(key_code, flags) {
                    return CallbackResult::Keep;
                }

                let is_repeat =
                    event.get_integer_value_field(EventField::KEYBOARD_EVENT_AUTOREPEAT) != 0;
                if !is_repeat && !active.swap(true, Ordering::SeqCst) {
                    let _ = tx.send(PushToTalkEvent::KeyDown);
                }
                CallbackResult::Drop
            }
            CGEventType::KeyUp => {
                if !active.load(Ordering::SeqCst) || !hotkey.matches_active_key_up(key_code) {
                    return CallbackResult::Keep;
                }

                active.store(false, Ordering::SeqCst);
                let _ = tx.send(PushToTalkEvent::KeyUp);
                CallbackResult::Drop
            }
            _ => CallbackResult::Keep,
        }
    }

    fn relevant_flags(flags: CGEventFlags) -> u64 {
        flags.bits() & ALL_MODIFIERS
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    use tokio::sync::mpsc;

    use super::{Hotkey, PushToTalkEvent};

    pub fn run_event_tap(
        _hotkey: Hotkey,
        _tx: mpsc::UnboundedSender<PushToTalkEvent>,
        startup_tx: std::sync::mpsc::Sender<Result<(), String>>,
    ) -> Result<(), String> {
        let error = "push-to-talk is only supported on macOS".to_string();
        let _ = startup_tx.send(Err(error.clone()));
        Err(error)
    }
}

#[cfg(test)]
mod tests {
    use super::{MOD_COMMAND, MOD_CONTROL, MOD_OPTION, MOD_SHIFT, parse_hotkey};

    /// opt+8 は ANSI 8 キーと option 修飾として解釈される
    #[test]
    fn opt_8_hotkey_is_parsed() {
        let hotkey = parse_hotkey("opt+8").unwrap();

        assert_eq!(hotkey.key_code, 0x1C);
        assert_eq!(hotkey.required_modifiers, MOD_OPTION);
    }

    /// 修飾キー名は一般的な別名を受け付ける
    #[test]
    fn modifier_aliases_are_parsed() {
        let hotkey = parse_hotkey("control+shift+command+v").unwrap();

        assert_eq!(hotkey.key_code, 0x09);
        assert_eq!(
            hotkey.required_modifiers,
            MOD_CONTROL | MOD_SHIFT | MOD_COMMAND
        );
    }

    /// raw keycode 指定は配列差分の逃げ道として利用できる
    #[test]
    fn raw_keycode_hotkey_is_parsed() {
        let hotkey = parse_hotkey("alt+keycode:28").unwrap();

        assert_eq!(hotkey.key_code, 28);
        assert_eq!(hotkey.required_modifiers, MOD_OPTION);
    }

    /// 修飾キーだけの指定は拒否される
    #[test]
    fn hotkey_without_non_modifier_key_is_rejected() {
        let error = parse_hotkey("opt+shift").unwrap_err();

        assert!(error.contains("must include a non-modifier key"));
    }
}
