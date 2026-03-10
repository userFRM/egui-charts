//! Settings dialog with tabs
//!
//! This implementation follows the corporate architecture guidelines:
//! - Uses `crate::styles::sizing::settings_dialog` for all layout dimensions
//! - Uses `crate::theme::components::SettingsDialogStyle` for all colors
//! - Implements full interaction states (hover, active, focus)

mod tabs;

use tabs::configure_dialog_visuals;

use super::{
    actions::SettingsAction, config::SettingsDialogConfig, data::ChartSettingsState,
    types::SettingsTab,
};
use crate::ext::UiExt;
use crate::icons::{Icon, icons as embedded_icons};
use crate::styles::{stroke, typography};
use crate::templates::TemplateManager;
use crate::theme::Theme;
use crate::theme::components::SettingsDialogStyle;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::stubs::LayoutStyle;
use crate::ui_kit::ColorPickerState;
use egui::{
    Color32, Context, FontId, Pos2, Rect, Response, RichText, Sense, Stroke, Ui, Vec2, Window,
};

// ============================================================================
// Settings Dialog (Modal)
// ============================================================================

/// Modal settings dialog with tabbed interface.
///
/// Provides full chart settings configuration organized into tabs:
/// Symbol, Status Line, Scales and Lines, Canvas, Trading, Alerts, Events.
pub struct SettingsDialog {
    /// Whether the dialog is currently open
    pub is_open: bool,
    active_tab: SettingsTab,
    settings: ChartSettingsState,
    original_settings: ChartSettingsState,
    /// Current template name (empty if using default/unsaved settings)
    template_name: String,
    /// Whether the template dropdown menu is open
    template_menu_open: bool,
    /// Template manager for save/load chart settings templates
    template_manager: TemplateManager,
    timezones: Vec<String>,
    /// Dialog styling configuration (width, height overrides)
    config: SettingsDialogConfig,
    color_picker_states: ColorPickerStates,
    margin_top_str: String,
    margin_bottom_str: String,
    margin_right_str: String,
    /// Current layout style (Modern or Classic)
    layout_style: LayoutStyle,
}

#[derive(Clone, Default)]
struct ColorPickerStates {
    body_up: ColorPickerState,
    body_down: ColorPickerState,
    border_up: ColorPickerState,
    border_down: ColorPickerState,
    wick_up: ColorPickerState,
    wick_down: ColorPickerState,
    background: ColorPickerState,
    background_gradient_top: ColorPickerState,
    background_gradient_bottom: ColorPickerState,
    grid_h: ColorPickerState,
    grid_v: ColorPickerState,
    crosshair: ColorPickerState,
    watermark: ColorPickerState,
    scales_text: ColorPickerState,
    scales_lines: ColorPickerState,
    alert: ColorPickerState,
}

impl Default for SettingsDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl SettingsDialog {
    pub fn new() -> Self {
        let settings = ChartSettingsState::default();
        Self {
            is_open: false,
            active_tab: SettingsTab::Canvas,
            color_picker_states: ColorPickerStates {
                body_up: ColorPickerState::new(settings.candle_colors.body_up),
                body_down: ColorPickerState::new(settings.candle_colors.body_down),
                border_up: ColorPickerState::new(settings.candle_colors.border_up),
                border_down: ColorPickerState::new(settings.candle_colors.border_down),
                wick_up: ColorPickerState::new(settings.candle_colors.wick_up),
                wick_down: ColorPickerState::new(settings.candle_colors.wick_down),
                background: ColorPickerState::new(settings.chart_basic_styles.background_color),
                background_gradient_top: ColorPickerState::new(
                    settings.chart_basic_styles.background_gradient_top,
                ),
                background_gradient_bottom: ColorPickerState::new(
                    settings.chart_basic_styles.background_gradient_bottom,
                ),
                grid_h: ColorPickerState::new(settings.grid_lines.horizontal_color),
                grid_v: ColorPickerState::new(settings.grid_lines.vertical_color),
                crosshair: ColorPickerState::new(settings.crosshair.color),
                watermark: ColorPickerState::new(settings.watermark.color),
                scales_text: ColorPickerState::new(settings.scales_appearance.text_color),
                scales_lines: ColorPickerState::new(settings.scales_appearance.lines_color),
                alert: ColorPickerState::new(settings.alerts.alert_color),
            },
            margin_top_str: format!("{}", settings.margins.top_percent as u32),
            margin_bottom_str: format!("{}", settings.margins.bottom_percent as u32),
            margin_right_str: format!("{}", settings.margins.right_bars),
            settings,
            original_settings: ChartSettingsState::default(),
            template_name: String::new(),
            template_menu_open: false,
            template_manager: TemplateManager::new(),
            timezones: vec![
                "UTC".to_string(),
                "America/New_York".to_string(),
                "America/Chicago".to_string(),
                "America/Los_Angeles".to_string(),
                "Europe/London".to_string(),
                "Europe/Paris".to_string(),
                "Asia/Tokyo".to_string(),
                "Asia/Hong_Kong".to_string(),
                "Asia/Singapore".to_string(),
                "Australia/Sydney".to_string(),
            ],
            config: SettingsDialogConfig::default(),
            layout_style: LayoutStyle::Classic,
        }
    }

    pub fn open(&mut self, settings: ChartSettingsState) {
        self.is_open = true;
        self.settings = settings.clone();
        self.original_settings = settings;
        self.active_tab = SettingsTab::Canvas;
        self.update_color_pickers_from_settings();
    }

    /// Open the dialog with layout style
    pub fn open_with_layout(&mut self, settings: ChartSettingsState, layout_style: LayoutStyle) {
        self.layout_style = layout_style;
        self.open(settings);
    }

    /// Set the current layout style (call before show() if needed)
    pub fn set_layout_style(&mut self, layout_style: LayoutStyle) {
        self.layout_style = layout_style;
    }

    /// Sync all color picker states from the current settings
    fn update_color_pickers_from_settings(&mut self) {
        self.color_picker_states.body_up.sel_color = self.settings.candle_colors.body_up;
        self.color_picker_states.body_down.sel_color = self.settings.candle_colors.body_down;
        self.color_picker_states.border_up.sel_color = self.settings.candle_colors.border_up;
        self.color_picker_states.border_down.sel_color = self.settings.candle_colors.border_down;
        self.color_picker_states.wick_up.sel_color = self.settings.candle_colors.wick_up;
        self.color_picker_states.wick_down.sel_color = self.settings.candle_colors.wick_down;
        self.color_picker_states.background.sel_color =
            self.settings.chart_basic_styles.background_color;
        self.color_picker_states.background_gradient_top.sel_color =
            self.settings.chart_basic_styles.background_gradient_top;
        self.color_picker_states
            .background_gradient_bottom
            .sel_color = self.settings.chart_basic_styles.background_gradient_bottom;
        self.color_picker_states.grid_h.sel_color = self.settings.grid_lines.horizontal_color;
        self.color_picker_states.grid_v.sel_color = self.settings.grid_lines.vertical_color;
        self.color_picker_states.crosshair.sel_color = self.settings.crosshair.color;
        self.color_picker_states.watermark.sel_color = self.settings.watermark.color;
        self.color_picker_states.scales_text.sel_color = self.settings.scales_appearance.text_color;
        self.color_picker_states.scales_lines.sel_color =
            self.settings.scales_appearance.lines_color;
        self.color_picker_states.alert.sel_color = self.settings.alerts.alert_color;

        self.margin_top_str = format!("{}", self.settings.margins.top_percent as u32);
        self.margin_bottom_str = format!("{}", self.settings.margins.bottom_percent as u32);
        self.margin_right_str = format!("{}", self.settings.margins.right_bars);
    }

    pub fn close(&mut self) {
        self.is_open = false;
    }

    pub fn settings(&self) -> &ChartSettingsState {
        &self.settings
    }

    /// Get the current layout style setting
    pub fn layout_style(&self) -> LayoutStyle {
        self.layout_style
    }

    pub fn show(&mut self, ctx: &Context, theme: &Theme) -> SettingsAction {
        let mut action = SettingsAction::None;

        if !self.is_open {
            return action;
        }

        let mut is_open = self.is_open;
        let style = &theme.components.settings_dialog;

        // fixed dialog size
        let w = if self.config.width > 0.0 {
            self.config.width
        } else {
            DESIGN_TOKENS.sizing.settings_dialog.width
        };
        let h = if self.config.height > 0.0 {
            self.config.height
        } else {
            DESIGN_TOKENS.sizing.settings_dialog.height
        };
        let (dialog_width, dialog_height) = (w, h);

        // Corner radius - desktop style
        let corner_radius = DESIGN_TOKENS.sizing.settings_dialog.rounding as u8;

        Window::new("")
            .title_bar(false)
            .open(&mut is_open)
            .resizable(false)
            .collapsible(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .fixed_size(Vec2::new(dialog_width, dialog_height))
            .frame(
                egui::Frame::new()
                    .fill(style.content_bg)
                    .corner_radius(corner_radius),
            )
            .show(ctx, |ui| {
                action = self.draw_content(ui, style);
            });

        // Only update is_open from egui's Window if btns didn't already close it
        // (btns set self.is_open = false directly)
        if self.is_open && !is_open {
            // Window was closed via egui's mechanism (outside click, etc)
            self.is_open = false;
            action = SettingsAction::Cancel;
        }

        action
    }

    fn draw_content(&mut self, ui: &mut Ui, style: &SettingsDialogStyle) -> SettingsAction {
        let mut action = SettingsAction::None;

        // Configure visuals for dialog widgets
        configure_dialog_visuals(ui, style);

        // Custom title bar
        self.draw_title_bar(ui, style);

        // Calculate content area height (dialog height - title - footer)
        let content_height = DESIGN_TOKENS.sizing.settings_dialog.height
            - DESIGN_TOKENS.sizing.settings_dialog.title_height
            - DESIGN_TOKENS.sizing.settings_dialog.footer_height;

        // Main content area with fixed height
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), content_height),
            egui::Layout::left_to_right(egui::Align::TOP),
            |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;

                // Left sidebar with tabs
                self.draw_sidebar(ui, style, content_height);

                // Right content area
                ui.vertical(|ui| {
                    ui.set_min_width(DESIGN_TOKENS.sizing.settings_dialog.content_min_width + 20.0);
                    ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.content_padding_top);

                    egui::ScrollArea::vertical()
                        .auto_shrink([false, false])
                        .max_height(
                            content_height
                                - DESIGN_TOKENS.sizing.settings_dialog.content_padding_top * 2.0,
                        )
                        .show(ui, |ui| {
                            ui.set_min_width(
                                DESIGN_TOKENS.sizing.settings_dialog.content_min_width,
                            );
                            ui.spacing_mut().item_spacing.y =
                                DESIGN_TOKENS.sizing.settings_dialog.row_spacing;

                            match self.active_tab {
                                SettingsTab::Symbol => self.draw_symbol_tab(ui, style),
                                SettingsTab::StatusLine => self.draw_status_line_tab(ui, style),
                                SettingsTab::ScalesAndLines => {
                                    self.draw_scales_and_lines_tab(ui, style)
                                }
                                SettingsTab::Canvas => self.draw_canvas_tab(ui, style),
                                SettingsTab::Trading => self.draw_trading_tab(ui, style),
                                SettingsTab::Alerts => self.draw_alerts_tab(ui, style),
                                SettingsTab::Events => self.draw_events_tab(ui, style),
                            }
                        });
                });
            },
        );

        // Footer with btns
        action = self.draw_footer(ui, action, style);

        action
    }

    fn draw_title_bar(&mut self, ui: &mut Ui, style: &SettingsDialogStyle) {
        let (rect, _) = ui.allocate_exact_size(
            Vec2::new(
                ui.available_width(),
                DESIGN_TOKENS.sizing.settings_dialog.title_height,
            ),
            Sense::hover(),
        );

        // Title text
        ui.painter().text(
            Pos2::new(
                rect.min.x + DESIGN_TOKENS.sizing.settings_dialog.title_padding_left,
                rect.center().y,
            ),
            egui::Align2::LEFT_CENTER,
            "Settings",
            FontId::proportional(DESIGN_TOKENS.sizing.settings_dialog.title_font_size),
            style.dialog.title_text,
        );

        // Close button (X) - INTERACTIVE
        let close_rect = Rect::from_center_size(
            Pos2::new(
                rect.max.x
                    - DESIGN_TOKENS.sizing.settings_dialog.close_button_margin
                    - DESIGN_TOKENS.sizing.settings_dialog.close_button_size / 2.0,
                rect.center().y,
            ),
            Vec2::splat(DESIGN_TOKENS.sizing.settings_dialog.close_button_size),
        );

        // Make it clickable
        let close_res = ui.interact(close_rect, ui.id().with("close_btn"), Sense::click());

        // Determine colors based on hover state
        let (bg_color, icon_color) = if close_res.hovered() {
            (
                style.dialog.close_button_bg_hover,
                style.dialog.close_button_icon_hover,
            )
        } else {
            (style.dialog.close_button_bg, style.dialog.close_button_icon)
        };

        // Draw button background (only visible on hover)
        if close_res.hovered() {
            ui.painter()
                .rect_filled(close_rect, DESIGN_TOKENS.rounding.lg, bg_color);
        }

        // X icon
        let x_size = DESIGN_TOKENS.sizing.settings_dialog.close_icon_size;
        let center = close_rect.center();
        ui.painter().line_segment(
            [
                Pos2::new(center.x - x_size / 2.0, center.y - x_size / 2.0),
                Pos2::new(center.x + x_size / 2.0, center.y + x_size / 2.0),
            ],
            Stroke::new(stroke::MEDIUM, icon_color),
        );
        ui.painter().line_segment(
            [
                Pos2::new(center.x + x_size / 2.0, center.y - x_size / 2.0),
                Pos2::new(center.x - x_size / 2.0, center.y + x_size / 2.0),
            ],
            Stroke::new(stroke::MEDIUM, icon_color),
        );

        // Handle click
        if close_res.clicked() {
            self.is_open = false;
        }

        // Bottom border
        ui.painter().hline(
            rect.min.x..=rect.max.x,
            rect.max.y,
            Stroke::new(stroke::HAIRLINE, style.dialog.title_border),
        );
    }

    fn draw_sidebar(&mut self, ui: &mut Ui, style: &SettingsDialogStyle, height: f32) {
        let sidebar_rect = ui.available_rect_before_wrap();
        let sidebar_rect = Rect::from_min_size(
            sidebar_rect.min,
            Vec2::new(DESIGN_TOKENS.sizing.settings_dialog.sidebar_width, height),
        );

        // Sidebar background
        ui.painter()
            .rect_filled(sidebar_rect, DESIGN_TOKENS.rounding.none, style.sidebar_bg);

        // Constrain the vertical layout to the sidebar width
        ui.allocate_ui_with_layout(
            Vec2::new(DESIGN_TOKENS.sizing.settings_dialog.sidebar_width, height),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.tab_padding_v);

                for tab in SettingsTab::all() {
                    let is_active = self.active_tab == *tab;
                    if self.draw_tab_btn(ui, *tab, is_active, style).clicked() {
                        self.active_tab = *tab;
                    }
                }
            },
        );

        // Right border
        ui.painter().vline(
            sidebar_rect.max.x,
            sidebar_rect.min.y..=sidebar_rect.max.y,
            Stroke::new(stroke::HAIRLINE, style.sidebar_border),
        );
    }

    fn draw_tab_btn(
        &self,
        ui: &mut Ui,
        tab: SettingsTab,
        is_active: bool,
        style: &SettingsDialogStyle,
    ) -> Response {
        let desired_size = Vec2::new(
            DESIGN_TOKENS.sizing.settings_dialog.sidebar_width
                - DESIGN_TOKENS.sizing.settings_dialog.tab_padding_v,
            DESIGN_TOKENS.sizing.settings_dialog.tab_height,
        );
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

        // Determine colors based on state (hover, active)
        let bg_color = if is_active {
            style.tab_bg_active
        } else if response.hovered() {
            style.tab_bg_hover
        } else {
            style.tab_bg
        };

        let text_color = if is_active {
            style.tab_text_active
        } else if response.hovered() {
            style.tab_text_hover
        } else {
            style.tab_text
        };

        let icon_color = if is_active {
            style.tab_icon_active
        } else if response.hovered() {
            style.tab_icon_hover
        } else {
            style.tab_icon
        };

        // Active indicator strip on left (blue accent)
        if is_active {
            let indicator_rect = Rect::from_min_size(
                Pos2::new(
                    rect.min.x - DESIGN_TOKENS.sizing.settings_dialog.indicator_offset,
                    rect.min.y + DESIGN_TOKENS.sizing.settings_dialog.indicator_offset,
                ),
                Vec2::new(
                    DESIGN_TOKENS.sizing.settings_dialog.indicator_width,
                    rect.height() - DESIGN_TOKENS.spacing.lg,
                ),
            );
            ui.painter().rect_filled(
                indicator_rect,
                DESIGN_TOKENS.rounding.sm,
                style.tab_icon_active,
            );
        }

        // Tab background with rounded corners
        ui.painter()
            .rect_filled(rect, DESIGN_TOKENS.rounding.md, bg_color);

        // Icon
        let icon: &Icon = match tab {
            SettingsTab::Symbol => &embedded_icons::SETTINGS_SYMBOL,
            SettingsTab::StatusLine => &embedded_icons::SETTINGS_STATUS_LINE,
            SettingsTab::ScalesAndLines => &embedded_icons::SETTINGS_SCALES_LINES,
            SettingsTab::Canvas => &embedded_icons::SETTINGS_CANVAS,
            SettingsTab::Trading => &embedded_icons::SETTINGS,
            SettingsTab::Alerts => &embedded_icons::ALERTS,
            SettingsTab::Events => &embedded_icons::SETTINGS_EVENTS,
        };

        // Pos icon centered vertically, with left padding
        let icon_size = DESIGN_TOKENS.sizing.settings_dialog.tab_icon_size;
        let icon_rect = Rect::from_min_size(
            Pos2::new(
                rect.min.x + DESIGN_TOKENS.sizing.settings_dialog.tab_padding_h,
                rect.center().y - icon_size / 2.0,
            ),
            Vec2::splat(icon_size),
        );

        // Render icon with theme-aware tint color
        icon.as_image_tinted(icon_rect.size(), icon_color)
            .paint_at(ui, icon_rect);

        // Tab name
        let label_pos = Pos2::new(
            rect.min.x
                + DESIGN_TOKENS.sizing.settings_dialog.tab_padding_h
                + DESIGN_TOKENS.sizing.settings_dialog.tab_icon_size
                + DESIGN_TOKENS.sizing.settings_dialog.tab_icon_text_gap
                - DESIGN_TOKENS.spacing.lg,
            rect.center().y,
        );
        ui.painter().text(
            label_pos,
            egui::Align2::LEFT_CENTER,
            tab.name(),
            FontId::proportional(DESIGN_TOKENS.sizing.settings_dialog.tab_font_size),
            text_color,
        );

        response
    }

    fn draw_footer(
        &mut self,
        ui: &mut Ui,
        action: SettingsAction,
        style: &SettingsDialogStyle,
    ) -> SettingsAction {
        ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.button_gap);
        Self::draw_footer_separator(ui, style);
        ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.footer_padding_v);

        let result = ui
            .horizontal(|ui| {
                ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.footer_padding_h);

                let template_btn = self.draw_template_button(ui, style);
                if template_btn.clicked() {
                    self.template_menu_open = !self.template_menu_open;
                }

                let action = if self.template_menu_open {
                    self.draw_template_menu(ui, style, &template_btn, action)
                } else {
                    action
                };

                self.draw_footer_action_buttons(ui, style, action)
            })
            .inner;

        ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.footer_padding_v);
        result
    }

    fn draw_footer_separator(ui: &mut Ui, style: &SettingsDialogStyle) {
        let rect = ui.available_rect_before_wrap();
        ui.painter().hline(
            rect.min.x..=rect.max.x,
            rect.min.y,
            Stroke::new(stroke::HAIRLINE, style.footer_border),
        );
    }

    fn draw_template_button(&self, ui: &mut Ui, style: &SettingsDialogStyle) -> Response {
        let template_label = if self.template_name.is_empty() {
            "Template  ▾".to_string()
        } else {
            format!("{}  ▾", self.template_name)
        };
        ui.add(
            egui::Button::new(
                RichText::new(&template_label)
                    .size(typography::LG)
                    .color(style.val_text),
            )
            .fill(style.dropdown_bg)
            .corner_radius(DESIGN_TOKENS.rounding.md as u8)
            .min_size(Vec2::new(
                DESIGN_TOKENS.sizing.settings_dialog.template_button_width,
                DESIGN_TOKENS.sizing.settings_dialog.button_height,
            )),
        )
    }

    fn draw_template_menu(
        &mut self,
        ui: &mut Ui,
        style: &SettingsDialogStyle,
        template_btn: &Response,
        action: SettingsAction,
    ) -> SettingsAction {
        let menu_id = ui.id().with("template_menu");
        let menu_rect = Rect::from_min_size(
            template_btn.rect.left_bottom() + Vec2::new(0.0, DESIGN_TOKENS.spacing.xs),
            Vec2::new(
                DESIGN_TOKENS.sizing.settings_dialog.template_menu_width,
                DESIGN_TOKENS
                    .sizing
                    .settings_dialog
                    .template_menu_max_height,
            ),
        );

        let result = egui::Area::new(menu_id)
            .fixed_pos(menu_rect.min)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::popup(ui.style())
                    .fill(style.dropdown_bg)
                    .corner_radius(DESIGN_TOKENS.rounding.md as u8)
                    .stroke(style.dropdown_border)
                    .show(ui, |ui| {
                        ui.set_min_width(DESIGN_TOKENS.sizing.settings_dialog.template_menu_width);
                        self.draw_template_menu_items(ui, style, action)
                    })
                    .inner
            })
            .inner;

        self.handle_template_menu_close(ui, template_btn, menu_rect);
        result
    }

    fn draw_template_menu_items(
        &mut self,
        ui: &mut Ui,
        style: &SettingsDialogStyle,
        mut action: SettingsAction,
    ) -> SettingsAction {
        action = self.draw_save_as_button(ui, action);
        action = self.draw_update_current_button(ui, action);
        ui.separator();
        action = self.draw_template_list(ui, style, action);
        ui.separator();
        self.draw_reset_button(ui);
        action
    }

    fn draw_save_as_button(&mut self, ui: &mut Ui, mut action: SettingsAction) -> SettingsAction {
        if ui
            .button(RichText::new("Save as...").size(typography::MD))
            .clicked()
        {
            let name = format!("Template {}", self.template_manager.template_cnt() + 1);
            self.template_manager
                .save_template(&name, self.settings.clone());
            self.template_name = name.clone();
            action = SettingsAction::SaveTemplate(name);
            self.template_menu_open = false;
        }
        action
    }

    fn draw_update_current_button(
        &mut self,
        ui: &mut Ui,
        mut action: SettingsAction,
    ) -> SettingsAction {
        if !self.template_name.is_empty()
            && ui
                .button(RichText::new("Update current").size(typography::MD))
                .clicked()
        {
            self.template_manager
                .save_template(&self.template_name, self.settings.clone());
            action = SettingsAction::SaveTemplate(self.template_name.clone());
            self.template_menu_open = false;
        }
        action
    }

    fn draw_template_list(
        &mut self,
        ui: &mut Ui,
        style: &SettingsDialogStyle,
        mut action: SettingsAction,
    ) -> SettingsAction {
        let templates: Vec<String> = self
            .template_manager
            .list_templates()
            .iter()
            .map(|s| s.to_string())
            .collect();

        if templates.is_empty() {
            ui.label(
                RichText::new("No saved templates")
                    .size(typography::SM)
                    .color(style.section_header_text),
            );
        } else {
            for name in templates {
                action = self.draw_template_item(ui, style, name, action);
            }
        }
        action
    }

    fn draw_template_item(
        &mut self,
        ui: &mut Ui,
        style: &SettingsDialogStyle,
        name: String,
        mut action: SettingsAction,
    ) -> SettingsAction {
        let is_sel = self.template_name == name;
        let text = if is_sel {
            RichText::new(&name)
                .size(typography::MD)
                .color(style.tab_text_active)
        } else {
            RichText::new(&name).size(typography::MD)
        };

        if ui.button(text).clicked() {
            if let Some(template) = self.template_manager.get_template(&name) {
                self.settings = template.settings.clone();
                self.template_name = name.clone();
                self.update_color_pickers_from_settings();
                action = SettingsAction::LoadTemplate(name);
            }
            self.template_menu_open = false;
        }
        action
    }

    fn draw_reset_button(&mut self, ui: &mut Ui) {
        if ui
            .button(RichText::new("Reset to defaults").size(typography::MD))
            .clicked()
        {
            self.settings = ChartSettingsState::default();
            self.template_name.clear();
            self.update_color_pickers_from_settings();
            self.template_menu_open = false;
        }
    }

    fn handle_template_menu_close(&mut self, ui: &Ui, template_btn: &Response, menu_rect: Rect) {
        if ui.input(|i| i.pointer.any_click())
            && !template_btn.hovered()
            && let Some(pos) = ui.input(|i| i.pointer.interact_pos())
            && !menu_rect.contains(pos)
        {
            self.template_menu_open = false;
        }
    }

    fn draw_footer_action_buttons(
        &mut self,
        ui: &mut Ui,
        style: &SettingsDialogStyle,
        mut action: SettingsAction,
    ) -> SettingsAction {
        ui.right_aligned(|ui| {
            ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.footer_padding_h);

            if self.draw_ok_button(ui, style) {
                self.settings.sync_legacy_fields();
                action = SettingsAction::Apply(self.settings.clone());
                self.is_open = false;
            }

            ui.add_space(DESIGN_TOKENS.sizing.settings_dialog.button_gap);

            if self.draw_cancel_button(ui, style) {
                action = SettingsAction::Cancel;
                self.is_open = false;
            }
        });
        action
    }

    fn draw_ok_button(&self, ui: &mut Ui, style: &SettingsDialogStyle) -> bool {
        ui.add(
            egui::Button::new(
                RichText::new("Ok")
                    .size(typography::LG)
                    .color(style.btn_primary_text)
                    .strong(),
            )
            .fill(style.btn_primary_bg)
            .stroke(style.btn_secondary_border)
            .corner_radius(DESIGN_TOKENS.rounding.md as u8)
            .min_size(Vec2::new(
                DESIGN_TOKENS.sizing.settings_dialog.button_min_width,
                DESIGN_TOKENS.sizing.settings_dialog.button_height,
            )),
        )
        .clicked()
    }

    fn draw_cancel_button(&self, ui: &mut Ui, style: &SettingsDialogStyle) -> bool {
        ui.add(
            egui::Button::new(
                RichText::new("Cancel")
                    .size(typography::LG)
                    .color(style.btn_secondary_text),
            )
            .fill(Color32::TRANSPARENT)
            .stroke(style.btn_secondary_border)
            .corner_radius(DESIGN_TOKENS.rounding.md as u8)
            .min_size(Vec2::new(
                DESIGN_TOKENS.sizing.settings_dialog.button_min_width,
                DESIGN_TOKENS.sizing.settings_dialog.button_height,
            )),
        )
        .clicked()
    }
}
