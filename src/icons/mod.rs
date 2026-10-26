//! Embedded SVG icons for chart controls and host UI.
//!
//! Use [`icons`] for built-in glyphs or [`Icon`] and [`crate::icon_from_path`] to embed
//! an application SVG. The chart installs the icon image loaders when shown.
//!
//! ```
//! use egui_charts::icons::icons;
//!
//! # egui::__run_test_ui(|ui| {
//! ui.add(icons::SETTINGS.as_image(egui::Vec2::splat(20.0)));
//! # });
//! ```

mod icon;
mod loader;

pub use icon::Icon;
pub(crate) use loader::install as install_loader;

/// Macro to embed an SVG icon at compile time.
///
/// The path is relative to the `assets/icons/` directory of the crate that invokes the macro.
///
/// # Example
///
/// ```
/// use egui_charts::icon_from_path;
/// use egui_charts::icons::Icon;
///
/// const ERASER: Icon = icon_from_path!("lines/eraser.svg");
/// ```
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

    // Cursors
    /// Crosshair SVG icon.
    pub const CROSSHAIR: Icon = crate::icon_from_path!("lines/cursor-cross.svg");
    /// Dot SVG icon.
    pub const DOT: Icon = crate::icon_from_path!("lines/cursor-dot.svg");
    /// Arrow SVG icon.
    pub const ARROW: Icon = crate::icon_from_path!("lines/cursor-arrow.svg");
    /// Pointer with a halo for presenting a position on the chart.
    pub const DEMONSTRATION: Icon = crate::icon_from_path!("lines/cursor-demonstration.svg");
    /// Eraser SVG icon.
    pub const ERASER: Icon = crate::icon_from_path!("lines/eraser.svg");

    // Lines and channels
    /// Trend line SVG icon.
    pub const TREND_LINE: Icon = crate::icon_from_path!("lines/trend-line.svg");
    /// Ray SVG icon.
    pub const RAY: Icon = crate::icon_from_path!("lines/ray.svg");
    /// Info line SVG icon.
    pub const INFO_LINE: Icon = crate::icon_from_path!("lines/info-line.svg");
    /// Extended SVG icon.
    pub const EXTENDED: Icon = crate::icon_from_path!("lines/extended-line.svg");
    /// Trend angle SVG icon.
    pub const TREND_ANGLE: Icon = crate::icon_from_path!("lines/trend-angle.svg");
    /// Horizontal line SVG icon.
    pub const HORIZONTAL_LINE: Icon = crate::icon_from_path!("lines/horizontal-line.svg");
    /// Horizontal ray SVG icon.
    pub const HORIZONTAL_RAY: Icon = crate::icon_from_path!("lines/horizontal-ray.svg");
    /// Vertical line SVG icon.
    pub const VERTICAL_LINE: Icon = crate::icon_from_path!("lines/vertical-line.svg");
    /// Cross line SVG icon.
    pub const CROSS_LINE: Icon = crate::icon_from_path!("lines/cross-line.svg");
    /// Parallel channel SVG icon.
    pub const PARALLEL_CHANNEL: Icon = crate::icon_from_path!("lines/parallel-channel.svg");
    /// Regression trend SVG icon.
    pub const REGRESSION_TREND: Icon = crate::icon_from_path!("lines/regression-trend.svg");
    /// Flat top or bottom channel SVG icon.
    pub const FLAT_BOTTOM: Icon = crate::icon_from_path!("lines/flat-top-bottom.svg");
    /// Disjoint angle SVG icon.
    pub const DISJOINT_ANGLE: Icon = crate::icon_from_path!("lines/disjoint-channel.svg");

    // Pitchforks and Gann
    /// Pitchfork SVG icon.
    pub const PITCHFORK: Icon = crate::icon_from_path!("pitchforks-gann/pitchfork.svg");
    /// Schiff pitchfork SVG icon.
    pub const SCHIFF_PITCHFORK: Icon =
        crate::icon_from_path!("pitchforks-gann/schiff-pitchfork.svg");
    /// Modified Schiff pitchfork SVG icon.
    pub const SCHIFF_PITCHFORK2: Icon =
        crate::icon_from_path!("pitchforks-gann/modified-schiff-pitchfork.svg");
    /// Inside pitchfork SVG icon.
    pub const INSIDE_PITCHFORK: Icon =
        crate::icon_from_path!("pitchforks-gann/inside-pitchfork.svg");
    /// Pitchfan SVG icon.
    pub const PITCHFAN: Icon = crate::icon_from_path!("pitchforks-gann/pitchfan.svg");
    /// Gann box SVG icon.
    pub const GANN_SQUARE: Icon = crate::icon_from_path!("pitchforks-gann/gann-box.svg");
    /// Gann square SVG icon.
    pub const GANN_COMPLEX: Icon = crate::icon_from_path!("pitchforks-gann/gann-square.svg");
    /// Fixed Gann square SVG icon.
    pub const GANN_FIXED: Icon = crate::icon_from_path!("pitchforks-gann/gann-square-fixed.svg");
    /// Gann fan SVG icon.
    pub const GANN_FAN: Icon = crate::icon_from_path!("pitchforks-gann/gann-fan.svg");

    // Fibonacci
    /// Fibonacci retracement SVG icon.
    pub const FIB_RETRACEMENT: Icon = crate::icon_from_path!("fibonacci/fib-retracement.svg");
    /// Fibonacci extension SVG icon.
    pub const FIB_EXTENSION: Icon = crate::icon_from_path!("fibonacci/fib-extension.svg");
    /// Fibonacci channel SVG icon.
    pub const FIB_CHANNEL: Icon = crate::icon_from_path!("fibonacci/fib-channel.svg");
    /// Fibonacci time zone SVG icon.
    pub const FIB_TIME_ZONE: Icon = crate::icon_from_path!("fibonacci/fib-time-zone.svg");
    /// Fibonacci speed resistance fan SVG icon.
    pub const FIB_SPEED_RESISTANCE_FAN: Icon =
        crate::icon_from_path!("fibonacci/fib-speed-fan.svg");
    /// Trend based fibonacci time SVG icon.
    pub const TREND_BASED_FIB_TIME: Icon =
        crate::icon_from_path!("fibonacci/trend-based-fib-time.svg");
    /// Fibonacci circles SVG icon.
    pub const FIB_CIRCLES: Icon = crate::icon_from_path!("fibonacci/fib-circles.svg");
    /// Fibonacci spiral SVG icon.
    pub const FIB_SPIRAL: Icon = crate::icon_from_path!("fibonacci/fib-spiral.svg");
    /// Fibonacci speed resistance arcs SVG icon.
    pub const FIB_SPEED_RESISTANCE_ARCS: Icon =
        crate::icon_from_path!("fibonacci/fib-speed-arcs.svg");
    /// Fibonacci wedge SVG icon.
    pub const FIB_WEDGE: Icon = crate::icon_from_path!("fibonacci/fib-wedge.svg");

    // Patterns and Elliott waves
    /// XABCD pattern SVG icon.
    pub const XABCD_PATTERN: Icon = crate::icon_from_path!("patterns-elliott/xabcd.svg");
    /// Cypher pattern SVG icon.
    pub const CYPHER_PATTERN: Icon = crate::icon_from_path!("patterns-elliott/cypher.svg");
    /// Head and shoulders SVG icon.
    pub const HEAD_AND_SHOULDERS: Icon =
        crate::icon_from_path!("patterns-elliott/head-and-shoulders.svg");
    /// ABCD SVG icon.
    pub const ABCD: Icon = crate::icon_from_path!("patterns-elliott/abcd.svg");
    /// Triangle pattern SVG icon.
    pub const TRIANGLE_PATTERN: Icon =
        crate::icon_from_path!("patterns-elliott/triangle-pattern.svg");
    /// Three drives SVG icon.
    pub const THREE_DRIVERS: Icon = crate::icon_from_path!("patterns-elliott/three-drives.svg");
    /// Elliott impulse SVG icon.
    pub const ELLIOTT_IMPULSE: Icon =
        crate::icon_from_path!("patterns-elliott/elliott-impulse.svg");
    /// Elliott correction SVG icon.
    pub const ELLIOTT_CORRECTION: Icon =
        crate::icon_from_path!("patterns-elliott/elliott-correction.svg");
    /// Elliott triangle SVG icon.
    pub const ELLIOTT_TRIANGLE: Icon =
        crate::icon_from_path!("patterns-elliott/elliott-triangle.svg");
    /// Elliott double combo SVG icon.
    pub const ELLIOTT_DOUBLE_COMBO: Icon =
        crate::icon_from_path!("patterns-elliott/elliott-double-combo.svg");
    /// Elliott triple combo SVG icon.
    pub const ELLIOTT_TRIPLE_COMBO: Icon =
        crate::icon_from_path!("patterns-elliott/elliott-triple-combo.svg");

    // Projection, volume, measurers and cycles
    /// Long position SVG icon.
    pub const LONG_POSITION: Icon =
        crate::icon_from_path!("projection-volume-measure/long-position.svg");
    /// Short position SVG icon.
    pub const SHORT_POSITION: Icon =
        crate::icon_from_path!("projection-volume-measure/short-position.svg");
    /// Prediction SVG icon.
    pub const PREDICTION: Icon = crate::icon_from_path!("projection-volume-measure/forecast.svg");
    /// Bars pattern SVG icon.
    pub const BARS_PATTERN: Icon =
        crate::icon_from_path!("projection-volume-measure/bars-pattern.svg");
    /// Ghost feed SVG icon.
    pub const GHOST_FEED: Icon = crate::icon_from_path!("projection-volume-measure/ghost-feed.svg");
    /// Projection SVG icon.
    pub const PROJECTION: Icon = crate::icon_from_path!("projection-volume-measure/projection.svg");
    /// Anchored vwap SVG icon.
    pub const ANCHORED_VWAP: Icon =
        crate::icon_from_path!("projection-volume-measure/anchored-vwap.svg");
    /// Fixed range volume profile SVG icon.
    pub const FIXED_RANGE_VOLUME_PROFILE: Icon =
        crate::icon_from_path!("projection-volume-measure/fixed-range-volume-profile.svg");
    /// Anchored volume profile SVG icon.
    pub const ANCHORED_VOLUME_PROFILE: Icon =
        crate::icon_from_path!("projection-volume-measure/anchored-volume-profile.svg");
    /// Measure SVG icon.
    pub const MEASURE: Icon = crate::icon_from_path!("projection-volume-measure/measure.svg");
    /// Price range SVG icon.
    pub const PRICE_RANGE: Icon =
        crate::icon_from_path!("projection-volume-measure/price-range.svg");
    /// Date range SVG icon.
    pub const DATE_RANGE: Icon = crate::icon_from_path!("projection-volume-measure/date-range.svg");
    /// Date and price range SVG icon.
    pub const DATE_AND_PRICE_RANGE: Icon =
        crate::icon_from_path!("projection-volume-measure/date-price-range.svg");
    /// Circle lines SVG icon.
    pub const CIRCLE_LINES: Icon =
        crate::icon_from_path!("projection-volume-measure/cyclic-lines.svg");
    /// Time cycles SVG icon.
    pub const TIME_CYCLES: Icon =
        crate::icon_from_path!("projection-volume-measure/time-cycles.svg");
    /// Sine line SVG icon.
    pub const SINE_LINE: Icon = crate::icon_from_path!("projection-volume-measure/sine-line.svg");

    // Shapes, brushes and annotations
    /// Brush SVG icon.
    pub const BRUSH: Icon = crate::icon_from_path!("shapes-annotations/brush.svg");
    /// Highlighter SVG icon.
    pub const HIGHLIGHTER: Icon = crate::icon_from_path!("shapes-annotations/highlighter.svg");
    /// Paintbrush SVG icon.
    pub const PAINTBRUSH: Icon = crate::icon_from_path!("shapes-annotations/paintbrush.svg");
    /// Arrow marker SVG icon.
    pub const ARROW_MARKER: Icon = crate::icon_from_path!("shapes-annotations/arrow-marker.svg");
    /// Arrow line SVG icon.
    pub const ARROW_LINE: Icon = crate::icon_from_path!("shapes-annotations/arrow.svg");
    /// Arrow mark up SVG icon.
    pub const ARROW_MARK_UP: Icon = crate::icon_from_path!("shapes-annotations/arrow-mark-up.svg");
    /// Arrow mark down SVG icon.
    pub const ARROW_MARK_DOWN: Icon =
        crate::icon_from_path!("shapes-annotations/arrow-mark-down.svg");
    /// Arrow mark left SVG icon.
    pub const ARROW_MARK_LEFT: Icon =
        crate::icon_from_path!("shapes-annotations/arrow-mark-left.svg");
    /// Arrow mark right SVG icon.
    pub const ARROW_MARK_RIGHT: Icon =
        crate::icon_from_path!("shapes-annotations/arrow-mark-right.svg");
    /// Rect SVG icon.
    pub const RECT: Icon = crate::icon_from_path!("shapes-annotations/rectangle.svg");
    /// Rotated rect SVG icon.
    pub const ROTATED_RECT: Icon =
        crate::icon_from_path!("shapes-annotations/rotated-rectangle.svg");
    /// Path SVG icon.
    pub const PATH: Icon = crate::icon_from_path!("shapes-annotations/path.svg");
    /// Circle SVG icon.
    pub const CIRCLE: Icon = crate::icon_from_path!("shapes-annotations/circle.svg");
    /// Ellipse SVG icon.
    pub const ELLIPSE: Icon = crate::icon_from_path!("shapes-annotations/ellipse.svg");
    /// Polyline SVG icon.
    pub const POLYLINE: Icon = crate::icon_from_path!("shapes-annotations/polyline.svg");
    /// Triangle SVG icon.
    pub const TRIANGLE: Icon = crate::icon_from_path!("shapes-annotations/triangle.svg");
    /// Sector SVG icon.
    pub const SECTOR: Icon = crate::icon_from_path!("shapes-annotations/sector.svg");
    /// Arc SVG icon.
    pub const ARC: Icon = crate::icon_from_path!("shapes-annotations/arc.svg");
    /// Bezier quadro SVG icon.
    pub const BEZIER_QUADRO: Icon = crate::icon_from_path!("shapes-annotations/curve.svg");
    /// Bezier cubic SVG icon.
    pub const BEZIER_CUBIC: Icon = crate::icon_from_path!("shapes-annotations/double-curve.svg");
    /// Text SVG icon.
    pub const TEXT: Icon = crate::icon_from_path!("shapes-annotations/text.svg");
    /// Anchored text SVG icon.
    pub const ANCHORED_TEXT: Icon = crate::icon_from_path!("shapes-annotations/anchored-text.svg");
    /// Note SVG icon.
    pub const NOTE: Icon = crate::icon_from_path!("shapes-annotations/note.svg");
    /// Price note SVG icon.
    pub const PRICE_NOTE: Icon = crate::icon_from_path!("shapes-annotations/price-note.svg");
    /// Anchored note SVG icon.
    pub const ANCHORED_NOTE: Icon = crate::icon_from_path!("shapes-annotations/anchored-note.svg");
    /// Callout SVG icon.
    pub const CALLOUT: Icon = crate::icon_from_path!("shapes-annotations/callout.svg");
    /// Comment SVG icon.
    pub const COMMENT: Icon = crate::icon_from_path!("shapes-annotations/comment.svg");
    /// Price label SVG icon.
    pub const PRICE_LABEL: Icon = crate::icon_from_path!("shapes-annotations/price-label.svg");
    /// Signpost SVG icon.
    pub const SIGNPOST: Icon = crate::icon_from_path!("shapes-annotations/signpost.svg");
    /// Flag mark SVG icon.
    pub const FLAG_MARK: Icon = crate::icon_from_path!("shapes-annotations/flag-mark.svg");
    /// Table SVG icon.
    pub const TABLE: Icon = crate::icon_from_path!("shapes-annotations/table.svg");
    /// Image SVG icon.
    pub const IMAGE: Icon = crate::icon_from_path!("shapes-annotations/image.svg");
    /// Post SVG icon.
    pub const POST: Icon = crate::icon_from_path!("shapes-annotations/post.svg");
    /// Ideas SVG icon.
    pub const IDEAS: Icon = crate::icon_from_path!("shapes-annotations/idea.svg");
    /// Sticker SVG icon.
    pub const STICKER: Icon = crate::icon_from_path!("shapes-annotations/sticker.svg");
    /// Emoji icon SVG icon.
    pub const EMOJI_ICON: Icon = crate::icon_from_path!("shapes-annotations/emoji.svg");

    // Chart types
    /// Chart bars SVG icon.
    pub const CHART_BARS: Icon = crate::icon_from_path!("chart-types/chart-bars.svg");
    /// Chart candles SVG icon.
    pub const CHART_CANDLES: Icon = crate::icon_from_path!("chart-types/chart-candles.svg");
    /// Chart hollow candles SVG icon.
    pub const CHART_HOLLOW_CANDLES: Icon =
        crate::icon_from_path!("chart-types/chart-hollow-candles.svg");
    /// Chart volume candles SVG icon.
    pub const CHART_VOLUME_CANDLES: Icon =
        crate::icon_from_path!("chart-types/chart-volume-candles.svg");
    /// Chart line SVG icon.
    pub const CHART_LINE: Icon = crate::icon_from_path!("chart-types/chart-line.svg");
    /// Chart line markers SVG icon.
    pub const CHART_LINE_MARKERS: Icon =
        crate::icon_from_path!("chart-types/chart-line-markers.svg");
    /// Chart step line SVG icon.
    pub const CHART_STEP_LINE: Icon = crate::icon_from_path!("chart-types/chart-step-line.svg");
    /// Chart area SVG icon.
    pub const CHART_AREA: Icon = crate::icon_from_path!("chart-types/chart-area.svg");
    /// Chart hlc area SVG icon.
    pub const CHART_HLC_AREA: Icon = crate::icon_from_path!("chart-types/chart-hlc-area.svg");
    /// Chart baseline SVG icon.
    pub const CHART_BASELINE: Icon = crate::icon_from_path!("chart-types/chart-baseline.svg");
    /// Chart columns SVG icon.
    pub const CHART_COLUMNS: Icon = crate::icon_from_path!("chart-types/chart-columns.svg");
    /// Chart high low SVG icon.
    pub const CHART_HIGH_LOW: Icon = crate::icon_from_path!("chart-types/chart-high-low.svg");
    /// Chart tpo SVG icon.
    pub const CHART_TPO: Icon = crate::icon_from_path!("chart-types/chart-tpo.svg");
    /// Chart line break SVG icon.
    pub const CHART_LINE_BREAK: Icon = crate::icon_from_path!("chart-types/chart-line-break.svg");
    /// Chart kagi SVG icon.
    pub const CHART_KAGI: Icon = crate::icon_from_path!("chart-types/chart-kagi.svg");
    /// Chart range SVG icon.
    pub const CHART_RANGE: Icon = crate::icon_from_path!("chart-types/chart-range.svg");
    /// Chart point figure SVG icon.
    pub const CHART_POINT_FIGURE: Icon =
        crate::icon_from_path!("chart-types/chart-point-figure.svg");
    /// Chart renko SVG icon.
    pub const CHART_RENKO: Icon = crate::icon_from_path!("chart-types/chart-renko.svg");
    /// Chart heikin ashi SVG icon.
    pub const CHART_HEIKIN_ASHI: Icon = crate::icon_from_path!("chart-types/chart-heikin-ashi.svg");

    // Interface
    /// Menu SVG icon.
    pub const MENU: Icon = crate::icon_from_path!("ui/menu.svg");
    /// Settings SVG icon.
    pub const SETTINGS: Icon = crate::icon_from_path!("ui/settings.svg");
    /// Camera SVG icon.
    pub const CAMERA: Icon = crate::icon_from_path!("ui/camera.svg");
    /// Plus SVG icon.
    pub const PLUS: Icon = crate::icon_from_path!("ui/plus.svg");
    /// Circled plus for adding a comparison series.
    pub const COMPARE_ADD: Icon = crate::icon_from_path!("ui/compare-add.svg");
    /// Indicators SVG icon.
    pub const INDICATORS: Icon = crate::icon_from_path!("ui/indicators.svg");
    /// Alerts SVG icon.
    pub const ALERTS: Icon = crate::icon_from_path!("ui/alert.svg");
    /// Replay SVG icon.
    pub const REPLAY: Icon = crate::icon_from_path!("ui/replay.svg");
    /// Undo SVG icon.
    pub const UNDO: Icon = crate::icon_from_path!("ui/undo.svg");
    /// Redo SVG icon.
    pub const REDO: Icon = crate::icon_from_path!("ui/redo.svg");
    /// Minus SVG icon.
    pub const MINUS: Icon = crate::icon_from_path!("ui/minus.svg");
    /// Check SVG icon.
    pub const CHECK: Icon = crate::icon_from_path!("ui/check.svg");
    /// Chevron down SVG icon.
    pub const CHEVRON_DOWN: Icon = crate::icon_from_path!("ui/chevron-down.svg");
    /// Chevron up SVG icon.
    pub const CHEVRON_UP: Icon = crate::icon_from_path!("ui/chevron-up.svg");
    /// Chevron left SVG icon.
    pub const CHEVRON_LEFT: Icon = crate::icon_from_path!("ui/chevron-left.svg");
    /// Chevron right SVG icon.
    pub const CHEVRON_RIGHT: Icon = crate::icon_from_path!("ui/chevron-right.svg");
    /// Fullscreen SVG icon.
    pub const FULLSCREEN: Icon = crate::icon_from_path!("ui/fullscreen.svg");
    /// Theme toggle SVG icon.
    pub const THEME_TOGGLE: Icon = crate::icon_from_path!("ui/theme-toggle.svg");
    /// Quick search SVG icon.
    pub const QUICK_SEARCH: Icon = crate::icon_from_path!("ui/search.svg");
    /// Magnifier with a lightning bolt for Quick Search, the command search.
    pub const COMMAND_SEARCH: Icon = crate::icon_from_path!("ui/quick-search.svg");
    /// Hex nut for the top toolbar's chart settings.
    pub const SETTINGS_NUT: Icon = crate::icon_from_path!("ui/settings-nut.svg");
    /// Light bulb with a plus for publishing the chart.
    pub const PUBLISH: Icon = crate::icon_from_path!("ui/publish.svg");
    /// Widget bar calendar SVG icon.
    pub const WIDGET_BAR_CALENDAR: Icon = crate::icon_from_path!("ui/calendar.svg");
    /// Go to date SVG icon.
    pub const GO_TO_DATE: Icon = crate::icon_from_path!("ui/go-to-date.svg");
    /// Widget bar help SVG icon.
    pub const WIDGET_BAR_HELP: Icon = crate::icon_from_path!("ui/help.svg");
    /// Info SVG icon.
    pub const INFO: Icon = crate::icon_from_path!("ui/info.svg");
    /// Success SVG icon.
    pub const SUCCESS: Icon = crate::icon_from_path!("ui/success.svg");
    /// Warning SVG icon.
    pub const WARNING: Icon = crate::icon_from_path!("ui/warning.svg");
    /// Status error SVG icon.
    pub const STATUS_ERROR: Icon = crate::icon_from_path!("ui/error.svg");
    /// Stacked layers for the widget bar object panel.
    pub const WIDGET_BAR_LAYERS: Icon = crate::icon_from_path!("ui/widget-bar-layers.svg");
    /// Bulleted list for the widget bar watchlist.
    pub const WIDGET_BAR_WATCHLIST: Icon = crate::icon_from_path!("ui/widget-bar-watchlist.svg");
    /// Four-point sparkle for the widget bar highlights.
    pub const WIDGET_BAR_HIGHLIGHTS: Icon = crate::icon_from_path!("ui/highlights.svg");
    /// Funnel for the widget bar screener.
    pub const WIDGET_BAR_SCREENER: Icon = crate::icon_from_path!("ui/screener.svg");
    /// Flame for the widget bar hotlists.
    pub const WIDGET_BAR_HOTLISTS: Icon = crate::icon_from_path!("ui/hotlists.svg");
    /// Folded newspaper for the widget bar news.
    pub const WIDGET_BAR_NEWS: Icon = crate::icon_from_path!("ui/news.svg");
    /// Bell for the widget bar notifications.
    pub const WIDGET_BAR_NOTIFICATIONS: Icon = crate::icon_from_path!("ui/notifications.svg");
    /// Two speech bubbles for the widget bar chats.
    pub const WIDGET_BAR_CHATS: Icon = crate::icon_from_path!("ui/chats.svg");
    /// Grid of filled squares for the widget bar products launcher.
    pub const WIDGET_BAR_PRODUCTS: Icon = crate::icon_from_path!("ui/products.svg");
    /// Object tree SVG icon.
    pub const OBJECT_TREE: Icon = crate::icon_from_path!("ui/object-tree.svg");
    /// Eye SVG icon.
    pub const EYE: Icon = crate::icon_from_path!("ui/eye.svg");
    /// Eye hide SVG icon.
    pub const EYE_HIDE: Icon = crate::icon_from_path!("ui/eye-off.svg");
    /// Lock SVG icon.
    pub const LOCK: Icon = crate::icon_from_path!("ui/lock.svg");
    /// Unlock SVG icon.
    pub const UNLOCK: Icon = crate::icon_from_path!("ui/unlock.svg");
    /// Zoom in SVG icon.
    pub const ZOOM_IN: Icon = crate::icon_from_path!("ui/zoom-in.svg");
    /// Zoom out SVG icon.
    pub const ZOOM_OUT: Icon = crate::icon_from_path!("ui/zoom-out.svg");
    /// Move vertical SVG icon.
    pub const MOVE_VERTICAL: Icon = crate::icon_from_path!("ui/move-vertical.svg");
    /// Percent SVG icon.
    pub const PERCENT: Icon = crate::icon_from_path!("ui/percent.svg");
    /// Rotate ccw SVG icon.
    pub const ROTATE_CCW: Icon = crate::icon_from_path!("ui/rotate-ccw.svg");
    /// Pencil and lock for keeping the drawing tool active.
    pub const DRAWING_KEEP: Icon = crate::icon_from_path!("ui/drawing-keep.svg");
    /// Upright horseshoe magnet for the drawing rail.
    pub const DRAWING_MAGNET: Icon = crate::icon_from_path!("ui/drawing-magnet.svg");
    /// Magnet SVG icon.
    pub const MAGNET: Icon = crate::icon_from_path!("ui/magnet.svg");
    /// Trash SVG icon.
    pub const TRASH: Icon = crate::icon_from_path!("ui/trash.svg");
    /// Duplicate SVG icon.
    pub const DUPLICATE: Icon = crate::icon_from_path!("ui/duplicate.svg");
    /// Star empty SVG icon.
    pub const STAR_EMPTY: Icon = crate::icon_from_path!("ui/star.svg");
    /// Star filled SVG icon.
    pub const STAR_FILLED: Icon = crate::icon_from_path!("ui/star-filled.svg");
    /// Layout grid SVG icon.
    pub const LAYOUT_GRID: Icon = crate::icon_from_path!("ui/layout-grid.svg");
    /// Single chart pane for choosing the chart layout.
    pub const LAYOUT_SINGLE: Icon = crate::icon_from_path!("ui/layout-single.svg");
    // One glyph per multi-chart preset, the cells arranged as `ChartLayoutMode` tiles them.
    /// Layout two horizontal SVG icon.
    pub const LAYOUT_TWO_HORIZONTAL: Icon = crate::icon_from_path!("ui/layout-2-columns.svg");
    /// Layout two vertical SVG icon.
    pub const LAYOUT_TWO_VERTICAL: Icon = crate::icon_from_path!("ui/layout-2-rows.svg");
    /// Layout three SVG icon.
    pub const LAYOUT_THREE: Icon = crate::icon_from_path!("ui/layout-3.svg");
    /// Layout four SVG icon.
    pub const LAYOUT_FOUR: Icon = crate::icon_from_path!("ui/layout-4.svg");
    /// Layout five SVG icon.
    pub const LAYOUT_FIVE: Icon = crate::icon_from_path!("ui/layout-5.svg");
    /// Layout six SVG icon.
    pub const LAYOUT_SIX: Icon = crate::icon_from_path!("ui/layout-6.svg");
    /// Layout seven SVG icon.
    pub const LAYOUT_SEVEN: Icon = crate::icon_from_path!("ui/layout-7.svg");
    /// Layout eight SVG icon.
    pub const LAYOUT_EIGHT: Icon = crate::icon_from_path!("ui/layout-8.svg");
    /// Layout nine SVG icon.
    pub const LAYOUT_NINE: Icon = crate::icon_from_path!("ui/layout-9.svg");
    /// Layout ten SVG icon.
    pub const LAYOUT_TEN: Icon = crate::icon_from_path!("ui/layout-10.svg");
    /// Layout eleven SVG icon.
    pub const LAYOUT_ELEVEN: Icon = crate::icon_from_path!("ui/layout-11.svg");
    /// Layout twelve SVG icon.
    pub const LAYOUT_TWELVE: Icon = crate::icon_from_path!("ui/layout-12.svg");
    /// Layout thirteen SVG icon.
    pub const LAYOUT_THIRTEEN: Icon = crate::icon_from_path!("ui/layout-13.svg");
    /// Layout fourteen SVG icon.
    pub const LAYOUT_FOURTEEN: Icon = crate::icon_from_path!("ui/layout-14.svg");
    /// Layout fifteen SVG icon.
    pub const LAYOUT_FIFTEEN: Icon = crate::icon_from_path!("ui/layout-15.svg");
    /// Layout sixteen SVG icon.
    pub const LAYOUT_SIXTEEN: Icon = crate::icon_from_path!("ui/layout-16.svg");
    /// One chart with a tab strip, for the tabbed preset.
    pub const LAYOUT_TABS: Icon = crate::icon_from_path!("ui/layout-tabs.svg");
    /// Layout setup SVG icon.
    pub const LAYOUT_SETUP: Icon = crate::icon_from_path!("ui/layout-setup.svg");
    /// Close SVG icon.
    pub const CLOSE: Icon = crate::icon_from_path!("ui/close.svg");
    /// Three columns, for a table's column chooser.
    pub const COLUMNS: Icon = crate::icon_from_path!("ui/columns.svg");
    /// An empty square, for maximizing a panel.
    pub const MAXIMIZE: Icon = crate::icon_from_path!("ui/maximize.svg");
    /// A square with a plus.
    pub const SQUARE_PLUS: Icon = crate::icon_from_path!("ui/square-plus.svg");
    /// Move pane SVG icon.
    pub const MOVE_PANE: Icon = crate::icon_from_path!("ui/move-pane.svg");
    /// An arrow into a price axis, for pinning a study to a scale.
    pub const PIN_TO_SCALE: Icon = crate::icon_from_path!("ui/pin-scale.svg");
    /// Watchlist SVG icon.
    pub const WATCHLIST: Icon = crate::icon_from_path!("ui/watchlist.svg");
    /// Ui drag handle SVG icon.
    pub const UI_DRAG_HANDLE: Icon = crate::icon_from_path!("ui/drag-handle.svg");
    /// Ui more horizontal SVG icon.
    pub const UI_MORE_HORIZONTAL: Icon = crate::icon_from_path!("ui/more.svg");
    /// Ui buffering SVG icon.
    pub const UI_BUFFERING: Icon = crate::icon_from_path!("ui/loading.svg");

    // Settings dialog tabs
    /// Settings symbol SVG icon.
    pub const SETTINGS_SYMBOL: Icon = crate::icon_from_path!("ui/symbol.svg");
    /// Settings status line SVG icon.
    pub const SETTINGS_STATUS_LINE: Icon = crate::icon_from_path!("ui/status-line.svg");
    /// Settings scales lines SVG icon.
    pub const SETTINGS_SCALES_LINES: Icon = crate::icon_from_path!("ui/scales-lines.svg");
    /// Settings canvas SVG icon.
    pub const SETTINGS_CANVAS: Icon = crate::icon_from_path!("ui/canvas.svg");
    /// Settings events SVG icon.
    pub const SETTINGS_EVENTS: Icon = crate::icon_from_path!("ui/events.svg");

    // Indicator categories and object tree groups
    /// Code SVG icon.
    pub const CODE: Icon = crate::icon_from_path!("ui/code.svg");
    /// Users SVG icon.
    pub const USERS: Icon = crate::icon_from_path!("ui/users.svg");
    /// Crown SVG icon.
    pub const CROWN: Icon = crate::icon_from_path!("ui/crown.svg");
    /// Database SVG icon.
    pub const DATABASE: Icon = crate::icon_from_path!("ui/database.svg");
    /// Pencil SVG icon.
    pub const PENCIL: Icon = crate::icon_from_path!("ui/pencil.svg");
    /// Template SVG icon.
    pub const TEMPLATE: Icon = crate::icon_from_path!("ui/template.svg");

    // Connection status
    /// Status connected SVG icon.
    pub const STATUS_CONNECTED: Icon = crate::icon_from_path!("ui/connected.svg");
    /// Status connecting SVG icon.
    pub const STATUS_CONNECTING: Icon = crate::icon_from_path!("ui/connecting.svg");
    /// Status disconnected SVG icon.
    pub const STATUS_DISCONNECTED: Icon = crate::icon_from_path!("ui/disconnected.svg");

    // Media and replay
    /// Media play SVG icon.
    pub const MEDIA_PLAY: Icon = crate::icon_from_path!("ui/play.svg");
    /// Media pause SVG icon.
    pub const MEDIA_PAUSE: Icon = crate::icon_from_path!("ui/pause.svg");
    /// Media stop SVG icon.
    pub const MEDIA_STOP: Icon = crate::icon_from_path!("ui/stop.svg");
    /// Media skip back SVG icon.
    pub const MEDIA_SKIP_BACK: Icon = crate::icon_from_path!("ui/skip-back.svg");
    /// Media skip forward SVG icon.
    pub const MEDIA_SKIP_FORWARD: Icon = crate::icon_from_path!("ui/skip-forward.svg");
    /// Ui arrow back SVG icon.
    pub const UI_ARROW_BACK: Icon = crate::icon_from_path!("ui/step-back.svg");
    /// Ui arrow forward SVG icon.
    pub const UI_ARROW_FORWARD: Icon = crate::icon_from_path!("ui/step-forward.svg");

    // Aliases
    /// Bell SVG icon.
    pub const BELL: Icon = ALERTS;
    /// Widget bar alerts SVG icon.
    pub const WIDGET_BAR_ALERTS: Icon = ALERTS;
    /// Candlestick SVG icon.
    pub const CANDLESTICK: Icon = CHART_CANDLES;
    /// Settings gear SVG icon.
    pub const SETTINGS_GEAR: Icon = SETTINGS;
    /// Bar chart SVG icon.
    pub const BAR_CHART: Icon = CHART_BARS;
    /// Remove SVG icon.
    pub const REMOVE: Icon = TRASH;
    /// Long pos SVG icon.
    pub const LONG_POS: Icon = LONG_POSITION;
    /// Short pos SVG icon.
    pub const SHORT_POS: Icon = SHORT_POSITION;
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;
    use std::path::Path;

    /// Asset paths of every `icon_from_path!` constant in this file.
    fn embedded_paths() -> Vec<String> {
        let needle = concat!("icon_from_path", "!(\"");
        include_str!("mod.rs")
            .split(needle)
            .skip(1)
            .map(|rest| rest[..rest.find('"').unwrap()].to_string())
            .collect()
    }

    fn svg_files(dir: &Path, out: &mut Vec<String>, root: &Path) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                svg_files(&path, out, root);
            } else if path.extension().is_some_and(|e| e == "svg") {
                let rel = path.strip_prefix(root).unwrap();
                out.push(rel.to_string_lossy().replace('\\', "/"));
            }
        }
    }

    /// The SVG loader resolves `currentColor` to black unless the root sets a colour, and egui's
    /// tint multiplies the texture, so an icon without a white root stays black in every theme.
    #[test]
    fn every_icon_file_is_embedded_and_tintable() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("assets/icons");
        let mut files = Vec::new();
        svg_files(&root, &mut files, &root);
        let embedded: HashSet<String> = embedded_paths().into_iter().collect();
        for f in &files {
            assert!(embedded.contains(f), "{f} is not embedded by any constant");
            let svg = std::fs::read_to_string(root.join(f)).unwrap();
            let open_tag = &svg[..svg.find('>').unwrap()];
            assert!(
                open_tag.contains(r##"color="#fff""##),
                "{f}: root element must set color=\"#fff\" so tinting works"
            );
        }
    }
}
