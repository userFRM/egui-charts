//! Pane study for a metric published by the host's data source.

use super::{Indicator, IndicatorValue, IndicatorValues, StudyData, StudyDataRequest};

use crate::data::MetricSeries;
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
    values: IndicatorValues,
    color: Color32,
    visible: bool,
    /// The chart's bar calendar, which dates each bar's close.
    clock: crate::model::BarClock,
}

impl MetricStudy {
    /// Display this metric of the chart's current instrument.
    pub fn new(metric: MetricSeries) -> Self {
        Self {
            metric,
            values: IndicatorValues::new(),
            color: palette::ACCENT,
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
            plot_type: super::PlotType::StepLine,
            visible: true,
        }
    }
    fn data_requests(&self) -> Vec<StudyDataRequest> {
        vec![StudyDataRequest {
            symbol: String::new(),
            timeframe: None,
            metric: Some(self.metric.id.clone()),
        }]
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
        self.values = data
            .iter()
            .enumerate()
            .map(|(at, bar)| {
                // The clock bounds the live bar too; a point at its close waits for the next bar.
                let close = self
                    .clock
                    .next_open(self.clock.bar_open(bar.time))
                    .or_else(|| data.get(at + 1).map(|next| next.time));
                let end = close.map_or(points.len(), |close| {
                    points.partition_point(|point| point.time < close)
                });
                end.checked_sub(1).map_or(IndicatorValue::None, |last| {
                    IndicatorValue::Single(points[last].close)
                })
            })
            .collect();
    }
    fn set_bar_clock(&mut self, clock: &crate::model::BarClock) -> bool {
        let changed = self.clock != *clock;
        self.clock = clock.clone();
        changed
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }
    fn colors(&self) -> Vec<Color32> {
        vec![self.color]
    }
    fn set_colors(&mut self, colors: Vec<Color32>) {
        if let Some(&color) = colors.first() {
            self.color = color;
        }
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
        let day = |d| 1_704_067_200 + d * 86_400;
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
}
