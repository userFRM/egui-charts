use super::over_finite_bars;
use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::{update_window, usable_behind};
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Vortex Indicator (VI)
/// Identifies trend direction and strength
/// Uses True Range and directional movement concepts
#[derive(Clone)]
pub struct VortexIndicator {
    /// Lookback period (typically 14)
    period: usize,
    values: IndicatorValues,
    /// Colors: [+VI (bullish), -VI (bearish)]
    colors: Vec<Color32>,
    visible: bool,
}

impl VortexIndicator {
    /// Create with a lookback of `period` bars, clamped to at least 1.
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            values: IndicatorValues::new(),
            colors: vec![
                palette::BLUE, // +VI
                palette::PINK, // -VI
            ],
            visible: true,
        }
    }

    /// Set plot colors in the order returned by [`Indicator::line_names`].
    pub fn with_colors(mut self, colors: Vec<Color32>) -> Self {
        self.colors = colors;
        self
    }
}

/// Construct with the conventional default parameters.
impl Default for VortexIndicator {
    fn default() -> Self {
        Self::new(14)
    }
}

impl VortexIndicator {
    /// The values over `data`, every bar of which has finite inputs.
    fn compute(&self, data: &[Bar]) -> IndicatorValues {
        let mut values = IndicatorValues::new();
        if data.len() < self.period.saturating_add(1) {
            for _ in 0..data.len() {
                values.push(IndicatorValue::None);
            }
            return values;
        }

        // Calculate True Range, +VM, -VM for each bar
        let mut tr_values = Vec::with_capacity(data.len());
        let mut vm_plus = Vec::with_capacity(data.len());
        let mut vm_minus = Vec::with_capacity(data.len());

        tr_values.push(0.0);
        vm_plus.push(0.0);
        vm_minus.push(0.0);

        for i in 1..data.len() {
            let current = &data[i];
            let prev = &data[i - 1];

            // True Range = max(High - Low, |High - Prev Close|, |Low - Prev Close|)
            let tr = (current.high - current.low)
                .max((current.high - prev.close).abs())
                .max((current.low - prev.close).abs());

            // +VM = |Current High - Previous Low|
            let vm_p = (current.high - prev.low).abs();

            // -VM = |Current Low - Previous High|
            let vm_m = (current.low - prev.high).abs();

            tr_values.push(tr);
            vm_plus.push(vm_p);
            vm_minus.push(vm_m);
        }

        // Calculate VI+ and VI-
        for i in 0..data.len() {
            if i < self.period {
                values.push(IndicatorValue::None);
            } else {
                // Sum over period
                let tr_sum: f64 = tr_values[i + 1 - self.period..=i].iter().sum();
                let vm_plus_sum: f64 = vm_plus[i + 1 - self.period..=i].iter().sum();
                let vm_minus_sum: f64 = vm_minus[i + 1 - self.period..=i].iter().sum();

                if tr_sum == 0.0 {
                    values.push(IndicatorValue::None);
                } else {
                    let vi_plus = vm_plus_sum / tr_sum;
                    let vi_minus = vm_minus_sum / tr_sum;
                    values.push_lines(&[vi_plus, vi_minus]);
                }
            }
        }

        values
    }
}

impl Indicator for VortexIndicator {
    crate::studies::inputs::study_inputs!(self;
        period: length("Period", 1, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "Vortex"
    }

    fn desc(&self) -> &str {
        "Vortex Indicator - Identifies trend direction"
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
        self.colors.clone()
    }

    fn set_colors(&mut self, colors: Vec<Color32>) {
        if colors.len() >= 2 {
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
        vec![
            format!("+VI({})", self.period),
            format!("-VI({})", self.period),
        ]
    }
}
