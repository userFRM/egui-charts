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
            .without_crossings()
            .with_shared_method()
            .with_max_period(10_000),
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
            .without_crossings()
            .with_shared_method()
            .with_max_period(10_000),
        }
    }
}
impl Default for DoubleMA {
    fn default() -> Self {
        Self::new(14, 21)
    }
}
impl_ma_preset!(DoubleMA, "Double MA", "Two simple moving averages");

/// Four moving averages with independent formulas, sources and lengths.
#[derive(Clone, Debug)]
pub struct MARibbon {
    inner: MACombination,
}
impl MARibbon {
    /// Four simple averages of 20, 50, 100 and 200 bars from yellow to red,
    /// each with its own formula, source and length.
    pub fn new() -> Self {
        Self {
            inner: MACombination::new(
                [
                    (20, (246, 195, 9)),
                    (50, (251, 152, 0)),
                    (100, (251, 101, 0)),
                    (200, (246, 12, 12)),
                ]
                .into_iter()
                .map(|(period, (r, g, b))| {
                    AverageLine::new(
                        AverageType::Simple,
                        period,
                        egui::Color32::from_rgb(r, g, b),
                    )
                })
                .collect(),
            )
            .unwrap()
            .without_crossings(),
        }
    }
    /// Twelve exponential averages of lengths 5 through 60, coloured by their own direction,
    /// the slowest 3 px wide over a fading fill: the ribbon of the MA Exp Ribbon template.
    pub fn exponential() -> Self {
        Self {
            inner: MACombination::ribbon(),
        }
    }
    /// The twelve-average ribbon in dim magenta without a fill, each line 1 px wide.
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
    "Four moving averages of independent formula, source and length"
);
