//! Grid presets and synchronization switches for a chart layout.

use crate::icons::{Icon, icons};
use crate::localization as l10n;
use crate::ui::model::ChartLayoutMode;
use crate::ui_kit::buttons::IconButton;
use crate::widget::ChartSyncOptions;
use egui::Ui;

/// A layout trigger with an anchored, scrollable preset menu.
#[derive(Default)]
pub struct ChartGridSelector {
    current: ChartLayoutMode,
    /// Synchronization options chosen by the user. Apply these to the layout each frame.
    pub sync: ChartSyncOptions,
}

impl ChartGridSelector {
    /// Creates a selector for the initial preset.
    pub fn new(initial: ChartLayoutMode) -> Self {
        Self {
            current: initial,
            ..Default::default()
        }
    }

    /// Selected preset.
    pub fn current(&self) -> ChartLayoutMode {
        self.current
    }

    /// Updates the selection from host state.
    pub fn set_layout(&mut self, layout: ChartLayoutMode) {
        self.current = layout;
    }

    /// Shows the trigger and returns a changed grid or tab preset.
    pub fn show(&mut self, ui: &mut Ui) -> Option<ChartLayoutMode> {
        // The toolbar's 24 px icon size, so the cells read at the size of the glyphs beside them.
        let button = IconButton::new(layout_icon(self.current))
            .icon_scale(24.0 / 28.0)
            .tooltip("Chart layout")
            .show(ui);
        let mut selected = None;
        egui::Popup::menu(&button)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                selected = self.show_menu(ui);
            });
        selected
    }

    /// Grid presets and synchronization menu contents.
    pub fn show_menu(&mut self, ui: &mut Ui) -> Option<ChartLayoutMode> {
        ui.set_min_width(220.0);
        let mut selected = None;
        egui::ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                for &layout in ChartLayoutMode::all() {
                    if ui
                        .selectable_label(
                            self.current == layout,
                            l10n::text(ui.ctx(), layout.label()),
                        )
                        .clicked()
                    {
                        self.current = layout;
                        selected = Some(layout);
                        ui.close();
                    }
                }
            });
        ui.separator();
        ui.label(l10n::text(ui.ctx(), "Synchronize within group"));
        ui.checkbox(&mut self.sync.sync_symbol, l10n::text(ui.ctx(), "Symbol"));
        ui.checkbox(
            &mut self.sync.sync_timeframe,
            l10n::text(ui.ctx(), "Interval"),
        );
        ui.checkbox(
            &mut self.sync.sync_crosshair,
            l10n::text(ui.ctx(), "Crosshair"),
        );
        ui.checkbox(
            &mut self.sync.sync_time_axis,
            l10n::text(ui.ctx(), "Scroll and zoom"),
        );
        ui.checkbox(
            &mut self.sync.sync_date_range,
            l10n::text(ui.ctx(), "Date range"),
        );
        ui.checkbox(
            &mut self.sync.sync_drawings,
            l10n::text(ui.ctx(), "Drawings on the same symbol"),
        );
        selected
    }
}

/// The trigger glyph of `layout`: its cells, as the layout tiles the charts.
fn layout_icon(layout: ChartLayoutMode) -> &'static Icon {
    use ChartLayoutMode as M;
    match layout {
        M::Single => &icons::LAYOUT_SINGLE,
        M::TwoHorizontal => &icons::LAYOUT_TWO_HORIZONTAL,
        M::TwoVertical => &icons::LAYOUT_TWO_VERTICAL,
        M::Three => &icons::LAYOUT_THREE,
        M::Four => &icons::LAYOUT_FOUR,
        M::Five => &icons::LAYOUT_FIVE,
        M::Six => &icons::LAYOUT_SIX,
        M::Seven => &icons::LAYOUT_SEVEN,
        M::Eight => &icons::LAYOUT_EIGHT,
        M::Nine => &icons::LAYOUT_NINE,
        M::Ten => &icons::LAYOUT_TEN,
        M::Eleven => &icons::LAYOUT_ELEVEN,
        M::Twelve => &icons::LAYOUT_TWELVE,
        M::Thirteen => &icons::LAYOUT_THIRTEEN,
        M::Fourteen => &icons::LAYOUT_FOURTEEN,
        M::Fifteen => &icons::LAYOUT_FIFTEEN,
        M::Sixteen => &icons::LAYOUT_SIXTEEN,
        M::Tabs => &icons::LAYOUT_TABS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chart_grid_selector_default() {
        let selector = ChartGridSelector::default();
        assert_eq!(selector.current(), ChartLayoutMode::Single);
    }

    #[test]
    fn test_set_layout() {
        let mut selector = ChartGridSelector::new(ChartLayoutMode::Single);
        selector.set_layout(ChartLayoutMode::Four);
        assert_eq!(selector.current(), ChartLayoutMode::Four);
    }
}
