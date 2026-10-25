//! Trading and volume tool rendering implementations
//!
//! Split into submodules for maintainability:
//! - `position` - Long/short position tool rendering
//! - `forecast` - Forecast tool rendering
//! - `ghost_feed` - Ghost feed rendering
//! - `projection` - Projection tool rendering
//! - `bars_pattern` - Bars pattern rendering
//! - `volume_profile` - VWAP and volume profile rendering

mod bars_pattern;
mod forecast;
mod ghost_feed;
mod position;
mod projection;
mod volume_profile;
