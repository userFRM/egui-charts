use super::ma_combination::impl_ma_preset;
use super::{AverageLine, AverageType, MACombination};
use egui::Color32;

/// Simple averages of lengths 20, 50, 100 and 200, as a combination preset.
#[derive(Clone)]
pub struct MultipleMA {
    inner: MACombination,
}

impl MultipleMA {
    /// Create the conventional four-average preset.
    pub fn new() -> Self {
        let colors = [
            Color32::from_rgb(41, 98, 255),
            Color32::from_rgb(255, 152, 0),
            Color32::from_rgb(239, 83, 80),
            Color32::from_rgb(156, 39, 176),
        ];
        Self {
            inner: MACombination::new(
                [20, 50, 100, 200]
                    .into_iter()
                    .zip(colors)
                    .map(|(period, color)| AverageLine::new(AverageType::Simple, period, color))
                    .collect(),
            )
            .unwrap(),
        }
    }
}
impl Default for MultipleMA {
    fn default() -> Self {
        Self::new()
    }
}
impl_ma_preset!(
    MultipleMA,
    "Multi MA",
    "Multiple Moving Averages - SMA 20, 50, 100, 200"
);
