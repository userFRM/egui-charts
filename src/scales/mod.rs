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
//! | [`PriceScale`]            | Price axis range: autoscale, zoom, Normal/Log/%/Indexed  |
//! | [`PriceMap`]              | Per-frame price-to-Y mapping produced by a [`PriceScale`] |
//! | [`PriceMarkGenerator`]    | Smart tick placement on the price axis ("nice numbers")  |
//! | [`TickMarkGenerator`]     | Hierarchical tick placement on the time axis              |
//! | [`PriceFormatter`] trait  | Customizable price label formatting                      |
//! | [`TimeFormatter`] trait   | Customizable time label formatting                       |

mod price_formatter;
mod pricescale;
mod pricescale_marks;
mod time_formatter;
mod timescale_marks;

pub use price_formatter::{
    CurrencyFormatter, CustomPriceFormatter, DefaultPriceFormatter, PercentageFormatter,
    PriceFormatter, PriceFormatterBuilder, ScientificFormatter, VolumeFormatter,
};
pub(crate) use pricescale::auto_scale_span;
pub use pricescale::{
    PriceMap, PriceRange, PriceScale, PriceScaleId, PriceScaleMargins, PriceScaleMode,
    PriceScaleOptions, PriceTransform,
};
pub use pricescale_marks::{PriceMark, PriceMarkGenerator, PriceMarkGeneratorConfig};
pub use time_formatter::{
    CustomTimeFormatter, DefaultTimeFormatter, LocaleTimeFormatter, RelativeTimeFormatter,
    TimeFormatter, TimeFormatterBuilder, TimezoneMode,
};
pub use timescale_marks::{
    TickMark, TickMarkGenerator, TickMarkGeneratorConfig, TickMarkType, TickMarkWeight,
};
