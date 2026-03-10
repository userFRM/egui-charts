//! Icons Module - Compile-time embedded icons following rerun patterns
//!
//! This module provides:
//! - `Icon` struct for compile-time embedded SVG icons
//! - `icon_from_path!` macro for embedding icons at compile time
//! - Pre-defined icon constants for all icons
//!
//! # Design Decision
//!
//! This file is intentionally large (~1500 lines) as it defines all icon constants
//! in one place. This is acceptable because:
//! - **Single responsibility**: Pure data (compile-time constants)
//! - **No logic**: No rendering code, no business logic
//! - **Well-organized**: Clear section headers by category
//! - **Stable**: Rarely modified except when adding new icons
//!
//! Splitting 280+ icon constants across multiple files would:
//! - Fragment discoverability (harder to find icons)
//! - Add unnecessary import complexity
//! - Provide no architectural benefit since there's no logic to isolate
//!
//! # Usage
//!
//! ```ignore
//! use egui_charts::ui_kit::icons::{icons, Icon};
//!
//! // Use pre-defined icons
//! ui.add(icons::SETTINGS.as_image(Vec2::splat(20.0)));
//!
//! // Or use the UiExt trait
//! use egui_charts::ui_kit::UiExt;
//! ui.medium_icon(&icons::SETTINGS);
//! ```

mod icon;

pub use icon::Icon;

/// Macro to embed an SVG icon at compile time.
///
/// The path is relative to the `assets/icons/` directory at the workspace root.
///
/// # Example
///
/// ```ignore
/// const MY_ICON: Icon = icon_from_path!("toolbar/settings.svg");
/// ```
/// Macro to embed an SVG icon at compile time (re-exported from crate root).
#[macro_export]
macro_rules! icon_from_path {
    ($path:literal) => {
        $crate::icons::Icon::new(
            $path,
            include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/icons/", $path)),
        )
    };
}

/// Pre-defined icon constants - all icons embedded at compile time.
pub mod icons {
    use super::Icon;

    // =========================================================================
    // CURSOR ICONS
    // =========================================================================

    pub const ARROW: Icon = Icon::new(
        "drawing_toolbar/cursors/arrow.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/cursors/arrow.svg"
        )),
    );

    pub const DOT: Icon = Icon::new(
        "drawing_toolbar/cursors/dot.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/cursors/dot.svg"
        )),
    );

    pub const ERASER: Icon = Icon::new(
        "drawing_toolbar/cursors/eraser.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/cursors/eraser.svg"
        )),
    );

    // =========================================================================
    // LINES & TRENDS
    // =========================================================================

    pub const TREND_LINE: Icon = Icon::new(
        "drawing_toolbar/lines/trend-line.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/lines/trend-line.svg"
        )),
    );

    pub const TREND_ANGLE: Icon = Icon::new(
        "drawing_toolbar/lines/trend-angle.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/lines/trend-angle.svg"
        )),
    );

    pub const RAY: Icon = Icon::new(
        "drawing_toolbar/lines/ray.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/lines/ray.svg"
        )),
    );

    pub const HORIZONTAL_LINE: Icon = Icon::new(
        "drawing_toolbar/lines/horz-line.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/lines/horz-line.svg"
        )),
    );

    pub const HORIZONTAL_RAY: Icon = Icon::new(
        "drawing_toolbar/lines/horz-ray.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/lines/horz-ray.svg"
        )),
    );

    pub const VERTICAL_LINE: Icon = Icon::new(
        "drawing_toolbar/lines/vert-line.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/lines/vert-line.svg"
        )),
    );

    pub const INFO_LINE: Icon = Icon::new(
        "drawing_toolbar/lines/info-line.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/lines/info-line.svg"
        )),
    );

    pub const CROSS_LINE: Icon = Icon::new(
        "drawing_toolbar/lines/cross-line.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/lines/cross-line.svg"
        )),
    );

    pub const PARALLEL_CHANNEL: Icon = Icon::new(
        "drawing_toolbar/lines/parallel-channel.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/lines/parallel-channel.svg"
        )),
    );

    pub const DISJOINT_ANGLE: Icon = Icon::new(
        "drawing_toolbar/lines/disjoint-angle.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/lines/disjoint-angle.svg"
        )),
    );

    // =========================================================================
    // PITCHFORKS
    // =========================================================================

    pub const PITCHFORK: Icon = Icon::new(
        "drawing_toolbar/pitchforks/pitchfork.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/pitchforks/pitchfork.svg"
        )),
    );

    pub const INSIDE_PITCHFORK: Icon = Icon::new(
        "drawing_toolbar/pitchforks/inside-pitchfork.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/pitchforks/inside-pitchfork.svg"
        )),
    );

    pub const SCHIFF_PITCHFORK: Icon = Icon::new(
        "drawing_toolbar/pitchforks/schiff-pitchfork.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/pitchforks/schiff-pitchfork.svg"
        )),
    );

    pub const SCHIFF_PITCHFORK2: Icon = Icon::new(
        "drawing_toolbar/pitchforks/schiff-pitchfork2.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/pitchforks/schiff-pitchfork2.svg"
        )),
    );

    pub const PITCHFAN: Icon = Icon::new(
        "drawing_toolbar/pitchforks/pitchfan.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/pitchforks/pitchfan.svg"
        )),
    );

    // =========================================================================
    // FIBONACCI
    // =========================================================================

    pub const FIB_RETRACEMENT: Icon = Icon::new(
        "drawing_toolbar/fibonacci/fib-retracement.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/fibonacci/fib-retracement.svg"
        )),
    );

    pub const FIB_EXTENSION: Icon = Icon::new(
        "drawing_toolbar/fibonacci/fib-extension.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/fibonacci/fib-extension.svg"
        )),
    );

    pub const FIB_CHANNEL: Icon = Icon::new(
        "drawing_toolbar/fibonacci/fib-channel.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/fibonacci/fib-channel.svg"
        )),
    );

    pub const FIB_CIRCLES: Icon = Icon::new(
        "drawing_toolbar/fibonacci/fib-circles.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/fibonacci/fib-circles.svg"
        )),
    );

    pub const FIB_SPIRAL: Icon = Icon::new(
        "drawing_toolbar/fibonacci/fib-spiral.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/fibonacci/fib-spiral.svg"
        )),
    );

    pub const FIB_TIME_ZONE: Icon = Icon::new(
        "drawing_toolbar/fibonacci/fib-time-zone.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/fibonacci/fib-time-zone.svg"
        )),
    );

    pub const FIB_WEDGE: Icon = Icon::new(
        "drawing_toolbar/fibonacci/fib-wedge.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/fibonacci/fib-wedge.svg"
        )),
    );

    pub const FIB_SPEED_RESISTANCE_ARCS: Icon = Icon::new(
        "drawing_toolbar/fibonacci/fib-speed-resistance-arcs.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/fibonacci/fib-speed-resistance-arcs.svg"
        )),
    );

    pub const FIB_SPEED_RESISTANCE_FAN: Icon = Icon::new(
        "drawing_toolbar/fibonacci/fib-speed-resistance-fan.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/fibonacci/fib-speed-resistance-fan.svg"
        )),
    );

    pub const TREND_BASED_FIB_EXTENSION: Icon = Icon::new(
        "drawing_toolbar/fibonacci/trend-based-fib-extension.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/fibonacci/trend-based-fib-extension.svg"
        )),
    );

    pub const TREND_BASED_FIB_TIME: Icon = Icon::new(
        "drawing_toolbar/fibonacci/trend-based-fib-time.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/fibonacci/trend-based-fib-time.svg"
        )),
    );

    // =========================================================================
    // GANN
    // =========================================================================

    pub const GANN_FAN: Icon = Icon::new(
        "drawing_toolbar/gann/gann-fan.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/gann/gann-fan.svg"
        )),
    );

    pub const GANN_SQUARE: Icon = Icon::new(
        "drawing_toolbar/gann/gann-square.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/gann/gann-square.svg"
        )),
    );

    pub const GANN_FIXED: Icon = Icon::new(
        "drawing_toolbar/gann/gann-fixed.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/gann/gann-fixed.svg"
        )),
    );

    pub const GANN_COMPLEX: Icon = Icon::new(
        "drawing_toolbar/gann/gann-complex.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/gann/gann-complex.svg"
        )),
    );

    // =========================================================================
    // ELLIOTT WAVE
    // =========================================================================

    pub const ELLIOTT_IMPULSE: Icon = Icon::new(
        "drawing_toolbar/elliott/elliott-impulse.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/elliott/elliott-impulse.svg"
        )),
    );

    pub const ELLIOTT_CORRECTION: Icon = Icon::new(
        "drawing_toolbar/elliott/elliott-correction.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/elliott/elliott-correction.svg"
        )),
    );

    pub const ELLIOTT_TRIANGLE: Icon = Icon::new(
        "drawing_toolbar/elliott/elliott-triangle.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/elliott/elliott-triangle.svg"
        )),
    );

    pub const ELLIOTT_DOUBLE_COMBO: Icon = Icon::new(
        "drawing_toolbar/elliott/elliott-double-combo.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/elliott/elliott-double-combo.svg"
        )),
    );

    pub const ELLIOTT_TRIPLE_COMBO: Icon = Icon::new(
        "drawing_toolbar/elliott/elliott-triple-combo.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/elliott/elliott-triple-combo.svg"
        )),
    );

    // =========================================================================
    // PATTERNS
    // =========================================================================

    pub const ABCD: Icon = Icon::new(
        "drawing_toolbar/patterns/abcd.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/patterns/abcd.svg"
        )),
    );

    pub const XABCD_PATTERN: Icon = Icon::new(
        "drawing_toolbar/patterns/xabcd-pattern.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/patterns/xabcd-pattern.svg"
        )),
    );

    pub const CYPHER_PATTERN: Icon = Icon::new(
        "drawing_toolbar/patterns/cypher-pattern.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/patterns/cypher-pattern.svg"
        )),
    );

    pub const HEAD_AND_SHOULDERS: Icon = Icon::new(
        "drawing_toolbar/patterns/head-and-shoulders.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/patterns/head-and-shoulders.svg"
        )),
    );

    pub const TRIANGLE_PATTERN: Icon = Icon::new(
        "drawing_toolbar/patterns/triangle-pattern.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/patterns/triangle-pattern.svg"
        )),
    );

    pub const BARS_PATTERN: Icon = Icon::new(
        "drawing_toolbar/patterns/bars-pattern.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/patterns/bars-pattern.svg"
        )),
    );

    pub const THREE_DRIVERS: Icon = Icon::new(
        "drawing_toolbar/patterns/three-drivers.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/patterns/three-drivers.svg"
        )),
    );

    // =========================================================================
    // PROJECTIONS & ANALYSIS
    // =========================================================================

    pub const LONG_POSITION: Icon = Icon::new(
        "drawing_toolbar/projections/long-position.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/projections/long-position.svg"
        )),
    );

    pub const SHORT_POSITION: Icon = Icon::new(
        "drawing_toolbar/projections/short-position.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/projections/short-position.svg"
        )),
    );

    pub const RISK_REWARD_SHORT: Icon = Icon::new(
        "drawing_toolbar/projections/risk-reward-short.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/projections/risk-reward-short.svg"
        )),
    );

    pub const PREDICTION: Icon = Icon::new(
        "drawing_toolbar/projections/prediction.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/projections/prediction.svg"
        )),
    );

    pub const PROJECTION: Icon = Icon::new(
        "drawing_toolbar/projections/projection.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/projections/projection.svg"
        )),
    );

    pub const REGRESSION_TREND: Icon = Icon::new(
        "drawing_toolbar/projections/regression-trend.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/projections/regression-trend.svg"
        )),
    );

    // =========================================================================
    // VOLUME TOOLS
    // =========================================================================

    pub const ANCHORED_VWAP: Icon = Icon::new(
        "drawing_toolbar/volume/anchored-vwap.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/volume/anchored-vwap.svg"
        )),
    );

    pub const ANCHORED_VOLUME_PROFILE: Icon = Icon::new(
        "drawing_toolbar/volume/anchored-volume-profile.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/volume/anchored-volume-profile.svg"
        )),
    );

    pub const FIXED_RANGE_VOLUME_PROFILE: Icon = Icon::new(
        "drawing_toolbar/volume/fixed-range-volume-profile.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/volume/fixed-range-volume-profile.svg"
        )),
    );

    // =========================================================================
    // SHAPES
    // =========================================================================

    pub const BRUSH: Icon = Icon::new(
        "drawing_toolbar/shapes/brush.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/shapes/brush.svg"
        )),
    );

    pub const HIGHLIGHTER: Icon = Icon::new(
        "drawing_toolbar/shapes/highlighter.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/shapes/highlighter.svg"
        )),
    );

    pub const PAINTBRUSH: Icon = Icon::new(
        "drawing_toolbar/shapes/paintbrush.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/shapes/paintbrush.svg"
        )),
    );

    pub const RECT: Icon = Icon::new(
        "drawing_toolbar/shapes/rectangle.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/shapes/rectangle.svg"
        )),
    );

    pub const ROTATED_RECT: Icon = Icon::new(
        "drawing_toolbar/shapes/rotated-rectangle.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/shapes/rotated-rectangle.svg"
        )),
    );

    pub const CIRCLE: Icon = Icon::new(
        "drawing_toolbar/shapes/circle.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/shapes/circle.svg"
        )),
    );

    pub const CIRCLE_LINES: Icon = Icon::new(
        "drawing_toolbar/shapes/circle-lines.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/shapes/circle-lines.svg"
        )),
    );

    pub const ELLIPSE: Icon = Icon::new(
        "drawing_toolbar/shapes/ellipse.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/shapes/ellipse.svg"
        )),
    );

    pub const TRIANGLE: Icon = Icon::new(
        "drawing_toolbar/shapes/triangle.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/shapes/triangle.svg"
        )),
    );

    pub const ARC: Icon = Icon::new(
        "drawing_toolbar/shapes/arc.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/shapes/arc.svg"
        )),
    );

    pub const POLYLINE: Icon = Icon::new(
        "drawing_toolbar/shapes/polyline.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/shapes/polyline.svg"
        )),
    );

    pub const PATH: Icon = Icon::new(
        "drawing_toolbar/shapes/path.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/shapes/path.svg"
        )),
    );

    pub const BEZIER_CUBIC: Icon = Icon::new(
        "drawing_toolbar/shapes/bezier-cubic.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/shapes/bezier-cubic.svg"
        )),
    );

    pub const BEZIER_QUADRO: Icon = Icon::new(
        "drawing_toolbar/shapes/bezier-quadro.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/shapes/bezier-quadro.svg"
        )),
    );

    // =========================================================================
    // ANNOTATIONS & MARKERS
    // =========================================================================

    pub const ARROW_MARKER: Icon = Icon::new(
        "drawing_toolbar/annotations/arrow-marker.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/annotations/arrow-marker.svg"
        )),
    );

    pub const ARROW_MARK_UP: Icon = Icon::new(
        "drawing_toolbar/annotations/arrow-mark-up.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/annotations/arrow-mark-up.svg"
        )),
    );

    pub const ARROW_MARK_DOWN: Icon = Icon::new(
        "drawing_toolbar/annotations/arrow-mark-down.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/annotations/arrow-mark-down.svg"
        )),
    );

    pub const TEXT: Icon = Icon::new(
        "drawing_toolbar/annotations/text.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/annotations/text.svg"
        )),
    );

    pub const ANCHORED_TEXT: Icon = Icon::new(
        "drawing_toolbar/annotations/anchored-text.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/annotations/anchored-text.svg"
        )),
    );

    // =========================================================================
    // MEASURERS & RANGES
    // =========================================================================

    pub const DATE_AND_PRICE_RANGE: Icon = Icon::new(
        "drawing_toolbar/measurers/date-and-price-range.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/measurers/date-and-price-range.svg"
        )),
    );

    pub const DATE_RANGE: Icon = Icon::new(
        "drawing_toolbar/measurers/date-range.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/measurers/date-range.svg"
        )),
    );

    pub const TIME_CYCLES: Icon = Icon::new(
        "drawing_toolbar/measurers/time-cycles.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/measurers/time-cycles.svg"
        )),
    );

    pub const SINE_LINE: Icon = Icon::new(
        "drawing_toolbar/measurers/sine-line.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/measurers/sine-line.svg"
        )),
    );

    pub const EXTENDED: Icon = Icon::new(
        "drawing_toolbar/measurers/extended.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/measurers/extended.svg"
        )),
    );

    pub const FLAT_BOTTOM: Icon = Icon::new(
        "drawing_toolbar/measurers/flat-bottom.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/measurers/flat-bottom.svg"
        )),
    );

    pub const GHOST_FEED: Icon = Icon::new(
        "drawing_toolbar/measurers/ghost-feed.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/measurers/ghost-feed.svg"
        )),
    );

    pub const MEASURE: Icon = Icon::new(
        "drawing_toolbar/measurers/measure.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/measurers/measure.svg"
        )),
    );

    pub const ZOOM_IN: Icon = Icon::new(
        "drawing_toolbar/measurers/zoom.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/measurers/zoom.svg"
        )),
    );

    // =========================================================================
    // TOP TOOLBAR
    // =========================================================================

    pub const MENU: Icon = Icon::new(
        "top_toolbar/menu.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/top_toolbar/menu.svg"
        )),
    );

    pub const SETTINGS: Icon = Icon::new(
        "top_toolbar/settings.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/top_toolbar/settings.svg"
        )),
    );

    pub const CAMERA: Icon = Icon::new(
        "top_toolbar/take-a-snapshot.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/top_toolbar/take-a-snapshot.svg"
        )),
    );

    pub const PLUS: Icon = Icon::new(
        "top_toolbar/compare-or-add-symbol.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/top_toolbar/compare-or-add-symbol.svg"
        )),
    );

    pub const INDICATORS: Icon = Icon::new(
        "top_toolbar/indicators-metrics-and-strategies.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/top_toolbar/indicators-metrics-and-strategies.svg"
        )),
    );

    pub const ALERTS: Icon = Icon::new(
        "top_toolbar/alerts.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/settings_dialog/alerts.svg"
        )),
    );

    pub const REPLAY: Icon = Icon::new(
        "top_toolbar/bar-replay.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/top_toolbar/bar-replay.svg"
        )),
    );

    pub const UNDO: Icon = Icon::new(
        "top_toolbar/undo.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/top_toolbar/undo.svg"
        )),
    );

    pub const REDO: Icon = Icon::new(
        "top_toolbar/redo.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/top_toolbar/redo.svg"
        )),
    );

    pub const CHEVRON_DOWN: Icon = Icon::new(
        "top_toolbar/chevron-down.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/top_toolbar/chevron-down.svg"
        )),
    );

    pub const FULLSCREEN: Icon = Icon::new(
        "top_toolbar/header-toolbar-fullscreen.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/top_toolbar/header-toolbar-fullscreen.svg"
        )),
    );

    pub const QUICK_SEARCH: Icon = Icon::new(
        "top_toolbar/header-toolbar-quick-search.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/top_toolbar/header-toolbar-quick-search.svg"
        )),
    );

    // =========================================================================
    // TIMEFRAME TOOLBAR
    // =========================================================================

    pub const GO_TO_DATE: Icon = Icon::new(
        "timeframe_toolbar/go-to-date.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/timeframe_toolbar/go-to-date.svg"
        )),
    );

    // =========================================================================
    // WIDGET BAR
    // =========================================================================

    pub const WIDGET_BAR_ALERTS: Icon = Icon::new(
        "widget_bar/alerts.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/widget_bar/alerts.svg"
        )),
    );

    pub const WIDGET_BAR_CALENDAR: Icon = Icon::new(
        "widget_bar/calendar.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/widget_bar/calendar.svg"
        )),
    );

    pub const WIDGET_BAR_NOTIFICATIONS: Icon = Icon::new(
        "widget_bar/notifications.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/widget_bar/notifications.svg"
        )),
    );

    pub const WIDGET_BAR_HELP: Icon = Icon::new(
        "widget_bar/help.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/widget_bar/help.svg"
        )),
    );

    pub const IDEAS: Icon = Icon::new(
        "widget_bar/ideas.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/widget_bar/ideas.svg"
        )),
    );

    pub const OBJECT_TREE: Icon = Icon::new(
        "widget_bar/object-tree.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/widget_bar/object-tree.svg"
        )),
    );

    // =========================================================================
    // COMMON ICONS
    // =========================================================================

    /// Eye with slash - Hide all drawings
    pub const EYE_HIDE: Icon = Icon::new(
        "common/eye-hide.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/common/eye-hide.svg"
        )),
    );

    pub const UNLOCK: Icon = Icon::new(
        "common/unlock.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/common/unlock.svg"
        )),
    );

    pub const ZOOM_OUT: Icon = Icon::new(
        "common/zoom-out.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/common/zoom-out.svg"
        )),
    );

    pub const MAGNET: Icon = Icon::new(
        "common/magnet.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/common/magnet.svg"
        )),
    );

    pub const LOCK: Icon = Icon::new(
        "common/lock.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/common/lock.svg"
        )),
    );

    pub const TRASH: Icon = Icon::new(
        "common/trash.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/common/trash.svg"
        )),
    );

    // =========================================================================
    // CHART TYPES
    // =========================================================================

    pub const CHART_BARS: Icon = Icon::new(
        "chart_types/chart-bars.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/chart_types/chart-bars.svg"
        )),
    );

    pub const CHART_CANDLES: Icon = Icon::new(
        "chart_types/chart-candles.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/chart_types/chart-candles.svg"
        )),
    );

    pub const CHART_HOLLOW_CANDLES: Icon = Icon::new(
        "chart_types/chart-hollow-candles.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/chart_types/chart-hollow-candles.svg"
        )),
    );

    pub const CHART_VOLUME_CANDLES: Icon = Icon::new(
        "chart_types/chart-volume-candles.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/chart_types/chart-volume-candles.svg"
        )),
    );

    pub const CHART_LINE: Icon = Icon::new(
        "chart_types/chart-line.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/chart_types/chart-line.svg"
        )),
    );

    pub const CHART_LINE_MARKERS: Icon = Icon::new(
        "chart_types/chart-line-markers.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/chart_types/chart-line-markers.svg"
        )),
    );

    pub const CHART_STEP_LINE: Icon = Icon::new(
        "chart_types/chart-step-line.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/chart_types/chart-step-line.svg"
        )),
    );

    pub const CHART_AREA: Icon = Icon::new(
        "chart_types/chart-area.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/chart_types/chart-area.svg"
        )),
    );

    pub const CHART_HLC_AREA: Icon = Icon::new(
        "chart_types/chart-hlc-area.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/chart_types/chart-hlc-area.svg"
        )),
    );

    pub const CHART_BASELINE: Icon = Icon::new(
        "chart_types/chart-baseline.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/chart_types/chart-baseline.svg"
        )),
    );

    pub const CHART_HIGH_LOW: Icon = Icon::new(
        "chart_types/chart-high-low.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/chart_types/chart-high-low.svg"
        )),
    );

    pub const CHART_VOLUME_FOOTPRINT: Icon = Icon::new(
        "chart_types/chart-volume-footprint.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/chart_types/chart-volume-footprint.svg"
        )),
    );

    pub const CHART_TPO: Icon = Icon::new(
        "chart_types/chart-tpo.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/chart_types/chart-tpo.svg"
        )),
    );

    pub const CHART_SESSION_VOLUME: Icon = Icon::new(
        "chart_types/chart-session-volume.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/chart_types/chart-session-volume.svg"
        )),
    );

    pub const CHART_LINE_BREAK: Icon = Icon::new(
        "chart_types/chart-line-break.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/chart_types/chart-line-break.svg"
        )),
    );

    pub const CHART_KAGI: Icon = Icon::new(
        "chart_types/chart-kagi.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/chart_types/chart-kagi.svg"
        )),
    );

    pub const CHART_RANGE: Icon = Icon::new(
        "chart_types/chart-range.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/chart_types/chart-range.svg"
        )),
    );

    pub const CHART_POINT_FIGURE: Icon = Icon::new(
        "chart_types/chart-point-figure.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/chart_types/chart-point-figure.svg"
        )),
    );

    pub const CHART_RENKO: Icon = Icon::new(
        "chart_types/chart-renko.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/chart_types/chart-renko.svg"
        )),
    );

    pub const CHART_HEIKIN_ASHI: Icon = Icon::new(
        "chart_types/chart-heikin-ashi.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/chart_types/chart-heikin-ashi.svg"
        )),
    );

    // =========================================================================
    // SETTINGS DIALOG
    // =========================================================================

    pub const SETTINGS_SYMBOL: Icon = Icon::new(
        "settings_dialog/symbol.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/settings_dialog/symbol.svg"
        )),
    );

    pub const SETTINGS_STATUS_LINE: Icon = Icon::new(
        "settings_dialog/status-line.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/settings_dialog/status-line.svg"
        )),
    );

    pub const SETTINGS_SCALES_LINES: Icon = Icon::new(
        "settings_dialog/scales-lines.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/settings_dialog/scales-lines.svg"
        )),
    );

    pub const SETTINGS_CANVAS: Icon = Icon::new(
        "settings_dialog/canvas.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/settings_dialog/canvas.svg"
        )),
    );

    pub const SETTINGS_EVENTS: Icon = Icon::new(
        "settings_dialog/events.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/settings_dialog/events.svg"
        )),
    );

    // =========================================================================
    // EMOJIS
    // =========================================================================

    // Faces

    // Hands

    // Trading/Finance

    // Symbols

    // Animals
    pub const EMOJI_EAGLE: Icon = Icon::new(
        "emojis/eagle.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/emojis/eagle.svg"
        )),
    );

    // Holiday emojis
    pub const EMOJI_ORNAMENT: Icon = Icon::new(
        "emojis/ornament.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/emojis/ornament.svg"
        )),
    );

    // =========================================================================
    // STATUS ICONS (Connection indicators)
    // =========================================================================

    pub const STATUS_CONNECTED: Icon = Icon::new(
        "status/connected.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/status/connected.svg"
        )),
    );

    pub const STATUS_CONNECTING: Icon = Icon::new(
        "status/connecting.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/status/connecting.svg"
        )),
    );

    pub const STATUS_DISCONNECTED: Icon = Icon::new(
        "status/disconnected.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/status/disconnected.svg"
        )),
    );

    pub const STATUS_ERROR: Icon = Icon::new(
        "status/error.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/status/error.svg"
        )),
    );

    // =========================================================================
    // MEDIA ICONS (Play/Pause/Stop controls)
    // =========================================================================

    pub const MEDIA_PLAY: Icon = Icon::new(
        "media/play.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/media/play.svg"
        )),
    );

    pub const MEDIA_PAUSE: Icon = Icon::new(
        "media/pause.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/media/pause.svg"
        )),
    );

    pub const MEDIA_STOP: Icon = Icon::new(
        "media/stop.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/media/stop.svg"
        )),
    );

    pub const MEDIA_SKIP_BACK: Icon = Icon::new(
        "media/skip-back.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/media/skip-back.svg"
        )),
    );

    pub const MEDIA_SKIP_FORWARD: Icon = Icon::new(
        "media/skip-forward.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/media/skip-forward.svg"
        )),
    );

    // =========================================================================
    // UI ICONS (General interface elements)
    // =========================================================================

    pub const UI_DRAG_HANDLE: Icon = Icon::new(
        "ui/drag-handle.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/ui/drag-handle.svg"
        )),
    );

    pub const UI_MORE_HORIZONTAL: Icon = Icon::new(
        "ui/more-horizontal.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/ui/more-horizontal.svg"
        )),
    );

    pub const UI_ARROW_BACK: Icon = Icon::new(
        "ui/arrow-back.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/ui/arrow-back.svg"
        )),
    );

    pub const UI_BUFFERING: Icon = Icon::new(
        "ui/buffering.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/ui/buffering.svg"
        )),
    );

    // =========================================================================
    // CATEGORY ICONS (Emoji picker categories)
    // =========================================================================

    // =========================================================================
    // CONTEXT MENU ICONS
    // =========================================================================

    // =========================================================================
    // ADDITIONAL ICONS
    // =========================================================================

    /// Layout grid icon (4 boxes)
    pub const LAYOUT_GRID: Icon = Icon::new(
        "top_toolbar/layout-grid.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/top_toolbar/layout-grid.svg"
        )),
    );

    /// Layout setup icon (21x19)
    pub const LAYOUT_SETUP: Icon = Icon::new(
        "top_toolbar/layout-setup.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/top_toolbar/layout-setup.svg"
        )),
    );

    /// Star empty icon for favorites
    pub const STAR_EMPTY: Icon = Icon::new(
        "common/star-empty.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/common/star-empty.svg"
        )),
    );

    /// Zoom toggle icon (combined zoom in/out)
    pub const ZOOM_TOGGLE: Icon = Icon::new(
        "common/zoom-toggle.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/common/zoom-toggle.svg"
        )),
    );

    /// Crosshair cursor icon
    pub const CROSSHAIR: Icon = Icon::new(
        "drawing_toolbar/cursors/crosshair.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/cursors/crosshair.svg"
        )),
    );

    /// Emoji/icon picker icon
    pub const EMOJI_ICON: Icon = Icon::new(
        "drawing_toolbar/annotations/emoji.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/drawing_toolbar/annotations/emoji.svg"
        )),
    );

    // =========================================================================
    // WIDGET BAR 44x44 ICONS - native size
    // =========================================================================

    /// Watchlist icon - 44x44
    pub const WATCHLIST_44: Icon = Icon::new(
        "widget_bar/watchlist-44.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/widget_bar/watchlist-44.svg"
        )),
    );

    /// Screener icon - 44x44
    pub const SCREENER_44: Icon = Icon::new(
        "widget_bar/screener-44.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/widget_bar/screener-44.svg"
        )),
    );

    /// Pine Script icon - 44x44
    pub const PINE_SCRIPT_44: Icon = Icon::new(
        "widget_bar/pine-script-44.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/widget_bar/pine-script-44.svg"
        )),
    );

    /// Community icon - 44x44
    pub const COMMUNITY_44: Icon = Icon::new(
        "widget_bar/community-44.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/widget_bar/community-44.svg"
        )),
    );

    /// Products icon - 44x44
    pub const PRODUCTS_44: Icon = Icon::new(
        "widget_bar/products-44.svg",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/assets/icons/widget_bar/products-44.svg"
        )),
    );

    // =========================================================================
    // ALIASES (for backward compatibility with SvgIcon names)
    // =========================================================================

    /// Alias: Bell -> ALERTS
    pub const BELL: Icon = ALERTS;
    /// Alias: Close -> ERASER
    pub const CLOSE: Icon = ERASER;
    /// Alias: Candlestick -> CHART_CANDLES
    pub const CANDLESTICK: Icon = CHART_CANDLES;
    /// Alias: SettingsGear -> SETTINGS
    pub const SETTINGS_GEAR: Icon = SETTINGS;
    /// Alias: BarChart -> CHART_BARS
    pub const BAR_CHART: Icon = CHART_BARS;
    /// Alias: LineChart -> CHART_LINE
    pub const LINE_CHART: Icon = CHART_LINE;
    /// Alias: AreaChart -> CHART_AREA
    pub const AREA_CHART: Icon = CHART_AREA;
    /// Alias: Hide -> EYE_HIDE
    pub const HIDE: Icon = EYE_HIDE;
    /// Alias: Remove -> TRASH
    pub const REMOVE: Icon = TRASH;
    /// Alias: MovePane -> ZOOM_OUT
    pub const MOVE_PANE: Icon = ZOOM_OUT;
    /// Alias: Watchlist -> WIDGET_BAR_ALERTS
    pub const WATCHLIST: Icon = WIDGET_BAR_ALERTS;
    /// Alias: OrderBook -> WIDGET_BAR_ALERTS
    pub const ORDER_BOOK: Icon = WIDGET_BAR_ALERTS;
    /// Alias: TimeAndSales -> WIDGET_BAR_ALERTS
    pub const TIME_AND_SALES: Icon = WIDGET_BAR_ALERTS;
    /// Alias: Info -> WIDGET_BAR_HELP
    pub const INFO: Icon = WIDGET_BAR_HELP;
    /// Alias: LongPos -> LONG_POSITION
    pub const LONG_POS: Icon = LONG_POSITION;
    /// Alias: ShortPos -> SHORT_POSITION
    pub const SHORT_POS: Icon = SHORT_POSITION;

    // =========================================================================
    // SEMANTIC ALIASES (for Alert component, etc.)
    // =========================================================================
}
