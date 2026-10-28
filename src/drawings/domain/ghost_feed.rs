//! The projected candles of a Ghost Feed drawing.

use super::ChartPoint;
use serde::{Deserialize, Serialize};

/// The most candles one Ghost Feed projects, however small its spacing.
pub const MAX_GHOST_CANDLES: usize = 1000;

/// How a Ghost Feed builds its projected candles from the clicked path.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GhostFeedConfig {
    /// Bars one projected candle spans; the first starts at the path's first anchor. Must be
    /// positive and finite.
    pub spacing: f64,
    /// Extra wick above the high and below the low, as a fraction of the candle's range. Must be
    /// nonnegative and finite.
    pub wick: f64,
}

impl Default for GhostFeedConfig {
    fn default() -> Self {
        Self {
            spacing: 1.0,
            wick: 0.25,
        }
    }
}

/// One projected candle: where the path opens and closes over its bars, and the highest and
/// lowest price it reaches in between, widened by the wick.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GhostCandle {
    /// Bar index where the candle opens.
    pub bar_idx: f32,
    /// Price at the open.
    pub open: f64,
    /// Highest price, at least the open and the close.
    pub high: f64,
    /// Lowest price, at most the open and the close.
    pub low: f64,
    /// Price at the close, `spacing` bars after the open.
    pub close: f64,
}

impl GhostFeedConfig {
    /// Whether the spacing is positive and finite and the wick nonnegative and finite.
    pub fn is_valid(&self) -> bool {
        self.spacing.is_finite() && self.spacing > 0.0 && self.wick.is_finite() && self.wick >= 0.0
    }

    /// The candles that follow `path` from its first to its last bar, up to
    /// [`MAX_GHOST_CANDLES`]. The path runs straight between its anchors taken in bar order; a
    /// final stretch shorter than the spacing, an invalid config or a path with less than two
    /// bars projects nothing.
    pub fn candles(&self, path: &[ChartPoint]) -> Vec<GhostCandle> {
        let mut path: Vec<(f64, f64)> = path
            .iter()
            .filter(|p| p.bar_idx.is_finite() && p.price.is_finite())
            .map(|p| (f64::from(p.bar_idx), p.price))
            .collect();
        path.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (Some(&(first, _)), Some(&(last, _))) = (path.first(), path.last()) else {
            return Vec::new();
        };
        if !self.is_valid() {
            return Vec::new();
        }
        let price_at = |bar: f64| {
            let i = path
                .partition_point(|p| p.0 <= bar)
                .clamp(1, path.len() - 1);
            let ((b0, p0), (b1, p1)) = (path[i - 1], path[i]);
            if b1 > b0 {
                p0 + (p1 - p0) * ((bar - b0) / (b1 - b0)).clamp(0.0, 1.0)
            } else {
                p1
            }
        };
        // The tolerance keeps a span of exactly n spacings from losing its last candle to rounding.
        let count = (((last - first) / self.spacing) * (1.0 + 1e-9)).floor();
        (0..count.min(MAX_GHOST_CANDLES as f64) as usize)
            .map(|i| {
                let (from, to) = (
                    first + i as f64 * self.spacing,
                    first + (i + 1) as f64 * self.spacing,
                );
                let (open, close) = (price_at(from), price_at(to));
                let (high, low) = path
                    .iter()
                    .filter(|p| p.0 > from && p.0 < to)
                    .fold((open.max(close), open.min(close)), |(h, l), p| {
                        (h.max(p.1), l.min(p.1))
                    });
                let extra = (high - low) * self.wick;
                GhostCandle {
                    bar_idx: from as f32,
                    open,
                    high: high + extra,
                    low: low - extra,
                    close,
                }
            })
            .collect()
    }
}

/// A projected candle in screen space: the pixel position of its centre and the half width of its
/// body, and of the prices of its open, high, low and close, and whether it closes above its open.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct GhostCandleScreen {
    pub x: f32,
    pub half_width: f32,
    pub open: f32,
    pub high: f32,
    pub low: f32,
    pub close: f32,
    pub rising: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Anchors out of bar order are put in order. The path (0, 10), (1, 20), (4, 10) falls from 20
    /// to 10 over three bars, so at bar 2 it is 20 - 10 / 3 = 50 / 3. Candles of two bars open and
    /// close on the path, reach its peak, and add a quarter of their range as wick.
    #[test]
    fn candles_follow_the_path_between_its_anchors() {
        let path = [
            ChartPoint::new(4.0, 10.0),
            ChartPoint::new(0.0, 10.0),
            ChartPoint::new(1.0, 20.0),
        ];
        let config = GhostFeedConfig {
            spacing: 2.0,
            wick: 0.25,
        };
        let got: Vec<_> = config
            .candles(&path)
            .iter()
            .map(|c| [f64::from(c.bar_idx), c.open, c.high, c.low, c.close])
            .collect();
        let want = [
            [0.0, 10.0, 22.5, 7.5, 50.0 / 3.0],
            [2.0, 50.0 / 3.0, 55.0 / 3.0, 25.0 / 3.0, 10.0],
        ];
        assert_eq!(got.len(), want.len());
        for (got, want) in got.iter().zip(want) {
            for (got, want) in got.iter().zip(want) {
                assert!((got - want).abs() < 1e-6, "{got:?} != {want:?}");
            }
        }
        for config in [
            GhostFeedConfig {
                spacing: 0.0,
                ..config
            },
            GhostFeedConfig {
                wick: f64::NAN,
                ..config
            },
        ] {
            assert!(config.candles(&path).is_empty());
        }
    }
}
