//! Actions from the drawing properties dialog.

use super::tabs::CoordinatesState;
use crate::drawings::domain::GANN_SQUARE_UNITS;
use crate::drawings::validation::{MAX_PITCHFORK_LEVEL, MAX_STROKE_WIDTH};
use crate::drawings::{Drawing, DrawingTemplate, DrawingToolType, TimeframeVisibility};
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
    /// Instrument tick size for position level inputs; absent disables editing by ticks.
    pub tick_size: Option<f64>,
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
    /// The intervals the drawing shows on
    pub timeframe_visibility: TimeframeVisibility,
}

impl Default for DrawingProps {
    fn default() -> Self {
        Self {
            symbol: None,
            tick_size: None,
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
            timeframe_visibility: TimeframeVisibility::all(),
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
    Apply(DrawingId, Box<DrawingProps>),
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
            tick_size: chart
                .instrument()
                .and_then(|instrument| instrument.tick_size),
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
            timeframe_visibility: drawing.timeframe_visibility.clone(),
            label: drawing.text.clone().unwrap_or_default(),
            ..Self::default()
        }
    }

    /// Validate and apply one undoable edit to the unlocked selection. A value that differs from
    /// drawing `id`'s, which these properties were read from, was edited, and only edited values
    /// change the drawings: text only those that show text, tool inputs only drawings of the same
    /// tool, coordinates only `id`. Errors leave all live drawings unchanged.
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
    /// Values compare with drawing `id` as `chart` holds it, so a preview restores its drawings
    /// before calling this.
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
        let adds_position_stop = matches!(
            target.tool_type,
            crate::drawings::DrawingToolType::LongPos | crate::drawings::DrawingToolType::ShortPos
        ) && target.chart_points.len() == 2
            && points.len() == 3;
        if !points.is_empty() && points.len() != target.chart_points.len() && !adds_position_stop {
            return Err("Drawing anchors changed; reopen its properties");
        }
        let selected = chart.drawings().is_selected(id.0);
        let fill = self
            .fill_enabled
            .then(|| self.fill_color.unwrap_or(self.color.gamma_multiply(0.2)));
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
            // The dialog opened with the target's values: one that differs from them was edited,
            // and only edited values are written, so each drawing keeps the rest of its own.
            if self.color != target.color32() {
                drawing.color = self.color.to_srgba_unmultiplied();
            }
            if self.line_width != target.stroke_width {
                drawing.stroke_width = self.line_width;
            }
            if self.line_style != target.line_style {
                drawing.line_style = self.line_style;
            }
            if self.extend_left != target.extend_left {
                drawing.extend_left = self.extend_left;
            }
            if self.extend_right != target.extend_right {
                drawing.extend_right = self.extend_right;
            }
            // A tool that shows no text keeps it: it holds a sticker, an image or an icon.
            if self.label != target.text.as_deref().unwrap_or_default()
                && (drawing.id == id.0 || drawing.tool_type.supports_text())
            {
                drawing.text = (!self.label.is_empty()).then(|| self.label.clone());
            }
            if fill != target.fill_color32() && drawing.fill_color32() != fill {
                drawing.fill_color = fill.map(|color| color.to_srgba_unmultiplied());
            }
            if self.font_size != target.font_size {
                drawing.font_size = self.font_size;
            }
            if self.timeframe_visibility != target.timeframe_visibility {
                drawing.timeframe_visibility = self.timeframe_visibility.clone();
            }
            if let Some(template) = &self.tool_settings
                && template.settings.tool_type == drawing.tool_type
                && template.settings.custom_tool == drawing.custom_tool
            {
                let settings = &template.settings;
                // A tool setting that differs from the target's was edited.
                macro_rules! edited {
                    ($($field:ident),+ $(,)?) => {
                        $(if settings.$field != target.$field {
                            drawing.$field.clone_from(&settings.$field);
                        })+
                    };
                }
                edited!(
                    font_weight,
                    italic,
                    text_color,
                    background,
                    border,
                    border_color,
                    arrow_start_style,
                    arrow_end_style,
                    custom_settings,
                    fib_config,
                    pitchfork,
                    gann,
                    ghost_feed,
                    position_size,
                    position_sizing,
                    position_target_color,
                    position_stop_color,
                    volume_profile,
                    regression_trend,
                    channel_midline,
                    elliott_degree,
                );
            }
            if drawing.id == id.0
                && !points.is_empty()
                && (drawing.chart_points.len() != points.len()
                    || drawing
                        .chart_points
                        .iter()
                        .zip(&points)
                        .any(|(a, b)| a != b || a.time_anchor != b.time_anchor))
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
            if settings
                .fib_config
                .as_ref()
                .is_some_and(|c| !(c.level_width > 0.0 && c.level_width <= MAX_STROKE_WIDTH))
            {
                return Err("Fibonacci widths must be above 0 and at most 100");
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
            if let Some(c) = settings.gann.as_ref() {
                let square = matches!(
                    settings.tool_type,
                    DrawingToolType::GannSquare | DrawingToolType::GannFixed
                );
                let max = if square { GANN_SQUARE_UNITS } else { 1.0 };
                if c.price_levels
                    .iter()
                    .chain(&c.time_levels)
                    .any(|l| !(0.0..=max).contains(&l.value))
                {
                    return Err(if square {
                        "Gann square levels must be between 0 and 5"
                    } else {
                        "Gann levels must be between 0 and 1"
                    });
                }
                if c.fan_ratios
                    .iter()
                    .chain(&c.arc_ratios)
                    .any(|l| !(l.value.is_finite() && l.value > 0.0))
                {
                    return Err("Gann fan and arc ratios must be positive and finite");
                }
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
        assert_eq!(
            props.timeframe_visibility,
            TimeframeVisibility::specific(&["1h"])
        );
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
        let mut visibility = TimeframeVisibility::default();
        visibility.set_range(crate::model::TimeframeUnit::Hour, Some(2..=4));
        let changed = DrawingProps {
            timeframe_visibility: visibility,
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
            FibonacciConfig, GannConfig, GannLevel, PitchforkConfig, PositionSize, PositionSizing,
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
            // A boxed label, so that the settings below would show on it if they leaked.
            (peer.background, peer.border, peer.border_color) = (true, true, None);
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
                price_levels: [0.0, 0.5, 1.0]
                    .map(|v| GannLevel::new(v, [128, 128, 128, 255]))
                    .to_vec(),
                time_levels: [0.25, 0.75]
                    .map(|v| GannLevel::new(v, [128, 128, 128, 255]))
                    .to_vec(),
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
            settings.position_target_color = Some([40, 100, 220, 255]);
            settings.position_stop_color = Some([220, 140, 40, 255]);
            settings.volume_profile = VolumeProfileConfig::new(48, 0.8);
            settings.regression_trend = RegressionTrendConfig::new(3.0);
            settings.channel_midline = true;
            settings.elliott_degree = crate::drawings::ElliottDegree::Minute;
            settings.italic = true;
            settings.text_color = Some([1, 2, 3, 255]);
            settings.background = false;
            settings.border = false;
            settings.border_color = Some([4, 5, 6, 255]);
            settings.arrow_start_style = crate::drawings::ArrowStyle::Arrow;
            settings.arrow_end_style = crate::drawings::ArrowStyle::None;
            let expected = format!("{:?}", settings.fib_config);
            props.apply_to_chart(DrawingId(7), &mut chart).unwrap();
            let saved = &chart.drawings().drawings[0];
            assert_eq!(format!("{:?}", saved.fib_config), expected);
            assert_eq!(saved.pitchfork.as_ref().unwrap().background_opacity, 75);
            assert_eq!(
                saved
                    .gann
                    .as_ref()
                    .unwrap()
                    .time_levels
                    .iter()
                    .map(|l| l.value)
                    .collect::<Vec<_>>(),
                [0.25, 0.75]
            );
            assert_eq!(
                saved.position_size.unwrap().quantity(100.0, 98.0),
                Some(50.0)
            );
            assert_eq!(saved.volume_profile.unwrap().rows(), 48);
            assert_eq!(saved.position_sizing, sizing);
            assert_eq!(saved.position_target_color, Some([40, 100, 220, 255]));
            assert_eq!(saved.position_stop_color, Some([220, 140, 40, 255]));
            assert_eq!(saved.regression_trend.unwrap().deviations(), 3.0);
            assert!(saved.channel_midline);
            assert_eq!(saved.elliott_degree, crate::drawings::ElliottDegree::Minute);
            assert!(saved.italic && !saved.background && !saved.border);
            assert_eq!(saved.text_color, Some([1, 2, 3, 255]));
            assert_eq!(saved.border_color, Some([4, 5, 6, 255]));
            assert_eq!(saved.arrow_start_style, crate::drawings::ArrowStyle::Arrow);
            assert_eq!(saved.arrow_end_style, crate::drawings::ArrowStyle::None);
            assert!(chart.drawings().drawings[1].fib_config.is_none());
            let peer = &chart.drawings().drawings[1];
            assert!(!peer.italic && peer.background && peer.border);
            assert_eq!((peer.text_color, peer.border_color), (None, None));
            assert_eq!(chart.drawings().drawings[1].chart_points[0].price, 75.0);
            assert!(chart.undo());
            assert_eq!(
                format!("{:?}", chart.drawings().drawings[0].fib_config),
                format!("{:?}", Drawing::new(7, tool).fib_config)
            );
            assert!(chart.redo());
            chart.drawings_mut().edit_drawing(7, |d| d.locked = true);
            props.font_size = 18.0;
            props.apply_to_chart(DrawingId(7), &mut chart).unwrap();
            assert_eq!(chart.drawings().drawings[0].font_size, 14.0);
            assert_eq!(chart.drawings().drawings[1].font_size, 18.0);
            // Malformed ratios or widths cannot partly change another selected drawing.
            for (ratio, width) in [(f32::NAN, 2.0), (1.0, 0.0), (1.0, 100.01), (1.0, f32::NAN)] {
                let config = props
                    .tool_settings
                    .as_mut()
                    .unwrap()
                    .settings
                    .fib_config
                    .as_mut()
                    .unwrap();
                config.levels[0].value = ratio;
                config.level_width = width;
                props.line_width = 9.0;
                assert!(props.apply_to_chart(DrawingId(7), &mut chart).is_err());
                assert_eq!(
                    chart.drawings().drawings[0].stroke_width,
                    Drawing::new(7, tool).stroke_width
                );
                assert_eq!(
                    chart.drawings().drawings[1].stroke_width,
                    Drawing::new(8, DrawingToolType::TextLabel).stroke_width
                );
            }
        }
    }

    /// Editing another property keeps each count's degree; editing the degree reaches matching tools.
    #[test]
    fn selection_preserves_wave_degrees_until_the_degree_is_edited() {
        use crate::drawings::{DrawingToolType::*, ElliottDegree::*};
        let mut chart = Chart::new(crate::model::BarData::new());
        chart.drawings_mut().drawings = [
            (ElliottImpulse, Minor),
            (ElliottImpulse, Minute),
            (ElliottCorrection, Cycle),
        ]
        .into_iter()
        .enumerate()
        .map(|(id, (tool, degree))| {
            let mut drawing = Drawing::new(id, tool);
            drawing.elliott_degree = degree;
            drawing
        })
        .collect();
        chart.drawings_mut().select_drawings([0, 1, 2]);
        let mut props = DrawingProps::from_drawing(&chart.drawings().drawings[0], &chart);
        // A colour edit keeps the degrees of the other selected counts.
        props.color = Color32::GREEN;
        props.apply_to_chart(DrawingId(0), &mut chart).unwrap();
        assert_eq!(
            chart
                .drawings()
                .drawings
                .iter()
                .map(|d| d.elliott_degree)
                .collect::<Vec<_>>(),
            [Minor, Minute, Cycle]
        );
        // A degree edit reaches the other impulse, leaving the correction's degree alone.
        props
            .tool_settings
            .as_mut()
            .unwrap()
            .settings
            .elliott_degree = Minuette;
        props.apply_to_chart(DrawingId(0), &mut chart).unwrap();
        assert_eq!(
            chart
                .drawings()
                .drawings
                .iter()
                .map(|d| d.elliott_degree)
                .collect::<Vec<_>>(),
            [Minuette, Minuette, Cycle]
        );
    }

    /// Confirming the dialog of one drawing in a selection changes the others only where it was
    /// edited: each keeps its own text, width, extension, visibility and tool settings, and an
    /// edited text reaches only the drawings that show text.
    #[test]
    fn selection_edits_change_only_the_edited_values() {
        use crate::drawings::FontWeight;
        let mut alpha = Drawing::new(1, DrawingToolType::TextLabel);
        alpha.text = Some("Alpha".into());
        let mut beta = Drawing::new(2, DrawingToolType::TextLabel);
        beta.text = Some("Beta".into());
        beta.color = [0, 128, 0, 255];
        beta.font_weight = FontWeight::Bold;
        beta.italic = true;
        let mut line = Drawing::new(3, DrawingToolType::TrendLine);
        line.text = Some("Breakout".into());
        line.stroke_width = 3.0;
        line.extend_right = true;
        line.timeframe_visibility = TimeframeVisibility::specific(&["1h"]);
        // A sticker keeps its kind in its text, which it does not show.
        let sticker = Drawing::new(4, DrawingToolType::Sticker);
        let drawings = vec![alpha, beta, line, sticker];
        let mut chart = Chart::new(crate::model::BarData::new());
        chart.drawings_mut().drawings = drawings.clone();
        chart.drawings_mut().select_drawings([1, 2, 3, 4]);
        let opened = DrawingProps::from_drawing(&drawings[0], &chart);
        // Each case: the edit made in the dialog, then what it changes on each drawing.
        type Case = (&'static str, fn(&mut DrawingProps), fn(&mut Drawing));
        let cases: [Case; 4] = [
            ("nothing edited", |_| {}, |_| {}),
            (
                "colour",
                |props| props.color = Color32::BLUE,
                |d| d.color = [0, 0, 255, 255],
            ),
            (
                "text",
                |props| props.label = "Gamma".into(),
                |d| {
                    if d.tool_type != DrawingToolType::Sticker {
                        d.text = Some("Gamma".into());
                    }
                },
            ),
            (
                "text cleared",
                |props| props.label.clear(),
                |d| {
                    if d.tool_type != DrawingToolType::Sticker {
                        d.text = None;
                    }
                },
            ),
        ];
        for (case, edit, change) in cases {
            let mut props = opened.clone();
            edit(&mut props);
            let edits = props.edited_drawings(DrawingId(1), &chart).unwrap();
            let mut expected = drawings.clone();
            expected.iter_mut().for_each(change);
            assert_eq!(format!("{edits:?}"), format!("{expected:?}"), "{case}");
        }
    }
}
