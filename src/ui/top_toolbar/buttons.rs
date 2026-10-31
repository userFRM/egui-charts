//! Icon, text and dropdown buttons sized for the top toolbar.
//!
//! Use these components when extending the top toolbar with host controls.

use crate::localization as l10n;
use std::sync::Arc;

use crate::icons::Icon;
use crate::styles::{icons as icon_sizes, stroke};
use egui::{Color32, Galley, Pos2, Rect, Response, Role, Sense, Ui, Vec2, WidgetInfo};

use super::TopToolbarConfig;
use crate::theme::{Theme, ThemeExt};
use crate::tokens::DESIGN_TOKENS;
use crate::ui::dialogs::keyboard_shortcuts::shortcut_key_labels;
use crate::ui_kit::ext::UiExt;

/// Menu glyph with three 18 px bars spanning 14 px on the toolbar's 24 px icon grid.
pub const MENU_ICON: Icon = Icon::new("top-toolbar-menu.svg", include_bytes!("menu.svg"));

/// Small icon-only button (config-driven size)
///
/// Used for: undo, redo, settings, theme, fullscreen, screenshot, save, etc.
///
/// Note: This is the top-toolbar-specific variant that uses [`TopToolbarConfig`]
/// for sizing. For a generic icon button, see [`crate::ui_kit::buttons::IconButton`].
pub struct ToolbarIconButton<'a> {
    /// Glyph displayed by the component.
    pub icon: &'a Icon,
    /// Help text displayed on hover.
    pub tooltip: &'static str,
    /// Display and layout options for this component.
    pub config: TopToolbarConfig,
}

impl<'a> ToolbarIconButton<'a> {
    /// Create an icon button with hover text and toolbar sizing.
    pub fn new(icon: &'a Icon, tooltip: &'static str, config: &TopToolbarConfig) -> Self {
        Self {
            icon,
            tooltip,
            config: config.clone(),
        }
    }

    /// Show the component and return its response.
    pub fn show(self, ui: &mut Ui) -> Response {
        // Match glyph ink bounds while preserving the host's configured icon scale.
        let icon_size = match self.icon.name() {
            "ui/settings.svg" => Vec2::new(24.0, 22.0),
            "ui/camera.svg" | "ui/compare-add.svg" => Vec2::splat(23.0),
            _ => Vec2::splat(24.0),
        } * (self.config.small_icon_size / 24.0);
        let btn_size = self.config.btn_size;
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(btn_size), Sense::click());
        crate::styles::focus::draw_focus_ring(ui, &response, self.config.rounding);

        // Background with theme-aware colors
        let bg_color = if response.is_pointer_button_down_on() {
            ui.visuals().widgets.active.bg_fill
        } else if response.hovered() {
            ui.visuals().widgets.hovered.bg_fill
        } else {
            Color32::TRANSPARENT
        };

        ui.painter()
            .rect_filled(rect, self.config.rounding, bg_color);

        // Icon with hover/pressed color change
        let is_hovered = response.hovered();
        let is_active = response.is_pointer_button_down_on();

        // Render icon at exact position (with visibility check)
        let icon_pos = rect.center() - icon_size / 2.0;
        let icon_rect = Rect::from_min_size(icon_pos, icon_size);
        if ui.is_rect_visible(icon_rect) {
            let icon_color = if !ui.is_enabled() {
                ui.active_theme().ui.text_disabled
            } else if is_active || is_hovered {
                ui.visuals().widgets.hovered.fg_stroke.color
            } else {
                ui.visuals().text_color()
            };
            self.icon
                .as_image_tinted(icon_size, icon_color)
                .paint_at(ui, icon_rect);
        }

        let translated_label = l10n::text(ui.ctx(), self.tooltip);
        response
            .widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), &translated_label));
        toolbar_tooltip(response, self.tooltip, None)
    }
}

/// Icon button with text label
///
/// Used for: Indicators, Alert, Replay btns.
pub struct IconTextButton<'a> {
    /// Glyph displayed by the component.
    pub icon: &'a Icon,
    /// Text displayed by the component.
    pub label: &'static str,
    /// Display and layout options for this component.
    pub config: TopToolbarConfig,
}

impl<'a> IconTextButton<'a> {
    /// Create an icon and label button with toolbar sizing.
    pub fn new(icon: &'a Icon, label: &'static str, config: &TopToolbarConfig) -> Self {
        Self {
            icon,
            label,
            config: config.clone(),
        }
    }

    pub(crate) fn width(&self, ui: &Ui) -> f32 {
        let galley = ui.painter().layout_no_wrap(
            l10n::text(ui.ctx(), self.label),
            egui::FontId::proportional(ui.active_theme().text.title),
            Color32::PLACEHOLDER,
        );
        self.config.small_icon_size + self.config.padding * 3.0 + galley.size().x
    }

    /// Show the component and return its response.
    pub fn show(self, ui: &mut Ui) -> Response {
        let icon_size = self.config.small_icon_size;
        let padding = self.config.padding;
        // Measure actual text width instead of approximation
        let total_width = self.width(ui);
        let height = self.config.btn_size;

        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(total_width, height), Sense::click());
        crate::styles::focus::draw_focus_ring(ui, &response, self.config.rounding);

        // All paint operations wrapped with visibility check
        if ui.is_rect_visible(rect) {
            // Background
            let bg_color = if response.is_pointer_button_down_on() {
                ui.visuals().widgets.active.bg_fill
            } else if response.hovered() {
                ui.visuals().widgets.hovered.bg_fill
            } else {
                Color32::TRANSPARENT
            };

            ui.painter()
                .rect_filled(rect, self.config.rounding, bg_color);

            // Hover/pressed state
            let is_hovered = response.hovered();
            let is_active = response.is_pointer_button_down_on();

            // Icon - using ui.put() for precise positioning
            let icon_pos = Pos2::new(rect.min.x + padding, rect.center().y - icon_size / 2.0);
            let icon_rect = Rect::from_min_size(icon_pos, Vec2::splat(icon_size));
            let icon_color = if is_active || is_hovered {
                ui.visuals().widgets.hovered.fg_stroke.color
            } else {
                ui.visuals().text_color()
            };
            self.icon
                .as_image_tinted(Vec2::splat(icon_size), icon_color)
                .paint_at(ui, icon_rect);

            // Text color
            let color = if is_hovered || is_active {
                ui.visuals().widgets.hovered.fg_stroke.color
            } else {
                ui.visuals().text_color()
            };

            // Text
            ui.painter().text(
                Pos2::new(rect.min.x + padding + icon_size + padding, rect.center().y),
                egui::Align2::LEFT_CENTER,
                l10n::text(ui.ctx(), self.label),
                egui::FontId::proportional(ui.active_theme().text.title),
                color,
            );
        }

        let translated_label = l10n::text(ui.ctx(), self.label);
        response
            .widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), &translated_label));
        toolbar_tooltip(
            response,
            if self.label == "Alert" {
                "Create alert"
            } else {
                self.label
            },
            None,
        )
    }
}

/// Text-only button (used for interval dropdown)
///
/// Displays text with hover/pressed background.
pub struct TextButton {
    /// Button label.
    pub text: String,
    /// Help text displayed on hover.
    pub tooltip: Option<&'static str>,
    /// Display and layout options for this component.
    pub config: TopToolbarConfig,
}

impl TextButton {
    /// Create a text button with toolbar sizing.
    pub fn new(text: String, config: &TopToolbarConfig) -> Self {
        Self {
            text,
            tooltip: None,
            config: config.clone(),
        }
    }

    /// Set help text shown on hover.
    pub fn with_tooltip(mut self, tooltip: &'static str) -> Self {
        self.tooltip = Some(tooltip);
        self
    }

    pub(crate) fn width(&self, ui: &Ui) -> f32 {
        let galley = ui.painter().layout_no_wrap(
            l10n::text(ui.ctx(), &self.text),
            egui::FontId::proportional(ui.active_theme().text.title),
            Color32::PLACEHOLDER,
        );
        galley.size().x + self.config.padding * 2.0
    }

    /// Show the component and return its response.
    pub fn show(self, ui: &mut Ui) -> Response {
        let total_width = self.width(ui);
        let height = self.config.btn_size;

        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(total_width, height), Sense::click());
        crate::styles::focus::draw_focus_ring(ui, &response, self.config.rounding);

        // Background
        let bg_color = if response.is_pointer_button_down_on() {
            ui.visuals().widgets.active.bg_fill
        } else if response.hovered() {
            ui.visuals().widgets.hovered.bg_fill
        } else {
            Color32::TRANSPARENT
        };

        ui.painter()
            .rect_filled(rect, self.config.rounding, bg_color);

        // Text
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            l10n::text(ui.ctx(), &self.text),
            egui::FontId::proportional(ui.active_theme().text.title),
            ui.visuals().text_color(),
        );

        let translated_label = l10n::text(ui.ctx(), self.tooltip.unwrap_or(&self.text));
        response.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, response.enabled(), &translated_label)
        });
        if let Some(tooltip) = self.tooltip {
            toolbar_tooltip(response, tooltip, None)
        } else {
            response
        }
    }
}

/// Vertical separator line
pub fn separator(ui: &mut Ui) {
    ui.space_lg();
    let rect = ui
        .allocate_exact_size(Vec2::new(stroke::HAIRLINE, icon_sizes::MD), Sense::hover())
        .0;
    ui.painter().rect_filled(
        rect,
        DESIGN_TOKENS.rounding.none,
        ui.visuals().widgets.noninteractive.bg_stroke.color,
    );
    ui.space_lg();
}

/// Text + icon dropdown button (Save dropdown, etc.)
pub struct TextIconButton<'a> {
    /// Button label.
    pub text: &'static str,
    /// Glyph displayed by the component.
    pub icon: &'a crate::icons::Icon,
    /// Help text displayed on hover.
    pub tooltip: &'static str,
    /// Display and layout options for this component.
    pub config: TopToolbarConfig,
}

impl<'a> TextIconButton<'a> {
    /// Create a text and icon button with hover text and toolbar sizing.
    pub fn new(
        text: &'static str,
        icon: &'a crate::icons::Icon,
        tooltip: &'static str,
        config: &TopToolbarConfig,
    ) -> Self {
        Self {
            text,
            icon,
            tooltip,
            config: config.clone(),
        }
    }

    pub(crate) fn width(&self, ui: &Ui) -> f32 {
        if self.text.is_empty() {
            return self.config.padding * 3.0 + icon_sizes::XS;
        }
        let font_id = egui::FontId::proportional(ui.active_theme().text.title);
        let galley = ui.painter().layout_no_wrap(
            l10n::text(ui.ctx(), self.text),
            font_id,
            Color32::PLACEHOLDER,
        );
        galley.rect.width() + self.config.padding * 3.0 + icon_sizes::XS
    }

    /// Show the component and return its response.
    pub fn show(self, ui: &mut Ui) -> Response {
        let padding = self.config.padding;
        let icon_size = icon_sizes::XS;
        let font_id = egui::FontId::proportional(ui.active_theme().text.title);
        let size = Vec2::new(self.width(ui), self.config.btn_size);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        crate::styles::focus::draw_focus_ring(ui, &response, self.config.rounding);

        if ui.is_rect_visible(rect) {
            // Background
            let bg_color = if response.is_pointer_button_down_on() {
                ui.visuals().widgets.active.bg_fill
            } else if response.hovered() {
                ui.visuals().widgets.hovered.bg_fill
            } else {
                Color32::TRANSPARENT
            };

            ui.painter()
                .rect_filled(rect, self.config.rounding, bg_color);

            // Text
            let text_color = if response.hovered() {
                ui.visuals().widgets.hovered.fg_stroke.color
            } else {
                ui.visuals().text_color()
            };
            if !self.text.is_empty() {
                let text_x = rect.min.x + padding;
                ui.painter().text(
                    Pos2::new(text_x, rect.center().y),
                    egui::Align2::LEFT_CENTER,
                    l10n::text(ui.ctx(), self.text),
                    font_id,
                    text_color,
                );
            }

            // Icon (chevron)
            let icon_x = rect.max.x - padding - icon_size;
            let icon_pos = Pos2::new(icon_x, rect.center().y - icon_size / 2.0);
            let icon_rect = Rect::from_min_size(icon_pos, Vec2::splat(icon_size));
            let icon_color = if response.hovered() {
                ui.visuals().widgets.hovered.fg_stroke.color
            } else {
                ui.visuals().text_color()
            };
            self.icon
                .as_image_tinted(Vec2::splat(icon_size), icon_color)
                .paint_at(ui, icon_rect);
        }

        let translated_label = l10n::text(ui.ctx(), self.tooltip);
        response
            .widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), &translated_label));
        toolbar_tooltip(response, self.tooltip, None)
    }
}

/// Neutral colour of the toolbar's group rules and pill outlines.
pub(crate) fn rule_color(theme: &Theme) -> Color32 {
    if theme.dark {
        theme.ui.border.lerp_to_gamma(theme.ui.text, 0.16)
    } else {
        theme.ui.panel_bg.lerp_to_gamma(theme.ui.text, 0.085)
    }
}

/// Labelled pill at the right end of the toolbar: outlined for Trade, filled in the text colour
/// for a host call to action.
pub(crate) struct PillButton<'a> {
    pub label: &'a str,
    pub tooltip: Option<&'a str>,
    pub filled: bool,
}

impl PillButton<'_> {
    pub(crate) fn width(&self, ui: &Ui) -> f32 {
        let font_id = egui::FontId::proportional(ui.active_theme().text.title);
        let galley = ui.painter().layout_no_wrap(
            l10n::text(ui.ctx(), self.label),
            font_id,
            Color32::PLACEHOLDER,
        );
        galley.size().x + 2.0 * ui.active_theme().spacing.xl
    }

    pub(crate) fn show(self, ui: &mut Ui) -> Response {
        let theme = ui.active_theme();
        let size = Vec2::new(self.width(ui), DESIGN_TOKENS.sizing.button_md);
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        let radius = rect.height() / 2.0;
        crate::styles::focus::draw_focus_ring(ui, &response, radius);

        if ui.is_rect_visible(rect) {
            let pressed = response.is_pointer_button_down_on();
            let hovered = response.hovered();
            let widgets = &ui.visuals().widgets;
            let (fill, text) = if self.filled {
                // Inverse of the panel: dark on light themes, light on dark ones; hover eases
                // the fill towards the panel.
                let t = if pressed {
                    0.3
                } else if hovered {
                    0.2
                } else {
                    0.0
                };
                (
                    theme.ui.text.lerp_to_gamma(theme.ui.panel_bg, t),
                    theme.ui.panel_bg,
                )
            } else if pressed {
                (widgets.active.bg_fill, theme.ui.text)
            } else if hovered {
                (widgets.hovered.bg_fill, theme.ui.text)
            } else {
                (Color32::TRANSPARENT, theme.ui.text)
            };
            ui.painter().rect_filled(rect, radius, fill);
            if !self.filled {
                ui.painter().rect_stroke(
                    rect,
                    radius,
                    egui::Stroke::new(DESIGN_TOKENS.stroke.hairline, rule_color(&theme)),
                    egui::StrokeKind::Inside,
                );
            }
            ui.painter().text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                l10n::text(ui.ctx(), self.label),
                egui::FontId::proportional(theme.text.title),
                text,
            );
        }

        let translated_label = l10n::text(ui.ctx(), self.label);
        response
            .widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), &translated_label));
        match self.tooltip {
            Some(tooltip) => toolbar_tooltip(response, tooltip, None),
            None => response,
        }
    }
}

/// Toolbar menu trigger: plain text or an icon, or a filled symbol pill.
pub(crate) struct Trigger<'a> {
    pub icon: Option<&'a Icon>,
    pub label: String,
    pub name: String,
    /// The trigger is at least this wide.
    pub min_width: f32,
    /// Filled symbol pill with a semibold label and a leading icon.
    pub filled: bool,
}

impl Trigger<'_> {
    fn galley(&self, ui: &Ui, theme: &Theme) -> Arc<Galley> {
        let mut format = egui::text::TextFormat::simple(
            egui::FontId::proportional(theme.text.title),
            theme.ui.text,
        );
        if self.filled {
            format.coords.push("wght", 600.0);
        }
        ui.painter()
            .layout_job(egui::text::LayoutJob::single_section(
                l10n::text(ui.ctx(), self.label.clone()),
                format,
            ))
    }

    fn width_for(&self, theme: &Theme, label_width: f32) -> f32 {
        let icon = self.icon.map_or(0.0, |_| icon_sizes::LG);
        let gap = if icon > 0.0 && !self.label.is_empty() {
            theme.spacing.sm
        } else {
            0.0
        };
        (icon + gap + label_width + theme.spacing.lg * 2.0).max(self.min_width)
    }

    pub(crate) fn show(self, ui: &mut Ui, open: bool) -> Response {
        let theme = ui.active_theme();
        let galley = self.galley(ui, &theme);
        let size = Vec2::new(
            self.width_for(&theme, galley.size().x),
            if self.filled {
                28.0
            } else {
                super::config::BUTTON_SIZE
            },
        );
        let (rect, response) = ui.allocate_exact_size(size, Sense::click());
        crate::styles::focus::draw_focus_ring(
            ui,
            &response,
            if self.filled {
                rect.height() / 2.0
            } else {
                theme.radius.md
            },
        );

        if ui.is_rect_visible(rect) {
            let widgets = &ui.visuals().widgets;
            let pressed = open || response.is_pointer_button_down_on();
            let hovered = response.hovered();
            let fill = if pressed {
                widgets.active.bg_fill
            } else if hovered {
                widgets.hovered.bg_fill
            } else if self.filled {
                theme.ui.panel_bg.lerp_to_gamma(theme.ui.text, 0.085)
            } else {
                Color32::TRANSPARENT
            };
            let icon_color = if pressed || hovered {
                widgets.hovered.fg_stroke.color
            } else {
                theme.ui.text
            };
            ui.painter().rect_filled(
                rect,
                if self.filled {
                    rect.height() / 2.0
                } else {
                    theme.radius.md
                },
                fill,
            );

            let paint_icon = |icon: &Icon, x: f32, size: f32| {
                let min = Pos2::new(x, rect.center().y - size / 2.0);
                icon.as_image_tinted(Vec2::splat(size), icon_color)
                    .paint_at(ui, Rect::from_min_size(min, Vec2::splat(size)));
            };
            let content_width = galley.size().x
                + self.icon.map_or(0.0, |_| icon_sizes::LG)
                + if self.icon.is_some() && !self.label.is_empty() {
                    theme.spacing.sm
                } else {
                    0.0
                };
            let x = if self.filled {
                rect.min.x + theme.spacing.lg
            } else {
                rect.center().x - content_width / 2.0
            };
            let label_x = x + self.icon.map_or(0.0, |_| icon_sizes::LG + theme.spacing.sm);
            let label_pos = Pos2::new(label_x, rect.center().y - galley.size().y / 2.0);
            ui.painter().galley(label_pos, galley, theme.ui.text);
            if let Some(icon) = self.icon {
                paint_icon(icon, x, icon_sizes::LG);
            }
        }

        let translated_label = l10n::text(ui.ctx(), &self.name);
        response
            .widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), &translated_label));
        toolbar_tooltip(response, &self.name, None)
    }
}

/// Fill of the hover labels, which stay dark in every preset.
const HOVER_LABEL_FILL: Color32 = Color32::from_gray(28);
/// Text of the hover labels.
const HOVER_LABEL_TEXT: Color32 = Color32::from_gray(240);
/// Key chips of the hover labels, and the rule before them.
const HOVER_LABEL_KEY: Color32 = Color32::from_gray(99);
/// Height of a hover label's row, which its 3 px margins bring to 24 px.
const HOVER_LABEL_ROW: f32 = 18.0;
/// Text size of the hover labels.
const HOVER_LABEL_TEXT_SIZE: f32 = 13.0;

/// One run of a hover label, as tall as the row whatever the font's line height.
fn hover_label_run(ui: &mut Ui, text: String, padding: f32, fill: Color32, color: Color32) {
    let font = egui::FontId::proportional(HOVER_LABEL_TEXT_SIZE);
    let galley = ui.painter().layout_no_wrap(text, font, color);
    let size = Vec2::new(galley.size().x + 2.0 * padding, HOVER_LABEL_ROW);
    let (rect, response) = ui.allocate_exact_size(size, Sense::hover());
    response.widget_info(|| WidgetInfo::labeled(Role::Label, true, galley.text()));
    ui.painter()
        .rect_filled(rect, ui.active_theme().radius.xs, fill);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, color);
}

/// Attach a compact toolbar tooltip, with optional platform-specific shortcut chips.
///
/// Uses the normal hover delay and stays below the trigger while there is room.
pub fn toolbar_tooltip(
    response: Response,
    text: &str,
    shortcut: Option<egui::KeyboardShortcut>,
) -> Response {
    let shortcut = shortcut.or_else(|| {
        let action = match text {
            "Quick Search" | "Quick search" => crate::config::KeyboardAction::OpenQuickSearch,
            "Indicators" => crate::config::KeyboardAction::OpenIndicatorSearch,
            "Create alert" => crate::config::KeyboardAction::CreateAlert,
            _ => return None,
        };
        response
            .ctx
            .data(|data| {
                data.get_temp::<crate::widget::ShortcutMap>(egui::Id::NULL.with("chart_shortcuts"))
            })
            .unwrap_or_default()
            .shortcut(action)
    });
    egui::Popup::from_response(&response)
        .id(response.id.with("toolbar_tooltip"))
        .kind(egui::PopupKind::Tooltip)
        .align(egui::RectAlign::BOTTOM)
        .gap(9.0)
        .sense(Sense::hover())
        .interactable(false)
        .open(response.enabled() && egui::Tooltip::should_show_tooltip(&response, false))
        .frame(
            egui::Frame::new()
                .fill(HOVER_LABEL_FILL)
                .corner_radius(response.ctx.active_theme().radius.sm)
                .inner_margin(egui::Margin::symmetric(9, 3)),
        )
        .show(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.spacing_mut().interact_size.y = HOVER_LABEL_ROW;
            ui.horizontal(|ui| {
                let label = l10n::text(ui.ctx(), text);
                hover_label_run(ui, label, 0.0, Color32::TRANSPARENT, HOVER_LABEL_TEXT);
                if let Some(shortcut) = shortcut {
                    // With the row's 4 px spacing, 9 px after the text and 8 px before the chip.
                    ui.add_space(5.0);
                    let (rule, _) =
                        ui.allocate_exact_size(Vec2::new(1.0, HOVER_LABEL_ROW), Sense::hover());
                    ui.painter().rect_filled(rule, 0.0, HOVER_LABEL_KEY);
                    ui.add_space(4.0);
                    let font = egui::FontId::proportional(HOVER_LABEL_TEXT_SIZE);
                    let keys = shortcut_key_labels(ui.ctx(), &font, &shortcut);
                    for (index, key) in keys.iter().enumerate() {
                        if index > 0 {
                            let plus = "+".to_owned();
                            hover_label_run(ui, plus, 0.0, Color32::TRANSPARENT, Color32::WHITE);
                        }
                        let key = l10n::text(ui.ctx(), key);
                        hover_label_run(ui, key, 4.0, HOVER_LABEL_KEY, Color32::WHITE);
                    }
                }
            });
            let x = response
                .rect
                .center()
                .x
                .clamp(ui.min_rect().left(), ui.min_rect().right());
            let y = ui.min_rect().top() - 4.0;
            ui.painter()
                .with_clip_rect(egui::Rect::EVERYTHING)
                .add(egui::Shape::convex_polygon(
                    vec![
                        egui::pos2(x - 5.0, y),
                        egui::pos2(x, y - 5.0),
                        egui::pos2(x + 5.0, y),
                    ],
                    HOVER_LABEL_FILL,
                    egui::Stroke::NONE,
                ));
        });
    response
}
