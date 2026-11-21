//! Pane study for a metric published by the host's data source.

use super::{Indicator, IndicatorValue, IndicatorValues, StudyCandle, StudyData, StudyDataRequest};

use crate::data::{MetricColors, MetricPlot, MetricSeries};
use crate::model::Bar;
use crate::scales::StudyFormat;
use crate::studies::palette;
use egui::Color32;

/// A host metric drawn in an independent pane. Each chart bar shows the latest point
/// published before the bar closes, and the current bar the latest point published so far
/// within it; a point is held until the next one, and bars before the first point have no value.
/// Its output can be selected as another study's [`super::StudySource::Study`].
///
/// Connected charts request its history and live updates through the datafeed. Hosts
/// supplying study data directly convert [`crate::data::MetricPoint`] into [`Bar`]
/// and pass them to `Chart::set_study_data` with this study's request.
#[derive(Debug, Clone)]
pub struct MetricStudy {
    metric: MetricSeries,
    /// Host symbol whose metric is shown, or empty for the chart's.
    symbol: String,
    values: IndicatorValues,
    candles: Vec<StudyCandle>,
    color: Color32,
    /// Bars drawn in the alternate colour of [`MetricSeries::colors`].
    alternate: Vec<bool>,
    alternate_color: Option<Color32>,
    visible: bool,
    /// The chart's bar calendar, which dates each bar's close.
    clock: crate::model::BarClock,
}

impl MetricStudy {
    /// Display this metric of the chart's current instrument.
    pub fn new(metric: MetricSeries) -> Self {
        let (color, alternate_color) = match metric.colors {
            None => (palette::ACCENT, None),
            Some(MetricColors::Single(color)) => (color, None),
            Some(MetricColors::Change { rising, falling }) => (rising, Some(falling)),
            Some(MetricColors::Sign { positive, negative }) => (positive, Some(negative)),
        };
        Self {
            metric,
            symbol: String::new(),
            values: IndicatorValues::new(),
            candles: Vec::new(),
            color,
            alternate: Vec::new(),
            alternate_color,
            visible: true,
            clock: crate::model::BarClock::exact(),
        }
    }
}

impl Indicator for MetricStudy {
    fn name(&self) -> &str {
        &self.metric.name
    }
    fn metric(&self) -> Option<&MetricSeries> {
        Some(&self.metric)
    }
    fn desc(&self) -> &str {
        &self.metric.unit
    }
    fn is_overlay(&self) -> bool {
        false
    }
    fn value_format(&self) -> StudyFormat {
        self.metric.format
    }
    fn line_names(&self) -> Vec<String> {
        vec![if self.metric.unit.is_empty() {
            self.metric.name.clone()
        } else {
            format!("{} ({})", self.metric.name, self.metric.unit)
        }]
    }
    fn plot_style(&self, _: usize) -> super::PlotStyle {
        super::PlotStyle {
            color: self.color,
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: match self.metric.plot {
                MetricPlot::Line | MetricPlot::Candles => super::PlotType::Line,
                MetricPlot::StepLine => super::PlotType::StepLine,
                MetricPlot::Columns => super::PlotType::Columns,
                MetricPlot::Histogram => super::PlotType::Histogram,
                MetricPlot::Area => super::PlotType::Area,
            },
            visible: true,
        }
    }
    fn data_requests(&self) -> Vec<StudyDataRequest> {
        vec![StudyDataRequest {
            symbol: self.symbol.clone(),
            timeframe: None,
            metric: Some(self.metric.id.clone()),
        }]
    }
    fn inputs(&self) -> Vec<super::StudyInput> {
        vec![
            super::StudyInput::new(
                "symbol",
                "Symbol",
                super::InputValue::Symbol(self.symbol.clone()),
                super::InputKind::Symbol,
            )
            .with_default(super::InputValue::Symbol(String::new())),
        ]
    }
    fn set_input(&mut self, id: &str, value: super::InputValue) -> Result<(), super::StudyError> {
        if id != "symbol" {
            return Err(super::StudyError::UnknownInput(id.into()));
        }
        if !self.inputs()[0].accepts(&value) {
            return Err(super::StudyError::invalid_input(id));
        }
        if let super::InputValue::Symbol(symbol) = value {
            self.symbol = symbol;
        }
        self.values.clear();
        self.candles.clear();
        self.alternate.clear();
        Ok(())
    }
    fn calculate(&mut self, data: &[Bar]) {
        self.update_data(data, 0, &[]);
    }
    fn update_data(&mut self, data: &[Bar], _: usize, sources: &[StudyData<'_>]) {
        let request = &self.data_requests()[0];
        let points = sources
            .iter()
            .find(|source| &source.request == request)
            .map_or(&[][..], |source| source.bars);
        let mut values = IndicatorValues::new();
        self.alternate.clear();
        self.candles.clear();
        let mut previous = None;
        let mut falling = false;
        for (at, bar) in data.iter().enumerate() {
            // The clock bounds the live bar too; a point at its close waits for the next bar.
            let open = self.clock.bar_open(bar.time);
            let close = self
                .clock
                .next_open(open)
                .or_else(|| data.get(at + 1).map(|next| next.time));
            let end = close.map_or(points.len(), |close| {
                points.partition_point(|point| point.time < close)
            });
            let value = end
                .checked_sub(1)
                .map(|last| points[last].close)
                .filter(|v| v.is_finite());
            let mut reference = previous;
            if self.metric.plot == MetricPlot::Candles
                && let Some(value) = value
            {
                let start = points.partition_point(|point| point.time < open).min(end);
                let inside = &points[start..end];
                let candle = if let Some(first) = inside.first() {
                    Bar::new(
                        bar.time,
                        first.open,
                        inside
                            .iter()
                            .map(|point| point.high)
                            .fold(f64::NEG_INFINITY, f64::max),
                        inside
                            .iter()
                            .map(|point| point.low)
                            .fold(f64::INFINITY, f64::min),
                        value,
                        0.0,
                    )
                } else {
                    Bar::new(bar.time, value, value, value, value, 0.0)
                };
                reference = Some(candle.open);
                if [candle.open, candle.high, candle.low]
                    .iter()
                    .all(|v| v.is_finite())
                {
                    self.candles.push(StudyCandle {
                        bars: at..at + 1,
                        bar: candle,
                        plot: 0,
                    });
                }
            }
            let alternate = value.is_some_and(|value| match self.metric.colors {
                Some(MetricColors::Change { .. }) => {
                    if let Some(reference) = reference {
                        if value < reference {
                            falling = true;
                        } else if value > reference {
                            falling = false;
                        }
                    }
                    falling
                }
                Some(MetricColors::Sign { .. }) => value < 0.0,
                _ => false,
            });
            self.alternate.push(alternate);
            values.push(value.map_or(IndicatorValue::None, IndicatorValue::Single));
            if value.is_some() {
                previous = value;
            }
        }
        self.values = values;
    }

    fn set_bar_clock(&mut self, clock: &crate::model::BarClock) -> bool {
        let changed = self.clock != *clock;
        self.clock = clock.clone();
        changed
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }
    fn candles(&self) -> &[StudyCandle] {
        &self.candles
    }
    fn colors(&self) -> Vec<Color32> {
        std::iter::once(self.color)
            .chain(self.alternate_color)
            .collect()
    }
    fn set_colors(&mut self, colors: Vec<Color32>) {
        if let Some(&color) = colors.first() {
            self.color = color;
        }
        if self.alternate_color.is_some()
            && let Some(&color) = colors.get(1)
        {
            self.alternate_color = Some(color);
        }
    }
    fn plot_variants(&self, line: usize) -> Vec<super::PlotVariant> {
        if line == 0
            && let Some(color) = self.alternate_color
        {
            let caption = match self.metric.colors {
                Some(MetricColors::Change { .. }) => "Falling",
                _ => "Below zero",
            };
            vec![super::PlotVariant::new(caption, color)]
        } else {
            Vec::new()
        }
    }
    fn plot_variant(&self, line: usize, bar: usize) -> Option<usize> {
        (line == 0 && self.alternate.get(bar) == Some(&true)).then_some(0)
    }
    fn is_visible(&self) -> bool {
        self.visible
    }
    fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }
    fn clone_box(&self) -> Box<dyn Indicator> {
        Box::new(self.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::MetricPoint;
    use crate::model::{BarClock, Timeframe};
    use chrono::DateTime;

    /// Each bar reads strictly before its close, holds earlier points, and accepts live updates.
    #[test]
    fn irregular_metric_points_hold_until_the_next_publication() {
        let time = |seconds| DateTime::from_timestamp(seconds, 0).unwrap();
        let day = |d: i64| 1_704_067_200 + d * 86_400;
        type Row = (
            &'static str,
            Option<Timeframe>,
            Vec<i64>,
            Vec<(i64, f64)>,
            Vec<Option<f64>>,
        );
        let irregular = vec![0, 30, 60, 120, 150, 180, 240, 300, 360];
        let daily: Vec<_> = (0..=20).map(|d| (day(d), d as f64)).collect();
        let mut live = daily.clone();
        live.push((day(20) + 43_200, 99.0));
        let mut boundary = live.clone();
        boundary.push((day(21), 50.0));
        let rows: Vec<Row> = vec![
            (
                "irregular publications, no clock",
                None,
                irregular.clone(),
                vec![(30, 10.0), (150, 20.0), (301, 5.0)],
                vec![
                    None,
                    Some(10.0),
                    Some(10.0),
                    Some(10.0),
                    Some(20.0),
                    Some(20.0),
                    Some(20.0),
                    Some(5.0),
                    Some(5.0),
                ],
            ),
            (
                "a correction replaces the point",
                None,
                irregular,
                vec![(30, 12.0), (150, 20.0), (301, 5.0)],
                vec![
                    None,
                    Some(12.0),
                    Some(12.0),
                    Some(12.0),
                    Some(20.0),
                    Some(20.0),
                    Some(20.0),
                    Some(5.0),
                    Some(5.0),
                ],
            ),
            (
                "weekly bars over daily points",
                Some(Timeframe::Week1),
                vec![day(0), day(7), day(14)],
                daily,
                vec![Some(6.0), Some(13.0), Some(20.0)],
            ),
            (
                "a point inside the live bar",
                Some(Timeframe::Week1),
                vec![day(0), day(7), day(14)],
                live,
                vec![Some(6.0), Some(13.0), Some(99.0)],
            ),
            (
                "a point at the live bar's close",
                Some(Timeframe::Week1),
                vec![day(0), day(7), day(14)],
                boundary,
                vec![Some(6.0), Some(13.0), Some(99.0)],
            ),
        ];
        for (name, interval, times, points, expected) in rows {
            let mut study = MetricStudy::new(MetricSeries::new(
                "oi",
                "Open interest",
                StudyFormat::Volume,
            ));
            if let Some(interval) = interval {
                study.set_bar_clock(&BarClock::new(interval));
            }
            let chart: Vec<_> = times
                .into_iter()
                .map(|t| Bar::new(time(t), 1.0, 1.0, 1.0, 1.0, 1.0))
                .collect();
            let points: Vec<_> = points
                .into_iter()
                .map(|(t, v)| MetricPoint::new(time(t), v).into())
                .collect();
            let source = StudyData {
                request: study.data_requests().pop().unwrap(),
                bars: &points,
                clock: BarClock::new(Timeframe::Ticks(1.try_into().unwrap())),
                instrument: None,
                timeframe: Timeframe::Min1,
                chart: None,
            };
            study.update_data(&chart, 0, &[source]);
            for (i, want) in expected.into_iter().enumerate() {
                assert_eq!(study.values().value(0, i), want, "{name}, bar {i}");
            }
            study.calculate(&chart);
            assert!(
                study.values().columns().is_empty(),
                "missing histories stay unavailable"
            );
        }
    }

    /// Points in a day aggregate as OHLC; held days stay flat and retain the falling colour.
    #[cfg(feature = "widget")]
    #[test]
    fn candle_metrics_paint_the_points_of_each_bar() {
        use crate::widget::{ChartBuilder, SeriesId, test_frames::shapes};
        use egui::{Shape, epaint::ColorMode};
        let hours = |h: i64| DateTime::from_timestamp(1_704_067_200 + 3_600 * h, 0).unwrap();
        let rising = Color32::from_rgb(20, 120, 240);
        let falling = Color32::from_rgb(250, 150, 10);
        let line = Color32::from_rgb(140, 40, 200);
        let mut chart = ChartBuilder::new().with_timeframe(Timeframe::Day1).build();
        chart.set_data(
            SeriesId::MAIN,
            (0..3)
                .map(|d| Bar::new(hours(d * 24), 1.0, 1.0, 1.0, 1.0, 1.0))
                .collect(),
        );
        assert!(chart.set_series_visible(SeriesId::MAIN, false));
        let mut metric = MetricSeries::new("oi", "Open interest", StudyFormat::Volume);
        metric.plot = MetricPlot::Candles;
        metric.colors = Some(MetricColors::Change { rising, falling });
        let id = chart.add_indicator(Box::new(MetricStudy::new(metric)));
        chart.set_study_data(
            StudyDataRequest {
                symbol: String::new(),
                timeframe: None,
                metric: Some("oi".into()),
            },
            vec![
                MetricPoint::ohlc(hours(0), 10.0, 12.0, 9.0, 11.0).into(),
                MetricPoint::ohlc(hours(12), 11.0, 15.0, 11.0, 14.0).into(),
                MetricPoint::ohlc(hours(30), 14.0, 14.0, 8.0, 9.0).into(),
            ],
            None,
        );
        let mut metric = MetricSeries::new("funding", "Funding rate", StudyFormat::Percent);
        metric.plot = MetricPlot::Line;
        metric.colors = Some(MetricColors::Single(line));
        chart.add_indicator(Box::new(MetricStudy::new(metric)));
        chart.set_study_data(
            StudyDataRequest {
                symbol: String::new(),
                timeframe: None,
                metric: Some("funding".into()),
            },
            (0..3)
                .map(|d| MetricPoint::new(hours(d * 24), (d + 1) as f64).into())
                .collect(),
            None,
        );
        let ctx = egui::Context::default();
        let mut painted = Vec::new();
        for frame in 0..12 {
            painted = shapes(&ctx, &mut chart, frame as f64, Vec::new());
        }
        let study = chart.indicators().find(|(key, _)| *key == id).unwrap().1;
        assert_eq!(
            study
                .candles()
                .iter()
                .map(|c| (
                    c.bars.clone(),
                    [c.bar.open, c.bar.high, c.bar.low, c.bar.close]
                ))
                .collect::<Vec<_>>(),
            [
                (0..1, [10.0, 15.0, 9.0, 14.0]),
                (1..2, [14.0, 14.0, 8.0, 9.0]),
                (2..3, [9.0, 9.0, 9.0, 9.0]),
            ]
        );
        assert_eq!(
            (0..3)
                .map(|bar| study.values().value(0, bar))
                .collect::<Vec<_>>(),
            [Some(14.0), Some(9.0), Some(9.0)]
        );
        assert_eq!(
            (0..3)
                .map(|bar| study.plot_color(0, bar))
                .collect::<Vec<_>>(),
            [None, Some(falling), Some(falling)]
        );
        for color in [rising, falling] {
            assert!(painted.iter().any(|shape| matches!(shape, Shape::LineSegment { stroke, .. } if stroke.color == color)), "wick in {color:?}");
        }
        assert!(painted.iter().any(|shape| matches!(shape, Shape::Path(path) if matches!(path.stroke.color, ColorMode::Solid(color) if color == line))), "scalar metric line");
    }
}
