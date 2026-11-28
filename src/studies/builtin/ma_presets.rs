use super::ma_combination::impl_ma_preset;
use super::{AverageLine, AverageType, MACombination};
use crate::studies::palette;

/// A simple average crossing an exponential one.
#[derive(Clone, Debug)]
pub struct MixedMACross {
    inner: MACombination,
}
impl MixedMACross {
    /// Simple and exponential lengths, each at least one.
    pub fn new(simple: usize, exponential: usize) -> Self {
        Self {
            inner: MACombination::new(vec![
                AverageLine::new(AverageType::Simple, simple, palette::ORANGE),
                AverageLine::new(AverageType::Exponential, exponential, palette::DARK_GREEN),
            ])
            .unwrap()
            .with_crosses(palette::LIGHT_BLUE)
            .with_max_period(2000),
        }
    }
}
impl Default for MixedMACross {
    fn default() -> Self {
        Self::new(10, 10)
    }
}
impl_ma_preset!(
    MixedMACross,
    "SMA/EMA Cross",
    "Simple and exponential moving averages with crossing markers"
);

/// Three simple averages with independently chosen lengths.
#[derive(Clone, Debug)]
pub struct TripleMA {
    inner: MACombination,
}
impl TripleMA {
    /// Three lengths, each clamped to at least one. Marks no crossings until a pair is chosen.
    pub fn new(fast: usize, middle: usize, slow: usize) -> Self {
        let colors = [palette::ORANGE, palette::LIGHT_BLUE, palette::CYAN];
        Self {
            inner: MACombination::new(
                [fast, middle, slow]
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
impl Default for TripleMA {
    fn default() -> Self {
        Self::new(14, 21, 35)
    }
}
impl_ma_preset!(TripleMA, "Triple MA", "Three simple moving averages");

/// Two simple averages with independently chosen lengths, without crossing markers until a
/// pair is chosen. [`MACross`](super::MACross) marks the crossings of two averages.
#[derive(Clone, Debug)]
pub struct DoubleMA {
    inner: MACombination,
}
impl DoubleMA {
    ///     let _ = egui_charts::studies::DoubleMA::new(14, 21);
    pub fn new(first: usize, second: usize) -> Self {
        Self {
            inner: MACombination::new(vec![
                AverageLine::new(AverageType::Simple, first, palette::ORANGE),
                AverageLine::new(AverageType::Simple, second, palette::LIGHT_BLUE),
            ])
            .unwrap()
            .without_crossings(),
        }
    }
}
impl Default for DoubleMA {
    fn default() -> Self {
        Self::new(14, 21)
    }
}
impl_ma_preset!(DoubleMA, "Double MA", "Two simple moving averages");

/// Twelve evenly spaced exponential averages coloured by their individual direction.
#[derive(Clone, Debug)]
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
