//! Price grids for spreading bars over price levels (TPO profiles).

use super::Bar;

/// Most price levels one series is spread over.
const MAX_PRICE_LEVELS: f64 = 1_000.0;

/// Price grid one series is keyed by: `tick`, widened to a whole number of ticks
/// when the finite prices of `data` span more than [`MAX_PRICE_LEVELS`] ticks.
/// Without a usable tick (zero, negative or non-finite) the grid is the span
/// divided into [`MAX_PRICE_LEVELS`] levels, or 1.0 for a flat series. Every key
/// is a multiple of the grid, so a series touches about that many levels at most.
pub(crate) fn level_grid(data: &[Bar], tick: f64) -> f64 {
    let (low, high) = data.iter().fold((f64::MAX, f64::MIN), extend);
    range_grid(low, high, tick)
}

/// `range` extended by the finite low, high and close of `bar`.
fn extend((low, high): (f64, f64), bar: &Bar) -> (f64, f64) {
    [bar.low, bar.high, bar.close]
        .into_iter()
        .filter(|p| p.is_finite())
        .fold((low, high), |(lo, hi), p| (lo.min(p), hi.max(p)))
}

/// [`level_grid`] of bars whose finite prices run from `low` to `high`.
fn range_grid(low: f64, high: f64, tick: f64) -> f64 {
    // Dividing before subtracting keeps the span finite for prices near f64::MAX.
    let level_span = high / MAX_PRICE_LEVELS - low / MAX_PRICE_LEVELS;
    if tick > 0.0 && tick.is_finite() {
        // A whole number of ticks keeps level prices on the tick
        let grid = tick * (level_span / tick).ceil().max(1.0);
        // A subnormal tick overflows the tick count
        if grid.is_finite() {
            return grid;
        }
    }
    if level_span > 0.0 { level_span } else { 1.0 }
}

/// Grid key of a price; the f64 division saturates instead of overflowing.
fn level_key(price: f64, grid: f64) -> i64 {
    (price / grid).round() as i64
}

/// Grid keys of a bar's low and high, or `None` for a non-finite or inverted bar.
pub(crate) fn bar_level_keys(bar: &Bar, grid: f64) -> Option<(i64, i64)> {
    let (low, high) = (level_key(bar.low, grid), level_key(bar.high, grid));
    (bar.low.is_finite() && bar.high.is_finite() && low <= high).then_some((low, high))
}

#[cfg(test)]
mod tests {
    use super::Bar;

    #[test]
    fn level_grid_stays_finite_for_a_subnormal_tick() {
        let bar = Bar {
            time: chrono::DateTime::UNIX_EPOCH,
            open: 100.0,
            high: 101.0,
            low: 99.0,
            close: 100.0,
            volume: 0.0,
            colors: None,
            payload: None,
        };
        let grid = super::level_grid(&[bar], 5e-324);
        assert!(grid.is_finite() && grid > 0.0, "grid {grid}");
    }
}
