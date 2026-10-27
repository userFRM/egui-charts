//! Grid presets and synchronization switches for a chart layout.

use crate::icons::{Icon, icons};
use crate::localization as l10n;
use crate::theme::ThemeExt;
use crate::ui::model::ChartLayoutMode;
use crate::ui::top_toolbar::TopToolbarConfig;
use crate::ui::top_toolbar::buttons::ToolbarIconButton;
use crate::ui_kit::buttons::{ButtonSize, IconButton};
use crate::widget::ChartSyncOptions;
use egui::{RichText, Ui};

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
        // Sized and labelled like the toolbar buttons beside it, with their tooltip.
        let button = ToolbarIconButton::new(
            layout_icon(self.current),
            "Chart layout",
            &TopToolbarConfig::default(),
        )
        .show(ui);
        let mut selected = None;
        egui::Popup::menu(&button)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                selected = self.show_menu(ui);
            });
        selected
    }

    /// Grid presets, as diagrams grouped by chart count, and synchronization menu contents.
    pub fn show_menu(&mut self, ui: &mut Ui) -> Option<ChartLayoutMode> {
        let theme = ui.active_theme();
        // The menu scrolls only when the window is too short to hold it below the trigger.
        let max_height = ui.ctx().content_rect().bottom()
            - ui.next_widget_position().y
            - ui.spacing().menu_margin.sum().y
            - theme.spacing.lg;
        ui.set_max_height(max_height);
        let mut selected = None;
        egui::ScrollArea::vertical()
            .max_height(max_height)
            .show(ui, |ui| {
                egui::Grid::new("chart_layout_presets")
                    .num_columns(2)
                    .min_col_width(0.0)
                    .show(ui, |ui| {
                        for count in 1..=16 {
                            ui.label(
                                RichText::new(count.to_string())
                                    .size(theme.text.small)
                                    .color(theme.ui.text_secondary),
                            );
                            let row = egui::vec2(0.0, ButtonSize::MD.pixels());
                            let row_layout = egui::Layout::left_to_right(egui::Align::Center);
                            ui.allocate_ui_with_layout(row, row_layout, |ui| {
                                for &layout in ChartLayoutMode::all()
                                    .iter()
                                    .filter(|layout| layout.chart_count() == count)
                                {
                                    if IconButton::new(layout_icon(layout))
                                        .icon_scale(24.0 / 28.0)
                                        .selected(self.current == layout)
                                        .tooltip(layout.label())
                                        .show(ui)
                                        .clicked()
                                    {
                                        self.current = layout;
                                        selected = Some(layout);
                                        ui.close();
                                    }
                                }
                            });
                            ui.end_row();
                        }
                    });
                ui.separator();
                ui.label(l10n::text(ui.ctx(), "Synchronize within group"));
                for (value, label, effect) in [
                    (
                        &mut self.sync.sync_symbol,
                        "Symbol",
                        "A symbol chosen on one chart applies to every chart of the group.",
                    ),
                    (
                        &mut self.sync.sync_timeframe,
                        "Interval",
                        "An interval chosen on one chart applies to every chart of the group.",
                    ),
                    (
                        &mut self.sync.sync_crosshair,
                        "Crosshair",
                        "Every chart of the group marks the time hovered on one of them.",
                    ),
                    (
                        &mut self.sync.sync_time_axis,
                        "Scroll and zoom",
                        "Scrolling or zooming one chart shows the same time span on every chart of the group.",
                    ),
                    (
                        &mut self.sync.sync_date_range,
                        "Date range",
                        "A date range chosen on one chart applies to every chart of the group.",
                    ),
                    (
                        &mut self.sync.sync_drawings,
                        "Drawings on the same symbol",
                        "A drawing completed on one chart appears on the charts of the group showing the same symbol.",
                    ),
                ] {
                    ui.checkbox(value, l10n::text(ui.ctx(), label))
                        .on_hover_text(l10n::text(ui.ctx(), effect));
                }
            });
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
