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
