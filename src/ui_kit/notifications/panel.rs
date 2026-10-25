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

    fn render_trading_toast(
        &self,
        ui: &mut Ui,
        toast: &Toast,
        trade: &super::TradingToast,
    ) -> bool {
        use egui::{FontId, Rect, Sense, Stroke, pos2, vec2};
        let theme = ui.active_theme();
        let buy = trade.side == crate::model::OrderSide::Buy;
        let side_color = if buy { theme.ui.accent } else { theme.ui.error };
        let icon_color = if trade.executed {
            theme.ui.take_profit
        } else {
            side_color
        };
        let font = FontId::proportional(13.0);
        let mut title_format = egui::text::TextFormat::simple(font.clone(), theme.ui.text);
        title_format.coords.push("wght", 600.0);
        let title = ui
            .painter()
            .layout_job(egui::text::LayoutJob::single_section(
                toast.title.clone().unwrap_or_default(),
                title_format,
            ));
        let symbol = ui
            .painter()
            .layout_no_wrap(trade.symbol.clone(), font.clone(), theme.ui.text);
        let side_text = if theme.dark {
            side_color.lerp_to_gamma(theme.ui.text, 0.25)
        } else {
            side_color
        };
        let mut side_format = egui::text::TextFormat::simple(font.clone(), side_text);
        side_format.coords.push("wght", 600.0);
        let side = ui
            .painter()
            .layout_job(egui::text::LayoutJob::single_section(
                format!("{} {}", if buy { "Buy" } else { "Sell" }, trade.quantity),
                side_format,
            ));
        let price = ui
            .painter()
            .layout_no_wrap(format!("at {}", trade.price), font, theme.ui.text);
        let width = (62.0 + title.size().x + symbol.size().x + 44.0)
            .max(62.0 + side.size().x + price.size().x + 30.0)
            .max(self.config.width)
            .min(ui.ctx().content_rect().width() - 32.0);
        let (rect, _) = ui.allocate_exact_size(vec2(width, 66.0), Sense::hover());
        let painter = ui.painter().with_clip_rect(rect);
        painter.rect_filled(rect, 8.0, theme.chart.bg);
        let tile = Rect::from_min_size(rect.min, vec2(42.0, rect.height()));
        painter.rect_filled(
            tile,
            egui::CornerRadius {
                nw: 8,
                sw: 8,
                ne: 0,
                se: 0,
            },
            icon_color.gamma_multiply(0.14),
        );
        let c = tile.center();
        let points = if trade.executed {
            painter.circle_filled(c, 9.0, icon_color);
            [
                c + vec2(-4.0, 0.0),
                c + vec2(-1.0, 3.0),
                c + vec2(5.0, -3.5),
            ]
        } else if buy {
            [c + vec2(-6.0, 2.5), c + vec2(0.0, -2.5), c + vec2(6.0, 2.5)]
        } else {
            [
                c + vec2(-6.0, -2.5),
                c + vec2(0.0, 2.5),
                c + vec2(6.0, -2.5),
            ]
        };
        let glyph_color = if trade.executed {
            theme.chart.bg
        } else {
            icon_color
        };
        for pair in points.windows(2) {
            painter.line_segment([pair[0], pair[1]], Stroke::new(1.5, glyph_color));
        }
        for point in points {
            painter.circle_filled(point, 0.75, glyph_color);
        }
        let content = painter.with_clip_rect(Rect::from_min_max(
            rect.min,
            pos2(rect.right() - 28.0, rect.bottom()),
        ));
        let x = tile.right() + 10.0;
        let top = rect.top() + 10.0;
        let symbol_rect = Rect::from_min_size(
            pos2(x + title.size().x + 6.0, top - 2.0),
            symbol.size() + vec2(12.0, 4.0),
        );
        content.galley(pos2(x, top), title, theme.ui.text);
        content.rect_filled(symbol_rect, 10.0, theme.ui.btn_bg_hover);
        content.galley(symbol_rect.min + vec2(6.0, 2.0), symbol, theme.ui.text);
        let side_rect =
            Rect::from_min_size(pos2(x, rect.top() + 36.0), side.size() + vec2(12.0, 4.0));
        content.rect_filled(side_rect, 4.0, side_color.gamma_multiply(0.12));
        content.galley(side_rect.min + vec2(6.0, 2.0), side, side_text);
        content.galley(
            pos2(side_rect.right() + 6.0, side_rect.top() + 2.0),
            price,
            theme.ui.text,
        );
        let close = Rect::from_center_size(
            pos2(rect.right() - 14.0, rect.top() + 18.0),
            vec2(20.0, 20.0),
        );
        if toast.dismissible {
            let c = close.center();
            for d in [-1.0, 1.0] {
                painter.line_segment(
                    [c + vec2(-4.0, d * 4.0), c + vec2(4.0, -d * 4.0)],
                    Stroke::new(1.5, theme.ui.text),
                );
            }
            return ui
                .interact(
                    close,
                    ui.scope_id().with(("dismiss", toast.id)),
                    Sense::click(),
                )
                .on_hover_text("Dismiss")
                .clicked();
        }
        false
    }

    /// Render a single toast, returns true if dismissed
    fn render_toast(&self, ui: &mut Ui, toast: &Toast) -> bool {
        if let Some(trading) = &toast.trading {
            return self.render_trading_toast(ui, toast, trading);
        }
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

                // The close button first, so that a long message wraps short of it.
                ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                    if toast.dismissible {
                        dismissed = IconButton::new(&icons::CLOSE)
                            .size(ButtonSize::SM)
                            .tooltip("Dismiss")
                            .show(ui)
                            .clicked();
                    }

                    // Content
                    ui.with_layout(egui::Layout::top_down(egui::Align::LEFT), |ui| {
                        // Title, then the message muted so the title stands out
                        let mut message_color = text_color;
                        if let Some(title) = &toast.title {
                            ui.label(RichText::new(title).strong());
                            message_color = theme.ui.text_secondary;
                        }
                        ui.label(RichText::new(&toast.message).color(message_color));
                    });
                });
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

    /// Toasts stack 8 points apart from the corner, and a long message wraps short of the close
    /// button.
    #[test]
    fn toasts_stack_8_px_apart_16_px_from_the_corner() {
        let ctx = Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1000.0, 800.0));
        let mut toasts = Toasts::new();
        toasts.add(Toast::info("Chart layout saved").permanent());
        let long =
            "Order rejected: Insufficient buying power: 762450.00 USD needed, 100000.00 available";
        toasts.add(Toast::error(long).permanent());
        // A sizing pass, then a visible frame.
        let mut wrap = f32::INFINITY;
        for _ in 0..2 {
            let input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| {
                NotificationPanel::new().show_with_toasts(ui.ctx(), &mut toasts);
            });
            for shape in &output.shapes {
                if let egui::Shape::Text(text) = &shape.shape
                    && text.galley.text() == long
                {
                    wrap = text.pos.x + text.galley.job.wrap.max_width;
                }
            }
            output.drop_without_applying_deltas();
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
        // The close button sits in the toast's inner margin's corner.
        let button = newer.right() - ctx.active_theme().spacing.lg - DESIGN_TOKENS.sizing.button_sm;
        assert!(
            wrap <= button,
            "the message wraps at {wrap}, past the close button at {button}"
        );
    }
}
