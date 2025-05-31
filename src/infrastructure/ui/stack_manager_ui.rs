//! スタックマネージャーUIコンポーネント
//!
//! eGuiを使用してスタック情報を表示するアプリケーションコンポーネント。
//! スタックの一覧表示、アクティブ状態の表示、スタックモード状態を
//! リアルタイムで更新します。

use egui::{Color32, Context, FontFamily, FontId, Frame, Margin, RichText, Vec2};
use std::time::Duration;
use tokio::sync::mpsc;

use super::types::{StackDisplayInfo, UiNotification, UiState};

pub struct StackManagerApp {
    rx: mpsc::UnboundedReceiver<UiNotification>,
    state: UiState,
}

impl StackManagerApp {
    pub fn new(rx: mpsc::UnboundedReceiver<UiNotification>) -> Self {
        Self {
            rx,
            state: UiState::default(),
        }
    }

    fn handle_notification(&mut self, notification: UiNotification) {
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
                            for stack in &self.state.stacks {
                                self.render_stack_item(ui, stack);
                            }
                        });
                }
            });

        // ウィンドウサイズを内容に合わせて調整
        let desired_height = 100.0 + (self.state.stacks.len() as f32 * 60.0);
        let desired_size = Vec2::new(300.0, desired_height.min(500.0));

        ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(desired_size));
    }

    fn render_stack_item(&self, ui: &mut egui::Ui, stack: &StackDisplayInfo) {
        let bg_color = if stack.is_active {
            Color32::from_rgba_unmultiplied(100, 150, 255, 80) // アクティブスタックのハイライト
        } else {
            Color32::from_rgba_unmultiplied(60, 60, 60, 80)
        };

        let frame = Frame::none()
            .fill(bg_color)
            .rounding(4.0)
            .inner_margin(Margin::same(4.0));

        frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                // スタック番号
                ui.label(
                    RichText::new(format!("[{}]", stack.number))
                        .strong()
                        .font(FontId::new(14.0, FontFamily::Proportional)),
                );

                ui.vertical(|ui| {
                    // プレビューテキスト
                    ui.label(
                        RichText::new(&stack.preview)
                            .font(FontId::new(12.0, FontFamily::Proportional)),
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
