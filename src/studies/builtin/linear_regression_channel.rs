use super::clamp_param;
use crate::model::{Bar, PriceSource};
use crate::studies::math::{LinearFit, Moments};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use crate::tokens::DESIGN_TOKENS;
use egui::Color32;
use std::borrow::Cow;

/// Linear regression channel fitted to the closes of the last `period` bars.
///
/// Outputs upper, middle and lower lines across that window, with no value on
/// earlier bars or until a full window is available. The bands are `multiplier`
/// residual standard deviations from the line, using an n - 1 denominator.
/// Every calculation or update refits the whole channel. A window containing
/// a non-finite close has no values.
#[derive(Clone)]
pub struct LinearRegressionChannel {
    period: usize,
    multiplier: f64,
    values: IndicatorValues,
    colors: Vec<Color32>,
    visible: bool,
    source: PriceSource,
    extend: bool,
    fit: Option<LinearFit>,
}

impl LinearRegressionChannel {
    /// Create a channel with a period of at least 2. `multiplier` is clamped
    /// into 0 to `f64::MAX`, NaN to 0.
    pub fn new(period: usize, multiplier: f64) -> Self {
        Self {
            period: period.max(2),
            multiplier: clamp_param(multiplier, 0.0, f64::MAX),
            values: IndicatorValues::new(),
            colors: vec![
                DESIGN_TOKENS.semantic.indicators.bb_upper,
                DESIGN_TOKENS.semantic.indicators.bb_middle,
                DESIGN_TOKENS.semantic.indicators.bb_lower,
            ],
            visible: true,
            source: PriceSource::Close,
            extend: false,
            fit: None,
        }
    }
    /// Select the input price for the fit. Defaults to close.
    pub fn with_source(mut self, source: PriceSource) -> Self {
        self.source = source;
        self
    }

    /// Extend the three fitted lines to the right edge of the plot.
    pub fn with_extend_right(mut self, extend: bool) -> Self {
        self.extend = extend;
        self
    }

    /// Pearson correlation of the source with bar position, unavailable before a fit or on flat input.
    pub fn correlation(&self) -> Option<f64> {
        self.fit.and_then(|fit| fit.correlation)
    }

    /// Fraction of source variance explained by the line; a flat source is fitted exactly.
    pub fn r_squared(&self) -> Option<f64> {
        self.fit.map(|fit| fit.r_squared)
    }
}

impl Default for LinearRegressionChannel {
    fn default() -> Self {
        Self::new(20, 2.0)
    }
}

impl Indicator for LinearRegressionChannel {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 2, usize::MAX),
        multiplier: number("Multiplier", 0.0, f64::MAX),
        source: choice("Source", crate::model::PriceSource::all()),
        extend: toggle("Extend")
    ; {});

    fn name(&self) -> &str {
        "LinReg Channel"
    }

    fn desc(&self) -> &str {
        "Linear Regression Channel - Best fit line and residual deviation bands over the last bars"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], _from: usize) {
        self.fit = None;
        let previous = self.values.len().saturating_sub(self.period);
        self.values.truncate(previous);
        self.values.resize(data.len(), IndicatorValue::None);
        if data.len() < self.period {
            return;
        }
        let start = data.len() - self.period;
        let window: Cow<'_, [Bar]> = if self.source == PriceSource::Close {
            Cow::Borrowed(&data[start..])
        } else {
            Cow::Owned(
                data[start..]
                    .iter()
                    .map(|bar| {
                        let mut sourced = bar.clone();
                        sourced.close = self.source.compute(bar.open, bar.high, bar.low, bar.close);
                        sourced
                    })
                    .collect(),
            )
        };
        if window.iter().any(|bar| !bar.close.is_finite()) {
            return;
        }
        let Some(fit) = Moments::of(&window).fit() else {
            return;
        };
        self.fit = Some(fit);
        let offset = self.multiplier * fit.sigma;
        for i in 0..self.period {
            let middle = fit.intercept + fit.slope * i as f64;
            self.values.set(
                start + i,
                IndicatorValue::Multiple(vec![middle + offset, middle, middle - offset]),
            );
        }
    }

    fn is_incremental(&self) -> bool {
        true
    }

    fn extend_right(&self) -> bool {
        self.extend
    }

    fn plot_label(&self) -> Option<(usize, String)> {
        self.fit.map(|fit| {
            (
                1,
                fit.correlation
                    .map_or_else(|| "R: undefined".to_owned(), |r| format!("R: {r:.3}")),
            )
        })
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
            self.colors = vec![colors[0]; 3];
        }
    }

    fn is_overlay(&self) -> bool {
        true
    }

    fn line_cnt(&self) -> usize {
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
            format!("LinReg Upper({}, {})", self.period, self.multiplier),
            format!("LinReg Middle({})", self.period),
            format!("LinReg Lower({}, {})", self.period, self.multiplier),
        ]
    }
}
