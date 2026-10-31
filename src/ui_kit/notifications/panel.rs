//! Notification Panel
//!
//! A panel for displaying toast notifications.

use crate::localization as l10n;
use egui::accesskit::{Live, Role};
use egui::{Align2, Context, Order, RichText, Ui, Vec2};

use super::toast::{Toast, ToastKind};
use super::toasts::{Toasts, current_time_seconds};
use crate::icons::icons;
use crate::theme::{Spacing, ThemeExt};
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::buttons::{Button, ButtonSize, IconButton};

/// Position for the notification panel
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotificationPosition {
    /// Top-right corner
    TopRight,
    /// Bottom-right corner
    BottomRight,
    /// Bottom-left corner (default)
    #[default]
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
    /// Maximum number of toasts shown when the stack is expanded, or when only one toast is queued
    pub max_visible: usize,
    /// Toast width
    pub width: f32,
    /// Gap between toasts
    pub gap: f32,
}

impl Default for NotificationPanelConfig {
    fn default() -> Self {
        Self {
            position: NotificationPosition::default(),
            max_visible: 5,
            width: DESIGN_TOKENS.sizing.notification.panel_width,
            gap: Spacing::default().lg,
        }
    }
}

/// A control of the collapsed or expanded stack that the user pressed this frame.
enum StackAction {
    Toggle,
    DismissAll,
}

/// A panel for displaying toast notifications
///
/// Two or more toasts collapse into a stack showing the newest; a pill expands it and a button
/// beside it dismisses every dismissible toast.
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

    /// Set the maximum number of toasts shown when the stack is expanded, or when only one toast
    /// is queued
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

        if toasts.len() < 2 {
            toasts.expanded = false;
        }

        if toasts.is_empty() {
            return;
        }

        // Show toasts
        let (dismissed, action) = self.show_toasts(ctx, toasts.toasts(), toasts.expanded);

        // Remove dismissed toasts
        for id in dismissed {
            toasts.remove(id);
        }

        match action {
            Some(StackAction::Toggle) => toasts.expanded = !toasts.expanded,
            Some(StackAction::DismissAll) => toasts.toasts_mut().retain(|t| !t.dismissible),
            None => {}
        }

        if toasts.len() < 2 {
            toasts.expanded = false;
        }

        // Request repaint if there are toasts
        ctx.request_repaint();
    }

    /// Internal: show toasts and return IDs of dismissed toasts and the stack control pressed
    fn show_toasts(
        &self,
        ctx: &Context,
        toasts: &[Toast],
        expanded: bool,
    ) -> (Vec<u64>, Option<StackAction>) {
        let mut dismissed = Vec::new();
        let corner = self.config.position.anchor();
        let margin = ctx.active_theme().spacing.xxl;
        let base_pos = corner.pos_in_rect(&ctx.content_rect().shrink(margin));
        let stack_dir = self.config.position.stack_direction();

        let stacked = toasts.len() >= 2 && self.config.max_visible >= 2;
        let collapsed = stacked && !expanded;
        let (shown, layers) = if collapsed {
            (1, (toasts.len() - 1).min(2))
        } else {
            (self.config.max_visible, 0)
        };

        // Show toasts (limited to `shown`), newest nearest the corner
        let visible_toasts: Vec<_> = toasts.iter().rev().take(shown).collect();
        let mut y_offset = 0.0;

        for toast in visible_toasts {
            let toast_id = egui::Id::unique("toast").with(toast.id);

            // A pivot, not an anchor: egui ignores `fixed_pos` on an anchored area.
            let pos = base_pos + Vec2::new(0.0, stack_dir * y_offset);
            let response = egui::Area::new(toast_id)
                .order(Order::Foreground)
                .pivot(corner)
                .fixed_pos(pos)
                .show(ctx, |ui| {
                    // Reserved first, so that the edges of the older toasts sit under the card.
                    let edges = ui.painter().add(egui::Shape::Noop);
                    let dismissed = self.render_toast(ui, toast);
                    let card = ui.min_rect();
                    let spacing = ui.active_theme().spacing;
                    let visuals = ui.visuals();
                    let shapes = (1..=layers).rev().map(|layer| {
                        let layer = layer as f32;
                        egui::epaint::RectShape::new(
                            card.shrink2(Vec2::new(layer * spacing.lg, 0.0))
                                .translate(Vec2::new(0.0, -stack_dir * layer * spacing.sm)),
                            visuals.window_corner_radius,
                            visuals.window_fill,
                            visuals.window_stroke,
                            egui::StrokeKind::Inside,
                        )
                        .into()
                    });
                    ui.painter().set(edges, egui::Shape::Vec(shapes.collect()));
                    dismissed
                });

            // One live node per toast, so a screen reader announces it once when it appears.
            let error = toast.kind == ToastKind::Error;
            let text = toast
                .title
                .iter()
                .chain([&toast.message])
                .map(|text| l10n::text(ctx, text).to_string())
                .collect::<Vec<_>>()
                .join(". ");
            ctx.accesskit_node_builder(response.response.id, |node| {
                node.set_role(if error { Role::Alert } else { Role::Status });
                node.set_label(text);
                node.set_live(if error { Live::Assertive } else { Live::Polite });
            });

            // Track height for stacking
            y_offset += response.response.rect.height() + self.config.gap;

            // Check if toast was dismissed
            if response.inner {
                dismissed.push(toast.id);
            }
        }

        let action = stacked.then(|| {
            let pos = base_pos + Vec2::new(0.0, stack_dir * y_offset);
            self.show_stack_controls(ctx, corner, pos, toasts, expanded)
        });

        (dismissed, action.flatten())
    }

    /// Internal: show the pill and the dismiss-all button beyond the stack, on the side away from
    /// the corner, and return the one pressed
    fn show_stack_controls(
        &self,
        ctx: &Context,
        corner: Align2,
        pos: egui::Pos2,
        toasts: &[Toast],
        expanded: bool,
    ) -> Option<StackAction> {
        egui::Area::new(egui::Id::unique("toast_stack").with(corner))
            .order(Order::Foreground)
            .pivot(corner)
            .fixed_pos(pos)
            .show(ctx, |ui| {
                let spacing = ui.active_theme().spacing;
                let visuals = ui.visuals();
                let frame = egui::Frame::NONE
                    .fill(visuals.window_fill)
                    .stroke(visuals.window_stroke)
                    .shadow(visuals.popup_shadow)
                    .corner_radius(ButtonSize::SM.pixels() / 2.0 + spacing.xs)
                    .inner_margin(egui::Margin::same(spacing.xs as i8));
                frame
                    .show(ui, |ui| {
                        ui.spacing_mut().item_spacing.x = spacing.xs;
                        ui.horizontal(|ui| {
                            let label = if expanded {
                                "Show less".to_owned()
                            } else {
                                let hidden = (toasts.len() - 1).to_string();
                                l10n::format_text(ui.ctx(), "Show more {0}", &[("0", hidden)])
                            };
                            let pill = Button::new(label).size(ButtonSize::SM).show(ui);
                            ui.ctx().accesskit_node_builder(pill.id, |node| {
                                node.set_expanded(expanded)
                            });
                            let dismiss_all = toasts.iter().any(|t| t.dismissible)
                                && IconButton::new(&icons::CLOSE)
                                    .size(ButtonSize::SM)
                                    .tooltip("Dismiss all")
                                    .show(ui)
                                    .clicked();
                            if pill.clicked() {
                                Some(StackAction::Toggle)
                            } else if dismiss_all {
                                Some(StackAction::DismissAll)
                            } else {
                                None
                            }
                        })
                        .inner
                    })
                    .inner
            })
            .inner
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
        let side_color = crate::theme::trading_side_color(&theme, trade.side);
        let icon_color = if trade.executed {
            theme.ui.take_profit
        } else {
            side_color
        };
        let font = FontId::proportional(ui.active_theme().text.title);
        let mut title_job = egui::text::LayoutJob::single_section(
            l10n::text(ui.ctx(), toast.title.clone().unwrap_or_default()),
            egui::text::TextFormat::simple(FontId::proportional(theme.text.heading), theme.ui.text),
        );
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
                l10n::text(
                    ui.ctx(),
                    format!("{} {}", if buy { "Buy" } else { "Sell" }, trade.quantity),
                ),
                side_format,
            ));
        let price = ui.painter().layout_no_wrap(
            l10n::format_text(ui.ctx(), "at {0}", &[("0", trade.price.to_string())]),
            font,
            theme.ui.text_secondary,
        );
        let width = self
            .config
            .width
            .min(ui.ctx().content_rect().width() - 32.0)
            .floor();
        let content_width = (width - 80.0).max(1.0);
        title_job.wrap.max_width = content_width;
        let title = ui.painter().layout_job(title_job);
        let symbol = ui.painter().layout(
            trade.symbol.clone(),
            FontId::proportional(ui.active_theme().text.title),
            theme.ui.text,
            (content_width - 12.0).max(1.0),
        );
        let symbol_at = vec2(0.0, title.size().y + 4.0);
        let second_row = symbol_at.y + symbol.size().y + 4.0 + 12.0;
        let price_at = if side.size().x + price.size().x + 18.0 <= content_width {
            vec2(side.size().x + 18.0, second_row + 2.0)
        } else {
            vec2(0.0, second_row + side.size().y + 10.0)
        };
        let price = ui.painter().layout(
            l10n::format_text(ui.ctx(), "at {0}", &[("0", trade.price.to_string())]),
            FontId::proportional(ui.active_theme().text.title),
            theme.ui.text_secondary,
            (content_width - price_at.x).max(1.0),
        );
        let height = (price_at.y + price.size().y + 24.0).max(72.0);
        let (rect, response) = ui.allocate_exact_size(vec2(width, height.ceil()), Sense::hover());
        // The text is painted, so name the card for screen readers.
        let message = l10n::text(ui.ctx(), &toast.message);
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, &message));
        let painter = ui.painter().with_clip_rect(rect);
        let radius = theme.radius.lg;
        painter.rect_filled(rect, radius, theme.ui.panel_bg_floating);
        let tile = Rect::from_min_size(rect.min, vec2(42.0, rect.height()));
        painter.rect_filled(
            tile,
            egui::CornerRadius {
                nw: radius as u8,
                sw: radius as u8,
                ne: 0,
                se: 0,
            },
            icon_color.gamma_multiply(0.14),
        );
        painter.rect_stroke(
            rect,
            radius,
            Stroke::new(1.0, theme.ui.border),
            egui::StrokeKind::Inside,
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
        let weight = if trade.executed { 1.5 } else { 4.0 };
        for pair in points.windows(2) {
            painter.line_segment([pair[0], pair[1]], Stroke::new(weight, glyph_color));
        }
        for point in points {
            painter.circle_filled(point, weight / 2.0, glyph_color);
        }
        let content = painter.with_clip_rect(Rect::from_min_max(
            rect.min,
            pos2(rect.right() - 28.0, rect.bottom()),
        ));
        let x = tile.right() + 10.0;
        let top = rect.top() + 12.0;
        let symbol_rect =
            Rect::from_min_size(pos2(x, top) + symbol_at, symbol.size() + vec2(12.0, 4.0));
        content.galley(pos2(x, top), title, theme.ui.text);
        content.rect_filled(symbol_rect, 10.0, theme.ui.btn_bg_hover);
        content.galley(symbol_rect.min + vec2(6.0, 2.0), symbol, theme.ui.text);
        let side_rect =
            Rect::from_min_size(pos2(x, top + second_row), side.size() + vec2(12.0, 4.0));
        content.rect_filled(side_rect, 4.0, side_color.gamma_multiply(0.12));
        content.galley(side_rect.min + vec2(6.0, 2.0), side, side_text);
        content.galley(pos2(x, top) + price_at, price, theme.ui.text_secondary);
        if toast.dismissible {
            let close = Rect::from_center_size(
                pos2(rect.right() - 14.0, rect.top() + 18.0),
                vec2(20.0, 20.0),
            );
            return ui
                .scope_builder(egui::UiBuilder::new().max_rect(close), |ui| {
                    IconButton::new(&icons::CLOSE)
                        .size(ButtonSize::SM)
                        .tooltip("Dismiss")
                        .show(ui)
                        .clicked()
                })
                .inner;
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
            ui.set_width(
                self.config.width.min(
                    (ui.ctx().content_rect().width()
                        - 2.0 * (theme.spacing.xxl + theme.spacing.lg)
                        - 2.0)
                        .max(1.0),
                ),
            );

            ui.horizontal(|ui| {
                // Icon
                let icon = toast.kind.icon();
                let icon_size = DESIGN_TOKENS.sizing.icon_md;
                ui.add(icon.as_image_tinted(Vec2::splat(icon_size), kind_color));

                ui.add_space(ui.active_theme().spacing.md);

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
                            ui.label(RichText::new(l10n::text(ui.ctx(), title)).strong());
                            message_color = theme.ui.text_secondary;
                        }
                        ui.label(
                            RichText::new(l10n::text(ui.ctx(), &toast.message))
                                .color(message_color),
                        );
                    });
                });
            });

            // Progress bar (if duration > 0)
            if toast.duration > 0.0 {
                ui.add_space(ui.active_theme().spacing.sm);
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

    /// A trading notice is a card of its own over whatever is behind it, with a border, and its
    /// title is at regular weight.
    #[test]
    fn trading_notices_are_bordered_cards_with_regular_titles() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let ctx = Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut toast = Toast::info("Limit order placed").with_title("Limit order placed on");
            toast.trading = Some(super::super::TradingToast {
                symbol: "SYM".into(),
                side: crate::model::OrderSide::Buy,
                quantity: 1.0,
                price: "100.00".into(),
                executed: false,
            });
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                NotificationPanel::new().render_toast(ui, &toast);
            });
            let title = output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == "Limit order placed on" => {
                        Some(text)
                    }
                    _ => None,
                })
                .expect("the title");
            assert_eq!(
                title.galley.job.sections[0].format.coords,
                Default::default()
            );
            let card = |check: &dyn Fn(&egui::epaint::RectShape) -> bool| {
                output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Rect(rect)
                        if rect.rect.contains(title.pos) && check(rect))
                })
            };
            assert!(card(&|rect| rect.fill == theme.ui.panel_bg_floating));
            assert!(card(&|rect| rect.stroke.color == theme.ui.border));
            output.drop_without_applying_deltas();
        }
    }

    /// Four notices show one card and a pill for the other three. Hidden notices keep their own
    /// lifetimes, the pill expands the stack and reports that it did, dismiss-all leaves a notice
    /// that cannot be dismissed, and a single notice has no controls.
    #[test]
    fn four_notices_collapse_behind_a_pill_that_expands_and_dismisses_them() {
        use egui::accesskit::Role;
        use egui_kittest::kittest::{NodeT, Queryable};
        let mut toasts = Toasts::new();
        for message in ["A", "B", "C"] {
            toasts.add(Toast::info(message));
        }
        let mut newest =
            Toast::info("Placed: Buy 1 AAPL at 100.00").with_title("Limit order placed on");
        newest.trading = Some(super::super::TradingToast {
            symbol: "AAPL".into(),
            side: crate::model::OrderSide::Buy,
            quantity: 1.0,
            price: "100.00".into(),
            executed: false,
        });
        toasts.add(newest);
        let mut harness = egui_kittest::Harness::builder()
            .with_context_setup(|ctx| {
                egui_extras::install_image_loaders(ctx);
                crate::theme::apply_to_egui(ctx, &crate::theme::Theme::dark());
            })
            .with_size(Vec2::new(1440.0, 900.0))
            .build_ui_state(
                |ui, toasts: &mut Toasts| {
                    NotificationPanel::new().show_with_toasts(ui.ctx(), toasts);
                },
                toasts,
            );
        // The panel repaints while notices are up, so the harness never settles by itself.
        harness.run_steps(3);
        let pill = harness.get_by_role_and_label(Role::Button, "Show more 3");
        assert_eq!(pill.accesskit_node().data().is_expanded(), Some(false));
        harness.get_by_role_and_label(Role::Label, "Placed: Buy 1 AAPL at 100.00");
        for hidden in ["A", "B", "C"] {
            assert!(
                harness.query_by_label(hidden).is_none(),
                "{hidden} is hidden"
            );
        }

        harness.state_mut().toasts_mut()[0].created_at -= 10.0;
        harness.run_steps(3);
        harness
            .get_by_role_and_label(Role::Button, "Show more 2")
            .click();
        harness.run_steps(3);
        let pill = harness.get_by_role_and_label(Role::Button, "Show less");
        assert_eq!(pill.accesskit_node().data().is_expanded(), Some(true));
        for shown in ["B", "C"] {
            harness.get_by_role_and_label(Role::Label, shown);
        }

        harness
            .state_mut()
            .add(Toast::info("Kept").permanent().dismissible(false));
        harness.run_steps(3);
        harness
            .get_by_role_and_label(Role::Button, "Dismiss all")
            .click();
        harness.run_steps(3);
        assert_eq!(harness.state().len(), 1);
        assert_eq!(harness.state().toasts()[0].message, "Kept");
        for gone in ["Show less", "Show more 0", "Dismiss all"] {
            assert!(
                harness
                    .query_by_role_and_label(Role::Button, gone)
                    .is_none(),
                "{gone} is gone"
            );
        }
    }

    /// Each toast is one live node named by its title and message, polite except for errors, and
    /// the node goes with the toast when it is dismissed.
    #[test]
    fn each_toast_is_a_live_region_that_goes_with_the_toast() {
        use egui::accesskit::Live;
        use egui_kittest::kittest::{NodeT, Queryable};
        let cases = [
            (
                Toast::info("Body").with_title("Title"),
                "Title. Body",
                Live::Polite,
            ),
            (Toast::success("Order filled"), "Order filled", Live::Polite),
            (Toast::warning("Low margin"), "Low margin", Live::Polite),
            (
                Toast::error("Order rejected"),
                "Order rejected",
                Live::Assertive,
            ),
        ];
        for (toast, label, live) in cases {
            let mut toasts = Toasts::new();
            toasts.add(toast.permanent());
            let mut harness = egui_kittest::Harness::builder()
                .with_context_setup(|ctx| {
                    egui_extras::install_image_loaders(ctx);
                    crate::theme::apply_to_egui(ctx, &crate::theme::Theme::dark());
                })
                .with_size(Vec2::new(1440.0, 900.0))
                .build_ui_state(
                    |ui, toasts: &mut Toasts| {
                        NotificationPanel::new().show_with_toasts(ui.ctx(), toasts);
                    },
                    toasts,
                );
            harness.run_steps(3);
            let announced = |harness: &egui_kittest::Harness<'_, Toasts>| {
                harness
                    .query_all_by_label(label)
                    .filter_map(|node| node.accesskit_node().data().live())
                    .collect::<Vec<_>>()
            };
            assert_eq!(announced(&harness), [live], "{label}");

            harness.state_mut().clear();
            harness.run_steps(3);
            assert!(announced(&harness).is_empty(), "{label} is gone");
        }
    }

    /// Toasts stack from the corner without moving the newest between the two states: expanded 8
    /// points apart, collapsed with the controls 8 points beyond the card and the edge of the older
    /// toast behind it, all on screen. A long message wraps short of the close button.
    #[test]
    fn toasts_stack_from_the_corner_collapsed_and_expanded() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for (width, panel_width) in [(1000.0, 340.0), (640.0, 200.0), (320.0, 340.0)] {
                for (position, corner, direction) in [
                    (None, Align2::LEFT_BOTTOM, -1.0),
                    (
                        Some(NotificationPosition::BottomRight),
                        Align2::RIGHT_BOTTOM,
                        -1.0,
                    ),
                    (Some(NotificationPosition::TopLeft), Align2::LEFT_TOP, 1.0),
                    (Some(NotificationPosition::TopRight), Align2::RIGHT_TOP, 1.0),
                ] {
                    for (trading, expanded) in
                        [(false, false), (false, true), (true, false), (true, true)]
                    {
                        let ctx = Context::default();
                        crate::theme::apply_to_egui(&ctx, &theme);
                        let screen =
                            egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(width, 800.0));
                        let mut toasts = Toasts::new();
                        toasts.add(Toast::info("Chart layout saved").permanent());
                        let long = "Order rejected: Insufficient buying power: 762450.00 USD needed, 100000.00 available";
                        let title = "Limit order partially executed (1 of 3) on";
                        let mut toast = Toast::error(long).permanent();
                        if trading {
                            toast = toast.with_title(title);
                            toast.trading = Some(super::super::TradingToast {
                                symbol: "LONG:SYMBOL".into(),
                                side: crate::model::OrderSide::Buy,
                                quantity: 0.5,
                                price: "123456789.00".into(),
                                executed: true,
                            });
                        }
                        toasts.add(toast);
                        toasts.expanded = expanded;
                        let mut wrap = f32::INFINITY;
                        let edge = |newer: egui::Rect| {
                            newer
                                .shrink2(Vec2::new(8.0, 0.0))
                                .translate(Vec2::new(0.0, -direction * 4.0))
                        };
                        let mut edge_drawn = false;
                        for pass in 0..3 {
                            let output = ctx.run_ui(
                                egui::RawInput {
                                    screen_rect: Some(screen),
                                    ..Default::default()
                                },
                                |ui| {
                                    let panel = NotificationPanel::new().width(panel_width);
                                    let panel = position.map_or(panel, |position| {
                                        NotificationPanel::new()
                                            .width(panel_width)
                                            .position(position)
                                    });
                                    panel.show_with_toasts(ui.ctx(), &mut toasts);
                                },
                            );
                            for shape in &output.shapes {
                                if let egui::Shape::Text(text) = &shape.shape {
                                    if text.galley.text() == long {
                                        wrap = text.pos.x + text.galley.job.wrap.max_width;
                                    }
                                    if pass == 2 {
                                        let bounds = text.galley.rect.translate(text.pos.to_vec2());
                                        assert!(
                                            shape.clip_rect.expand(0.5).contains_rect(bounds),
                                            "text is fully visible at {width}: {}",
                                            text.galley.text()
                                        );
                                    }
                                }
                            }
                            if trading && pass == 2 {
                                assert!(output.shapes.iter().any(|shape| {
                                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == title)
                            }), "partial-fill progress remains visible");
                            }
                            if pass == 2 {
                                let newer = ctx
                                    .memory(|m| {
                                        m.area_rect(
                                            egui::Id::unique("toast").with(toasts.toasts()[1].id),
                                        )
                                    })
                                    .unwrap();
                                edge_drawn = output.shapes.iter().any(|shape| {
                                    matches!(&shape.shape, egui::Shape::Vec(shapes) if shapes.iter().any(|shape| {
                                        matches!(shape, egui::Shape::Rect(rect) if rect.rect == edge(newer))
                                    }))
                                });
                            }
                            output.drop_without_applying_deltas();
                        }
                        let rect = |toast: &Toast| {
                            ctx.memory(|m| m.area_rect(egui::Id::unique("toast").with(toast.id)))
                        };
                        let (older, newer) = (rect(&toasts.toasts()[0]), rect(&toasts.toasts()[1]));
                        let newer = newer.unwrap();
                        assert_eq!(
                            corner.pos_in_rect(&newer),
                            corner.pos_in_rect(&screen.shrink(16.0))
                        );
                        let beyond = |far: egui::Rect| {
                            if direction < 0.0 {
                                newer.top() - far.bottom()
                            } else {
                                far.top() - newer.bottom()
                            }
                        };
                        assert_eq!(edge_drawn, !expanded);
                        if expanded {
                            assert_eq!(beyond(older.unwrap()), 8.0);
                        } else {
                            assert_eq!(older, None);
                            let controls = ctx
                                .memory(|m| {
                                    m.area_rect(egui::Id::unique("toast_stack").with(corner))
                                })
                                .unwrap();
                            assert_eq!(beyond(controls), 8.0);
                            assert!(screen.shrink(15.0).contains_rect(controls));
                            assert!(
                                (corner.pos_in_rect(&controls).x - corner.pos_in_rect(&newer).x)
                                    .abs()
                                    < 1.0
                            );
                            assert!(screen.shrink(12.0).contains_rect(edge(newer)));
                        }
                        assert!(screen.shrink(15.0).contains_rect(newer));
                        if trading {
                            assert!(newer.width() <= panel_width);
                        }
                        if !trading {
                            let button = newer.right()
                                - ctx.active_theme().spacing.lg
                                - DESIGN_TOKENS.sizing.button_sm;
                            assert!(
                                wrap <= button,
                                "the message wraps at {wrap}, past the close button at {button}"
                            );
                        }
                    }
                }
            }
        }
    }
}
