use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Choppiness Index
/// Measures market trendiness vs choppiness
/// Values close to 100 = choppy market, values close to 0 = trending market
///
/// The pane draws dashed levels at 61.8 and 38.2, with the band between them filled in light blue
/// at a tenth of full opacity. A true range can reach back past the window to a gap, so a reading
/// can pass 100.
#[derive(Clone)]
pub struct ChoppinessIndex {
    /// Lookback period (typically 14)
    period: usize,
    /// Bars the plot is drawn later, earlier when negative.
    offset: i32,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl ChoppinessIndex {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            offset: 0,
            values: IndicatorValues::new(),
            color: palette::BLUE,
            visible: true,
        }
    }

    /// Set the plot color.
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for ChoppinessIndex {
    fn default() -> Self {
        Self::new(14)
    }
}

impl ChoppinessIndex {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        if data.len() < self.period.saturating_add(1) {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        // Calculate True Range for each bar
        let mut tr_values = Vec::with_capacity(data.len());
        tr_values.push(data[0].high - data[0].low); // First bar TR is just range

        for i in 1..data.len() {
            let current = &data[i];
            let prev = &data[i - 1];

            let tr = (current.high - current.low)
                .max((current.high - prev.close).abs())
                .max((current.low - prev.close).abs());

            tr_values.push(tr);
        }

        // Calculate Choppiness Index
        for i in 0..data.len() {
            if i < self.period {
                values.push(IndicatorValue::None);
            } else {
                // Sum of TR over period
                let atr_sum: f64 = tr_values[i + 1 - self.period..=i].iter().sum();

                // Highest High and Lowest Low over period
                let window = &data[i + 1 - self.period..=i];
                let highest = window
                    .iter()
                    .map(|b| b.high)
                    .fold(f64::NEG_INFINITY, f64::max);
                let lowest = window.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);

                let range = highest - lowest;

                if range == 0.0 {
                    values.push(IndicatorValue::None);
                } else {
                    // CHOP = 100 * LOG10(SUM(TR, n) / (Highest High - Lowest Low)) / LOG10(n)
                    let chop = 100.0 * (atr_sum / range).log10() / (self.period as f64).log10();
                    values.push(IndicatorValue::Single(chop));
                }
            }
        }

        values
    }
}

impl Indicator for ChoppinessIndex {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX),
        offset: integer("Offset", i32::MIN, i32::MAX)
    ; {});

    fn plot_offset(&self, _line: usize) -> isize {
        self.offset as isize
    }

    fn precision(&self) -> Option<usize> {
        Some(2)
    }

    fn name(&self) -> &str {
        "CHOP"
    }

    fn desc(&self) -> &str {
        "Choppiness Index - Measures market trendiness"
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
            usable_behind(self.period, |b| {
                b.high.is_finite() && b.low.is_finite() && b.close.is_finite()
            }),
            |data| {
                over_finite_bars(
                    data,
                    |b| b.high.is_finite() && b.low.is_finite() && b.close.is_finite(),
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
        vec![self.color]
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if !colors.is_empty() {
            self.color = colors[0];
        }
    }

    fn level_fill_color(&self) -> Color32 {
        palette::LIGHT_BLUE.gamma_multiply_u8(26)
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            height: None,
            range: None,
            levels: vec![
                (61.8, palette::MUTED, "Upper Band"),
                (38.2, palette::MUTED, "Lower Band"),
            ],
            fill_between_levels: true,
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
        vec![format!("CHOP({})", self.period)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::LineStyle;
    use crate::studies::{ConfiguredStudy, InputValue, StudyTemplate};

    /// Highs 12, 11 and lows 9, 7 in the two-bar window of the last bar, with true ranges 3 and 4.
    fn bars() -> Vec<Bar> {
        [(10.0, 8.0, 9.0), (12.0, 9.0, 11.0), (11.0, 7.0, 8.0)]
            .into_iter()
            .enumerate()
            .map(|(i, (high, low, close))| {
                let time = chrono::DateTime::from_timestamp(i as i64 * 60, 0).unwrap();
                Bar::new(time, high, high, low, close, 1.0)
            })
            .collect()
    }

    /// The study opens with dashed one-pixel levels at 61.8 and 38.2 and a light blue band between
    /// them at a tenth of full opacity, undisplaced and reading to two decimals.
    #[test]
    fn opens_with_its_bands_background_and_no_offset() {
        let study = ConfiguredStudy::new(Box::new(ChoppinessIndex::default()));
        let settings = study.settings();
        assert_eq!(
            study.pane().levels,
            [
                (61.8, palette::MUTED, "Upper Band"),
                (38.2, palette::MUTED, "Lower Band"),
            ]
        );
        for level in &settings.levels {
            assert_eq!(
                (level.pattern, level.width, level.visible),
                (LineStyle::Dashed, 1.0, true)
            );
        }
        assert!(settings.fill_between_levels);
        assert_eq!(
            settings.level_fill_color,
            Color32::from_rgba_unmultiplied(33, 150, 243, 26)
        );
        assert_eq!(study.plot_offset(0), 0);
        assert_eq!(study.precision(), Some(2));
    }

    /// The offset moves where a value is drawn and leaves the value at its source bar, through a
    /// correction of the last bar and a saved copy. By hand, with a two-bar window:
    /// 100 * log10(7 / 5) / log10(2) = 48.5427, and with the last low corrected to 5,
    /// 100 * log10(9 / 7) / log10(2) = 36.2570.
    #[test]
    fn offset_shifts_the_plot_through_a_correction_and_a_saved_copy() {
        for (offset, drawn) in [(2_i32, 4_usize), (-1, 1)] {
            let mut bars = bars();
            let mut study = ConfiguredStudy::new(Box::new(ChoppinessIndex::new(2)));
            study
                .set_input("offset", InputValue::Integer(offset))
                .unwrap();
            study.calculate(&bars);
            let read = |study: &ConfiguredStudy, bar| study.plot_value(0, bar).unwrap();
            assert!((read(&study, drawn) - 48.5427).abs() < 1e-4);
            assert!(study.values().value(0, 2).is_some());
            assert_eq!(study.plot_value(0, 2), None);

            bars[2].low = 5.0;
            study.update(&bars, 2);
            assert!((read(&study, drawn) - 36.2570).abs() < 1e-4);

            let mut saved = StudyTemplate::from_study(&study).create().unwrap();
            saved.calculate(&bars);
            assert_eq!(saved.plot_offset(0), offset as isize);
            assert_eq!(saved.plot_value(0, drawn), study.plot_value(0, drawn));
        }
    }
}
