use super::ma_combination::impl_ma_preset;
use super::{AverageLine, AverageType, MACombination};
use egui::Color32;

/// Simple averages of lengths 20, 50, 100 and 200, as a combination preset, without crossing
/// markers until a pair is chosen.
#[derive(Clone)]
pub struct MultipleMA {
    inner: MACombination,
}

impl MultipleMA {
    /// Create the conventional four-average preset.
    pub fn new() -> Self {
        let colors = [
            Color32::from_rgb(246, 195, 9),
            Color32::from_rgb(251, 152, 0),
            Color32::from_rgb(251, 101, 0),
            Color32::from_rgb(246, 12, 12),
        ];
        Self {
            inner: MACombination::new(
                [20, 50, 100, 200]
                    .into_iter()
                    .zip(colors)
                    .map(|(period, color)| AverageLine::new(AverageType::Simple, period, color))
                    .collect(),
            )
            .unwrap()
            .without_crossings(),
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
