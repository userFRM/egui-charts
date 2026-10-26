use super::ma_combination::impl_ma_preset;
use super::{AverageLine, AverageType, MACombination};
use crate::studies::palette;

/// MA Cross, a preset of independently configurable averages.
#[derive(Clone)]
pub struct MACross {
    inner: MACombination,
}

impl MACross {
    /// Create two simple averages with lengths clamped to at least one.
    pub fn new(fast_period: usize, slow_period: usize) -> Self {
        let mut inner = MACombination::new(vec![
            AverageLine::new(AverageType::Simple, fast_period, palette::GREEN),
            AverageLine::new(AverageType::Simple, slow_period, palette::RED),
        ])
        .unwrap();
        inner.wait_for_all = true;
        Self { inner }
    }
}

impl Default for MACross {
    fn default() -> Self {
        Self::new(9, 21)
    }
}

impl_ma_preset!(
    MACross,
    "MA Cross",
    "MA Cross - Two simple averages with crossing markers"
);
