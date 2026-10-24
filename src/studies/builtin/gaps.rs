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
//! a bar that enters it part way keeps the zone's extent on that bar, and from the next bar
//! on the zone holds only the part that bar has not covered. With `limit_trail`, a zone also
//! closes `trail_length` bars after its left edge, the bar before the gap or, after a partial
//! fill, the bar that filled it.
//!
//! Only the latest `max_gaps` gaps are shown, open or closed. Gap `j` (counting from the first
//! of the series) occupies outputs `2 * (j % max_gaps)` and `2 * (j % max_gaps) + 1`: the near
//! edge, the side price enters from (the top of an up gap, the bottom of a down gap), then the
//! far edge. A fill between them is green where the near edge is above the far edge, an up
//! gap, and red below, a down gap. The defaults are 15 gaps, a deviation of 30%, partial
//! closing off and a trail length of 300 bars, off.
//!
//! # Example
//!
//! ```rust
//! use chrono::{TimeZone, Utc};
//! use egui_charts::model::Bar;
//! use egui_charts::studies::{Gaps, Indicator};
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
//! for bar in 13..16 {
//!     assert_eq!(gaps.values().value(0, bar), Some(102.0));
//!     assert_eq!(gaps.values().value(1, bar), Some(101.0));
//! }
//! ```

use super::{clamp_param, over_finite_bars};
use crate::model::Bar;
use crate::studies::stepper::rolling_mean;
use crate::studies::{FillBoundary, Indicator, IndicatorValues, StudyFill};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Most gaps a chart shows, each two outputs.
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
    up_color: Color32,
    down_color: Color32,
    visible: bool,
}

/// One gap while it is tracked.
struct Gap {
    up: bool,
    near: f64,
    far: f64,
    left: usize,
    open: bool,
    /// The bars it covers and its near edge on each.
    span: Vec<(usize, f64)>,
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

    /// The values over `data`, every bar of which has a finite high and low.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::missing(data.len());
        let ranges: Vec<f64> = data.iter().map(|b| b.high - b.low).collect();
        let average = rolling_mean(&ranges, RANGE_LENGTH);
        // Every gap found, the untracked ones no longer shown.
        let mut gaps: Vec<Gap> = Vec::new();
        for (i, bar) in data.iter().enumerate() {
            let tracked = gaps.len().saturating_sub(self.max_gaps);
            for gap in gaps[tracked..].iter_mut().filter(|gap| gap.open) {
                gap.span.push((i, gap.near));
                let (entered, covered) = if gap.up {
                    (bar.low < gap.near, bar.low <= gap.far)
                } else {
                    (bar.high > gap.near, bar.high >= gap.far)
                };
                if entered && (covered || !self.close_partially) {
                    gap.open = false;
                } else if entered {
                    gap.near = if gap.up { bar.low } else { bar.high };
                    gap.left = i;
                }
                if self.limit_trail && i - gap.left >= self.trail_length {
                    gap.open = false;
                }
            }
            let Some(previous) = i.checked_sub(1).map(|p| &data[p]) else {
                continue;
            };
            let threshold = average[i] * self.deviation / 100.0;
            let gap = if bar.low > previous.high && bar.low - previous.high >= threshold {
                (true, bar.low, previous.high)
            } else if bar.high < previous.low && previous.low - bar.high >= threshold {
                (false, bar.high, previous.low)
            } else {
                continue;
            };
            gaps.push(Gap {
                up: gap.0,
                near: gap.1,
                far: gap.2,
                left: i - 1,
                open: true,
                span: vec![(i - 1, gap.1), (i, gap.1)],
            });
        }
        if !data.is_empty() {
            // Every column exists, so a bar has a value per line.
            values.set_value(2 * self.max_gaps - 1, 0, f64::NAN);
        }
        let shown = gaps.len().saturating_sub(self.max_gaps);
        for (j, gap) in gaps.iter().enumerate().skip(shown) {
            let slot = j % self.max_gaps;
            for &(bar, near) in &gap.span {
                values.set_value(2 * slot, bar, near);
                values.set_value(2 * slot + 1, bar, gap.far);
            }
        }
        values
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
        self.values = over_finite_bars(
            data,
            |b| b.high.is_finite() && b.low.is_finite(),
            |data| self.compute(data),
        );
    }

    fn plot_style(&self, _line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: self.up_color,
            width: 1.0,
            pattern: crate::studies::PlotPattern::Solid,
            plot_type: crate::studies::PlotType::Line,
            visible: true,
        }
    }

    /// Edges in the colour of their gap's direction.
    fn plot_color(&self, line: usize, bar: usize) -> Option<Color32> {
        let slot = line / 2 * 2;
        let (near, far) = (
            self.values.value(slot, bar)?,
            self.values.value(slot + 1, bar)?,
        );
        Some(if near >= far {
            self.up_color
        } else {
            self.down_color
        })
    }

    fn fills(&self) -> Vec<StudyFill> {
        let alpha = |c: Color32| Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), 38);
        (0..self.max_gaps)
            .map(|slot| StudyFill {
                first: FillBoundary::Plot(2 * slot),
                second: FillBoundary::Plot(2 * slot + 1),
                color: alpha(self.up_color),
                inverted_color: Some(alpha(self.down_color)),
            })
            .collect()
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
        (1..=self.max_gaps)
            .flat_map(|k| [format!("Gap {k} near edge"), format!("Gap {k} far edge")])
            .collect()
    }
}
