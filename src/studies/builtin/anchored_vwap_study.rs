use crate::model::Bar;
use crate::studies::palette;
use crate::studies::stepper::Stepper;
use crate::studies::{Indicator, IndicatorValue, IndicatorValues};
use egui::Color32;

/// Anchored VWAP - VWAP calculated from a specific anchor bar index forward.
/// typical_price = (high + low + close) / 3
/// VWAP = cumsum(volume * typical_price) / cumsum(volume) from anchor_bar onward.
#[derive(Clone)]
pub struct AnchoredVWAP {
    anchor_bar: usize,
    values: IndicatorValues,
    stepper: Stepper<(usize, f64, f64)>,
    colors: Vec<Color32>,
    visible: bool,
}

impl AnchoredVWAP {
    /// Create with a zero-based anchor bar index for volume-weighted averaging.
    pub fn new(anchor_bar: usize) -> Self {
        Self {
            anchor_bar,
            values: IndicatorValues::new(),
            stepper: Stepper::default(),
            colors: vec![palette::YELLOW],
            visible: true,
        }
    }
}

impl Default for AnchoredVWAP {
    fn default() -> Self {
        Self::new(0)
    }
}

impl Indicator for AnchoredVWAP {
    fn source(&self) -> Option<crate::studies::StudySource> {
        Some(crate::studies::StudySource::Price(
            crate::model::PriceSource::HLC3,
        ))
    }

    crate::studies::inputs::study_inputs!(self;
        anchor_bar: length("Anchor bar", 0, usize::MAX)
    ; {});

    fn name(&self) -> &str {
        "AVWAP"
    }

    fn desc(&self) -> &str {
        "Anchored VWAP - Volume weighted average price from anchor bar"
    }

    fn calculate(&mut self, data: &[Bar]) {
        self.update(data, 0);
    }

    fn update(&mut self, data: &[Bar], from: usize) {
        self.stepper
            .run(data, from, &mut self.values, |state, bar| {
                let index = state.0;
                state.0 += 1;
                if index < self.anchor_bar
                    || ![bar.high, bar.low, bar.close, bar.volume]
                        .iter()
                        .all(|x| x.is_finite())
                {
                    return IndicatorValue::None;
                }
                let price = bar.typical_price();
                state.1 += price * bar.volume;
                state.2 += bar.volume;
                IndicatorValue::Single(if state.2 == 0.0 {
                    price
                } else {
                    state.1 / state.2
                })
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
        if !colors.is_empty() {
            self.colors = colors;
        }
    }

    fn is_overlay(&self) -> bool {
        true
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
        vec![format!("AVWAP({})", self.anchor_bar)]
    }
}
