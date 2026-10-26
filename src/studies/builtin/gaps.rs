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
use crate::studies::stepper::rolling_mean;
use crate::studies::{Indicator, IndicatorValues, StudyZone};
use crate::tokens::DESIGN_TOKENS;
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
    zones: Vec<StudyZone>,
    up_color: Color32,
    down_color: Color32,
    visible: bool,
}

/// One open gap.
struct Gap {
    /// Gaps found before it.
    index: usize,
    up: bool,
    near: f64,
    far: f64,
    left: usize,
}

impl Gap {
    /// Its current extent from its left edge to `last`, inclusive.
    fn zone(&self, last: usize) -> (usize, StudyZone) {
        let zone = StudyZone {
            bars: self.left..last + 1,
            top: self.near.max(self.far),
            bottom: self.near.min(self.far),
            plot: usize::from(!self.up),
        };
        (self.index, zone)
    }
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
            zones: Vec::new(),
            up_color: DESIGN_TOKENS.semantic.extended.success,
            down_color: DESIGN_TOKENS.semantic.extended.error,
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

    /// The zones over `data`, every bar of which has a finite high and low.
    fn compute(&self, data: &[Bar]) -> Vec<StudyZone> {
        let ranges: Vec<f64> = data.iter().map(|b| b.high - b.low).collect();
        let average = rolling_mean(&ranges, RANGE_LENGTH);
        let mut open: Vec<Gap> = Vec::new();
        // Each zone with the index of its gap.
        let mut zones = Vec::new();
        let mut found = 0;
        for (i, bar) in data.iter().enumerate() {
            open.retain_mut(|gap| {
                let (entered, covered) = if gap.up {
                    (bar.low < gap.near, bar.low <= gap.far)
                } else {
                    (bar.high > gap.near, bar.high >= gap.far)
                };
                if entered {
                    zones.push(gap.zone(i));
                    if covered || !self.close_partially {
                        return false;
                    }
                    gap.near = if gap.up { bar.low } else { bar.high };
                    gap.left = i;
                } else if self.limit_trail && i - gap.left >= self.trail_length {
                    zones.push(gap.zone(i));
                    return false;
                }
                true
            });
            let Some(previous) = i.checked_sub(1).map(|p| &data[p]) else {
                continue;
            };
            let threshold = average[i] * self.deviation / 100.0;
            let (up, near, far) = if bar.low > previous.high && bar.low - previous.high >= threshold
            {
                (true, bar.low, previous.high)
            } else if bar.high < previous.low && previous.low - bar.high >= threshold {
                (false, bar.high, previous.low)
            } else {
                continue;
            };
            let gap = Gap {
                index: found,
                up,
                near,
                far,
                left: i - 1,
            };
            found += 1;
            // A trail of one bar ends on the bar that opens the gap.
            if self.limit_trail && self.trail_length == 1 {
                zones.push(gap.zone(i));
            } else {
                open.push(gap);
            }
            // Gaps no longer shown need not be followed.
            open.retain(|gap| gap.index + self.max_gaps >= found);
        }
        if let Some(last) = data.len().checked_sub(1) {
            zones.extend(open.iter().map(|gap| gap.zone(last)));
        }
        let shown = found.saturating_sub(self.max_gaps);
        zones
            .into_iter()
            .filter(|(gap, _)| *gap >= shown)
            .map(|(_, zone)| zone)
            .collect()
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
        let finite = |b: &Bar| b.high.is_finite() && b.low.is_finite();
        self.values = IndicatorValues::missing(data.len());
        self.zones = if data.iter().all(finite) {
            self.compute(data)
        } else {
            let kept: Vec<usize> = (0..data.len()).filter(|&i| finite(&data[i])).collect();
            let bars: Vec<Bar> = kept.iter().map(|&i| data[i].clone()).collect();
            let mut zones = self.compute(&bars);
            for zone in &mut zones {
                zone.bars = kept[zone.bars.start]..kept[zone.bars.end - 1] + 1;
            }
            zones
        };
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
