//! Magnet mode button with dropdown menu.
//!
//! Provides magnetic snapping functionality for drawing tools:
//! - Off: no snapping
//! - Weak Magnet: Snap within a pixel threshold
//! - Strong Magnet: Nearest OHLC of the bar under the pointer
//! - OHLC Magnet: Snap to OHLC price values

use crate::icons::icons;
use crate::ui::drawing_toolbar::DrawingToolbarAction;
use crate::ui::drawing_toolbar::components::icon_button::draw_latched_button;
use crate::ui::drawing_toolbar::state::MagnetType;
use crate::ui::drawing_toolbar::submenu_builder::SubmenuBuilder;
use egui::{Response, Ui};

/// Magnet button with dropdown component.
pub struct MagnetButton;

impl MagnetButton {
    /// Render the magnet button, filled while snapping is on. A click opens the dropdown.
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
            .add_text_item_with_action("OHLC magnet", "Snap precisely to OHLC price values", || {
                Some(DrawingToolbarAction::SetMagnetType(MagnetType::OHLC))
            })
            .icon(&icons::CHART_BARS)
            .checked(is_current(MagnetType::OHLC))
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
}
