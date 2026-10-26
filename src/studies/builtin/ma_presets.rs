use super::ma_combination::impl_ma_preset;
use super::{AverageLine, AverageType, MACombination};
use crate::studies::palette;

/// A simple average crossing an exponential one.
#[derive(Clone)]
pub struct MixedMACross {
    inner: MACombination,
}
impl MixedMACross {
    /// Simple fast and exponential slow lengths, each at least one.
    pub fn new(fast: usize, slow: usize) -> Self {
        Self {
            inner: MACombination::new(vec![
                AverageLine::new(AverageType::Simple, fast, palette::GREEN),
                AverageLine::new(AverageType::Exponential, slow, palette::RED),
            ])
            .unwrap(),
        }
    }
}
impl Default for MixedMACross {
    fn default() -> Self {
        Self::new(9, 21)
    }
}
impl_ma_preset!(
    MixedMACross,
    "SMA/EMA Cross",
    "Simple and exponential moving averages with crossing markers"
);

/// Three simple averages with independently chosen lengths.
#[derive(Clone)]
pub struct TripleMA {
    inner: MACombination,
}
impl TripleMA {
    /// Three lengths, each clamped to at least one. Marks crossings of the first two.
    pub fn new(fast: usize, middle: usize, slow: usize) -> Self {
        let colors = [palette::GREEN, palette::ORANGE, palette::RED];
        Self {
            inner: MACombination::new(
                [fast, middle, slow]
                    .into_iter()
                    .zip(colors)
                    .map(|(period, color)| AverageLine::new(AverageType::Simple, period, color))
                    .collect(),
            )
            .unwrap(),
        }
    }
}
impl Default for TripleMA {
    fn default() -> Self {
        Self::new(9, 21, 50)
    }
}
impl_ma_preset!(
    TripleMA,
    "Triple MA",
    "Three simple moving averages with crossing markers"
);

/// Twelve evenly spaced exponential averages coloured by their individual direction.
#[derive(Clone)]
pub struct MARibbon {
    inner: MACombination,
}
impl MARibbon {
    /// Direction-coloured ribbon with a thicker slowest line and fading bottom fill.
    pub fn new() -> Self {
        Self {
            inner: MACombination::ribbon(),
        }
    }
    /// Dim magenta lines without a fill, each 1 px wide.
    pub fn muted() -> Self {
        Self {
            inner: MACombination::muted_ribbon(),
        }
    }
}
impl Default for MARibbon {
    fn default() -> Self {
        Self::new()
    }
}
impl_ma_preset!(
    MARibbon,
    "MA Ribbon",
    "Twelve exponential averages of lengths 5 through 60"
);
