//! Core data models for financial charting
//!
//! Provides fundamental data structures for bars, candles, markers,
//! symbols, timeframes, and chart state management.
//!
//! # Module Organization
//!
//! - [`bar`] - Core OHLCV data structures
//! - `chart_type` - Chart visualization type definitions
//! - `renko`, `kagi`, `line_break`, etc. - Data transformations
//! - `markers`, `annotations` - Chart overlays
//! - `strategy` - A strategy run's trades and equity, and its performance metrics

mod alert;
mod annotations;
pub mod bar;
mod chart_type;
mod chartstate;
mod clock;
mod color;
mod date_range;
mod instrument;
pub mod kagi;
pub mod line_break;
mod markers;
pub mod point_figure;
mod price_source;
pub mod quote;
pub mod range_bar;
pub mod renko;
pub mod session;
mod signal;
mod strategy;
mod symbol;
pub mod symbol_info;
mod timeframe;
mod timescale;
pub mod tpo;
mod trading;

pub use alert::{Alert, AlertCondition, AlertFrequency, AlertId, AlertStatus};
pub use annotations::{Annotation, MarkerType};
#[cfg(feature = "widget")]
pub(crate) use bar::SpanCache;
pub use bar::{Bar, BarData};
pub use chart_type::{ChartType, ChartTypeCategory, ChartTypeParams};
pub use chartstate::{ChartState, ZoomState};
pub use clock::BarClock;
pub use color::ColorType;
pub use date_range::DateRange;
pub use instrument::{Instrument, Session, SessionWindow, TimezoneId};
pub use kagi::{KagiConfig, KagiLine, KagiThickness, to_kagi_lines};
pub use line_break::{
    LineBreakConfig, LineBreakLine, LineBreakSignal, LineDirection,
    detect_signal as detect_line_break_signal, to_line_break_lines,
};
pub use markers::{Marker, MarkerPos, MarkerShape};
pub use point_figure::{ColumnDirection, PnfColumn, PointFigureConfig, to_pnf_columns};
pub use price_source::PriceSource;
pub use quote::{
    Level1Quote, OrderBook, OrderBookLevel, OrderBookSource, QuoteSource, QuoteSourceError, Tick,
    TickSource, TradeSide,
};
pub use range_bar::{
    RangeBar, RangeBarConfig, TickData as RangeTickData, to_range_bars_from_ohlc,
    to_range_bars_from_ticks,
};
pub use renko::{RenkoBrick, RenkoConfig, RenkoDirection, to_renko_bricks};
pub use session::{
    CompositeSessionProvider, DailySessionProvider, MonthlySessionProvider, SessionBreak,
    SessionBreakType, SessionProvider, WeeklySessionProvider, find_session_breaks,
};
pub use signal::{Direction, Signal, SignalId, SignalShape};
pub use strategy::{
    StrategyExit, StrategyPerformance, StrategyReport, StrategySide, StrategyTrade,
};
pub use symbol::Symbol;
pub use symbol_info::{DataStatus, LibrarySymbolInfo, SubsessionInfo, SymbolFormat, SymbolType};
pub use timeframe::Timeframe;
pub use timescale::{LogicalRange, TimeScale};
pub use tpo::{
    ProfileShape, SessionType, TPOColorMode, TPOConfig, TPODisplayMode, TPOLetter, TPOProfile,
    derive_tick_size, to_tpo_profiles,
};
pub use trading::{
    Account, Capabilities, Execution, Order, OrderId, OrderRequest, OrderSide, OrderStatus,
    OrderType, Position,
};

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
}
