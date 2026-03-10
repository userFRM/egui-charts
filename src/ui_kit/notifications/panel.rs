//! Notification Panel
//!
//! A panel for displaying toast notifications.

use egui::{Align2, Color32, Context, Order, Pos2, RichText, Ui, Vec2};

use super::toast::Toast;
use super::toasts::{Toasts, cleanup_expired_toasts, get_toasts, remove_toast};
use crate::icons::icons;
use crate::tokens::DESIGN_TOKENS;

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

    /// Get the base position for this corner
    fn base_pos(&self, screen_rect: egui::Rect) -> Pos2 {
        let margin = DESIGN_TOKENS.spacing.lg;
        match self {
            NotificationPosition::TopRight => {
                Pos2::new(screen_rect.right() - margin, screen_rect.top() + margin)
            }
            NotificationPosition::BottomRight => {
                Pos2::new(screen_rect.right() - margin, screen_rect.bottom() - margin)
            }
            NotificationPosition::BottomLeft => {
                Pos2::new(screen_rect.left() + margin, screen_rect.bottom() - margin)
            }
            NotificationPosition::TopLeft => {
                Pos2::new(screen_rect.left() + margin, screen_rect.top() + margin)
            }
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
            gap: DESIGN_TOKENS.spacing.md,
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

    /// Show the notification panel using the global toast list
    pub fn show(&self, ctx: &Context) {
        // Clean up expired toasts
        cleanup_expired_toasts();

        // Get toasts
        let toasts = get_toasts();
        if toasts.is_empty() {
            return;
        }

        // Show toasts
        let dismissed = self.show_toasts(ctx, &toasts);

        // Remove dismissed toasts
        for id in dismissed {
            remove_toast(id);
        }

        // Request repaint if there are toasts (for animations/expiry)
        ctx.request_repaint();
    }

    /// Show the notification panel with a custom toast list
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
        let screen_rect = ctx.content_rect();
        let base_pos = self.config.position.base_pos(screen_rect);
        let stack_dir = self.config.position.stack_direction();

        // Show toasts (limited to max_visible)
        let visible_toasts: Vec<_> = toasts.iter().rev().take(self.config.max_visible).collect();
        let mut y_offset = 0.0;

        for toast in visible_toasts {
            let toast_id = egui::Id::new("toast").with(toast.id);

            // Calculate position
            let pos = match self.config.position {
                NotificationPosition::TopRight | NotificationPosition::TopLeft => {
                    Pos2::new(base_pos.x, base_pos.y + y_offset)
                }
                NotificationPosition::BottomRight | NotificationPosition::BottomLeft => {
                    Pos2::new(base_pos.x, base_pos.y - y_offset)
                }
            };

            // Show toast in an Area
            let response = egui::Area::new(toast_id)
                .order(Order::Foreground)
                .anchor(self.config.position.anchor(), [0.0, 0.0])
                .fixed_pos(pos)
                .show(ctx, |ui| self.render_toast(ui, toast));

            // Track height for stacking
            y_offset += (response.response.rect.height() + self.config.gap) * stack_dir.abs();

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

        let bg_color = toast.kind.bg_color();
        let text_color = toast.kind.text_color();

        let frame = egui::Frame::new()
            .fill(bg_color)
            .corner_radius(DESIGN_TOKENS.rounding.md)
            .inner_margin(egui::Margin::same(DESIGN_TOKENS.spacing.lg as i8))
            .shadow(egui::epaint::Shadow {
                offset: [0, 2],
                blur: 8,
                spread: 0,
                color: Color32::from_black_alpha(60),
            });

        frame.show(ui, |ui| {
            ui.set_width(self.config.width);

            ui.horizontal(|ui| {
                // Icon
                let icon = toast.kind.icon();
                let icon_size = DESIGN_TOKENS.sizing.icon_md;
                ui.add(icon.as_image_tinted(Vec2::splat(icon_size), text_color));

                ui.add_space(DESIGN_TOKENS.spacing.md);

                // Content
                ui.vertical(|ui| {
                    // Title
                    if let Some(title) = &toast.title {
                        ui.label(RichText::new(title).color(text_color).strong());
                    }

                    // Message
                    ui.label(RichText::new(&toast.message).color(text_color));
                });

                // Close button
                if toast.dismissible {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                        let close_btn = ui.add(icons::CLOSE.as_image_tinted(
                            Vec2::splat(DESIGN_TOKENS.sizing.icon_sm),
                            text_color.linear_multiply(0.7),
                        ));
                        if close_btn.clicked() {
                            dismissed = true;
                        }
                    });
                }
            });

            // Progress bar (if duration > 0)
            if toast.duration > 0.0 {
                ui.add_space(DESIGN_TOKENS.spacing.sm);
                let current_time = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs_f64())
                    .unwrap_or(0.0);
                let fraction = toast.remaining_fraction(current_time);

                let (rect, _) = ui.allocate_exact_size(
                    Vec2::new(
                        self.config.width - DESIGN_TOKENS.spacing.xxl,
                        DESIGN_TOKENS.spacing.xs,
                    ),
                    egui::Sense::hover(),
                );

                // Background bar
                ui.painter()
                    .rect_filled(rect, 1.0, text_color.linear_multiply(0.2));

                // Progress bar
                let progress_rect = egui::Rect::from_min_size(
                    rect.min,
                    Vec2::new(rect.width() * fraction, rect.height()),
                );
                ui.painter()
                    .rect_filled(progress_rect, 1.0, text_color.linear_multiply(0.5));
            }
        });

        dismissed
    }
}

/// Show the global notification panel
pub fn show_notifications(ctx: &Context) {
    NotificationPanel::new().show(ctx);
}

/// Show the global notification panel at a specific position
pub fn show_notifications_at(ctx: &Context, position: NotificationPosition) {
    NotificationPanel::new().position(position).show(ctx);
}
