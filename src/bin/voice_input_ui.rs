//! voice_input_ui: UI専用プロセス（メインスレッドでegui EventLoopを実行）
//!
//! macOSではEventLoopをメインスレッドで実行する必要があるため、
//! UIを別プロセスとして分離し、Unix Socketを通じてデーモンと通信する。

use std::error::Error;
use tokio::net::UnixStream;
use voice_input::{
    infrastructure::ui::{
        stack_manager_ui::StackManagerApp, types::UiNotification, ui_ipc_client::UiIpcClient,
    },
    utils::config::EnvConfig,
};

fn get_screen_size() -> Option<(f32, f32)> {
    // macOSでAppleScriptを使用して画面サイズを取得
    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg("tell application \"Finder\" to get bounds of window of desktop")
        .output()
        .ok()?;

    let size_str = String::from_utf8(output.stdout).ok()?;
    let parts: Vec<&str> = size_str.trim().split(", ").collect();
    if parts.len() >= 4 {
        let width: f32 = parts[2].parse().ok()?;
        let height: f32 = parts[3].parse().ok()?;
        Some((width, height))
    } else {
        None
    }
}

fn setup_fonts(ctx: &egui::Context) {
    use egui::{FontData, FontDefinitions, FontFamily};

    let mut fonts = FontDefinitions::default();

    // システムフォントのパスを確認し、実際に存在するものを使用
    let candidates = vec![
        "/System/Library/Fonts/ヒラギノ角ゴシック W3.ttc",
        "/System/Library/Fonts/Hiragino Sans W3.ttc",
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/Arial Unicode MS.ttf",
        "/System/Library/Fonts/AppleSDGothicNeo.ttc",
    ];

    println!("Trying to load system fonts for Japanese text...");

    for font_path in candidates {
        if std::path::Path::new(font_path).exists() {
            match std::fs::read(font_path) {
                Ok(font_data) => {
                    println!("Successfully loaded font: {}", font_path);
                    fonts
                        .font_data
                        .insert("japanese_font".to_owned(), FontData::from_owned(font_data));

                    // 日本語フォントを最優先に設定
                    fonts
                        .families
                        .get_mut(&FontFamily::Proportional)
                        .unwrap()
                        .insert(0, "japanese_font".to_owned());
                    fonts
                        .families
                        .get_mut(&FontFamily::Monospace)
                        .unwrap()
                        .insert(0, "japanese_font".to_owned());

                    ctx.set_fonts(fonts);
                    return;
                }
                Err(e) => {
                    println!("Failed to read font {}: {}", font_path, e);
                }
            }
        } else {
            println!("Font not found: {}", font_path);
        }
    }

    println!("Warning: Could not load any Japanese fonts, using default fonts");
    // デフォルトフォントを設定（最後の手段）
    ctx.set_fonts(fonts);
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    // 環境変数設定を初期化
    EnvConfig::init()?;

    // Unix Socketでデーモンに接続
    let socket_path = "/tmp/voice_input_ui.sock";
    let stream = match UnixStream::connect(socket_path).await {
        Ok(stream) => stream,
        Err(e) => {
            eprintln!("Failed to connect to daemon UI socket: {}", e);
            return Err(e.into());
        }
    };

    // IPC通信クライアントを初期化
    let ipc_client = UiIpcClient::new(stream);

    // 通知受信チャネルを作成
    let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<UiNotification>();

    // IPCクライアントを別タスクで実行
    let ipc_handle = tokio::spawn(async move { ipc_client.run(tx).await });

    // 画面サイズを取得して中央下に配置
    let (screen_width, screen_height) = get_screen_size().unwrap_or((1920.0, 1080.0));
    let window_width = 300.0;
    let window_height = 200.0;
    let x_pos = (screen_width - window_width) / 2.0; // 水平中央
    let y_pos = screen_height - window_height - 100.0; // 下から100px上

    // メインスレッドでeframe/eguiを実行
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_always_on_top()
            .with_transparent(true)
            .with_decorations(false)
            .with_inner_size([window_width, window_height])
            .with_position([x_pos, y_pos])
            .with_resizable(false), // リサイズ無効化
        ..Default::default()
    };

    println!("Starting UI process...");

    // eframe::run_nativeはブロッキング実行
    if let Err(e) = eframe::run_native(
        "Stack Manager",
        native_options,
        Box::new(|cc| {
            // フォント設定をここで行う
            setup_fonts(&cc.egui_ctx);
            Box::new(StackManagerApp::new(rx))
        }),
    ) {
        eprintln!("Failed to run UI: {}", e);
        return Err(e.into());
    }

    // UIが終了したらIPCタスクもキャンセル
    ipc_handle.abort();
    Ok(())
}
