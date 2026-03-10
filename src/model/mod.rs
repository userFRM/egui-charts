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

mod annotations;
pub mod bar;
mod chart_type;
mod chartstate;
mod color;
mod date_range;
pub mod enums;
pub mod footprint;
pub mod kagi;
pub mod line_break;
mod markers;
pub mod point_figure;
mod price_source;
pub mod quote;
pub mod range_bar;
pub mod renko;
pub mod session;
mod symbol;
pub mod symbol_info;
mod timeframe;
mod timescale;
pub mod tpo;

pub use annotations::{Annotation, MarkerType};
pub use bar::{Bar, BarData};
pub use chart_type::{ChartType, ChartTypeCategory, ChartTypeParams};
pub use chartstate::{ChartState, ZoomState};
pub use color::ColorType;
pub use date_range::DateRange;
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
pub use symbol::Symbol;
pub use symbol_info::{DataStatus, LibrarySymbolInfo, SubsessionInfo, SymbolFormat, SymbolType};
pub use timeframe::Timeframe;
pub use timescale::{LogicalRange, TimeScale};
pub use tpo::{
    ProfileShape, SessionType, TPOColorMode, TPOConfig, TPODisplayMode, TPOLetter, TPOProfile,
    to_tpo_profiles,
};
