//! Actions from the drawing properties dialog.

use super::tabs::CoordinatesState;
use crate::drawings::validation::{MAX_PITCHFORK_LEVEL, MAX_STROKE_WIDTH};
use crate::drawings::{Drawing, DrawingTemplate, TimeframeVisibility};
use crate::model::LineStyle;
use crate::styles::typography::FONT_SIZES;
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
        let edits = self.edited_drawings(id, chart)?;
        chart.user_action(|chart| {
            let manager = chart.drawings_mut();
            if !manager.is_selected(id.0) {
                manager.select(id.0);
            }
            let ids: Vec<_> = edits.iter().map(|drawing| drawing.id).collect();
            manager.edit_drawings(&ids, |drawing| {
                if let Some(edited) = edits.iter().find(|edited| edited.id == drawing.id) {
                    *drawing = edited.clone();
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

    /// Prepare validated edits without history or persistence side effects, for live preview.
    pub(super) fn edited_drawings(
        &self,
        id: DrawingId,
        chart: &Chart,
    ) -> Result<Vec<Drawing>, &'static str> {
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
        let selected = chart.drawings().is_selected(id.0);
        let mut edits = Vec::new();
        for mut drawing in chart
            .drawings()
            .drawings
            .iter()
            .filter(|drawing| {
                !drawing.locked
                    && if selected {
                        chart.drawings().is_selected(drawing.id)
                    } else {
                        drawing.id == id.0
                    }
            })
            .cloned()
        {
            drawing.color = self.color.to_srgba_unmultiplied();
            drawing.stroke_width = self.line_width;
            drawing.line_style = self.line_style;
            drawing.extend_left = self.extend_left;
            drawing.extend_right = self.extend_right;
            drawing.text = (!self.label.is_empty()).then(|| self.label.clone());
            let fill = self
                .fill_enabled
                .then(|| self.fill_color.unwrap_or(self.color.gamma_multiply(0.2)));
            if drawing.fill_color32() != fill {
                drawing.fill_color = fill.map(|color| color.to_srgba_unmultiplied());
            }
            drawing.font_size = self.font_size;
            drawing.timeframe_visibility = TimeframeVisibility {
                show_on_all: self.visible_all_timeframes,
                visible_timeframes: self.visible_timeframes.iter().cloned().collect(),
            };
            if let Some(template) = &self.tool_settings
                && template.settings.tool_type == drawing.tool_type
                && template.settings.custom_tool == drawing.custom_tool
            {
                let settings = &template.settings;
                drawing.font_weight = settings.font_weight;
                drawing.arrow_start_style = settings.arrow_start_style;
                drawing.arrow_end_style = settings.arrow_end_style;
                drawing.custom_settings = settings.custom_settings.clone();
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
            edits.push(drawing);
        }
        Ok(edits)
    }

    pub(crate) fn validate(&self) -> Result<(), &'static str> {
        if !(self.line_width > 0.0
            && self.line_width <= MAX_STROKE_WIDTH
            && FONT_SIZES.contains(&self.font_size))
        {
            return Err("Line width must be above 0 and at most 100, font size 1 to 256");
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
                    .any(|l| !(0.0..=MAX_PITCHFORK_LEVEL).contains(&l.value))
            }) {
                return Err("Pitchfork multiples must be between 0 and 100");
            }
            if settings.pitchfork.as_ref().is_some_and(|c| {
                c.levels
                    .iter()
                    .any(|l| l.width.is_some_and(|w| !(w > 0.0 && w <= MAX_STROKE_WIDTH)))
            }) {
                return Err("Pitchfork widths must be above 0 and at most 100");
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
                    "Ghost feed average range must be positive and noise nonnegative, both finite",
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Existing drawing settings survive opening and applying the properties, bulk edits over the
    /// selection stay atomic, and every tool's saved settings reach the drawing.
    #[test]
    fn props_round_trip_visibility_fill_fonts_and_tool_settings() {
        use crate::drawings::{Drawing, DrawingToolType, TimeframeVisibility};
        use crate::model::{BarData, LineStyle};
        use crate::widget::Chart;
        let mut chart = Chart::new(BarData::new());
        let mut drawing = Drawing::new(7, DrawingToolType::TextLabel);
        drawing.line_style = LineStyle::Dotted;
        drawing.fill_color = Some([51, 102, 153, 85]);
        drawing.font_size = 24.0;
        drawing.extend_left = true;
        drawing.timeframe_visibility = TimeframeVisibility::specific(&["1h"]);
        drawing.text = Some("Annotation".into());
        drawing.locked = true;
        chart.drawings_mut().drawings.push(drawing);
        let props = DrawingProps::from_drawing(&chart.drawings().drawings[0], &chart);
        assert!(!props.visible_all_timeframes);
        assert_eq!(props.visible_timeframes, ["1h"]);
        assert!(props.fill_enabled);
        assert_eq!(
            props.fill_color.unwrap().to_srgba_unmultiplied(),
            [51, 102, 153, 85]
        );
        assert_eq!(props.font_size, 24.0);
        assert!(matches!(props.line_style, LineStyle::Dotted));
        assert!(props.extend_left);
        assert!(
            DrawingProps {
                font_size: 40.0,
                ..props.clone()
            }
            .apply_to_chart(DrawingId(7), &mut chart)
            .is_err()
        );
        assert_eq!(chart.drawings().drawings[0].font_size, 24.0);
        chart.drawings_mut().edit_drawing(7, |d| d.locked = false);
        // A size the drawings file would refuse cannot be applied.
        assert!(
            DrawingProps {
                font_size: 300.0,
                ..props.clone()
            }
            .apply_to_chart(DrawingId(7), &mut chart)
            .is_err()
        );
        props.apply_to_chart(DrawingId(7), &mut chart).unwrap();
        let mut second = chart.drawings().drawings[0].clone();
        second.id = 8;
        chart.drawings_mut().drawings.push(second);
        chart.drawings_mut().select_drawings([7, 8]);
        let changed = DrawingProps {
            visible_all_timeframes: false,
            visible_timeframes: vec!["4h".into()],
            font_size: 28.0,
            fill_enabled: false,
            ..props.clone()
        };
        changed.apply_to_chart(DrawingId(7), &mut chart).unwrap();
        let saved = &chart.drawings().drawings[0];
        assert!(saved.timeframe_visibility.is_visible_on("4h"));
        assert!(!saved.timeframe_visibility.is_visible_on("1h"));
        assert_eq!(saved.fill_color, None);
        assert_eq!(saved.font_size, 28.0);
        assert_eq!(chart.drawings().drawings[1].font_size, 28.0);
        assert!(chart.undo());
        assert!(
            chart
                .drawings()
                .drawings
                .iter()
                .all(|d| d.font_size == 24.0)
        );
        assert!(chart.redo());

        use crate::data::VolumeProfileConfig;
        use crate::drawings::{
            FibonacciConfig, GannConfig, PitchforkConfig, PositionSize, PositionSizing,
            RegressionTrendConfig,
        };
        for tool in [
            DrawingToolType::FibonacciRetracement,
            DrawingToolType::Pitchfork,
            DrawingToolType::GannBox,
            DrawingToolType::LongPos,
            DrawingToolType::ShortPos,
            DrawingToolType::FixedRangeVolumeProfile,
            DrawingToolType::RegressionTrend,
            DrawingToolType::ParallelChannel,
            DrawingToolType::Rect,
            DrawingToolType::TrendLine,
            DrawingToolType::Ray,
            DrawingToolType::ExtendedLine,
            DrawingToolType::InfoLine,
            DrawingToolType::TrendAngle,
            DrawingToolType::Arrow,
            DrawingToolType::Path,
            DrawingToolType::Polyline,
        ] {
            chart.drawings_mut().drawings.clear();
            chart.drawings_mut().drawings.push(Drawing::new(7, tool));
            let mut peer = Drawing::new(8, DrawingToolType::TextLabel);
            peer.chart_points
                .push(crate::drawings::ChartPoint::new(5.0, 75.0));
            chart.drawings_mut().drawings.push(peer);
            chart.drawings_mut().select_drawings([7, 8]);
            let mut props = DrawingProps::from_drawing(&chart.drawings().drawings[0], &chart);
            let output = egui::Context::default().run_ui(Default::default(), |ui| {
                let mut pickers = [
                    crate::ui_kit::ColorPickerState::new(props.color),
                    crate::ui_kit::ColorPickerState::new(props.color),
                ];
                super::super::tabs::StyleTab::show(ui, &mut props, &mut pickers);
            });
            let labels: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                    _ => None,
                })
                .collect();
            let line = tool.supports_line_ends();
            assert_eq!(labels.contains(&"Line start"), line, "{tool:?}");
            assert_eq!(labels.contains(&"Line end"), line, "{tool:?}");
            let middle = tool.supports_middle_line();
            assert_eq!(labels.contains(&"Middle line"), middle, "{tool:?}");
            output.drop_without_applying_deltas();
            let settings = &mut props.tool_settings.as_mut().unwrap().settings;
            settings.fib_config = Some(FibonacciConfig {
                reverse: true,
                ..Default::default()
            });
            settings.pitchfork = Some(PitchforkConfig {
                background_opacity: 75,
                ..Default::default()
            });
            settings.gann = Some(GannConfig {
                price_levels: vec![0.0, 0.5, 1.0],
                time_levels: vec![0.25, 0.75],
                ..Default::default()
            });
            settings.position_size = Some(PositionSize::RiskPercent {
                account_size: 10_000.0,
                percent: 1.0,
            });
            settings.position_sizing = PositionSizing {
                point_value: 50.0,
                lot_size: 10.0,
                quantity_step: 0.01,
                leverage: 5.0,
            };
            let sizing = settings.position_sizing;
            settings.volume_profile = VolumeProfileConfig::new(48, 0.8);
            settings.regression_trend = RegressionTrendConfig::new(3.0);
            settings.channel_midline = true;
            settings.arrow_start_style = crate::drawings::ArrowStyle::Arrow;
            settings.arrow_end_style = crate::drawings::ArrowStyle::None;
            let expected = format!("{:?}", settings.fib_config);
            props.apply_to_chart(DrawingId(7), &mut chart).unwrap();
            let saved = &chart.drawings().drawings[0];
            assert_eq!(format!("{:?}", saved.fib_config), expected);
            assert_eq!(saved.pitchfork.as_ref().unwrap().background_opacity, 75);
            assert_eq!(saved.gann.as_ref().unwrap().time_levels, [0.25, 0.75]);
            assert_eq!(
                saved.position_size.unwrap().quantity(100.0, 98.0),
                Some(50.0)
            );
            assert_eq!(saved.volume_profile.unwrap().rows(), 48);
            assert_eq!(saved.position_sizing, sizing);
            assert_eq!(saved.regression_trend.unwrap().deviations(), 3.0);
            assert!(saved.channel_midline);
            assert_eq!(saved.arrow_start_style, crate::drawings::ArrowStyle::Arrow);
            assert_eq!(saved.arrow_end_style, crate::drawings::ArrowStyle::None);
            assert!(chart.drawings().drawings[1].fib_config.is_none());
            assert_eq!(chart.drawings().drawings[1].chart_points[0].price, 75.0);
            assert!(chart.undo());
            assert!(chart.drawings().drawings[0].fib_config.is_none());
            assert!(chart.redo());
            chart.drawings_mut().edit_drawing(7, |d| d.locked = true);
            props.font_size = 18.0;
            props.apply_to_chart(DrawingId(7), &mut chart).unwrap();
            assert_eq!(chart.drawings().drawings[0].font_size, 12.0);
            assert_eq!(chart.drawings().drawings[1].font_size, 18.0);
            // A malformed ratio cannot partially change common style or another selected tool.
            props
                .tool_settings
                .as_mut()
                .unwrap()
                .settings
                .fib_config
                .as_mut()
                .unwrap()
                .levels[0]
                .value = f32::NAN;
            props.line_width = 9.0;
            assert!(props.apply_to_chart(DrawingId(7), &mut chart).is_err());
            assert_eq!(chart.drawings().drawings[0].stroke_width, 2.0);
            assert_eq!(chart.drawings().drawings[1].stroke_width, 2.0);
        }
    }
}
