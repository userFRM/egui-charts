//! Williams Fractals.
//!
//! Bill Williams ("Trading Chaos", 1995) marks a swing high where a bar's high stands above
//! the highs around it, and a swing low where a bar's low stands below the lows around it. The
//! conventional fractal uses five bars: two on each side.
//!
//! # Rule
//!
//! ```text
//! Up fractal at bar p:   high[p] > high[p + 1 ..= p + n]
//!                        and, for some k in 0..=4,
//!                        high[p] >= high[p - k ..= p - 1] and high[p] > high[p - k - n ..= p - k - 1]
//! Down fractal at bar p: the same with lows, below instead of above
//! ```
//!
//! The bars after the fractal must be strictly lower. Before it, up to four bars just before
//! the fractal bar may equal its high, as long as the `n` bars before them are strictly lower,
//! so a flat top of up to five bars is marked once, on its last bar.
//!
//! An up fractal is the high of its bar, drawn as an upward triangle; a down fractal is the
//! low, drawn as a downward triangle. A fractal is known `n` bars after its bar, so the last
//! `n` bars have no value yet.
//!
//! # Example
//!
//! ```rust
//! use chrono::{TimeZone, Utc};
//! use egui_charts::model::Bar;
//! use egui_charts::studies::{Indicator, WilliamsFractals};
//!
//! let bars: Vec<Bar> = [1.0, 2.0, 5.0, 3.0, 2.0]
//!     .into_iter()
//!     .enumerate()
//!     .map(|(i, p)| Bar::new(Utc.timestamp_opt(i as i64 * 60, 0).unwrap(), p, p, p - 1.0, p, 1.0))
//!     .collect();
//! let mut fractals = WilliamsFractals::default();
//! fractals.calculate(&bars);
//! assert_eq!(fractals.values().value(0, 2), Some(5.0));
//! assert_eq!(fractals.values().value(1, 2), None);
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::stepper::{rolling_max, update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValues};
use egui::Color32;

/// Williams Fractals: up fractals at swing highs and down fractals at swing lows.
#[derive(Clone)]
pub struct WilliamsFractals {
    periods: usize,
    values: IndicatorValues,
    up_color: Color32,
    down_color: Color32,
    visible: bool,
}

impl WilliamsFractals {
    /// Fractals with `periods` bars on each side, raised to at least 2 (default 2).
    pub fn new(periods: usize) -> Self {
        Self {
            periods: periods.max(2),
            values: IndicatorValues::new(),
            up_color: Color32::from_rgb(0, 150, 136),
            down_color: Color32::from_rgb(244, 67, 54),
            visible: true,
        }
    }

    /// The values over `data`, every bar of which has a finite high and low.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::missing(data.len());
        if !data.is_empty() {
            // Both columns exist, so a bar with one fractal still has a value per line.
            values.set_value(1, 0, f64::NAN);
        }
        for (line, sign) in [(0, 1.0), (1, -1.0)] {
            // Lows negated, so a down fractal is an up fractal of the negated lows.
            let x: Vec<f64> = data
                .iter()
                .map(|b| sign * if line == 0 { b.high } else { b.low })
                .collect();
            // Highest of the `periods` values ending at each bar.
            let highest = rolling_max(x.iter().copied(), self.periods);
            for p in 0..x.len() {
                let after = p.checked_add(self.periods).and_then(|end| highest.get(end));
                if !after.is_some_and(|&after| after < x[p]) {
                    continue;
                }
                let before = (0..=4).any(|k: usize| {
                    p.checked_sub(k + self.periods).is_some()
                        && x[p - k..p].iter().all(|&v| v <= x[p])
                        && highest[p - k - 1] < x[p]
                });
                if before {
                    values.set_value(line, p, sign * x[p]);
                }
            }
        }
        values
    }
}

/// Construct with the conventional default parameters.
impl Default for WilliamsFractals {
    fn default() -> Self {
        Self::new(2)
    }
}

impl Indicator for WilliamsFractals {
    crate::studies::inputs::study_inputs!(self;
        periods: length("Periods", 2, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Williams Fractals"
    }

    fn desc(&self) -> &str {
        "Williams Fractals - Swing highs and lows confirmed by the bars on each side"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        // Confirmation of a right-hand window can change earlier pivot bars.
        let from = usable_behind(self.periods, |b| b.high.is_finite() && b.low.is_finite())(
            data,
            from.min(self.values.len()).min(data.len()),
        );
        let mut values = std::mem::take(&mut self.values);
        update_window(
            &mut values,
            data,
            from,
            usable_behind(self.periods.saturating_add(4), |b| {
                b.high.is_finite() && b.low.is_finite()
            }),
            |data| {
                over_finite_bars(
                    data,
                    |b| b.high.is_finite() && b.low.is_finite(),
                    |data| self.compute(data),
                )
            },
        );
        self.values = values;
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn plot_style(&self, line: usize) -> crate::studies::PlotStyle {
        crate::studies::PlotStyle {
            color: if line == 0 {
                self.up_color
            } else {
                self.down_color
            },
            width: 2.0,
            pattern: crate::model::LineStyle::Solid,
            plot_type: if line == 0 {
                crate::studies::PlotType::TriangleUp
            } else {
                crate::studies::PlotType::TriangleDown
            },
            visible: true,
        }
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
        vec!["Up Fractal".to_string(), "Down Fractal".to_string()]
    }
}
