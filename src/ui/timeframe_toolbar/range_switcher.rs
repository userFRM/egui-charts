//! Range Switcher Widget
//!
//! Provides quick date range selection buttons: 1D, 5D, 1M, 6M, YTD, 1Y, 5Y, All
use crate::localization as l10n;
use crate::theme::{Theme, follow_active_theme, theme_derived};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, FontId, Pos2, Response, Stroke, Ui, Vec2};

pub use crate::model::DateRange;

/// Configuration for the Range Switcher widget
#[derive(Debug, Clone)]
pub struct RangeSwitcherConfig {
    /// Available date ranges to show
    pub ranges: Vec<DateRange>,
    /// Button height in pixels
    pub btn_height: f32,
    /// Button min width in pixels
    pub btn_min_width: f32,
    /// Spacing between btns
    pub spacing: f32,
    /// Background color (inactive button)
    pub background_color: Color32,
    /// Active button background color
    pub active_color: Color32,
    /// Hover background color
    pub hover_color: Color32,
    /// Text color (inactive)
    pub text_color: Color32,
    /// Text color (active)
    pub active_text_color: Color32,
    /// Border radius
    pub border_radius: f32,
    /// Font size
    pub font_size: f32,
    /// Show border around btns
    pub show_border: bool,
    /// Border color
    pub border_color: Color32,
}

impl RangeSwitcherConfig {
    /// Create config from `theme`
    pub fn from_theme(theme: &Theme) -> Self {
        let ui = &theme.ui;
        Self {
            ranges: DateRange::default_presets(),
            btn_height: DESIGN_TOKENS.sizing.button_sm,
            btn_min_width: DESIGN_TOKENS.sizing.button_md + theme.spacing.sm, // ~32px
            spacing: theme.spacing.xs,
            background_color: Color32::TRANSPARENT, // Let parent show through
            active_color: ui.btn_bg_active,
            hover_color: ui.btn_bg_hover,
            text_color: ui.text_secondary,
            active_text_color: ui.text,
            border_radius: theme.radius.md,
            font_size: theme.text.small,
            show_border: true,
            border_color: ui.border,
        }
    }

    /// Create config with standard presets
    pub fn standard() -> Self {
        Self::default()
    }

    /// Create config with minimal presets (1D, 1W, 1M, 1Y)
    pub fn minimal() -> Self {
        Self {
            ranges: vec![
                DateRange::Day1,
                DateRange::Month1,
                DateRange::Year1,
                DateRange::All,
            ],
            ..Default::default()
        }
    }

    /// Create config with custom ranges
    pub fn with_ranges(ranges: Vec<DateRange>) -> Self {
        Self {
            ranges,
            ..Default::default()
        }
    }
}

impl Default for RangeSwitcherConfig {
    fn default() -> Self {
        // Default uses light UI chrome theme
        Self::from_theme(&Theme::dark())
    }
}

theme_derived!(RangeSwitcherConfig {
    ranges,
    btn_height,
    btn_min_width,
    spacing,
    background_color,
    active_color,
    hover_color,
    text_color,
    active_text_color,
    border_radius,
    font_size,
    show_border,
    border_color
});

/// Range Switcher Widget
///
/// Displays horizontal row of date range btns (1D, 5D, 1M, etc.)
/// Returns the selected range when a button is clicked.
pub struct RangeSwitcher {
    /// Currently selected range
    selected: DateRange,
    /// Configuration
    config: RangeSwitcherConfig,
    /// Theme and config `config` was last derived from; `None` when the caller supplied it.
    config_theme: Option<(Theme, RangeSwitcherConfig)>,
    /// Unique ID for the widget
    id: egui::Id,
}

impl Default for RangeSwitcher {
    fn default() -> Self {
        Self::new()
    }
}

impl RangeSwitcher {
    /// Create a new range switcher with default config
    pub fn new() -> Self {
        Self {
            selected: DateRange::Month1,
            config: RangeSwitcherConfig::default(),
            config_theme: Some((Theme::dark(), RangeSwitcherConfig::default())),
            id: egui::Id::unique("range_switcher"),
        }
    }

    /// Create with custom config
    pub fn with_config(config: RangeSwitcherConfig) -> Self {
        Self {
            selected: config.ranges.first().copied().unwrap_or(DateRange::Month1),
            config,
            config_theme: None,
            id: egui::Id::unique("range_switcher"),
        }
    }

    /// Create with custom ID (for multiple instances)
    pub fn with_id(mut self, id: impl std::hash::Hash + std::fmt::Debug) -> Self {
        self.id = egui::Id::unique(id);
        self
    }

    /// Set the selected range
    pub fn set_sel(&mut self, range: DateRange) {
        self.selected = range;
    }

    /// Get the currently selected range
    pub fn selected(&self) -> DateRange {
        self.selected
    }

    /// Calculate the total width needed for the widget
    pub fn preferred_width(&self, _ui: &Ui) -> f32 {
        let mut total_width = 0.0;

        for range in &self.config.ranges {
            // Estimate width based on character count (roughly 7px per char at 11pt)
            let char_width = self.config.font_size * 0.6;
            let text_width = char_width * range.label().len() as f32;
            total_width += text_width.max(self.config.btn_min_width) + 16.0; // padding
            total_width += self.config.spacing;
        }

        total_width - self.config.spacing // Remove last spacing
    }

    /// Render the range switcher and return the newly selected range (if changed)
    pub fn show(&mut self, ui: &mut Ui) -> Option<DateRange> {
        follow_active_theme(
            ui.ctx(),
            &mut self.config_theme,
            &mut self.config,
            RangeSwitcherConfig::from_theme,
        );
        let mut new_selection = None;

        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = self.config.spacing;

            for &range in &self.config.ranges {
                let is_sel = self.selected == range;
                let response = self.render_btn(ui, range, is_sel);

                if response.clicked() && !is_sel {
                    self.selected = range;
                    new_selection = Some(range);
                }
            }
        });

        new_selection
    }

    /// Render at a specific position (for overlay positioning)
    pub fn show_at(&mut self, ui: &mut Ui, pos: Pos2) -> Option<DateRange> {
        let mut new_selection = None;

        egui::Area::new(self.id)
            .fixed_pos(pos)
            .order(egui::Order::Foreground)
            .show(ui.ctx(), |ui| {
                egui::Frame::NONE.fill(Color32::TRANSPARENT).show(ui, |ui| {
                    new_selection = self.show(ui);
                });
            });

        new_selection
    }

    /// Render a single button
    fn render_btn(&self, ui: &mut Ui, range: DateRange, is_sel: bool) -> Response {
        let label = range.label();
        let font_id = FontId::proportional(self.config.font_size);

        // Calculate button size (estimate width based on character count)
        let char_width = self.config.font_size * 0.6;
        let text_width = char_width * label.len() as f32;
        let btn_width = (text_width + 16.0).max(self.config.btn_min_width);
        let btn_size = Vec2::new(btn_width, self.config.btn_height);

        // Allocate space and get response
        let (rect, response) = ui.allocate_exact_size(btn_size, egui::Sense::click());
        let translated_label = l10n::text(ui.ctx(), label);
        response.widget_info(|| {
            egui::WidgetInfo::selected(
                egui::Role::Button,
                response.enabled(),
                is_sel,
                &translated_label,
            )
        });
        crate::styles::focus::draw_focus_ring(ui, &response, self.config.border_radius);

        // Determine colors based on state
        let (bg_color, text_color) = if is_sel {
            (self.config.active_color, self.config.active_text_color)
        } else if response.hovered() {
            (self.config.hover_color, self.config.text_color)
        } else {
            (self.config.background_color, self.config.text_color)
        };

        // Draw background
        ui.painter()
            .rect_filled(rect, self.config.border_radius, bg_color);

        // Draw border if enabled
        if self.config.show_border {
            ui.painter().rect_stroke(
                rect,
                self.config.border_radius,
                Stroke::new(DESIGN_TOKENS.stroke.hairline, self.config.border_color),
                egui::epaint::StrokeKind::Inside,
            );
        }

        // Draw text
        ui.painter().text(
            rect.center(),
            egui::Align2::CENTER_CENTER,
            l10n::text(ui.ctx(), label),
            font_id,
            text_color,
        );

        response
    }
}
