use super::ma_combination::impl_ma_preset;
use super::{AverageLine, AverageType, MACombination};
use crate::tokens::DESIGN_TOKENS;

/// EMA Cross, a preset of independently configurable averages.
#[derive(Clone)]
pub struct EMACross {
    inner: MACombination,
}

impl EMACross {
    /// Create two exponential averages with lengths clamped to at least one.
    pub fn new(fast_period: usize, slow_period: usize) -> Self {
        let inner = MACombination::new(vec![
            AverageLine::new(
                AverageType::Exponential,
                fast_period,
                DESIGN_TOKENS.semantic.extended.success,
            ),
            AverageLine::new(
                AverageType::Exponential,
                slow_period,
                DESIGN_TOKENS.semantic.extended.error,
            ),
        ])
        .unwrap();
        Self { inner }
    }
}

impl Default for EMACross {
    fn default() -> Self {
        Self::new(12, 26)
    }
}

impl_ma_preset!(
    EMACross,
    "EMA Cross",
    "EMA Cross - Two exponential averages with crossing markers"
);
