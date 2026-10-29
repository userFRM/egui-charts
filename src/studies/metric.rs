//! Pane study for a scalar metric published by the host's data source.

use super::{Indicator, IndicatorValue, IndicatorValues, StudyData, StudyDataRequest};

use crate::data::MetricSeries;
use crate::model::Bar;
use crate::scales::StudyFormat;
use crate::studies::palette;
use egui::Color32;

/// A host metric, held by time on the chart's bars and drawn in an independent pane.
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
}

impl MetricStudy {
    /// Display this metric of the chart's current instrument.
    pub fn new(metric: MetricSeries) -> Self {
        Self {
            metric,
            values: IndicatorValues::new(),
            color: palette::ACCENT,
            visible: true,
        }
    }
}

impl Indicator for MetricStudy {
    fn name(&self) -> &str {
        &self.metric.name
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
        let source = sources.iter().find(|source| &source.request == request);
        self.values = data
            .iter()
            .map(|bar| {
                source
                    .and_then(|source| source.bar_at(bar.time).map(|i| source.bars[i].close))
                    .map_or(IndicatorValue::None, IndicatorValue::Single)
            })
            .collect();
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

    /// Publication instants are exact: earlier chart bars have no value, later bars
    /// carry the last point, and a correction replaces every bar that held that point.
    #[test]
    fn irregular_metric_points_hold_until_the_next_publication() {
        let time = |seconds| DateTime::from_timestamp(seconds, 0).unwrap();
        let points = [(30, 10.0), (150, 20.0), (301, 5.0)];
        let mut bars: Vec<Bar> = points
            .into_iter()
            .map(|(t, value)| {
                MetricPoint {
                    time: time(t),
                    value,
                }
                .into()
            })
            .collect();
        let chart: Vec<_> = [0, 30, 60, 120, 150, 180, 240, 300, 360]
            .into_iter()
            .map(|t| Bar::new(time(t), 1.0, 1.0, 1.0, 1.0, 1.0))
            .collect();
        let mut study = MetricStudy::new(MetricSeries {
            id: "oi".into(),
            name: "Open interest".into(),
            unit: "contracts".into(),
            format: StudyFormat::Volume,
        });
        let request = study.data_requests().pop().unwrap();
        // 10 applies from second 30, 20 from second 150, and 5 from second 301.
        for (edit, expected) in [
            (
                None,
                [
                    None,
                    Some(10.0),
                    Some(10.0),
                    Some(10.0),
                    Some(20.0),
                    Some(20.0),
                    Some(20.0),
                    Some(20.0),
                    Some(5.0),
                ],
            ),
            (
                Some(12.0),
                [
                    None,
                    Some(12.0),
                    Some(12.0),
                    Some(12.0),
                    Some(20.0),
                    Some(20.0),
                    Some(20.0),
                    Some(20.0),
                    Some(5.0),
                ],
            ),
        ] {
            if let Some(value) = edit {
                bars[0] = MetricPoint {
                    time: time(30),
                    value,
                }
                .into();
            }
            let source = StudyData {
                request: request.clone(),
                bars: &bars,
                clock: BarClock::new(Timeframe::Ticks(1.try_into().unwrap())),
                instrument: None,
                timeframe: Timeframe::Min1,
            };
            study.update_data(&chart, 0, &[source]);
            for (i, want) in expected.into_iter().enumerate() {
                assert_eq!(
                    study.values().value(0, i),
                    want,
                    "bar {i}, correction {edit:?}"
                );
            }
        }
        study.calculate(&chart);
        assert!(
            study.values().columns().is_empty(),
            "missing histories stay unavailable"
        );
    }
}
