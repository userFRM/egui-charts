//! Market breadth from two host-supplied symbol histories.
//!
//! The ratio is advancing issues divided by declining issues. The line is the running
//! total of advancing minus declining issues. The volume index applies that same running
//! difference to advancing and declining volume. Each input bar's close carries the count
//! or volume, not the trading volume of an index instrument. Missing pairs and zero ratio
//! denominators have no value; a missing pair leaves the running total unchanged.

use crate::model::Bar;
use crate::studies::{
    Indicator, IndicatorValues, InputKind, InputValue, StudyData, StudyDataRequest, StudyInput,
};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Advancing issues divided by declining issues.
pub type AdvanceDeclineRatio = MarketBreadth<true, false>;
/// Running total of advancing minus declining issues.
pub type AdvanceDeclineLine = MarketBreadth<false, false>;
/// Running total of advancing volume minus declining volume.
pub type CumulativeVolumeIndex = MarketBreadth<false, true>;

/// A ratio or cumulative difference of two market series. `VOLUME` chooses volume-series
/// defaults and the volume-index name. Hosts map the default symbol keys to their own source,
/// or replace them using the typed inputs. Calculations require [`Indicator::update_data`].
#[derive(Clone)]
pub struct MarketBreadth<const RATIO: bool, const VOLUME: bool> {
    advancing_symbol: String,
    declining_symbol: String,
    timeframe: Option<crate::model::Timeframe>,
    clock: Option<crate::model::BarClock>,
    wait_for_close: bool,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl<const RATIO: bool, const VOLUME: bool> Default for MarketBreadth<RATIO, VOLUME> {
    fn default() -> Self {
        Self {
            advancing_symbol: if VOLUME {
                "ADVANCING_VOLUME"
            } else {
                "ADVANCING_ISSUES"
            }
            .into(),
            declining_symbol: if VOLUME {
                "DECLINING_VOLUME"
            } else {
                "DECLINING_ISSUES"
            }
            .into(),
            timeframe: None,
            clock: None,
            wait_for_close: true,
            values: IndicatorValues::new(),
            color: DESIGN_TOKENS.semantic.indicators.ma,
            visible: true,
        }
    }
}

impl<const RATIO: bool, const VOLUME: bool> Indicator for MarketBreadth<RATIO, VOLUME> {
    fn name(&self) -> &str {
        if RATIO {
            "Advance/Decline Ratio"
        } else if VOLUME {
            "Cumulative Volume Index"
        } else {
            "Advance/Decline Line"
        }
    }
    fn desc(&self) -> &str {
        "Market breadth from host-supplied advancing and declining series"
    }
    fn inputs(&self) -> Vec<StudyInput> {
        let defaults = Self::default();
        vec![
            StudyInput::new(
                "advancing_symbol",
                "Advancing symbol",
                InputValue::Symbol(self.advancing_symbol.clone()),
                InputKind::Symbol,
            )
            .with_default(InputValue::Symbol(defaults.advancing_symbol)),
            StudyInput::new(
                "declining_symbol",
                "Declining symbol",
                InputValue::Symbol(self.declining_symbol.clone()),
                InputKind::Symbol,
            )
            .with_default(InputValue::Symbol(defaults.declining_symbol)),
            StudyInput::new(
                "timeframe",
                "Interval",
                InputValue::Timeframe(self.timeframe),
                InputKind::Timeframe,
            )
            .with_default(InputValue::Timeframe(None)),
            StudyInput::new(
                "wait_for_close",
                "Wait for interval close",
                InputValue::Toggle(self.wait_for_close),
                InputKind::Toggle,
            )
            .with_default(InputValue::Toggle(true)),
        ]
    }
    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), &'static str> {
        let schema = self
            .inputs()
            .into_iter()
            .find(|input| input.id == id)
            .ok_or("unknown study input")?;
        if !schema.accepts(&value) {
            return Err("invalid breadth input");
        }
        match (id, value) {
            ("advancing_symbol", InputValue::Symbol(value)) => self.advancing_symbol = value,
            ("declining_symbol", InputValue::Symbol(value)) => self.declining_symbol = value,
            ("timeframe", InputValue::Timeframe(value)) => self.timeframe = value,
            ("wait_for_close", InputValue::Toggle(value)) => self.wait_for_close = value,
            _ => return Err("invalid breadth input"),
        }
        self.values.clear();
        Ok(())
    }
    fn data_requests(&self) -> Vec<StudyDataRequest> {
        [&self.advancing_symbol, &self.declining_symbol]
            .into_iter()
            .map(|symbol| StudyDataRequest {
                symbol: symbol.clone(),
                timeframe: self.timeframe,
            })
            .collect()
    }
    fn set_bar_clock(&mut self, clock: &crate::model::BarClock) -> bool {
        let changed = self.clock.as_ref() != Some(clock);
        self.clock = Some(clock.clone());
        changed
    }
    fn calculate(&mut self, data: &[Bar]) {
        self.values = IndicatorValues::missing(data.len());
    }
    fn update_data(&mut self, data: &[Bar], _from: usize, sources: &[StudyData<'_>]) {
        self.calculate(data);
        let requests = self.data_requests();
        let Some(advancing) = sources.iter().find(|source| source.request == requests[0]) else {
            return;
        };
        let Some(declining) = sources.iter().find(|source| source.request == requests[1]) else {
            return;
        };
        let mut total = 0.0;
        let values: Vec<_> = advancing
            .bars
            .iter()
            .map(|bar| {
                let other = declining
                    .bar_at(bar.time)
                    .map(|at| declining.bars[at].close)?;
                if !bar.close.is_finite() || !other.is_finite() || (RATIO && other == 0.0) {
                    return None;
                }
                let value = if RATIO {
                    bar.close / other
                } else {
                    total + bar.close - other
                };
                if !value.is_finite() {
                    return None;
                }
                if !RATIO {
                    total = value;
                }
                Some(value)
            })
            .collect();
        self.values.clear();
        let clock = self.clock.as_ref().unwrap_or(&advancing.clock);
        for (at, bar) in data.iter().enumerate() {
            let value = advancing
                .bar_at(bar.time)
                .and_then(|_| {
                    let forming_higher = at + 1 == data.len()
                        && self.wait_for_close
                        && clock
                            .timeframe()
                            .is_some_and(|tf| advancing.timeframe.duration_ms() > tf.duration_ms());
                    let close = if forming_higher {
                        bar.time
                    } else {
                        clock
                            .next_open(clock.bar_open(bar.time))
                            .unwrap_or(bar.time)
                    };
                    advancing.output_at(close, !self.wait_for_close && at + 1 == data.len())
                })
                .and_then(|at| values[at])
                .unwrap_or(f64::NAN);
            self.values.push_lines(&[value]);
        }
    }
    fn values(&self) -> &IndicatorValues {
        &self.values
    }
    fn colors(&self) -> Vec<Color32> {
        vec![self.color]
    }
    fn set_colors(&mut self, colors: Vec<Color32>) {
        if let Some(color) = colors.first() {
            self.color = *color;
        }
    }
    fn is_overlay(&self) -> bool {
        false
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
    use crate::model::{BarClock, Timeframe};
    use chrono::{DateTime, Duration};

    #[test]
    fn breadth_uses_paired_market_values_and_skips_missing_observations() {
        let bars = |values: &[Option<f64>]| {
            values
                .iter()
                .enumerate()
                .filter_map(|(at, value)| {
                    let value = (*value)?;
                    Some(Bar::new(
                        DateTime::UNIX_EPOCH + Duration::days(at as i64),
                        value,
                        value,
                        value,
                        value,
                        999.0,
                    ))
                })
                .collect::<Vec<_>>()
        };
        let chart = bars(&[Some(1.0); 5]);
        for (name, advancing, declining, expected) in [
            (
                "Advance/Decline Ratio",
                [Some(10.0), Some(14.0), Some(8.0), None, Some(5.0)],
                [Some(4.0), Some(7.0), Some(0.0), Some(3.0), Some(10.0)],
                [Some(2.5), Some(2.0), None, None, Some(0.5)],
            ),
            (
                "Advance/Decline Line",
                [Some(10.0), Some(14.0), Some(8.0), None, Some(5.0)],
                [Some(4.0), Some(7.0), Some(0.0), Some(3.0), Some(10.0)],
                [Some(6.0), Some(13.0), Some(21.0), None, Some(16.0)],
            ),
            (
                "Cumulative Volume Index",
                [Some(30.0), Some(70.0), Some(50.0), None, Some(20.0)],
                [Some(10.0), Some(20.0), Some(0.0), Some(5.0), Some(5.0)],
                [Some(20.0), Some(70.0), Some(120.0), None, Some(135.0)],
            ),
        ] {
            for stamp in [0, 6] {
                let mut study = crate::studies::IndicatorFactory::new()
                    .create(name)
                    .unwrap();
                let requests = study.data_requests();
                let mut advancing = bars(&advancing);
                let mut declining = bars(&declining);
                for bar in &mut advancing {
                    bar.time += Duration::hours(stamp);
                }
                for bar in &mut declining {
                    bar.time += Duration::hours(stamp * 2);
                }
                let sources: Vec<_> = requests
                    .into_iter()
                    .zip([&advancing, &declining])
                    .map(|(request, bars)| StudyData {
                        request,
                        bars,
                        clock: BarClock::new(Timeframe::Day1),
                        instrument: None,
                        timeframe: Timeframe::Day1,
                    })
                    .collect();
                study.calculate(&chart);
                assert!((0..chart.len()).all(|at| study.values().value(0, at).is_none()));
                study.set_bar_clock(&BarClock::new(Timeframe::Day1));
                study.update_data(&chart, 0, &sources);
                let actual: Vec<_> = (0..chart.len())
                    .map(|at| study.values().value(0, at))
                    .collect();
                assert_eq!(actual, expected, "{name}");
                let half_days: Vec<_> = (0..11)
                    .map(|at| {
                        Bar::new(
                            DateTime::UNIX_EPOCH + Duration::hours(at * 12),
                            1.0,
                            1.0,
                            1.0,
                            1.0,
                            1.0,
                        )
                    })
                    .collect();
                study.set_bar_clock(&BarClock::new("12h".parse().unwrap()));
                study.update_data(&half_days, 0, &sources);
                // Daily values become available on the second half-day; the missing day stays empty.
                let aligned = [
                    None,
                    expected[0],
                    expected[0],
                    expected[1],
                    expected[1],
                    expected[2],
                    None,
                    None,
                    expected[2],
                    expected[4],
                    None,
                ];
                let actual: Vec<_> = (0..11).map(|at| study.values().value(0, at)).collect();
                assert_eq!(actual, aligned, "{name} confirmed intervals");
                study.update_data(&half_days[..10], 0, &sources);
                assert_eq!(
                    study.values().value(0, 9),
                    expected[2],
                    "{name} forming chart bar"
                );
                study
                    .set_input("wait_for_close", InputValue::Toggle(false))
                    .unwrap();
                study.update_data(&half_days[..9], 0, &sources);
                assert_eq!(
                    study.values().value(0, 8),
                    expected[4],
                    "{name} developing interval"
                );
                assert_eq!(study.values().value(0, 0), None);
            }
        }
    }
}
