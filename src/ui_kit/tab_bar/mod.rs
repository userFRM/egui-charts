//! Tab bar component with underline indicator.
//!
//! Provides a horizontal tab bar that renders clickable text labels with
//! an accent-colored underline on the active tab. Used across all tabbed dialogs.
//!
//! # Example
//! ```
//! use egui_charts::ui_kit::tab_bar::{TabBar, TabLabel};
//!
//! #[derive(Clone, Copy, PartialEq)]
//! enum MyTab {
//!     Chart,
//!     Orders,
//! }
//!
//! impl TabLabel for MyTab {
//!     fn tab_label(&self) -> &str {
//!         match self {
//!             MyTab::Chart => "Chart",
//!             MyTab::Orders => "Orders",
//!         }
//!     }
//! }
//!
//! # egui::__run_test_ui(|ui| {
//! # let mut active_tab = MyTab::Chart;
//! TabBar::new(&[MyTab::Chart, MyTab::Orders], &mut active_tab).show(ui);
//! # });
//! ```

use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::buttons::ButtonSize;
use egui::{Role, Sense, Ui, WidgetInfo, vec2};

/// Trait for tab enum types to provide their display label.
///
/// Implement this on your tab enum so `TabBar` can render it.
pub trait TabLabel: Copy + PartialEq {
    fn tab_label(&self) -> &str;
}

/// Horizontal tab bar with underline indicator on the active tab.
///
/// Renders clickable text labels 16 px apart over a full-width rule, starting at the content
/// edge. The active tab's accent underline sits on the rule, and content starts 8 px below it.
pub struct TabBar<'a, T: TabLabel> {
    tabs: &'a [T],
    active: &'a mut T,
    pills: bool,
}

impl<'a, T: TabLabel> TabBar<'a, T> {
    pub fn new(tabs: &'a [T], active: &'a mut T) -> Self {
        Self {
            tabs,
            active,
            pills: false,
        }
    }

    /// Render rounded chips instead of an underline.
    pub fn pills(mut self) -> Self {
        self.pills = true;
        self
    }

    /// Show the tab bar. Returns `true` if the active tab changed.
    pub fn show(self, ui: &mut Ui) -> bool {
        let theme = ui.active_theme();
        let mut changed = false;
        let rule_slot = ui.painter().add(egui::Shape::Noop);

        let row = ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = if self.pills { 4.0 } else { theme.spacing.xxl };
            for tab in self.tabs {
                let is_active = *tab == *self.active;
                let galley = ui.painter().layout_no_wrap(
                    tab.tab_label().to_owned(),
                    egui::FontId::proportional(if self.pills { 14.0 } else { theme.text.label }),
                    egui::Color32::PLACEHOLDER,
                );
                let (rect, response) = ui.allocate_exact_size(
                    vec2(
                        galley.size().x + if self.pills { 24.0 } else { 0.0 },
                        ButtonSize::MD.pixels(),
                    ),
                    Sense::click(),
                );
                response
                    .widget_info(|| WidgetInfo::labeled(Role::Tab, ui.is_enabled(), galley.text()));
                ui.ctx()
                    .accesskit_node_builder(response.id, |node| node.set_selected(is_active));

                let visuals = ui.visuals();
                if self.pills {
                    let fill = if is_active {
                        theme.ui.text
                    } else {
                        theme.ui.btn_bg_hover
                    };
                    ui.painter().rect_filled(rect, rect.height() / 2.0, fill);
                }
                let text_color = if self.pills && is_active {
                    theme.ui.panel_bg
                } else if is_active {
                    visuals.strong_text_color()
                } else if response.hovered() {
                    visuals.widgets.hovered.fg_stroke.color
                } else {
                    visuals.text_color()
                };
                let text_pos = rect.center() - galley.size() / 2.0;
                ui.painter().galley(text_pos, galley, text_color);

                if is_active && !self.pills {
                    let thick = DESIGN_TOKENS.stroke.thick;
                    ui.painter().hline(
                        rect.x_range(),
                        rect.bottom() - thick / 2.0,
                        egui::Stroke::new(thick, theme.ui.accent),
                    );
                }
                crate::styles::focus::draw_focus_ring(
                    ui,
                    &response,
                    if self.pills { rect.height() / 2.0 } else { 0.0 },
                );

                if response.clicked() {
                    *self.active = *tab;
                    changed = true;
                }
            }
        });

        // The rule spans the parent's width, under the active underline.
        let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
        let rule = egui::Shape::hline(
            ui.max_rect().x_range(),
            row.response.rect.bottom() - stroke.width / 2.0,
            stroke,
        );
        if !self.pills {
            ui.painter().set(rule_slot, rule);
        }
        ui.add_space(theme.spacing.lg - ui.spacing().item_spacing.y);

        changed
    }
}
