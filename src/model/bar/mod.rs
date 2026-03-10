//! Bar (OHLCV) Data Module
//!
//! Core data structures for financial bar/candlestick data.
//!
//! # Module Structure
//!
//! - `bar` - Core `Bar` struct representing a single OHLCV data point
//! - `bar_data` - `BarData` container for multiple bars
//! - `patterns` - Candlestick pattern detection
//! - `io` - CSV and JSON I/O operations
//!
//! # Example
//!
//! ```
//! use chrono::Utc;
//! use egui_charts::model::{Bar, BarData};
//! use egui_charts::model::bar::CandlePatterns;
//!
//! // Create a single bar
//! let bar = Bar::new(Utc::now(), 100.0, 105.0, 98.0, 103.0, 1000.0);
//! assert!(bar.is_bullish());
//!
//! // Check patterns
//! if bar.is_hammer() {
//!     println!("Hammer pattern detected!");
//! }
//!
//! // Create a collection
//! let mut data = BarData::new();
//! data.push(bar);
//! ```

mod bar;
mod bar_data;
mod io;
mod patterns;

// Re-export main types
pub use bar::Bar;
pub use bar_data::{BarData, MAX_BARS, MAX_VISIBLE_BARS};
pub use patterns::CandlePatterns;
