//! Chart Type Selector
//!
//! A dropdown menu for selecting different chart visualization types.
//! Uses [`crate::model::ChartType`] for the data definition.

use crate::icons::{Icon, icons as embedded_icons};
use crate::model::ChartType;
use crate::styles::{icons as icon_sizes, typography};
use crate::theming;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

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
            ChartType::HighLow => &embedded_icons::CHART_HIGH_LOW,
            ChartType::VolumeFootprint => &embedded_icons::CHART_VOLUME_FOOTPRINT,
            ChartType::TimePriceOpportunity => &embedded_icons::CHART_TPO,
            ChartType::SessionVolume => &embedded_icons::CHART_SESSION_VOLUME,
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
    pub btn_width: f32,
    pub dropdown_width: f32,
    pub item_height: f32,
    pub bg_color: Color32,
    pub hover_color: Color32,
    pub sel_color: Color32,
    pub text_color: Color32,
    pub muted_color: Color32,
}

impl Default for ChartTypeSelectorConfig {
    fn default() -> Self {
        // Colors are TRANSPARENT to signal that colors should be fetched from
        // ui.style().visuals at render time for proper theme support.
        Self {
            btn_width: 100.0,
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
        let mut action = ChartTypeAction::None;

        // Main button
        let btn_res = self.draw_btn(ui);
        if btn_res.clicked() {
            self.is_open = !self.is_open;
        }

        // Dropdown
        if self.is_open {
            let btn_rect = btn_res.rect;
            action = self.draw_dropdown(ui, btn_rect);

            // Close on click outside
            if ui.input(|i| i.pointer.any_click())
                && let Some(pos) = ui.input(|i| i.pointer.hover_pos())
            {
                let dropdown_height = ChartType::all().len() as f32 * self.config.item_height + 8.0;
                let dropdown_rect = Rect::from_min_size(
                    Pos2::new(btn_rect.min.x, btn_rect.max.y + 2.0),
                    Vec2::new(self.config.dropdown_width, dropdown_height.min(400.0)),
                );
                if !btn_rect.contains(pos) && !dropdown_rect.contains(pos) {
                    self.is_open = false;
                }
            }
        }

        action
    }

    fn draw_btn(&self, ui: &mut Ui) -> Response {
        let desired_size = Vec2::new(self.config.btn_width, DESIGN_TOKENS.sizing.button_md);
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

        // Theme-aware colors
        let bg_color = if self.is_open || response.hovered() {
            ui.style().visuals.widgets.hovered.bg_fill
        } else {
            Color32::TRANSPARENT
        };
        let border_color = ui.style().visuals.widgets.noninteractive.bg_stroke.color;
        let text_color = ui.style().visuals.text_color();

        ui.painter()
            .rect_filled(rect, DESIGN_TOKENS.rounding.md, bg_color);
        ui.painter().rect_stroke(
            rect,
            DESIGN_TOKENS.rounding.md,
            Stroke::new(DESIGN_TOKENS.stroke.hairline, border_color),
            StrokeKind::Outside,
        );

        // Icon
        let icon_size = DESIGN_TOKENS.sizing.button_md;
        let icon_pos = Pos2::new(
            rect.min.x + DESIGN_TOKENS.spacing.md,
            rect.center().y - icon_size / 2.0,
        );
        let icon_rect = Rect::from_min_size(icon_pos, Vec2::splat(icon_size));
        let icon_color = if response.hovered() {
            theming::icon_hover_color(ui)
        } else {
            theming::icon_normal(ui)
        };
        self.current
            .icon()
            .as_image_tinted(Vec2::splat(icon_size), icon_color)
            .paint_at(ui, icon_rect);

        // Name
        ui.painter().text(
            Pos2::new(rect.min.x + DESIGN_TOKENS.sizing.button_xl, rect.center().y),
            egui::Align2::LEFT_CENTER,
            self.current.name(),
            egui::FontId::proportional(typography::MD),
            text_color,
        );

        // Dropdown arrow
        let chevron_size = icon_sizes::XS;
        let chevron_pos = Pos2::new(
            rect.right() - DESIGN_TOKENS.spacing.xxl,
            rect.center().y - chevron_size / 2.0,
        );
        let chevron_rect = Rect::from_min_size(chevron_pos, Vec2::splat(chevron_size));
        let chevron_color = if response.hovered() {
            theming::icon_hover_color(ui)
        } else {
            theming::icon_normal(ui)
        };
        embedded_icons::CHEVRON_DOWN
            .as_image_tinted(Vec2::splat(chevron_size), chevron_color)
            .paint_at(ui, chevron_rect);

        response
    }

    fn draw_dropdown(&mut self, ui: &mut Ui, btn_rect: Rect) -> ChartTypeAction {
        let mut action = ChartTypeAction::None;
        let dropdown_pos = Pos2::new(btn_rect.min.x, btn_rect.max.y + DESIGN_TOKENS.spacing.xs);
        let colors = ChartTypeDropdownColors::from_ui(ui);
        let current = self.current;
        let dropdown_width = self.config.dropdown_width;

        egui::Area::new(egui::Id::new("chart_type_dropdown"))
            .fixed_pos(dropdown_pos)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                Self::draw_dropdown_frame(ui, &colors, dropdown_width, |ui| {
                    self.draw_chart_type_list(ui, &colors, current, dropdown_width, &mut action);
                });
            });

        action
    }

    fn draw_dropdown_frame(
        ui: &mut Ui,
        colors: &ChartTypeDropdownColors,
        width: f32,
        content: impl FnOnce(&mut Ui),
    ) {
        egui::Frame::new()
            .fill(colors.bg)
            .stroke(Stroke::new(DESIGN_TOKENS.stroke.hairline, colors.border))
            .corner_radius(DESIGN_TOKENS.rounding.md)
            .shadow(egui::Shadow {
                spread: 2,
                blur: 8,
                offset: [2, 2],
                color: Color32::from_black_alpha(40),
            })
            .inner_margin(egui::Margin::same(DESIGN_TOKENS.spacing.sm as i8))
            .show(ui, |ui| {
                ui.set_min_width(width);
                egui::ScrollArea::vertical()
                    .max_height(500.0)
                    .show(ui, content);
            });
    }

    fn draw_chart_type_list(
        &mut self,
        ui: &mut Ui,
        colors: &ChartTypeDropdownColors,
        current: ChartType,
        width: f32,
        action: &mut ChartTypeAction,
    ) {
        let item_height = DESIGN_TOKENS.sizing.button_lg + DESIGN_TOKENS.spacing.xs;
        for chart_type in ChartType::all() {
            if self.draw_chart_type_item(
                ui,
                colors,
                *chart_type,
                current == *chart_type,
                width,
                item_height,
            ) {
                *action = ChartTypeAction::Select(*chart_type);
            }
        }
    }

    fn draw_chart_type_item(
        &mut self,
        ui: &mut Ui,
        colors: &ChartTypeDropdownColors,
        chart_type: ChartType,
        is_current: bool,
        width: f32,
        height: f32,
    ) -> bool {
        let (rect, response) = ui.allocate_exact_size(
            Vec2::new(width - DESIGN_TOKENS.spacing.lg, height),
            Sense::click(),
        );

        // Background
        if is_current {
            ui.painter()
                .rect_filled(rect, DESIGN_TOKENS.rounding.sm, colors.selected);
        } else if response.hovered() {
            ui.painter()
                .rect_filled(rect, DESIGN_TOKENS.rounding.sm, colors.hover);
        }

        // Icon
        let icon_size = DESIGN_TOKENS.sizing.button_md;
        let icon_rect = Rect::from_min_size(
            Pos2::new(
                rect.min.x + DESIGN_TOKENS.spacing.md,
                rect.center().y - icon_size / 2.0,
            ),
            Vec2::splat(icon_size),
        );
        let icon_color = if is_current {
            theming::icon_active(ui)
        } else if response.hovered() {
            theming::icon_hover_color(ui)
        } else {
            theming::icon_normal(ui)
        };
        chart_type
            .icon()
            .as_image_tinted(Vec2::splat(icon_size), icon_color)
            .paint_at(ui, icon_rect);

        // Name
        let name_color = if is_current {
            colors.strong
        } else {
            colors.text
        };
        ui.painter().text(
            Pos2::new(
                rect.min.x + DESIGN_TOKENS.sizing.toolbar.left_width,
                rect.min.y + DESIGN_TOKENS.spacing.xl,
            ),
            egui::Align2::LEFT_CENTER,
            chart_type.name(),
            egui::FontId::proportional(typography::LG),
            name_color,
        );

        // Description
        let desc_color = if is_current {
            colors.strong.gamma_multiply(0.7)
        } else {
            colors.muted
        };
        ui.painter().text(
            Pos2::new(
                rect.min.x + DESIGN_TOKENS.sizing.toolbar.left_width,
                rect.min.y + 26.0,
            ),
            egui::Align2::LEFT_CENTER,
            chart_type.desc(),
            egui::FontId::proportional(typography::XS),
            desc_color,
        );

        // Checkmark
        if is_current {
            ui.painter().text(
                Pos2::new(rect.right() - DESIGN_TOKENS.spacing.xxl, rect.center().y),
                egui::Align2::CENTER_CENTER,
                "v",
                egui::FontId::proportional(typography::MD),
                colors.strong,
            );
        }

        if response.clicked() {
            self.current = chart_type;
            self.is_open = false;
            return true;
        }
        false
    }
}

/// Theme colors for chart type dropdown
struct ChartTypeDropdownColors {
    bg: Color32,
    hover: Color32,
    selected: Color32,
    text: Color32,
    muted: Color32,
    border: Color32,
    strong: Color32,
}

impl ChartTypeDropdownColors {
    fn from_ui(ui: &Ui) -> Self {
        let visuals = &ui.style().visuals;
        Self {
            bg: visuals.window_fill,
            hover: visuals.widgets.hovered.bg_fill,
            selected: visuals.selection.bg_fill,
            text: visuals.text_color(),
            muted: visuals.widgets.noninteractive.fg_stroke.color,
            border: visuals.widgets.noninteractive.bg_stroke.color,
            strong: visuals.strong_text_color(),
        }
    }
}
