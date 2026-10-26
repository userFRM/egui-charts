//! Chart Type Selector
//!
//! A dropdown menu for selecting different chart visualization types.
//! Uses [`crate::model::ChartType`] for the data definition.

use crate::icons::{Icon, icons as embedded_icons};
use crate::localization as l10n;
use crate::model::ChartType;
use crate::styles::icons as icon_sizes;
use crate::theme::{Theme, ThemeExt};
use crate::tokens::DESIGN_TOKENS;
use crate::ui::top_toolbar::buttons::Trigger;
use crate::ui::top_toolbar::components::timeframe_selector::{menu_list_height, menu_row_focus};
use crate::ui_kit::list_item::{ContentContext, ListItem, ListItemContent};
use egui::{Color32, Rect, Role, Ui, WidgetInfo, pos2, vec2};

/// Height of a chart type row.
const ROW_H: f32 = 40.0;

/// Extension trait for ChartType to provide UI-specific functionality
pub trait ChartTypeUiExt {
    /// Get the icon for this chart type
    fn icon(&self) -> &'static Icon;
}

impl ChartTypeUiExt for ChartType {
    fn icon(&self) -> &'static Icon {
        match self {
            ChartType::Bars => &embedded_icons::CHART_BARS,
            ChartType::Candles => &embedded_icons::CHART_CANDLES,
            ChartType::HollowCandles => &embedded_icons::CHART_HOLLOW_CANDLES,
            ChartType::VolumeCandles => &embedded_icons::CHART_VOLUME_CANDLES,
            ChartType::Line => &embedded_icons::CHART_LINE,
            ChartType::LineWithMarkers => &embedded_icons::CHART_LINE_MARKERS,
            ChartType::StepLine => &embedded_icons::CHART_STEP_LINE,
            ChartType::Area => &embedded_icons::CHART_AREA,
            ChartType::HlcArea => &embedded_icons::CHART_HLC_AREA,
            ChartType::Baseline => &embedded_icons::CHART_BASELINE,
            ChartType::Columns => &embedded_icons::CHART_COLUMNS,
            ChartType::HighLow => &embedded_icons::CHART_HIGH_LOW,
            ChartType::TimePriceOpportunity => &embedded_icons::CHART_TPO,
            ChartType::LineBreak => &embedded_icons::CHART_LINE_BREAK,
            ChartType::Kagi => &embedded_icons::CHART_KAGI,
            ChartType::Range => &embedded_icons::CHART_RANGE,
            ChartType::PointAndFigure => &embedded_icons::CHART_POINT_FIGURE,
            ChartType::Renko => &embedded_icons::CHART_RENKO,
            ChartType::Heikin => &embedded_icons::CHART_HEIKIN_ASHI,
        }
    }
}

/// Configuration for chart type selector
#[derive(Debug, Clone)]
pub struct ChartTypeSelectorConfig {
    /// Minimum width of the button; it is otherwise sized to its label.
    pub btn_width: f32,
    /// Minimum width of the menu.
    pub dropdown_width: f32,
    /// Not read: rows are laid out by the menu.
    pub item_height: f32,
    /// Not read: the menu takes its colours from the applied theme.
    pub bg_color: Color32,
    /// Not read: the menu takes its colours from the applied theme.
    pub hover_color: Color32,
    /// Not read: the menu takes its colours from the applied theme.
    pub sel_color: Color32,
    /// Not read: the menu takes its colours from the applied theme.
    pub text_color: Color32,
    /// Not read: the menu takes its colours from the applied theme.
    pub muted_color: Color32,
}

impl Default for ChartTypeSelectorConfig {
    fn default() -> Self {
        // Colors are TRANSPARENT to signal that colors should be fetched from
        // ui.style().visuals at render time for proper theme support.
        Self {
            btn_width: 0.0,
            dropdown_width: DESIGN_TOKENS.sizing.dialog.submenu_width + DESIGN_TOKENS.spacing.xxl,
            item_height: DESIGN_TOKENS.sizing.button_lg,
            bg_color: Color32::TRANSPARENT,
            hover_color: Color32::TRANSPARENT,
            sel_color: Color32::TRANSPARENT,
            text_color: Color32::TRANSPARENT,
            muted_color: Color32::TRANSPARENT,
        }
    }
}

/// Action from chart type selector
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChartTypeAction {
    None,
    Select(ChartType),
}

/// Chart type selector widget
pub struct ChartTypeSelector {
    /// Current chart type
    pub current: ChartType,
    /// Is dropdown open
    is_open: bool,
    /// Configuration
    pub config: ChartTypeSelectorConfig,
}

impl Default for ChartTypeSelector {
    fn default() -> Self {
        Self::new(ChartType::Candles)
    }
}

impl ChartTypeSelector {
    pub fn new(initial: ChartType) -> Self {
        Self {
            current: initial,
            is_open: false,
            config: ChartTypeSelectorConfig::default(),
        }
    }

    pub fn with_config(mut self, config: ChartTypeSelectorConfig) -> Self {
        self.config = config;
        self
    }

    /// Show the chart type selector
    pub fn show(&mut self, ui: &mut Ui) -> ChartTypeAction {
        // Main button
        let btn_res = self.trigger().show(ui, self.is_open);

        let theme = ui.active_theme();
        let id = ui.make_persistent_id("chart_type_menu");
        let max_height = menu_list_height(ui, btn_res.rect, ROW_H);
        let action = egui::Popup::menu(&btn_res)
            .id(id)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .align_alternatives(&[])
            .frame(egui::Frame::popup(ui.style()).fill(theme.ui.panel_bg))
            .gap(theme.spacing.xs)
            .show(|ui| self.menu_ui(ui, &theme, max_height))
            .map_or(ChartTypeAction::None, |menu| menu.inner);
        self.is_open = egui::Popup::is_id_open(ui.ctx(), id);
        action
    }

    /// The button that opens the dropdown, showing the current chart type.
    pub(crate) fn trigger(&self) -> Trigger<'static> {
        let label = self.current.name();
        Trigger {
            icon: Some(self.current.icon()),
            label: String::new(),
            name: label.to_owned(),
            min_width: self.config.btn_width.max(40.0),
            filled: false,
        }
    }

    fn menu_ui(&mut self, ui: &mut Ui, theme: &Theme, max_height: f32) -> ChartTypeAction {
        let mut action = ChartTypeAction::None;
        // Rows sit edge to edge, and the scroll bar is visible whenever the list overflows.
        let spacing = ui.spacing_mut();
        spacing.item_spacing.y = 0.0;
        spacing.scroll.floating = false;
        spacing.scroll.bar_width = 3.0;
        spacing.scroll.fade.strength = 0.0;

        ui.set_width(256.0_f32.max(self.config.dropdown_width));
        ui.visuals_mut().selection.bg_fill = theme.ui.text;
        ui.visuals_mut().selection.stroke.color = theme.ui.panel_bg;
        // A popup is first measured inside a 400 px high area; this lets the list use the room below.
        ui.set_max_height(max_height);

        let current = self.current;
        egui::ScrollArea::vertical()
            .max_height(max_height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for chart_type in ChartType::all() {
                    if matches!(
                        chart_type,
                        ChartType::Line
                            | ChartType::Area
                            | ChartType::Columns
                            | ChartType::TimePriceOpportunity
                            | ChartType::LineBreak
                    ) {
                        ui.add_space(12.0);
                        ui.separator();
                        ui.add_space(12.0);
                    }
                    let content = ChartTypeRow {
                        chart_type: *chart_type,
                        theme,
                    };
                    let row = ListItem::new()
                        .selected(current == *chart_type)
                        .height(ROW_H)
                        .show(ui, content);
                    if row.clicked() {
                        action = ChartTypeAction::Select(*chart_type);
                    }
                }
            });

        if let ChartTypeAction::Select(chart_type) = action {
            self.current = chart_type;
            ui.close();
        }
        action
    }
}

/// One chart type row: icon, name, and a check on the current type.
struct ChartTypeRow<'a> {
    chart_type: ChartType,
    theme: &'a Theme,
}

impl ListItemContent for ChartTypeRow<'_> {
    fn ui(self, ui: &mut Ui, context: &ContentContext<'_>) -> egui::Response {
        let theme = self.theme;
        let rect = context.rect;
        let color = if context.selected {
            theme.ui.panel_bg
        } else {
            theme.ui.text
        };
        let translated_label = l10n::text(ui.ctx(), self.chart_type.name());
        context.response.widget_info(|| {
            WidgetInfo::selected(
                Role::MenuItemRadio,
                context.response.enabled(),
                context.selected,
                &translated_label,
            )
        });

        let icon_rect = Rect::from_min_size(
            pos2(rect.min.x, rect.center().y - icon_sizes::MD / 2.0),
            vec2(icon_sizes::MD, icon_sizes::MD),
        );
        self.chart_type
            .icon()
            .as_image_tinted(icon_rect.size(), color)
            .paint_at(ui, icon_rect);

        ui.painter().text(
            pos2(icon_rect.max.x + theme.spacing.lg, rect.center().y),
            egui::Align2::LEFT_CENTER,
            l10n::text(ui.ctx(), self.chart_type.name()),
            egui::FontId::proportional(14.0),
            color,
        );

        if context.selected {
            let check = Rect::from_min_size(
                pos2(
                    rect.max.x - icon_sizes::SM,
                    rect.center().y - icon_sizes::SM / 2.0,
                ),
                vec2(icon_sizes::SM, icon_sizes::SM),
            );
            embedded_icons::CHECK
                .as_image_tinted(check.size(), color)
                .paint_at(ui, check);
        }
        menu_row_focus(ui, context.response);
        context.response.clone()
    }
}
