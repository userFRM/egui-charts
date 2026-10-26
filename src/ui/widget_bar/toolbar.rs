//! Right Edge Icon Bar
//!
//! Vertical strip of icon btns on the far right edge of the screen.

use crate::icons::{Icon, icons as embedded_icons};
use crate::localization as l10n;
use crate::styles::icons as icon_sizes;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::Control;
use egui::{Pos2, Rect, Sense, Ui, Vec2};
use std::collections::{HashMap, HashSet};

type PanelContent = Box<dyn FnMut(&mut Ui)>;

/// Section grouping for widget bar icons
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WidgetBarSection {
    /// Top section: host entries, then Watchlist, Alerts and ObjectTree
    Top,
    /// Bottom section, at the foot of the bar: host entries, then the market and communication
    /// entries, then a line, then Help
    Bottom,
}

/// Built-in entries of the right edge bar, in the order the bar draws them.
///
/// [`WidgetBar::default`] lists Alerts, ObjectTree and Help. Add the entry of each panel whose
/// feed the host supplies to [`WidgetBar::icons`]; entries draw in this order whatever their
/// order in that list.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[non_exhaustive]
pub enum RightBarIcon {
    /// The watchlist and the details of the chart's symbol.
    Watchlist,
    Alerts,
    /// The object tree, with the data window.
    ObjectTree,
    Highlights,
    Screener,
    Hotlists,
    /// A treemap of symbols sized and coloured by host-supplied values.
    MarketHeatmap,
    /// The economic and corporate event calendars.
    Calendar,
    News,
    /// The alert and event log.
    Notifications,
    Chats,
    /// The products launcher, drawn as a filled round button.
    Products,
    Help,
}

impl RightBarIcon {
    pub fn icon(&self) -> &'static Icon {
        match self {
            RightBarIcon::Watchlist => &embedded_icons::WIDGET_BAR_WATCHLIST,
            RightBarIcon::Alerts => &embedded_icons::WIDGET_BAR_ALERTS,
            RightBarIcon::ObjectTree => &embedded_icons::WIDGET_BAR_LAYERS,
            RightBarIcon::Highlights => &embedded_icons::WIDGET_BAR_HIGHLIGHTS,
            RightBarIcon::Screener => &embedded_icons::WIDGET_BAR_SCREENER,
            RightBarIcon::Hotlists => &embedded_icons::WIDGET_BAR_HOTLISTS,
            RightBarIcon::MarketHeatmap => &embedded_icons::LAYOUT_GRID,
            RightBarIcon::Calendar => &embedded_icons::WIDGET_BAR_CALENDAR,
            RightBarIcon::News => &embedded_icons::WIDGET_BAR_NEWS,
            RightBarIcon::Notifications => &embedded_icons::WIDGET_BAR_NOTIFICATIONS,
            RightBarIcon::Chats => &embedded_icons::WIDGET_BAR_CHATS,
            RightBarIcon::Products => &embedded_icons::WIDGET_BAR_PRODUCTS,
            RightBarIcon::Help => &embedded_icons::WIDGET_BAR_HELP,
        }
    }

    pub fn tooltip(&self) -> &'static str {
        match self {
            RightBarIcon::Watchlist => "Watchlist and details",
            RightBarIcon::Alerts => "Alerts",
            RightBarIcon::ObjectTree => "Object Tree",
            RightBarIcon::Highlights => "Highlights",
            RightBarIcon::Screener => "Screener",
            RightBarIcon::Hotlists => "Hotlists",
            RightBarIcon::MarketHeatmap => "Market heatmap",
            RightBarIcon::Calendar => "Calendars",
            RightBarIcon::News => "News",
            RightBarIcon::Notifications => "Notifications",
            RightBarIcon::Chats => "Chats",
            RightBarIcon::Products => "Products",
            RightBarIcon::Help => "Help",
        }
    }

    /// Get the section this icon belongs to
    pub fn section(&self) -> WidgetBarSection {
        if *self <= RightBarIcon::ObjectTree {
            WidgetBarSection::Top
        } else {
            WidgetBarSection::Bottom
        }
    }
}

/// Configuration for the right icon bar (exact dimensions)
#[derive(Clone, Debug)]
pub struct WidgetBarConfig {
    /// Bar width, 45px by default.
    pub width: f32,
    /// Square button size, 44px by default.
    pub btn_size: f32,
    /// Icon size inside the button
    pub icon_size: f32,
    /// Button spacing - 2px gap between buttons
    pub btn_spacing: f32,
    /// Hover margin - 3px inset for hover rect
    pub hover_margin: f32,
    /// Hover rounding - 8px corners on hover
    pub hover_rounding: f32,
    /// Top padding
    pub top_padding: f32,
}

impl Default for WidgetBarConfig {
    fn default() -> Self {
        Self {
            width: 45.0,
            btn_size: DESIGN_TOKENS.sizing.toolbar.right_btn_size,
            icon_size: icon_sizes::TOOLBAR,
            btn_spacing: crate::theme::Spacing::default().xs,
            hover_margin: DESIGN_TOKENS.sizing.toolbar.right_hover_margin, // 3px
            hover_rounding: crate::theme::Radii::default().lg,
            top_padding: crate::theme::Spacing::default().sm,
        }
    }
}

/// A host-supplied widget entry. Register a content callback with [`WidgetBar::register_panel`],
/// or render its panel yourself after receiving its id.
#[derive(Clone)]
pub struct WidgetBarWidget {
    /// Stable host id, unique within the bar.
    pub id: u64,
    /// Icon drawn using the same size and tint as the built-in entries.
    pub icon: Icon,
    /// Tooltip and accessible name.
    pub tooltip: String,
    /// Host entries precede the built-in entries in their section.
    pub section: WidgetBarSection,
}

/// Action returned by the right icon bar
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum WidgetBarAction {
    None,
    Clicked(RightBarIcon),
    /// A host widget was clicked, including when it was toggled closed.
    WidgetClicked(u64),
}

/// Right Edge Icon Bar
pub struct WidgetBar {
    pub config: WidgetBarConfig,
    /// Built-in entries shown, drawn in [`RightBarIcon`] order.
    pub icons: Vec<RightBarIcon>,
    /// Currently open built-in panel. Clicks toggle this value; the host must keep it
    /// in step with panels opened or closed elsewhere before calling `show`.
    pub active_icon: Option<RightBarIcon>,
    /// Host widget entries, empty until the host supplies them.
    pub widgets: Vec<WidgetBarWidget>,
    /// Active host widget. Selecting a built-in entry clears it, and vice versa.
    pub active_widget: Option<u64>,
    pub notification_counts: std::collections::HashMap<RightBarIcon, u32>,
    /// Controls the host hid.
    hidden: HashSet<Control>,
    panels: HashMap<u64, PanelContent>,
}

/// The host control that hides `icon`, if any.
fn control(icon: RightBarIcon) -> Option<Control> {
    match icon {
        RightBarIcon::Alerts => Some(Control::Alerts),
        RightBarIcon::ObjectTree => Some(Control::ObjectTree),
        RightBarIcon::Help => Some(Control::Help),
        _ => None,
    }
}

impl Default for WidgetBar {
    fn default() -> Self {
        Self {
            config: WidgetBarConfig::default(),
            icons: vec![
                // Top section
                RightBarIcon::Alerts,
                RightBarIcon::ObjectTree,
                // Bottom section
                RightBarIcon::Help,
            ],
            active_icon: None,
            widgets: Vec::new(),
            active_widget: None,
            notification_counts: std::collections::HashMap::new(),
            hidden: HashSet::new(),
            panels: HashMap::new(),
        }
    }
}

impl WidgetBar {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_config(mut self, config: WidgetBarConfig) -> Self {
        self.config = config;
        self
    }

    /// Register a host panel, using the widget tooltip as its title and accessible name.
    ///
    /// Replacing an existing id preserves its position in the strip and its active state.
    /// The callback may capture host state and runs only while this panel is open. Call
    /// [`Self::show_panel`] after [`Self::show`] and before the central chart panel.
    /// Hosts own the contents, data sources and persistence.
    ///
    /// ```no_run
    /// use egui_charts::icons::icons;
    /// use egui_charts::ui::widget_bar::{WidgetBar, WidgetBarSection, WidgetBarWidget};
    /// let mut bar = WidgetBar::new();
    /// let mut notes = String::new();
    /// bar.register_panel(WidgetBarWidget {
    ///     id: 42, icon: icons::WATCHLIST, tooltip: "Session notes".into(),
    ///     section: WidgetBarSection::Top,
    /// }, move |ui| { ui.text_edit_multiline(&mut notes); });
    /// // In each frame: bar.show(rail_ui); bar.show_panel(root_ui); then show the chart.
    /// ```
    pub fn register_panel(
        &mut self,
        widget: WidgetBarWidget,
        content: impl FnMut(&mut Ui) + 'static,
    ) {
        self.panels.insert(widget.id, Box::new(content));
        if let Some(existing) = self.widgets.iter_mut().find(|entry| entry.id == widget.id) {
            *existing = widget;
        } else {
            self.widgets.push(widget);
        }
    }

    /// Show the active registered panel beside the strip, with a themed title and close button.
    ///
    /// Clicking the same strip entry or the close button closes it. Built-in selections clear
    /// the active host panel. Widget entries without a registered callback remain host-rendered.
    /// Panel width is resizable and retained by egui under the widget's stable id.
    pub fn show_panel(&mut self, ui: &mut Ui) {
        let Some(id) = self.active_widget else {
            return;
        };
        let Some(widget) = self.widgets.iter().find(|widget| widget.id == id) else {
            self.active_widget = None;
            return;
        };
        let Some(content) = self.panels.get_mut(&id) else {
            return;
        };
        let mut close = false;
        egui::Panel::right(ui.scope_id().with(("host_widget_panel", id)))
            .resizable(true)
            .default_size(DESIGN_TOKENS.sizing.toolbar.right_panel_width)
            .show(ui, |ui| {
                crate::ui_kit::PanelHeader::new(&widget.tooltip).show(ui, |ui| {
                    close = crate::ui_kit::buttons::dialog_close_button(ui).clicked();
                });
                if !close {
                    content(ui);
                }
            });
        if close {
            self.active_widget = None;
        }
    }

    /// Shows or hides Alerts, ObjectTree or Help and clears a hidden active entry.
    /// Other controls are ignored. Hosts close any corresponding panel they render.
    pub fn set_control_visible(&mut self, control: Control, visible: bool) {
        crate::ui::app_state::set_control_visible(&mut self.hidden, control, visible);
        if !visible && self.active_icon.and_then(self::control) == Some(control) {
            self.active_icon = None;
        }
    }

    pub fn set_notification_cnt(&mut self, icon: RightBarIcon, count: u32) {
        self.notification_counts.insert(icon, count);
    }

    /// Show the bar, keeping the bottom section at the foot and scrolling if it cannot fit.
    /// Host widget clicks return their id; the host owns the corresponding panel contents.
    pub fn show(&mut self, ui: &mut Ui) -> WidgetBarAction {
        let theme = ui.active_theme();
        let defaults = WidgetBarConfig::default();
        let spacing = if self.config.btn_spacing == defaults.btn_spacing {
            theme.spacing.xs
        } else {
            self.config.btn_spacing
        };
        let padding = if self.config.top_padding == defaults.top_padding {
            theme.spacing.sm
        } else {
            self.config.top_padding
        };
        let mut action = WidgetBarAction::None;
        ui.scope(|ui| {
            ui.spacing_mut().scroll.floating = true;
            egui::ScrollArea::vertical()
                .id_salt("widget_bar")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    let mut shown: Vec<RightBarIcon> = self
                        .icons
                        .iter()
                        .copied()
                        .filter(|&i| control(i).is_none_or(|c| !self.hidden.contains(&c)))
                        .collect();
                    shown.sort();
                    shown.dedup();
                    let count = shown.len() + self.widgets.len();
                    let pitch = self.config.btn_size + spacing;
                    let height = ui
                        .available_height()
                        .max(padding + count as f32 * pitch + ui.active_theme().spacing.lg);
                    let (rect, _) = ui
                        .allocate_exact_size(Vec2::new(self.config.width, height), Sense::hover());
                    let painter = ui.painter().clone();
                    painter.rect_filled(rect, ui.active_theme().radius.md, ui.visuals().panel_fill);
                    let mut y = rect.top() + padding;
                    for section in [WidgetBarSection::Top, WidgetBarSection::Bottom] {
                        let built_in = shown.iter().filter(|i| i.section() == section).count();
                        if section == WidgetBarSection::Bottom {
                            // Host entries sit just above the line, the built-in entries below it.
                            let count = built_in
                                + self.widgets.iter().filter(|w| w.section == section).count();
                            if count == 0 {
                                continue;
                            }
                            y = y.max(
                                rect.bottom() - count as f32 * pitch - ui.active_theme().spacing.md,
                            );
                        }
                        for widget in self.widgets.iter().filter(|w| w.section == section) {
                            let id = ui.scope_id().with(("host_widget", widget.id));
                            let response = self.render_icon_button(
                                ui,
                                id,
                                &widget.icon,
                                &widget.tooltip,
                                self.active_widget == Some(widget.id),
                                0,
                                false,
                                rect.center().x,
                                y,
                            );
                            if response.clicked() {
                                action = WidgetBarAction::WidgetClicked(widget.id);
                            }
                            y += pitch;
                        }
                        for icon in shown.iter().filter(|i| i.section() == section) {
                            if *icon == RightBarIcon::Help {
                                let sep = Rect::from_min_size(
                                    Pos2::new(rect.left() + 8.0, y),
                                    Vec2::new(self.config.width - 16.0, 1.0),
                                );
                                painter.rect_filled(sep, 0.0, ui.active_theme().ui.border);
                                y += 1.0 + ui.active_theme().spacing.md;
                                // Centre Help on the adjacent 38 px bottom row.
                                y = y.max(rect.bottom() - 19.0 - self.config.btn_size / 2.0);
                            }
                            let id = ui.scope_id().with(("built_in", icon));
                            let response = self.render_icon_button(
                                ui,
                                id,
                                icon.icon(),
                                icon.tooltip(),
                                self.active_icon == Some(*icon),
                                self.notification_counts.get(icon).copied().unwrap_or(0),
                                *icon == RightBarIcon::Products,
                                rect.center().x,
                                y,
                            );
                            if response.clicked() {
                                action = WidgetBarAction::Clicked(*icon);
                            }
                            y += pitch;
                        }
                    }
                });
        });
        match action {
            WidgetBarAction::Clicked(icon) => {
                self.active_icon = if self.active_icon == Some(icon) {
                    None
                } else {
                    Some(icon)
                };
                self.active_widget = None;
            }
            WidgetBarAction::WidgetClicked(id) => {
                self.active_widget = if self.active_widget == Some(id) {
                    None
                } else {
                    Some(id)
                };
                self.active_icon = None;
            }
            WidgetBarAction::None => {}
        }
        action
    }

    /// Render a single icon button with hover rect, `filled` drawing the icon cut out of a disc.
    fn render_icon_button(
        &self,
        ui: &mut Ui,
        id: egui::Id,
        icon: &Icon,
        tooltip: &str,
        is_active: bool,
        count: u32,
        filled: bool,
        center_x: f32,
        y: f32,
    ) -> egui::Response {
        let painter = ui.painter().clone();
        let theme = ui.active_theme();
        let rounding = if self.config.hover_rounding == WidgetBarConfig::default().hover_rounding {
            theme.radius.lg
        } else {
            self.config.hover_rounding
        };
        let icon_color = theme.ui.text;
        let icon_hover_color = theme.ui.text;
        let active_color = theme.ui.selection_text;
        let hover_bg = theme.ui.btn_bg_hover;
        let pressed_bg = theme.ui.btn_bg_active;
        let badge_color = theme.ui.danger_bg;
        let btn_rect = Rect::from_center_size(
            Pos2::new(center_x, y + self.config.btn_size / 2.0),
            Vec2::splat(self.config.btn_size),
        );

        // Inner hover rect with margins
        let hover_rect = Rect::from_min_max(
            Pos2::new(
                btn_rect.min.x + self.config.hover_margin,
                btn_rect.min.y + self.config.hover_margin,
            ),
            Pos2::new(
                btn_rect.max.x - self.config.hover_margin,
                btn_rect.max.y - self.config.hover_margin,
            ),
        );

        // Check for clicks
        let btn_res = ui.interact(btn_rect, id, Sense::click());
        crate::styles::focus::draw_focus_ring(ui, &btn_res, rounding);
        let is_hovered = btn_res.hovered();
        let is_pressed = btn_res.is_pointer_button_down_on();

        // Button background on inner rect only - with visibility check
        if ui.is_rect_visible(hover_rect) {
            if is_pressed {
                painter.rect_filled(hover_rect, rounding, pressed_bg);
            } else if is_active {
                painter.rect_filled(hover_rect, rounding, theme.ui.selection_bg);
            } else if is_hovered {
                painter.rect_filled(hover_rect, rounding, hover_bg);
            }
        }

        // Icon color based on state
        let curr_icon_color = if is_pressed {
            icon_hover_color
        } else if is_active {
            active_color
        } else if is_hovered {
            icon_hover_color
        } else {
            icon_color
        };

        // Render icon centered in button rect (with visibility check)
        let icon_rect =
            Rect::from_center_size(btn_rect.center(), Vec2::splat(self.config.icon_size));
        if ui.is_rect_visible(icon_rect) {
            let tint = if filled {
                // A 34 px disc in the icon colour, the glyph in the bar's fill.
                painter.circle_filled(btn_rect.center(), 17.0, curr_icon_color);
                ui.visuals().panel_fill
            } else {
                curr_icon_color
            };
            icon.as_image_tinted(Vec2::splat(self.config.icon_size), tint)
                .paint_at(ui, icon_rect);
        }

        // Notification badge (with visibility check)
        if count > 0 {
            let badge_radius = ui.active_theme().spacing.md;
            let badge_pos = Pos2::new(btn_rect.max.x - badge_radius, btn_rect.min.y + badge_radius);
            let badge_rect = Rect::from_center_size(badge_pos, Vec2::splat(badge_radius * 2.0));

            if ui.is_rect_visible(badge_rect) {
                painter.circle_filled(badge_pos, badge_radius, badge_color);

                let cnt_text = if count > 9 {
                    "9+".to_string()
                } else {
                    count.to_string()
                };
                let badge_galley = painter.layout_no_wrap(
                    l10n::text(ui.ctx(), cnt_text),
                    egui::FontId::proportional(ui.active_theme().text.tiny),
                    ui.active_theme().ui.danger_text,
                );
                painter.galley(
                    Pos2::new(
                        badge_pos.x - badge_galley.size().x / 2.0,
                        badge_pos.y - badge_galley.size().y / 2.0,
                    ),
                    badge_galley,
                    egui::Color32::TRANSPARENT,
                );
            }
        }

        // The bar is at the window's right edge, so the tooltip opens to the left, clear of the
        // next button. Unlike `on_hover_text`, `Tooltip` does not name the button.
        let translated_label = l10n::text(ui.ctx(), tooltip);
        btn_res.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::Role::Button,
                btn_res.enabled(),
                is_active,
                &translated_label,
            )
        });
        let mut tip = egui::Tooltip::for_enabled(&btn_res);
        tip.popup = tip.popup.align(egui::RectAlign::LEFT);
        tip.show(|ui| ui.label(l10n::text(ui.ctx(), tooltip)));

        btn_res
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::{Theme, apply_to_egui};
    use egui::{Context, Event, PointerButton, RawInput, Shape};

    #[test]
    fn registered_panels_render_toggle_replace_and_close() {
        for (theme, width) in [
            (Theme::light(), 320.0),
            (Theme::light(), 800.0),
            (Theme::dark(), 320.0),
            (Theme::dark(), 800.0),
        ] {
            let ctx = egui::Context::default();
            egui_extras::install_image_loaders(&ctx);
            crate::theme::apply_to_egui(&ctx, &theme);
            let calls = std::rc::Rc::new(std::cell::Cell::new(0));
            let mut bar = WidgetBar::new();
            let widget = WidgetBarWidget {
                id: 42,
                icon: embedded_icons::WATCHLIST,
                tooltip: "Host notes".into(),
                section: WidgetBarSection::Top,
            };
            let captured = calls.clone();
            bar.register_panel(widget.clone(), move |ui| {
                captured.set(captured.get() + 1);
                ui.label("Host panel content");
            });
            let paint = |bar: &mut WidgetBar, events| {
                let output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(width, 600.0))),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        egui::Panel::right("rail")
                            .exact_size(45.0)
                            .frame(egui::Frame::NONE)
                            .show(ui, |ui| {
                                bar.show(ui);
                            });
                        bar.show_panel(ui);
                        egui::CentralPanel::default().show(ui, |ui| {
                            ui.label("Chart area");
                        });
                    },
                );
                let text = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::Text(text) => Some(text.galley.text().to_owned()),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let icons = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::Rect(rect) if rect.brush.is_some() => Some(rect.rect.center()),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                output.drop_without_applying_deltas();
                (text, icons)
            };
            let click = |bar: &mut WidgetBar, pos| {
                paint(bar, vec![Event::PointerMoved(pos)]);
                for pressed in [true, false] {
                    paint(
                        bar,
                        vec![Event::PointerButton {
                            pos,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        }],
                    );
                }
            };
            paint(&mut bar, vec![]);
            let (_, icons) = paint(&mut bar, vec![]);
            assert_eq!(calls.get(), 0, "closed callbacks do not run");
            let host = icons[0];
            let alerts = icons[1];
            click(&mut bar, host);
            let (text, _) = paint(&mut bar, vec![]);
            assert!(text.iter().any(|label| label == "Host panel content"));
            assert!(calls.get() > 0);
            assert_eq!((bar.active_widget, bar.active_icon), (Some(42), None));
            click(&mut bar, host);
            let count = calls.get();
            assert!(
                !paint(&mut bar, vec![])
                    .0
                    .iter()
                    .any(|label| label == "Host panel content")
            );
            assert_eq!(calls.get(), count);
            click(&mut bar, host);
            click(&mut bar, alerts);
            assert_eq!(
                (bar.active_widget, bar.active_icon),
                (None, Some(RightBarIcon::Alerts))
            );
            let count = calls.get();
            paint(&mut bar, vec![]);
            assert_eq!(calls.get(), count);
            click(&mut bar, host);
            bar.register_panel(widget, |ui| {
                ui.label("Replacement content");
            });
            assert_eq!(
                bar.widgets.len(),
                1,
                "replacement retains its strip position"
            );
            assert_eq!(bar.active_widget, Some(42));
            let (text, icons) = paint(&mut bar, vec![]);
            assert!(text.iter().any(|label| label == "Replacement content"));
            assert!(!text.iter().any(|label| label == "Host panel content"));
            let close = *icons.last().expect("panel close icon");
            assert!(close.x >= 0.0 && close.x < width, "close stays reachable");
            click(&mut bar, close);
            assert_eq!(bar.active_widget, None);
            assert!(
                !paint(&mut bar, vec![])
                    .0
                    .iter()
                    .any(|label| label == "Replacement content")
            );
            bar.active_widget = Some(42);
            bar.widgets.clear();
            paint(&mut bar, vec![]);
            assert_eq!(
                bar.active_widget, None,
                "removing metadata closes the panel"
            );
        }
    }

    /// Built-in entries draw in their fixed order whatever the order of `icons`, a duplicate draws
    /// once, the line sits between the second group and Help, and Products is a disc in the icon
    /// colour with its glyph in the bar's fill.
    #[test]
    fn built_in_entries_keep_their_order_with_help_below_the_line() {
        use RightBarIcon::*;
        let all = [
            Watchlist,
            Alerts,
            ObjectTree,
            Highlights,
            Screener,
            Hotlists,
            Calendar,
            News,
            Notifications,
            Chats,
            Products,
            Help,
        ];
        for theme in [Theme::light(), Theme::dark()] {
            let ctx = Context::default();
            egui_extras::install_image_loaders(&ctx);
            apply_to_egui(&ctx, &theme);
            let mut bar = WidgetBar::new();
            bar.icons = all.iter().rev().copied().chain([Alerts]).collect();
            let frame = |bar: &mut WidgetBar, events| {
                let mut action = WidgetBarAction::None;
                let mut output = ctx.run_ui(
                    RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(45.0, 900.0))),
                        events,
                        ..Default::default()
                    },
                    |ui| action = bar.show(ui),
                );
                let shapes = std::mem::take(&mut output.shapes);
                output.drop_without_applying_deltas();
                (action, shapes)
            };
            frame(&mut bar, vec![]);
            let (_, shapes) = frame(&mut bar, vec![]);
            let icons: Vec<_> = shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    Shape::Rect(r) if r.brush.is_some() => Some((r.rect, r.fill)),
                    _ => None,
                })
                .collect();
            assert_eq!(icons.len(), all.len());
            let mut clicked = Vec::new();
            for (rect, _) in &icons {
                let pos = rect.center();
                frame(&mut bar, vec![Event::PointerMoved(pos)]);
                for pressed in [true, false] {
                    let (action, _) = frame(
                        &mut bar,
                        vec![Event::PointerButton {
                            pos,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        }],
                    );
                    if let WidgetBarAction::Clicked(icon) = action {
                        clicked.push(icon);
                    }
                }
            }
            assert_eq!(clicked, all);
            let lines: Vec<_> = shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    Shape::Rect(r) if r.rect.height() == 1.0 => Some(r.rect),
                    _ => None,
                })
                .collect();
            let (products, help) = (icons[10].0, icons[11].0);
            assert!(
                lines.len() == 1
                    && products.bottom() < lines[0].top()
                    && lines[0].bottom() < help.top(),
                "{lines:?}"
            );
            assert_eq!(icons[10].1, ctx.global_style().visuals.panel_fill);
            assert!(shapes.iter().any(|s| matches!(&s.shape, Shape::Circle(c)
                if c.radius == 17.0
                    && c.center.distance(products.center()) <= 1.0
                    && c.fill == theme.ui.text)));
        }
    }

    /// Host entries share the rail's grid and report their ids, without leaving a built-in
    /// entry selected. Clicks stay with the pressed id, and an active entry toggles closed.
    #[test]
    fn host_widgets_share_the_grid_and_report_their_own_clicks() {
        for theme in [Theme::light(), Theme::dark(), Theme::high_contrast()] {
            for height in [900.0, 450.0] {
                let ctx = Context::default();
                egui_extras::install_image_loaders(&ctx);
                apply_to_egui(&ctx, &theme);
                let mut bar = WidgetBar::new();
                assert_eq!(bar.config.width, 45.0);
                bar.widgets = vec![
                    WidgetBarWidget {
                        id: 42,
                        icon: embedded_icons::WATCHLIST,
                        tooltip: "Watchlist".into(),
                        section: WidgetBarSection::Top,
                    },
                    WidgetBarWidget {
                        id: 7,
                        icon: embedded_icons::WIDGET_BAR_CALENDAR,
                        tooltip: "Calendar".into(),
                        section: WidgetBarSection::Bottom,
                    },
                ];
                let button = |pos, pressed| Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                };
                let frame = |bar: &mut WidgetBar, events| {
                    let mut origin = Pos2::ZERO;
                    let mut action = WidgetBarAction::None;
                    let mut output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                Vec2::new(45.0, height),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            origin = ui.cursor().min;
                            action = bar.show(ui);
                        },
                    );
                    let shapes = std::mem::take(&mut output.shapes);
                    output.drop_without_applying_deltas();
                    (action, shapes, origin)
                };
                frame(&mut bar, vec![]);
                let (_, shapes, origin) = frame(&mut bar, vec![]);
                let icons: Vec<_> = shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        Shape::Rect(r) if r.brush.is_some() => Some(r.rect),
                        _ => None,
                    })
                    .collect();
                assert_eq!(icons.len(), 5);
                for icon in &icons {
                    assert_eq!(icon.size(), Vec2::splat(28.0));
                    assert!(
                        (icon.center().x - origin.x - 22.5).abs() <= 0.5,
                        "icon {icon:?}, origin {origin:?}"
                    );
                    assert!(icon.top() >= 0.0 && icon.bottom() <= height);
                }
                assert_eq!(icons.last().unwrap().center().y, height - origin.y - 19.0);
                assert_eq!(icons[0].center().y, origin.y + 26.0);
                assert_eq!(icons[1].center().y - icons[0].center().y, 46.0);
                let line = shapes
                    .iter()
                    .find_map(|s| match &s.shape {
                        Shape::Rect(r) if r.rect.height() == 1.0 => Some(r.rect),
                        _ => None,
                    })
                    .expect("bottom section line");
                assert!(
                    icons[3].bottom() < line.top() && line.bottom() < icons[4].top(),
                    "host entry above the line, Help below it: {line:?}"
                );
                bar.active_icon = Some(RightBarIcon::Alerts);
                bar.set_notification_cnt(RightBarIcon::Alerts, 3);
                let pos = icons[1].center();
                for (events, fill) in [
                    (vec![], theme.ui.selection_bg),
                    (vec![Event::PointerMoved(pos)], theme.ui.selection_bg),
                    (vec![button(pos, true)], theme.ui.btn_bg_active),
                ] {
                    let (_, shapes, _) = frame(&mut bar, events);
                    assert!(
                        shapes
                            .iter()
                            .any(|s| matches!(&s.shape, Shape::Rect(r) if r.fill == fill)),
                        "active/pressed rail fill"
                    );
                    assert!(
                        shapes.iter().any(
                            |s| matches!(&s.shape, Shape::Circle(c) if c.fill == theme.ui.danger_bg)
                        ),
                        "badge colour pair"
                    );
                }
                frame(&mut bar, vec![button(pos, false)]);
                bar.active_icon = None;
                for (index, action, active_icon, active_widget) in [
                    (0, WidgetBarAction::WidgetClicked(42), None, Some(42)),
                    (
                        1,
                        WidgetBarAction::Clicked(RightBarIcon::Alerts),
                        Some(RightBarIcon::Alerts),
                        None,
                    ),
                    (3, WidgetBarAction::WidgetClicked(7), None, Some(7)),
                    (3, WidgetBarAction::WidgetClicked(7), None, None),
                ] {
                    let pos = icons[index].center();
                    frame(&mut bar, vec![Event::PointerMoved(pos)]);
                    frame(&mut bar, vec![button(pos, true)]);
                    let (got, ..) = frame(&mut bar, vec![button(pos, false)]);
                    assert_eq!(got, action);
                    assert_eq!(
                        (bar.active_icon, bar.active_widget),
                        (active_icon, active_widget)
                    );
                }
                let pos = icons[0].center();
                frame(&mut bar, vec![Event::PointerMoved(pos)]);
                frame(&mut bar, vec![button(pos, true)]);
                bar.widgets[0].id = 99;
                let (action, ..) = frame(&mut bar, vec![button(pos, false)]);
                assert_eq!(
                    action,
                    WidgetBarAction::None,
                    "a replacement is not the pressed entry"
                );

                // Hidden built-ins leave no painted button and clear their active panel id.
                bar.widgets.clear();
                for (icon, control) in [
                    (RightBarIcon::Alerts, Control::Alerts),
                    (RightBarIcon::ObjectTree, Control::ObjectTree),
                    (RightBarIcon::Help, Control::Help),
                ] {
                    bar.active_icon = Some(icon);
                    bar.set_control_visible(control, false);
                    assert_eq!(bar.active_icon, None);
                }
                let (_, shapes, _) = frame(&mut bar, vec![]);
                assert!(
                    !shapes
                        .iter()
                        .any(|s| matches!(&s.shape, Shape::Rect(r) if r.brush.is_some()))
                );
                for control in [Control::Alerts, Control::ObjectTree, Control::Help] {
                    bar.set_control_visible(control, true);
                }
                bar.widgets.extend((100..120).map(|id| WidgetBarWidget {
                    id,
                    icon: embedded_icons::WATCHLIST,
                    tooltip: format!("Panel {id}"),
                    section: WidgetBarSection::Top,
                }));
                frame(&mut bar, vec![]);
                frame(
                    &mut bar,
                    vec![Event::PointerMoved(Pos2::new(22.0, height / 2.0))],
                );
                frame(
                    &mut bar,
                    vec![Event::MouseWheel {
                        unit: egui::MouseWheelUnit::Point,
                        delta: Vec2::new(0.0, -2000.0),
                        phase: egui::TouchPhase::Move,
                        source: egui::MouseWheelSource::Wheel,
                        modifiers: Default::default(),
                    }],
                );
                for _ in 0..30 {
                    frame(&mut bar, vec![]);
                }
                let (_, shapes, _) = frame(&mut bar, vec![]);
                let help = shapes
                    .iter()
                    .rev()
                    .find_map(|s| match &s.shape {
                        Shape::Rect(r) if r.brush.is_some() => Some(r.rect),
                        _ => None,
                    })
                    .expect("help remains reachable by scrolling");
                assert!(
                    help.top() >= 0.0 && help.bottom() <= height,
                    "help: {help:?}"
                );
                let pos = help.center();
                frame(&mut bar, vec![Event::PointerMoved(pos)]);
                for pressed in [true, false] {
                    let (action, ..) = frame(&mut bar, vec![button(pos, pressed)]);
                    if !pressed {
                        assert_eq!(action, WidgetBarAction::Clicked(RightBarIcon::Help));
                    }
                }
            }
        }
    }
}
