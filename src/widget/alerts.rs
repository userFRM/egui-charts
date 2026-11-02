//! The values and sources an alert can test on a chart.

use crate::Chart;
use crate::chart::series::SeriesId;
use crate::model::AlertSource;

impl Chart {
    /// Capture calculation inputs for alert creation, independently of later chart changes.
    /// Copies and edits retain the inputs already carried by the alert.
    pub fn prepare_alert(&self, alert: &mut crate::model::Alert) {
        alert.timeframe = alert.timeframe.or(self.timeframe());
        if let Some(inputs) = &mut alert.inputs {
            inputs.add_sources_from(&self.alert_inputs());
        } else if alert.id.is_none() {
            let mut inputs = self.alert_inputs();
            if alert.symbol != self.symbol()
                || alert.timeframe != self.timeframe()
                || alert.watchlist.is_some()
            {
                inputs.start = None;
                inputs.visible = None;
                inputs.count = inputs.count.max(1000);
            }
            alert.inputs = Some(Box::new(inputs));
        }
    }

    /// A snapshot for an inline alert form. Capture again when the form is shown; submitting it
    /// copies this view into the alert, so storage never needs the chart to recover its inputs.
    pub fn alert_inputs(&self) -> crate::model::AlertInputs {
        let bars = self.bars(SeriesId::MAIN);
        let (left, right) = self.state.visible_range();
        crate::model::AlertInputs {
            timezone: match &self.chart_options().time_scale.timezone {
                crate::config::TimezoneMode::Local => None,
                zone => Some(zone.to_tz().unwrap_or_else(crate::model::TimezoneId::utc)),
            },
            calendar: self
                .instrument()
                .map(|instrument| crate::model::AlertCalendar {
                    symbol: self.symbol().to_owned(),
                    timezone: instrument.time_zone.clone(),
                    session: instrument.session.clone(),
                }),
            sources: self.alert_sources(),
            extended_hours: self.extended_hours(),
            conversion: self.conversion().map(str::to_owned),
            count: if bars.is_empty() { 1000 } else { bars.len() },
            start: bars.first().map(|bar| bar.time),
            visible: bars
                .get(left)
                .zip(bars.get(right.min(bars.len()).saturating_sub(1)))
                .map(|(first, last)| (first.time, last.time)),
            logarithmic: self.config().right_axis_scale_mode
                == crate::scales::PriceScaleMode::Logarithmic,
            studies: self
                .indicators()
                .map(|(id, study)| crate::model::AlertStudy {
                    id: id.0,
                    name: study.name().to_owned(),
                    inputs: study
                        .inputs()
                        .into_iter()
                        .map(|input| (input.id, input.value))
                        .collect(),
                })
                .collect(),
        }
    }

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
