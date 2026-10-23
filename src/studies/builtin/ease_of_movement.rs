use super::{clamp_param, over_finite_bars};
use crate::model::Bar;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

/// Ease of Movement (EoM / EMV), Richard Arms: how easily price moves for the volume traded.
///
/// The raw value of a bar is its midpoint move over the previous bar divided by its box
/// ratio, (volume / divisor) / (high - low), and the study is the mean of the raw values of
/// the last `period` bars. A bar with zero range has a raw value of 0. A bar with zero
/// volume and a nonzero range has no raw value (its box ratio is 0). A bar with a NaN or
/// infinite high, low or volume has no value and is left out: the next bar's raw value
/// compares with the last usable bar. The mean is taken over the bars of the window that have a raw value, and a
/// window without one has no value.
#[derive(Clone)]
pub struct EaseOfMovement {
    /// SMA smoothing period (typically 14)
    period: usize,
    /// Volume divisor: Arms states volume in units of 10,000
    divisor: f64,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl EaseOfMovement {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            divisor: 10_000.0,
            values: IndicatorValues::new(),
            color: DESIGN_TOKENS.semantic.extended.warning, // Orange
            visible: true,
        }
    }

    /// Set the volume divisor, 10,000 by default (Arms states volume in units of 10,000), clamped into `f64::EPSILON` to `f64::MAX`, NaN to `f64::EPSILON`.
    pub fn with_divisor(mut self, divisor: f64) -> Self {
        self.divisor = clamp_param(divisor, f64::EPSILON, f64::MAX);
        self
    }

    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for EaseOfMovement {
    fn default() -> Self {
        Self::new(14)
    }
}

impl EaseOfMovement {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        // The raw value of bar `i + 1`, or `None` where it is undefined.
        let raw: Vec<Option<f64>> = data
            .windows(2)
            .map(|pair| {
                let (prev, current) = (&pair[0], &pair[1]);
                // Distance Moved = ((High + Low) / 2) - ((Prev High + Prev Low) / 2)
                let distance_moved =
                    (current.high + current.low) / 2.0 - (prev.high + prev.low) / 2.0;
                let range = current.high - current.low;
                if range == 0.0 {
                    Some(0.0)
                } else {
                    // Box Ratio = (Volume / divisor) / (High - Low); zero volume makes the value infinite.
                    let box_ratio = (current.volume / self.divisor) / range;
                    Some(distance_moved / box_ratio).filter(|eom| eom.is_finite())
                }
            })
            .collect();

        // Bar 0 has no raw value, so the first full window ends on bar `period`.
        (0..data.len())
            .map(|i| {
                let Some(window) = i.checked_sub(self.period).map(|start| &raw[start..i]) else {
                    return IndicatorValue::None;
                };
                let defined = window.iter().flatten();
                match defined.clone().count() {
                    0 => IndicatorValue::None,
                    n => IndicatorValue::Single(defined.sum::<f64>() / n as f64),
                }
            })
            .collect()
    }
}

impl Indicator for EaseOfMovement {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        divisor: number("Divisor", f64::EPSILON, f64::MAX)
    ; {});

    fn name(&self) -> &str {
        "EoM"
    }

    fn desc(&self) -> &str {
        "Ease of Movement - Relates price change to volume"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values = over_finite_bars(
            data,
            |b| b.high.is_finite() && b.low.is_finite() && b.volume.is_finite(),
            |data| self.compute(data),
        );
    }

    fn values(&self) -> &IndicatorValues {
        &self.values
    }

    fn colors(&self) -> Vec<Color32> {
        vec![self.color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.color = colors[0];
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

    fn line_names(&self) -> Vec<String> {
        vec![format!("EoM({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn make_bar(high: f64, low: f64, close: f64, volume: f64) -> Bar {
        Bar {
            time: Utc::now(),
            open: (high + low) / 2.0,
            high,
            low,
            close,
            volume,
        }
    }

    /// Arms' definition with volume in 10,000s, from the worked example in
    /// "Arms' Ease of Movement: Adding Volume to the Equation" (AAII Journal,
    /// October 2001): EMV 0.00638 on the second bar.
    #[test]
    fn default_divisor_matches_reference() {
        // June 19 volume implied by the article's box ratio of 60.37 on a 1.63 range.
        let example = [(75.10, 74.10, 584_800.0), (75.80, 74.17, 984_031.0)];
        let trend = [
            example[0],
            example[1],
            (76.20, 75.00, 700_000.0),
            (75.90, 74.60, 1_200_000.0),
            (77.00, 75.50, 900_000.0),
            (77.40, 76.10, 650_000.0),
        ];
        let flat = [(100.0, 100.0, 1_000.0); 4];
        let zero_volume = [
            (10.5, 9.5, 1_000.0),
            (12.0, 10.0, 0.0),
            (13.0, 11.0, 1_000.0),
        ];
        // Bar 6 closes a window of three zero-volume bars; bar 8 has zero range and zero volume.
        let zero_volume_windows = [
            (10.0, 9.0, 50_000.0),
            (11.0, 10.0, 40_000.0),
            (12.0, 10.0, 0.0),
            (12.5, 11.5, 60_000.0),
            (12.0, 11.0, 0.0),
            (13.0, 12.0, 0.0),
            (13.0, 12.0, 0.0),
            (14.0, 13.0, 30_000.0),
            (14.0, 14.0, 0.0),
            (15.0, 14.0, 20_000.0),
        ];
        // Bar 2 has a NaN input and is left out: bar 3 follows bar 1, so the raw values are 0.25, 1 and 1.
        let non_finite = [
            (10.0, 9.0, 50_000.0),
            (11.0, 10.0, 40_000.0),
            (f64::NAN, f64::NAN, 30_000.0),
            (13.0, 12.0, 20_000.0),
            (14.0, 13.0, 10_000.0),
        ];
        let cases = [
            (
                "worked example",
                &example[..],
                1,
                vec![None, Some(0.00637733973828067)],
            ),
            (
                "trend",
                &trend[..],
                3,
                vec![
                    None,
                    None,
                    None,
                    Some(0.004376176738157037),
                    Some(0.007805952380952369),
                    Some(0.007625000000000039),
                ],
            ),
            ("flat", &flat[..], 2, vec![None, None, Some(0.0), Some(0.0)]),
            (
                "zero volume",
                &zero_volume[..],
                1,
                vec![None, None, Some(20.0)],
            ),
            (
                "zero volume windows",
                &zero_volume_windows[..],
                3,
                vec![
                    None,
                    None,
                    None,
                    Some(0.20833333333333334),
                    Some(0.16666666666666666),
                    Some(0.16666666666666666),
                    None,
                    Some(0.3333333333333333),
                    Some(0.16666666666666666),
                    Some(0.19444444444444445),
                ],
            ),
            (
                "non-finite bar",
                &non_finite[..],
                2,
                vec![None, None, None, Some(0.625), Some(1.0)],
            ),
        ];

        for (name, bars, period, expected) in cases {
            let data: Vec<Bar> = bars
                .iter()
                .map(|&(h, l, v)| make_bar(h, l, (h + l) / 2.0, v))
                .collect();
            let mut eom = EaseOfMovement::new(period);
            eom.calculate(&data);
            assert_eq!(eom.values.len(), expected.len(), "{name}");
            for (i, (got, want)) in eom.values.to_rows().iter().zip(expected.iter()).enumerate() {
                match (got, want) {
                    (IndicatorValue::None, None) => {}
                    (IndicatorValue::Single(g), Some(w)) => {
                        assert!((g - w).abs() <= 1e-12, "{name}[{i}]: got {g}, want {w}")
                    }
                    _ => panic!("{name}[{i}]: got {got:?}, want {want:?}"),
                }
            }
        }
    }
}
