//! Timeframe Selector
//!
//! A dropdown menu for selecting chart timeframes with favorites support.
use crate::ext::UiExt;
use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Response, RichText, Sense, Stroke, StrokeKind, Ui, Vec2};
use std::collections::HashSet;

/// Theme colors for dropdown rendering
struct DropdownColors {
    bg: Color32,
    hover: Color32,
    selected: Color32,
    text: Color32,
    muted: Color32,
    border: Color32,
    favorite: Color32,
}

impl DropdownColors {
    fn from_ui(ui: &Ui) -> Self {
        let visuals = &ui.style().visuals;
        Self {
            bg: visuals.window_fill,
            hover: visuals.widgets.hovered.bg_fill,
            selected: visuals.selection.bg_fill,
            text: visuals.text_color(),
            muted: visuals.widgets.noninteractive.fg_stroke.color,
            border: visuals.widgets.noninteractive.bg_stroke.color,
            favorite: visuals.warn_fg_color,
        }
    }
}

/// Timeframe unit
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TimeframeUnit {
    Tick,
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
            TimeframeUnit::Tick => "T",
            TimeframeUnit::Second => "S",
            TimeframeUnit::Minute => "",
            TimeframeUnit::Hour => "H",
            TimeframeUnit::Day => "D",
            TimeframeUnit::Week => "W",
            TimeframeUnit::Month => "M",
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            TimeframeUnit::Tick => "ticks",
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

    /// Get display string like "1D", "4H", "15"
    pub fn display(&self) -> String {
        match self.unit {
            TimeframeUnit::Minute => format!("{}", self.value),
            _ => format!("{}{}", self.value, self.unit.short_name()),
        }
    }

    /// Get long display string like "1 day", "4 hours"
    pub fn display_long(&self) -> String {
        let unit_name = if self.value == 1 {
            match self.unit {
                TimeframeUnit::Tick => "tick",
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

    /// Get all predefined timeframes
    pub fn all_presets() -> Vec<Timeframe> {
        vec![
            // Ticks
            Timeframe::new(1, TimeframeUnit::Tick),
            Timeframe::new(10, TimeframeUnit::Tick),
            Timeframe::new(100, TimeframeUnit::Tick),
            Timeframe::new(1000, TimeframeUnit::Tick),
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
    pub btn_width: f32,
    pub dropdown_width: f32,
    pub item_height: f32,
    pub bg_color: Color32,
    pub hover_color: Color32,
    pub sel_color: Color32,
    pub text_color: Color32,
    pub muted_color: Color32,
    pub favorite_color: Color32,
}

impl Default for TimeframeSelectorConfig {
    fn default() -> Self {
        // Colors are TRANSPARENT to signal that colors should be fetched from
        // ui.style().visuals at render time for proper theme support.
        Self {
            btn_width: 50.0,
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
        let mut result = None;
        let text_color = ui.style().visuals.text_color();
        let error_color = ui.style().visuals.error_fg_color;

        ui.spaced_separator();

        ui.label(
            RichText::new("Custom Timeframe")
                .size(typography::SM)
                .color(text_color),
        );
        ui.space_xs();

        ui.horizontal(|ui| {
            // Number input
            let input = egui::TextEdit::singleline(&mut self.value_input)
                .desired_width(DESIGN_TOKENS.sizing.button_lg)
                .font(egui::FontId::proportional(typography::MD));
            ui.add(input);

            // Unit selector
            ui.combo_select_width(
                "custom_tf_unit",
                &mut self.unit,
                Self::available_units().iter().map(|(u, _)| *u),
                |u| {
                    match u {
                        TimeframeUnit::Second => "Seconds",
                        TimeframeUnit::Minute => "Minutes",
                        TimeframeUnit::Hour => "Hours",
                        TimeframeUnit::Day => "Days",
                        other => other.name(),
                    }
                    .to_string()
                },
                DESIGN_TOKENS.sizing.button_lg,
            );
        });

        // Error message
        if let Some(ref err) = self.error {
            ui.space_xs();
            ui.label(RichText::new(err).size(typography::XS).color(error_color));
        }

        ui.space_sm();

        // Apply button
        if ui
            .button(RichText::new("Apply").size(typography::SM))
            .clicked()
        {
            result = self.validate();
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
        let mut action = TimeframeAction::None;

        // Main button
        let btn_res = self.draw_btn(ui);
        if btn_res.clicked() {
            self.is_open = !self.is_open;
        }

        // Dropdown
        if self.is_open {
            let btn_rect = btn_res.rect;
            action = self.draw_dropdown(ui, btn_rect);

            // Close if clicked outside
            if ui.input(|i| i.pointer.any_click())
                && let Some(pos) = ui.input(|i| i.pointer.hover_pos())
                && !btn_rect.contains(pos)
            {
                // Check if inside dropdown
                let dropdown_rect = Rect::from_min_size(
                    Pos2::new(btn_rect.min.x, btn_rect.max.y + 2.0),
                    Vec2::new(self.config.dropdown_width, 400.0),
                );
                if !dropdown_rect.contains(pos) {
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
        let muted_color = ui.style().visuals.widgets.noninteractive.fg_stroke.color;

        ui.painter()
            .rect_filled(rect, DESIGN_TOKENS.rounding.md, bg_color);
        ui.painter().rect_stroke(
            rect,
            DESIGN_TOKENS.rounding.md,
            Stroke::new(DESIGN_TOKENS.stroke.hairline, border_color),
            StrokeKind::Outside,
        );

        // Timeframe text
        ui.painter().text(
            rect.center() - Vec2::new(5.0, 0.0),
            egui::Align2::CENTER_CENTER,
            self.current.display(),
            egui::FontId::proportional(typography::LG),
            text_color,
        );

        // Dropdown arrow
        ui.painter().text(
            Pos2::new(rect.right() - DESIGN_TOKENS.spacing.xl, rect.center().y),
            egui::Align2::CENTER_CENTER,
            "▾",
            egui::FontId::proportional(typography::XS),
            muted_color,
        );

        response
    }

    fn draw_dropdown(&mut self, ui: &mut Ui, btn_rect: Rect) -> TimeframeAction {
        let mut action = TimeframeAction::None;
        let colors = DropdownColors::from_ui(ui);
        let current = self.current;
        let favorites = self.favorites.clone();
        let custom_tfs = self.custom_timeframes.clone();
        let show_custom = self.show_custom_input;

        // fixed dropdown dimensions
        let dropdown_height = 380.0;
        let dropdown_width = self.config.dropdown_width;

        // Calculate position - ensure dropdown stays within screen bounds
        let screen_rect = ui.ctx().content_rect();
        let ideal_pos = Pos2::new(btn_rect.min.x, btn_rect.max.y + DESIGN_TOKENS.spacing.xs);

        // Desktop: ensure dropdown doesn't go off right edge
        let dropdown_pos = {
            let x = ideal_pos
                .x
                .min(screen_rect.max.x - dropdown_width - DESIGN_TOKENS.spacing.sm);
            // Ensure dropdown doesn't go off bottom edge
            let y = if ideal_pos.y + dropdown_height > screen_rect.max.y - DESIGN_TOKENS.spacing.sm
            {
                // Position above the button if not enough space below
                (btn_rect.min.y - dropdown_height - DESIGN_TOKENS.spacing.xs)
                    .max(DESIGN_TOKENS.spacing.sm)
            } else {
                ideal_pos.y
            };
            Pos2::new(x.max(DESIGN_TOKENS.spacing.sm), y)
        };

        let mut toggle_custom = false;
        let mut custom_dialog = self.custom_dialog.clone();

        egui::Area::new(egui::Id::new("timeframe_dropdown"))
            .fixed_pos(dropdown_pos)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                Self::draw_dropdown_frame(ui, &colors, dropdown_width, |ui| {
                    Self::draw_dropdown_content(ui, &colors, current, &favorites, &mut action);

                    // Custom timeframes section
                    if !custom_tfs.is_empty() {
                        ui.space_sm();
                        Self::draw_section_label(ui, "Custom", colors.muted);
                        let item_height = DESIGN_TOKENS.sizing.button_sm - DESIGN_TOKENS.spacing.xs;
                        for tf in &custom_tfs {
                            if Self::draw_tf_row(
                                ui,
                                *tf,
                                current == *tf,
                                false,
                                &colors,
                                170.0,
                                item_height,
                            ) {
                                action = TimeframeAction::Select(*tf);
                            }
                        }
                    }

                    // "Custom..." button
                    ui.space_sm();
                    let custom_btn_size = Vec2::new(
                        170.0,
                        DESIGN_TOKENS.sizing.button_sm - DESIGN_TOKENS.spacing.xs,
                    );
                    let (rect, response) = ui.allocate_exact_size(custom_btn_size, Sense::click());

                    if response.hovered() {
                        ui.painter()
                            .rect_filled(rect, DESIGN_TOKENS.rounding.sm, colors.hover);
                    }
                    ui.painter().text(
                        Pos2::new(rect.min.x + DESIGN_TOKENS.spacing.lg, rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        "+",
                        egui::FontId::proportional(typography::MD),
                        colors.muted,
                    );
                    ui.painter().text(
                        Pos2::new(rect.min.x + DESIGN_TOKENS.sizing.button_md, rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        "Custom...",
                        egui::FontId::proportional(typography::MD),
                        colors.text,
                    );
                    if response.clicked() {
                        toggle_custom = true;
                    }

                    // Custom dialog
                    if show_custom && let Some(tf) = custom_dialog.show(ui) {
                        action = TimeframeAction::Select(tf);
                    }
                });
            });

        self.custom_dialog = custom_dialog;
        if toggle_custom {
            self.show_custom_input = !self.show_custom_input;
        }

        if matches!(action, TimeframeAction::Select(_)) {
            self.is_open = false;
            self.show_custom_input = false;
        }
        action
    }

    fn draw_dropdown_frame(
        ui: &mut Ui,
        colors: &DropdownColors,
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
                ui.set_min_width(width - DESIGN_TOKENS.spacing.lg);
                ui.set_max_height(400.0);
                egui::ScrollArea::vertical()
                    .max_height(380.0)
                    .show(ui, content);
            });
    }

    fn draw_dropdown_content(
        ui: &mut Ui,
        colors: &DropdownColors,
        current: Timeframe,
        favorites: &HashSet<Timeframe>,
        action: &mut TimeframeAction,
    ) {
        const ITEM_WIDTH: f32 = 170.0;
        let item_height = DESIGN_TOKENS.sizing.button_sm - DESIGN_TOKENS.spacing.xs;

        // Favorites section
        if !favorites.is_empty() {
            Self::draw_section_label(ui, "Favorites", colors.muted);
            let mut favorites_vec: Vec<_> = favorites.iter().copied().collect();
            favorites_vec.sort_by_key(|tf| (tf.unit as u8, tf.value));
            for tf in favorites_vec {
                if Self::draw_tf_row(ui, tf, current == tf, true, colors, ITEM_WIDTH, item_height) {
                    *action = TimeframeAction::Select(tf);
                }
            }
            ui.space_lg();
        }

        // Unit sections
        Self::draw_unit_sections(
            ui,
            colors,
            current,
            favorites,
            action,
            ITEM_WIDTH,
            item_height,
        );
    }

    fn draw_section_label(ui: &mut Ui, text: &str, color: Color32) {
        ui.label(RichText::new(text).size(typography::SM).color(color));
        ui.space_xs();
    }

    fn draw_unit_sections(
        ui: &mut Ui,
        colors: &DropdownColors,
        current: Timeframe,
        favorites: &HashSet<Timeframe>,
        action: &mut TimeframeAction,
        item_width: f32,
        item_height: f32,
    ) {
        let units = [
            (TimeframeUnit::Tick, "Ticks"),
            (TimeframeUnit::Second, "Seconds"),
            (TimeframeUnit::Minute, "Minutes"),
            (TimeframeUnit::Hour, "Hours"),
            (TimeframeUnit::Day, "Days"),
            (TimeframeUnit::Week, "Weeks"),
            (TimeframeUnit::Month, "Months"),
        ];

        for (unit, header) in units {
            let tfs = Timeframe::by_unit(unit);
            if tfs.is_empty() {
                continue;
            }
            Self::draw_section_label(ui, header, colors.muted);
            for tf in tfs {
                let is_fav = favorites.contains(&tf);
                if Self::draw_tf_row(
                    ui,
                    tf,
                    current == tf,
                    is_fav,
                    colors,
                    item_width,
                    item_height,
                ) {
                    *action = TimeframeAction::Select(tf);
                }
            }
            ui.space_sm();
        }
    }

    fn draw_tf_row(
        ui: &mut Ui,
        tf: Timeframe,
        is_current: bool,
        is_fav: bool,
        colors: &DropdownColors,
        width: f32,
        height: f32,
    ) -> bool {
        let (rect, response) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click());

        // Background
        if is_current {
            ui.painter()
                .rect_filled(rect, DESIGN_TOKENS.rounding.sm, colors.selected);
        } else if response.hovered() {
            ui.painter()
                .rect_filled(rect, DESIGN_TOKENS.rounding.sm, colors.hover);
        }

        // Star indicator
        let (star_char, star_color) = if is_fav {
            ("*", colors.favorite)
        } else {
            ("*", colors.muted)
        };
        ui.painter().text(
            Pos2::new(rect.min.x + DESIGN_TOKENS.spacing.lg, rect.center().y),
            egui::Align2::LEFT_CENTER,
            star_char,
            egui::FontId::proportional(typography::SM),
            star_color,
        );

        // Timeframe display
        ui.painter().text(
            Pos2::new(rect.min.x + DESIGN_TOKENS.sizing.button_md, rect.center().y),
            egui::Align2::LEFT_CENTER,
            tf.display(),
            egui::FontId::proportional(typography::MD),
            colors.text,
        );

        // Long description
        ui.painter().text(
            Pos2::new(rect.min.x + 60.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            tf.display_long(),
            egui::FontId::proportional(typography::SM),
            colors.muted,
        );

        response.clicked()
    }
}
