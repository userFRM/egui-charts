//! Timeframe Selector
//!
//! A dropdown menu for selecting chart timeframes with favorites support.
use crate::icons::icons as embedded_icons;
use crate::styles::focus::focus_ring_color;
use crate::styles::icons as icon_sizes;
use crate::theme::{Theme, ThemeExt};
use crate::tokens::DESIGN_TOKENS;
use crate::ui::top_toolbar::buttons::Trigger;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::list_item::{ContentContext, LabelContent, ListItem, ListItemContent};
use egui::{Color32, Rect, RichText, Role, Ui, WidgetInfo, pos2, vec2};
use std::collections::HashSet;

/// Height of the interval menu's rows and section headers.
const ROW_H: f32 = 32.0;

/// Least width of the menu, so that the custom interval form fits a field, a unit button and "Add".
const CUSTOM_FORM_W: f32 = 232.0;

/// Timeframe unit
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeframeUnit {
    Second,
    Minute,
    Hour,
    Day,
    Week,
    Month,
}

impl TimeframeUnit {
    pub fn short_name(&self) -> &'static str {
        match self {
            TimeframeUnit::Second => "S",
            TimeframeUnit::Minute => "m",
            TimeframeUnit::Hour => "h",
            TimeframeUnit::Day => "D",
            TimeframeUnit::Week => "W",
            TimeframeUnit::Month => "M",
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            TimeframeUnit::Second => "seconds",
            TimeframeUnit::Minute => "minutes",
            TimeframeUnit::Hour => "hours",
            TimeframeUnit::Day => "days",
            TimeframeUnit::Week => "weeks",
            TimeframeUnit::Month => "months",
        }
    }
}

/// Timeframe definition
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Timeframe {
    pub value: u32,
    pub unit: TimeframeUnit,
}

impl Timeframe {
    pub const fn new(value: u32, unit: TimeframeUnit) -> Self {
        Self { value, unit }
    }

    /// Get display string like "1D", "4h", "15m"
    pub fn display(&self) -> String {
        format!("{}{}", self.value, self.unit.short_name())
    }

    /// Get long display string like "1 day", "4 hours"
    pub fn display_long(&self) -> String {
        let unit_name = if self.value == 1 {
            match self.unit {
                TimeframeUnit::Second => "second",
                TimeframeUnit::Minute => "minute",
                TimeframeUnit::Hour => "hour",
                TimeframeUnit::Day => "day",
                TimeframeUnit::Week => "week",
                TimeframeUnit::Month => "month",
            }
        } else {
            self.unit.name()
        };
        format!("{} {}", self.value, unit_name)
    }

    /// The model timeframe this entry selects: the preset of the same length, else a custom
    /// one. `None` only for a zero value.
    pub(crate) fn to_model(self) -> Option<crate::model::Timeframe> {
        let unit_seconds: u64 = match self.unit {
            TimeframeUnit::Second => 1,
            TimeframeUnit::Minute => 60,
            TimeframeUnit::Hour => 3_600,
            TimeframeUnit::Day => 86_400,
            TimeframeUnit::Week => 604_800,
            // The model's month is 30 days.
            TimeframeUnit::Month => 2_592_000,
        };
        // At most `u32::MAX` months, so neither product overflows a `u64`.
        let seconds = u64::from(self.value) * unit_seconds;
        let custom = crate::model::Timeframe::Custom(std::num::NonZeroU64::new(seconds)?);
        Some(
            crate::model::Timeframe::all()
                .into_iter()
                .find(|tf| tf.duration_ms() as u64 == seconds * 1000)
                .unwrap_or(custom),
        )
    }

    /// Get all predefined timeframes
    pub fn all_presets() -> Vec<Timeframe> {
        vec![
            // Seconds
            Timeframe::new(1, TimeframeUnit::Second),
            Timeframe::new(5, TimeframeUnit::Second),
            Timeframe::new(10, TimeframeUnit::Second),
            Timeframe::new(15, TimeframeUnit::Second),
            Timeframe::new(30, TimeframeUnit::Second),
            Timeframe::new(45, TimeframeUnit::Second),
            // Minutes
            Timeframe::new(1, TimeframeUnit::Minute),
            Timeframe::new(2, TimeframeUnit::Minute),
            Timeframe::new(3, TimeframeUnit::Minute),
            Timeframe::new(5, TimeframeUnit::Minute),
            Timeframe::new(10, TimeframeUnit::Minute),
            Timeframe::new(15, TimeframeUnit::Minute),
            Timeframe::new(30, TimeframeUnit::Minute),
            Timeframe::new(45, TimeframeUnit::Minute),
            // Hours
            Timeframe::new(1, TimeframeUnit::Hour),
            Timeframe::new(2, TimeframeUnit::Hour),
            Timeframe::new(3, TimeframeUnit::Hour),
            Timeframe::new(4, TimeframeUnit::Hour),
            // Day/Week/Month
            Timeframe::new(1, TimeframeUnit::Day),
            Timeframe::new(1, TimeframeUnit::Week),
            Timeframe::new(1, TimeframeUnit::Month),
        ]
    }

    /// Get timeframes by unit
    pub fn by_unit(unit: TimeframeUnit) -> Vec<Timeframe> {
        Self::all_presets()
            .into_iter()
            .filter(|tf| tf.unit == unit)
            .collect()
    }
}

/// Configuration for timeframe selector
#[derive(Debug, Clone)]
pub struct TimeframeSelectorConfig {
    /// Minimum width of the button; it is otherwise sized to its label.
    pub btn_width: f32,
    /// Minimum width of the menu; never below 232 px, the width the custom interval form needs.
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
    /// Not read: the menu takes its colours from the applied theme.
    pub favorite_color: Color32,
}

impl Default for TimeframeSelectorConfig {
    fn default() -> Self {
        // Colors are TRANSPARENT to signal that colors should be fetched from
        // ui.style().visuals at render time for proper theme support.
        Self {
            btn_width: 0.0,
            dropdown_width: 180.0,
            item_height: DESIGN_TOKENS.sizing.button_md,
            bg_color: Color32::TRANSPARENT,
            hover_color: Color32::TRANSPARENT,
            sel_color: Color32::TRANSPARENT,
            text_color: Color32::TRANSPARENT,
            muted_color: Color32::TRANSPARENT,
            favorite_color: Color32::TRANSPARENT,
        }
    }
}

/// Action from timeframe selector
#[derive(Debug, Clone, PartialEq)]
pub enum TimeframeAction {
    None,
    Select(Timeframe),
    ToggleFavorite(Timeframe),
    /// Not emitted: a custom interval is returned as `Select`.
    AddCustom,
}

/// Custom timeframe dialog state
#[derive(Debug, Clone)]
pub struct CustomTimeframeDialog {
    /// The numeric value input
    pub value_input: String,
    /// The selected unit
    pub unit: TimeframeUnit,
    /// Validation error message
    pub error: Option<String>,
}

impl Default for CustomTimeframeDialog {
    fn default() -> Self {
        Self {
            value_input: "1".to_string(),
            unit: TimeframeUnit::Minute,
            error: None,
        }
    }
}

impl CustomTimeframeDialog {
    /// Validate and return the custom Timeframe, or None if invalid
    pub fn validate(&mut self) -> Option<Timeframe> {
        let trimmed = self.value_input.trim();
        if trimmed.is_empty() {
            self.error = Some("Enter a number".to_string());
            return None;
        }
        match trimmed.parse::<u32>() {
            Ok(0) => {
                self.error = Some("Value must be > 0".to_string());
                None
            }
            Ok(v) if v > 10_000 => {
                self.error = Some("Value too large (max 10000)".to_string());
                None
            }
            Ok(v) => {
                self.error = None;
                Some(Timeframe::new(v, self.unit))
            }
            Err(_) => {
                self.error = Some("Enter a valid number".to_string());
                None
            }
        }
    }

    /// Available units for the custom dialog
    fn available_units() -> &'static [(TimeframeUnit, &'static str)] {
        &[
            (TimeframeUnit::Second, "Seconds"),
            (TimeframeUnit::Minute, "Minutes"),
            (TimeframeUnit::Hour, "Hours"),
            (TimeframeUnit::Day, "Days"),
        ]
    }

    /// Show the custom timeframe dialog inside the dropdown.
    /// Returns Some(Timeframe) if the user confirms, None otherwise.
    pub fn show(&mut self, ui: &mut Ui) -> Option<Timeframe> {
        let theme = ui.active_theme();
        let mut result = None;

        ui.add_space(theme.spacing.lg);
        // Right to left, so that the field takes the width the buttons leave.
        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), DESIGN_TOKENS.sizing.button_md),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                ui.add_space(theme.spacing.lg);
                // The controls share the height of the button.
                ui.spacing_mut().interact_size.y = DESIGN_TOKENS.sizing.button_md;
                // The menu style draws inactive controls without a frame; the form's fields need one.
                let inactive = &mut ui.visuals_mut().widgets.inactive;
                inactive.bg_stroke = egui::Stroke::new(1.0, theme.ui.border_control);
                inactive.weak_bg_fill = theme.ui.btn_bg_hover;
                if Button::new("Add").primary().show(ui).clicked() {
                    result = self.validate();
                }
                let unit = Self::available_units()
                    .iter()
                    .find(|(u, _)| *u == self.unit)
                    .map_or_else(|| self.unit.name(), |(_, label)| label);
                // A submenu keeps the interval menu open when a unit is picked.
                ui.menu_button(unit, |ui| {
                    for (u, label) in Self::available_units() {
                        if ui.selectable_label(self.unit == *u, *label).clicked() {
                            self.unit = *u;
                        }
                    }
                });
                ui.add(
                    egui::TextEdit::singleline(&mut self.value_input)
                        .desired_width(ui.available_width() - theme.spacing.lg)
                        .align(egui::Align2::LEFT_CENTER)
                        .font(egui::FontId::proportional(theme.text.body)),
                );
            },
        );

        if let Some(err) = &self.error {
            ui.horizontal(|ui| {
                ui.add_space(theme.spacing.lg);
                ui.label(
                    RichText::new(err)
                        .size(theme.text.small)
                        .color(ui.visuals().error_fg_color),
                );
            });
        }

        result
    }
}

/// Timeframe selector
pub struct TimeframeSelector {
    /// Currently selected timeframe
    pub current: Timeframe,
    /// Is dropdown open
    is_open: bool,
    /// Favorite timeframes
    pub favorites: HashSet<Timeframe>,
    /// Custom timeframes
    pub custom_timeframes: Vec<Timeframe>,
    /// Configuration
    pub config: TimeframeSelectorConfig,
    /// Show custom input dialog
    show_custom_input: bool,
    /// Custom timeframe dialog state
    custom_dialog: CustomTimeframeDialog,
}

impl Default for TimeframeSelector {
    fn default() -> Self {
        Self::new(Timeframe::new(1, TimeframeUnit::Day))
    }
}

impl TimeframeSelector {
    pub fn new(initial: Timeframe) -> Self {
        let mut favorites = HashSet::new();
        // Default favorites
        favorites.insert(Timeframe::new(1, TimeframeUnit::Minute));
        favorites.insert(Timeframe::new(5, TimeframeUnit::Minute));
        favorites.insert(Timeframe::new(15, TimeframeUnit::Minute));
        favorites.insert(Timeframe::new(1, TimeframeUnit::Hour));
        favorites.insert(Timeframe::new(4, TimeframeUnit::Hour));
        favorites.insert(Timeframe::new(1, TimeframeUnit::Day));

        Self {
            current: initial,
            is_open: false,
            favorites,
            custom_timeframes: Vec::new(),
            config: TimeframeSelectorConfig::default(),
            show_custom_input: false,
            custom_dialog: CustomTimeframeDialog::default(),
        }
    }

    pub fn with_config(mut self, config: TimeframeSelectorConfig) -> Self {
        self.config = config;
        self
    }

    /// Show the timeframe selector
    pub fn show(&mut self, ui: &mut Ui) -> TimeframeAction {
        // Main button
        let btn_res = self.trigger().show(ui, self.is_open);

        let theme = ui.active_theme();
        let id = ui.make_persistent_id("timeframe_menu");
        let max_height = menu_list_height(ui, btn_res.rect, ROW_H);
        let action = egui::Popup::menu(&btn_res)
            .id(id)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .align_alternatives(&[])
            .frame(egui::Frame::popup(ui.style()).fill(theme.ui.panel_bg))
            .gap(theme.spacing.xs)
            .show(|ui| self.menu_ui(ui, &theme, max_height))
            .map_or(TimeframeAction::None, |menu| menu.inner);
        self.is_open = egui::Popup::is_id_open(ui.ctx(), id);
        action
    }

    /// The button that opens the dropdown, showing the current timeframe.
    pub(crate) fn trigger(&self) -> Trigger<'static> {
        let label = self.current.display();
        Trigger {
            icon: None,
            name: self.current.display_long(),
            label,
            min_width: self.config.btn_width.max(44.0),
            filled: false,
        }
    }

    /// Widths of the widest interval code and the widest description in the menu.
    fn column_widths(&self, ui: &Ui, theme: &Theme) -> (f32, f32) {
        let font = egui::FontId::proportional(theme.text.body);
        let width = |text: String| {
            ui.painter()
                .layout_no_wrap(text, font.clone(), Color32::PLACEHOLDER)
                .size()
                .x
        };
        Timeframe::all_presets()
            .iter()
            .chain(&self.custom_timeframes)
            .chain(&self.favorites)
            .fold((0.0, 0.0), |(code, description), tf| {
                (
                    f32::max(code, width(tf.display())),
                    f32::max(description, width(tf.display_long())),
                )
            })
    }

    fn menu_ui(&mut self, ui: &mut Ui, theme: &Theme, max_height: f32) -> TimeframeAction {
        let mut action = TimeframeAction::None;
        // Rows sit edge to edge, and the scroll bar is visible whenever the list overflows.
        let spacing = ui.spacing_mut();
        spacing.item_spacing.y = 0.0;
        spacing.scroll.floating = false;
        spacing.scroll.bar_width = 3.0;
        spacing.scroll.fade.strength = 0.0;
        let bar = spacing.scroll.allocated_width();

        let (_, desc_w) = self.column_widths(ui, theme);
        ui.set_width(
            (desc_w + 64.0 + bar)
                .max(self.config.dropdown_width)
                .max(CUSTOM_FORM_W),
        );
        ui.visuals_mut().selection.bg_fill = theme.ui.text;
        ui.visuals_mut().selection.stroke.color = theme.ui.panel_bg;
        // A popup is first measured inside a 400 px high area; this lets the list use the room below.
        ui.set_max_height(max_height);

        let row = |ui: &mut Ui, tf: Timeframe, favorite: bool, current: Timeframe| {
            let content = TimeframeRow {
                tf,
                favorite,
                theme,
            };
            ListItem::new()
                .selected(current == tf)
                .height(ROW_H)
                .show(ui, content)
                .clicked()
        };

        let current = self.current;
        egui::ScrollArea::vertical()
            .max_height(max_height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for (unit, header) in [
                    (TimeframeUnit::Second, "Seconds"),
                    (TimeframeUnit::Minute, "Minutes"),
                    (TimeframeUnit::Hour, "Hours"),
                    (TimeframeUnit::Day, "Days"),
                    (TimeframeUnit::Week, "Weeks"),
                    (TimeframeUnit::Month, "Months"),
                ] {
                    section_header(ui, theme, header);
                    for tf in Timeframe::by_unit(unit) {
                        if row(ui, tf, self.favorites.contains(&tf), current) {
                            action = TimeframeAction::Select(tf);
                        }
                    }
                }
                if !self.custom_timeframes.is_empty() {
                    section_header(ui, theme, "Custom");
                    for tf in &self.custom_timeframes {
                        if row(ui, *tf, false, current) {
                            action = TimeframeAction::Select(*tf);
                        }
                    }
                }

                let custom = ListItem::new().height(ROW_H).show(
                    ui,
                    LabelContent::new("Custom interval\u{2026}").with_icon(&embedded_icons::PLUS),
                );
                if custom.clicked() {
                    self.show_custom_input = !self.show_custom_input;
                }
                if self.show_custom_input
                    && let Some(tf) = self.custom_dialog.show(ui)
                {
                    action = TimeframeAction::Select(tf);
                }
            });

        if let TimeframeAction::Select(timeframe) = action {
            self.current = timeframe;
            self.show_custom_input = false;
            ui.close();
        }
        action
    }
}

/// Height of a menu list that ends on a whole row above the window's bottom edge, so the last
/// row is never cut. At least three rows.
pub(super) fn menu_list_height(ui: &Ui, trigger: Rect, row_h: f32) -> f32 {
    let theme = ui.active_theme();
    let available = ui.ctx().content_rect().bottom()
        - trigger.bottom()
        - theme.spacing.xs
        - theme.spacing.lg
        - ui.spacing().menu_margin.sum().y;
    (available / row_h).floor().max(3.0) * row_h
}

/// Draws the focus ring inside a menu row, which the scroll area would clip outside it, and keeps
/// a row focused with the arrow keys in view.
pub(super) fn menu_row_focus(ui: &Ui, response: &egui::Response) {
    if response.has_focus() {
        ui.painter().rect_stroke(
            response.rect,
            ui.visuals().menu_corner_radius,
            egui::Stroke::new(DESIGN_TOKENS.stroke.thick, focus_ring_color(ui)),
            egui::StrokeKind::Inside,
        );
    }
    if response.gained_focus() {
        response.scroll_to_me(None);
    }
}

/// A section title on the menu's row grid, with its text on the bottom of the row.
fn section_header(ui: &mut Ui, theme: &Theme, text: &str) {
    if ui.cursor().top() > ui.max_rect().top() + 1.0 {
        ui.add_space(12.0);
        ui.separator();
    }
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), egui::Sense::hover());
    // A child Ui, so that the row keeps its full height in the parent's layout.
    let mut title = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(rect.shrink2(vec2(theme.spacing.lg, theme.spacing.sm)))
            .layout(egui::Layout::bottom_up(egui::Align::Min)),
    );
    title.add(
        egui::Label::new(
            RichText::new(text.to_uppercase())
                .size(theme.text.small)
                .color(theme.ui.text_secondary),
        )
        .selectable(false),
    );
}

/// One interval row with a trailing favourite star.
struct TimeframeRow<'a> {
    tf: Timeframe,
    favorite: bool,
    theme: &'a Theme,
}

impl ListItemContent for TimeframeRow<'_> {
    fn ui(self, ui: &mut Ui, context: &ContentContext<'_>) -> egui::Response {
        let theme = self.theme;
        let rect = context.rect;
        let text = if context.selected {
            theme.ui.panel_bg
        } else {
            theme.ui.text
        };
        context.response.widget_info(|| {
            WidgetInfo::selected(
                Role::Button,
                ui.is_enabled(),
                context.selected,
                self.tf.display_long(),
            )
        });

        if self.favorite {
            let star_rect = Rect::from_min_size(
                pos2(
                    rect.max.x - icon_sizes::SM,
                    rect.center().y - icon_sizes::SM / 2.0,
                ),
                vec2(icon_sizes::SM, icon_sizes::SM),
            );
            embedded_icons::STAR_FILLED
                .as_image_tinted(star_rect.size(), text)
                .paint_at(ui, star_rect);
        }
        ui.painter().text(
            pos2(rect.min.x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            self.tf.display_long(),
            egui::FontId::proportional(14.0),
            text,
        );
        menu_row_focus(ui, context.response);
        context.response.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Timeframe as Model;

    #[test]
    fn interval_pick_updates_the_trigger_and_selection() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for selected in [
                Timeframe::new(5, TimeframeUnit::Minute),
                Timeframe::new(1, TimeframeUnit::Hour),
            ] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut selector = TimeframeSelector::new(Timeframe::new(1, TimeframeUnit::Minute));
                let mut frame = |events| {
                    let mut action = TimeframeAction::None;
                    let mut output = ctx.run_ui(
                        egui::RawInput {
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            action = selector.menu_ui(ui, &theme, 800.0);
                        },
                    );
                    let pos = output
                        .shapes
                        .iter()
                        .find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text)
                                if text.galley.text() == selected.display_long() =>
                            {
                                Some(text.pos + text.galley.size() / 2.0)
                            }
                            _ => None,
                        })
                        .expect("visible favourite interval");
                    output.textures_delta.clear();
                    output.drop_without_applying_deltas();
                    (pos, action)
                };
                let (pos, _) = frame(vec![]);
                frame(vec![egui::Event::PointerMoved(pos)]);
                let click = |pressed| egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                };
                frame(vec![click(true)]);
                let (_, action) = frame(vec![click(false)]);
                assert!(
                    matches!(action, TimeframeAction::Select(timeframe) if timeframe == selected)
                );
                assert_eq!(selector.current, selected);
            }
        }
    }

    #[test]
    fn every_preset_maps_to_a_model_timeframe() {
        let custom = |s| Model::Custom(std::num::NonZeroU64::new(s).unwrap());
        let expected = vec![
            Model::Sec1,
            Model::Sec5,
            Model::Sec10,
            custom(15),
            Model::Sec30,
            custom(45),
            Model::Min1,
            custom(120),
            custom(180),
            Model::Min5,
            custom(600),
            Model::Min15,
            Model::Min30,
            custom(2700),
            Model::Hour1,
            custom(7200),
            custom(10800),
            Model::Hour4,
            Model::Day1,
            Model::Week1,
            Model::Month1,
        ];
        let mapped: Vec<_> = Timeframe::all_presets()
            .iter()
            .map(|tf| tf.to_model())
            .collect();
        assert_eq!(mapped, expected.into_iter().map(Some).collect::<Vec<_>>());
    }
}
