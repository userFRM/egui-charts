use super::clamp_param;
use crate::model::Bar;
/// Ease of Movement (EoM / EMV)
/// Relates price change to volume to identify the ease at which prices move
/// Developed by Richard Arms
use crate::studies::{Indicator, IndicatorValue};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;

#[derive(Clone)]
pub struct EaseOfMovement {
    /// SMA smoothing period (typically 14)
    period: usize,
    /// Volume divisor: Arms states volume in units of 10,000
    divisor: f64,
    values: Vec<IndicatorValue>,
    color: Color32,
    visible: bool,
}

impl EaseOfMovement {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            divisor: 10_000.0,
            values: Vec::new(),
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

    fn calculate_sma(data: &[f64], period: usize) -> Vec<f64> {
        let mut result = Vec::with_capacity(data.len());

        for i in 0..data.len() {
            if i < period - 1 {
                result.push(f64::NAN);
            } else {
                let sum: f64 = data[i + 1 - period..=i].iter().sum();
                result.push(sum / period as f64);
            }
        }

        result
    }
}

/// Construct with the conventional default parameters.
impl Default for EaseOfMovement {
    fn default() -> Self {
        Self::new(14)
    }
}

impl Indicator for EaseOfMovement {
    fn name(&self) -> &str {
        "EoM"
    }

    fn desc(&self) -> &str {
        "Ease of Movement - Relates price change to volume"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.values.clear();

        if data.len() < 2 {
            for _ in 0..data.len() {
                self.values.push(IndicatorValue::None);
            }
            return;
        }

        // Calculate raw EoM values
        let mut raw_eom = Vec::with_capacity(data.len());
        raw_eom.push(f64::NAN); // First bar has no previous

        for i in 1..data.len() {
            let current = &data[i];
            let prev = &data[i - 1];

            // Distance Moved = ((High + Low) / 2) - ((Prev High + Prev Low) / 2)
            let distance_moved = (current.high + current.low) / 2.0 - (prev.high + prev.low) / 2.0;

            // Box Ratio = (Volume / divisor) / (High - Low)
            let range = current.high - current.low;
            if range == 0.0 || current.volume == 0.0 {
                raw_eom.push(0.0);
            } else {
                let box_ratio = (current.volume / self.divisor) / range;
                let eom = distance_moved / box_ratio;
                raw_eom.push(eom);
            }
        }

        // Apply SMA smoothing
        let smoothed = Self::calculate_sma(&raw_eom, self.period);

        // Store results
        for value in smoothed {
            if value.is_nan() {
                self.values.push(IndicatorValue::None);
            } else {
                self.values.push(IndicatorValue::Single(value));
            }
        }
    }

    fn values(&self) -> &[IndicatorValue] {
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
        ];

        for (name, bars, period, expected) in cases {
            let data: Vec<Bar> = bars
                .iter()
                .map(|&(h, l, v)| make_bar(h, l, (h + l) / 2.0, v))
                .collect();
            let mut eom = EaseOfMovement::new(period);
            eom.calculate(&data);
            assert_eq!(eom.values.len(), expected.len(), "{name}");
            for (i, (got, want)) in eom.values.iter().zip(expected.iter()).enumerate() {
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
