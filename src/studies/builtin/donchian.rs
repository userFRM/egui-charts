//! Donchian Channels Indicator
//!
//! Donchian Channels consist of three lines based on moving highest high
//! and lowest low calculations. They're used to identify breakouts and trends.
//!
//! # Components
//! - Upper Band: Highest high over N periods
//! - Lower Band: Lowest low over N periods
//! - Middle Band: Avg of upper and lower
//!
//! # Interpretation
//! - Price breaking above upper band: Bullish breakout
//! - Price breaking below lower band: Bearish breakout
//! - Channel width indicates volatility
//!
//! # Example
//! ```
//! # let bars: Vec<egui_charts::model::Bar> = (0..60).map(|i| { let p = 100.0 + (i as f64 * 0.3).sin() * 5.0; egui_charts::model::Bar::new(chrono::Utc::now(), p, p + 1.0, p - 1.0, p + 0.5, 1_000.0) }).collect();
//! use egui_charts::studies::{DonchianChannels, Indicator};
//!
//! let mut donchian = DonchianChannels::new(20);
//! donchian.calculate(&bars);
//! ```

use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{rolling_max, rolling_min};
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Donchian Channels indicator
#[derive(Clone)]
pub struct DonchianChannels {
    period: usize,
    offset: i32,
    values: IndicatorValues,
    upper_color: Color32,
    middle_color: Color32,
    lower_color: Color32,
    visible: bool,
}

impl DonchianChannels {
    /// Create a new Donchian Channels indicator
    ///
    /// # Arguments
    /// * `period` - Lookback period (typically 20)
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            offset: 0,
            values: IndicatorValues::new(),
            upper_color: palette::BLUE,
            middle_color: palette::ORANGE,
            lower_color: palette::BLUE,
            visible: true,
        }
    }

    /// Set colors for upper, middle, and lower bands
    pub fn with_colors(mut self, upper: Color32, middle: Color32, lower: Color32) -> Self {
        self.upper_color = upper;
        self.middle_color = middle;
        self.lower_color = lower;
        self
    }

    /// Draw every line `offset` bars later, earlier when negative.
    pub fn with_offset(mut self, offset: i32) -> Self {
        self.offset = offset;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for DonchianChannels {
    fn default() -> Self {
        Self::new(20)
    }
}

impl DonchianChannels {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        if data.len() < self.period {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        let highest = rolling_max(data.iter().map(|b| b.high), self.period);
        let lowest = rolling_min(data.iter().map(|b| b.low), self.period);
        for i in 0..data.len() {
            if i < self.period - 1 {
                values.push(IndicatorValue::None);
            } else {
                let (upper, lower) = (highest[i], lowest[i]);
                let middle = (upper + lower) / 2.0;

                values.push_lines(&[middle, upper, lower]);
            }
        }

        values
    }
}

impl Indicator for DonchianChannels {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        offset: integer("Offset", i32::MIN, i32::MAX)
    ; {});

    fn plot_offset(&self, _line: usize) -> isize {
        self.offset as isize
    }

    fn name(&self) -> &str {
        "Donchian"
    }

    fn desc(&self) -> &str {
        "Donchian Channels - Breakout indicator"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let mut values = std::mem::take(&mut self.values);
        update_window(
            &mut values,
            data,
            from,
            usable_behind(self.period - 1, |b| b.high.is_finite() && b.low.is_finite()),
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

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.middle_color, self.upper_color, self.lower_color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.middle_color = colors[0];
        }
        if colors.len() > 1 {
            self.upper_color = colors[1];
        }
        if colors.len() > 2 {
            self.lower_color = colors[2];
        }
    }

    fn is_overlay(&self) -> bool {
        true // Donchian is drawn on main chart
    }

    fn line_count(&self) -> usize {
        3 // Middle, Upper, Lower
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
        vec![
            format!("Middle({})", self.period),
            "Upper".to_string(),
            "Lower".to_string(),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::test_support::make_range_bar as make_bar;
    use crate::studies::{InputValue, PriceChannel};

    /// Period 2 over highs 1, 3, 2, 5 and lows 0, 1, 1, 2: from bar 1 the upper line is 3, 3, 5
    /// and the middle line 1.5, 2, 3.
    #[test]
    fn channels_draw_their_lines_offset_bars_later() {
        let bars = [
            make_bar(1.0, 0.0, 0.5),
            make_bar(3.0, 1.0, 2.0),
            make_bar(2.0, 1.0, 1.5),
            make_bar(5.0, 2.0, 3.0),
        ];
        type Build = fn(i32) -> Box<dyn Indicator>;
        let rows: [(usize, usize, Build); 2] = [
            (1, 0, |offset| {
                Box::new(DonchianChannels::new(2).with_offset(offset))
            }),
            (0, 2, |offset| {
                Box::new(PriceChannel::new(2).with_offset(offset))
            }),
        ];
        for (upper, middle, build) in rows {
            // Bar 1 at the upper line's first 3, bar 3 at the middle line's 3.
            for (offset, first, last) in [(5, 6, 8), (-1, 0, 2)] {
                let mut typed = build(0);
                typed
                    .set_input("offset", InputValue::Integer(offset))
                    .unwrap();
                for mut study in [build(offset), typed] {
                    study.calculate(&bars);
                    for line in 0..3 {
                        assert_eq!(study.plot_offset(line), offset as isize);
                    }
                    assert_eq!(study.plot_value(upper, first), Some(3.0));
                    assert_eq!(study.plot_value(middle, last), Some(3.0));
                    // Bar 0 has no value at offset 5, and no bar 6 exists at offset -1.
                    assert_eq!(study.plot_value(upper, 5), None);
                }
            }
        }
    }
}
