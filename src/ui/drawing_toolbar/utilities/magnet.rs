//! Magnet button and its strength menu.
//!
//! A click switches snapping on in its last strength or off; a secondary click opens the menu:
//! - Off: no snapping
//! - Weak magnet: snap within a pixel threshold
//! - Strong magnet: nearest OHLC of the bar under the pointer
//! - Snap to indicators: include visible study values
//!
//! The first switch-on from the toolbar shows a card beside the button, centred on it, that
//! explains snapping until its button dismisses it.

use crate::icons::icons;
use crate::localization as l10n;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::drawing_toolbar::DrawingToolbarAction;
use crate::ui::drawing_toolbar::components::icon_button::draw_latched_button;
use crate::ui::drawing_toolbar::state::MagnetType;
use crate::ui::drawing_toolbar::submenu_builder::SubmenuBuilder;
use egui::{Align, Align2, Id, Layout, Rect, Response, RichText, Ui, Vec2};

/// Width of the magnet card's text: about three even lines at the 13 point interface size.
const HINT_TEXT_WIDTH: f32 = 236.0;

/// Magnet button with dropdown component.
pub struct MagnetButton;

impl MagnetButton {
    /// Render the magnet button, filled while snapping is on. The toolbar switches
    /// snapping on a click and opens the strength menu on a secondary click.
    pub fn show(ui: &mut Ui, is_active: bool) -> Response {
        draw_latched_button(ui, &icons::DRAWING_MAGNET, "Magnet mode", is_active)
    }

    /// Render the magnet dropdown rows, with a check on the current mode. "Off" returns
    /// [`DrawingToolbarAction::ToggleMagnet`], a choice of strength [`DrawingToolbarAction::SetMagnetType`].
    pub fn show_submenu(
        ui: &mut Ui,
        magnet_on: bool,
        magnet_type: MagnetType,
        snap_to_indicators: bool,
    ) -> Option<DrawingToolbarAction> {
        let is_current = |kind| magnet_on && magnet_type == kind;
        SubmenuBuilder::new(ui)
            .add_text_item_with_action("Off", "Turn magnetic snapping off", || {
                Some(DrawingToolbarAction::ToggleMagnet)
            })
            .icon(&icons::CLOSE)
            .checked(!magnet_on)
            .add_text_item_with_action("Weak magnet", "Snap only near chart values", || {
                Some(DrawingToolbarAction::SetMagnetType(MagnetType::Weak))
            })
            .icon(&icons::MAGNET)
            .checked(is_current(MagnetType::Weak))
            .add_text_item_with_action(
                "Strong magnet",
                "Snap to the nearest bar OHLC value",
                || Some(DrawingToolbarAction::SetMagnetType(MagnetType::Strong)),
            )
            .icon(&icons::DRAWING_MAGNET)
            .checked(is_current(MagnetType::Strong))
            .add_text_item_with_action(
                "Snap to indicators",
                "Include visible study values",
                move || {
                    Some(DrawingToolbarAction::SetSnapToIndicators(
                        !snap_to_indicators,
                    ))
                },
            )
            .checked(snap_to_indicators)
            .show()
    }

    /// Show the card that explains the magnet right of `anchor`, the magnet button, centred on the
    /// button and kept inside the window, in the accent colours of the active theme. It takes no
    /// focus; its button is in the Tab order. Returns true when the button dismisses the card.
    pub(crate) fn show_hint(ui: &Ui, anchor: Rect) -> bool {
        let theme = ui.active_theme();
        let ctx = ui.ctx();
        egui::Area::new(Id::NULL.with("drawing_toolbar_magnet_hint"))
            .order(egui::Order::Foreground)
            .enabled(ui.is_enabled())
            .pivot(Align2::LEFT_CENTER)
            .fixed_pos(anchor.right_center() + Vec2::new(theme.spacing.sm, 0.0))
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(theme.ui.accent)
                    .corner_radius(theme.radius.md)
                    .inner_margin(theme.spacing.xxl + theme.spacing.sm)
                    .shadow(ui.visuals().popup_shadow)
                    .show(ui, |ui| {
                        ui.set_width(HINT_TEXT_WIDTH);
                        ui.spacing_mut().item_spacing.y = theme.spacing.xl;
                        // One paragraph, so the text breaks into even lines.
                        let text = [
                            "Drawing points snap to the open, high, low or close of the nearest bar.",
                            "Click the magnet again to turn it off.",
                        ]
                        .map(|text| l10n::text(ctx, text))
                        .join(" ");
                        ui.label(RichText::new(text).color(theme.ui.accent_text));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            // The dialog footer's button padding.
                            ui.spacing_mut().button_padding.x = theme.spacing.xl + theme.spacing.xs;
                            let response = ui.add(
                                egui::Button::new(
                                    RichText::new(l10n::text(ctx, "Got it")).color(theme.ui.accent),
                                )
                                .fill(theme.ui.accent_text)
                                // At least the WCAG 2.2 AA minimum target size.
                                .min_size(Vec2::new(
                                    DESIGN_TOKENS.sizing.target_min,
                                    DESIGN_TOKENS.sizing.settings_dialog.button_height,
                                ))
                                .corner_radius(theme.radius.md),
                            );
                            crate::styles::focus::draw_focus_ring(ui, &response, theme.radius.md);
                            response.clicked()
                        })
                        .inner
                    })
                    .inner
            })
            .inner
    }
}
