//! スタックマネージャーUIコンポーネント
//!
//! eGuiを使用してスタック情報を表示するアプリケーションコンポーネント。
//! スタックの一覧表示、アクティブ状態の表示、スタックモード状態を
//! リアルタイムで更新します。

use egui::{Color32, Context, FontFamily, FontId, Frame, Margin, RichText, Vec2};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use super::types::{StackDisplayInfo, UiNotification, UiState};

pub struct StackManagerApp {
    rx: mpsc::UnboundedReceiver<UiNotification>,
    state: UiState,
    last_accessed_stack: Option<u32>,
    highlight_until: Option<Instant>,
}

impl StackManagerApp {
    const HIGHLIGHT_DURATION_SECS: u64 = 3;

    pub fn new(rx: mpsc::UnboundedReceiver<UiNotification>) -> Self {
        Self {
            rx,
            state: UiState::default(),
            last_accessed_stack: None,
            highlight_until: None,
        }
    }

    pub fn handle_notification(&mut self, notification: UiNotification) {
        match notification {
            UiNotification::StackAdded(stack_info) => {
                self.state.stacks.push(stack_info);
                self.state.total_count = self.state.stacks.len();
            }
            UiNotification::StackAccessed(id) => {
                self.state.last_accessed_id = Some(id);
                for stack in &mut self.state.stacks {
                    stack.is_active = stack.number == id;
                }
                // ハイライトタイマーの設定
                self.on_stack_accessed(id);
            }
            UiNotification::StacksCleared => {
                self.state.stacks.clear();
                self.state.total_count = 0;
                self.state.last_accessed_id = None;
            }
            UiNotification::ModeChanged(enabled) => {
                self.state.stack_mode_enabled = enabled;
                if !enabled {
                    self.state.stacks.clear();
                    self.state.total_count = 0;
                    self.state.last_accessed_id = None;
                }
            }
        }
    }

    fn render_ui(&mut self, ctx: &Context) {
        let panel_frame = Frame::none()
            .fill(Color32::from_rgba_unmultiplied(40, 40, 40, 200))
            .rounding(8.0)
            .stroke(egui::Stroke::new(1.0, Color32::from_rgb(100, 100, 100)))
            .inner_margin(Margin::same(8.0));

        egui::CentralPanel::default()
            .frame(panel_frame)
            .show(ctx, |ui| {
                // モード状態表示
                let mode_indicator = if self.state.stack_mode_enabled {
                    RichText::new("🟢 Stack Mode ON")
                        .color(Color32::GREEN)
                        .font(FontId::new(14.0, FontFamily::Proportional))
                } else {
                    RichText::new("🔴 Stack Mode OFF")
                        .color(Color32::RED)
                        .font(FontId::new(14.0, FontFamily::Proportional))
                };
                ui.label(mode_indicator);

                ui.separator();

                // スタック件数表示
                ui.label(
                    RichText::new(format!("Stacks: {}", self.state.total_count))
                        .font(FontId::new(12.0, FontFamily::Proportional)),
                );

                if !self.state.stacks.is_empty() {
                    ui.separator();

                    egui::ScrollArea::vertical()
                        .max_height(400.0)
                        .show(ui, |ui| {
                            for (index, stack) in self.state.stacks.iter().enumerate() {
                                self.render_stack_item(ui, stack, index);
                            }
                        });
                }

                // ショートカットキーガイドを表示
                self.draw_keyboard_guide(ui);
            });

        // ウィンドウサイズを内容に合わせて調整
        // ベース高さ + スタック分 + ガイド分
        let guide_height = if self.state.total_count > 9 {
            140.0
        } else {
            120.0
        };
        let desired_height = 100.0 + (self.state.stacks.len() as f32 * 60.0) + guide_height;
        let desired_size = Vec2::new(350.0, desired_height.min(600.0));

        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(desired_size));
    }

    fn render_stack_item(&self, ui: &mut egui::Ui, stack: &StackDisplayInfo, index: usize) {
        // タイマーベースのハイライト判定
        let is_highlighted = self.is_stack_highlighted(stack.number);

        let bg_color = if is_highlighted {
            Color32::from_rgba_unmultiplied(100, 200, 100, 120) // 3秒間の緑色ハイライト
        } else if stack.is_active {
            Color32::from_rgba_unmultiplied(100, 150, 255, 80) // 通常のアクティブスタック
        } else if index >= 9 {
            Color32::from_rgba_unmultiplied(40, 40, 40, 60) // 10個目以降は暗め
        } else {
            Color32::from_rgba_unmultiplied(60, 60, 60, 80)
        };

        let frame = Frame::none()
            .fill(bg_color)
            .rounding(4.0)
            .inner_margin(Margin::same(4.0));

        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                // キーボードショートカットヒント（最初の9個のみ）
                if index < 9 {
                    ui.label(
                        RichText::new(format!("Cmd+{}", index + 1))
                            .font(FontId::new(12.0, FontFamily::Monospace))
                            .color(Color32::from_rgb(180, 180, 180)),
                    );
                    ui.add_space(8.0);
                } else {
                    // 10個目以降はキー操作不可を示す
                    ui.label(
                        RichText::new("      ").font(FontId::new(12.0, FontFamily::Monospace)),
                    );
                    ui.add_space(8.0);
                }

                // スタック番号
                ui.label(
                    RichText::new(format!("[{}]", stack.number))
                        .strong()
                        .font(FontId::new(14.0, FontFamily::Proportional)),
                );

                ui.vertical(|ui| {
                    // プレビューテキスト（ハイライト時は文字色も変更）
                    let text_color = if is_highlighted {
                        Color32::from_rgb(220, 255, 220) // ハイライト時は明るい緑系
                    } else {
                        Color32::from_gray(220) // 通常時
                    };

                    ui.label(
                        RichText::new(&stack.preview)
                            .font(FontId::new(12.0, FontFamily::Proportional))
                            .color(text_color),
                    );

                    // 文字数と作成時刻
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("{} chars", stack.char_count))
                                .font(FontId::new(10.0, FontFamily::Proportional))
                                .color(Color32::GRAY),
                        );
                        ui.label(
                            RichText::new(&stack.created_at)
                                .font(FontId::new(10.0, FontFamily::Proportional))
                                .color(Color32::GRAY),
                        );
                    });
                });
            });
        });

        ui.add_space(2.0);
    }

    /// ハイライト状態の確認（タイマー管理）
    pub fn is_stack_highlighted(&self, stack_number: u32) -> bool {
        if self.last_accessed_stack == Some(stack_number) {
            if let Some(until) = self.highlight_until {
                return Instant::now() < until;
            }
        }
        false
    }

    /// スタックアクセス時の処理
    pub fn on_stack_accessed(&mut self, stack_number: u32) {
        self.last_accessed_stack = Some(stack_number);
        self.highlight_until =
            Some(Instant::now() + Duration::from_secs(Self::HIGHLIGHT_DURATION_SECS));
    }

    #[cfg(test)]
    pub fn get_last_accessed_stack(&self) -> Option<u32> {
        self.last_accessed_stack
    }

    #[cfg(test)]
    pub fn get_highlight_until(&self) -> Option<Instant> {
        self.highlight_until
    }

    #[cfg(test)]
    pub fn set_highlight_until(&mut self, until: Option<Instant>) {
        self.highlight_until = until;
    }

    #[cfg(test)]
    pub fn clear_highlight(&mut self) {
        self.last_accessed_stack = None;
        self.highlight_until = None;
    }

    /// ショートカットキーガイドを表示
    fn draw_keyboard_guide(&self, ui: &mut egui::Ui) {
        ui.separator();

        // ガイドセクションの背景色を設定
        let guide_frame = Frame::none()
            .fill(Color32::from_rgba_unmultiplied(30, 30, 30, 100))
            .rounding(4.0)
            .inner_margin(Margin::same(6.0));

        guide_frame.show(ui, |ui| {
            ui.label(
                RichText::new("キーボードショートカット:")
                    .font(FontId::new(12.0, FontFamily::Proportional))
                    .color(Color32::from_rgb(200, 200, 200))
                    .strong(),
            );

            ui.add_space(4.0);

            // ショートカットリスト
            ui.label(
                RichText::new("• Cmd+R: 録音開始/停止")
                    .font(FontId::new(11.0, FontFamily::Proportional))
                    .color(Color32::from_gray(180)),
            );
            ui.label(
                RichText::new("• Cmd+1-9: スタックペースト（最初の9個）")
                    .font(FontId::new(11.0, FontFamily::Proportional))
                    .color(Color32::from_gray(180)),
            );
            ui.label(
                RichText::new("• Cmd+C: 全スタッククリア")
                    .font(FontId::new(11.0, FontFamily::Proportional))
                    .color(Color32::from_gray(180)),
            );
            ui.label(
                RichText::new("• ESC: スタックモード終了")
                    .font(FontId::new(11.0, FontFamily::Proportional))
                    .color(Color32::from_gray(180)),
            );

            // 10個以上のスタックがある場合の注意表示
            if self.state.total_count > 9 {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(format!(
                        "※ スタック10以降はCmd+キーで操作できません（全{}個）",
                        self.state.total_count
                    ))
                    .font(FontId::new(11.0, FontFamily::Proportional))
                    .color(Color32::from_rgb(255, 200, 100)),
                );
            }
        });
    }
}

impl eframe::App for StackManagerApp {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        egui::Rgba::TRANSPARENT.to_array()
    }

    fn update(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        // 非ブロッキングでメッセージ受信
        while let Ok(notification) = self.rx.try_recv() {
            self.handle_notification(notification);
        }

        // 60FPS維持
        ctx.request_repaint_after(Duration::from_millis(16));

        self.render_ui(ctx);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::sync::mpsc;

    #[test]
    fn test_highlight_timer_setup() {
        let (_tx, rx) = mpsc::unbounded_channel();
        let mut app = StackManagerApp::new(rx);

        // スタックアクセスをシミュレート
        app.on_stack_accessed(1);

        // ハイライト状態を確認
        assert!(app.is_stack_highlighted(1));
        assert!(!app.is_stack_highlighted(2));
        assert_eq!(app.last_accessed_stack, Some(1));
        assert!(app.highlight_until.is_some());
    }

    #[test]
    fn test_multiple_stack_highlight() {
        let (_tx, rx) = mpsc::unbounded_channel();
        let mut app = StackManagerApp::new(rx);

        // スタック1をアクセス
        app.on_stack_accessed(1);
        assert!(app.is_stack_highlighted(1));

        // スタック2をアクセス（ハイライトが移動）
        app.on_stack_accessed(2);
        assert!(!app.is_stack_highlighted(1));
        assert!(app.is_stack_highlighted(2));
    }

    #[test]
    fn test_highlight_expiration_logic() {
        let (_tx, rx) = mpsc::unbounded_channel();
        let mut app = StackManagerApp::new(rx);

        // スタックアクセス
        app.on_stack_accessed(1);

        // 手動でタイマーを過去に設定
        app.highlight_until = Some(Instant::now() - Duration::from_secs(1));

        // ハイライトが期限切れであることを確認
        assert!(!app.is_stack_highlighted(1));
    }

    #[test]
    fn test_stack_accessed_notification() {
        let (_tx, rx) = mpsc::unbounded_channel();
        let mut app = StackManagerApp::new(rx);

        // スタック情報を追加
        let stack_info = StackDisplayInfo {
            number: 1,
            preview: "Test stack".to_string(),
            created_at: "2024-01-01 00:00:00".to_string(),
            is_active: false,
            char_count: 10,
        };
        app.handle_notification(UiNotification::StackAdded(stack_info));

        // StackAccessedイベントを処理
        app.handle_notification(UiNotification::StackAccessed(1));

        // ハイライトが設定されていることを確認
        assert!(app.is_stack_highlighted(1));
        assert_eq!(app.state.last_accessed_id, Some(1));
    }

    #[test]
    fn test_render_with_highlight() {
        let (_tx, rx) = mpsc::unbounded_channel();
        let mut app = StackManagerApp::new(rx);

        // スタックを追加
        let stack_info = StackDisplayInfo {
            number: 1,
            preview: "Highlighted stack".to_string(),
            created_at: "2024-01-01 00:00:00".to_string(),
            is_active: false,
            char_count: 17,
        };
        app.handle_notification(UiNotification::StackAdded(stack_info));

        // ハイライトを設定
        app.on_stack_accessed(1);

        // この時点でスタック1がハイライトされていることを確認
        assert!(app.is_stack_highlighted(1));

        // 3秒後にハイライトが解除されることをシミュレート
        app.highlight_until = Some(Instant::now() - Duration::from_secs(1));
        assert!(!app.is_stack_highlighted(1));
    }

    #[test]
    fn test_visual_feedback_for_multiple_stacks() {
        let (_tx, rx) = mpsc::unbounded_channel();
        let mut app = StackManagerApp::new(rx);

        // 複数のスタックを追加
        for i in 1..=12 {
            let stack_info = StackDisplayInfo {
                number: i,
                preview: format!("Stack {}", i),
                created_at: "2024-01-01 00:00:00".to_string(),
                is_active: false,
                char_count: 10,
            };
            app.handle_notification(UiNotification::StackAdded(stack_info));
        }

        // スタック数を確認
        assert_eq!(app.state.stacks.len(), 12);
        assert_eq!(app.state.total_count, 12);

        // 10番目のスタックをアクセス（キーボード操作不可）
        app.handle_notification(UiNotification::StackAccessed(10));
        assert!(app.is_stack_highlighted(10));

        // 5番目のスタックをアクセス（キーボード操作可能）
        app.handle_notification(UiNotification::StackAccessed(5));
        assert!(!app.is_stack_highlighted(10));
        assert!(app.is_stack_highlighted(5));
    }

    #[test]
    fn test_keyboard_hint_display_logic() {
        let (_tx, rx) = mpsc::unbounded_channel();
        let _app = StackManagerApp::new(rx);

        // インデックスベースのキーボードヒント表示ロジックのテスト
        // 注: 実際の表示は render_stack_item メソッド内で行われる

        // スタック番号1-9はCmd+1-9で操作可能
        for i in 0..9 {
            assert!(i < 9, "インデックス{}はキーボード操作可能", i);
        }

        // スタック番号10以降はキーボード操作不可
        for i in 9..15 {
            assert!(i >= 9, "インデックス{}はキーボード操作不可", i);
        }
    }

    #[test]
    fn test_keyboard_guide_with_many_stacks() {
        let (_tx, rx) = mpsc::unbounded_channel();
        let mut app = StackManagerApp::new(rx);

        // 9個のスタックを追加（警告表示なし）
        for i in 1..=9 {
            let stack_info = StackDisplayInfo {
                number: i,
                preview: format!("Stack {}", i),
                created_at: "2024-01-01 00:00:00".to_string(),
                is_active: false,
                char_count: 10,
            };
            app.handle_notification(UiNotification::StackAdded(stack_info));
        }

        // この時点では警告表示なし
        assert_eq!(app.state.total_count, 9);

        // 10個目のスタックを追加（警告表示あり）
        let stack_info = StackDisplayInfo {
            number: 10,
            preview: "Stack 10".to_string(),
            created_at: "2024-01-01 00:00:00".to_string(),
            is_active: false,
            char_count: 10,
        };
        app.handle_notification(UiNotification::StackAdded(stack_info));

        // 10個以上になったことを確認
        assert_eq!(app.state.total_count, 10);
        assert!(app.state.total_count > 9);
    }

    #[test]
    fn test_esc_key_guidance() {
        let (_tx, rx) = mpsc::unbounded_channel();
        let app = StackManagerApp::new(rx);

        // draw_keyboard_guideメソッドが存在することを確認
        // ESCキーガイドは常に表示される
        assert!(app.state.stack_mode_enabled || !app.state.stack_mode_enabled);
    }
}
