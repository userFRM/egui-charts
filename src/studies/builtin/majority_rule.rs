use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// What [`MajorityRule`] reports for each window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MajorityMode {
    /// 100 times the share of the window's closes that rose above the close before. Equal and
    /// falling closes count against it.
    Percentage,
    /// +1 when more closes rose than fell, -1 when more fell, 0 when the counts are equal.
    Direction,
}

impl MajorityMode {
    const ALL: [Self; 2] = [Self::Percentage, Self::Direction];
}

/// Majority Rule - counts up vs down closes over `period` changes.
///
/// In [`MajorityMode::Percentage`] (the default) it outputs the percentage of rising closes; in
/// [`MajorityMode::Direction`] it outputs +1 if the majority rose, -1 if the majority fell and 0
/// on a tie. A bar without a finite close has no value and does not break the window.
#[derive(Clone)]
pub struct MajorityRule {
    period: usize,
    mode: MajorityMode,
    values: IndicatorValues,
    colors: Vec<Color32>,
    visible: bool,
}

impl MajorityRule {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            mode: MajorityMode::Percentage,
            values: IndicatorValues::new(),
            colors: vec![Color32::from_rgb(255, 82, 82)],
            visible: true,
        }
    }

    /// Choose the percentage of rising closes or the signed direction.
    pub fn with_mode(mut self, mode: MajorityMode) -> Self {
        self.mode = mode;
        self
    }
}

impl Default for MajorityRule {
    fn default() -> Self {
        Self::new(10)
    }
}

impl MajorityRule {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        if data.is_empty() {
            return values;
        }

        // First bar has no previous close to compare
        values.push(IndicatorValue::None);

        for i in 1..data.len() {
            if i < self.period {
                values.push(IndicatorValue::None);
                continue;
            }

            let start = i + 1 - self.period;
            let mut up_count: i32 = 0;
            let mut down_count: i32 = 0;

            for idx in start..=i {
                if idx == 0 {
                    continue;
                }
                if data[idx].close > data[idx - 1].close {
                    up_count += 1;
                } else if data[idx].close < data[idx - 1].close {
                    down_count += 1;
                }
            }

            let result = match self.mode {
                MajorityMode::Percentage => 100.0 * f64::from(up_count) / self.period as f64,
                MajorityMode::Direction => f64::from((up_count - down_count).signum()),
            };

            values.push(IndicatorValue::Single(result));
        }

        values
    }
}

impl Indicator for MajorityRule {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        mode: choice("Output", MajorityMode::ALL)
    ; {});

    fn value_format(&self) -> crate::scales::StudyFormat {
        match self.mode {
            MajorityMode::Percentage => crate::scales::StudyFormat::Percent,
            MajorityMode::Direction => crate::scales::StudyFormat::Fixed(2),
        }
    }

    fn name(&self) -> &str {
        "MajRule"
    }

    fn desc(&self) -> &str {
        "Majority Rule - percentage of rising closes, or +1 / -1 / 0 for the majority direction"
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
            usable_behind(self.period, |b| b.close.is_finite()),
            |data| over_finite_bars(data, |b| b.close.is_finite(), |data| self.compute(data)),
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
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.colors = colors;
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
        let mode = match self.mode {
            MajorityMode::Percentage => "Percentage",
            MajorityMode::Direction => "Direction",
        };
        vec![format!("MajRule({}, {mode})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Bars whose open, high and low lie away from the close the rule reads.
    fn bars(closes: &[f64]) -> Vec<Bar> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &close)| {
                let time = chrono::DateTime::from_timestamp(i as i64 * 60, 0).unwrap();
                Bar::new(time, close - 0.5, close + 2.0, close - 1.0, close, 1.0)
            })
            .collect()
    }

    /// Both outputs over three changes, by hand. The bar without a close has no value and is
    /// skipped, so the closes read 10, 12, 12, 11, 13, 14, 14, 9, 8. Bar 3 sees up, equal, down
    /// (33.33 %, tie); bar 5 sees equal, down, up (33.33 %, tie); bar 6 sees down, up, up
    /// (66.67 %, +1); bar 7 sees up, up, equal (66.67 %, +1); bar 8 sees up, equal, down
    /// (33.33 %, tie); bar 9 sees equal, down, down (0 %, -1). Correcting the close of bar 6 from
    /// 14 to 12 changes bars 6 to 9: bar 6 sees down, up, down (33.33 %, -1); bar 7 up, down, up
    /// (66.67 %, +1); bar 8 down, up, down (33.33 %, -1); bar 9 up, down, down (33.33 %, -1).
    #[test]
    fn both_outputs_follow_the_rising_closes_through_ties_a_gap_and_a_correction() {
        let third = 100.0 / 3.0;
        let none = f64::NAN;
        let mut closes = [10.0, 12.0, 12.0, 11.0, f64::NAN, 13.0, 14.0, 14.0, 9.0, 8.0];
        let saved = [
            (
                MajorityMode::Percentage,
                [
                    none,
                    none,
                    none,
                    third,
                    none,
                    third,
                    2.0 * third,
                    2.0 * third,
                    third,
                    0.0,
                ],
            ),
            (
                MajorityMode::Direction,
                [none, none, none, 0.0, none, 0.0, 1.0, 1.0, 0.0, -1.0],
            ),
        ];
        let corrected = [
            (
                MajorityMode::Percentage,
                [
                    none,
                    none,
                    none,
                    third,
                    none,
                    third,
                    third,
                    2.0 * third,
                    third,
                    third,
                ],
            ),
            (
                MajorityMode::Direction,
                [none, none, none, 0.0, none, 0.0, -1.0, 1.0, -1.0, -1.0],
            ),
        ];
        let fresh = |mode, closes: &[f64]| {
            let mut study = MajorityRule::new(3).with_mode(mode);
            study.calculate(&bars(closes));
            study
        };
        let check = |study: &MajorityRule, want: &[f64; 10]| {
            for (bar, want) in want.iter().enumerate() {
                let got = study.values().value(0, bar).unwrap_or(f64::NAN);
                let equal = (got.is_nan() && want.is_nan()) || (got - want).abs() < 1e-9;
                assert!(equal, "bar {bar}: {got}, not {want}");
            }
        };
        let studies: Vec<_> = saved
            .iter()
            .map(|(mode, _)| fresh(*mode, &closes))
            .collect();
        for (study, (_, want)) in studies.iter().zip(&saved) {
            check(study, want);
        }
        closes[6] = 12.0;
        for (study, (mode, want)) in studies.iter().zip(&corrected) {
            let mut study = study.clone();
            study.update(&bars(&closes), 6);
            check(&study, want);
            check(&fresh(*mode, &closes), want);
        }
    }
}
