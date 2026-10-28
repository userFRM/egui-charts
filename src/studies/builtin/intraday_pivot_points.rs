//! Intraday Pivot Points.
//!
//! Pivot levels for each block of a trading session from the block before it. The session is an
//! exchange trading day, counted as [`PivotPoints`](super::PivotPoints) counts them: the chart
//! supplies the instrument's time zone and session through [`Indicator::set_instrument`] (UTC days
//! until then). The first bar of a session opens its first block; block `k` holds the bars from
//! `k * source period` to the next multiple after that bar's time, so a block is the same length
//! in every session and never crosses one. The levels of a block come from the high, low and close
//! of the latest earlier block of the same session, with the formulas of the chosen
//! [`PivotMethod`]; the first block of a session has none, and bars a day or longer, which each
//! open a session, never have any.
//!
//! # Output
//!
//! Eleven lines, `[P, R1, S1, R2, S2, R3, S3, R4, S4, R5, S5]`, constant within a block. A line is
//! not drawn across the change from one block to the next. Overlay indicator. The default is
//! Traditional pivots from 60-minute blocks.
//!
//! # Example
//!
//! ```rust
//! use chrono::{TimeZone, Utc};
//! use egui_charts::model::Bar;
//! use egui_charts::studies::{Indicator, IntradayPivotPoints};
//!
//! // Half-hourly bars: the second hour takes its levels from the first.
//! let bars: Vec<Bar> = [(10.0, 8.0, 9.0), (12.0, 9.0, 11.0), (11.0, 7.0, 8.0)]
//!     .into_iter()
//!     .enumerate()
//!     .map(|(i, (high, low, close))| {
//!         let time = Utc.with_ymd_and_hms(2026, 3, 2, 9, 0, 0).unwrap() + chrono::Duration::minutes(30 * i as i64);
//!         Bar::new(time, close, high, low, close, 1.0)
//!     })
//!     .collect();
//! let mut pivots = IntradayPivotPoints::new(60);
//! pivots.calculate(&bars);
//! assert_eq!(pivots.values().value(0, 1), None);
//! // P = (12 + 8 + 11) / 3 from the high, low and close of the first hour.
//! assert!((pivots.values().value(0, 2).unwrap() - 31.0 / 3.0).abs() < 1e-12);
//! ```

use super::anchor_period::ExchangeDays;
use super::pivot_points::{PivotMethod, PivotPoints};
use crate::model::{Bar, Instrument};
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use chrono::{DateTime, NaiveDate, Utc};
use egui::Color32;

/// Intraday Pivot Points: levels for each block of a trading session from the block before it.
#[derive(Clone)]
pub struct IntradayPivotPoints {
    method: PivotMethod,
    minutes: usize,
    days: ExchangeDays,
    values: IndicatorValues,
    stepper: Stepper<State>,
    /// One per line: P, then R1, S1 through R5, S5.
    colors: Vec<Color32>,
    visible: bool,
}

#[derive(Clone)]
struct State {
    session: Option<NaiveDate>,
    origin: DateTime<Utc>,
    block: i64,
    current: Option<Bar>,
    levels: [f64; 11],
}

impl Default for State {
    fn default() -> Self {
        Self {
            session: None,
            origin: DateTime::UNIX_EPOCH,
            block: 0,
            current: None,
            levels: [f64::NAN; 11],
        }
    }
}

impl IntradayPivotPoints {
    /// Traditional pivots from blocks of `minutes` (1 to 1440; 0 becomes 1).
    pub fn new(minutes: usize) -> Self {
        Self {
            method: PivotMethod::Traditional,
            minutes: minutes.clamp(1, 1440),
            days: ExchangeDays::default(),
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: PivotPoints::new().colors(),
            visible: true,
        }
    }

    /// The level formulas.
    pub fn with_method(mut self, method: PivotMethod) -> Self {
        self.method = method;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for IntradayPivotPoints {
    fn default() -> Self {
        Self::new(60)
    }
}

impl Indicator for IntradayPivotPoints {
    crate::studies::inputs::study_inputs!(self;
        method: choice("Type", [PivotMethod::Traditional, PivotMethod::Fibonacci, PivotMethod::Woodie, PivotMethod::Classic, PivotMethod::DM, PivotMethod::Camarilla]),
        minutes: length("Source period (minutes)", 1, 1440)
    ; {});

    fn name(&self) -> &str {
        "Intraday Pivot Points"
    }

    fn desc(&self) -> &str {
        "Intraday Pivot Points - Support and resistance levels from the previous block of the session"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let method = self.method;
        let block_ms = self.minutes as i64 * 60_000;
        let needs_open = matches!(method, PivotMethod::Woodie | PivotMethod::DM);
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !(bar.high.is_finite()
                    && bar.low.is_finite()
                    && bar.close.is_finite()
                    && (!needs_open || bar.open.is_finite()))
                {
                    return IndicatorValue::None;
                }
                let day = self.days.day(bar, false);
                let block = (bar.time - state.origin)
                    .num_milliseconds()
                    .div_euclid(block_ms);
                match &mut state.current {
                    Some(current) if state.session == Some(day) => {
                        if block != state.block {
                            state.levels = PivotPoints::levels(method, current, bar.open);
                            state.block = block;
                            *current = bar.clone();
                        } else {
                            current.high = current.high.max(bar.high);
                            current.low = current.low.min(bar.low);
                            current.close = bar.close;
                        }
                    }
                    _ => {
                        state.session = Some(day);
                        state.origin = bar.time;
                        state.block = 0;
                        state.current = Some(bar.clone());
                        state.levels = [f64::NAN; 11];
                    }
                }
                IndicatorValue::Multiple(state.levels.to_vec())
            });
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn set_instrument(&mut self, instrument: Option<&Instrument>) -> bool {
        self.days.set(instrument)
    }

    /// Transparent on the first bar of a block, so the change of level is not drawn.
    fn plot_color(&self, line: usize, bar: usize) -> Option<Color32> {
        let previous = self.values.value(line, bar.checked_sub(1)?)?;
        (Some(previous) != self.values.value(line, bar)).then_some(Color32::TRANSPARENT)
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        self.colors.clone()
    }

    /// Sets the colors line by line; lines without a color keep theirs.
    fn set_colors(&mut self, colors: Vec<Color32>) {
        for (line, color) in self.colors.iter_mut().zip(colors) {
            *line = color;
        }
    }

    fn is_overlay(&self) -> bool {
        true
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

    fn line_names(&self) -> Vec<String> {
        [
            "P", "R1", "S1", "R2", "S2", "R3", "S3", "R4", "S4", "R5", "S5",
        ]
        .map(String::from)
        .to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    /// Half-hourly bars of two sessions, with 60-minute blocks from the first bar of each.
    /// Block 0 of the first session has high 12, low 8 and close 11, so block 1 has P = 31/3,
    /// R1 = 2P - L = 38/3, S1 = 2P - H = 26/3, R2 = P + (H - L) = 43/3 and S2 = 19/3; block 2 takes
    /// block 1 (high 13, low 7, close 12), P = 32/3 and R1 = 2P - L = 43/3. The next session starts
    /// afresh, with no levels in its first block.
    #[test]
    fn levels_come_from_the_previous_block_of_the_session() {
        let start = Utc.with_ymd_and_hms(2026, 3, 2, 9, 0, 0).unwrap();
        let next_day = Utc.with_ymd_and_hms(2026, 3, 3, 9, 0, 0).unwrap();
        let data: Vec<Bar> = [
            (start, 10.0, 8.0, 9.0),
            (start + chrono::Duration::minutes(30), 12.0, 9.0, 11.0),
            (start + chrono::Duration::minutes(60), 11.0, 7.0, 8.0),
            (start + chrono::Duration::minutes(90), 13.0, 9.0, 12.0),
            (start + chrono::Duration::minutes(120), 9.0, 8.0, 8.5),
            (next_day, 9.0, 8.0, 8.5),
            (next_day + chrono::Duration::minutes(30), 9.5, 8.0, 9.0),
            (next_day + chrono::Duration::minutes(60), 9.5, 8.0, 9.0),
        ]
        .into_iter()
        .map(|(time, high, low, close)| Bar::new(time, close, high, low, close, 1.0))
        .collect();
        let mut pivots = IntradayPivotPoints::new(60);
        pivots.calculate(&data);
        let level = |line, bar| pivots.values().value(line, bar);
        for bar in [0, 1, 5, 6] {
            assert_eq!(level(0, bar), None, "bar {bar}");
        }
        for (line, want) in [31.0 / 3.0, 38.0 / 3.0, 26.0 / 3.0, 43.0 / 3.0, 19.0 / 3.0]
            .into_iter()
            .enumerate()
        {
            for bar in [2, 3] {
                assert!(
                    (level(line, bar).unwrap() - want).abs() < 1e-12,
                    "line {line} bar {bar}"
                );
            }
        }
        assert!((level(0, 4).unwrap() - 32.0 / 3.0).abs() < 1e-12);
        assert!((level(1, 4).unwrap() - 43.0 / 3.0).abs() < 1e-12);
        // The second session's second block uses its first block, 9.5, 8 and 9: P = 8.8333.
        assert!((level(0, 7).unwrap() - (9.5 + 8.0 + 9.0) / 3.0).abs() < 1e-12);
    }
}
