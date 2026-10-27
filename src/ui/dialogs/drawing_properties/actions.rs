//! Actions from the drawing properties dialog.

use super::tabs::CoordinatesState;
use crate::drawings::{Drawing, DrawingTemplate, TimeframeVisibility};
use crate::model::LineStyle;
use crate::widget::Chart;
use egui::Color32;

/// Unique identifier for a drawing
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct DrawingId(pub usize);

/// Drawing properties that can be edited
#[derive(Clone, Debug)]
pub struct DrawingProps {
    /// Symbol whose drawing was opened; absent for manually assembled properties.
    pub symbol: Option<String>,
    /// Tool-specific settings, edited by the Inputs page.
    pub tool_settings: Option<Box<DrawingTemplate>>,
    /// Staged anchor edits, applied only to the drawing whose dialog was opened.
    pub coordinates: CoordinatesState,
    /// Save these properties as the default for new drawings of this tool.
    pub save_as_default: bool,
    /// Primary color
    pub color: Color32,
    /// Line width (1-10)
    pub line_width: f32,
    /// Line style
    pub line_style: LineStyle,
    /// Fill color (for shapes)
    pub fill_color: Option<Color32>,
    /// Fill enabled
    pub fill_enabled: bool,
    /// Extend line left
    pub extend_left: bool,
    /// Extend line right
    pub extend_right: bool,
    /// Text label
    pub label: String,
    /// Font size
    pub font_size: f32,
    /// Visible on all timeframes
    pub visible_all_timeframes: bool,
    /// Specific timeframes (if not all)
    pub visible_timeframes: Vec<String>,
}

impl Default for DrawingProps {
    fn default() -> Self {
        Self {
            symbol: None,
            tool_settings: None,
            coordinates: CoordinatesState::default(),
            save_as_default: false,
            color: crate::theme::Theme::default().ui.accent,
            line_width: 1.0,
            line_style: LineStyle::Solid,
            fill_color: None,
            fill_enabled: false,
            extend_left: false,
            extend_right: false,
            label: String::new(),
            font_size: 12.0,
            visible_all_timeframes: true,
            visible_timeframes: Vec::new(),
        }
    }
}

/// Actions from the drawing properties dialog
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum DrawingPropertiesAction {
    /// No action requested.
    None,
    /// Cancel the current interaction.
    Cancel,
    /// Apply the supplied properties to the drawing.
    Apply(DrawingId, DrawingProps),
    /// Delete the identified drawing.
    Delete(DrawingId),
    /// Duplicate the identified drawing.
    Clone(DrawingId),
}

impl DrawingProps {
    /// Read every editable property without changing the drawing or its selection.
    pub fn from_drawing(drawing: &Drawing, chart: &Chart) -> Self {
        Self {
            symbol: Some(chart.symbol().to_owned()),
            tool_settings: Some(Box::new(DrawingTemplate::from_drawing(drawing))),
            coordinates: CoordinatesState::from_drawing(drawing, chart),
            color: drawing.color32(),
            line_width: drawing.stroke_width,
            line_style: drawing.line_style,
            fill_color: drawing.fill_color32(),
            fill_enabled: drawing.fill_color.is_some(),
            extend_left: drawing.extend_left,
            extend_right: drawing.extend_right,
            font_size: drawing.font_size,
            visible_all_timeframes: drawing.timeframe_visibility.show_on_all,
            visible_timeframes: drawing
                .timeframe_visibility
                .visible_timeframes
                .iter()
                .cloned()
                .collect(),
            label: drawing.text.clone().unwrap_or_default(),
            ..Self::default()
        }
    }

    /// Validate and apply one undoable edit to the unlocked selection. Tool inputs affect only
    /// matching tools; coordinates affect only `id`. Errors leave all live drawings unchanged.
    pub fn apply_to_chart(&self, id: DrawingId, chart: &mut Chart) -> Result<(), &'static str> {
        if self
            .symbol
            .as_ref()
            .is_some_and(|symbol| symbol != chart.symbol())
        {
            return Err("The chart symbol changed; reopen drawing properties");
        }
        self.validate()?;
        let points = self.coordinates.resolve(chart)?;
        let target = chart
            .drawings()
            .drawings
            .iter()
            .find(|d| d.id == id.0)
            .ok_or("Drawing was removed")?;
        if target.locked
            && (self.save_as_default
                || !chart.drawings().is_selected(id.0)
                || !chart
                    .drawings()
                    .drawings
                    .iter()
                    .any(|drawing| !drawing.locked && chart.drawings().is_selected(drawing.id)))
        {
            return Err("Unlock the drawing before editing its properties");
        }
        if !points.is_empty() && points.len() != target.chart_points.len() {
            return Err("Drawing anchors changed; reopen its properties");
        }
        chart.user_action(|chart| {
            let manager = chart.drawings_mut();
            if !manager.is_selected(id.0) {
                manager.select(id.0);
            }
            manager.edit_selected(|drawing| {
                drawing.color = self.color.to_srgba_unmultiplied();
                drawing.stroke_width = self.line_width;
                drawing.line_style = self.line_style;
                drawing.extend_left = self.extend_left;
                drawing.extend_right = self.extend_right;
                drawing.text = (!self.label.is_empty()).then(|| self.label.clone());
                drawing.fill_color = self.fill_enabled.then(|| {
                    self.fill_color
                        .unwrap_or(self.color.gamma_multiply(0.2))
                        .to_srgba_unmultiplied()
                });
                drawing.font_size = self.font_size;
                drawing.timeframe_visibility = TimeframeVisibility {
                    show_on_all: self.visible_all_timeframes,
                    visible_timeframes: self.visible_timeframes.iter().cloned().collect(),
                };
                if let Some(template) = &self.tool_settings
                    && template.settings.tool_type == drawing.tool_type
                {
                    let settings = &template.settings;
                    drawing.fib_config = settings.fib_config.clone();
                    drawing.pitchfork = settings.pitchfork.clone();
                    drawing.gann = settings.gann.clone();
                    drawing.ghost_feed = settings.ghost_feed;
                    drawing.position_size = settings.position_size;
                    drawing.position_sizing = settings.position_sizing;
                    drawing.volume_profile = settings.volume_profile;
                    drawing.regression_trend = settings.regression_trend;
                    drawing.channel_midline = settings.channel_midline;
                }
                if drawing.id == id.0
                    && !points.is_empty()
                    && drawing
                        .chart_points
                        .iter()
                        .zip(&points)
                        .any(|(a, b)| a != b || a.time_anchor != b.time_anchor)
                {
                    drawing.chart_points = points.clone();
                    drawing.screen_anchor = None;
                }
            });
            if self.save_as_default
                && let Some(drawing) = manager.drawings.iter().find(|d| d.id == id.0)
            {
                let mut defaults = DrawingTemplate::from_drawing(drawing);
                defaults.settings.visible = true;
                manager.tool_defaults.insert(drawing.tool_type, defaults);
            }
        });
        Ok(())
    }

    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if !self.line_width.is_finite()
            || self.line_width <= 0.0
            || !self.font_size.is_finite()
            || self.font_size <= 0.0
        {
            return Err("Line width and font size must be positive and finite");
        }
        if let Some(template) = &self.tool_settings {
            let settings = &template.settings;
            if settings
                .fib_config
                .as_ref()
                .is_some_and(|c| c.levels.iter().any(|l| !l.value.is_finite()))
            {
                return Err("Fibonacci ratios must be finite");
            }
            if settings.pitchfork.as_ref().is_some_and(|c| {
                c.levels
                    .iter()
                    .any(|l| !l.value.is_finite() || l.value < 0.0)
            }) {
                return Err("Pitchfork multiples must be finite and nonnegative");
            }
            if settings.gann.as_ref().is_some_and(|c| {
                c.price_levels
                    .iter()
                    .chain(&c.time_levels)
                    .any(|v| !v.is_finite() || !(0.0..=1.0).contains(v))
            }) {
                return Err("Gann divisions must be between 0 and 1");
            }
            if settings.gann.as_ref().is_some_and(|c| {
                c.fan_ratios
                    .iter()
                    .chain(&c.arc_ratios)
                    .any(|v| !v.is_finite() || *v <= 0.0)
            }) {
                return Err("Gann fan and arc ratios must be positive and finite");
            }
            if settings.ghost_feed.is_some_and(|c| !c.is_valid()) {
                return Err(
                    "Ghost feed spacing must be positive and wick nonnegative, both finite",
                );
            }
            if !settings.position_sizing.is_valid() {
                return Err(
                    "Position economics must be positive and finite; quantity step cannot be negative",
                );
            }
            if let Some(size) = settings.position_size
                && size.quantity(1.0, 0.0).is_none()
            {
                return Err("Position size must be positive and finite; risk cannot exceed 100%");
            }
        }
        Ok(())
    }
}
