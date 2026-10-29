//! Data types for bars, instruments, chart styles, trading and host analytics.
//!
//! Start with [`Bar`] and [`BarData`] for price history, [`Instrument`] for symbol
//! metadata and [`Timeframe`] for bar intervals. The transformation modules build
//! price-based charts and TPO profiles without rendering; strategy records describe
//! host-computed runs and provide performance calculations.

/// The most price movement, in sizes, that a Renko or range bar build walks over. A smaller size
/// on a longer history is built from a later bar; see `renko::work_start`.
pub(crate) const MAX_WORK: f64 = 2_000_000.0;

/// A price-based chart built one bar at a time, so a cached build can be extended by the newest
/// bar instead of rebuilt. The first `push` seeds the state. `elements` returns the newest
/// elements up to the type's limit, including the one still forming.
pub(crate) trait PriceBuilder {
    type Element: Clone;
    fn push(&mut self, bar: &Bar);
    fn elements(&self) -> Vec<Self::Element>;
}

mod alert;
mod annotations;
pub mod bar;
mod box_size;
mod chart_style;
mod chart_type;
mod clock;
mod color;
mod date_range;
mod instrument;
pub mod kagi;
pub mod line_break;
mod markers;
#[cfg(any(feature = "studies", feature = "drawings"))]
pub(crate) mod math;
pub mod point_figure;
mod price_source;
pub mod quote;
pub mod range_bar;
pub mod renko;
mod replay;
mod revision;
mod series_scale;
mod series_style;
pub mod session;
mod signal;
mod strategy;
mod time_scale_event;
mod timeframe;
mod timescale;
mod timezone;
pub mod tpo;
mod trading;
mod watchlist;

pub use alert::{
    Alert, AlertComparison, AlertCondition, AlertFrequency, AlertId, AlertRule, AlertSource,
    AlertStatus, NotificationSchedule, NotificationWindow,
};
pub use annotations::{Annotation, MarkerType};
#[cfg(feature = "widget")]
pub(crate) use bar::SpanCache;
pub use bar::{Bar, BarColors, BarData};
#[cfg(feature = "widget")]
pub(crate) use bar::{index_at, index_at_with_timeframe, time_at, time_at_with_timeframe};
pub(crate) use bar::{last_with_value, percent_change};
pub use box_size::{BoxSizeMethod, PriceBasedSource};
pub use chart_style::{
    AreaStyle, BarsStyle, BaselineStyle, CandlesStyle, ChartTypeStyles, HlcAreaStyle, KagiStyle,
    LastPriceAnimation, LinePlotStyle, LineType,
};
pub use chart_type::{BaselineLevel, ChartType, ChartTypeCategory, ChartTypeParams};
pub use clock::BarClock;
pub use color::ColorType;
pub use date_range::DateRange;
pub use instrument::{
    DataStatus, Instrument, OverrideWindow, Session, SessionOverride, SessionWindow, SymbolType,
};
pub use kagi::{KagiConfig, KagiLine, KagiThickness, to_kagi_lines};
pub use line_break::{
    LineBreakConfig, LineBreakLine, LineBreakSignal, LineDirection,
    detect_signal as detect_line_break_signal, to_line_break_lines,
};
pub use markers::{
    ChangeMarkers, LineStyle, Marker, MarkerOptions, MarkerPos, MarkerShape, MarkerZOrder,
    PriceLine,
};
pub use point_figure::{ColumnDirection, PnfColumn, PointFigureConfig, to_pnf_columns};
pub use price_source::PriceSource;
pub use quote::{
    ExtendedHoursQuote, ExtendedSession, Level1Quote, OrderBook, OrderBookLevel, Quote, Tick,
    TradeConditions,
};
pub use range_bar::{
    RangeBar, RangeBarConfig, RangeBarSource, to_range_bars_from_ohlc, to_range_bars_from_ticks,
};
pub use renko::{RenkoBrick, RenkoConfig, RenkoDirection, to_renko_bricks};
pub use replay::ReplaySession;
pub(crate) use revision::next_revision;
pub use series_scale::SeriesScale;
pub use series_style::{AutoscaleProvider, PriceLineSource, SeriesAlerts, SeriesStyle};
pub use session::{
    CompositeSessionProvider, DailySessionProvider, ExchangeSessionProvider,
    MonthlySessionProvider, SessionBreak, SessionBreakType, SessionProvider, WeeklySessionProvider,
    find_session_breaks,
};
pub use signal::{Direction, Signal, SignalId, SignalShape};
pub use strategy::{
    StrategyComparison, StrategyExcursion, StrategyExit, StrategyPerformance, StrategyReport,
    StrategyTrade,
};
pub use time_scale_event::{EventKind, EventVisibility, TimeScaleEvent};
pub use timeframe::Timeframe;
pub use timescale::{LogicalRange, TimeScale};
pub use timezone::{TimezoneId, TimezoneOffset};
pub(crate) use timezone::{local_instant, local_time};
pub use tpo::{
    ProfileShape, SessionType, TPOColorMode, TPOConfig, TPODisplayMode, TPOLetter, TPOProfile,
    derive_tick_size, to_tpo_profiles,
};
pub use trading::{
    Account, BalanceChange, Capabilities, Execution, JournalEntry, Order, OrderId, OrderPreview,
    OrderRequest, OrderSide, OrderStatus, OrderType, Position, PositionBracket, TimeInForce,
    TradingRules,
};

pub use watchlist::{Watchlist, WatchlistFlag, WatchlistItem};

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Duration, TimeZone, Utc};

    /// One-minute bars from 2024-01-01 whose open, high, low and close are each price.
    fn flat_bars(prices: impl Iterator<Item = f64>) -> Vec<Bar> {
        let start = Utc.with_ymd_and_hms(2024, 1, 1, 0, 0, 0).unwrap();
        prices
            .enumerate()
            .map(|(i, p)| Bar {
                time: start + Duration::minutes(i as i64),
                open: p,
                high: p,
                low: p,
                close: p,
                volume: 0.0,
                colors: None,
                payload: None,
            })
            .collect()
    }

    #[test]
    fn price_based_charts_keep_their_newest_elements_over_the_limit() {
        let alternating = |n: usize| flat_bars((0..n).map(|i| [100.0, 110.0][i % 2]));
        // Each series produces more elements than the limit, twice as many for Renko and range bars.
        let (short, long) = (alternating(201), alternating(20_001));
        let rising = flat_bars((0..20_001).map(|i| 100.0 + i as f64));
        type Run = dyn Fn(&[Bar]) -> (usize, Option<DateTime<Utc>>);
        let cases: [(&str, &[Bar], usize, &Run); 5] = [
            ("renko", &short, 10_000, &|b| {
                let v = to_renko_bricks(b, &RenkoConfig::new(0.05));
                (v.len(), v.last().map(|e| e.ts))
            }),
            ("range bars", &short, 5_000, &|b| {
                let v = to_range_bars_from_ohlc(b, &RangeBarConfig::new(0.05));
                (v.len(), v.last().map(|e| e.end_time))
            }),
            ("kagi", &long, 10_000, &|b| {
                let v = to_kagi_lines(b, &KagiConfig::new(0.02));
                (v.len(), v.last().map(|e| e.ts))
            }),
            ("point and figure", &long, 5_000, &|b| {
                let v = to_pnf_columns(b, &PointFigureConfig::new(0.02, 3));
                (v.len(), v.last().map(|e| e.end_time))
            }),
            ("line break", &rising, 10_000, &|b| {
                let v = to_line_break_lines(b, &LineBreakConfig::new(3));
                (v.len(), v.last().map(|e| e.ts))
            }),
        ];
        let failures: Vec<String> = cases
            .iter()
            .filter_map(|&(name, bars, limit, run)| {
                let (len, last) = run(bars);
                let want = bars.last().map(|b| b.time);
                (len != limit || last != want).then(|| {
                    format!("{name}: {len} elements ending {last:?}, want {limit} ending {want:?}")
                })
            })
            .collect();
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn price_based_charts_use_the_configured_size() {
        // Prices and sizes are binary fractions, so scaling by 2^-20 is exact.
        let u = 1.0 / 256.0;
        let z = [
            100.0,
            110.0,
            110.0 - 5.0 * u,
            110.0,
            110.0 - 13.0 * u,
            110.0,
            100.0,
        ];
        type Run = dyn Fn(&[Bar], f64) -> Vec<(f64, f64)>;
        type Pair = (f64, f64);
        // (name, size, build as (open, close) pairs, expected count, first, last)
        let cases: [(&str, f64, &Run, usize, Pair, Pair); 4] = [
            (
                "renko",
                1.0 / 32.0,
                &|b, s| {
                    to_renko_bricks(b, &RenkoConfig::new(s))
                        .iter()
                        .map(|e| (e.open, e.close))
                        .collect()
                },
                639,
                (100.0, 100.03125),
                (100.03125, 100.0),
            ),
            (
                "kagi",
                1.0 / 64.0,
                &|b, s| {
                    to_kagi_lines(b, &KagiConfig::new(s))
                        .iter()
                        .map(|e| (e.start_price, e.end_price))
                        .collect()
                },
                6,
                (100.0, 110.0),
                (110.0, 100.0),
            ),
            (
                "point and figure",
                1.0 / 64.0,
                &|b, s| {
                    to_pnf_columns(b, &PointFigureConfig::new(s, 3))
                        .iter()
                        .map(|e| (e.start_price, e.end_price))
                        .collect()
                },
                4,
                (100.0, 110.0),
                (109.984375, 100.0),
            ),
            (
                "range bars",
                1.0 / 32.0,
                &|b, s| {
                    to_range_bars_from_ohlc(b, &RangeBarConfig::new(s))
                        .iter()
                        .map(|e| (e.open, e.close))
                        .collect()
                },
                643,
                (100.0, 100.03125),
                (100.0, 100.0),
            ),
        ];
        let mut failures = Vec::new();
        for (name, size, run, count, first, last) in cases {
            for scale in [1.0, 2f64.powi(-20)] {
                let got = run(&flat_bars(z.iter().map(|p| p * scale)), size * scale);
                let want = (
                    count,
                    (first.0 * scale, first.1 * scale),
                    (last.0 * scale, last.1 * scale),
                );
                if (got.len(), got[0], got[got.len() - 1]) != want {
                    failures.push(format!(
                        "{name} at scale {scale}: {} elements, want {want:?}",
                        got.len()
                    ));
                }
            }
            let none = run(&flat_bars(z.iter().copied()), 0.0);
            if !none.is_empty() {
                failures.push(format!("{name}: {} elements for size 0", none.len()));
            }
        }
        // A step of 1.1e13 sizes is not built: the build starts at the bar after it.
        let u40 = 2f64.powi(-40);
        let bars = flat_bars([100.0, 110.0, 110.0 + 4.0 * u40].into_iter());
        let v = to_renko_bricks(&bars, &RenkoConfig::new(u40));
        if v.len() != 4
            || v.last().map(|e| (e.open, e.close)) != Some((110.0 + 3.0 * u40, 110.0 + 4.0 * u40))
        {
            failures.push(format!("renko work bound: {} bricks", v.len()));
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }
}
