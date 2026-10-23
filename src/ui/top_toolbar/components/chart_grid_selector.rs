//! Grid presets and synchronization switches for a chart layout.

use crate::icons::icons;
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
        let button = IconButton::new(&icons::LAYOUT_GRID)
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

    /// Menu contents, also used when the toolbar moves this control into overflow.
    pub fn show_menu(&mut self, ui: &mut Ui) -> Option<ChartLayoutMode> {
        ui.set_min_width(220.0);
        let mut selected = None;
        egui::ScrollArea::vertical()
            .max_height(300.0)
            .show(ui, |ui| {
                for &layout in ChartLayoutMode::all() {
                    if ui
                        .selectable_label(self.current == layout, layout.label())
                        .clicked()
                    {
                        self.current = layout;
                        selected = Some(layout);
                        ui.close();
                    }
                }
            });
        ui.separator();
        ui.label("Synchronize within group");
        ui.checkbox(&mut self.sync.sync_symbol, "Symbol");
        ui.checkbox(&mut self.sync.sync_timeframe, "Interval");
        ui.checkbox(&mut self.sync.sync_crosshair, "Crosshair");
        ui.checkbox(&mut self.sync.sync_time_axis, "Scroll and zoom");
        ui.checkbox(&mut self.sync.sync_date_range, "Date range");
        ui.checkbox(&mut self.sync.sync_drawings, "Drawings on the same symbol");
        selected
    }
}
