//! Shared generated OHLCV history for the standalone examples.

use chrono::{DateTime, TimeDelta, Utc};
use egui_charts::model::Bar;
use rand::{RngExt, SeedableRng, rngs::StdRng};

/// Generate a repeatable random walk with positive prices, valid wicks and volume.
/// Bars are oldest first, spaced by `interval`, with the last opening one interval before `end`.
pub fn sample_bars(
    count: usize,
    base_price: f64,
    interval: TimeDelta,
    end: DateTime<Utc>,
) -> Vec<Bar> {
    let mut rng = StdRng::seed_from_u64(42);
    let mut price = base_price;
    (0..count)
        .map(|i| {
            let open = price;
            price *= 1.0 + rng.random_range(-0.01_f64..0.01);
            let high = open.max(price) * (1.0 + rng.random_range(0.001..0.008));
            let low = open.min(price) * (1.0 - rng.random_range(0.001..0.008));
            Bar::new(
                end - interval * (count - i) as i32,
                open,
                high,
                low,
                price,
                rng.random_range(500.0..5_000.0),
            )
        })
        .collect()
}
