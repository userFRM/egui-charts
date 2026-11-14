//! The values and sources an alert can test on a chart.

use crate::Chart;
use crate::chart::series::SeriesId;
use crate::model::AlertSource;

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
        request.adjusted = self.dividend_adjusted();
        request.conversion = self.conversion().map(str::to_owned);
        Some(crate::services::AlertData {
            request,
            bars: self.bars(SeriesId::MAIN),
            history_complete: self.feed.history_complete,
            #[cfg(feature = "ui")]
            drawings: &self.drawings().drawings,
        })
    }

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
                    d.alert_levels_with_bars(bar as f64, scale, self.bars(SeriesId::MAIN))
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
                let levels = d.alert_levels_with_bars(bar, scale, self.bars(SeriesId::MAIN));
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
    /// Fork medians and tines use anchor construction, and regression fits the supplied closes.
    #[test]
    fn fork_and_regression_sources_follow_edits_and_saved_geometry() {
        let bars = [100.0, 104.0, 102.0, 108.0]
            .into_iter()
            .enumerate()
            .map(|(i, close)| {
                Bar::new(
                    DateTime::from_timestamp(i as i64 * 60, 0).unwrap(),
                    close,
                    close,
                    close,
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
