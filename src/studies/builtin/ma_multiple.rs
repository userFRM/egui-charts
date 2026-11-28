use super::ma_combination::impl_ma_preset;
use super::{AverageLine, AverageType, MACombination};
use crate::studies::palette;
use egui::Color32;

/// Simple averages of lengths 14, 21, 35, 50, 100 and 200, as a combination preset, without crossing
/// markers until a pair is chosen.
#[derive(Clone, Debug)]
pub struct MultipleMA {
    inner: MACombination,
}

impl MultipleMA {
    /// Create the six-average preset.
    pub fn new() -> Self {
        let colors = [
            Color32::from_rgb(156, 39, 176),
            palette::ORANGE,
            palette::DARK_GREEN,
            palette::CYAN,
            Color32::from_rgb(245, 0, 87),
            palette::LIGHT_BLUE,
        ];
        Self {
            inner: MACombination::new(
                [14, 21, 35, 50, 100, 200]
                    .into_iter()
                    .zip(colors)
                    .map(|(period, color)| AverageLine::new(AverageType::Simple, period, color))
                    .collect(),
            )
            .unwrap()
            .without_crossings()
            .with_shared_method()
            .with_max_period(10_000),
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
    "Multiple Moving Averages - SMA 14, 21, 35, 50, 100, 200"
);
