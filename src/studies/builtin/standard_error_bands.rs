use super::clamp_param;
use super::smoothing::{Smoothed, SmoothingType};
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;
use std::collections::VecDeque;

/// The averages the regression can be smoothed with, in the order the type input lists them.
const SMOOTHING: [SmoothingType; 6] = [
    SmoothingType::None,
    SmoothingType::Sma,
    SmoothingType::Ema,
    SmoothingType::Smma,
    SmoothingType::Wma,
    SmoothingType::Vwma,
];
/// The positions in [`SMOOTHING`] that use a length.
const WITH_LENGTH: [usize; 5] = [1, 2, 3, 4, 5];

/// Standard Error Bands - upper/middle (linear regression)/lower bands
/// where upper = LinReg + multiplier * StdError, lower = LinReg - multiplier * StdError.
///
/// The smoothing inputs average the regression value and its standard error before the bands are
/// formed, so the middle line and the distance of the bands from it follow the chosen average.
/// With no smoothing, the default, both are the values of the latest `period` closes. The three
/// lines are undefined while the window or the average lacks bars. A missing close makes them
/// undefined while it is in the window; an SMA, WMA or VWMA stays undefined until it has left the
/// average too, where an EMA or SMMA resumes from the value it held before the gap.
#[derive(Clone)]
pub struct StandardErrorBands {
    period: usize,
    multiplier: f64,
    smoothing: SmoothingType,
    smoothing_length: usize,
    values: IndicatorValues,
    stepper: Stepper<Rolling>,
    colors: Vec<Color32>,
    visible: bool,
}

/// The latest `period` closes, and the averages of the regression value and its standard error.
#[derive(Clone, Default)]
struct Rolling {
    window: VecDeque<f64>,
    middle: Smoothed,
    error: Smoothed,
}

impl StandardErrorBands {
    /// `multiplier` is clamped into 0 to `f64::MAX`, NaN to 0.
    pub fn new(period: usize, multiplier: f64) -> Self {
        Self {
            period: period.max(2),
            multiplier: clamp_param(multiplier, 0.0, f64::MAX),
            smoothing: SmoothingType::None,
            smoothing_length: 3,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: vec![palette::LIGHT_BLUE, palette::ORANGE, palette::LIGHT_BLUE],
            visible: true,
        }
    }

    /// Compute linear regression slope, intercept, and standard error.
    fn linreg_with_se(prices: &[f64]) -> (f64, f64, f64) {
        let n = prices.len() as f64;
        let mut sum_x = 0.0;
        let mut sum_y = 0.0;
        let mut sum_xy = 0.0;
        let mut sum_x2 = 0.0;

        for (j, &y) in prices.iter().enumerate() {
            let x = j as f64;
            sum_x += x;
            sum_y += y;
            sum_xy += x * y;
            sum_x2 += x * x;
        }

        let denom = n * sum_x2 - sum_x * sum_x;
        if denom.abs() < 1e-15 {
            let mean = sum_y / n;
            return (0.0, mean, 0.0);
        }

        let slope = (n * sum_xy - sum_x * sum_y) / denom;
        let intercept = (sum_y - slope * sum_x) / n;

        let sum_sq_err: f64 = prices
            .iter()
            .enumerate()
            .map(|(j, &y)| {
                let predicted = intercept + slope * j as f64;
                (y - predicted).powi(2)
            })
            .sum();

        let se = if n > 2.0 {
            (sum_sq_err / (n - 2.0)).sqrt()
        } else {
            0.0
        };

        (slope, intercept, se)
    }
}

impl Default for StandardErrorBands {
    fn default() -> Self {
        Self::new(20, 2.0)
    }
}

impl Indicator for StandardErrorBands {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::Close,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 2, usize::MAX),
        multiplier: number("Multiplier", 0.0, f64::MAX),
        smoothing: choice("Smoothing", SMOOTHING),
        smoothing_length: length("Smoothing length", 1, usize::MAX) if smoothing in WITH_LENGTH
    ; {});

    fn name(&self) -> &str {
        "StdErrBands"
    }

    fn desc(&self) -> &str {
        "Standard Error Bands - Linear regression with standard error envelope"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        let (period, multiplier) = (self.period, self.multiplier);
        let (kind, length) = (self.smoothing, self.smoothing_length);
        let mut prices = Vec::new();
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                state.window.push_back(bar.close);
                if state.window.len() > period {
                    state.window.pop_front();
                }
                if state.window.len() < period {
                    return IndicatorValue::None;
                }
                prices.clear();
                prices.extend(&state.window);
                // A missing close in the window makes all three values NaN.
                let (slope, intercept, se) = Self::linreg_with_se(&prices);
                let middle = intercept + slope * (period - 1) as f64;
                let average = |smoothed: &mut Smoothed, value| match smoothed
                    .next(kind, length, 0.0, value, bar.volume)
                {
                    IndicatorValue::Multiple(values) => values[1],
                    _ => value,
                };
                let middle = average(&mut state.middle, middle);
                let se = average(&mut state.error, se);
                IndicatorValue::Multiple(vec![
                    middle + multiplier * se,
                    middle,
                    middle - multiplier * se,
                ])
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
        if colors.len() >= 3 {
            self.colors = colors;
        } else if !colors.is_empty() {
            self.colors = vec![colors[0], colors[0], colors[0]];
        }
    }

    fn is_overlay(&self) -> bool {
        true
    }

    fn line_count(&self) -> usize {
        3
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
            format!("SE Upper({}, {})", self.period, self.multiplier),
            format!("SE Middle({})", self.period),
            format!("SE Lower({}, {})", self.period, self.multiplier),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::studies::{InputValue, StudyTemplate};

    /// Bars whose open, high and low lie away from the close and whose volumes differ.
    fn bars(closes: &[f64]) -> Vec<Bar> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &close)| {
                let time = chrono::DateTime::from_timestamp(i as i64 * 60, 0).unwrap();
                let volume = [1.0, 3.0, 2.0, 4.0, 5.0, 6.0, 7.0][i % 7];
                Bar::new(time, close - 0.5, close + 2.0, close - 1.0, close, volume)
            })
            .collect()
    }

    fn assert_lines(study: &StandardErrorBands, bar: usize, want: [Option<f64>; 3]) {
        for (line, want) in want.into_iter().enumerate() {
            let got = study.values().value(line, bar);
            let equal = match (got, want) {
                (Some(got), Some(want)) => (got - want).abs() < 1e-9,
                (got, want) => got == want,
            };
            assert!(equal, "bar {bar}, line {line}: {got:?}, not {want:?}");
        }
    }

    fn bands(middle: f64, error: f64) -> [Option<f64>; 3] {
        [
            Some(middle + 2.0 * error),
            Some(middle),
            Some(middle - 2.0 * error),
        ]
    }

    /// Three-bar fits by hand, with x = 0, 1, 2 so the slope is (last - first) / 2, the endpoint
    /// the mean plus the slope, and the squared error the spread about the mean minus twice the
    /// squared slope (one degree of freedom left). Closes 1, 2, 2 give an endpoint 13/6 and an
    /// error 1/sqrt(6); 2, 2, 4 (tied first closes) give 11/3 and sqrt(2/3); 2, 4, 3 give 7/2 and
    /// sqrt(3/2). Each type then averages two of these on volumes 4 and 5 for the bars it reads:
    /// the SMA is their midpoint, the WMA weighs them 1 and 2, the VWMA 4 and 5, and the EMA and
    /// SMMA start from the mean of the first two and add 2/3 and 1/2 of the step to the third.
    #[test]
    fn the_smoothing_averages_the_regression_and_its_error() {
        let (m, e) = (
            [13.0 / 6.0, 11.0 / 3.0, 3.5],
            [1.0 / 6.0f64.sqrt(), (2.0f64 / 3.0).sqrt(), 1.5f64.sqrt()],
        );
        let at_bar_4: [fn(f64, f64, f64) -> f64; 6] = [
            |_, _, c| c,
            |_, b, c| (b + c) / 2.0,
            |a, b, c| (a + b) / 6.0 + 2.0 * c / 3.0,
            |a, b, c| (a + b) / 4.0 + c / 2.0,
            |_, b, c| (b + 2.0 * c) / 3.0,
            |_, b, c| (4.0 * b + 5.0 * c) / 9.0,
        ];
        for (choice, average) in at_bar_4.into_iter().enumerate() {
            let mut study = StandardErrorBands::new(3, 2.0);
            study
                .set_input("smoothing", InputValue::Choice(choice))
                .unwrap();
            study
                .set_input("smoothing_length", InputValue::Length(2))
                .unwrap();
            study.calculate(&bars(&[1.0, 2.0, 2.0, 4.0, 3.0]));
            let (middle, error) = (average(m[0], m[1], m[2]), average(e[0], e[1], e[2]));
            assert_lines(&study, 4, bands(middle, error));
            if choice == 0 {
                assert_lines(&study, 2, bands(m[0], e[0]));
            } else {
                // The first bar the average covers.
                assert_lines(&study, 2, [None; 3]);
            }
        }
    }

    /// A missing close leaves every band undefined while it is in the three-bar window and then
    /// while the two-bar average still holds a value from it. After closes 4, 3, 5 (endpoint 9/2,
    /// error sqrt(3/2)) and 3, 5, 6 (endpoint 37/6, error 1/sqrt(6)) the average of both is the
    /// midpoint. Correcting the third close from 2 to 3 turns the first windows into 1, 2, 3 and
    /// 2, 3, 4, exact lines with endpoints 3 and 4, and the next into 3, 4, 3: 10/3 and
    /// sqrt(2/3), so the averages move from that bar on and an earlier bar stays as it was.
    #[test]
    fn a_missing_close_and_a_historical_correction_reach_every_later_band() {
        let mut study = StandardErrorBands::new(3, 2.0);
        for (id, value) in [
            ("smoothing", InputValue::Choice(1)),
            ("smoothing_length", InputValue::Length(2)),
        ] {
            study.set_input(id, value).unwrap();
        }
        study.calculate(&bars(&[1.0, 2.0, 2.0, f64::NAN, 4.0, 3.0, 5.0, 6.0]));
        for bar in 3..7 {
            assert_lines(&study, bar, [None; 3]);
        }
        let error = (1.5f64.sqrt() + (1.0 / 6.0f64.sqrt())) / 2.0;
        assert_lines(&study, 7, bands((4.5 + 37.0 / 6.0) / 2.0, error));

        study.calculate(&bars(&[1.0, 2.0, 2.0, 4.0, 3.0]));
        study.update(&bars(&[1.0, 2.0, 3.0, 4.0, 3.0]), 2);
        assert_lines(&study, 2, [None; 3]);
        assert_lines(&study, 3, bands(3.5, 0.0));
        assert_lines(
            &study,
            4,
            bands((4.0 + 10.0 / 3.0) / 2.0, (2.0f64 / 3.0).sqrt() / 2.0),
        );
    }

    /// A study saved before the smoothing inputs existed restores unsmoothed, with its three plots.
    #[test]
    fn a_study_saved_before_the_smoothing_inputs_restores_unsmoothed() {
        let saved: StudyTemplate = serde_json::from_str(
            r#"{"name":"StdErrBands","inputs":[["period",{"Length":3}],["multiplier",{"Number":2.0}]],"settings":{"show_legend_values":true,"user_edit_enabled":true,"help_url":"https://userfrm.github.io/egui-charts/docs/api/egui_charts/studies/struct.StandardErrorBands.html","plots":[{"color":[33,150,243,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true},{"color":[255,109,0,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true},{"color":[33,150,243,255],"width":1.0,"pattern":"Solid","plot_type":"Line","visible":true}],"precision":null,"hidden_intervals":[],"interval_range":"All","levels":[],"fill_between_levels":false,"level_fill_color":[2,9,15,16],"untagged_plots":[],"price_line_plots":[],"plot_variants":[[],[],[]],"background":null,"background_visible":true,"fills":[],"gradients":[]},"visible":true,"scale":"Main"}"#,
        )
        .unwrap();
        let mut study = saved.create().unwrap();
        assert_eq!(study.line_count(), 3);
        study.calculate(&bars(&[1.0, 2.0, 2.0]));
        let values = study.values();
        let (middle, error) = (13.0 / 6.0, 1.0 / 6.0f64.sqrt());
        for (line, want) in [middle + 2.0 * error, middle, middle - 2.0 * error]
            .into_iter()
            .enumerate()
        {
            assert!(
                (values.value(line, 2).unwrap() - want).abs() < 1e-9,
                "line {line}"
            );
        }
    }
}
