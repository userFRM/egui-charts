//! Price and time scale engines for charting.
//!
//! This module provides the coordinate-system and formatting infrastructure
//! that maps raw data values to pixel positions and human-readable labels on
//! the chart axes.
//!
//! # Sub-modules
//!
//! | Component                  | Purpose                                                  |
//! |---------------------------|----------------------------------------------------------|
//! | [`PriceScale`]            | Price-to-Y mapping with Normal, Log, %, Indexed modes    |
//! | [`DualPriceScaleManager`] | Left/right Y-axis management with series assignment      |
//! | [`PriceMarkGenerator`]    | Smart tick placement on the price axis ("nice numbers")  |
//! | [`TickMarkGenerator`]     | Hierarchical tick placement on the time axis              |
//! | [`PriceFormatter`] trait  | Customizable price label formatting                      |
//! | [`TimeFormatter`] trait   | Customizable time label formatting                       |
//! | [`PriceDisplayMode`]      | Absolute / Percentage / Points / BPS display              |
//! | [`AutoScaleMode`]         | Auto-scale strategies for the price axis                  |

mod dual_pricescale;
mod price_display;
mod price_formatter;
mod pricescale;
mod pricescale_marks;
mod time_formatter;
mod timescale_marks;

pub use dual_pricescale::{
    DualPriceScaleManager, PosedPriceScaleOptions, PriceScaleId, SeriesScaleAssignment,
};
pub use price_display::{AutoScaleMode, PriceDisplayMode};
pub use price_formatter::{
    CurrencyFormatter, CustomPriceFormatter, DefaultPriceFormatter, PercentageFormatter,
    PriceFormatter, PriceFormatterBuilder, ScientificFormatter, VolumeFormatter,
};
pub use pricescale::{
    PriceRange, PriceScale, PriceScaleMargins, PriceScaleMode, PriceScaleOptions,
};
pub use pricescale_marks::{PriceMark, PriceMarkGenerator, PriceMarkGeneratorConfig};
pub use time_formatter::{
    CustomTimeFormatter, DefaultTimeFormatter, LocaleTimeFormatter, RelativeTimeFormatter,
    TimeFormatter, TimeFormatterBuilder,
};
pub use timescale_marks::{
    TickMark, TickMarkGenerator, TickMarkGeneratorConfig, TickMarkType, TickMarkWeight,
};
