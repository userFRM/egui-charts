//! Price ranges, coordinate transforms, axis ticks and label formatting.
//!
//! Use [`PriceScale`] and its per-frame [`PriceMap`] to map prices to screen
//! coordinates. [`PriceFormatter`] and [`TimeFormatter`] customize axis labels;
//! [`HorizontalScale`] supports non-time axes such as strikes and maturities.

mod countdown;
mod horizontal;
mod price_formatter;
mod pricescale;
mod pricescale_marks;
mod study_format;
mod time_formatter;
mod timescale_marks;

pub use countdown::bar_countdown;
pub use horizontal::{
    HorizontalScale, MaturityScale, PriceHorizontalScale, TickMarkLayout, key_time, time_key,
};
#[cfg(feature = "widget")]
pub(crate) use price_formatter::price_decimals;
pub use price_formatter::{
    CurrencyFormatter, CustomPriceFormatter, DefaultPriceFormatter, PercentageFormatter,
    PriceFormatter, PriceFormatterBuilder, ScientificFormatter, VolumeFormatter,
    instrument_price_precision,
};
#[cfg(feature = "widget")]
pub(crate) use pricescale::auto_scale_span;
pub use pricescale::{
    PriceMap, PriceRange, PriceScale, PriceScaleId, PriceScaleMargins, PriceScaleMode,
    PriceScaleOptions, PriceTransform,
};
pub use pricescale_marks::{PriceMark, PriceMarkGenerator, PriceMarkGeneratorConfig};
pub use study_format::StudyFormat;
pub use time_formatter::{
    CustomTimeFormatter, DefaultTimeFormatter, LocaleTimeFormatter, RelativeTimeFormatter,
    TimeFormatter, TimeFormatterBuilder, TimezoneMode,
};
pub use timescale_marks::{
    TickMark, TickMarkGenerator, TickMarkGeneratorConfig, TickMarkType, TickMarkWeight,
};
