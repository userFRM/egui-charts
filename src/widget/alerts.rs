//! The values and sources an alert can test on a chart.

use crate::Chart;
use crate::chart::series::SeriesId;
use crate::model::{AlertSource, AlertSourceName, ChartType, TPOLevelSource};
use crate::scales::PriceScaleMode;

impl Chart {
    /// Lend loaded live bars to the alert evaluator. A paused chart, or one in candle or market
    /// replay, cannot take over an alert's live subscription, and loading history keeps its
    /// background reader.
    #[cfg(feature = "alert-engine")]
    pub fn alert_data(&self) -> Option<crate::services::AlertData<'_>> {
        let link = self.feed_link.as_ref()?;
        if link.paused
            || self.replay_time().is_some()
            || self.feed.loading_history
            || self.feed.last_error.is_some()
            || self.bars(SeriesId::MAIN).is_empty()
        {
            return None;
        }
        let mut request =
            crate::services::AlertFeed::history_request(self.symbol(), self.timeframe()?);
        request.clock = self.bar_clock();
        request.extended_hours = self.extended_hours();
        request.overnight_hours = self.overnight_hours();
        request.adjusted = self.dividend_adjusted();
        request.conversion = self.conversion().map(str::to_owned);
        Some(crate::services::AlertData {
            request,
            bars: self.bars(SeriesId::MAIN),
            history_complete: self.feed.history_complete,
            #[cfg(feature = "ui")]
            drawings: &self.drawings().drawings,
            signals: self.signals().collect(),
            exchange: self
                .instrument()
                .and_then(|instrument| instrument.exchange.as_deref()),
        })
    }

    /// Capture calculation inputs for alert creation, independently of later chart changes.
    /// Copies and edits retain the inputs already carried by the alert. A new alert on the chart's
    /// symbol interpolates drawing levels on the scale of the pane that paints the drawing its
    /// first condition names.
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
        // The evaluator resolves every drawing level of an alert on this one scale. Another
        // symbol's drawings reuse the numbers of this chart's, so only its own symbol is looked up.
        // shortcut: one scale per alert, so a later condition's drawing in a pane on another
        // scale resolves on the first one's; upgrade when inputs carry a scale per drawing.
        if alert.id.is_none()
            && alert.symbol == self.symbol()
            && let Some(inputs) = &mut alert.inputs
            && let Some(drawing) = std::iter::once(&alert.source)
                .chain(alert.condition.partners().into_iter().flatten())
                .find_map(|source| {
                    self.drawings()
                        .drawings
                        .iter()
                        .find(|d| d.alert_level(source).is_some())
                })
        {
            inputs.logarithmic = self.drawing_subject(drawing.id).0 == PriceScaleMode::Logarithmic;
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
            overnight_hours: self.overnight_hours(),
            dividend_adjusted: self.dividend_adjusted(),
            conversion: self.conversion().map(str::to_owned),
            count: if bars.is_empty() { 1000 } else { bars.len() },
            start: bars.first().map(|bar| bar.time),
            visible: bars
                .get(left)
                .zip(bars.get(right.min(bars.len()).saturating_sub(1)))
                .map(|(first, last)| (first.time, last.time)),
            logarithmic: self.config().right_axis_scale_mode == PriceScaleMode::Logarithmic,
            studies: self
                .indicators()
                .map(|(id, study)| crate::model::AlertStudy {
                    id: id.0,
                    name: study.registered_name().to_owned(),
                    inputs: study
                        .inputs()
                        .into_iter()
                        .map(|input| (input.id, input.value))
                        .collect(),
                })
                .collect(),
            footprint: Some(crate::model::FootprintAlertRule::from_chart(
                &self.chart_type_params,
                self.footprint_row_ticks(),
            )),
        }
    }

    /// The value `source` has at bar `bar` of the main series, which `AlertEngine::update` takes as
    /// its `value` function and any other evaluator can use: the close for
    /// [`Price`](AlertSource::Price), the volume for [`Volume`](AlertSource::Volume), the value of
    /// the study output at that bar for a [`Study`](AlertSource::Study) named by its [`SeriesId`]
    /// in decimal, `1` or `0` for a [`StudyCondition`](AlertSource::StudyCondition) that holds or
    /// does not at that bar, and the value of the drawing's level at that bar, on the scale of
    /// the pane that paints the drawing, an anchored VWAP counting the executions the chart holds, for a
    /// [`Drawing`](AlertSource::Drawing) named as
    /// [`Drawing::alert_source`] does, and the price a [`TPOLevel`](AlertSource::TPOLevel)
    /// keeps.
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
            AlertSource::StudyCondition { id, condition } => {
                if self
                    .indicators()
                    .find(|(study, _)| study.0.to_string() == *id)?
                    .1
                    .alert_condition(*condition, bar)?
                {
                    1.0
                } else {
                    0.0
                }
            }
            AlertSource::Drawing(_) => self.drawings().drawings.iter().find_map(|d| {
                let level = d.alert_level(source)?;
                let (scale, ..) = self.drawing_subject(d.id);
                d.alert_levels_with_executions(
                    bar as f64,
                    scale,
                    self.bars(SeriesId::MAIN),
                    self.drawing_executions(),
                )
                .into_iter()
                .find(|(index, ..)| *index == level)
                .map(|(.., price)| price)
            })?,
            AlertSource::TPOLevel(level) => level.price,
        };
        value.is_finite().then_some(value)
    }

    /// Each study's alert outputs and declared conditions and every level of every completed
    /// drawing whose pane marks the price or a study, as [`Self::alert_value`] resolves it, with
    /// the name the alert dialog
    /// shows, for `AlertDialog::sources`. Drawing levels are those at the newest bar. A name keeps
    /// the study's name, the drawing's number and a fork or fan level's value as given and
    /// translates the study output, the drawing tool and the level when shown. A level in a study
    /// pane is named after the study first in its pane, as in `RSI: Horizontal Line 7`.
    /// A study offers the outputs [`Indicator::alert_outputs`] lists. Then, for the TPO chart type
    /// and each visible TPO study, come the extended levels it draws of completed profiles that no
    /// later bar has reached, at their current prices, named `TPO {session}: {level} {price}` or
    /// after the study, the session written as [`Self::format_time`] writes it; see
    /// [`TPOLevelSource`].
    ///
    /// [`Indicator::alert_outputs`]: crate::studies::Indicator::alert_outputs
    pub fn alert_sources(&self) -> Vec<(AlertSource, AlertSourceName)> {
        let bar = self.bars(SeriesId::MAIN).len().saturating_sub(1) as f64;
        let studies = self.indicators().flat_map(|(id, study)| {
            study
                .alert_outputs()
                .into_iter()
                .map(move |(output, name)| {
                    (
                        AlertSource::Study {
                            id: id.0.to_string(),
                            output,
                        },
                        AlertSourceName::new(
                            "{study}: {output}",
                            &[("study", study.name())],
                            &[("output", name.as_str())],
                        ),
                    )
                })
                .chain(study.alert_conditions().into_iter().enumerate().map(
                    move |(condition, name)| {
                        (
                            AlertSource::StudyCondition {
                                id: id.0.to_string(),
                                condition,
                            },
                            AlertSourceName::new(
                                "{study}: {condition}",
                                &[("study", study.name())],
                                &[("condition", name.as_str())],
                            ),
                        )
                    },
                ))
        });
        let drawings = self
            .drawings()
            .drawings
            .iter()
            .filter(|d| d.completed)
            .flat_map(|d| {
                let (scale, subject, study) = self.drawing_subject(d.id);
                let levels = if subject.is_some() {
                    d.alert_levels_with_bars(bar, scale, self.bars(SeriesId::MAIN))
                } else {
                    Vec::new()
                };
                let single = levels.len() == 1;
                let tool_name = self.drawings().registry.tool_name(d);
                levels.into_iter().map(move |(level, name, _)| {
                    let id = d.id.to_string();
                    let tool = ("tool", tool_name);
                    let named =
                        |template: &str, values: &[(&str, &str)], labels: &[(&str, &str)]| {
                            match study {
                                Some(study) => AlertSourceName::new(
                                    &format!("{{pane}}: {template}"),
                                    &[values, &[("pane", study)]].concat(),
                                    labels,
                                ),
                                None => AlertSourceName::new(template, values, labels),
                            }
                        };
                    let name = if single {
                        named("{tool} {id}", &[("id", id.as_str())], &[tool])
                    } else if let Some((line, value)) = d.level_line(level) {
                        named(
                            line,
                            &[("id", id.as_str()), ("value", value.as_str())],
                            &[tool],
                        )
                    } else {
                        named(
                            "{tool} {id}: {level}",
                            &[("id", id.as_str())],
                            &[tool, ("level", name.as_str())],
                        )
                    };
                    (d.alert_source(level), name)
                })
            });
        let (start, end) = self.state.visible_range();
        let chart_type = (self.chart_type == ChartType::TimePriceOpportunity
            && self.main_series_visible)
            .then(|| (self.tpo_chart_division(), None));
        let tpo_studies = self
            .indicators()
            .filter(|(_, study)| self.shows_study(*study) && !self.chart_type.is_time_independent())
            .filter_map(|(_, study)| Some((self.tpo_division(study.tpo()?), Some(study.name()))));
        let mut tpo: Vec<(AlertSource, AlertSourceName)> = Vec::new();
        for ((clock, config), study) in chart_type.into_iter().chain(tpo_studies) {
            let levels = crate::chart::rendering::candles::tpo::open_levels(
                &self.state.data().bars,
                start..end,
                self.state.revision(),
                (&config, self.instrument().and_then(|i| i.tick_size)),
                &clock,
            );
            for (profile, level, price, text) in levels {
                let source = AlertSource::TPOLevel(TPOLevelSource::new(profile, level, price));
                if tpo.iter().any(|(offered, _)| *offered == source) {
                    continue;
                }
                let session = self.format_time(profile);
                let values = [("session", session.as_str()), ("price", text.as_str())];
                let labels = [("level", level.label())];
                let name = match study {
                    Some(study) => AlertSourceName::new(
                        "{study} {session}: {level} {price}",
                        &[&values[..], &[("study", study)]].concat(),
                        &labels,
                    ),
                    None => {
                        AlertSourceName::new("TPO {session}: {level} {price}", &values, &labels)
                    }
                };
                tpo.push((source, name));
            }
        }
        studies.chain(drawings).chain(tpo).collect()
    }

    /// The scale mode the levels of drawing `id` resolve with, what they mark and the study that
    /// names them, as the pane painting the drawing shows them. In the pane holding the main
    /// series they mark the price on that series' scale. In another pane they mark the first
    /// alert output of the pane's first member, or the volume for a volume study, on the scale
    /// that pane paints the drawing with, and are named after that study. They mark nothing an alert
    /// can name when that member is a bar series, the pane is empty or the study offers none.
    pub(crate) fn drawing_subject(
        &self,
        id: usize,
    ) -> (PriceScaleMode, Option<AlertSource>, Option<&str>) {
        let main = self.main_series_pane();
        let pane = self.drawing_pane(id).unwrap_or(main);
        if pane == main {
            return (
                self.config().right_axis_scale_mode,
                Some(AlertSource::Price),
                None,
            );
        }
        let first = self.pane_series_iter(pane).next();
        let study = first.and_then(|first| self.indicators().find(|(id, _)| *id == first));
        // The scale the pane paints its drawings on, as `show_with_panes` picks it.
        let key = first
            .and_then(|first| self.series_scale(first))
            .or_else(|| match study?.1.scale() {
                crate::studies::StudyScale::Shared(key) => Some(key),
                _ => None,
            })
            .unwrap_or(crate::model::SeriesScale::Right);
        // A study that offers no alert output marks nothing an alert can test.
        let study = study.and_then(|(id, study)| {
            let (output, _) = study.alert_outputs().into_iter().next()?;
            Some((id, study, output))
        });
        let source = study.map(|(id, study, output)| {
            if study.is_volume() {
                AlertSource::Volume
            } else {
                AlertSource::Study {
                    id: id.0.to_string(),
                    output,
                }
            }
        });
        (
            self.pane_price_scale(pane, key).options().mode,
            source,
            study.map(|(_, study, _)| study.name()),
        )
    }
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use super::*;
    use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
    use crate::model::{Bar, BarData, Timeframe};
    use crate::studies::{
        AutoKeyLevels, ChartPatterns, Gaps, Indicator, InputValue, KeyLevels, MissedPivots,
    };
    use chrono::{DateTime, TimeZone, Utc};

    /// Hand-calculated flips, crosses and changes of histogram direction, read through the
    /// chart's configured studies. Plot offsets never displace a calculated condition.
    #[test]
    fn study_conditions_read_one_where_they_hold() {
        use crate::studies::{AwesomeOscillator, Indicator, InputValue, MACD, SuperTrend};
        type Case = (
            Box<dyn Indicator>,
            Vec<f64>,
            Vec<(&'static str, Vec<Option<f64>>)>,
        );
        let rows: Vec<Case> = vec![
            (
                Box::new(SuperTrend::new(1, 1.0)),
                vec![100., 101., 105., 106., 100., 99., 103.],
                vec![
                    (
                        "SuperTrend: Trend turns up",
                        vec![None, None, Some(1.), Some(0.), Some(0.), Some(0.), Some(1.)],
                    ),
                    (
                        "SuperTrend: Trend turns down",
                        vec![None, None, Some(0.), Some(0.), Some(1.), Some(0.), Some(0.)],
                    ),
                    (
                        "SuperTrend: Trend change",
                        vec![None, None, Some(1.), Some(0.), Some(1.), Some(0.), Some(1.)],
                    ),
                ],
            ),
            (
                Box::new(MACD::new(1, 2, 2)),
                vec![30., 60., 50., 80., 40.],
                vec![
                    (
                        "MACD: Falling to rising",
                        vec![None, None, None, Some(1.), Some(0.)],
                    ),
                    (
                        "MACD: Rising to falling",
                        vec![None, None, None, Some(0.), Some(1.)],
                    ),
                ],
            ),
            (
                Box::new(AwesomeOscillator::new(1, 2)),
                vec![10., 20., 40., 50., 40., 60.],
                vec![
                    (
                        "AO: Changes to increasing bars",
                        vec![None, None, None, Some(0.), Some(0.), Some(1.)],
                    ),
                    (
                        "AO: Changes to decreasing bars",
                        vec![None, None, None, Some(1.), Some(0.), Some(0.)],
                    ),
                ],
            ),
            // AO is missing, 5, 10, 10, 10, 10: an equal bar changes rising to falling.
            (
                Box::new(AwesomeOscillator::new(1, 2)),
                vec![10., 20., 40., 60., 80., 100.],
                vec![
                    (
                        "AO: Changes to increasing bars",
                        vec![None, None, None, Some(0.), Some(0.), Some(0.)],
                    ),
                    (
                        "AO: Changes to decreasing bars",
                        vec![None, None, None, Some(1.), Some(0.), Some(0.)],
                    ),
                ],
            ),
        ];
        for (study, closes, conditions) in rows {
            let bars: Vec<_> = closes
                .into_iter()
                .enumerate()
                .map(|(i, c)| {
                    Bar::new(
                        DateTime::from_timestamp(i as i64 * 60, 0).unwrap(),
                        c,
                        c + 1.,
                        c - 1.,
                        c,
                        1.,
                    )
                })
                .collect();
            let mut chart = Chart::new(BarData::from_bars(bars.clone()));
            let id = chart.add_indicator(study);
            let sources = chart.alert_sources();
            for (condition, (name, expected)) in conditions.iter().enumerate() {
                let source = AlertSource::StudyCondition {
                    id: id.0.to_string(),
                    condition,
                };
                assert!(
                    sources
                        .iter()
                        .any(|(s, n)| *s == source && n.to_string() == *name),
                    "{name}"
                );
                for (bar, expected) in expected.iter().enumerate() {
                    assert_eq!(
                        chart.alert_value(&source, bar),
                        *expected,
                        "{name} at {bar}"
                    );
                }
                assert_eq!(chart.alert_value(&source, expected.len()), None);
            }
            assert_eq!(
                chart.alert_value(
                    &AlertSource::StudyCondition {
                        id: id.0.to_string(),
                        condition: conditions.len()
                    },
                    2
                ),
                None
            );
            let study = chart.indicator_mut(id).unwrap();
            if study.inputs().iter().any(|input| input.id == "plot_offset") {
                study
                    .set_input("plot_offset", InputValue::Integer(2))
                    .unwrap();
                study.calculate(&bars);
                for (condition, (_, expected)) in conditions.iter().enumerate() {
                    for (bar, expected) in expected.iter().enumerate() {
                        assert_eq!(
                            chart.alert_value(
                                &AlertSource::StudyCondition {
                                    id: id.0.to_string(),
                                    condition
                                },
                                bar
                            ),
                            *expected
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn study_conditions_follow_calculation_bars_and_eviction() {
        use crate::studies::{
            ConfiguredStudy, Indicator, InputValue, StudyDataRequest, SuperTrend,
        };
        let bar = |minute, close| {
            Bar::new(
                DateTime::from_timestamp(minute * 60, 0).unwrap(),
                close,
                close + 1.0,
                close - 1.0,
                close,
                1.0,
            )
        };
        let mut chart = Chart::new(BarData::from_bars(
            (0..16).map(|minute| bar(minute, 100.0)).collect(),
        ));
        chart.set_timeframe(Timeframe::Min1);
        let mut study = ConfiguredStudy::new(Box::new(SuperTrend::new(1, 1.0)));
        study
            .set_input(
                "calculation_timeframe",
                InputValue::Timeframe(Some(Timeframe::Min5)),
            )
            .unwrap();
        chart.set_study_data(
            StudyDataRequest {
                symbol: String::new(),
                timeframe: Some(Timeframe::Min5),
                metric: None,
            },
            [100.0, 101.0, 105.0, 106.0]
                .into_iter()
                .enumerate()
                .map(|(i, close)| bar(i as i64 * 5, close))
                .collect(),
            None,
        );
        let id = chart.add_indicator(Box::new(study));
        let source = AlertSource::StudyCondition {
            id: id.0.to_string(),
            condition: 2,
        };
        // Five-minute trends are none, down, up, up. Closed results reach minute bars
        // 4, 9 and 14; the change therefore holds at 14 and the forming minute 15.
        let expected = [None, None, Some(1.0)];
        for removed in [0, 2] {
            assert_eq!(
                chart.remove_oldest_bars(SeriesId::MAIN, removed).len(),
                removed
            );
            for at in 0..16 - removed {
                let minute = at + removed;
                let value = expected[if minute < 9 {
                    0
                } else if minute < 14 {
                    1
                } else {
                    2
                }];
                assert_eq!(
                    chart.alert_value(&source, at),
                    value,
                    "minute {minute}, removed {removed}"
                );
            }
            assert_eq!(chart.alert_value(&source, 16 - removed), None);
        }
    }

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
        let mut fork = Drawing::new(9, DrawingToolType::Pitchfork);
        fork.chart_points = vec![
            ChartPoint::new(0.0, 100.0),
            ChartPoint::new(1.0, 200.0),
            ChartPoint::new(2.0, 50.0),
        ];
        // Saved without a config: levels 0, 0.5, 1 and 1.5, all shown.
        fork.pitchfork = None;
        fork.completed = true;
        chart.drawings_mut().drawings.push(fork);
        let mut fan = Drawing::new(11, DrawingToolType::FibonacciSpeedFan);
        fan.chart_points = vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(2.0, 120.0)];
        fan.completed = true;
        let level = crate::drawings::FibonacciLevel::new(0.5, "", [0; 4]);
        fan.fib_config = Some(crate::drawings::FibonacciConfig {
            levels: vec![level.clone()],
            time_levels: Some(vec![level]),
            levels_as_percent: true,
            ..crate::drawings::FibonacciConfig::legacy()
        });
        chart.drawings_mut().drawings.push(fan);
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
        // Names keep the English text of the names that alerts and presets saved before.
        let fork = |level: &str| AlertSource::Drawing(format!("9{level}"));
        assert_eq!(
            chart.alert_sources(),
            [
                (average, AlertSourceName::from("SMA: SMA(2)")),
                (trend, "Trend Line 7".into()),
                (fork(""), "Pitchfork 9: Median".into()),
                (fork("#2"), "Pitchfork 9: 0.5: first line".into()),
                (fork("#3"), "Pitchfork 9: 0.5: second line".into()),
                (fork("#4"), "Pitchfork 9: 1: first line".into()),
                (fork("#5"), "Pitchfork 9: 1: second line".into()),
                (fork("#6"), "Pitchfork 9: 1.5: first line".into()),
                (fork("#7"), "Pitchfork 9: 1.5: second line".into()),
                (
                    AlertSource::Drawing("11".into()),
                    "Fib Speed Resistance Fan 11: 50%: price level".into()
                ),
                (
                    AlertSource::Drawing("11#1".into()),
                    "Fib Speed Resistance Fan 11: 50%: time level".into()
                ),
            ]
        );
        let (_, name) = chart.alert_sources().pop().unwrap();
        assert_eq!(
            serde_json::to_value(name).unwrap()["template"],
            "{tool} {id}: {value}: time level"
        );
    }

    /// A drawing in a study pane marks the leading study's first alert output, on
    /// that pane's scale, and nothing when it offers none; price-pane drawings mark the price.
    #[test]
    fn study_pane_drawings_alert_on_their_study() {
        let bars = [10.0, 20.0, 30.0, 40.0]
            .into_iter()
            .enumerate()
            .map(|(i, close)| {
                let time = DateTime::from_timestamp(60 * i as i64, 0).unwrap();
                Bar::new(time, close, close, close, close, 1.0)
            })
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        chart.set_symbol("SAMPLE");
        chart.config_mut().right_axis_scale_mode = PriceScaleMode::Logarithmic;
        let rsi = chart.add_indicator(Box::new(
            crate::studies::CustomIndicator::new("RSI", |bars: &[Bar]| {
                vec![crate::studies::IndicatorValue::Single(50.0); bars.len()]
            })
            .with_overlay(false),
        ));
        let gaps = chart.add_indicator(Box::new(Gaps::default()));
        let levels = chart.add_indicator(Box::new(AutoKeyLevels::new()));
        assert!(chart.move_indicator_to_pane(gaps, gaps));
        assert!(chart.move_indicator_to_pane(levels, levels));
        let drawing = |id, tool, points: &[(f32, f64)], pane| {
            let mut drawing = Drawing::new(id, tool);
            drawing.chart_points = points
                .iter()
                .map(|&(bar, price)| ChartPoint::new(bar, price))
                .collect();
            drawing.completed = true;
            drawing.pane = pane;
            drawing
        };
        chart.drawings_mut().drawings = vec![
            drawing(
                7,
                DrawingToolType::HorizontalLine,
                &[(0.0, 70.0)],
                Some(rsi.0),
            ),
            drawing(
                8,
                DrawingToolType::TrendLine,
                &[(0.0, 40.0), (2.0, 60.0)],
                Some(rsi.0),
            ),
            drawing(9, DrawingToolType::HorizontalLine, &[(0.0, 25.0)], None),
            drawing(
                10,
                DrawingToolType::HorizontalLine,
                &[(0.0, 25.0)],
                Some(gaps.0),
            ),
            drawing(
                11,
                DrawingToolType::HorizontalLine,
                &[(0.0, 25.0)],
                Some(levels.0),
            ),
        ];
        // The study pane's own scale is linear: the line through (0, 40) and (2, 60) is 50 at
        // bar 1, where the main pane's logarithmic scale would give sqrt(40 * 60).
        for (id, bar, expected) in [("7", 3, 70.0), ("8", 1, 50.0), ("9", 3, 25.0)] {
            let source = AlertSource::Drawing(id.into());
            assert_eq!(chart.alert_value(&source, bar), Some(expected), "{id}");
        }
        let names = chart.alert_sources();
        for (id, name) in [("7", "RSI: Horizontal Line 7"), ("9", "Horizontal Line 9")] {
            assert!(
                names.contains(&(AlertSource::Drawing(id.into()), name.into())),
                "{names:?}"
            );
        }
        assert!(
            !names
                .iter()
                .any(|(source, _)| *source == AlertSource::Drawing("10".into()))
        );
        assert!(names.contains(&(
            AlertSource::Drawing("11".into()),
            "Auto Key Levels: Horizontal Line 11".into()
        )));
        // Drawing 7 of another symbol is not this chart's: that alert keeps the price scale.
        let mut other = crate::model::Alert::new(
            "OTHER",
            crate::model::AlertCondition::Compare {
                other: AlertSource::Drawing("7".into()),
                comparison: crate::model::AlertComparison::Crossing,
            },
        );
        chart.prepare_alert(&mut other);
        assert!(other.inputs.unwrap().logarithmic);
        #[cfg(feature = "ui")]
        {
            let study = AlertSource::Study {
                id: rsi.0.to_string(),
                output: 0,
            };
            // Rows are (drawing, the source its alert tests, that source at bar 3, whether the
            // saved alert interpolates drawing levels logarithmically).
            for (id, source, value, logarithmic) in
                [(7, study, 50.0, false), (9, AlertSource::Price, 40.0, true)]
            {
                let mut menu = crate::ui::context_menu::drawing::DrawingContextMenu::new();
                menu.open_on(&egui::Context::default(), &chart, egui::Pos2::ZERO, id);
                let mut alert = menu.alert.unwrap();
                assert_eq!(
                    (&alert.source, &alert.condition),
                    (
                        &source,
                        &crate::model::AlertCondition::Compare {
                            other: AlertSource::Drawing(id.to_string()),
                            comparison: crate::model::AlertComparison::Crossing,
                        }
                    ),
                    "{id}"
                );
                assert_eq!(chart.alert_value(&alert.source, 3), Some(value), "{id}");
                chart.prepare_alert(&mut alert);
                assert_eq!(alert.inputs.unwrap().logarithmic, logarithmic, "{id}");
            }
            let mut menu = crate::ui::context_menu::drawing::DrawingContextMenu::new();
            menu.open_on(&egui::Context::default(), &chart, egui::Pos2::ZERO, 10);
            assert!(menu.alert.is_none());
            menu.open_on(&egui::Context::default(), &chart, egui::Pos2::ZERO, 11);
            assert_eq!(
                menu.alert.unwrap().source,
                AlertSource::Study {
                    id: levels.0.to_string(),
                    output: 7,
                }
            );
        }
    }
    fn level_bars() -> Vec<Bar> {
        [
            ((1, 30, 12), 10.0, 3.0),
            ((1, 30, 18), 34.0, 1.0),
            ((2, 2, 0), 54.0, 3.0),
            ((2, 2, 6), 30.0, 1.0),
            ((2, 3, 0), 30.0, 3.0),
            ((2, 3, 6), 54.0, 1.0),
        ]
        .into_iter()
        .map(|((month, day, hour), price, volume)| {
            let time = Utc.with_ymd_and_hms(2026, month, day, hour, 0, 0).unwrap();
            Bar::new(time, price, price, price, price, volume)
        })
        .collect()
    }

    /// Guards #3658: studies offer alerts only on outputs that can hold values, each read at
    /// the bars it covers, and zone and mark studies list no empty values in the legend.
    #[test]
    fn studies_offer_only_outputs_that_hold_values() {
        // Feb 3: 3/4 volume at 30 in [30, 31), so POC=30.5, VAL=30, VAH=31.
        // The week ties endpoint rows at 4 each: lower POC=30.5, area [30, 53], since
        // adding the top row would exceed 70% of 8. Both periods have 24 rows of width 1.
        // An output's value at each of the six bars.
        type Output = (usize, String, Vec<Option<f64>>);
        type Row = (Box<dyn Indicator>, Vec<Output>, usize);
        let from = |price: f64, first: usize| -> Vec<Option<f64>> {
            (0..6).map(|bar| (bar >= first).then_some(price)).collect()
        };
        let levels: Vec<Output> = [
            (7, "dPOC", 30.5, 4),
            (8, "dVAL", 30.0, 4),
            (9, "dVAH", 31.0, 4),
            (14, "wPOC", 30.5, 2),
            (15, "wVAL", 30.0, 2),
            (16, "wVAH", 53.0, 2),
        ]
        .into_iter()
        .map(|(output, name, price, first)| (output, name.to_owned(), from(price, first)))
        .collect();
        let mut hidden = AutoKeyLevels::new();
        hidden
            .set_input("current_levels [0]", InputValue::Toggle(false))
            .unwrap();
        let mut all = AutoKeyLevels::new();
        for input in all.inputs() {
            if matches!(input.value, InputValue::Toggle(_)) {
                all.set_input(&input.id, InputValue::Toggle(true)).unwrap();
            }
        }
        all.set_input("length", InputValue::Length(2)).unwrap();
        // Previous week/month are Jan 30; previous day is Feb 2. Custom spans Feb 2 and 3.
        let all_outputs = [
            (7, "d", 4, [30.5, 30.0, 31.0, 30.0, 54.0, 30.0, 54.0]),
            (14, "w", 2, [30.5, 30.0, 53.0, 54.0, 54.0, 30.0, 54.0]),
            (21, "m", 2, [30.5, 30.0, 53.0, 54.0, 54.0, 30.0, 54.0]),
            (28, "pd", 2, [53.5, 53.0, 54.0, 54.0, 54.0, 30.0, 30.0]),
            (35, "pw", 0, [10.5, 10.0, 11.0, 10.0, 34.0, 10.0, 34.0]),
            (42, "pm", 0, [10.5, 10.0, 11.0, 10.0, 34.0, 10.0, 34.0]),
            (49, "Custom ", 2, [30.5, 30.0, 53.0, 54.0, 54.0, 30.0, 54.0]),
        ]
        .into_iter()
        .flat_map(|(output, prefix, first, prices)| {
            ["POC", "VAL", "VAH", "O", "H", "L", "C"]
                .into_iter()
                .zip(prices)
                .enumerate()
                .map(move |(k, (tag, price))| {
                    (output + k, format!("{prefix}{tag}"), from(price, first))
                })
        })
        .collect();
        let factory = crate::studies::IndicatorFactory::new();
        let create = |name: &str| factory.create(name).unwrap();
        // An offered output that holds no value on these bars.
        let offered = |output: usize, name: &str| -> Output { (output, name.to_owned(), vec![]) };
        // Each bar closes at its open, which counts as down in Bollinger Bars and up in Volume.
        let closes: Vec<_> = level_bars().iter().map(|bar| Some(bar.close)).collect();
        let volumes: Vec<_> = level_bars().iter().map(|bar| Some(bar.volume)).collect();
        let rows: Vec<Row> = vec![
            (Box::new(AutoKeyLevels::new()), levels.clone(), 0),
            (Box::new(all), all_outputs, 0),
            (
                Box::new(hidden),
                levels
                    .into_iter()
                    .filter(|(output, ..)| *output != 7)
                    .collect(),
                0,
            ),
            (Box::new(Gaps::default()), vec![], 0),
            (Box::new(KeyLevels::default()), vec![], 0),
            (Box::new(MissedPivots::default()), vec![], 0),
            (
                Box::new(ChartPatterns::all()),
                vec![offered(0, "Bullish"), offered(1, "Bearish")],
                0,
            ),
            // A one-sided formation fails on a break the other way.
            (create("Double Top"), vec![offered(1, "Bearish")], 0),
            (create("Double Bottom"), vec![offered(0, "Bullish")], 0),
            (
                create("Rectangle"),
                vec![offered(0, "Bullish"), offered(1, "Bearish")],
                0,
            ),
            // A single candlestick pattern marks its own side on output 0.
            (
                create("Engulfing - Bullish"),
                vec![offered(0, "Bullish")],
                0,
            ),
            (
                create("Engulfing - Bearish"),
                vec![offered(0, "Bearish")],
                0,
            ),
            (create("Doji"), vec![offered(0, "Neutral")], 0),
            (
                create("All Candlestick Patterns"),
                vec![
                    offered(0, "Bullish"),
                    offered(1, "Bearish"),
                    offered(2, "Neutral"),
                ],
                0,
            ),
            // Wicks only colours the candle blocks.
            (
                create("Bollinger Bars"),
                vec![offered(1, "Up"), (2, "Down".into(), closes)],
                2,
            ),
            // The moving average is off by default.
            (
                create("Volume"),
                vec![(0, "Up volume".into(), volumes), offered(1, "Down volume")],
                0,
            ),
        ];
        for (study, outputs, empty_values) in rows {
            let name = study.name().to_owned();
            let mut chart = Chart::new(BarData::from_bars(level_bars()));
            let id = chart.add_indicator(study);
            let expected: Vec<_> = outputs
                .iter()
                .map(|(output, tag, _)| {
                    (
                        AlertSource::Study {
                            id: id.0.to_string(),
                            output: *output,
                        },
                        AlertSourceName::from(format!("{name}: {tag}")),
                    )
                })
                .collect();
            let offered: Vec<_> = chart
                .alert_sources()
                .into_iter()
                .filter(|(source, _)| matches!(source, AlertSource::Study { .. }))
                .collect();
            assert_eq!(offered, expected, "{name}");
            for output in 0..57 {
                let source = AlertSource::Study {
                    id: id.0.to_string(),
                    output,
                };
                for bar in 0..=6 {
                    let expected = outputs
                        .iter()
                        .find(|(at, ..)| *at == output)
                        .and_then(|(_, _, values)| values.get(bar).copied().flatten());
                    assert_eq!(
                        chart.alert_value(&source, bar),
                        expected,
                        "{name} output {output} bar {bar}"
                    );
                }
            }
            let shapes = crate::widget::test_frames::shapes(
                &egui::Context::default(),
                &mut chart,
                0.0,
                vec![],
            );
            let empty = shapes
                .iter()
                .filter(
                    |shape| matches!(shape, egui::Shape::Text(text) if text.galley.text() == "∅"),
                )
                .count();
            assert_eq!(empty, empty_values, "{name} legend");
        }
    }

    /// A live downward crossing of the day POC fires; an undrawn previous-day POC cannot fire
    /// even though its price (53.5) is crossed by the same 54-to-30 move.
    #[cfg(feature = "alert-engine")]
    #[test]
    fn level_crossing_alerts_fire_and_hidden_levels_never_do() {
        use crate::model::{Alert, AlertComparison, AlertCondition, Timeframe};
        use crate::services::AlertEngine;
        use std::sync::{Arc, Mutex};
        let mut chart = Chart::new(BarData::from_bars(level_bars()));
        chart.set_symbol("SAMPLE");
        let id = chart.add_indicator(Box::new(AutoKeyLevels::new()));
        let alerts = [(7, "dPOC"), (28, "pdPOC")]
            .into_iter()
            .map(|(output, name)| {
                let mut alert = Alert::new(
                    "SAMPLE",
                    AlertCondition::Compare {
                        other: AlertSource::Study {
                            id: id.0.to_string(),
                            output,
                        },
                        comparison: AlertComparison::CrossingDown,
                    },
                );
                alert.timeframe = Some(Timeframe::Min1);
                alert.name = name.to_owned();
                alert
            })
            .collect();
        let fired = Arc::new(Mutex::new(Vec::new()));
        let log = fired.clone();
        let engine = AlertEngine::new()
            .with_alerts(alerts)
            .on_trigger(move |alert, _| {
                log.lock().unwrap().push(alert.name.clone());
            });
        let at = |hour| Utc.with_ymd_and_hms(2026, 2, 3, hour, 0, 0).unwrap();
        engine.update(
            "SAMPLE",
            None,
            Timeframe::Min1,
            chart.bars(SeriesId::MAIN),
            at(6),
            |source, bar| chart.alert_value(source, bar),
        );
        assert!(fired.lock().unwrap().is_empty());
        chart.update(
            SeriesId::MAIN,
            Bar::new(at(12), 30.0, 30.0, 30.0, 30.0, 1.0),
        );
        chart.refresh_studies(0.0);
        engine.update(
            "SAMPLE",
            None,
            Timeframe::Min1,
            chart.bars(SeriesId::MAIN),
            at(12),
            |source, bar| chart.alert_value(source, bar),
        );
        assert_eq!(*fired.lock().unwrap(), ["dPOC"]);
    }

    /// A chart of `bars`, taken from `tpo_level_bars`, of 30-minute bars of an instrument
    /// ticking by 1.0, drawn as TPO profiles of one-tick rows that show poor lows and single
    /// prints, with `edit` applied to those settings.
    fn tpo_chart(bars: &[Bar], edit: impl FnOnce(&mut crate::model::ChartTypeParams)) -> Chart {
        let mut chart = Chart::new(BarData::from_bars(bars.to_vec()));
        chart.set_symbol("SYM");
        chart.set_timeframe(Timeframe::Min30);
        chart.set_instrument(Some(
            crate::model::Instrument::new("SYM").with_tick_size(1.0),
        ));
        chart.set_chart_type(ChartType::TimePriceOpportunity);
        let mut params = chart.chart_type_params().clone();
        params.tpo_row_ticks = 1;
        params.tpo_poor_low = true;
        params.tpo_single_prints = true;
        edit(&mut params);
        chart.set_chart_type_params(params);
        chart
    }

    /// The TPO levels `chart` offers alerts, as (profile open, level, price).
    fn tpo_sources(chart: &Chart) -> Vec<(DateTime<Utc>, crate::model::TPOLevel, f64)> {
        chart
            .alert_sources()
            .into_iter()
            .filter_map(|(source, _)| match source {
                AlertSource::TPOLevel(level) => Some((level.profile, level.level, level.price)),
                _ => None,
            })
            .collect()
    }

    /// Guards #4265: the TPO chart type and a TPO study offer the extended levels of completed
    /// profiles that no later bar has reached, each once its switch is on, also for profiles
    /// before or after the view, and none of the forming profile. The levels are hand-calculated
    /// on `tpo_level_bars`; the last bar here, bar 8, forms the third profile.
    #[test]
    fn tpo_extensions_offer_completed_levels_no_later_bar_reached() {
        use crate::model::{ChartTypeParams, LogicalRange, TPOLevel::*};
        let bars = crate::chart::rendering::candles::tpo::tpo_level_bars();
        let bars = &bars[..9];
        let [d0, d1] = [bars[0].time, bars[4].time];
        type Row = (
            fn(&mut ChartTypeParams),
            Vec<(DateTime<Utc>, crate::model::TPOLevel, f64)>,
        );
        let rows: [Row; 7] = [
            (|_| {}, vec![]),
            (
                |p| p.tpo_extend_poc = true,
                vec![(d0, PointOfControl, 102.0), (d1, PointOfControl, 112.0)],
            ),
            (
                |p| p.tpo_extend_vah = true,
                vec![(d0, ValueAreaHigh, 104.0), (d1, ValueAreaHigh, 114.0)],
            ),
            (
                |p| p.tpo_extend_val = true,
                vec![(d0, ValueAreaLow, 101.0), (d1, ValueAreaLow, 111.0)],
            ),
            // Bar 8 trades down to 107, through the second profile's poor low of 110.
            (|p| p.tpo_extend_poor = true, vec![(d0, PoorLow, 100.0)]),
            (
                |p| p.tpo_extend_single_prints = true,
                vec![
                    (d0, SinglePrints, 105.0),
                    (d0, SinglePrints, 106.0),
                    (d1, SinglePrints, 115.0),
                    (d1, SinglePrints, 116.0),
                ],
            ),
            (
                |p| {
                    p.tpo_extend_poc = true;
                    p.tpo_poc = false;
                },
                vec![],
            ),
        ];
        for (row, (edit, expected)) in rows.into_iter().enumerate() {
            assert_eq!(tpo_sources(&tpo_chart(bars, edit)), expected, "row {row}");
        }

        let mut chart = tpo_chart(bars, |p| p.tpo_extend_poc = true);
        let name = chart
            .alert_sources()
            .into_iter()
            .find(|(source, _)| {
                *source == AlertSource::TPOLevel(TPOLevelSource::new(d1, PointOfControl, 112.0))
            })
            .map(|(_, name)| name.to_string());
        assert_eq!(
            name,
            Some(format!(
                "TPO {}: Point of control 112",
                chart.format_time(d1)
            ))
        );
        // The first profile ends before the view.
        assert!(chart.set_visible_logical_range(LogicalRange::new(4.5, 12.5)));
        assert_eq!(chart.state.visible_range().0, 4);
        assert_eq!(
            tpo_sources(&chart),
            [(d0, PointOfControl, 102.0), (d1, PointOfControl, 112.0)]
        );
        // The second profile starts after the view.
        assert!(chart.set_visible_logical_range(LogicalRange::new(-0.5, 2.5)));
        assert!(chart.state.visible_range().1 <= 4);
        assert_eq!(
            tpo_sources(&chart),
            [(d0, PointOfControl, 102.0), (d1, PointOfControl, 112.0)]
        );

        let mut chart = Chart::new(BarData::from_bars(bars.to_vec()));
        chart.set_timeframe(Timeframe::Min30);
        chart.set_instrument(Some(
            crate::model::Instrument::new("SYM").with_tick_size(1.0),
        ));
        let mut study = crate::studies::TimePriceOpportunity::new(30);
        study.set_input("row_ticks", InputValue::Length(1)).unwrap();
        study
            .set_input("extend_poc", InputValue::Toggle(true))
            .unwrap();
        chart.add_indicator(Box::new(study));
        let names: Vec<_> = chart
            .alert_sources()
            .into_iter()
            .filter(|(source, _)| matches!(source, AlertSource::TPOLevel(_)))
            .map(|(_, name)| name.to_string())
            .collect();
        assert_eq!(
            tpo_sources(&chart),
            [(d0, PointOfControl, 102.0), (d1, PointOfControl, 112.0)]
        );
        assert_eq!(
            names[0],
            format!(
                "Time Price Opportunity {}: Point of control 102",
                chart.format_time(d0)
            )
        );
    }

    /// Guards #4265: an alert crossing the extended point of control of a completed profile
    /// keeps the price the level had when chosen. It fires after the profiles are merged, when
    /// the chart draws no such level, after a restart with no chart to resolve it, and on a bar
    /// whose range reaches the level but closes back below it.
    #[cfg(feature = "alert-engine")]
    #[test]
    fn a_tpo_level_alert_fires_at_its_kept_price() {
        use crate::model::{Alert, AlertComparison, AlertCondition, TPOLevel};
        use crate::services::{AlertEngine, AlertEvent};
        use std::sync::{Arc, Mutex};
        let bars = crate::chart::rendering::candles::tpo::tpo_level_bars();
        let [d0, d1] = [bars[0].time, bars[4].time];
        let source =
            AlertSource::TPOLevel(TPOLevelSource::new(d1, TPOLevel::PointOfControl, 112.0));
        // Bar 8 closes at 108, below the point of control of 112, and bar 9 trades up to 113.
        for (merge, restart, close) in [
            (false, false, 113.0),
            (true, false, 113.0),
            (false, true, 113.0),
            (false, false, 108.0),
        ] {
            let mut chart = tpo_chart(&bars[..9], |p| p.tpo_extend_poc = true);
            assert!(chart.alert_sources().iter().any(|(s, _)| *s == source));
            if merge {
                let mut params = chart.chart_type_params().clone();
                params.tpo_composites = vec![d0..=d1];
                chart.set_chart_type_params(params);
                assert!(!chart.alert_sources().iter().any(|(s, _)| *s == source));
                assert_eq!(chart.alert_value(&source, 8), Some(112.0));
            }
            let mut alert = Alert::new(
                "SYM",
                AlertCondition::Compare {
                    other: source.clone(),
                    comparison: AlertComparison::Crossing,
                },
            );
            alert.timeframe = Some(Timeframe::Min30);
            if restart {
                alert = serde_json::from_str(&serde_json::to_string(&alert).unwrap()).unwrap();
            }
            let fired = Arc::new(Mutex::new(Vec::new()));
            let log = fired.clone();
            let engine = AlertEngine::new()
                .with_alerts(vec![alert])
                .on_trigger(move |_, event| {
                    if let AlertEvent::TriggeredForSymbol { price, .. } = event {
                        log.lock().unwrap().push(*price);
                    }
                });
            let feed = |chart: &Chart| {
                let shown = chart.bars(SeriesId::MAIN);
                engine.update(
                    "SYM",
                    None,
                    Timeframe::Min30,
                    shown,
                    shown[shown.len() - 1].time,
                    |source, bar| {
                        if restart {
                            None
                        } else {
                            chart.alert_value(source, bar)
                        }
                    },
                );
            };
            feed(&chart);
            chart.update(
                SeriesId::MAIN,
                Bar::new(bars[9].time, 108.0, 113.0, 108.0, close, 1.0),
            );
            feed(&chart);
            assert_eq!(
                *fired.lock().unwrap(),
                [Some(close)],
                "merge {merge}, restart {restart}, close {close}"
            );
        }
    }

    /// Fork medians and tines use anchor construction, and regression fits the source prices of
    /// the supplied bars.
    #[test]
    fn fork_and_regression_sources_follow_edits_and_saved_geometry() {
        // Opens 100, 102, 104 and 106; each bar's high and low are its open and close.
        let bars = [100.0, 104.0, 102.0, 108.0]
            .into_iter()
            .enumerate()
            .map(|(i, close)| {
                let open = 100.0 + 2.0 * i as f64;
                Bar::new(
                    DateTime::from_timestamp(i as i64 * 60, 0).unwrap(),
                    open,
                    open.max(close),
                    open.min(close),
                    close,
                    1.0,
                )
            })
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        chart.set_symbol("SAMPLE");
        // A=(0,100), B=(1,200), C=(2,50), midpoint BC=(1.5,125).
        // Standard slope=50/3, Schiff=-50/3, modified Schiff=-25, inside=-200/3.
        for (tool, expected) in [
            (
                DrawingToolType::Pitchfork,
                [150.0, 700.0 / 3.0, 200.0 / 3.0],
            ),
            (
                DrawingToolType::SchiffPitchfork,
                [100.0, 500.0 / 3.0, 100.0 / 3.0],
            ),
            (
                DrawingToolType::ModifiedSchiffPitchfork,
                [87.5, 150.0, 25.0],
            ),
            (
                DrawingToolType::InsidePitchfork,
                [25.0, 200.0 / 3.0, -50.0 / 3.0],
            ),
            (DrawingToolType::Pitchfan, [150.0, 400.0, 25.0]),
        ] {
            for scale in [
                crate::scales::PriceScaleMode::Normal,
                crate::scales::PriceScaleMode::Logarithmic,
            ] {
                chart.config_mut().right_axis_scale_mode = scale;
                let log = scale == crate::scales::PriceScaleMode::Logarithmic;
                let expected = expected.map(|v| if log { (v / 100.0_f64).exp() } else { v });
                let mut drawing = Drawing::new(7, tool);
                drawing.stored_id = Some("fork#saved".into());
                drawing.chart_points = vec![
                    ChartPoint::new(0.0, 100.0),
                    ChartPoint::new(1.0, 200.0),
                    ChartPoint::new(2.0, 50.0),
                ];
                if log {
                    for point in &mut drawing.chart_points {
                        point.price = (point.price / 100.0).exp();
                    }
                }
                drawing.pitchfork = None;
                drawing.completed = true;
                chart.drawings_mut().drawings = vec![drawing];
                let drawing = &chart.drawings().drawings[0];
                // Legacy levels are 0, 0.5, 1, 1.5. Each setting owns two stable slots.
                let sources = [
                    drawing.alert_source(0),
                    drawing.alert_source(4),
                    drawing.alert_source(5),
                ];
                for (source, expected) in sources.iter().zip(expected) {
                    let got = chart.alert_value(source, 3).unwrap();
                    assert!(
                        (got - expected).abs() < 1e-9,
                        "{tool:?}: {source:?}: {got} != {expected}"
                    );
                    assert!(chart.alert_sources().iter().any(|(s, _)| s == source));
                }
                #[cfg(feature = "ui")]
                {
                    let mut menu = crate::ui::context_menu::drawing::DrawingContextMenu::new();
                    menu.open_on(&egui::Context::default(), &chart, egui::Pos2::ZERO, 7);
                    assert_eq!(
                        menu.alert.unwrap().condition,
                        crate::model::AlertCondition::Compare {
                            other: sources[0].clone(),
                            comparison: crate::model::AlertComparison::Crossing
                        }
                    );
                    let mut config = crate::drawings::PitchforkConfig::legacy();
                    config.levels[0].visible = false;
                    chart.drawings_mut().drawings[0].pitchfork = Some(config);
                    assert_eq!(chart.alert_value(&sources[0], 3), None);
                    assert!(
                        (chart.alert_value(&sources[1], 3).unwrap() - expected[1]).abs() < 1e-9
                    );
                    let config = chart.drawings_mut().drawings[0].pitchfork.as_mut().unwrap();
                    config.levels[0].visible = true;
                    config.extend_lines = false;
                    for (i, source) in sources.iter().enumerate() {
                        let visible = tool == DrawingToolType::Pitchfan
                            || i == 2
                            || (i == 0 && tool == DrawingToolType::InsidePitchfork);
                        let got = chart.alert_value(source, 3);
                        if visible {
                            assert!(
                                (got.unwrap() - expected[i]).abs() < 1e-9,
                                "{tool:?}: finite endpoint"
                            );
                        } else {
                            assert_eq!(got, None, "{tool:?}: past finite endpoint");
                        }
                    }
                    chart.drawings_mut().drawings[0].pitchfork = None;
                    let stored = crate::drawings::persistence::drawing_to_stored(
                        &chart.drawings().drawings[0],
                        |i| (i * 60_000.0) as i64,
                    );
                    let saved = serde_json::to_string(&stored).unwrap();
                    let restored = serde_json::from_str(&saved).unwrap();
                    chart.drawings_mut().drawings[0] =
                        crate::drawings::persistence::stored_to_drawing(
                            &restored,
                            |t| t as f32 / 60_000.0,
                            99,
                        );
                    assert!(chart.alert_value(&sources[0], 3).is_some());
                    chart.drawings_mut().drawings[0].chart_points[0].price += 10.0;
                    assert_ne!(chart.alert_value(&sources[0], 3).unwrap(), expected[0]);
                    for point in &mut chart.drawings_mut().drawings[0].chart_points {
                        point.bar_idx = 0.0;
                    }
                    assert_eq!(chart.alert_value(&sources[0], 3), None);
                    chart.drawings_mut().drawings.clear();
                    assert_eq!(chart.alert_value(&sources[0], 3), None);
                }
            }
        }
        chart.config_mut().right_axis_scale_mode = crate::scales::PriceScaleMode::Normal;
        // y=[100,104,102,108], x=[0,1,2,3]: mean=103.5, Sxx=5, Sxy=11.
        // slope=2.2, intercept=100.2, residual SS=10.8, sigma=sqrt(10.8/3).
        let mut regression = Drawing::new(8, DrawingToolType::RegressionTrend);
        regression.chart_points = vec![ChartPoint::new(0.0, 999.0), ChartPoint::new(3.0, 1.0)];
        regression.completed = true;
        chart.drawings_mut().drawings = vec![regression];
        let drawing = &chart.drawings().drawings[0];
        for (level, expected) in [
            (0, 106.8),
            (1, 106.8 + 2.0 * 3.6_f64.sqrt()),
            (2, 106.8 - 2.0 * 3.6_f64.sqrt()),
        ] {
            let got = chart.alert_value(&drawing.alert_source(level), 3).unwrap();
            assert!(
                (got - expected).abs() < 1e-9,
                "regression {level}: {got} != {expected}"
            );
        }
        // Without their deviations the bands pass through the high and the low furthest from the
        // line: 1.6 above and 2.6 below the closes' line, 2 either side of the opens' y=100+2x.
        let mut config = crate::drawings::RegressionTrendConfig::default();
        config.use_upper = false;
        config.use_lower = false;
        for (source, levels) in [
            (crate::model::PriceSource::Close, [106.8, 108.4, 104.2]),
            (crate::model::PriceSource::Open, [106.0, 108.0, 104.0]),
        ] {
            config.source = source;
            chart.drawings_mut().drawings[0].regression_trend = Some(config);
            for (level, expected) in levels.into_iter().enumerate() {
                let alert = chart.drawings().drawings[0].alert_source(level);
                let got = chart.alert_value(&alert, 3).unwrap();
                assert!(
                    (got - expected).abs() < 1e-9,
                    "{source:?} {level}: {got} != {expected}"
                );
            }
        }
        chart.drawings_mut().drawings[0].regression_trend = None;
        #[cfg(feature = "ui")]
        {
            let mut menu = crate::ui::context_menu::drawing::DrawingContextMenu::new();
            menu.open_on(&egui::Context::default(), &chart, egui::Pos2::ZERO, 8);
            assert_eq!(
                menu.alert.unwrap().condition,
                crate::model::AlertCondition::Compare {
                    other: AlertSource::Drawing("8".into()),
                    comparison: crate::model::AlertComparison::Crossing
                }
            );
        }
        chart.drawings_mut().drawings[0].chart_points[1].bar_idx = 2.0;
        assert!(
            (chart
                .alert_value(&AlertSource::Drawing("8".into()), 3)
                .unwrap()
                - 104.0)
                .abs()
                < 1e-9
        );
        chart.drawings_mut().drawings[0].chart_points[1].bar_idx = 3.0;
        chart.config_mut().right_axis_scale_mode = crate::scales::PriceScaleMode::Logarithmic;
        let got = chart
            .alert_value(&AlertSource::Drawing("8".into()), 1)
            .unwrap();
        let expected = (100.2_f64.powi(2) * 106.8).cbrt();
        assert!(
            (got - expected).abs() < 1e-9,
            "log regression interpolates its painted endpoints"
        );
        let drawing = &mut chart.drawings_mut().drawings[0];
        drawing.chart_points[1].bar_idx = 0.0;
        assert_eq!(
            chart.alert_value(&AlertSource::Drawing("8".into()), 1),
            None
        );
    }

    /// An anchored VWAP offers its line and the lines of its enabled bands as the values painted
    /// at a bar, from the bar its saved anchor time names.
    #[test]
    fn anchored_vwap_sources_follow_the_painted_lines_from_the_saved_anchor() {
        let time = |minute: i64| DateTime::from_timestamp(minute * 60, 0).unwrap();
        let flat =
            |minute, close, volume| Bar::new(time(minute), close, close, close, close, volume);
        let bars: Vec<_> = [(100.0, 1.0), (104.0, 2.0), (102.0, 3.0), (108.0, 4.0)]
            .into_iter()
            .zip(0..)
            .map(|((close, volume), minute)| flat(minute, close, volume))
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars.clone()));
        chart.set_symbol("SAMPLE");
        let mut vwap = Drawing::new(8, DrawingToolType::AnchoredVWAP);
        vwap.chart_points = vec![ChartPoint::new(1.0, 0.0)];
        vwap.completed = true;
        chart.drawings_mut().drawings.push(vwap);
        let source = |level| {
            AlertSource::Drawing(if level == 0 {
                "8".into()
            } else {
                format!("8#{level}")
            })
        };
        // The typical price of a flat bar is its close. From the anchor, bar 3 weighs closes
        // 104, 102 and 108 by volumes 2, 3 and 4: 946 / 9, with a weighted variance of
        // 99500 / 9 - (946 / 9)^2 = 584 / 81. The default first band lies one deviation away.
        let (mean, sigma) = (946.0 / 9.0, 584.0_f64.sqrt() / 9.0);
        for (level, bar, expected) in [
            (0, 3, Some(mean)),
            (1, 3, Some(mean + sigma)),
            (2, 3, Some(mean - sigma)),
            (0, 2, Some(514.0 / 5.0)),
            (0, 0, None),
            (3, 3, None),
        ] {
            let got = chart.alert_value(&source(level), bar);
            assert!(
                got.zip(expected)
                    .map_or(got == expected, |(got, want)| (got - want).abs() < 1e-9),
                "level {level} at bar {bar}: {got:?} != {expected:?}"
            );
        }
        let offered: Vec<_> = chart.alert_sources().into_iter().map(|(s, _)| s).collect();
        assert_eq!(offered, [source(0), source(1), source(2)]);
        #[cfg(feature = "ui")]
        {
            let mut menu = crate::ui::context_menu::drawing::DrawingContextMenu::new();
            menu.open_on(&egui::Context::default(), &chart, egui::Pos2::ZERO, 8);
            assert_eq!(
                menu.alert.unwrap().condition,
                crate::model::AlertCondition::Compare {
                    other: source(0),
                    comparison: crate::model::AlertComparison::Crossing
                }
            );
        }
        // Saved by time and restored on a series that gained an older bar, it still starts at
        // the bar of 104.
        let stored =
            crate::drawings::persistence::drawing_to_stored(&chart.drawings().drawings[0], |i| {
                (i * 60_000.0) as i64
            });
        let restored = crate::drawings::persistence::stored_to_drawing(
            &stored,
            |t| t as f32 / 60_000.0 + 1.0,
            9,
        );
        let mut longer = vec![flat(-1, 50.0, 10.0)];
        longer.extend(bars);
        let levels =
            restored.alert_levels_with_bars(4.0, crate::scales::PriceScaleMode::Normal, &longer);
        assert!((levels[0].2 - mean).abs() < 1e-9, "{levels:?}");
    }

    /// A chart in market replay lends no recorded bars to alerts, as in candle replay, and lends
    /// its live bars again after the exit.
    #[cfg(feature = "alert-engine")]
    #[test]
    fn market_replay_lends_no_alert_data() {
        use crate::data::{MarketEvent, ReplayDriver, ReplayEvent};
        use crate::model::{Instrument, Tick, Timeframe};
        use std::sync::Arc;
        let minute = |m: i64| DateTime::from_timestamp(60 * m, 0).unwrap();
        let driver = || {
            let events = (0..3)
                .map(|m| {
                    ReplayEvent::new(
                        "A",
                        minute(m),
                        m as u64,
                        MarketEvent::Trade(Tick::new(minute(m), 10.0, 1.0, None)),
                    )
                })
                .collect();
            Arc::new(
                ReplayDriver::new(events, vec![Instrument::new("A").with_tick_size(1.0)]).unwrap(),
            )
        };
        let live = driver();
        live.seek(minute(2));
        let mut chart = crate::widget::ChartBuilder::new()
            .with_feed(live)
            .with_symbol("A")
            .with_timeframe(Timeframe::Min1)
            .build();
        let ctx = egui::Context::default();
        let lent = |chart: &mut Chart| {
            for _ in 0..4 {
                chart.pump(&ctx);
            }
            chart.alert_data().map(|data| data.bars.len())
        };
        assert_eq!(lent(&mut chart), Some(3), "live");
        let replay = driver();
        replay.set_speed(0.0).unwrap();
        replay.seek(minute(1));
        chart.set_market_replay(Some(replay));
        assert_eq!(lent(&mut chart), None, "market replay");
        assert_eq!(chart.bars(SeriesId::MAIN).len(), 2);
        chart.set_market_replay(None);
        assert_eq!(lent(&mut chart), Some(3), "after the exit");
    }
}
