use super::ma_combination::impl_ma_preset;
use super::{AverageLine, AverageType, MACombination};
use crate::studies::palette;

/// Simple averages of lengths 20, 50, 100 and 200, as a combination preset.
#[derive(Clone)]
pub struct MultipleMA {
    inner: MACombination,
}

impl MultipleMA {
    /// Create the conventional four-average preset.
    pub fn new() -> Self {
        let colors = [
            palette::BLUE,
            palette::ORANGE,
            palette::RED,
            palette::PURPLE,
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
