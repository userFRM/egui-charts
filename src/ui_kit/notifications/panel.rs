//! Notification Panel
//!
//! A panel for displaying toast notifications.

use egui::{Align2, Context, Order, RichText, Ui, Vec2};

use super::toast::Toast;
use super::toasts::{Toasts, current_time_seconds};
use crate::icons::icons;
use crate::theme::{Spacing, ThemeExt};
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::buttons::{ButtonSize, IconButton};

/// Position for the notification panel
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationPosition {
    /// Top-right corner
    TopRight,
    /// Bottom-right corner (default)
    #[default]
    BottomRight,
    /// Bottom-left corner
    BottomLeft,
    /// Top-left corner
    TopLeft,
}

impl NotificationPosition {
    /// Get the anchor alignment for this position
    fn anchor(&self) -> Align2 {
        match self {
            NotificationPosition::TopRight => Align2::RIGHT_TOP,
            NotificationPosition::BottomRight => Align2::RIGHT_BOTTOM,
            NotificationPosition::BottomLeft => Align2::LEFT_BOTTOM,
            NotificationPosition::TopLeft => Align2::LEFT_TOP,
        }
    }

    /// Get the offset direction for stacking toasts
    fn stack_direction(&self) -> f32 {
        match self {
            NotificationPosition::TopRight | NotificationPosition::TopLeft => 1.0,
            NotificationPosition::BottomRight | NotificationPosition::BottomLeft => -1.0,
        }
    }
}

/// Configuration for the notification panel
pub struct NotificationPanelConfig {
    /// Position on screen
    pub position: NotificationPosition,
    /// Maximum number of visible toasts
    pub max_visible: usize,
    /// Toast width
    pub width: f32,
    /// Gap between toasts
    pub gap: f32,
}

impl Default for NotificationPanelConfig {
    fn default() -> Self {
        Self {
            position: NotificationPosition::BottomRight,
            max_visible: 5,
            width: DESIGN_TOKENS.sizing.notification.panel_width,
            gap: Spacing::default().lg,
        }
    }
}

/// A panel for displaying toast notifications
pub struct NotificationPanel {
    config: NotificationPanelConfig,
}

impl Default for NotificationPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl NotificationPanel {
    /// Create a new notification panel with default settings
    pub fn new() -> Self {
        Self {
            config: NotificationPanelConfig::default(),
        }
    }

    /// Set the position
    pub fn position(mut self, position: NotificationPosition) -> Self {
        self.config.position = position;
        self
    }

    /// Set the maximum number of visible toasts
    pub fn max_visible(mut self, max: usize) -> Self {
        self.config.max_visible = max;
        self
    }

    /// Set the toast width
    pub fn width(mut self, width: f32) -> Self {
        self.config.width = width;
        self
    }

    /// Show the notification panel with a per-instance toast list
    pub fn show_with_toasts(&self, ctx: &Context, toasts: &mut Toasts) {
        // Clean up expired toasts
        toasts.cleanup_expired();

        if toasts.is_empty() {
            return;
        }

        // Show toasts
        let dismissed = self.show_toasts(ctx, toasts.toasts());

        // Remove dismissed toasts
        for id in dismissed {
            toasts.remove(id);
        }

        // Request repaint if there are toasts
        ctx.request_repaint();
    }

    /// Internal: show toasts and return IDs of dismissed toasts
    fn show_toasts(&self, ctx: &Context, toasts: &[Toast]) -> Vec<u64> {
        let mut dismissed = Vec::new();
        let corner = self.config.position.anchor();
        let margin = ctx.active_theme().spacing.xxl;
        let base_pos = corner.pos_in_rect(&ctx.content_rect().shrink(margin));
        let stack_dir = self.config.position.stack_direction();

        // Show toasts (limited to max_visible), newest nearest the corner
        let visible_toasts: Vec<_> = toasts.iter().rev().take(self.config.max_visible).collect();
        let mut y_offset = 0.0;

        for toast in visible_toasts {
            let toast_id = egui::Id::unique("toast").with(toast.id);

            // A pivot, not an anchor: egui ignores `fixed_pos` on an anchored area.
            let pos = base_pos + Vec2::new(0.0, stack_dir * y_offset);
            let response = egui::Area::new(toast_id)
                .order(Order::Foreground)
                .pivot(corner)
                .fixed_pos(pos)
                .show(ctx, |ui| self.render_toast(ui, toast));

            // Track height for stacking
            y_offset += response.response.rect.height() + self.config.gap;

            // Check if toast was dismissed
            if response.inner {
                dismissed.push(toast.id);
            }
        }

        dismissed
    }

    /// Render a single toast, returns true if dismissed
    fn render_toast(&self, ui: &mut Ui, toast: &Toast) -> bool {
        let mut dismissed = false;

        let theme = ui.active_theme();
        let kind_color = toast.kind.color(&theme.ui);
        let text_color = theme.ui.text;

        let frame = egui::Frame::window(ui.style())
            .shadow(ui.visuals().popup_shadow)
            .inner_margin(egui::Margin::same(theme.spacing.lg as i8));

        frame.show(ui, |ui| {
            ui.set_width(self.config.width);

            ui.horizontal(|ui| {
                // Icon
                let icon = toast.kind.icon();
                let icon_size = DESIGN_TOKENS.sizing.icon_md;
                ui.add(icon.as_image_tinted(Vec2::splat(icon_size), kind_color));

                ui.add_space(DESIGN_TOKENS.spacing.md);

                // Content
                ui.vertical(|ui| {
                    // Title, then the message muted so the title stands out
                    let mut message_color = text_color;
                    if let Some(title) = &toast.title {
                        ui.label(RichText::new(title).strong());
                        message_color = theme.ui.text_secondary;
                    }
                    ui.label(RichText::new(&toast.message).color(message_color));
                });

                // Close button
                if toast.dismissible {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                        dismissed = IconButton::new(&icons::CLOSE)
                            .size(ButtonSize::SM)
                            .tooltip("Dismiss")
                            .show(ui)
                            .clicked();
                    });
                }
            });

            // Progress bar (if duration > 0)
            if toast.duration > 0.0 {
                ui.add_space(DESIGN_TOKENS.spacing.sm);
                let current_time = current_time_seconds();
                let fraction = toast.remaining_fraction(current_time);

                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), theme.spacing.xs),
                    egui::Sense::hover(),
                );

                // Background bar
                ui.painter().rect_filled(rect, 1.0, theme.ui.border);

                // Progress bar
                let progress_rect = egui::Rect::from_min_size(
                    rect.min,
                    Vec2::new(rect.width() * fraction, rect.height()),
                );
                ui.painter().rect_filled(progress_rect, 1.0, kind_color);
            }
        });

        dismissed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toasts_stack_8_px_apart_16_px_from_the_corner() {
        let ctx = Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1000.0, 800.0));
        let mut toasts = Toasts::new();
        toasts.add(Toast::info("Chart layout saved").permanent());
        toasts.add(Toast::error("Could not load symbol XYZ").permanent());
        // A sizing pass, then a visible frame.
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                NotificationPanel::new().show_with_toasts(ui.ctx(), &mut toasts);
            })
            .drop_without_applying_deltas();
        }
        let rect = |toast: &Toast| {
            ctx.memory(|m| m.area_rect(egui::Id::unique("toast").with(toast.id)))
                .unwrap()
        };
        let (older, newer) = (rect(&toasts.toasts()[0]), rect(&toasts.toasts()[1]));
        assert_eq!(
            newer.right_bottom(),
            screen.right_bottom() - Vec2::splat(16.0)
        );
        assert_eq!(newer.top() - older.bottom(), 8.0);
    }
}
