//! Chart Type Selector
//!
//! A dropdown menu for selecting different chart visualization types.
//! Uses [`crate::model::ChartType`] for the data definition.

use crate::icons::{Icon, icons as embedded_icons};
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
            name: format!("Chart type: {label}"),
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
        spacing.scroll.fade.strength = 0.0;
        let bar = spacing.scroll.allocated_width();

        let text_w = ChartType::all()
            .iter()
            .map(|ct| {
                let width = |text: &str, size: f32| {
                    ui.painter()
                        .layout_no_wrap(
                            text.to_owned(),
                            egui::FontId::proportional(size),
                            Color32::PLACEHOLDER,
                        )
                        .size()
                        .x
                };
                f32::max(
                    width(ct.name(), theme.text.body),
                    width(ct.desc(), theme.text.small),
                )
            })
            .fold(0.0, f32::max);
        let row_w = theme.spacing.lg * 2.0
            + icon_sizes::MD
            + theme.spacing.lg
            + text_w
            + theme.spacing.lg
            + icon_sizes::SM;
        let overflows = ChartType::all().len() as f32 * ROW_H > max_height;
        let bar = if overflows { bar } else { 0.0 };
        ui.set_width((row_w + bar).max(self.config.dropdown_width));
        // A popup is first measured inside a 400 px high area; this lets the list use the room below.
        ui.set_max_height(max_height);

        let current = self.current;
        egui::ScrollArea::vertical()
            .max_height(max_height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for chart_type in ChartType::all() {
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

/// One chart type row: icon, name with a one-line description, and a check on the current type.
struct ChartTypeRow<'a> {
    chart_type: ChartType,
    theme: &'a Theme,
}

impl ListItemContent for ChartTypeRow<'_> {
    fn ui(self, ui: &mut Ui, context: &ContentContext<'_>) -> egui::Response {
        let theme = self.theme;
        let rect = context.rect;
        let (icon, name, description) = if context.selected {
            let on_selection = theme.ui.selection_text;
            (on_selection, on_selection, on_selection)
        } else {
            let icon = if context.hovered {
                theme.ui.text
            } else {
                theme.ui.text_secondary
            };
            (icon, theme.ui.text, theme.ui.text_secondary)
        };
        context.response.widget_info(|| {
            WidgetInfo::selected(
                Role::Button,
                ui.is_enabled(),
                context.selected,
                self.chart_type.name(),
            )
        });

        let icon_rect = Rect::from_min_size(
            pos2(rect.min.x, rect.center().y - icon_sizes::MD / 2.0),
            vec2(icon_sizes::MD, icon_sizes::MD),
        );
        self.chart_type
            .icon()
            .as_image_tinted(icon_rect.size(), icon)
            .paint_at(ui, icon_rect);

        // The two lines are centred as one block.
        let name = ui.painter().layout_no_wrap(
            self.chart_type.name().to_owned(),
            egui::FontId::proportional(theme.text.body),
            name,
        );
        let description = ui.painter().layout_no_wrap(
            self.chart_type.desc().to_owned(),
            egui::FontId::proportional(theme.text.small),
            description,
        );
        let text_x = icon_rect.max.x + theme.spacing.lg;
        let top = rect.center().y - (name.size().y + description.size().y) / 2.0;
        let description_y = top + name.size().y;
        ui.painter()
            .galley(pos2(text_x, top), name, Color32::PLACEHOLDER);
        ui.painter().galley(
            pos2(text_x, description_y),
            description,
            Color32::PLACEHOLDER,
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
                .as_image_tinted(check.size(), theme.ui.selection_text)
                .paint_at(ui, check);
        }
        menu_row_focus(ui, context.response);
        context.response.clone()
    }
}
