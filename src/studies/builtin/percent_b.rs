use super::clamp_param;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::update_window;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Percent B (%B): where the close sits within its Bollinger Bands, 0 at the lower band and
/// 1 at the upper band. Half-opacity guides mark 1 in dashed red, 0 in dashed teal and 0.5 in
/// solid blue. The outer band is filled, with red shading above 1 and teal shading below 0.
#[derive(Clone, Debug)]
pub struct PercentB {
    /// Period for Bollinger Bands (typically 20)
    period: usize,
    /// Standard deviation multiplier (typically 2.0)
    multiplier: f64,
    values: IndicatorValues,
    color: Color32,
    visible: bool,
}

impl PercentB {
    /// `multiplier` is clamped into 0.001 to 50, NaN to 0.001.
    pub fn new(period: usize, multiplier: f64) -> Self {
        Self {
            period: period.max(1),
            multiplier: clamp_param(multiplier, 0.001, 50.0),
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

impl Default for PercentB {
    fn default() -> Self {
        Self::new(20, 2.0)
    }
}

impl PercentB {
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();

        if data.len() < self.period {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        for i in 0..data.len() {
            if i < self.period - 1 {
                values.push(IndicatorValue::None);
            } else {
                let window = &data[i + 1 - self.period..=i];

                // Calculate SMA
                let sma: f64 = window.iter().map(|b| b.close).sum::<f64>() / self.period as f64;

                // Calculate standard deviation
                let variance: f64 = window.iter().map(|b| (b.close - sma).powi(2)).sum::<f64>()
                    / self.period as f64;
                let std_dev = variance.sqrt();

                // Calculate bands
                let upper = sma + self.multiplier * std_dev;
                let lower = sma - self.multiplier * std_dev;

                // Calculate %B
                let band_width = upper - lower;
                if band_width == 0.0 {
                    values.push(IndicatorValue::Single(0.5));
                } else {
                    let percent_b = (data[i].close - lower) / band_width;
                    values.push(IndicatorValue::Single(percent_b));
                }
            }
        }
        values
    }
}

impl Indicator for PercentB {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Length", 1, usize::MAX),
        multiplier: number("StdDev", 0.001, 50.0)
    ; {});

    fn name(&self) -> &str {
        "%B"
    }

    fn desc(&self) -> &str {
        "Percent B - Price position within Bollinger Bands"
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
            |_data, from| from.saturating_sub(self.period - 1),
            |data| self.compute(data),
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

    /// The band between the outer band guides, whatever the middle guide's value.
    fn level_fill_indices(&self) -> Option<[usize; 2]> {
        Some([0, 1])
    }

    fn level_fill_color(&self) -> Color32 {
        Color32::from_rgba_unmultiplied(self.color.r(), self.color.g(), self.color.b(), 26)
    }

    /// The middle band is solid, the outer guides dashed.
    fn level_style(&self, value: f64) -> crate::model::LineStyle {
        if value == 0.5 {
            crate::model::LineStyle::Solid
        } else {
            crate::model::LineStyle::Dashed
        }
    }

    /// Shade only outside the fixed outer boundaries, at a tenth of full opacity.
    fn fills(&self) -> Vec<crate::studies::StudyFill> {
        use crate::studies::{FillBoundary, StudyFill};
        vec![
            StudyFill {
                first: FillBoundary::Plot(0),
                second: FillBoundary::Level(1.0),
                color: palette::RED.gamma_multiply_u8(26),
                inverted_color: Some(Color32::TRANSPARENT),
            },
            StudyFill {
                first: FillBoundary::Plot(0),
                second: FillBoundary::Level(0.0),
                color: Color32::TRANSPARENT,
                inverted_color: Some(palette::TEAL.gamma_multiply_u8(26)),
            },
        ]
    }

    fn pane(&self) -> crate::studies::StudyPane {
        crate::studies::StudyPane {
            levels: vec![
                (1.0, palette::RED.gamma_multiply_u8(128), "Overbought"),
                (0.0, palette::TEAL.gamma_multiply_u8(128), "Oversold"),
                (0.5, palette::BLUE.gamma_multiply_u8(128), "Middle Band"),
            ],
            fill_between_levels: true,
            ..Default::default()
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
        vec![format!("%B({},{})", self.period, self.multiplier)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::test_support::make_bar;

    /// Both the input editor and constructors enforce the supported deviation range.
    #[test]
    fn inputs_read_length_and_stddev_within_their_range() {
        use crate::studies::InputValue::Number;
        let mut study = PercentB::default();
        assert_eq!(
            study
                .inputs()
                .iter()
                .map(|input| input.label.as_str())
                .collect::<Vec<_>>(),
            ["Length", "StdDev"]
        );
        for (value, accepted) in [(0.0, false), (50.5, false), (0.001, true), (50.0, true)] {
            assert_eq!(
                study.set_input("multiplier", Number(value)).is_ok(),
                accepted
            );
        }
        for (value, want) in [(0.0, 0.001), (f64::NAN, 0.001), (1e9, 50.0)] {
            assert_eq!(PercentB::new(20, value).inputs()[1].value, Number(want));
        }
    }

    #[test]
    fn test_percent_b() {
        let mut pb = PercentB::new(5, 2.0);

        // Constant prices = no volatility = %B should be ~0.5
        let data: Vec<Bar> = (0..10).map(|_| make_bar(100.0)).collect();

        pb.calculate(&data);

        let rows = pb.values.to_rows();
        let IndicatorValue::Single(v) = rows.last().unwrap() else {
            panic!("no value: {:?}", pb.values.to_rows().last().unwrap());
        };
        // Constant prices collapse the bands to a point: %B is the middle of the band
        assert_eq!(*v, 0.5);
    }
}
