//! The values and sources an alert can test on a chart.

use crate::Chart;
use crate::chart::series::SeriesId;
use crate::model::AlertSource;

impl Chart {
    /// The value `source` has at bar `bar` of the main series, which `AlertEngine::update` takes as
    /// its `value` function and any other evaluator can use: the close for
    /// [`Price`](AlertSource::Price), the volume for [`Volume`](AlertSource::Volume), the value of
    /// the study output at that bar for a [`Study`](AlertSource::Study) named by its [`SeriesId`]
    /// in decimal, and the price of the drawing's level at that bar for a
    /// [`Drawing`](AlertSource::Drawing) named as [`Drawing::alert_source`] does.
    ///
    /// `None` for an unknown study, output or drawing, a bar the series does not have, and a value
    /// that is not finite.
    ///
    /// [`Drawing::alert_source`]: crate::drawings::Drawing::alert_source
    pub fn alert_value(&self, source: &AlertSource, bar: usize) -> Option<f64> {
        let at = self.bars(SeriesId::MAIN).get(bar)?;
        let value = match source {
            AlertSource::Price => at.close,
            AlertSource::Volume => at.volume,
            AlertSource::Study { id, output } => self
                .indicators()
                .find(|(study, _)| study.0.to_string() == *id)?
                .1
                .plot_value(*output, bar)?,
            AlertSource::Drawing(_) => {
                let scale = self.config().right_axis_scale_mode;
                self.drawings().drawings.iter().find_map(|d| {
                    let level = d.alert_level(source)?;
                    d.alert_levels(bar as f64, scale)
                        .into_iter()
                        .find(|(index, ..)| *index == level)
                        .map(|(.., price)| price)
                })?
            }
        };
        value.is_finite().then_some(value)
    }

    /// Every output of every study and every level of every completed drawing, with the name the
    /// alert dialog shows, for `AlertDialog::sources`. Drawing levels are those at the newest bar.
    pub fn alert_sources(&self) -> Vec<(AlertSource, String)> {
        let bar = self.bars(SeriesId::MAIN).len().saturating_sub(1) as f64;
        let scale = self.config().right_axis_scale_mode;
        let studies = self.indicators().flat_map(|(id, study)| {
            study
                .line_names()
                .into_iter()
                .enumerate()
                .map(move |(output, name)| {
                    (
                        AlertSource::Study {
                            id: id.0.to_string(),
                            output,
                        },
                        format!("{}: {name}", study.name()),
                    )
                })
        });
        let drawings = self
            .drawings()
            .drawings
            .iter()
            .filter(|d| d.completed)
            .flat_map(|d| {
                let levels = d.alert_levels(bar, scale);
                let single = levels.len() == 1;
                levels.into_iter().map(move |(level, name, _)| {
                    let drawing = format!("{} {}", d.tool_type.as_str(), d.id);
                    (
                        d.alert_source(level),
                        if single {
                            drawing
                        } else {
                            format!("{drawing}: {name}")
                        },
                    )
                })
            });
        studies.chain(drawings).collect()
    }
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use super::*;
    use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
    use crate::model::{Bar, BarData};
    use chrono::DateTime;

    /// A moving average and a trend line resolve at a bar to the values drawn there.
    #[test]
    fn study_and_drawing_sources_resolve_to_the_values_painted_at_a_bar() {
        let bars = [10.0, 20.0, 30.0, 40.0]
            .into_iter()
            .enumerate()
            .map(|(i, close)| {
                let time = DateTime::from_timestamp(60 * i as i64, 0).unwrap();
                Bar::new(time, close, close, close, close, 5.0 * (i + 1) as f64)
            })
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        let study = chart.add_indicator(Box::new(crate::studies::SMA::new(2)));
        let mut line = Drawing::new(7, DrawingToolType::TrendLine);
        line.chart_points = vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(2.0, 120.0)];
        line.completed = true;
        chart.drawings_mut().drawings.push(line);
        let average = AlertSource::Study {
            id: study.0.to_string(),
            output: 0,
        };
        let trend = AlertSource::Drawing("7".into());

        // Rows are (source, bar, value): the mean of the last two closes, and the line through
        // (0, 100) and (2, 120), which rises 10 a bar.
        for (source, bar, expected) in [
            (AlertSource::Price, 3, Some(40.0)),
            (AlertSource::Volume, 1, Some(10.0)),
            (average.clone(), 3, Some(35.0)),
            (average.clone(), 0, None),
            (
                AlertSource::Study {
                    id: study.0.to_string(),
                    output: 5,
                },
                3,
                None,
            ),
            (
                AlertSource::Study {
                    id: "99".into(),
                    output: 0,
                },
                3,
                None,
            ),
            (trend.clone(), 3, Some(130.0)),
            (trend.clone(), 4, None),
            (AlertSource::Drawing("8".into()), 3, None),
            (AlertSource::Price, 4, None),
        ] {
            assert_eq!(
                chart.alert_value(&source, bar),
                expected,
                "{source:?} at {bar}"
            );
        }
        assert_eq!(
            chart.alert_sources(),
            [
                (average, "SMA: SMA(2)".to_owned()),
                (trend, "Trend Line 7".to_owned())
            ]
        );
    }
}
