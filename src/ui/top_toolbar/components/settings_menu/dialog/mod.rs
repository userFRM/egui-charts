//! Settings dialog with tabs
//!
//! This implementation follows the corporate architecture guidelines:
//! - Uses `crate::styles::sizing::settings_dialog` for all layout dimensions
//! - Uses `crate::theme::components::SettingsDialogStyle` for all colors
//! - Implements full interaction states (hover, active, focus)

mod tabs;

use super::{
    actions::SettingsAction, config::SettingsDialogConfig, data::ChartSettingsState,
    types::SettingsTab,
};
use crate::icons::icons as embedded_icons;
use crate::styles::typography;
use crate::theme::Theme;
use crate::theme::components::SettingsDialogStyle;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::Control;
use crate::ui::model::TemplateManager;
use crate::ui_kit::ColorPickerState;
use crate::ui_kit::dialog::DialogFooter;
use crate::ui_kit::modal::Modal;
use crate::ui_kit::sidebar_layout::{SidebarLayout, SidebarTab};
use egui::{Color32, Context, Rect, Response, RichText, Ui, Vec2};
use std::collections::HashSet;

// ============================================================================
// Settings Dialog (Modal)
// ============================================================================

/// Modal settings dialog with tabbed interface.
///
/// Edits chart display settings. Host-managed trading, alerts and events have no editable controls.
pub struct SettingsDialog {
    /// Whether the dialog is currently open
    pub is_open: bool,
    active_tab: SettingsTab,
    settings: ChartSettingsState,
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
    /// Controls the host hid.
    hidden: HashSet<Control>,
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
            active_tab: SettingsTab::Symbol,
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
            },
            margin_top_str: format!("{}", settings.margins.top_percent),
            margin_bottom_str: format!("{}", settings.margins.bottom_percent),
            margin_right_str: format!("{}", settings.margins.right_bars),
            settings,
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
            hidden: HashSet::new(),
        }
    }

    /// Shows or hides `control`: Templates (the template button and menu; Reset to defaults
    /// then takes the button's place). The dialog ignores the other controls.
    pub fn set_control_visible(&mut self, control: Control, visible: bool) {
        crate::ui::app_state::set_control_visible(&mut self.hidden, control, visible);
        if control == Control::Templates && !visible {
            self.template_menu_open = false;
        }
    }

    pub fn open(&mut self, settings: ChartSettingsState) {
        self.is_open = true;
        self.settings = settings;
        self.active_tab = SettingsTab::Symbol;
        self.update_color_pickers_from_settings();
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

        self.margin_top_str = format!("{}", self.settings.margins.top_percent);
        self.margin_bottom_str = format!("{}", self.settings.margins.bottom_percent);
        self.margin_right_str = format!("{}", self.settings.margins.right_bars);
    }

    pub fn close(&mut self) {
        self.is_open = false;
    }

    pub fn settings(&self) -> &ChartSettingsState {
        &self.settings
    }

    pub fn show(&mut self, ctx: &Context, theme: &Theme) -> SettingsAction {
        let mut action = SettingsAction::None;

        if !self.is_open {
            return action;
        }

        let basic = &self.settings.chart_basic_styles;
        for (picker, color, themed) in [
            (
                &mut self.color_picker_states.background,
                basic.background_color,
                theme.chart.bg,
            ),
            (
                &mut self.color_picker_states.background_gradient_top,
                basic.background_gradient_top,
                theme.chart.bg,
            ),
            (
                &mut self.color_picker_states.background_gradient_bottom,
                basic.background_gradient_bottom,
                theme.chart.bg_axis,
            ),
        ] {
            picker.sel_color = if color == Color32::PLACEHOLDER {
                themed
            } else {
                color
            };
        }

        let mut is_open = self.is_open;
        let mut style = SettingsDialogStyle::from_theme(theme);
        style.label_text = theme.ui.text;
        style.section_header_text = theme.ui.text_secondary;
        let height = if self.config.height > 0.0 {
            self.config.height
        } else {
            match self.active_tab {
                SettingsTab::Symbol => 616.0,
                SettingsTab::Events => 560.0,
                SettingsTab::Alerts => 500.0,
                _ => 840.0,
            }
        };
        let width = if self.config.width > 0.0 {
            self.config.width
        } else {
            752.0
        };
        Modal::new("chart_settings", "Chart settings")
            .width(width)
            .height(height)
            .show(ctx, &mut is_open, |ui| {
                ui.add_space(16.0);
                action = self.draw_content(ui, &style);
            });

        // Only update is_open from egui's Window if btns didn't already close it
        // (btns set self.is_open = false directly)
        if self.is_open && !is_open {
            // Window was closed via egui's mechanism (outside click, etc)
            self.is_open = false;
            action = SettingsAction::Cancel;
        }

        if let SettingsAction::Apply(settings) = &mut action {
            let basic = &mut settings.chart_basic_styles;
            for (color, themed) in [
                (&mut basic.background_color, theme.chart.bg),
                (&mut basic.background_gradient_top, theme.chart.bg),
                (&mut basic.background_gradient_bottom, theme.chart.bg_axis),
            ] {
                if *color == Color32::PLACEHOLDER {
                    *color = themed;
                }
            }
            settings.sync_legacy_fields();
        }

        action
    }

    fn draw_content(&mut self, ui: &mut Ui, style: &SettingsDialogStyle) -> SettingsAction {
        let mut action = SettingsAction::None;

        let content_height = (ui.available_height() - 68.0).max(0.0);

        let seam_top = ui.cursor().top();
        let seam_x = ui.cursor().left()
            + 184.0_f32.min(ui.available_width() * 0.4)
            + ui.spacing().item_spacing.x;
        let mut active = self.active_tab;
        ui.allocate_ui(Vec2::new(ui.available_width(), content_height), |ui| {
            SidebarLayout::new(184.0_f32.min(ui.available_width() * 0.4)).show(
                ui,
                |ui| {
                    egui::ScrollArea::vertical()
                        .id_salt("settings_tabs")
                        .show(ui, |ui| {
                            for tab in SettingsTab::all() {
                                if SidebarTab::new(tab.name(), active == *tab).show(ui) {
                                    active = *tab;
                                }
                            }
                        });
                },
                |ui| {
                    egui::ScrollArea::both()
                        .id_salt("settings_content")
                        .auto_shrink([false, false])
                        .max_height(ui.available_height())
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(8.0, 8.0);
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
                },
            );
        });
        self.active_tab = active;

        ui.painter().vline(
            seam_x,
            seam_top..=ui.cursor().top() + 8.0,
            egui::Stroke::new(1.0, ui.visuals().widgets.noninteractive.bg_stroke.color),
        );
        let error = self.settings.validate().err().or_else(|| {
            (self.margin_top_str.parse::<f32>().is_err()
                || self.margin_bottom_str.parse::<f32>().is_err()
                || self.margin_right_str.parse::<u32>().is_err())
            .then_some("Enter valid margins")
        });
        let (ok, cancel) = DialogFooter::new("Ok")
            .secondary("Cancel")
            .primary_enabled(error.is_none())
            .show_with_left(ui, |ui| {
                if let Some(error) = error {
                    crate::ui_kit::status_message::status_message(
                        ui,
                        crate::ui_kit::status_message::MessageKind::Error,
                        error,
                    );
                } else if self.hidden.contains(&Control::Templates) {
                    self.draw_reset_button(ui);
                } else {
                    let button = self.draw_template_button(ui, style);
                    if button.clicked() {
                        self.template_menu_open = !self.template_menu_open;
                    }
                    if self.template_menu_open {
                        action = self.draw_template_menu(ui, style, &button, action.clone());
                    }
                }
            });
        if ok {
            self.settings.sync_legacy_fields();
            action = SettingsAction::Apply(self.settings.clone());
            self.is_open = false;
        } else if cancel {
            action = SettingsAction::Cancel;
            self.is_open = false;
        }
        action
    }

    fn draw_template_button(&self, ui: &mut Ui, style: &SettingsDialogStyle) -> Response {
        let template_label = if self.template_name.is_empty() {
            "Template".to_string()
        } else {
            self.template_name.clone()
        };
        ui.add(
            egui::Button::new((
                RichText::new(&template_label)
                    .size(typography::LG)
                    .color(style.val_text),
                embedded_icons::CHEVRON_DOWN.as_image_tinted(Vec2::splat(12.0), style.val_text),
            ))
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
        let menu_id = ui.scope_id().with("template_menu");
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::Pos2;

    #[test]
    fn grid_colour_pickers_follow_the_visible_directions() {
        use super::super::GridLinesMode;
        for theme in [Theme::light(), Theme::dark()] {
            for (mode, horizontal, vertical) in [
                (GridLinesMode::None, false, false),
                (GridLinesMode::Horizontal, true, false),
                (GridLinesMode::Vertical, false, true),
                (GridLinesMode::Both, true, true),
            ] {
                let ctx = Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut settings = ChartSettingsState::default();
                settings.grid_lines.mode = mode;
                settings.grid_lines.horizontal_color = Color32::from_rgb(151, 32, 19);
                settings.grid_lines.vertical_color = Color32::from_rgb(29, 43, 157);
                let mut dialog = SettingsDialog::new();
                dialog.open(settings.clone());
                let output = ctx.run_ui(Default::default(), |ui| {
                    dialog.draw_canvas_tab(ui, &SettingsDialogStyle::from_theme(&theme));
                });
                let shown = [settings.grid_lines.horizontal_color, settings.grid_lines.vertical_color].map(|colour| output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == colour)));
                output.drop_without_applying_deltas();
                assert_eq!(shown, [horizontal, vertical], "{mode:?}");
            }
        }
    }

    #[test]
    fn settings_tracks_theme_without_changing_defaults_until_apply() {
        for (width, apply) in [
            (1440.0, false),
            (480.0, false),
            (1440.0, true),
            (480.0, true),
        ] {
            for configured in [Color32::PLACEHOLDER, Color32::from_rgb(80, 90, 100)] {
                let ctx = Context::default();
                let mut dialog = SettingsDialog::new();
                let mut settings = ChartSettingsState::default();
                settings.chart_basic_styles.background_color = configured;
                settings.margins.top_percent = 12.5;
                settings.margins.bottom_percent = 7.5;
                settings.sync_legacy_fields();
                dialog.open(settings.clone());
                assert_eq!(dialog.margin_top_str, "12.5");
                assert_eq!(dialog.margin_bottom_str, "7.5");
                let mut frame = |theme: &Theme, events| {
                    crate::theme::apply_to_egui(&ctx, theme);
                    let mut action = SettingsAction::None;
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(width, 900.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            action = dialog.show(ui.ctx(), theme);
                        },
                    );
                    let symbol = output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Body"));
                    let ok = output.shapes.iter().find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == "Ok" => {
                            Some(text.pos + text.galley.size() / 2.0)
                        }
                        _ => None,
                    });
                    output.drop_without_applying_deltas();
                    assert_eq!(dialog.settings, settings);
                    assert_eq!(
                        dialog.color_picker_states.background.sel_color,
                        if configured == Color32::PLACEHOLDER {
                            theme.chart.bg
                        } else {
                            configured
                        }
                    );
                    assert_eq!(
                        dialog.color_picker_states.background_gradient_top.sel_color,
                        theme.chart.bg
                    );
                    assert_eq!(
                        dialog
                            .color_picker_states
                            .background_gradient_bottom
                            .sel_color,
                        theme.chart.bg_axis
                    );
                    (action, symbol, ok)
                };
                for theme in [Theme::light(), Theme::dark(), Theme::light()] {
                    frame(&theme, vec![]);
                    assert!(
                        frame(&theme, vec![]).1,
                        "settings starts with the symbol controls"
                    );
                }
                let action = if apply {
                    let pos = frame(&Theme::light(), vec![]).2.unwrap();
                    let button = |pressed| egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    };
                    frame(&Theme::light(), vec![egui::Event::PointerMoved(pos)]);
                    frame(&Theme::light(), vec![button(true)]);
                    frame(&Theme::light(), vec![button(false)]).0
                } else {
                    frame(
                        &Theme::light(),
                        vec![egui::Event::Key {
                            key: egui::Key::Escape,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: Default::default(),
                        }],
                    )
                    .0
                };
                if apply {
                    let SettingsAction::Apply(saved) = action else {
                        panic!("expected Apply");
                    };
                    let expected = if configured == Color32::PLACEHOLDER {
                        Theme::light().chart.bg
                    } else {
                        configured
                    };
                    assert_eq!(saved.chart_basic_styles.background_color, expected);
                    assert_eq!(saved.background_color, expected);
                    assert_eq!(
                        saved.chart_basic_styles.background_gradient_top,
                        Theme::light().chart.bg
                    );
                    assert_eq!(
                        saved.chart_basic_styles.background_gradient_bottom,
                        Theme::light().chart.bg_axis
                    );
                } else {
                    assert!(matches!(action, SettingsAction::Cancel));
                }
                assert!(!dialog.is_open);
                assert_eq!(dialog.settings, settings);
            }
        }
    }
    #[test]
    fn choosing_a_scale_mode_clears_the_other_mode() {
        let ctx = Context::default();
        let mut dialog = SettingsDialog::new();
        let style = SettingsDialogStyle::from_theme(&Theme::light());
        for (label, expected) in [
            ("Log scale", (true, false)),
            ("Percentage scale", (false, true)),
            ("Log scale", (true, false)),
        ] {
            let mut frame = |events| {
                let output = ctx.run_ui(
                    egui::RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        dialog.draw_scales_and_lines_tab(ui, &style);
                    },
                );
                let pos = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == label => {
                            Some(text.pos + text.galley.size() / 2.0)
                        }
                        _ => None,
                    })
                    .unwrap();
                output.drop_without_applying_deltas();
                pos
            };
            let pos = frame(vec![]);
            frame(vec![egui::Event::PointerMoved(pos)]);
            for pressed in [true, false] {
                frame(vec![egui::Event::PointerButton {
                    pos,
                    pressed,
                    button: egui::PointerButton::Primary,
                    modifiers: Default::default(),
                }]);
            }
            assert_eq!(
                (
                    dialog.settings.scales_and_lines.log_scale,
                    dialog.settings.scales_and_lines.percentage_scale
                ),
                expected
            );
        }
    }

    #[test]
    fn changing_settings_tabs_stays_centered_and_fits_the_window() {
        for theme in [Theme::light(), Theme::dark()] {
            for size in [Vec2::new(1440.0, 900.0), Vec2::new(600.0, 400.0)] {
                let ctx = Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut dialog = SettingsDialog::new();
                dialog.open(ChartSettingsState::default());
                let mut tops = Vec::new();
                for tab in SettingsTab::all() {
                    dialog.active_tab = *tab;
                    for _ in 0..3 {
                        ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
                                ..Default::default()
                            },
                            |ui| {
                                dialog.show(ui.ctx(), &theme);
                            },
                        )
                        .drop_without_applying_deltas();
                    }
                    let rect = ctx
                        .memory(|memory| memory.area_rect(egui::Id::unique("chart_settings")))
                        .unwrap();
                    assert!(
                        Rect::from_min_size(Pos2::ZERO, size).contains_rect(rect),
                        "{tab:?}: {rect:?} in {size:?}"
                    );
                    tops.push(rect.center().y);
                }
                assert!(
                    tops.iter().all(|top| (top - tops[0]).abs() < 1.0),
                    "{tops:?}"
                );
            }
        }
    }
}
