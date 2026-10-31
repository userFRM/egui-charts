//! Bands between study outputs or fixed levels, and shapes stored per shape instead of per bar.

use egui::Color32;

/// One boundary of a study band, in the study's value scale.
#[derive(Clone, Copy, Debug)]
pub enum FillBoundary {
    /// A zero-based output line, including its offset and visibility.
    Plot(usize),
    /// A constant level across the pane.
    Level(f64),
}

/// A band between two plots or levels.
#[derive(Clone, Copy, Debug)]
pub struct StudyFill {
    /// First boundary.
    pub first: FillBoundary,
    /// Second boundary.
    pub second: FillBoundary,
    /// Colour when the first value is at least the second.
    pub color: Color32,
    /// Optional colour when the first value is below the second.
    pub inverted_color: Option<Color32>,
}

/// One OHLCV candle spanning an exclusive range of source bar indices.
#[derive(Clone, Debug)]
pub struct StudyCandle {
    /// Loaded source bars covered by this candle; gaps do not invent positions.
    pub bars: std::ops::Range<usize>,
    /// Aggregated period, stamped with its opening calendar label.
    pub bar: crate::model::Bar,
    /// Output whose colour, stroke width and visibility apply to this candle.
    pub plot: usize,
}

/// A price band between two source bars, stored once instead of as a value on every bar.
#[derive(Clone, Debug, PartialEq)]
pub struct StudyZone {
    /// Source bars the band spans, from the centre of the first to the centre of the last, or
    /// from edge to edge when its study's zones cover whole bars.
    pub bars: std::ops::Range<usize>,
    /// Upper price.
    pub top: f64,
    /// Lower price.
    pub bottom: f64,
    /// Output whose colour, stroke width and visibility apply to this zone: its edges in that
    /// colour and its band at 15% opacity.
    pub plot: usize,
}

/// Where the text of a [`StudyLabel`] sits relative to its point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum LabelPlacement {
    /// Above the point, starting at it and extending right.
    AboveRight,
    /// Above the point, ending at it and extending left.
    AboveLeft,
    /// Centred above the point.
    Above,
    /// Centred below the point.
    Below,
}

/// Text beside a study value, stored once per label instead of per bar.
///
/// The text is written when the study calculates, so drawing a frame formats nothing.
#[derive(Clone, Debug, PartialEq)]
pub struct StudyLabel {
    /// Source bar the text is attached to; may lie past the last bar, in future whitespace.
    pub bar: usize,
    /// Price the text is attached to, in the study's value scale.
    pub price: f64,
    /// The text; a line break starts a new line.
    pub text: String,
    /// Where the text sits relative to `bar` and `price`.
    pub placement: LabelPlacement,
    /// Output whose colour and visibility apply to the text. The text is drawn opaque.
    pub plot: usize,
}

#[cfg(feature = "drawings")]
pub(crate) fn drawing_prices(
    study: &dyn super::Indicator,
    bars: std::ops::Range<usize>,
) -> impl Iterator<Item = f64> + '_ {
    study
        .drawings()
        .iter()
        .filter_map(move |drawing| {
            if !study.plot_style(0).visible || !drawing.visible || bars.is_empty() {
                return None;
            }
            let levels = drawing.fib_levels();
            let extent = if levels.is_some() {
                &drawing.chart_points[..2]
            } else {
                &drawing.chart_points
            };
            let first = extent.iter().map(|p| p.bar_idx).reduce(f32::min)?;
            let last = extent.iter().map(|p| p.bar_idx).reduce(f32::max)?;
            let left = drawing.extend_left
                || levels
                    .as_ref()
                    .is_some_and(|(c, _)| c.extend_left || c.show_background);
            let right = drawing.extend_right
                || drawing.tool_type == crate::drawings::DrawingToolType::Pitchfork
                || levels
                    .as_ref()
                    .is_some_and(|(c, _)| c.extend_right || c.show_background);
            if (!left && first >= bars.end as f32) || (!right && last < bars.start as f32) {
                return None;
            }
            Some(
                drawing
                    .chart_points
                    .iter()
                    .map(|p| p.price)
                    .chain(levels.into_iter().flat_map(|(_, prices)| prices)),
            )
        })
        .flatten()
        .filter(|price| price.is_finite())
}
