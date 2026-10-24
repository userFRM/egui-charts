//! Reusable button components for the top toolbar.
//!
//! Eliminates ~180 lines of repetitive button rendering code.

use std::sync::Arc;

use crate::icons::Icon;
use crate::styles::{icons as icon_sizes, stroke};
use egui::{Color32, Galley, Pos2, Rect, Response, Role, Sense, Ui, Vec2, WidgetInfo};

use super::TopToolbarConfig;
use crate::ext::UiExt;
use crate::theme::{Theme, ThemeExt};
use crate::tokens::DESIGN_TOKENS;

/// Small icon-only button (config-driven size)
///
/// Used for: undo, redo, settings, theme, fullscreen, screenshot, save, etc.
/// This component eliminates ~161 lines of duplication (used 7 times in show()).
///
/// Note: This is the top-toolbar-specific variant that uses [`TopToolbarConfig`]
/// for sizing. For a generic icon button, see [`crate::ui_kit::buttons::IconButton`].
pub struct ToolbarIconButton<'a> {
    pub icon: &'a Icon,
    pub tooltip: &'static str,
    pub config: TopToolbarConfig,
}

impl<'a> ToolbarIconButton<'a> {
    pub fn new(icon: &'a Icon, tooltip: &'static str, config: &TopToolbarConfig) -> Self {
        Self {
            icon,
            tooltip,
            config: config.clone(),
        }
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let icon_size = self.config.small_icon_size;
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
        let icon_pos = rect.center() - Vec2::splat(icon_size / 2.0);
        let icon_rect = Rect::from_min_size(icon_pos, Vec2::splat(icon_size));
        if ui.is_rect_visible(icon_rect) {
            let icon_color = if is_active || is_hovered {
                ui.visuals().widgets.hovered.fg_stroke.color
            } else {
                ui.visuals().text_color()
            };
            self.icon
                .as_image_tinted(Vec2::splat(icon_size), icon_color)
                .paint_at(ui, icon_rect);
        }

        response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), self.tooltip));
        toolbar_tooltip(response, self.tooltip, None)
    }
}

/// Icon button with text label
///
/// Used for: Indicators, Alert, Replay btns.
/// This component eliminates ~117 lines of duplication (used 3 times in show()).
pub struct IconTextButton<'a> {
    pub icon: &'a Icon,
    pub label: &'static str,
    pub config: TopToolbarConfig,
}

impl<'a> IconTextButton<'a> {
    pub fn new(icon: &'a Icon, label: &'static str, config: &TopToolbarConfig) -> Self {
        Self {
            icon,
            label,
            config: config.clone(),
        }
    }

    pub(crate) fn width(&self, ui: &Ui) -> f32 {
        let galley = ui.painter().layout_no_wrap(
            self.label.to_owned(),
            egui::FontId::proportional(ui.active_theme().text.title),
            Color32::PLACEHOLDER,
        );
        self.config.small_icon_size + self.config.padding * 3.0 + galley.size().x
    }

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
                self.label,
                egui::FontId::proportional(ui.active_theme().text.title),
                color,
            );
        }

        response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), self.label));
        toolbar_tooltip(
            response,
            if self.label == "Alert" {
                "Create alert"
            } else {
                self.label
            },
            match self.label {
                "Indicators" => Some(crate::config::KeyboardAction::OpenIndicatorSearch),
                "Alert" => Some(crate::config::KeyboardAction::CreateAlert),
                _ => None,
            }
            .and_then(|action| {
                ui.ctx()
                    .data(|data| {
                        data.get_temp::<crate::widget::ShortcutMap>(
                            egui::Id::NULL.with("chart_shortcuts"),
                        )
                    })
                    .unwrap_or_default()
                    .shortcut(action)
            }),
        )
    }
}

/// Text-only button (used for interval dropdown)
///
/// Displays text with hover/pressed background.
pub struct TextButton {
    pub text: String,
    pub tooltip: Option<&'static str>,
    pub config: TopToolbarConfig,
}

impl TextButton {
    pub fn new(text: String, config: &TopToolbarConfig) -> Self {
        Self {
            text,
            tooltip: None,
            config: config.clone(),
        }
    }

    pub fn with_tooltip(mut self, tooltip: &'static str) -> Self {
        self.tooltip = Some(tooltip);
        self
    }

    pub fn show(self, ui: &mut Ui) -> Response {
        let padding = self.config.padding;
        // Measure actual text width
        let font_id = egui::FontId::proportional(ui.active_theme().text.title);
        let galley = ui.painter().layout_no_wrap(
            self.text.clone(),
            font_id,
            DESIGN_TOKENS.semantic.ui.text_light,
        );
        let text_width = galley.rect.width();
        let total_width = text_width + padding * 2.0;
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
            &self.text,
            egui::FontId::proportional(ui.active_theme().text.title),
            ui.visuals().text_color(),
        );

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
    pub text: &'static str,
    pub icon: &'a crate::icons::Icon,
    pub tooltip: &'static str,
    pub config: TopToolbarConfig,
}

impl<'a> TextIconButton<'a> {
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
        let font_id = egui::FontId::proportional(ui.active_theme().text.title);
        let galley =
            ui.painter()
                .layout_no_wrap(self.text.to_owned(), font_id, Color32::PLACEHOLDER);
        galley.rect.width() + self.config.padding * 3.0 + icon_sizes::XS
    }

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
            let text_x = rect.min.x + padding;
            ui.painter().text(
                Pos2::new(text_x, rect.center().y),
                egui::Align2::LEFT_CENTER,
                self.text,
                font_id,
                text_color,
            );

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

        response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), self.tooltip));
        toolbar_tooltip(response, self.tooltip, None)
    }
}

/// Toolbar menu trigger: plain text or an icon, or a filled symbol pill.
pub(crate) struct Trigger<'a> {
    pub icon: Option<&'a Icon>,
    pub label: String,
    pub name: String,
    /// The trigger is at least this wide.
    pub min_width: f32,
    /// Filled symbol pill with a semibold label and a trailing icon.
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
                self.label.clone(),
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

    pub(crate) fn width(&self, ui: &Ui) -> f32 {
        let theme = ui.active_theme();
        self.width_for(&theme, self.galley(ui, &theme).size().x)
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
                rect.min.x + theme.spacing.lg + theme.spacing.sm
            } else {
                rect.center().x - content_width / 2.0
            };
            let label_pos = Pos2::new(x, rect.center().y - galley.size().y / 2.0);
            ui.painter().galley(label_pos, galley, theme.ui.text);
            if let Some(icon) = self.icon {
                let icon_x = if self.filled {
                    rect.max.x - theme.spacing.sm - icon_sizes::LG
                } else {
                    x
                };
                paint_icon(icon, icon_x, icon_sizes::LG);
            }
        }

        response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), &self.name));
        toolbar_tooltip(response, &self.name, None)
    }
}

/// Attach a compact toolbar tooltip, with optional platform-specific shortcut chips.
///
/// Uses the normal hover delay and stays below the trigger while there is room.
pub fn toolbar_tooltip(
    response: Response,
    text: &str,
    shortcut: Option<egui::KeyboardShortcut>,
) -> Response {
    let fill = Color32::from_gray(28);
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
                .fill(fill)
                .corner_radius(3)
                .inner_margin(egui::Margin::symmetric(8, 3)),
        )
        .show(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            ui.spacing_mut().interact_size.y = 17.0;
            ui.horizontal(|ui| {
                ui.label(
                    egui::RichText::new(text)
                        .size(14.0)
                        .color(Color32::from_gray(240)),
                );
                if let Some(shortcut) = shortcut {
                    ui.add_space(4.0);
                    for (index, key) in ui.ctx().format_shortcut(&shortcut).split('+').enumerate() {
                        if index > 0 {
                            ui.label(egui::RichText::new("+").size(14.0).color(Color32::WHITE));
                        }
                        egui::Frame::new()
                            .fill(Color32::from_gray(80))
                            .corner_radius(2)
                            .inner_margin(egui::Margin::symmetric(4, 0))
                            .show(ui, |ui| {
                                ui.label(egui::RichText::new(key).size(14.0).color(Color32::WHITE));
                            });
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
                    fill,
                    egui::Stroke::NONE,
                ));
        });
    response
}
