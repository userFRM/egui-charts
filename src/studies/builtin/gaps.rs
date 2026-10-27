//! Gaps.
//!
//! Marks price gaps between consecutive bars as zones that extend right until price fills
//! them.
//!
//! # Rule
//!
//! ```text
//! Up gap at bar i:   low[i] > high[i - 1], zone from high[i - 1] to low[i]
//! Down gap at bar i: high[i] < low[i - 1], zone from high[i] to low[i - 1]
//! kept when its height >= deviation% * SMA(high - low, 14) at bar i
//! ```
//!
//! A zone starts at the bar before the gap. A later bar enters an up gap when its low is below
//! the zone's top, and a down gap when its high is above the zone's bottom; touching the edge
//! does not enter it. By default the first bar to enter a gap closes it, and the zone ends on
//! that bar. With `close_partially`, the zone ends only when a bar covers it to its far edge;
//! a bar that enters it part way narrows it, at that bar, to the part the bar has not covered.
//! With `limit_trail`, a zone also closes `trail_length` bars after its left edge, the bar
//! before the gap or, after a partial fill, the bar that filled it.
//!
//! Only the latest `max_gaps` gaps are shown, open or closed, each as one [`StudyZone`] per
//! extent it has held, the next starting on the bar where the previous ends. Zones of up gaps
//! take the style of output 0, "Up gaps", and zones of down gaps that of output 1, "Down gaps";
//! neither output has values, so memory grows with the zones and not with the bars. The
//! defaults are 15 gaps, a deviation of 30%, partial closing off and a trail length of 300
//! bars, off.
//!
//! # Example
//!
//! ```rust
//! use chrono::{TimeZone, Utc};
//! use egui_charts::model::Bar;
//! use egui_charts::studies::{Gaps, Indicator, StudyZone};
//!
//! // Fourteen bars of range 2 around 100, then a bar that opens a gap from 101 to 102.
//! let bars: Vec<Bar> = [100.0; 14]
//!     .into_iter()
//!     .chain([103.0, 104.0])
//!     .enumerate()
//!     .map(|(i, p)| Bar::new(Utc.timestamp_opt(i as i64 * 60, 0).unwrap(), p, p + 1.0, p - 1.0, p, 1.0))
//!     .collect();
//! let mut gaps = Gaps::default();
//! gaps.calculate(&bars);
//! // The zone starts at the bar before the gap and is still open on the last bar.
//! let zone = StudyZone { bars: 13..16, top: 102.0, bottom: 101.0, plot: 0 };
//! assert_eq!(gaps.zones(), [zone]);
//! ```

use super::clamp_param;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues, StudyZone};
use egui::Color32;

/// Most gaps a chart shows.
const MAX_GAPS: usize = 500;

/// Bars in the average range a gap is measured against.
const RANGE_LENGTH: usize = 14;

/// Gaps: zones between consecutive bars that do not overlap, open until price fills them.
#[derive(Clone)]
pub struct Gaps {
    max_gaps: usize,
    deviation: f64,
    close_partially: bool,
    limit_trail: bool,
    trail_length: usize,
    values: IndicatorValues,
    stepper: Stepper<State>,
    closed: Vec<(usize, StudyZone)>,
    before_last_closed: usize,
    expired: Vec<(usize, (usize, StudyZone))>,
    zones: Vec<StudyZone>,
    up_color: Color32,
    down_color: Color32,
    visible: bool,
}

/// One open gap.
#[derive(Clone)]
struct Gap {
    /// Gaps found before it.
    index: usize,
    up: bool,
    near: f64,
    far: f64,
    left: usize,
    source_left: usize,
}

impl Gap {
    /// Its current extent from its left edge to `last`, inclusive.
    fn zone(&self, last: usize) -> (usize, StudyZone) {
        let zone = StudyZone {
            bars: self.source_left..last + 1,
            top: self.near.max(self.far),
            bottom: self.near.min(self.far),
            plot: usize::from(!self.up),
        };
        (self.index, zone)
    }
}

#[derive(Clone, Default)]
struct State {
    source: usize,
    usable: usize,
    previous: Option<(usize, [f64; 2])>,
    last_source: Option<usize>,
    ranges: std::collections::VecDeque<f64>,
    open: Vec<Gap>,
    found: usize,
}

impl Gaps {
    /// The latest `max_gaps` gaps (1 to 500) of at least `deviation` percent (1 to 100, NaN to
    /// 1) of the average bar range.
    pub fn new(max_gaps: usize, deviation: f64) -> Self {
        Self {
            max_gaps: max_gaps.clamp(1, MAX_GAPS),
            deviation: clamp_param(deviation, 1.0, 100.0),
            close_partially: false,
            limit_trail: false,
            trail_length: 300,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            closed: Vec::new(),
            before_last_closed: 0,
            expired: Vec::new(),
            zones: Vec::new(),
            up_color: palette::GREEN,
            down_color: palette::RED,
            visible: true,
        }
    }

    /// Shrink a gap a bar enters part way instead of closing it.
    pub fn with_partial_closing(mut self, close_partially: bool) -> Self {
        self.close_partially = close_partially;
        self
    }

    /// Close every gap `length` bars (at least 1) after its left edge, or `None` for no limit.
    pub fn with_trail_limit(mut self, length: Option<usize>) -> Self {
        self.limit_trail = length.is_some();
        if let Some(length) = length {
            self.trail_length = length.max(1);
        }
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for Gaps {
    fn default() -> Self {
        Self::new(15, 30.0)
    }
}

impl Indicator for Gaps {
    crate::studies::inputs::study_inputs!(self;
        close_partially: toggle("Close gaps partially"),
        max_gaps: length("Max number of gaps", 1, MAX_GAPS),
        deviation: number("Minimal deviation (%)", 1.0, 100.0),
        limit_trail: toggle("Limit max gap trail length"),
        trail_length: length("Max gap trail length (bars)", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Gaps"
    }

    fn desc(&self) -> &str {
        "Gaps - Price gaps between bars, extended until price fills them"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let len = self.values.len();
        if len > 0 && from == len && data.len() == len {
            return;
        }
        if len == 0 || from < len - 1 || data.len() < len {
            self.closed.clear();
            self.zones.clear();
        } else if from == len - 1 {
            if self.expired.is_empty() {
                self.closed.truncate(self.before_last_closed);
                self.zones.truncate(self.before_last_closed);
            } else {
                let mut kept = std::mem::take(&mut self.closed).into_iter();
                let mut expired = std::mem::take(&mut self.expired).into_iter().peekable();
                for at in 0..self.before_last_closed {
                    self.closed
                        .push(if expired.peek().is_some_and(|(index, _)| *index == at) {
                            expired.next().unwrap().1
                        } else {
                            kept.next().unwrap()
                        });
                }
                self.zones.clear();
            }
        } else {
            self.zones.truncate(self.closed.len());
        }
        let mut retained = self.zones.len();
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let source = state.source;
                state.source += 1;
                let last = source + 1 == data.len();
                if last {
                    self.before_last_closed = self.closed.len();
                    self.expired.clear();
                }
                if !bar.high.is_finite() || !bar.low.is_finite() {
                    return IndicatorValue::None;
                }
                let i = state.usable;
                state.usable += 1;
                state.last_source = Some(source);
                state.ranges.push_back(bar.high - bar.low);
                if state.ranges.len() > RANGE_LENGTH {
                    state.ranges.pop_front();
                }
                let average = if state.ranges.len() == RANGE_LENGTH
                    && state.ranges.iter().all(|v| v.is_finite())
                {
                    state.ranges.iter().sum::<f64>() / RANGE_LENGTH as f64
                } else {
                    f64::NAN
                };
                state.open.retain_mut(|gap| {
                    let (entered, covered) = if gap.up {
                        (bar.low < gap.near, bar.low <= gap.far)
                    } else {
                        (bar.high > gap.near, bar.high >= gap.far)
                    };
                    if entered {
                        self.closed.push(gap.zone(source));
                        if covered || !self.close_partially {
                            return false;
                        }
                        gap.near = if gap.up { bar.low } else { bar.high };
                        gap.left = i;
                        gap.source_left = source;
                    } else if self.limit_trail && i - gap.left >= self.trail_length {
                        self.closed.push(gap.zone(source));
                        return false;
                    }
                    true
                });
                let Some((previous_source, previous)) =
                    state.previous.replace((source, [bar.high, bar.low]))
                else {
                    return IndicatorValue::None;
                };
                let threshold = average * self.deviation / 100.0;
                let (up, near, far) = if bar.low > previous[0] && bar.low - previous[0] >= threshold
                {
                    (true, bar.low, previous[0])
                } else if bar.high < previous[1] && previous[1] - bar.high >= threshold {
                    (false, bar.high, previous[1])
                } else {
                    return IndicatorValue::None;
                };
                let gap = Gap {
                    index: state.found,
                    up,
                    near,
                    far,
                    left: i - 1,
                    source_left: previous_source,
                };
                state.found += 1;
                if self.limit_trail && self.trail_length == 1 {
                    self.closed.push(gap.zone(source));
                } else {
                    state.open.push(gap);
                }
                let shown = state.found.saturating_sub(self.max_gaps);
                state.open.retain(|gap| gap.index >= shown);
                let mut at = 0;
                self.closed.retain(|zone| {
                    let keep = zone.0 >= shown;
                    if !keep && last && at < self.before_last_closed {
                        self.expired.push((at, zone.clone()));
                    }
                    if !keep {
                        retained = 0;
                    }
                    at += 1;
                    keep
                });
                IndicatorValue::None
            });
        let state = self.stepper.state();
        // Completed segments do not move on a tick and must stay outside the cloned checkpoint.
        self.zones.truncate(retained);
        self.zones
            .extend(self.closed[retained..].iter().map(|(_, zone)| zone.clone()));
        if let Some(last) = state.last_source {
            self.zones
                .extend(state.open.iter().map(|gap| gap.zone(last).1));
        }
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn zones(&self) -> &[StudyZone] {
        &self.zones
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.up_color, self.down_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if let Some(&color) = colors.first() {
            self.up_color = color;
        }
        if let Some(&color) = colors.get(1) {
            self.down_color = color;
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

    fn line_names(&self) -> Vec<String> {
        vec!["Up gaps".to_string(), "Down gaps".to_string()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    /// Gaps keep no per-bar column, so their memory follows the gaps shown, not the bars.
    #[test]
    fn memory_grows_with_the_gaps_shown_not_the_bars() {
        // Range-2 bars that step 10 higher every 100 bars, leaving 999 gaps no bar fills.
        let bars = |step: f64| -> Vec<Bar> {
            (0..100_000)
                .map(|i| {
                    let p = 100.0 + step * (i / 100) as f64;
                    let time = Utc.timestamp_opt(i as i64 * 60, 0).unwrap();
                    Bar::new(time, p, p + 1.0, p - 1.0, p, 1.0)
                })
                .collect()
        };
        let mut gaps = Gaps::new(500, 30.0);
        gaps.calculate(&bars(0.0));
        assert!(gaps.values().columns().is_empty() && gaps.zones().is_empty());
        gaps.calculate(&bars(10.0));
        assert!(gaps.values().columns().is_empty());
        assert_eq!(gaps.zones().len(), 500);
        assert!(gaps.zones().iter().all(|zone| zone.bars.end == 100_000));
        assert_eq!(gaps.zones()[0].bars.start, 49_999);
    }

    /// A study zone that spans the visible bars widens the auto range to its edges, as a
    /// per-bar value would, although the bars it began on are out of view.
    #[test]
    fn visible_study_zones_auto_scale() {
        // Fourteen bars around 100, a bar that leaves a gap from 101 to 102, then bars at 150.
        let bars: Vec<Bar> = [100.0; 14]
            .into_iter()
            .chain([103.0])
            .chain([150.0; 100])
            .enumerate()
            .map(|(i, p)| {
                let t = Utc.timestamp_opt(1_700_000_000 + i as i64 * 60, 0).unwrap();
                Bar::new(t, p, p + 1.0, p - 1.0, p, 1.0)
            })
            .collect();
        let mut gaps = Gaps::default();
        gaps.calculate(&bars);
        let mut state = crate::studies::ChartState::new(crate::model::BarData::from_bars(bars));
        state.time_scale_mut().set_width(80.0);
        state.time_scale_mut().set_bar_spacing(8.0);
        let transform = state.fit_price_scale([&gaps as &dyn Indicator]);
        let range = state.price_scale().range();
        assert!(transform.to_price(range.min) <= 101.0, "{range:?}");
    }
}
