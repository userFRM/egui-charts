//! The projected candles of a Ghost Feed drawing.

use super::{ChartPoint, Drawing};
use serde::{Deserialize, Serialize};

/// The most candles one Ghost Feed projects.
pub const MAX_GHOST_CANDLES: usize = 1000;

/// How a Ghost Feed builds its projected candles from the clicked path.
/// Older saved `spacing` and `wick` settings are ignored and use these defaults.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GhostFeedConfig {
    /// Target average high-low range in ticks, before widening to contain the body and path.
    /// Must be positive and finite.
    pub average_range_ticks: f64,
    /// Maximum close displacement from the path, and range variation, in ticks.
    /// Zero follows the path exactly. Must be nonnegative and finite.
    pub noise_ticks: f64,
}

impl Default for GhostFeedConfig {
    fn default() -> Self {
        Self {
            average_range_ticks: 10.0,
            noise_ticks: 5.0,
        }
    }
}

/// One projected candle, with its body following the noisy path and wicks widened to the
/// configured range when the body and intervening anchors fit inside it.
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
    /// Price at the close, one bar after the open.
    pub close: f64,
}

impl GhostFeedConfig {
    /// Whether the range is positive and finite and the noise nonnegative and finite.
    pub fn is_valid(&self) -> bool {
        self.average_range_ticks.is_finite()
            && self.average_range_ticks > 0.0
            && self.noise_ticks.is_finite()
            && self.noise_ticks >= 0.0
    }

    /// One candle per whole bar from the first to the last anchor in bar order, up to
    /// [`MAX_GHOST_CANDLES`]. Each open is the preceding close; the first opens on the path.
    /// Closes follow the linear path plus uniform noise, seeded by the persistent drawing id.
    /// Range noise is also uniform; negative ranges become zero and bodies always fit.
    /// Invalid inputs or overflowing prices produce no candles.
    pub fn candles(
        &self,
        path: &[ChartPoint],
        tick_size: f64,
        drawing_id: &str,
    ) -> Vec<GhostCandle> {
        if !self.is_valid() || !tick_size.is_finite() || tick_size <= 0.0 {
            return Vec::new();
        }
        let mut path: Vec<_> = path
            .iter()
            .map(|p| (f64::from(p.bar_idx), p.price))
            .collect();
        if path.len() < 2 || path.iter().any(|p| !p.0.is_finite() || !p.1.is_finite()) {
            return Vec::new();
        }
        path.sort_by(|a, b| a.0.total_cmp(&b.0));
        let (first, first_price) = path[0];
        let last = path[path.len() - 1].0;
        let price_at = |bar: f64| {
            let i = path
                .partition_point(|p| p.0 <= bar)
                .clamp(1, path.len() - 1);
            let ((b0, p0), (b1, p1)) = (path[i - 1], path[i]);
            if b1 > b0 {
                let t = ((bar - b0) / (b1 - b0)).clamp(0.0, 1.0);
                p0 * (1.0 - t) + p1 * t
            } else {
                p1
            }
        };
        // Fixed integer arithmetic keeps the sequence independent of platform and redraw order.
        let mut state = drawing_id
            .bytes()
            .fold(0xcbf29ce484222325u64, |hash, byte| {
                (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3)
            });
        let mut noise = || {
            state = state.wrapping_add(0x9e3779b97f4a7c15);
            let mut value = state;
            value = (value ^ (value >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            value = (value ^ (value >> 27)).wrapping_mul(0x94d049bb133111eb);
            value ^= value >> 31;
            ((value >> 11) as f64 / (1u64 << 53) as f64 * 2.0 - 1.0) * self.noise_ticks
        };
        let count = (last - first).floor().min(MAX_GHOST_CANDLES as f64) as usize;
        let mut candles = Vec::with_capacity(count);
        let mut open = first_price;
        for i in 0..count {
            let from = first + i as f64;
            let to = from + 1.0;
            let close = price_at(to) + noise() * tick_size;
            let range = (self.average_range_ticks + noise()).max(0.0) * tick_size;
            let (high, low) = path
                .iter()
                .filter(|p| p.0 > from && p.0 < to)
                .fold((open.max(close), open.min(close)), |(h, l), p| {
                    (h.max(p.1), l.min(p.1))
                });
            let extra = ((range - (high - low)) / 2.0).max(0.0);
            let (high, low) = (high + extra, low - extra);
            if ![open, close, range, high, low]
                .into_iter()
                .all(f64::is_finite)
            {
                return Vec::new();
            }
            candles.push(GhostCandle {
                bar_idx: from as f32,
                open,
                high,
                low,
                close,
            });
            open = close;
        }
        candles
    }
}

impl Drawing {
    pub(crate) fn ghost_candles(&self, tick_size: f64) -> Vec<GhostCandle> {
        let id = self
            .stored_id
            .clone()
            .unwrap_or_else(|| self.id.to_string());
        self.ghost_feed
            .unwrap_or_default()
            .candles(&self.chart_points, tick_size, &id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Range 48 ticks at 0.25 is 12. The path falls 10 / 3 per bar after its peak;
    /// with no noise each high/low is the open-close midpoint plus/minus 6.
    #[test]
    fn candles_follow_the_path_between_its_anchors() {
        let path = [
            ChartPoint::new(4.0, 10.0),
            ChartPoint::new(0.0, 10.0),
            ChartPoint::new(1.0, 20.0),
        ];
        let config = GhostFeedConfig {
            average_range_ticks: 48.0,
            noise_ticks: 0.0,
        };
        let got = config.candles(&path, 0.25, "1404");
        let want = [
            [0.0, 10.0, 21.0, 9.0, 20.0],
            [1.0, 20.0, 73.0 / 3.0, 37.0 / 3.0, 50.0 / 3.0],
            [2.0, 50.0 / 3.0, 21.0, 9.0, 40.0 / 3.0],
            [3.0, 40.0 / 3.0, 53.0 / 3.0, 17.0 / 3.0, 10.0],
        ];
        assert_eq!(got.len(), want.len());
        for (got, want) in got.iter().zip(want) {
            for (got, want) in [
                f64::from(got.bar_idx),
                got.open,
                got.high,
                got.low,
                got.close,
            ]
            .into_iter()
            .zip(want)
            {
                assert!((got - want).abs() < 1e-6, "{got:?} != {want:?}");
            }
        }
        let mut drawing = Drawing::new(1404, super::super::DrawingToolType::GhostFeed);
        drawing.chart_points = vec![ChartPoint::new(0.0, 10.0), ChartPoint::new(32.0, 11.0)];
        // Legacy documents ignore spacing and wick and get the tick-based defaults.
        drawing.ghost_feed = Some(serde_json::from_str(r#"{"spacing":2.5,"wick":0.5}"#).unwrap());
        assert_eq!(drawing.ghost_feed.unwrap(), GhostFeedConfig::default());
        let noisy = drawing.ghost_candles(0.25);
        assert_eq!(noisy.len(), 32);
        assert!(noisy.iter().any(|c| c.close > c.open));
        assert!(noisy.iter().any(|c| c.close < c.open));
        for pair in noisy.windows(2) {
            assert_eq!(pair[0].close, pair[1].open);
        }
        for c in &noisy {
            let path_close = 10.0 + (f64::from(c.bar_idx) + 1.0) / 32.0;
            assert!((c.close - path_close).abs() <= 1.25);
            assert!(c.high >= c.open.max(c.close) && c.low <= c.open.min(c.close));
        }
        assert_eq!(noisy, drawing.ghost_candles(0.25));
        use crate::drawings::persistence::{StoredDrawing, drawing_to_stored, stored_to_drawing};
        let stored = drawing_to_stored(&drawing, |bar| bar as i64 * 1000);
        let json = serde_json::to_string(&stored).unwrap();
        let stored: StoredDrawing = serde_json::from_str(&json).unwrap();
        let restored = stored_to_drawing(&stored, |time| time as f32 / 1000.0, 99);
        assert_eq!(noisy, restored.ghost_candles(0.25));
        for (config, tick, path) in [
            (
                GhostFeedConfig {
                    average_range_ticks: 0.0,
                    ..config
                },
                0.25,
                path.to_vec(),
            ),
            (
                GhostFeedConfig {
                    noise_ticks: f64::NAN,
                    ..config
                },
                0.25,
                path.to_vec(),
            ),
            (config, 0.0, path.to_vec()),
            (config, f64::MAX, path.to_vec()),
            (config, 0.25, vec![]),
            (config, 0.25, vec![path[0]]),
            (config, 0.25, vec![path[0], path[0]]),
        ] {
            assert!(config.candles(&path, tick, "1404").is_empty());
        }
        let long = [ChartPoint::new(0.0, 10.0), ChartPoint::new(1e9, 10.0)];
        assert_eq!(config.candles(&long, 0.25, "1404").len(), MAX_GHOST_CANDLES);
    }
}
