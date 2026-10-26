//! Timeframe Selector
//!
//! A dropdown menu for selecting chart timeframes with favorites support.
use crate::icons::icons as embedded_icons;
use crate::localization as l10n;
use crate::model::Timeframe;
use crate::styles::focus::focus_ring_color;
use crate::styles::icons as icon_sizes;
use crate::theme::{Theme, ThemeExt};
use crate::tokens::DESIGN_TOKENS;
use crate::ui::top_toolbar::buttons::Trigger;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::list_item::{ContentContext, LabelContent, ListItem, ListItemContent};
use egui::{Color32, Rect, RichText, Role, Ui, WidgetInfo, pos2, vec2};
use std::collections::HashSet;
use std::num::{NonZeroU32, NonZeroU64};

/// Height of the interval menu's rows and section headers.
const ROW_H: f32 = 32.0;

/// Width reserved while the custom interval form is expanded.
const CUSTOM_FORM_W: f32 = 232.0;

/// Timeframe unit
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
pub enum TimeframeUnit {
    /// A fixed number of executions per bar.
    Tick,
    /// Traded volume in whole units, split exactly at the threshold.
    Volume,
    /// Price range in instrument ticks.
    Range,
    /// Absolute buy minus sell volume in whole units.
    Delta,
    /// One of the model's subsecond presets.
    Millisecond,
    /// Seconds.
    Second,
    /// Minutes.
    Minute,
    /// Hours.
    Hour,
    /// Days.
    Day,
    /// Weeks.
    Week,
    /// Months.
    Month,
}

impl TimeframeUnit {
    /// Abbreviated unit label.
    pub fn short_name(&self) -> &'static str {
        match self {
            TimeframeUnit::Tick => "T",
            TimeframeUnit::Volume => "V",
            TimeframeUnit::Range => "R",
            TimeframeUnit::Delta => "delta",
            TimeframeUnit::Millisecond => "ms",
            TimeframeUnit::Second => "S",
            TimeframeUnit::Minute => "m",
            TimeframeUnit::Hour => "h",
            TimeframeUnit::Day => "D",
            TimeframeUnit::Week => "W",
            TimeframeUnit::Month => "M",
        }
    }

    /// Display name for this value.
    pub fn name(&self) -> &'static str {
        match self {
            TimeframeUnit::Tick => "ticks",
            TimeframeUnit::Volume => "volume units",
            TimeframeUnit::Range => "range ticks",
            TimeframeUnit::Delta => "delta units",
            TimeframeUnit::Millisecond => "milliseconds",
            TimeframeUnit::Second => "seconds",
            TimeframeUnit::Minute => "minutes",
            TimeframeUnit::Hour => "hours",
            TimeframeUnit::Day => "days",
            TimeframeUnit::Week => "weeks",
            TimeframeUnit::Month => "months",
        }
    }
}

fn interval(value: u32, unit: TimeframeUnit) -> Timeframe {
    let count = std::num::NonZeroU64::new(u64::from(value)).expect("validated interval");
    match unit {
        TimeframeUnit::Tick => Timeframe::Ticks(count),
        TimeframeUnit::Volume => Timeframe::Volume(count),
        TimeframeUnit::Range => Timeframe::Range(count),
        TimeframeUnit::Delta => Timeframe::Delta(count),
        _ => format!(
            "{value}{}",
            match unit {
                TimeframeUnit::Minute => "min",
                TimeframeUnit::Month => "month",
                _ => unit.short_name(),
            }
        )
        .parse()
        .expect("valid interval unit"),
    }
}

fn parts(tf: Timeframe) -> (u64, TimeframeUnit) {
    use TimeframeUnit as Unit;
    let seconds = match tf {
        Timeframe::Custom(seconds) => seconds.get(),
        _ => tf.duration_ms() as u64 / 1000,
    };
    match tf {
        Timeframe::Ms100 => (100, Unit::Millisecond),
        Timeframe::Ms250 => (250, Unit::Millisecond),
        Timeframe::Ms500 => (500, Unit::Millisecond),
        Timeframe::Ticks(n) => (n.get(), Unit::Tick),
        Timeframe::Volume(n) => (n.get(), Unit::Volume),
        Timeframe::Range(n) => (n.get(), Unit::Range),
        Timeframe::Delta(n) => (n.get(), Unit::Delta),
        Timeframe::Months(n) => (u64::from(n.get()), Unit::Month),
        Timeframe::Weeks(n) => (u64::from(n.get()), Unit::Week),
        Timeframe::Month1 => (1, Unit::Month),
        Timeframe::Week1 => (1, Unit::Week),
        _ if seconds >= 86_400 && seconds % 86_400 == 0 => (seconds / 86_400, Unit::Day),
        _ if seconds >= 3_600 && seconds % 3_600 == 0 => (seconds / 3_600, Unit::Hour),
        _ if seconds >= 60 && seconds % 60 == 0 => (seconds / 60, Unit::Minute),
        _ => (seconds, Unit::Second),
    }
}

fn display(tf: Timeframe) -> String {
    let (value, unit) = parts(tf);
    format!("{value}{}", unit.short_name())
}

fn display_long(tf: Timeframe) -> String {
    let (value, unit) = parts(tf);
    let name = if value == 1 {
        match unit {
            TimeframeUnit::Tick => "tick",
            TimeframeUnit::Volume => "volume unit",
            TimeframeUnit::Range => "range tick",
            TimeframeUnit::Delta => "delta unit",
            TimeframeUnit::Millisecond => "millisecond",
            TimeframeUnit::Second => "second",
            TimeframeUnit::Minute => "minute",
            TimeframeUnit::Hour => "hour",
            TimeframeUnit::Day => "day",
            TimeframeUnit::Week => "week",
            TimeframeUnit::Month => "month",
        }
    } else {
        unit.name()
    };
    format!("{value} {name}")
}

const PRESETS: &[Timeframe] = &[
    // Ticks
    Timeframe::Ticks(NonZeroU64::new(1).unwrap()),
    Timeframe::Ticks(NonZeroU64::new(10).unwrap()),
    Timeframe::Ticks(NonZeroU64::new(100).unwrap()),
    Timeframe::Ticks(NonZeroU64::new(1000).unwrap()),
    Timeframe::Volume(NonZeroU64::new(100).unwrap()),
    Timeframe::Volume(NonZeroU64::new(1000).unwrap()),
    Timeframe::Range(NonZeroU64::new(10).unwrap()),
    Timeframe::Delta(NonZeroU64::new(100).unwrap()),
    // Milliseconds
    Timeframe::Ms100,
    Timeframe::Ms250,
    Timeframe::Ms500,
    // Seconds
    Timeframe::Sec1,
    Timeframe::Sec2,
    Timeframe::Sec5,
    Timeframe::Sec10,
    Timeframe::Custom(NonZeroU64::new(15).unwrap()),
    Timeframe::Sec30,
    Timeframe::Custom(NonZeroU64::new(45).unwrap()),
    // Minutes
    Timeframe::Min1,
    Timeframe::Custom(NonZeroU64::new(120).unwrap()),
    Timeframe::Custom(NonZeroU64::new(180).unwrap()),
    Timeframe::Min5,
    Timeframe::Custom(NonZeroU64::new(600).unwrap()),
    Timeframe::Min15,
    Timeframe::Min30,
    Timeframe::Custom(NonZeroU64::new(2700).unwrap()),
    // Hours
    Timeframe::Hour1,
    Timeframe::Custom(NonZeroU64::new(7200).unwrap()),
    Timeframe::Custom(NonZeroU64::new(10800).unwrap()),
    Timeframe::Hour4,
    // Day/Week/Month
    Timeframe::Day1,
    Timeframe::Week1,
    Timeframe::Month1,
    Timeframe::Months(NonZeroU32::new(3).unwrap()),
    Timeframe::Months(NonZeroU32::new(6).unwrap()),
    Timeframe::Months(NonZeroU32::new(12).unwrap()),
];

/// Configuration for timeframe selector
#[derive(Debug, Clone)]
pub struct TimeframeSelectorConfig {
    /// Minimum width of the button; it is otherwise sized to its label.
    pub btn_width: f32,
    /// Minimum menu content width; the expanded custom form reserves at least 232 px.
    pub dropdown_width: f32,
}

impl Default for TimeframeSelectorConfig {
    fn default() -> Self {
        Self {
            btn_width: 0.0,
            dropdown_width: 184.0,
        }
    }
}

/// Action from timeframe selector
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum TimeframeAction {
    /// No action requested.
    None,
    /// Select the supplied value.
    Select(Timeframe),
    /// Toggle the supplied interval's favorite state.
    ToggleFavorite(Timeframe),
    /// Hide an interval entry without changing the current interval.
    Hide(Timeframe),
    /// Restore all hidden interval entries.
    RestoreHidden,
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
            Ok(v) if self.unit == TimeframeUnit::Millisecond && ![100, 250, 500].contains(&v) => {
                self.error = Some("Use 100, 250 or 500 milliseconds".to_owned());
                None
            }
            Ok(v) => {
                self.error = None;
                Some(interval(v, self.unit))
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
            (TimeframeUnit::Tick, "Ticks"),
            (TimeframeUnit::Volume, "Volume"),
            (TimeframeUnit::Range, "Range"),
            (TimeframeUnit::Delta, "Delta"),
            (TimeframeUnit::Second, "Seconds"),
            (TimeframeUnit::Minute, "Minutes"),
            (TimeframeUnit::Hour, "Hours"),
            (TimeframeUnit::Day, "Days"),
            (TimeframeUnit::Week, "Weeks"),
            (TimeframeUnit::Month, "Months"),
        ]
    }

    /// Show the custom timeframe dialog inside the dropdown.
    /// Returns Some(Timeframe) if the user confirms, None otherwise.
    pub fn show(&mut self, ui: &mut Ui) -> Option<Timeframe> {
        let theme = ui.active_theme();
        let mut result = None;

        ui.add_space(theme.spacing.lg);
        ui.allocate_ui_with_layout(
            vec2(ui.available_width(), DESIGN_TOKENS.sizing.button_md),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.add_space(theme.spacing.lg);
                // The controls share the height of the button.
                ui.spacing_mut().interact_size.y = DESIGN_TOKENS.sizing.button_md;
                // The menu style draws inactive controls without a frame; the form's fields need one.
                let inactive = &mut ui.visuals_mut().widgets.inactive;
                inactive.bg_stroke = egui::Stroke::new(1.0, theme.ui.border_control);
                inactive.weak_bg_fill = theme.ui.btn_bg_hover;
                let width_id = ui.scope_id().with("interval_actions_width");
                let actions_width = ui.data(|d| d.get_temp::<f32>(width_id)).unwrap_or(0.0);
                let field_width =
                    (ui.available_width() - actions_width - 2.0 * theme.spacing.lg).max(0.0);
                ui.add_sized(
                    vec2(field_width, DESIGN_TOKENS.sizing.button_md),
                    egui::TextEdit::singleline(&mut self.value_input)
                        .desired_width(0.0)
                        .align(egui::Align2::LEFT_CENTER)
                        .font(egui::FontId::proportional(theme.text.body)),
                )
                .accessible_name(l10n::text(ui.ctx(), "Interval value"));
                let width = ui
                    .horizontal(|ui| {
                        let unit = Self::available_units()
                            .iter()
                            .find(|(u, _)| *u == self.unit)
                            .map_or_else(|| self.unit.name(), |(_, label)| label);
                        // A submenu keeps the interval menu open when a unit is picked.
                        ui.menu_button(l10n::text(ui.ctx(), unit), |ui| {
                            for (u, label) in Self::available_units() {
                                if ui
                                    .selectable_label(self.unit == *u, l10n::text(ui.ctx(), *label))
                                    .clicked()
                                {
                                    self.unit = *u;
                                }
                            }
                        });
                        if Button::new("Add").primary().show(ui).clicked() {
                            result = self.validate();
                        }
                    })
                    .response
                    .rect
                    .width();
                if (width - actions_width).abs() > 0.5 {
                    ui.data_mut(|d| d.insert_temp(width_id, width));
                    ui.ctx().request_discard("Interval actions changed width");
                }
            },
        );

        if let Some(err) = &self.error {
            ui.horizontal(|ui| {
                ui.add_space(theme.spacing.lg);
                ui.label(
                    RichText::new(l10n::text(ui.ctx(), err))
                        .size(theme.text.small)
                        .color(ui.visuals().error_fg_color),
                );
            });
        }

        result
    }
}

/// Host-owned interval settings. Save with the user's settings and restore with
/// [`TimeframeSelector::set_preferences`]. The selector owns no storage.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct TimeframePreferences {
    /// Pinned intervals, including those temporarily unavailable from a source.
    #[serde(default)]
    pub favorites: Vec<Timeframe>,
    /// User-created intervals in menu order.
    #[serde(default)]
    pub custom_timeframes: Vec<Timeframe>,
    /// Hidden entries, including those temporarily unavailable from a source.
    #[serde(default)]
    pub hidden_timeframes: Vec<Timeframe>,
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
    supported_timeframes: Option<HashSet<crate::model::Timeframe>>,
    hidden_timeframes: HashSet<Timeframe>,
}

impl Default for TimeframeSelector {
    fn default() -> Self {
        Self::new(Timeframe::Day1)
    }
}

impl TimeframeSelector {
    /// Create an interval selector with `initial` selected and default favorites.
    pub fn new(initial: Timeframe) -> Self {
        let mut favorites = HashSet::new();
        // Default favorites
        favorites.insert(Timeframe::Min1);
        favorites.insert(Timeframe::Min5);
        favorites.insert(Timeframe::Min15);
        favorites.insert(Timeframe::Hour1);
        favorites.insert(Timeframe::Hour4);
        favorites.insert(Timeframe::Day1);

        Self {
            current: initial,
            is_open: false,
            favorites,
            custom_timeframes: Vec::new(),
            config: TimeframeSelectorConfig::default(),
            show_custom_input: false,
            custom_dialog: CustomTimeframeDialog::default(),
            supported_timeframes: None,
            hidden_timeframes: HashSet::new(),
        }
    }

    /// Restrict interval choices to those the connected source can serve, such as the chart
    /// instrument's [`supported_timeframes`](crate::model::Instrument::supported_timeframes).
    /// `None` allows every interval; an empty list allows none. Update this when the source or
    /// symbol changes. Saved favourites and custom intervals are retained while unavailable.
    pub fn set_supported_timeframes(&mut self, timeframes: Option<Vec<crate::model::Timeframe>>) {
        self.supported_timeframes = timeframes.map(|values| values.into_iter().collect());
    }

    fn supports(&self, timeframe: Timeframe) -> bool {
        self.supported_timeframes
            .as_ref()
            .is_none_or(|supported| supported.contains(&timeframe))
    }

    /// Export the interval lists for the host's persisted user settings.
    pub fn preferences(&self) -> TimeframePreferences {
        let mut favorites: Vec<_> = self.favorites.iter().copied().collect();
        favorites.sort_by_key(|tf| (parts(*tf).1 as u8, parts(*tf).0));
        let mut hidden_timeframes: Vec<_> = self.hidden_timeframes.iter().copied().collect();
        hidden_timeframes.sort_by_key(|tf| (parts(*tf).1 as u8, parts(*tf).0));
        TimeframePreferences {
            favorites,
            hidden_timeframes,
            custom_timeframes: self.custom_timeframes.clone(),
        }
    }

    /// Restore saved lists, discarding duplicate custom entries.
    pub fn set_preferences(&mut self, preferences: TimeframePreferences) {
        self.hidden_timeframes = preferences.hidden_timeframes.into_iter().collect();
        self.favorites = preferences
            .favorites
            .into_iter()
            .filter(|tf| !self.hidden_timeframes.contains(tf))
            .collect();
        self.custom_timeframes.clear();
        for tf in preferences.custom_timeframes {
            if !self.custom_timeframes.contains(&tf) && !PRESETS.contains(&tf) {
                self.custom_timeframes.push(tf);
            }
        }
    }

    /// Use the supplied display and layout options.
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
        let label = display(self.current);
        Trigger {
            icon: None,
            name: display_long(self.current),
            label,
            min_width: self.config.btn_width.max(44.0),
            filled: false,
        }
    }

    /// Width of the widest interval label in the menu.
    fn label_width(&self, ui: &Ui) -> f32 {
        let font = egui::FontId::proportional(14.0);
        let width = |text: String| {
            ui.painter()
                .layout_no_wrap(
                    l10n::text(ui.ctx(), text),
                    font.clone(),
                    Color32::PLACEHOLDER,
                )
                .size()
                .x
        };
        PRESETS
            .iter()
            .chain(&self.custom_timeframes)
            .filter(|tf| self.supports(**tf) && !self.hidden_timeframes.contains(tf))
            .map(|tf| width(display_long(*tf)))
            .fold(0.0, f32::max)
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

        let desc_w = self.label_width(ui);
        ui.set_width((desc_w + 92.0 + bar).max(self.config.dropdown_width).max(
            if self.show_custom_input {
                CUSTOM_FORM_W
            } else {
                0.0
            },
        ));
        ui.visuals_mut().selection.bg_fill = theme.ui.text;
        ui.visuals_mut().selection.stroke.color = theme.ui.panel_bg;
        // A popup is first measured inside a 400 px high area; this lets the list use the room below.
        ui.set_max_height(max_height);

        let row = |ui: &mut Ui, tf: Timeframe, favorite: bool, current: Timeframe| {
            let mut toggle_favorite = false;
            let mut hide = false;
            let content = TimeframeRow {
                tf,
                favorite,
                theme,
                toggle_favorite: &mut toggle_favorite,
                hide: &mut hide,
            };
            let response = ListItem::new()
                .selected(current == tf)
                .height(ROW_H)
                .show(ui, content);
            if hide {
                TimeframeAction::Hide(tf)
            } else if toggle_favorite {
                TimeframeAction::ToggleFavorite(tf)
            } else if response.clicked() {
                TimeframeAction::Select(tf)
            } else {
                TimeframeAction::None
            }
        };

        let current = self.current;
        egui::ScrollArea::vertical()
            .max_height(max_height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for (unit, header) in [
                    (TimeframeUnit::Tick, "Ticks"),
                    (TimeframeUnit::Volume, "Volume"),
                    (TimeframeUnit::Range, "Range"),
                    (TimeframeUnit::Delta, "Delta"),
                    (TimeframeUnit::Millisecond, "Milliseconds"),
                    (TimeframeUnit::Second, "Seconds"),
                    (TimeframeUnit::Minute, "Minutes"),
                    (TimeframeUnit::Hour, "Hours"),
                    (TimeframeUnit::Day, "Days"),
                    (TimeframeUnit::Week, "Weeks"),
                    (TimeframeUnit::Month, "Months"),
                ] {
                    let available: Vec<_> = PRESETS
                        .iter()
                        .copied()
                        .filter(|tf| {
                            parts(*tf).1 == unit
                                && self.supports(*tf)
                                && !self.hidden_timeframes.contains(tf)
                        })
                        .collect();
                    if available.is_empty() {
                        continue;
                    }
                    section_header(ui, theme, header);
                    for tf in available {
                        let picked = row(ui, tf, self.favorites.contains(&tf), current);
                        if picked != TimeframeAction::None {
                            action = picked;
                        }
                    }
                }
                let custom: Vec<_> = self
                    .custom_timeframes
                    .iter()
                    .copied()
                    .filter(|tf| self.supports(*tf) && !self.hidden_timeframes.contains(tf))
                    .collect();
                if !custom.is_empty() {
                    section_header(ui, theme, "Custom");
                    for tf in &custom {
                        let picked = row(ui, *tf, self.favorites.contains(tf), current);
                        if picked != TimeframeAction::None {
                            action = picked;
                        }
                    }
                }

                if !self.hidden_timeframes.is_empty()
                    && ListItem::new()
                        .height(ROW_H)
                        .show(ui, LabelContent::new("Restore hidden intervals"))
                        .clicked()
                {
                    action = TimeframeAction::RestoreHidden;
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
                    if self.supports(tf) {
                        self.hidden_timeframes.remove(&tf);
                        if !PRESETS.contains(&tf) && !self.custom_timeframes.contains(&tf) {
                            self.custom_timeframes.push(tf);
                        }
                        action = TimeframeAction::Select(tf);
                    } else {
                        self.custom_dialog.error =
                            Some("Interval unavailable from this source".to_owned());
                    }
                }
            });

        match action {
            TimeframeAction::Hide(tf) => {
                self.hidden_timeframes.insert(tf);
                self.favorites.remove(&tf);
            }
            TimeframeAction::RestoreHidden => self.hidden_timeframes.clear(),
            _ => {}
        }
        if let TimeframeAction::ToggleFavorite(tf) = action
            && !self.favorites.remove(&tf)
        {
            self.favorites.insert(tf);
        }
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
            RichText::new(l10n::text(ui.ctx(), text.to_uppercase()))
                .size(theme.text.small)
                .color(theme.ui.text_secondary),
        )
        .selectable(false),
    );
}

/// One interval row with trailing hide and favourite buttons.
struct TimeframeRow<'a> {
    tf: Timeframe,
    favorite: bool,
    theme: &'a Theme,
    toggle_favorite: &'a mut bool,
    hide: &'a mut bool,
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
        let translated_label = l10n::text(ui.ctx(), display_long(self.tf));
        context.response.widget_info(|| {
            WidgetInfo::selected(
                Role::MenuItemRadio,
                context.response.enabled(),
                context.selected,
                &translated_label,
            )
        });

        let hide_rect = Rect::from_center_size(
            pos2(rect.right() - icon_sizes::SM / 2.0 - 28.0, rect.center().y),
            vec2(28.0, ROW_H),
        );
        let hide = ui.interact(
            hide_rect,
            context.response.id.with("hide"),
            egui::Sense::click(),
        );
        let label = l10n::text(ui.ctx(), "Hide interval");
        hide.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), &label));
        *self.hide = hide.clicked();
        embedded_icons::CLOSE
            .as_image_tinted(vec2(icon_sizes::SM, icon_sizes::SM), text)
            .paint_at(
                ui,
                Rect::from_center_size(hide_rect.center(), vec2(icon_sizes::SM, icon_sizes::SM)),
            );
        menu_row_focus(ui, &hide);
        hide.on_hover_text(label);

        let star_rect = Rect::from_center_size(
            pos2(rect.right() - icon_sizes::SM / 2.0, rect.center().y),
            vec2(28.0, ROW_H),
        );
        let star = ui.interact(
            star_rect,
            context.response.id.with("favorite"),
            egui::Sense::click(),
        );
        let tooltip = if self.favorite {
            "Remove from favorites"
        } else {
            "Add to favorites"
        };
        let translated_label = l10n::text(ui.ctx(), tooltip);
        star.widget_info(|| {
            WidgetInfo::selected(
                Role::Button,
                ui.is_enabled(),
                self.favorite,
                &translated_label,
            )
        });
        *self.toggle_favorite = star.clicked();
        let icon = if self.favorite {
            &embedded_icons::STAR_FILLED
        } else {
            &embedded_icons::STAR_EMPTY
        };
        icon.as_image_tinted(vec2(icon_sizes::SM, icon_sizes::SM), text)
            .paint_at(
                ui,
                Rect::from_center_size(star_rect.center(), vec2(icon_sizes::SM, icon_sizes::SM)),
            );
        menu_row_focus(ui, &star);
        star.on_hover_text(l10n::text(ui.ctx(), tooltip));
        ui.painter().text(
            pos2(rect.min.x, rect.center().y),
            egui::Align2::LEFT_CENTER,
            l10n::text(ui.ctx(), display_long(self.tf)),
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
    fn custom_intervals_and_stars_survive_saved_preferences() {
        let ctx = egui::Context::default();
        let theme = Theme::light();
        crate::theme::apply_to_egui(&ctx, &theme);
        let custom = interval(7, TimeframeUnit::Minute);
        let mut selector = TimeframeSelector {
            show_custom_input: true,
            custom_dialog: CustomTimeframeDialog {
                value_input: "7".to_owned(),
                ..Default::default()
            },
            ..Default::default()
        };
        let frame = |selector: &mut TimeframeSelector, events| {
            let mut action = TimeframeAction::None;
            let mut right = 0.0;
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(egui::Pos2::ZERO, vec2(400.0, 2600.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    action = selector.menu_ui(ui, &theme, 2500.0);
                    right = ui.min_rect().right();
                },
            );
            let labels: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some((
                        text.galley.text().to_owned(),
                        text.pos + text.galley.size() / 2.0,
                    )),
                    _ => None,
                })
                .collect();
            output.textures_delta.clear();
            output.drop_without_applying_deltas();
            (action, labels, right)
        };
        let click = |selector: &mut TimeframeSelector, pos| {
            frame(selector, vec![egui::Event::PointerMoved(pos)]);
            let event = |pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            frame(selector, vec![event(true)]);
            frame(selector, vec![event(false)]).0
        };
        frame(&mut selector, vec![]);
        let (_, labels, _) = frame(&mut selector, vec![]);
        let add = labels.iter().find(|(label, _)| label == "Add").unwrap().1;
        assert_eq!(click(&mut selector, add), TimeframeAction::Select(custom));
        assert_eq!(selector.custom_timeframes, vec![custom]);
        // Adding the same custom interval again must not duplicate its row.
        selector.show_custom_input = true;
        frame(&mut selector, vec![]);
        let (_, labels, _) = frame(&mut selector, vec![]);
        let add = labels.iter().find(|(label, _)| label == "Add").unwrap().1;
        click(&mut selector, add);
        assert_eq!(selector.custom_timeframes, vec![custom]);
        let selected = selector.current;
        for favorite in [true, false, true] {
            let (_, labels, right) = frame(&mut selector, vec![]);
            let y = labels
                .iter()
                .find(|(label, _)| *label == display_long(custom))
                .unwrap()
                .1
                .y;
            let action = click(
                &mut selector,
                pos2(right - theme.spacing.lg - icon_sizes::SM / 2.0, y),
            );
            assert_eq!(action, TimeframeAction::ToggleFavorite(custom));
            assert_eq!(selector.favorites.contains(&custom), favorite);
            assert_eq!(selector.current, selected);
        }
        // Hiding a favorite unpins it, retains the selected interval and persists with host settings.
        for tf in [Timeframe::Min1, custom] {
            let (_, labels, right) = frame(&mut selector, vec![]);
            let y = labels
                .iter()
                .find(|(label, _)| *label == display_long(tf))
                .unwrap()
                .1
                .y;
            assert_eq!(
                click(
                    &mut selector,
                    pos2(right - theme.spacing.lg - icon_sizes::SM / 2.0 - 28.0, y)
                ),
                TimeframeAction::Hide(tf)
            );
            let (_, labels, _) = frame(&mut selector, vec![]);
            assert!(!labels.iter().any(|(label, _)| *label == display_long(tf)));
            assert_eq!(selector.current, selected);
            assert!(!selector.favorites.contains(&tf));
        }
        let saved = serde_json::to_string(&selector.preferences()).unwrap();
        let mut restored = TimeframeSelector::default();
        restored.set_preferences(serde_json::from_str(&saved).unwrap());
        assert_eq!(restored.preferences(), selector.preferences());
        restored.set_supported_timeframes(Some(vec![]));
        frame(&mut restored, vec![]);
        assert_eq!(restored.preferences(), selector.preferences());
        restored.set_supported_timeframes(None);
        let legacy: TimeframePreferences =
            serde_json::from_str(r#"{"favorites":[],"custom_timeframes":[]}"#).unwrap();
        assert!(legacy.hidden_timeframes.is_empty());
        let (_, labels, _) = frame(&mut restored, vec![]);
        assert!(!labels.iter().any(|(label, _)| label == "1 minute"));
        let restore = labels
            .iter()
            .find(|(label, _)| label == "Restore hidden intervals")
            .unwrap()
            .1;
        click(&mut restored, restore);
        let (_, labels, _) = frame(&mut restored, vec![]);
        assert!(labels.iter().any(|(label, _)| label == "1 minute"));
        assert!(
            labels
                .iter()
                .any(|(label, _)| *label == display_long(custom))
        );
        assert!(!restored.favorites.contains(&Timeframe::Min1));
        let saved = restored.preferences();
        restored.set_supported_timeframes(Some(vec![Model::Min1]));
        restored.show_custom_input = true;
        restored.custom_dialog.value_input = "8".to_owned();
        let (_, labels, _) = frame(&mut restored, vec![]);
        assert!(
            !labels
                .iter()
                .any(|(label, _)| *label == display_long(custom))
        );
        let add = labels.iter().find(|(label, _)| label == "Add").unwrap().1;
        assert_eq!(click(&mut restored, add), TimeframeAction::None);
        assert!(restored.custom_dialog.error.is_some());
        assert_eq!(restored.preferences(), saved);
        restored.custom_dialog.unit = TimeframeUnit::Millisecond;
        restored.custom_dialog.value_input = "1".to_owned();
        assert_eq!(click(&mut restored, add), TimeframeAction::None);
        assert!(restored.custom_dialog.error.is_some());
        assert_eq!(restored.preferences(), saved);
        restored.set_supported_timeframes(None);
        let (_, labels, _) = frame(&mut restored, vec![]);
        assert!(
            labels
                .iter()
                .any(|(label, _)| *label == display_long(custom))
        );
    }

    #[test]
    fn custom_interval_tabs_from_value_to_unit_to_add() {
        for width in [320.0, 500.0] {
            for unit in [TimeframeUnit::Week, TimeframeUnit::Month] {
                assert!(
                    CustomTimeframeDialog::available_units()
                        .iter()
                        .any(|(u, _)| *u == unit)
                );
                let mut dialog = CustomTimeframeDialog {
                    value_input: "3".into(),
                    unit,
                    error: None,
                };
                assert_eq!(parts(dialog.validate().unwrap()), (3, unit));
                dialog.value_input = "0".into();
                assert!(dialog.validate().is_none());
            }
            let ctx = egui::Context::default();
            let mut dialog = CustomTimeframeDialog::default();
            let mut frame = |events| {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            vec2(width, 200.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        dialog.show(ui);
                    },
                )
                .drop_without_applying_deltas();
                ctx.memory(|m| m.focused())
                    .and_then(|id| ctx.read_response(id))
                    .map(|r| r.rect)
            };
            frame(vec![]);
            let mut right = 0.0;
            for _ in 0..3 {
                let rect = frame(vec![egui::Event::Key {
                    key: egui::Key::Tab,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::NONE,
                }])
                .unwrap();
                assert!(rect.left() >= right, "{rect:?} follows {right}");
                assert!(rect.right() <= width, "{rect:?}");
                right = rect.right();
            }
        }
    }

    #[test]
    fn interval_pick_updates_the_trigger_and_selection() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for (selected, short, long) in [
                (
                    Model::Ticks(std::num::NonZeroU64::new(100).unwrap()),
                    "100T",
                    "100 ticks",
                ),
                (Model::Ms100, "100ms", "100 milliseconds"),
                (Model::Ms250, "250ms", "250 milliseconds"),
                (Model::Ms500, "500ms", "500 milliseconds"),
                (Model::Sec2, "2S", "2 seconds"),
                ("45s".parse().unwrap(), "45S", "45 seconds"),
                (Model::Min5, "5m", "5 minutes"),
                (Model::Hour1, "1h", "1 hour"),
                ("3month".parse().unwrap(), "3M", "3 months"),
                ("6month".parse().unwrap(), "6M", "6 months"),
                ("12month".parse().unwrap(), "12M", "12 months"),
                (
                    Model::Custom(std::num::NonZeroU64::MAX),
                    "18446744073709551615S",
                    "18446744073709551615 seconds",
                ),
            ] {
                assert_eq!(display(selected), short);
                assert_eq!(display_long(selected), long);
                for supported in [None, Some(vec![selected]), Some(vec![])] {
                    let ctx = egui::Context::default();
                    crate::theme::apply_to_egui(&ctx, &theme);
                    let mut selector = TimeframeSelector::new(Model::Min1);
                    if matches!(selected, Model::Custom(_)) && !PRESETS.contains(&selected) {
                        selector.custom_timeframes.push(selected);
                    }
                    let available = supported.as_ref().is_none_or(|values| !values.is_empty());
                    selector.set_supported_timeframes(supported.clone());
                    let mut frame = |events| {
                        let mut action = TimeframeAction::None;
                        let mut output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    vec2(320.0, 2400.0),
                                )),
                                events,
                                ..Default::default()
                            },
                            |ui| {
                                action = selector.menu_ui(ui, &theme, 2400.0);
                                assert!(
                                    ui.min_rect().right() <= 320.0,
                                    "{selected:?}: {:?}",
                                    ui.min_rect()
                                );
                            },
                        );
                        output.textures_delta.clear();
                        if supported.is_some() {
                            let labels: Vec<_> = output
                                .shapes
                                .iter()
                                .filter_map(|shape| match &shape.shape {
                                    egui::Shape::Text(text) => Some(text.galley.text()),
                                    _ => None,
                                })
                                .collect();
                            for &tf in PRESETS {
                                assert_eq!(
                                    labels.contains(&display_long(tf).as_str()),
                                    available && tf == selected,
                                    "{tf:?}, {supported:?}"
                                );
                            }
                        }
                        let pos = output.shapes.iter().find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text)
                                if text.galley.text() == display_long(selected) =>
                            {
                                Some(text.pos + text.galley.size() / 2.0)
                            }
                            _ => None,
                        });
                        output.textures_delta.clear();
                        output.drop_without_applying_deltas();
                        (pos, action)
                    };
                    let (pos, _) = frame(vec![]);
                    let Some(pos) = pos else {
                        assert!(!available, "{selected:?}, {supported:?}");
                        continue;
                    };
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
    }
}
