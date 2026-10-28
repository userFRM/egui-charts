//! Timeframe Selector
//!
//! A dropdown menu for selecting chart timeframes with favorites support.
mod custom;

use crate::icons::icons as embedded_icons;
use crate::localization as l10n;
use crate::model::Timeframe;
use crate::styles::focus::focus_ring_color;
use crate::styles::icons as icon_sizes;
use crate::theme::{Theme, ThemeExt};
use crate::tokens::DESIGN_TOKENS;
use crate::ui::top_toolbar::buttons::Trigger;
use crate::ui_kit::list_item::{ContentContext, LabelContent, ListItem, ListItemContent};
use egui::{Color32, Rect, RichText, Role, Ui, WidgetInfo, pos2, vec2};
use std::collections::HashSet;
use std::hash::Hash;
use std::num::{NonZeroU32, NonZeroU64};

pub use custom::CustomTimeframeDialog;

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
    if value == 1
        && matches!(
            unit,
            TimeframeUnit::Day | TimeframeUnit::Week | TimeframeUnit::Month
        )
    {
        unit.short_name().to_owned()
    } else {
        format!("{value}{}", unit.short_name())
    }
}

pub(crate) fn display_long(tf: Timeframe) -> String {
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
    /// Minimum width of each interval button; it is otherwise sized to its label.
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
    last_non_favorite: Option<Timeframe>,
}

impl Default for TimeframeSelector {
    fn default() -> Self {
        Self::new(Timeframe::Day1)
    }
}

impl TimeframeSelector {
    /// Create an interval selector with `initial` selected and no favorites.
    pub fn new(initial: Timeframe) -> Self {
        Self {
            current: initial,
            is_open: false,
            favorites: HashSet::new(),
            custom_timeframes: Vec::new(),
            config: TimeframeSelectorConfig::default(),
            show_custom_input: false,
            custom_dialog: CustomTimeframeDialog::default(),
            supported_timeframes: None,
            hidden_timeframes: HashSet::new(),
            last_non_favorite: None,
        }
    }

    /// Restrict interval choices to those the connected source can serve, such as the chart
    /// instrument's [`supported_timeframes`](crate::model::Instrument::supported_timeframes).
    /// `None` allows every interval; an empty list allows none. Update this when the source or
    /// symbol changes. Saved favourites and custom intervals are retained while unavailable.
    pub fn set_supported_timeframes(&mut self, timeframes: Option<Vec<crate::model::Timeframe>>) {
        self.supported_timeframes = timeframes.map(|values| values.into_iter().collect());
    }

    pub(crate) fn supports(&self, timeframe: Timeframe) -> bool {
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

    /// Show favorite interval buttons and the interval menu trigger.
    pub fn show(&mut self, ui: &mut Ui) -> TimeframeAction {
        let extra = retain_non_favorite(&mut self.last_non_favorite, self.current, &self.favorites)
            .filter(|tf| self.supports(*tf));
        let favorites: Vec<_> = PRESETS
            .iter()
            .chain(&self.custom_timeframes)
            .copied()
            .filter(|tf| {
                self.favorites.contains(tf)
                    && self.supports(*tf)
                    && !self.hidden_timeframes.contains(tf)
            })
            .collect();
        let (btn_res, clicked) = favorite_strip(
            ui,
            self.current,
            favorites,
            extra,
            self.is_open,
            "Chart interval",
            |ui, tf| self.interval_button(ui, tf),
        );
        let mut selected = TimeframeAction::None;
        if let Some(tf) = clicked {
            self.current = tf;
            selected = TimeframeAction::Select(tf);
        }

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
        if action == TimeframeAction::None {
            selected
        } else {
            action
        }
    }

    /// A toolbar button showing `tf`'s short label, named by its long form.
    fn interval_button(&self, ui: &Ui, tf: Timeframe) -> Trigger<'static> {
        let theme = ui.active_theme();
        let label = display(tf);
        let text = ui.painter().layout_no_wrap(
            l10n::text(ui.ctx(), &label),
            egui::FontId::proportional(theme.text.title),
            Color32::PLACEHOLDER,
        );
        Trigger {
            icon: None,
            name: display_long(tf),
            label,
            // Whole points keep the end of the scrolling toolbar reachable.
            min_width: (text.size().x + 2.0 * theme.spacing.lg)
                .ceil()
                .max(self.config.btn_width),
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

/// Remembers `current` as the latest chosen choice that is not a favourite, and returns that
/// choice unless it has been starred since.
pub(super) fn retain_non_favorite<T: Copy + Eq + Hash>(
    last: &mut Option<T>,
    current: T,
    favorites: &HashSet<T>,
) -> Option<T> {
    if !favorites.contains(&current) {
        *last = Some(current);
    }
    last.filter(|choice| !favorites.contains(choice))
}

/// The toolbar buttons of a control with a menu of choices and starred favourites. Without
/// `favorites` (the starred choices in menu order) it is one button for `current` that opens the
/// menu. Otherwise there is a button for each favourite, then one for `extra`, then a chevron
/// named `menu_name` that opens the menu, and `current`'s button is pressed. Returns the menu's
/// anchor and the choice whose button was clicked.
pub(super) fn favorite_strip<T: Copy + PartialEq>(
    ui: &mut Ui,
    current: T,
    favorites: Vec<T>,
    extra: Option<T>,
    open: bool,
    menu_name: &str,
    button: impl Fn(&Ui, T) -> Trigger<'static>,
) -> (egui::Response, Option<T>) {
    if favorites.is_empty() {
        return (button(ui, current).show(ui, open), None);
    }
    let mut clicked = None;
    for choice in favorites.into_iter().chain(extra) {
        if button(ui, choice).show(ui, choice == current).clicked() {
            clicked = Some(choice);
        }
    }
    let menu = Trigger {
        icon: Some(&embedded_icons::CHEVRON_DOWN),
        name: menu_name.to_owned(),
        label: String::new(),
        min_width: 0.0,
        filled: false,
    }
    .show(ui, open);
    (menu, clicked)
}

/// Width of the star's slot at the end of a menu row.
pub(super) const STAR_SLOT_W: f32 = 28.0;

/// The star at the end of a menu row, which adds the row's choice to the favourites or removes
/// it. With `on_hover` it is drawn only on a favourite, a hovered row, or while a key has focus
/// in the row. Returns whether it was clicked.
pub(super) fn favorite_star(
    ui: &Ui,
    context: &ContentContext<'_>,
    favorite: bool,
    on_hover: bool,
    color: Color32,
) -> bool {
    let rect = context.rect;
    let star_rect = Rect::from_center_size(
        pos2(rect.right() - icon_sizes::SM / 2.0, rect.center().y),
        vec2(STAR_SLOT_W, rect.height()),
    );
    let star = ui.interact(
        star_rect,
        context.response.id.with("favorite"),
        egui::Sense::click(),
    );
    let tooltip = if favorite {
        "Remove from favorites"
    } else {
        "Add to favorites"
    };
    let translated_label = l10n::text(ui.ctx(), tooltip);
    star.widget_info(|| {
        WidgetInfo::selected(Role::Button, ui.is_enabled(), favorite, &translated_label)
    });
    let shown = !on_hover
        || favorite
        || ui.rect_contains_pointer(context.response.rect)
        || context.response.has_focus()
        || star.has_focus();
    if shown {
        let icon = if favorite {
            &embedded_icons::STAR_FILLED
        } else {
            &embedded_icons::STAR_EMPTY
        };
        icon.as_image_tinted(vec2(icon_sizes::SM, icon_sizes::SM), color)
            .paint_at(
                ui,
                Rect::from_center_size(star_rect.center(), vec2(icon_sizes::SM, icon_sizes::SM)),
            );
    }
    menu_row_focus(ui, &star);
    let clicked = star.clicked();
    star.on_hover_text(translated_label);
    clicked
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

        *self.toggle_favorite = favorite_star(ui, context, self.favorite, false, text);
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
    fn interval_buttons_retain_last_non_favorite_without_duplicates() {
        for theme in [Theme::light(), Theme::dark()] {
            let ctx = egui::Context::default();
            egui_extras::install_image_loaders(&ctx);
            crate::theme::apply_to_egui(&ctx, &theme);
            ctx.enable_accesskit();
            let mut selector = TimeframeSelector::new(Model::Min1);
            assert!(selector.preferences().favorites.is_empty());
            let frame = |selector: &mut TimeframeSelector, events| {
                let mut action = TimeframeAction::None;
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            vec2(1440.0, 2400.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        ui.horizontal(|ui| action = selector.show(ui));
                    },
                );
                let labels: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if !text.galley.text().is_empty() => Some((
                            text.galley.text().to_owned(),
                            text.pos + text.galley.size() / 2.0,
                        )),
                        _ => None,
                    })
                    .collect();
                let buttons: Vec<_> = output
                    .platform_output
                    .accesskit_update
                    .iter()
                    .flat_map(|update| &update.nodes)
                    .filter_map(|(_, node)| {
                        let bounds = node.bounds()?;
                        Some((
                            node.label()?.to_owned(),
                            Rect::from_min_max(
                                pos2(bounds.x0 as f32, bounds.y0 as f32),
                                pos2(bounds.x1 as f32, bounds.y1 as f32),
                            ),
                        ))
                    })
                    .collect();
                let active = ctx.global_style().visuals.widgets.active.bg_fill;
                let pressed: Vec<_> = labels
                    .iter()
                    .filter(|(_, pos)| {
                        output.shapes.iter().any(|shape| match &shape.shape {
                            egui::Shape::Rect(rect) => {
                                rect.rect.contains(*pos) && rect.fill == active
                            }
                            _ => false,
                        })
                    })
                    .map(|(label, _)| label.clone())
                    .collect();
                output.drop_without_applying_deltas();
                (action, labels, buttons, pressed)
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
            let strip = |selector: &mut TimeframeSelector, expected: &[&str], selected: &str| {
                frame(selector, vec![]);
                let (_, labels, _, pressed) = frame(selector, vec![egui::Event::PointerGone]);
                assert_eq!(
                    labels
                        .iter()
                        .map(|(text, _)| text.as_str())
                        .collect::<Vec<_>>(),
                    expected
                );
                assert_eq!(
                    pressed,
                    if selected.is_empty() {
                        vec![]
                    } else {
                        vec![selected.to_owned()]
                    }
                );
            };
            strip(&mut selector, &["1m"], "");
            for tf in [Model::Min5, Model::Hour1, Model::Day1] {
                let (_, _, buttons, _) = frame(&mut selector, vec![]);
                let trigger = buttons
                    .iter()
                    .find(|(name, _)| name == "Chart interval")
                    .or_else(|| buttons.iter().find(|(name, _)| name == "1 minute"))
                    .unwrap()
                    .1;
                click(&mut selector, trigger.center());
                frame(&mut selector, vec![]);
                let (_, _, buttons, _) = frame(&mut selector, vec![]);
                let row = buttons
                    .iter()
                    .find(|(name, _)| *name == display_long(tf))
                    .unwrap_or_else(|| panic!("{tf:?}: {buttons:?}"))
                    .1;
                assert_eq!(
                    click(
                        &mut selector,
                        pos2(row.right() - icon_sizes::SM / 2.0, row.center().y)
                    ),
                    TimeframeAction::ToggleFavorite(tf)
                );
                assert!(selector.preferences().favorites.contains(&tf));
                // The menu stays open after starring; close it before checking the strip.
                click(&mut selector, pos2(1000.0, 2200.0));
            }
            strip(&mut selector, &["5m", "1h", "D", "1m"], "1m");
            let (_, labels, _, _) = frame(&mut selector, vec![]);
            let favorite = labels.iter().find(|(name, _)| name == "5m").unwrap().1;
            assert_eq!(
                click(&mut selector, favorite),
                TimeframeAction::Select(Model::Min5)
            );
            strip(&mut selector, &["5m", "1h", "D", "1m"], "5m");
            let (_, _, buttons, _) = frame(&mut selector, vec![]);
            let trigger = buttons
                .iter()
                .find(|(name, _)| name == "Chart interval")
                .unwrap()
                .1;
            click(&mut selector, trigger.center());
            frame(&mut selector, vec![]);
            let (_, labels, _, _) = frame(&mut selector, vec![]);
            let quarter = labels
                .iter()
                .find(|(name, _)| name == "3 months")
                .unwrap()
                .1;
            assert_eq!(
                click(&mut selector, quarter),
                TimeframeAction::Select("3month".parse().unwrap())
            );
            strip(&mut selector, &["5m", "1h", "D", "3M"], "3M");
            let saved = selector.preferences();
            let mut restored = TimeframeSelector::new(Model::Min5);
            restored.set_preferences(saved.clone());
            strip(&mut restored, &["5m", "1h", "D"], "5m");
            assert_eq!(restored.preferences(), saved);
            restored.current = Model::Week1;
            strip(&mut restored, &["5m", "1h", "D", "W"], "W");
            restored.favorites.insert(Model::Week1);
            strip(&mut restored, &["5m", "1h", "D", "W"], "W");
            restored.favorites.remove(&Model::Week1);
            strip(&mut restored, &["5m", "1h", "D", "W"], "W");
            let custom = [
                interval(7, TimeframeUnit::Minute),
                interval(8, TimeframeUnit::Minute),
            ];
            restored.custom_timeframes.extend(custom);
            restored.favorites.extend(custom);
            strip(&mut restored, &["5m", "1h", "D", "7m", "8m", "W"], "W");
            restored.set_supported_timeframes(Some(vec![Model::Min5, Model::Week1]));
            strip(&mut restored, &["5m", "W"], "W");
        }
    }

    #[test]
    fn custom_intervals_and_stars_survive_saved_preferences() {
        for theme in [Theme::light(), Theme::dark()] {
            for width in [320.0, 400.0] {
                let ctx = egui::Context::default();
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
                            screen_rect: Some(Rect::from_min_size(
                                egui::Pos2::ZERO,
                                vec2(width, 2600.0),
                            )),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            action = selector.menu_ui(ui, &theme, 2500.0);
                            right = ui.min_rect().right();
                        },
                    );
                    assert!(right <= width, "{right} exceeds {width}");
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
                (Model::Day1, "D", "1 day"),
                (Model::Week1, "W", "1 week"),
                (Model::Month1, "M", "1 month"),
                ("2D".parse().unwrap(), "2D", "2 days"),
                ("2W".parse().unwrap(), "2W", "2 weeks"),
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
                    if !PRESETS.contains(&selected) {
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
