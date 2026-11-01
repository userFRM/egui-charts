//! 24-hour Volume
//!
//! The volume traded over the last 24 hours at each bar, whatever the chart interval: a
//! rolling sum over a fixed span of calendar time rather than a number of bars, so a
//! 1-minute and a 1-hour chart of the same market show the same turnover.
//!
//! # Calculation
//! On an interval that divides a day,
//!
//! Volume24h(t) = Sum(volume of the bars that open in (t - 24 h, t])
//!
//! where `t` is the open time of the current bar. A bar that opens exactly 24 hours before
//! the current one belongs to the previous day, so the window holds exactly one day of bars,
//! and on daily bars it holds the bar alone.
//!
//! Bars longer than a day, or of an interval that does not divide a day, cannot be split into
//! the last 24 hours, so on those intervals the study requests lower-interval candles through
//! [`Indicator::data_requests`]: one hour above a day, one minute below it and one second below
//! a minute, or one second on an interval that is not a whole number of those. Each bar then sums the candles that open in the 24 hours before the close of its
//! last candle: the bar's own close when trading runs to its end, and otherwise its last
//! trading hours, so a weekly bar of a market closed at the weekend sums its last trading day.
//! A bar without candles has no value. Until candles are supplied the study sums chart bars,
//! which has no value on bars longer than a day.
//!
//! The Units input defaults to supplied volume. Quote currency assumes the host supplies
//! volume in base units and sums `volume * close` for each contributing bar, never the
//! total volume times the chart bar's close. This is an estimate from candles, not execution
//! turnover. The host must supply [`crate::model::Instrument::currency`] to identify the quote
//! currency; already quoted volume or contract counts must use supplied units. Negative base
//! volume or a non-finite volume, close or product makes a quote window unavailable until that
//! bar leaves it. Quote readouts end with the currency code, and legends, tooltips and exports name
//! the selected units.
//!
//! # Example
//! ```
//! use chrono::{TimeZone, Utc};
//! use egui_charts::model::Bar;
//! use egui_charts::studies::{Indicator, Volume24h};
//!
//! // Hourly bars of 10 units: from the 25th bar on, the oldest bar leaves the window.
//! let bars: Vec<Bar> = (0..30)
//!     .map(|h| Bar::new(Utc.timestamp_opt(h * 3_600, 0).unwrap(), 1.0, 1.0, 1.0, 1.0, 10.0))
//!     .collect();
//! let mut study = Volume24h::new();
//! study.calculate(&bars);
//! assert_eq!(study.values().value(0, 0), Some(10.0));
//! assert_eq!(study.values().value(0, 23), Some(240.0));
//! assert_eq!(study.values().value(0, 29), Some(240.0));
//! ```

use super::lower_interval;
use crate::model::{Bar, BarClock, Timeframe};
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{
    Indicator, IndicatorValue, IndicatorValues, InputKind, InputValue, StudyCell, StudyData,
    StudyDataRequest, StudyInput, StudyTable,
};
use chrono::{DateTime, TimeDelta, Utc};
use egui::Color32;
use std::collections::VecDeque;

/// Rolling volume over the last 24 hours of calendar time, in a separate pane.
///
/// The `units` choice input is 0 for supplied volume (default), 1 for quote currency.
/// Quote mode requires base-unit volumes and the instrument's price currency metadata.
///
/// In supplied units a non-finite volume has no value and adds nothing; in quote units it
/// makes its entire window unavailable. The chart supplies its
/// interval through [`Indicator::set_bar_clock`] and the candles through
/// [`Indicator::update_data`]; a direct caller on bars longer than a day passes a clock of that
/// interval, without which every bar is summed as an intraday bar.
#[derive(Clone)]
pub struct Volume24h {
    values: IndicatorValues,
    stepper: Stepper<State>,
    /// The chart clock, from `set_bar_clock`.
    clock: BarClock,
    /// Whether `values` were summed from lower-interval candles rather than chart bars.
    from_candles: bool,
    quote_units: bool,
    currency: Option<String>,
    precision: Option<usize>,
    /// The report shown when quote units have no latest value.
    table: Option<StudyTable>,
    color: Color32,
    visible: bool,
}

#[derive(Clone, Default)]
struct State {
    /// Open time and amount of each bar in the window, oldest first; None is invalid quote data.
    window: VecDeque<(DateTime<Utc>, Option<f64>)>,
    sum: f64,
    invalid: usize,
}

impl Volume24h {
    /// A 24-hour volume study.
    pub fn new() -> Self {
        Self {
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            clock: BarClock::new(Timeframe::Min1),
            from_candles: false,
            quote_units: false,
            currency: None,
            precision: None,
            table: None,
            color: palette::BLUE,
            visible: true,
        }
    }

    fn clear(&mut self) {
        self.values.clear();
        self.stepper = Stepper::default();
        self.from_candles = false;
        self.update_table();
    }

    fn amount(bar: &Bar, quote_units: bool) -> Option<f64> {
        let amount = if quote_units {
            if !bar.close.is_finite() || bar.volume < 0.0 {
                return None;
            }
            bar.volume * bar.close
        } else {
            bar.volume
        };
        amount.is_finite().then_some(amount)
    }

    fn unit(&self) -> &str {
        if self.quote_units {
            self.currency
                .as_deref()
                .unwrap_or("quote currency unavailable")
        } else {
            "supplied units"
        }
    }

    fn update_table(&mut self) {
        let unavailable = self
            .values
            .len()
            .checked_sub(1)
            .is_some_and(|i| self.quote_units && self.values.value(0, i).is_none());
        self.table = unavailable.then(|| StudyTable {
            title: "24-hour Volume".into(),
            columns: vec![self.line_names().remove(0)],
            rows: vec![vec![StudyCell::Text(if self.currency.is_none() {
                "Quote currency unavailable".into()
            } else {
                "Quote amount unavailable".into()
            })]],
        });
    }

    /// The lower interval read where chart bars cannot hold exactly the last 24 hours: on
    /// intervals longer than a day or that do not divide it.
    fn lower_timeframe(&self) -> Option<Timeframe> {
        let span = self.clock.timeframe()?.duration_ms();
        let day = Timeframe::Day1.duration_ms();
        if span > 0 && span <= day && day % span == 0 {
            return None;
        }
        lower_interval::lower_timeframe(&self.clock, None)
    }

    /// Sums, for each bar of `data[from..]`, the candles that open in the 24 hours before the
    /// close of its last candle.
    fn sum_candles(&mut self, data: &[Bar], from: usize, lower: Timeframe, candles: &[Bar]) {
        let from = if self.from_candles { from } else { 0 };
        self.from_candles = true;
        if from == 0 {
            self.values = IndicatorValues::missing(data.len());
        } else {
            self.values.truncate(from);
            self.values.resize(data.len(), IndicatorValue::None);
        }
        let ranges = lower_interval::nest(&self.clock, lower, data, candles, from);
        for (i, range) in (from..).zip(ranges) {
            if range.is_empty() || (!self.quote_units && !data[i].close.is_finite()) {
                continue;
            }
            let last = range.end - 1;
            let end = candles[last]
                .time
                .checked_add_signed(lower.duration())
                .unwrap_or(DateTime::<Utc>::MAX_UTC);
            let start = end
                .checked_sub_signed(TimeDelta::days(1))
                .unwrap_or(DateTime::<Utc>::MIN_UTC);
            let first = candles[..last].partition_point(|candle| candle.time < start);
            // shortcut: sums each window afresh, up to 86,400 candles a bar on one-second
            // candles (intervals of a few seconds that do not divide the day); slide the window
            // if such charts become slow.
            let sum = if self.quote_units {
                candles[first..range.end]
                    .iter()
                    .map(|candle| Self::amount(candle, true))
                    .sum::<Option<f64>>()
                    .filter(|sum| sum.is_finite())
                    .filter(|_| self.currency.is_some())
            } else {
                Some(
                    candles[first..range.end]
                        .iter()
                        .map(|candle| candle.volume)
                        .sum(),
                )
            };
            if let Some(sum) = sum {
                self.values.set_value(0, i, sum);
            }
        }
        self.update_table();
    }
}

impl Default for Volume24h {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for Volume24h {
    fn inputs(&self) -> Vec<StudyInput> {
        vec![
            StudyInput::new(
                "units",
                "Units",
                InputValue::Choice(usize::from(self.quote_units)),
                InputKind::Choice(vec![
                    "Supplied volume".into(),
                    "Quote currency (volume in base units)".into(),
                ]),
            )
            .with_default(InputValue::Choice(0)),
        ]
    }

    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), &'static str> {
        if id != "units" {
            return Err("unknown study input");
        }
        let InputValue::Choice(units @ 0..=1) = value else {
            return Err("study input is outside its accepted values");
        };
        self.quote_units = units == 1;
        self.clear();
        Ok(())
    }

    fn name(&self) -> &str {
        "24-hour Volume"
    }

    fn desc(&self) -> &str {
        "24-hour Volume - Volume traded over the last 24 hours, on any interval"
    }

    fn value_format(&self) -> crate::scales::StudyFormat {
        if self.quote_units {
            crate::scales::StudyFormat::Fixed(2)
        } else {
            crate::scales::StudyFormat::Volume
        }
    }

    fn precision(&self) -> Option<usize> {
        self.precision.or(self.quote_units.then_some(2))
    }

    fn set_label_precision(&mut self, precision: Option<usize>) {
        self.precision = precision;
        self.update_table();
    }

    fn format_value(&self, value: f64) -> String {
        let value = self.value_format().format(value, self.precision());
        match &self.currency {
            Some(currency) if self.quote_units => format!("{value} {currency}"),
            _ => value,
        }
    }

    fn line_names(&self) -> Vec<String> {
        vec![format!(
            "{} ({})",
            if self.quote_units { "Amount" } else { "Volume" },
            self.unit()
        )]
    }

    fn table(&self) -> Option<&StudyTable> {
        self.table.as_ref()
    }

    fn set_instrument(&mut self, instrument: Option<&crate::model::Instrument>) -> bool {
        let currency = instrument
            .and_then(|i| i.currency.as_deref())
            .map(str::trim)
            .filter(|s| !s.is_empty());
        if self.currency.as_deref() == currency {
            return false;
        }
        self.currency = currency.map(str::to_owned);
        self.clear();
        true
    }

    fn plot_style(&self, _line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.color,
            width: 1.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: crate::studies::PlotType::Columns,
            visible: true,
        }
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        if std::mem::take(&mut self.from_candles) {
            // Chart bars replace candle sums from the first bar.
            self.values.clear();
            self.stepper = Stepper::default();
        }
        let long_bars = self
            .clock
            .timeframe()
            .is_some_and(|timeframe| timeframe.duration_ms() > Timeframe::Day1.duration_ms());
        let quote_units = self.quote_units;
        let available = !quote_units || self.currency.is_some();
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if long_bars || !available || (!quote_units && !bar.volume.is_finite()) {
                    return IndicatorValue::None;
                }
                let start = bar
                    .time
                    .checked_sub_signed(TimeDelta::days(1))
                    .unwrap_or(DateTime::<Utc>::MIN_UTC);
                let mut largest = 0.0_f64;
                while let Some(&(time, volume)) = state.window.front()
                    && time <= start
                {
                    state.window.pop_front();
                    if let Some(volume) = volume {
                        state.sum -= volume;
                        largest = largest.max(volume.abs());
                    } else {
                        state.invalid -= 1;
                    }
                }
                if largest > state.sum.abs() {
                    // A removed volume that dominated the sum leaves its rounding behind:
                    // add the window again so the error does not linger.
                    state.sum = state.window.iter().filter_map(|&(_, volume)| volume).sum();
                }
                let amount = Self::amount(bar, quote_units);
                state.window.push_back((bar.time, amount));
                if let Some(amount) = amount {
                    state.sum += amount;
                } else {
                    state.invalid += 1;
                }
                // Overflow of a finite sum must not poison later ordinary windows.
                if !state.sum.is_finite() {
                    state.sum = state.window.iter().filter_map(|&(_, volume)| volume).sum();
                }
                if state.invalid == 0 && state.sum.is_finite() {
                    IndicatorValue::Single(state.sum)
                } else {
                    IndicatorValue::None
                }
            });
        self.update_table();
    }

    fn set_bar_clock(&mut self, clock: &BarClock) -> bool {
        let changed = self.clock != *clock;
        if changed {
            self.clock = clock.clone();
            self.clear();
        }
        changed
    }

    fn data_requests(&self) -> Vec<StudyDataRequest> {
        self.lower_timeframe()
            .map(lower_interval::request)
            .into_iter()
            .collect()
    }

    fn update_data(&mut self, data: &[Bar], from: usize, sources: &[StudyData<'_>]) {
        let Some(lower) = self.lower_timeframe() else {
            return self.update(data, from);
        };
        let from = from.min(self.values.len()).min(data.len());
        let request = lower_interval::request(lower);
        let candles = if self.quote_units {
            // Validate the series here: invalid amounts affect their own windows, not all history.
            match sources.iter().find(|source| source.request == request) {
                Some(source)
                    if source.timeframe != lower
                        || (from == 0
                            && source
                                .bars
                                .windows(2)
                                .any(|pair| pair[0].time >= pair[1].time)) =>
                {
                    None
                }
                Some(source) => Some(source.bars),
                None => Some(&[][..]),
            }
        } else {
            lower_interval::reply(&request, from, sources)
        };
        match candles {
            None if self.quote_units => {
                self.clear();
                self.values = IndicatorValues::missing(data.len());
                self.update_table();
            }
            None => {}
            Some([]) => self.update(data, from),
            Some(candles) => self.sum_candles(data, from, lower, candles),
        }
    }

    fn is_incremental(&self) -> bool {
        true
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
    use chrono::TimeZone;

    /// Both paths use each contributing bar's close, and an unusable quote amount stays
    /// missing until that candle expires rather than silently becoming supplied volume.
    #[test]
    fn units_use_each_candle_close_and_recover_after_expiry() {
        use crate::model::Instrument;
        use crate::studies::{InputValue, StudyCell};
        let bar = |h: i64, volume, close| {
            Bar::new(
                Utc.timestamp_opt(h * 3_600, 0).unwrap(),
                close,
                close,
                close,
                close,
                volume,
            )
        };
        let five_hours = Timeframe::Custom(std::num::NonZeroU64::new(18_000).unwrap());
        // 2 + 3 = 5; 2×10 + 3×20 = 80. At hour 24 the first bar expires:
        // 3 + 4 = 7 and 3×20 + 4×30 = 180. Lower candles end at their close.
        for (interval, data, candles, supplied, quote) in [
            (
                Timeframe::Hour1,
                vec![
                    bar(0, 2.0, 10.0),
                    bar(1, 3.0, 20.0),
                    bar(24, 4.0, 30.0),
                    bar(25, 1.0, 40.0),
                ],
                vec![],
                vec![Some(2.0), Some(5.0), Some(7.0), Some(5.0)],
                vec![Some(20.0), Some(80.0), Some(180.0), Some(160.0)],
            ),
            (
                five_hours,
                vec![bar(0, 99.0, 100.0), bar(24, 99.0, 100.0)],
                vec![
                    bar(0, 2.0, 10.0),
                    bar(1, 3.0, 20.0),
                    bar(24, 4.0, 30.0),
                    bar(25, 1.0, 40.0),
                ],
                vec![Some(5.0), Some(5.0)],
                vec![Some(80.0), Some(160.0)],
            ),
            (
                Timeframe::Week1,
                vec![bar(4 * 24, 99.0, 100.0), bar(11 * 24, 99.0, 100.0)],
                vec![
                    bar(7 * 24 + 15, 7.0, 5.0),
                    bar(7 * 24 + 16, 2.0, 10.0),
                    bar(8 * 24 + 15, 3.0, 20.0),
                    bar(17 * 24, 4.0, 30.0),
                    bar(17 * 24 + 23, 1.0, 40.0),
                ],
                vec![Some(5.0), Some(5.0)],
                vec![Some(80.0), Some(160.0)],
            ),
        ] {
            for (units, expected) in [(0, supplied), (1, quote)] {
                let mut study = Volume24h::new();
                study.set_instrument(Some(&Instrument::new("TEST").with_currency("USD")));
                study.set_bar_clock(&BarClock::new(interval));
                study.set_input("units", InputValue::Choice(units)).unwrap();
                let lower = study.lower_timeframe().unwrap_or(Timeframe::Hour1);
                let source = StudyData {
                    request: lower_interval::request(lower),
                    bars: &candles,
                    clock: BarClock::new(lower),
                    instrument: None,
                    timeframe: lower,
                };
                for end in 1..=data.len() {
                    study.update_data(&data[..end], end - 1, std::slice::from_ref(&source));
                    assert_eq!(
                        study.values().value(0, end - 1),
                        expected[end - 1],
                        "{interval:?}, units {units}, bar {}",
                        end - 1
                    );
                }
                study.update_data(&data, 0, &[source]);
                assert_eq!(
                    (0..data.len())
                        .map(|i| study.values().value(0, i))
                        .collect::<Vec<_>>(),
                    expected
                );
            }
        }
        for (volume, close) in [
            (2.0, f64::NAN),
            (2.0, f64::INFINITY),
            (f64::NAN, 10.0),
            (-2.0, 10.0),
            (1e308, 10.0),
        ] {
            for interval in [Timeframe::Hour1, five_hours, Timeframe::Week1] {
                let mut study = Volume24h::new();
                study.set_input("units", InputValue::Choice(1)).unwrap();
                study.set_instrument(Some(&Instrument::new("TEST").with_currency("USD")));
                study.set_bar_clock(&BarClock::new(interval));
                let (data, candles, expected) = if interval == Timeframe::Hour1 {
                    (
                        vec![bar(0, volume, close), bar(1, 3.0, 20.0), bar(24, 4.0, 30.0)],
                        vec![],
                        vec![None, None, Some(180.0)],
                    )
                } else if interval == five_hours {
                    (
                        vec![bar(0, 99.0, 100.0), bar(24, 99.0, 100.0)],
                        vec![bar(0, volume, close), bar(1, 3.0, 20.0), bar(24, 4.0, 30.0)],
                        vec![None, Some(180.0)],
                    )
                } else {
                    (
                        vec![bar(4 * 24, 99.0, 100.0), bar(11 * 24, 99.0, 100.0)],
                        vec![
                            bar(7 * 24 + 16, volume, close),
                            bar(8 * 24 + 15, 3.0, 20.0),
                            bar(17 * 24 + 23, 4.0, 30.0),
                        ],
                        vec![None, Some(120.0)],
                    )
                };
                let lower = study.lower_timeframe().unwrap_or(Timeframe::Hour1);
                let source = StudyData {
                    request: lower_interval::request(lower),
                    bars: &candles,
                    clock: BarClock::new(lower),
                    instrument: None,
                    timeframe: lower,
                };
                study.update_data(&data, 0, &[source]);
                assert_eq!(
                    (0..data.len())
                        .map(|i| study.values().value(0, i))
                        .collect::<Vec<_>>(),
                    expected,
                    "{interval:?}, {volume} × {close}"
                );
                study.set_instrument(None);
                study.update_data(&data, 0, &[]);
                assert!((0..data.len()).all(|i| study.values().value(0, i).is_none()));
                assert_eq!(
                    study.table().unwrap().rows[0][0],
                    StudyCell::Text("Quote currency unavailable".into())
                );
            }
        }
    }

    /// Saved settings retain the unit choice and all public readouts describe the same amount.
    #[cfg(all(feature = "widget", feature = "drawings"))]
    #[test]
    fn units_survive_templates_and_layouts_with_labelled_readouts() {
        use crate::model::{BarData, Instrument};
        use crate::studies::{InputValue, StudyTemplate};
        use crate::widget::{Chart, ChartLayoutMode, SavedLayout};
        let data = [0_i64, 1].map(|h| {
            let (volume, close) = if h == 0 { (2.0, 10.0) } else { (3.0, 20.0) };
            Bar::new(
                Utc.timestamp_opt(h * 3_600, 0).unwrap(),
                close,
                close,
                close,
                close,
                volume,
            )
        });
        for (units, label, value, formatted) in [
            (0, "Volume (supplied units)", 5.0, "5"),
            (1, "Amount (USD)", 80.0, "80.00 USD"),
        ] {
            let mut study = Volume24h::new();
            study.set_input("units", InputValue::Choice(units)).unwrap();
            let json = serde_json::to_string(&StudyTemplate::from_study(&study)).unwrap();
            let restored = serde_json::from_str::<StudyTemplate>(&json)
                .unwrap()
                .create()
                .unwrap();
            let mut chart = Chart::new(BarData::from_bars(data.to_vec()));
            chart.set_timeframe(Timeframe::Hour1);
            chart.set_instrument(Some(Instrument::new("TEST").with_currency("USD")));
            let id = chart.add_indicator(restored);
            let study = chart
                .indicators()
                .find(|(candidate, _)| *candidate == id)
                .unwrap()
                .1;
            assert_eq!(study.values().value(0, 1), Some(value));
            assert!(study.table().is_none());
            assert_eq!(study.line_names(), [label]);
            assert_eq!(study.format_value(value), formatted);
            let table = chart.data_table();
            assert_eq!(
                table.headers.last().unwrap(),
                &format!("24-hour Volume: {label}")
            );
            assert_eq!(
                table.rows[1].last().unwrap(),
                if units == 0 { "5.00" } else { "80.00" }
            );
            let saved = SavedLayout {
                mode: ChartLayoutMode::Single,
                charts: vec![chart.snapshot().unwrap()],
            };
            let saved = SavedLayout::from_json(&saved.to_json().unwrap()).unwrap();
            let mut restored = Chart::new(BarData::from_bars(data.to_vec()));
            restored.set_instrument(Some(Instrument::new("TEST").with_currency("USD")));
            restored.restore(&saved.charts[0]).unwrap();
            let study = restored.indicators().next().unwrap().1;
            assert_eq!(
                study
                    .inputs()
                    .iter()
                    .find(|input| input.id == "units")
                    .unwrap()
                    .value,
                InputValue::Choice(units)
            );
            assert_eq!(restored.data_table().rows[1].last(), table.rows[1].last());
            let id = restored.indicators().next().unwrap().0;
            let study = restored.indicator_mut(id).unwrap();
            let mut settings = study.settings();
            settings.precision = Some(1);
            study.apply_settings(&[], settings).unwrap();
            assert_eq!(
                study.format_value(value),
                if units == 0 { "5.0" } else { "80.0 USD" }
            );
            assert_eq!(
                restored.data_table().rows[1].last().unwrap(),
                if units == 0 { "5.0" } else { "80.0" }
            );
            let study = restored.indicator_mut(id).unwrap();
            study.set_input("units", InputValue::Choice(0)).unwrap();
            study.set_value_formatter(Some(std::sync::Arc::new(crate::scales::VolumeFormatter {
                precision: 3,
            })));
            if units == 0 {
                study.set_input("units", InputValue::Choice(1)).unwrap();
            } else {
                let settings = study.settings();
                study
                    .apply_settings(&[("units".into(), InputValue::Choice(1))], settings)
                    .unwrap();
            }
            assert_eq!(study.format_value(80.0), "80.0 USD");
        }
    }

    /// Each value is the sum of its own window, even after a volume too large for the others to
    /// register in a running sum has left it.
    #[test]
    fn a_dominant_volume_leaves_no_residue() {
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        let bars: Vec<Bar> = (0..60)
            .map(|h| {
                let volume = if h == 0 { 1e17 } else { 1.0 };
                Bar::new(start + TimeDelta::hours(h), 1.0, 1.0, 1.0, 1.0, volume)
            })
            .collect();
        let mut study = Volume24h::new();
        study.calculate(&bars);
        for bar in 24..60 {
            assert_eq!(study.values().value(0, bar), Some(24.0), "bar {bar}");
        }
    }

    /// On intervals longer than a day or that do not divide it, each bar sums the lower-interval
    /// candles of the 24 hours before the close of its last candle; without candles it sums chart
    /// bars as before. Intervals that divide the day request nothing.
    #[test]
    fn long_and_uneven_intervals_sum_lower_interval_candles() {
        use crate::studies::{StudyData, StudyDataRequest};
        let (hour, day) = (3_600, 86_400);
        let bar =
            |s: i64, volume| Bar::new(Utc.timestamp_opt(s, 0).unwrap(), 1.0, 1.0, 1.0, 1.0, volume);
        let five_hours = Timeframe::Custom(std::num::NonZeroU64::new(18_000).unwrap());
        let mut study = Volume24h::new();
        for (chart, lower) in [
            (Timeframe::Hour1, None),
            (Timeframe::Day1, None),
            (five_hours, Some(Timeframe::Min1)),
            (Timeframe::Week1, Some(Timeframe::Hour1)),
        ] {
            study.set_bar_clock(&BarClock::new(chart));
            let requested = study.data_requests().first().and_then(|r| r.timeframe);
            assert_eq!(requested, lower, "{chart:?}");
        }
        let sums = |study: &mut Volume24h, data: &[Bar], lower: Timeframe, candles: &[Bar]| {
            let source = StudyData {
                request: StudyDataRequest {
                    metric: None,
                    symbol: String::new(),
                    timeframe: Some(lower),
                },
                bars: candles,
                clock: BarClock::new(lower),
                instrument: None,
                timeframe: lower,
            };
            study.update_data(data, 0, &[source]);
            (0..data.len())
                .map(|i| study.values().value(0, i))
                .collect::<Vec<_>>()
        };
        // Weekly bars from Monday 5 January 1970 (day 4). The first week's last candle closes on
        // Friday at 16:00, so its window starts on Thursday at 16:00; the second week's closes
        // with the bar, on Monday 19 January at 00:00.
        let weeks = [4, 11, 18].map(|d| bar(d * day, 1.0));
        let hourly = [
            bar(7 * day + 15 * hour, 1.0),   // Thursday 15:00, before the window
            bar(7 * day + 16 * hour, 2.0),   // Thursday 16:00, its first hour
            bar(8 * day + 15 * hour, 4.0),   // Friday 15:00, the week's last candle
            bar(16 * day + 23 * hour, 8.0),  // Saturday 23:00, before the window
            bar(17 * day, 16.0),             // Sunday 00:00, its first hour
            bar(17 * day + 23 * hour, 32.0), // Sunday 23:00
        ];
        assert_eq!(
            sums(&mut study, &weeks, Timeframe::Hour1, &hourly),
            [Some(6.0), Some(48.0), None]
        );
        // Five-hour bars from midnight on two days; minute candles at 04:59, 05:00 and 09:59
        // on the first day and at 04:59 and 05:00 on the second.
        study.set_bar_clock(&BarClock::new(five_hours));
        let bars = [0, 5, 10, 15, 20, 24, 29].map(|h| bar(h * hour, 1.0));
        let minutes = [
            (17_940, 1.0),
            (18_000, 2.0),
            (35_940, 4.0),
            (104_340, 8.0),
            (104_400, 16.0),
        ]
        .map(|(s, v)| bar(s, v));
        assert_eq!(
            sums(&mut study, &bars, Timeframe::Min1, &minutes),
            [
                Some(1.0),
                Some(7.0),
                None,
                None,
                None,
                Some(14.0),
                Some(28.0)
            ]
        );
        // Without candles the chart bars opened in the last 24 hours are summed again.
        assert_eq!(
            sums(&mut study, &bars, Timeframe::Min1, &[]),
            [1.0, 2.0, 3.0, 4.0, 5.0, 5.0, 5.0].map(Some)
        );
    }
}
