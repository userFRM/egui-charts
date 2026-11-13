use crate::model::Bar;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Guppy Multiple Moving Average (GMMA): twelve exponential moving averages of one source, six
/// short and six long, each seeded with the first close.
///
/// The lengths default to 3, 5, 8, 10, 12 and 15 for the short group and 30, 35, 40, 45, 50 and 60
/// for the long group; each one is an input.
#[derive(Clone)]
pub struct GuppyMA {
    periods: [usize; 12],
    values: IndicatorValues,
    stepper: Stepper<Option<[f64; 12]>>,
    colors: Vec<Color32>,
    visible: bool,
}

impl GuppyMA {
    /// Create the six short and six long exponential moving averages.
    pub fn new() -> Self {
        Self {
            periods: [3, 5, 8, 10, 12, 15, 30, 35, 40, 45, 50, 60],
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: [217, 224, 232, 240, 247, 255]
                .into_iter()
                .map(|alpha| Color32::from_rgba_unmultiplied(0, 255, 255, alpha))
                .chain(
                    [217, 224, 232, 240, 247, 255]
                        .into_iter()
                        .map(|alpha| Color32::from_rgba_unmultiplied(255, 0, 0, alpha)),
                )
                .collect(),
            visible: true,
        }
    }

    /// Set the twelve lengths, short group first. A length below 1 is raised to 1.
    pub fn with_periods(mut self, periods: [usize; 12]) -> Self {
        self.periods = periods.map(|period| period.max(1));
        self
    }
}

impl Default for GuppyMA {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for GuppyMA {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        periods[0]: length("Short EMA 1", 1, usize::MAX),
        periods[1]: length("Short EMA 2", 1, usize::MAX),
        periods[2]: length("Short EMA 3", 1, usize::MAX),
        periods[3]: length("Short EMA 4", 1, usize::MAX),
        periods[4]: length("Short EMA 5", 1, usize::MAX),
        periods[5]: length("Short EMA 6", 1, usize::MAX),
        periods[6]: length("Long EMA 1", 1, usize::MAX),
        periods[7]: length("Long EMA 2", 1, usize::MAX),
        periods[8]: length("Long EMA 3", 1, usize::MAX),
        periods[9]: length("Long EMA 4", 1, usize::MAX),
        periods[10]: length("Long EMA 5", 1, usize::MAX),
        periods[11]: length("Long EMA 6", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "GMMA"
    }

    fn desc(&self) -> &str {
        "Guppy Multiple Moving Average - 12 EMAs in short and long groups"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                if !bar.close.is_finite() {
                    return IndicatorValue::None;
                }
                let averages = state.get_or_insert([bar.close; 12]);
                for (average, period) in averages.iter_mut().zip(self.periods) {
                    *average = (bar.close - *average) * (2.0 / (period as f64 + 1.0)) + *average;
                }
                IndicatorValue::Multiple(averages.to_vec())
            });
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
        if colors.len() >= 12 {
            self.colors = colors;
        }
    }

    fn is_overlay(&self) -> bool {
        true
    }

    fn line_count(&self) -> usize {
        12
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
        self.periods.map(|p| format!("EMA({p})")).to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::InputValue;
    use chrono::{Duration, Utc};

    /// Bars whose open, high and low lie away from the close, so only the close can feed the averages.
    fn bars(closes: &[f64]) -> Vec<Bar> {
        let start = Utc::now();
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                Bar::new(
                    start + Duration::minutes(i as i64),
                    c - 3.0,
                    c + 5.0,
                    c - 7.0,
                    c,
                    1.0,
                )
            })
            .collect()
    }

    fn row(g: &GuppyMA, bar: usize) -> Option<Vec<f64>> {
        match &g.values().to_rows()[bar] {
            IndicatorValue::Multiple(v) => Some(v.clone()),
            _ => None,
        }
    }

    fn assert_close(got: f64, want: f64, what: &str) {
        assert!((got - want).abs() < 1e-9, "{what}: {got} != {want}");
    }

    /// Lengths 1 and 2 replace the first two defaults, giving alphas 1 and 2/3; the third stays 3
    /// (alpha 1/2) and the last 60 (alpha 2/61). Closes 10, 12, none, 9, 9, hand-derived:
    ///
    /// | bar | close | EMA(1) | EMA(2)  | EMA(3) | EMA(60)    |
    /// |-----|-------|--------|---------|--------|------------|
    /// | 0   | 10    | 10     | 10      | 10     | 10         |
    /// | 1   | 12    | 12     | 34/3    | 11     | 10 + 4/61  |
    /// | 2   | none  | none   | none    | none   | none       |
    /// | 3   | 9     | 9      | 88/9    | 10     | kept       |
    /// | 4   | 9     | 9      | 250/27  | 9.5    | kept       |
    ///
    /// A corrected close of 8 at bar 3 gives EMA(1) 8, EMA(2) 82/9 and EMA(3) 9.5.
    #[test]
    fn periods_drive_the_averages() {
        let mut g = GuppyMA::new();
        g.set_input("periods [0]", InputValue::Length(1)).unwrap();
        g.set_input("periods [1]", InputValue::Length(2)).unwrap();
        let mut data = bars(&[10.0, 12.0, f64::NAN, 9.0, 9.0]);
        g.calculate(&data);

        assert_eq!(&g.line_names()[..3], ["EMA(1)", "EMA(2)", "EMA(8)"]);
        assert_eq!(g.line_names()[11], "EMA(60)");
        assert_eq!(row(&g, 0).unwrap(), vec![10.0; 12]);
        let r = row(&g, 1).unwrap();
        assert_close(r[0], 12.0, "bar 1 EMA(1)");
        assert_close(r[1], 34.0 / 3.0, "bar 1 EMA(2)");
        assert_close(r[11], 10.0 + 4.0 / 61.0, "bar 1 EMA(60)");
        assert!(row(&g, 2).is_none());
        let r = row(&g, 3).unwrap();
        assert_close(r[0], 9.0, "bar 3 EMA(1)");
        assert_close(r[1], 88.0 / 9.0, "bar 3 EMA(2)");
        let r = row(&g, 4).unwrap();
        assert_close(r[1], 250.0 / 27.0, "bar 4 EMA(2), equal closes");

        // A correction to bar 3 recomputes from there and matches a fresh calculation.
        data[3] = bars(&[0.0, 0.0, 0.0, 8.0])[3].clone();
        let mut fresh = g.clone();
        fresh.calculate(&data);
        g.update(&data, 3);
        for bar in 0..data.len() {
            assert_eq!(row(&g, bar), row(&fresh, bar), "bar {bar}");
        }
        let r = row(&g, 3).unwrap();
        assert_close(r[0], 8.0, "corrected EMA(1)");
        assert_close(r[1], 82.0 / 9.0, "corrected EMA(2)");
    }

    #[test]
    fn with_periods_raises_zero_to_one() {
        let mut g = GuppyMA::new().with_periods([0; 12]);
        g.calculate(&bars(&[10.0, 12.0]));
        assert_eq!(row(&g, 1).unwrap(), vec![12.0; 12]);
    }
}
