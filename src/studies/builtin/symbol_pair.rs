//! Ratio, spread and rolling correlation of two timestamp-aligned series.
//!
//! Each of the two series is a price source of the chart symbol, when its symbol is empty, or of
//! a host-supplied symbol at the chart's interval. The studies are calculated per chart bar:
//! the first series is `a`, the second `b`, and a series is aligned to a bar when it has a bar
//! in the same interval.
//!
//! # Rule
//!
//! ```text
//! Ratio:       a / b
//! Spread:      a - b
//! Correlation: Pearson correlation of the last `length` values of a and b
//! ```
//!
//! With log returns the correlation uses `ln(v[t] / v[t - 1])` of each series over the last
//! `length` returns, which needs `length + 1` consecutive bars.
//!
//! # Missing data
//!
//! A bar where either series has no bar of its own, or a non-finite value, has no output. Nothing
//! is carried forward or taken from the chart in its place, and no gap is bridged: a correlation
//! window that holds a missing bar has no value until the bar leaves it. A zero ratio denominator,
//! a non-positive price in the log-return mode and a series that is constant over the window,
//! whose correlation has zero variance, are unavailable, never infinite or invented.
//!
//! Hosts supply the histories of the symbols named by the inputs, as for any other study with
//! [`Indicator::data_requests`]; no market data service is included.

use crate::model::{Bar, PriceSource};
use crate::studies::palette;
use crate::studies::{
    Indicator, IndicatorValues, InputKind, InputValue, StudyData, StudyDataRequest, StudyInput,
};
use egui::Color32;

/// The first series divided by the second.
pub type Ratio = SymbolPair<true, false>;
/// The first series minus the second.
pub type Spread = SymbolPair<false, false>;
/// Rolling Pearson correlation of the two series.
pub type Correlation = SymbolPair<false, true>;

/// A ratio, spread or correlation of two series, each a price source of a symbol. `RATIO` selects
/// the ratio and `CORRELATION` the correlation; neither selects the spread.
#[derive(Clone)]
pub struct SymbolPair<const RATIO: bool, const CORRELATION: bool> {
    symbols: [String; 2],
    sources: [PriceSource; 2],
    length: usize,
    log_returns: bool,
    /// Both series on each calculated bar, NaN where one is unavailable.
    pairs: Vec<[f64; 2]>,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl<const RATIO: bool, const CORRELATION: bool> SymbolPair<RATIO, CORRELATION> {
    /// A correlation over `length` values, raised to at least 2. Other studies of the pair
    /// ignore it.
    pub fn new(length: usize) -> Self {
        Self {
            symbols: [String::new(), String::new()],
            sources: [PriceSource::Close; 2],
            length: length.max(2),
            log_returns: false,
            pairs: Vec::new(),
            values: IndicatorValues::new(),
            color: if CORRELATION {
                palette::LIGHT_BLUE
            } else {
                Color32::from_rgba_unmultiplied(128, 0, 128, 166)
            },
            visible: true,
        }
    }

    /// Correlate log returns instead of the values.
    pub fn with_log_returns(mut self, log_returns: bool) -> Self {
        self.log_returns = log_returns;
        self
    }

    /// The history a series needs from the host, or `None` for the chart symbol.
    fn request(&self, leg: usize) -> Option<StudyDataRequest> {
        (!self.symbols[leg].is_empty()).then(|| StudyDataRequest {
            metric: None,
            symbol: self.symbols[leg].clone(),
            timeframe: None,
        })
    }

    /// Pearson correlation of the series over the window ending at bar `at`.
    fn correlation(&self, at: usize) -> Option<f64> {
        let span = self.length.saturating_add(usize::from(self.log_returns));
        let window = &self.pairs[(at + 1).checked_sub(span)?..=at];
        let series = |leg: usize| -> Option<Vec<f64>> {
            if self.log_returns {
                window
                    .windows(2)
                    .map(|pair| {
                        let (before, after) = (pair[0][leg], pair[1][leg]);
                        let change = (after / before).ln();
                        (before > 0.0 && after > 0.0 && change.is_finite()).then_some(change)
                    })
                    .collect()
            } else {
                window
                    .iter()
                    .map(|pair| pair[leg].is_finite().then_some(pair[leg]))
                    .collect()
            }
        };
        let (x, y) = (series(0)?, series(1)?);
        // Constant values have no variance, though their mean may not be exact.
        if x.iter().all(|v| *v == x[0]) || y.iter().all(|v| *v == y[0]) {
            return None;
        }
        let n = x.len() as f64;
        let (mean_x, mean_y) = (x.iter().sum::<f64>() / n, y.iter().sum::<f64>() / n);
        let (mut xy, mut xx, mut yy) = (0.0, 0.0, 0.0);
        for (x, y) in x.iter().zip(&y) {
            let (dx, dy) = (x - mean_x, y - mean_y);
            xy += dx * dy;
            xx += dx * dx;
            yy += dy * dy;
        }
        let correlation = xy / (xx * yy).sqrt();
        correlation
            .is_finite()
            .then_some(correlation.clamp(-1.0, 1.0))
    }
}

impl<const RATIO: bool, const CORRELATION: bool> Default for SymbolPair<RATIO, CORRELATION> {
    fn default() -> Self {
        Self::new(20)
    }
}

impl<const RATIO: bool, const CORRELATION: bool> Indicator for SymbolPair<RATIO, CORRELATION> {
    fn line_style(&self, _line: usize) -> crate::studies::IndicatorLineStyle {
        crate::studies::IndicatorLineStyle {
            width: if CORRELATION { 1.0 } else { 2.0 },
            ..Default::default()
        }
    }

    fn name(&self) -> &str {
        if CORRELATION {
            "Correlation Coefficient"
        } else if RATIO {
            "Ratio"
        } else {
            "Spread"
        }
    }

    fn desc(&self) -> &str {
        if CORRELATION {
            "Correlation Coefficient - Rolling Pearson correlation of two aligned series"
        } else if RATIO {
            "Ratio - One aligned series divided by another"
        } else {
            "Spread - One aligned series minus another"
        }
    }

    fn inputs(&self) -> Vec<StudyInput> {
        let defaults = Self::default();
        let mut inputs = Vec::new();
        for (leg, label) in ["First", "Second"].into_iter().enumerate() {
            let (symbol, source) = (format!("symbol_{leg}"), format!("source_{leg}"));
            inputs.push(
                StudyInput::new(
                    symbol,
                    format!("{label} symbol"),
                    InputValue::Symbol(self.symbols[leg].clone()),
                    InputKind::Symbol,
                )
                .with_default(InputValue::Symbol(defaults.symbols[leg].clone())),
            );
            let position = |source: PriceSource| {
                PriceSource::all()
                    .iter()
                    .position(|candidate| *candidate == source)
                    .unwrap_or(0)
            };
            inputs.push(
                StudyInput::new(
                    source,
                    format!("{label} source"),
                    InputValue::Choice(position(self.sources[leg])),
                    InputKind::Choice(
                        PriceSource::all()
                            .iter()
                            .map(|source| source.label().into())
                            .collect(),
                    ),
                )
                .with_default(InputValue::Choice(position(defaults.sources[leg]))),
            );
        }
        if CORRELATION {
            inputs.push(
                StudyInput::new(
                    "length",
                    "Length",
                    InputValue::Length(self.length),
                    InputKind::Length(2, usize::MAX),
                )
                .with_default(InputValue::Length(defaults.length)),
            );
            inputs.push(
                StudyInput::new(
                    "log_returns",
                    "Use log returns",
                    InputValue::Toggle(self.log_returns),
                    InputKind::Toggle,
                )
                .with_default(InputValue::Toggle(false)),
            );
        }
        inputs
    }

    fn set_input(&mut self, id: &str, value: InputValue) -> Result<(), &'static str> {
        let schema = self
            .inputs()
            .into_iter()
            .find(|input| input.id == id)
            .ok_or("unknown study input")?;
        if !schema.accepts(&value) {
            return Err("study input is outside its accepted values");
        }
        match (id, value) {
            ("symbol_0", InputValue::Symbol(value)) => self.symbols[0] = value,
            ("symbol_1", InputValue::Symbol(value)) => self.symbols[1] = value,
            ("source_0", InputValue::Choice(value)) => self.sources[0] = PriceSource::all()[value],
            ("source_1", InputValue::Choice(value)) => self.sources[1] = PriceSource::all()[value],
            ("length", InputValue::Length(value)) => self.length = value,
            ("log_returns", InputValue::Toggle(value)) => self.log_returns = value,
            _ => return Err("unknown study input"),
        }
        self.pairs.clear();
        self.values.clear();
        Ok(())
    }

    fn data_requests(&self) -> Vec<StudyDataRequest> {
        let mut requests: Vec<_> = (0..2).filter_map(|leg| self.request(leg)).collect();
        requests.dedup();
        requests
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.update_data(data, from, &[]);
    }

    fn update_data(&mut self, data: &[Bar], from: usize, sources: &[StudyData<'_>]) {
        let from = from.min(self.values.len()).min(data.len());
        self.pairs.truncate(from);
        self.values.truncate(from);
        // The chart's own bars, or the series of the host symbol; a series the host has not
        // supplied has no bars.
        let series: Vec<_> = (0..2)
            .map(|leg| {
                self.request(leg)
                    .map(|request| sources.iter().find(|source| source.request == request))
            })
            .collect();
        for bar in &data[from..] {
            let pair: [f64; 2] = std::array::from_fn(|leg| {
                let bar = match series[leg] {
                    None => Some(bar),
                    Some(source) => {
                        source.and_then(|source| source.bar_at(bar.time).map(|at| &source.bars[at]))
                    }
                };
                bar.map_or(f64::NAN, |bar| {
                    self.sources[leg].compute(bar.open, bar.high, bar.low, bar.close)
                })
            });
            self.pairs.push(pair);
            let at = self.pairs.len() - 1;
            let value = if CORRELATION {
                self.correlation(at)
            } else if pair.iter().all(|value| value.is_finite()) {
                if RATIO {
                    (pair[1] != 0.0).then(|| pair[0] / pair[1])
                } else {
                    Some(pair[0] - pair[1])
                }
            } else {
                None
            };
            self.values.push_lines(&[value.unwrap_or(f64::NAN)]);
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
        if let Some(color) = colors.first() {
            self.color = *color;
        }
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            range: CORRELATION.then_some((-1.1, 1.1)),
            levels: if CORRELATION {
                vec![(0.0, palette::GRAY.gamma_multiply_u8(128), "Zero")]
            } else {
                Vec::new()
            },
            ..Default::default()
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

    fn bars(values: &[Option<f64>]) -> Vec<Bar> {
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
                    1.0,
                ))
            })
            .collect()
    }

    fn run(
        name: &str,
        inputs: &[(&str, InputValue)],
        chart: &[Option<f64>],
        other: &[Option<f64>],
    ) -> Vec<Option<f64>> {
        let mut study = crate::studies::IndicatorFactory::new()
            .create(name)
            .unwrap();
        study
            .set_input("symbol_1", InputValue::Symbol("B".into()))
            .unwrap();
        for (id, value) in inputs {
            study.set_input(id, value.clone()).unwrap();
        }
        let (chart, other) = (bars(chart), bars(other));
        let sources: Vec<_> = study
            .data_requests()
            .into_iter()
            .map(|request| StudyData {
                request,
                bars: &other,
                clock: BarClock::new(Timeframe::Day1),
                instrument: None,
                timeframe: Timeframe::Day1,
            })
            .collect();
        study.update_data(&chart, 0, &sources);
        (0..chart.len())
            .map(|at| study.values().value(0, at))
            .collect()
    }

    fn near(actual: &[Option<f64>], expected: &[Option<f64>], case: &str) {
        assert_eq!(actual.len(), expected.len(), "{case}");
        for (at, (got, want)) in actual.iter().zip(expected).enumerate() {
            match (got, want) {
                (Some(got), Some(want)) => assert!((got - want).abs() < 1e-12, "{case} bar {at}"),
                _ => assert_eq!(got, want, "{case} bar {at}"),
            }
        }
    }

    /// Hand-calculated two-source fixtures: the other series misses day 3 and holds a zero on
    /// day 4, which leaves the ratio undefined there and the spread defined.
    #[test]
    fn arithmetic_aligns_by_timestamp_and_leaves_undefined_values_unavailable() {
        let s = Some;
        let chart = [s(8.0), s(9.0), s(-6.0), s(7.0), s(5.0), s(3.0)];
        let other = [s(2.0), s(-3.0), s(4.0), None, s(0.0), s(1.5)];
        near(
            &run("Ratio", &[], &chart, &other),
            &[s(4.0), s(-3.0), s(-1.5), None, None, s(2.0)],
            "ratio",
        );
        near(
            &run("Spread", &[], &chart, &other),
            &[s(6.0), s(12.0), s(-10.0), None, s(5.0), s(1.5)],
            "spread",
        );
        // The second source is the high, which equals the close here, and the first the open.
        let mut study = crate::studies::IndicatorFactory::new()
            .create("Spread")
            .unwrap();
        study.set_input("source_0", InputValue::Choice(0)).unwrap();
        study.set_input("source_1", InputValue::Choice(1)).unwrap();
        let bar = Bar::new(DateTime::UNIX_EPOCH, 10.0, 14.0, 9.0, 12.0, 1.0);
        study.calculate(&[bar]);
        assert_eq!(study.values().value(0, 0), Some(-4.0));
    }

    /// Windows of three values, by hand: x = 1 2 3 4 5 against y = 2 4 5 8 6 has
    /// r = 3/sqrt(28/3), 4/sqrt(52/3) and 1/sqrt(28/3) (sums of products over the root of the sums of squares).
    #[test]
    fn correlation_uses_complete_windows_and_leaves_zero_variance_unavailable() {
        let s = Some;
        let x = [s(1.0), s(2.0), s(3.0), s(4.0), s(5.0)];
        let y = [s(2.0), s(4.0), s(5.0), s(8.0), s(6.0)];
        let length = [("length", InputValue::Length(3))];
        near(
            &run("Correlation Coefficient", &length, &x, &y),
            &[
                None,
                None,
                s(3.0 / (28.0f64 / 3.0).sqrt()),
                s(4.0 / (52.0f64 / 3.0).sqrt()),
                s(1.0 / (28.0f64 / 3.0).sqrt()),
            ],
            "positive",
        );
        let falling = [s(5.0), s(4.0), s(3.0), s(2.0), s(1.0)];
        near(
            &run("Correlation Coefficient", &length, &x, &falling),
            &[None, None, s(-1.0), s(-1.0), s(-1.0)],
            "negative",
        );
        // A missing day blanks the windows that hold it.
        let gap = [s(2.0), s(4.0), None, s(8.0), s(6.0)];
        near(
            &run("Correlation Coefficient", &length, &x, &gap),
            &[None; 5],
            "missing",
        );
        // A constant series has no variance; the last window of y is 2, 2, 3, so r = 1/sqrt(4/3).
        let flat = [s(2.0), s(2.0), s(2.0), s(2.0), s(3.0)];
        near(
            &run("Correlation Coefficient", &length, &x, &flat),
            &[None, None, None, None, s(0.75f64.sqrt())],
            "undefined",
        );
    }

    /// Log returns: doubling and tripling twice then halving and thirding give returns
    /// (ln 2, ln 2, -ln 2) and (ln 3, ln 3, -ln 3), perfectly correlated. Negative prices
    /// remain unavailable even when their ratios are positive and their logs finite.
    #[test]
    fn log_return_correlation_needs_positive_prices() {
        let s = Some;
        let inputs = [
            ("length", InputValue::Length(3)),
            ("log_returns", InputValue::Toggle(true)),
        ];
        let a = [s(1.0), s(2.0), s(4.0), s(2.0)];
        let b = [s(1.0), s(3.0), s(9.0), s(3.0)];
        near(
            &run("Correlation Coefficient", &inputs, &a, &b),
            &[None, None, None, s(1.0)],
            "returns",
        );
        let zero = [s(1.0), s(0.0), s(9.0), s(3.0)];
        near(
            &run("Correlation Coefficient", &inputs, &a, &zero),
            &[None; 4],
            "zero",
        );
        let negative = [s(-1.0), s(-2.0), s(-4.0), s(-2.0)];
        for (a, b) in [(&negative, &b), (&a, &negative)] {
            near(
                &run("Correlation Coefficient", &inputs, a, b),
                &[None; 4],
                "negative",
            );
        }
    }
}
