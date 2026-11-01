//! Bars pattern rendering

use crate::drawings::domain::{BarsPatternConfig, BarsPatternMode, ChartPoint, Drawing};
use crate::drawings::rendering::{ChartView, bars_between};
use crate::model::Bar;
use crate::styles::stroke;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Stroke, vec2};

impl Drawing {
    /// Renders Bars Pattern: a copy of the bars between the first two anchors, drawn from the
    /// third anchor's bar and moved by its price distance from the first anchor, in the
    /// drawing's colour, width and line style. Without a third anchor, as while it is placed,
    /// the copy lies over its source.
    pub(crate) fn render_bars_pattern(&self, painter: &egui::Painter, view: &ChartView) {
        let &[from, to, ..] = self.chart_points.as_slice() else {
            return;
        };
        // Where placement puts the copy: on the left source bar, at the first anchor's price.
        let at = self
            .chart_points
            .get(2)
            .copied()
            .unwrap_or_else(|| ChartPoint::new(from.bar_idx.min(to.bar_idx), from.price));
        let config = self.bars_pattern.unwrap_or_default();
        if let Some(copy) = projected_bars(view.bars, [from, to, at], config) {
            let stroke = self.stroke();
            let tick = ((view.bar_to_x)(1.0) - (view.bar_to_x)(0.0)).abs() * 0.3;
            // A close with no line to a neighbour, such as the only bar, is a flat tick.
            let lone = |last: Option<(Pos2, bool)>| {
                if let Some((point, false)) = last {
                    painter
                        .line_segment([point - vec2(tick, 0.0), point + vec2(tick, 0.0)], stroke);
                }
            };
            // The previous close of the line, and whether a segment reaches it.
            let mut last: Option<(Pos2, bool)> = None;
            for (index, prices) in copy {
                // Whitespace bars, and prices past the range of f64, leave a gap.
                if !prices.iter().all(|price| price.is_finite()) {
                    lone(last.take());
                    continue;
                }
                let [open, high, low, close] = prices.map(|price| (view.price_to_y)(price));
                let x = (view.bar_to_x)(index);
                match config.mode {
                    BarsPatternMode::Bars => {
                        painter.line_segment([Pos2::new(x, high), Pos2::new(x, low)], stroke);
                        painter
                            .line_segment([Pos2::new(x - tick, open), Pos2::new(x, open)], stroke);
                        painter.line_segment(
                            [Pos2::new(x, close), Pos2::new(x + tick, close)],
                            stroke,
                        );
                    }
                    BarsPatternMode::Line => {
                        let point = Pos2::new(x, close);
                        if let Some((previous, _)) = last {
                            painter.line_segment([previous, point], stroke);
                        }
                        last = Some((point, last.is_some()));
                    }
                }
            }
            lone(last);
        }
        // While it is placed, the anchors show where the source starts and ends.
        if !self.completed && self.points.len() == 2 {
            for &point in &self.points {
                painter.circle_filled(point, DESIGN_TOKENS.rounding.lg, self.color32());
                painter.circle_stroke(
                    point,
                    DESIGN_TOKENS.rounding.lg,
                    Stroke::new(stroke::MEDIUM, Color32::WHITE),
                );
            }
        }
    }
}

/// The bars between the first two anchors as the copy draws them, in source order: the bar
/// index each is drawn at and its open, high, low and close. Source bar `j` is drawn at
/// `j + round(at) - round(min(from, to))`, or mirrored about the middle of the anchors' bars,
/// and moved by `at.price - from.price`. `None` when no bar lies between the anchors.
fn projected_bars(
    bars: &[Bar],
    [from, to, at]: [ChartPoint; 3],
    config: BarsPatternConfig,
) -> Option<impl Iterator<Item = (f32, [f64; 4])> + '_> {
    let (first, source) = bars_between(bars, from.bar_idx, to.bar_idx)?;
    let (left, right) = (
        from.bar_idx.min(to.bar_idx).round(),
        from.bar_idx.max(to.bar_idx).round(),
    );
    let offset = at.bar_idx.round() - left;
    let shift = at.price - from.price;
    // Twice the middle of the source bars' range, about which a flip turns each price.
    let flip = config.flipped.then(|| {
        let (low, high) = source
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(l, h), bar| {
                (l.min(bar.low), h.max(bar.high))
            });
        low + high
    });
    Some(source.iter().enumerate().map(move |(i, bar)| {
        let index = (first + i) as f32;
        let index = offset
            + if config.mirrored {
                left + right - index
            } else {
                index
            };
        let (mut open, mut close) = (bar.open, bar.close);
        if config.mirrored {
            (open, close) = (close, open);
        }
        let (mut high, mut low) = (bar.high, bar.low);
        if let Some(twice) = flip {
            (open, high, low, close) = (twice - open, twice - low, twice - high, twice - close);
        }
        (
            index,
            [open + shift, high + shift, low + shift, close + shift],
        )
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::rendering::sample_bars;

    /// Prices and indices worked out by hand: bars 1..=3 shift by +2; a flip uses 10 + 14 = 24.
    #[test]
    fn projected_bars_copy_source_bars() {
        let anchors = [
            ChartPoint::new(0.6, 12.0),
            ChartPoint::new(3.4, 13.0),
            ChartPoint::new(8.4, 14.0),
        ];
        let cases = [
            (
                anchors,
                false,
                false,
                vec![
                    (8., [13., 15., 12., 14.]),
                    (9., [14., 14.5, 13., 13.]),
                    (10., [13.5, 16., 13., 15.5]),
                ],
            ),
            (
                anchors,
                true,
                false,
                vec![
                    (10., [14., 15., 12., 13.]),
                    (9., [13., 14.5, 13., 14.]),
                    (8., [15.5, 16., 13., 13.5]),
                ],
            ),
            (
                anchors,
                false,
                true,
                vec![
                    (8., [15., 16., 13., 14.]),
                    (9., [14., 15., 13.5, 15.]),
                    (10., [14.5, 15., 12., 12.5]),
                ],
            ),
            (
                anchors,
                true,
                true,
                vec![
                    (10., [14., 16., 13., 15.]),
                    (9., [15., 15., 13.5, 14.]),
                    (8., [12.5, 15., 12., 14.5]),
                ],
            ),
            (
                [
                    ChartPoint::new(3.4, 13.),
                    ChartPoint::new(0.6, 12.),
                    ChartPoint::new(0.6, 13.),
                ],
                false,
                false,
                vec![
                    (1., [11., 13., 10., 12.]),
                    (2., [12., 12.5, 11., 11.]),
                    (3., [11.5, 14., 11., 13.5]),
                ],
            ),
            (
                [
                    ChartPoint::new(-2., 10.),
                    ChartPoint::new(1., 11.),
                    ChartPoint::new(5., 10.),
                ],
                false,
                false,
                vec![(7., [10., 12., 9., 11.]), (8., [11., 13., 10., 12.])],
            ),
        ];
        let bars = sample_bars();
        for (row, (anchors, mirrored, flipped, expected)) in cases.into_iter().enumerate() {
            let config = BarsPatternConfig {
                mirrored,
                flipped,
                ..Default::default()
            };
            let copy: Vec<_> = projected_bars(&bars, anchors, config).unwrap().collect();
            assert_eq!(copy, expected, "row {}", row + 1);
        }
    }

    /// A finished copy paints only opaque strokes; close lines break at whitespace and keep
    /// isolated closes visible as flat ticks. Overflowing transformations paint nothing.
    #[test]
    fn copied_bars_and_close_lines_paint_only_finite_strokes() {
        use crate::drawings::DrawingToolType;
        let bars = sample_bars();
        let mut gapped = bars[..5].to_vec();
        gapped[1] = Bar::whitespace(gapped[1].time);
        gapped[3] = Bar::whitespace(gapped[3].time);
        for (mode, source, end, price, expected) in [
            (
                BarsPatternMode::Bars,
                &bars[..],
                2.,
                12.,
                vec![
                    [egui::pos2(80., 120.), egui::pos2(80., 90.)],
                    [egui::pos2(77., 100.), egui::pos2(80., 100.)],
                    [egui::pos2(80., 110.), egui::pos2(83., 110.)],
                    [egui::pos2(90., 130.), egui::pos2(90., 100.)],
                    [egui::pos2(87., 110.), egui::pos2(90., 110.)],
                    [egui::pos2(90., 120.), egui::pos2(93., 120.)],
                    [egui::pos2(100., 125.), egui::pos2(100., 110.)],
                    [egui::pos2(97., 120.), egui::pos2(100., 120.)],
                    [egui::pos2(100., 110.), egui::pos2(103., 110.)],
                ],
            ),
            (
                BarsPatternMode::Line,
                &bars[..],
                2.,
                12.,
                vec![
                    [egui::pos2(80., 110.), egui::pos2(90., 120.)],
                    [egui::pos2(90., 120.), egui::pos2(100., 110.)],
                ],
            ),
            (
                BarsPatternMode::Line,
                &bars[..],
                0.,
                12.,
                vec![[egui::pos2(77., 110.), egui::pos2(83., 110.)]],
            ),
            (
                BarsPatternMode::Line,
                &gapped[..],
                4.,
                12.,
                vec![
                    [egui::pos2(77., 110.), egui::pos2(83., 110.)],
                    [egui::pos2(97., 110.), egui::pos2(103., 110.)],
                    [egui::pos2(117., 140.), egui::pos2(123., 140.)],
                ],
            ),
            (BarsPatternMode::Line, &bars[..0], 2., 12., vec![]),
            (BarsPatternMode::Bars, &bars[..], 2., f64::MAX, vec![]),
        ] {
            let mut drawing = Drawing::new(0, DrawingToolType::BarsPattern);
            drawing.completed = true;
            drawing.color = [30, 70, 210, 255];
            drawing.stroke_width = 4.0;
            drawing.bars_pattern = Some(BarsPatternConfig {
                mode,
                ..Default::default()
            });
            drawing.chart_points = vec![
                ChartPoint::new(0., 12.),
                ChartPoint::new(end, 13.),
                ChartPoint::new(8., price),
            ];
            if price == f64::MAX {
                drawing.chart_points[0].price = -f64::MAX;
            }
            drawing.update_screen_coords(|bar| bar * 10., |price| price as f32 * 10.);
            let ctx = egui::Context::default();
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                drawing.render_bars_pattern(
                    ui.painter(),
                    &ChartView {
                        bars: source,
                        revision: 0,
                        price_formatter: None,
                        instrument: None,
                        bar_to_x: &|bar| bar * 10.,
                        price_to_y: &|price| price as f32 * 10.,
                        y_to_price: &|y| f64::from(y / 10.),
                        time_axis: true,
                        executions: None,
                    },
                );
            });
            let shapes = output.shapes.clone();
            output.drop_without_applying_deltas();
            let segments: Vec<_> = shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Noop => None,
                    egui::Shape::LineSegment { points, stroke } => {
                        assert_eq!(*stroke, drawing.stroke());
                        Some(*points)
                    }
                    shape => panic!("copy painted a decoration: {shape:?}"),
                })
                .collect();
            assert_eq!(segments, expected, "{mode:?}, end={end}, price={price}");
        }
    }
}
