//! Icons Module - Compile-time embedded icons following rerun patterns
//!
//! This module provides:
//! - `Icon` struct for compile-time embedded SVG icons
//! - `icon_from_path!` macro for embedding icons at compile time
//! - Pre-defined icon constants for all icons
//!
//! # Icon files
//!
//! Every icon is drawn on a 24 px grid with a 1.5 px round stroke in `currentColor`, and the
//! root element sets `color="#fff"`. The SVG loader resolves `currentColor` against that white,
//! so the tint passed to `as_image_tinted` (or a button's text colour) gives the final colour in
//! every theme. The chart installs an icon loader that draws the 1.5 px stroke one logical pixel
//! wide at any icon size, with horizontal and vertical segments on the pixel grid.
//!
//! # Usage
//!
//! ```
//! use egui::Vec2;
//! use egui_charts::ext::UiExt;
//! use egui_charts::icons::icons;
//!
//! # egui::__run_test_ui(|ui| {
//! // Use a pre-defined icon directly
//! ui.add(icons::SETTINGS.as_image(Vec2::splat(20.0)));
//!
//! // Or, with the `ui` feature, through the UiExt helpers
//! ui.medium_icon(&icons::SETTINGS);
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
    pub const CROSSHAIR: Icon = crate::icon_from_path!("lines/cursor-cross.svg");
    pub const DOT: Icon = crate::icon_from_path!("lines/cursor-dot.svg");
    pub const ARROW: Icon = crate::icon_from_path!("lines/cursor-arrow.svg");
    /// Pointer with a halo for presenting a position on the chart.
    pub const DEMONSTRATION: Icon = crate::icon_from_path!("lines/cursor-demonstration.svg");
    pub const ERASER: Icon = crate::icon_from_path!("lines/eraser.svg");

    // Lines and channels
    pub const TREND_LINE: Icon = crate::icon_from_path!("lines/trend-line.svg");
    pub const RAY: Icon = crate::icon_from_path!("lines/ray.svg");
    pub const INFO_LINE: Icon = crate::icon_from_path!("lines/info-line.svg");
    pub const EXTENDED: Icon = crate::icon_from_path!("lines/extended-line.svg");
    pub const TREND_ANGLE: Icon = crate::icon_from_path!("lines/trend-angle.svg");
    pub const HORIZONTAL_LINE: Icon = crate::icon_from_path!("lines/horizontal-line.svg");
    pub const HORIZONTAL_RAY: Icon = crate::icon_from_path!("lines/horizontal-ray.svg");
    pub const VERTICAL_LINE: Icon = crate::icon_from_path!("lines/vertical-line.svg");
    pub const CROSS_LINE: Icon = crate::icon_from_path!("lines/cross-line.svg");
    pub const PARALLEL_CHANNEL: Icon = crate::icon_from_path!("lines/parallel-channel.svg");
    pub const REGRESSION_TREND: Icon = crate::icon_from_path!("lines/regression-trend.svg");
    pub const FLAT_BOTTOM: Icon = crate::icon_from_path!("lines/flat-top-bottom.svg");
    pub const DISJOINT_ANGLE: Icon = crate::icon_from_path!("lines/disjoint-channel.svg");

    // Pitchforks and Gann
    pub const PITCHFORK: Icon = crate::icon_from_path!("pitchforks-gann/pitchfork.svg");
    pub const SCHIFF_PITCHFORK: Icon =
        crate::icon_from_path!("pitchforks-gann/schiff-pitchfork.svg");
    pub const SCHIFF_PITCHFORK2: Icon =
        crate::icon_from_path!("pitchforks-gann/modified-schiff-pitchfork.svg");
    pub const INSIDE_PITCHFORK: Icon =
        crate::icon_from_path!("pitchforks-gann/inside-pitchfork.svg");
    pub const PITCHFAN: Icon = crate::icon_from_path!("pitchforks-gann/pitchfan.svg");
    pub const GANN_SQUARE: Icon = crate::icon_from_path!("pitchforks-gann/gann-box.svg");
    pub const GANN_COMPLEX: Icon = crate::icon_from_path!("pitchforks-gann/gann-square.svg");
    pub const GANN_FIXED: Icon = crate::icon_from_path!("pitchforks-gann/gann-square-fixed.svg");
    pub const GANN_FAN: Icon = crate::icon_from_path!("pitchforks-gann/gann-fan.svg");

    // Fibonacci
    pub const FIB_RETRACEMENT: Icon = crate::icon_from_path!("fibonacci/fib-retracement.svg");
    pub const FIB_EXTENSION: Icon = crate::icon_from_path!("fibonacci/fib-extension.svg");
    pub const FIB_CHANNEL: Icon = crate::icon_from_path!("fibonacci/fib-channel.svg");
    pub const FIB_TIME_ZONE: Icon = crate::icon_from_path!("fibonacci/fib-time-zone.svg");
    pub const FIB_SPEED_RESISTANCE_FAN: Icon =
        crate::icon_from_path!("fibonacci/fib-speed-fan.svg");
    pub const TREND_BASED_FIB_TIME: Icon =
        crate::icon_from_path!("fibonacci/trend-based-fib-time.svg");
    pub const FIB_CIRCLES: Icon = crate::icon_from_path!("fibonacci/fib-circles.svg");
    pub const FIB_SPIRAL: Icon = crate::icon_from_path!("fibonacci/fib-spiral.svg");
    pub const FIB_SPEED_RESISTANCE_ARCS: Icon =
        crate::icon_from_path!("fibonacci/fib-speed-arcs.svg");
    pub const FIB_WEDGE: Icon = crate::icon_from_path!("fibonacci/fib-wedge.svg");

    // Patterns and Elliott waves
    pub const XABCD_PATTERN: Icon = crate::icon_from_path!("patterns-elliott/xabcd.svg");
    pub const CYPHER_PATTERN: Icon = crate::icon_from_path!("patterns-elliott/cypher.svg");
    pub const HEAD_AND_SHOULDERS: Icon =
        crate::icon_from_path!("patterns-elliott/head-and-shoulders.svg");
    pub const ABCD: Icon = crate::icon_from_path!("patterns-elliott/abcd.svg");
    pub const TRIANGLE_PATTERN: Icon =
        crate::icon_from_path!("patterns-elliott/triangle-pattern.svg");
    pub const THREE_DRIVERS: Icon = crate::icon_from_path!("patterns-elliott/three-drives.svg");
    pub const ELLIOTT_IMPULSE: Icon =
        crate::icon_from_path!("patterns-elliott/elliott-impulse.svg");
    pub const ELLIOTT_CORRECTION: Icon =
        crate::icon_from_path!("patterns-elliott/elliott-correction.svg");
    pub const ELLIOTT_TRIANGLE: Icon =
        crate::icon_from_path!("patterns-elliott/elliott-triangle.svg");
    pub const ELLIOTT_DOUBLE_COMBO: Icon =
        crate::icon_from_path!("patterns-elliott/elliott-double-combo.svg");
    pub const ELLIOTT_TRIPLE_COMBO: Icon =
        crate::icon_from_path!("patterns-elliott/elliott-triple-combo.svg");

    // Projection, volume, measurers and cycles
    pub const LONG_POSITION: Icon =
        crate::icon_from_path!("projection-volume-measure/long-position.svg");
    pub const SHORT_POSITION: Icon =
        crate::icon_from_path!("projection-volume-measure/short-position.svg");
    pub const PREDICTION: Icon = crate::icon_from_path!("projection-volume-measure/forecast.svg");
    pub const BARS_PATTERN: Icon =
        crate::icon_from_path!("projection-volume-measure/bars-pattern.svg");
    pub const GHOST_FEED: Icon = crate::icon_from_path!("projection-volume-measure/ghost-feed.svg");
    pub const PROJECTION: Icon = crate::icon_from_path!("projection-volume-measure/projection.svg");
    pub const ANCHORED_VWAP: Icon =
        crate::icon_from_path!("projection-volume-measure/anchored-vwap.svg");
    pub const FIXED_RANGE_VOLUME_PROFILE: Icon =
        crate::icon_from_path!("projection-volume-measure/fixed-range-volume-profile.svg");
    pub const ANCHORED_VOLUME_PROFILE: Icon =
        crate::icon_from_path!("projection-volume-measure/anchored-volume-profile.svg");
    pub const MEASURE: Icon = crate::icon_from_path!("projection-volume-measure/measure.svg");
    pub const PRICE_RANGE: Icon =
        crate::icon_from_path!("projection-volume-measure/price-range.svg");
    pub const DATE_RANGE: Icon = crate::icon_from_path!("projection-volume-measure/date-range.svg");
    pub const DATE_AND_PRICE_RANGE: Icon =
        crate::icon_from_path!("projection-volume-measure/date-price-range.svg");
    pub const CIRCLE_LINES: Icon =
        crate::icon_from_path!("projection-volume-measure/cyclic-lines.svg");
    pub const TIME_CYCLES: Icon =
        crate::icon_from_path!("projection-volume-measure/time-cycles.svg");
    pub const SINE_LINE: Icon = crate::icon_from_path!("projection-volume-measure/sine-line.svg");

    // Shapes, brushes and annotations
    pub const BRUSH: Icon = crate::icon_from_path!("shapes-annotations/brush.svg");
    pub const HIGHLIGHTER: Icon = crate::icon_from_path!("shapes-annotations/highlighter.svg");
    pub const PAINTBRUSH: Icon = crate::icon_from_path!("shapes-annotations/paintbrush.svg");
    pub const ARROW_MARKER: Icon = crate::icon_from_path!("shapes-annotations/arrow-marker.svg");
    pub const ARROW_LINE: Icon = crate::icon_from_path!("shapes-annotations/arrow.svg");
    pub const ARROW_MARK_UP: Icon = crate::icon_from_path!("shapes-annotations/arrow-mark-up.svg");
    pub const ARROW_MARK_DOWN: Icon =
        crate::icon_from_path!("shapes-annotations/arrow-mark-down.svg");
    pub const ARROW_MARK_LEFT: Icon =
        crate::icon_from_path!("shapes-annotations/arrow-mark-left.svg");
    pub const ARROW_MARK_RIGHT: Icon =
        crate::icon_from_path!("shapes-annotations/arrow-mark-right.svg");
    pub const RECT: Icon = crate::icon_from_path!("shapes-annotations/rectangle.svg");
    pub const ROTATED_RECT: Icon =
        crate::icon_from_path!("shapes-annotations/rotated-rectangle.svg");
    pub const PATH: Icon = crate::icon_from_path!("shapes-annotations/path.svg");
    pub const CIRCLE: Icon = crate::icon_from_path!("shapes-annotations/circle.svg");
    pub const ELLIPSE: Icon = crate::icon_from_path!("shapes-annotations/ellipse.svg");
    pub const POLYLINE: Icon = crate::icon_from_path!("shapes-annotations/polyline.svg");
    pub const TRIANGLE: Icon = crate::icon_from_path!("shapes-annotations/triangle.svg");
    pub const SECTOR: Icon = crate::icon_from_path!("shapes-annotations/sector.svg");
    pub const ARC: Icon = crate::icon_from_path!("shapes-annotations/arc.svg");
    pub const BEZIER_QUADRO: Icon = crate::icon_from_path!("shapes-annotations/curve.svg");
    pub const BEZIER_CUBIC: Icon = crate::icon_from_path!("shapes-annotations/double-curve.svg");
    pub const TEXT: Icon = crate::icon_from_path!("shapes-annotations/text.svg");
    pub const ANCHORED_TEXT: Icon = crate::icon_from_path!("shapes-annotations/anchored-text.svg");
    pub const NOTE: Icon = crate::icon_from_path!("shapes-annotations/note.svg");
    pub const PRICE_NOTE: Icon = crate::icon_from_path!("shapes-annotations/price-note.svg");
    pub const ANCHORED_NOTE: Icon = crate::icon_from_path!("shapes-annotations/anchored-note.svg");
    pub const CALLOUT: Icon = crate::icon_from_path!("shapes-annotations/callout.svg");
    pub const COMMENT: Icon = crate::icon_from_path!("shapes-annotations/comment.svg");
    pub const PRICE_LABEL: Icon = crate::icon_from_path!("shapes-annotations/price-label.svg");
    pub const SIGNPOST: Icon = crate::icon_from_path!("shapes-annotations/signpost.svg");
    pub const FLAG_MARK: Icon = crate::icon_from_path!("shapes-annotations/flag-mark.svg");
    pub const TABLE: Icon = crate::icon_from_path!("shapes-annotations/table.svg");
    pub const IMAGE: Icon = crate::icon_from_path!("shapes-annotations/image.svg");
    pub const POST: Icon = crate::icon_from_path!("shapes-annotations/post.svg");
    pub const IDEAS: Icon = crate::icon_from_path!("shapes-annotations/idea.svg");
    pub const STICKER: Icon = crate::icon_from_path!("shapes-annotations/sticker.svg");
    pub const EMOJI_ICON: Icon = crate::icon_from_path!("shapes-annotations/emoji.svg");

    // Chart types
    pub const CHART_BARS: Icon = crate::icon_from_path!("chart-types/chart-bars.svg");
    pub const CHART_CANDLES: Icon = crate::icon_from_path!("chart-types/chart-candles.svg");
    pub const CHART_HOLLOW_CANDLES: Icon =
        crate::icon_from_path!("chart-types/chart-hollow-candles.svg");
    pub const CHART_VOLUME_CANDLES: Icon =
        crate::icon_from_path!("chart-types/chart-volume-candles.svg");
    pub const CHART_LINE: Icon = crate::icon_from_path!("chart-types/chart-line.svg");
    pub const CHART_LINE_MARKERS: Icon =
        crate::icon_from_path!("chart-types/chart-line-markers.svg");
    pub const CHART_STEP_LINE: Icon = crate::icon_from_path!("chart-types/chart-step-line.svg");
    pub const CHART_AREA: Icon = crate::icon_from_path!("chart-types/chart-area.svg");
    pub const CHART_HLC_AREA: Icon = crate::icon_from_path!("chart-types/chart-hlc-area.svg");
    pub const CHART_BASELINE: Icon = crate::icon_from_path!("chart-types/chart-baseline.svg");
    pub const CHART_COLUMNS: Icon = crate::icon_from_path!("chart-types/chart-columns.svg");
    pub const CHART_HIGH_LOW: Icon = crate::icon_from_path!("chart-types/chart-high-low.svg");
    pub const CHART_TPO: Icon = crate::icon_from_path!("chart-types/chart-tpo.svg");
    pub const CHART_LINE_BREAK: Icon = crate::icon_from_path!("chart-types/chart-line-break.svg");
    pub const CHART_KAGI: Icon = crate::icon_from_path!("chart-types/chart-kagi.svg");
    pub const CHART_RANGE: Icon = crate::icon_from_path!("chart-types/chart-range.svg");
    pub const CHART_POINT_FIGURE: Icon =
        crate::icon_from_path!("chart-types/chart-point-figure.svg");
    pub const CHART_RENKO: Icon = crate::icon_from_path!("chart-types/chart-renko.svg");
    pub const CHART_HEIKIN_ASHI: Icon = crate::icon_from_path!("chart-types/chart-heikin-ashi.svg");

    // Interface
    pub const MENU: Icon = crate::icon_from_path!("ui/menu.svg");
    pub const SETTINGS: Icon = crate::icon_from_path!("ui/settings.svg");
    pub const CAMERA: Icon = crate::icon_from_path!("ui/camera.svg");
    pub const PLUS: Icon = crate::icon_from_path!("ui/plus.svg");
    /// Circled plus for adding a comparison series.
    pub const COMPARE_ADD: Icon = crate::icon_from_path!("ui/compare-add.svg");
    pub const INDICATORS: Icon = crate::icon_from_path!("ui/indicators.svg");
    pub const ALERTS: Icon = crate::icon_from_path!("ui/alert.svg");
    pub const REPLAY: Icon = crate::icon_from_path!("ui/replay.svg");
    pub const UNDO: Icon = crate::icon_from_path!("ui/undo.svg");
    pub const REDO: Icon = crate::icon_from_path!("ui/redo.svg");
    pub const MINUS: Icon = crate::icon_from_path!("ui/minus.svg");
    pub const CHECK: Icon = crate::icon_from_path!("ui/check.svg");
    pub const CHEVRON_DOWN: Icon = crate::icon_from_path!("ui/chevron-down.svg");
    pub const CHEVRON_UP: Icon = crate::icon_from_path!("ui/chevron-up.svg");
    pub const CHEVRON_LEFT: Icon = crate::icon_from_path!("ui/chevron-left.svg");
    pub const CHEVRON_RIGHT: Icon = crate::icon_from_path!("ui/chevron-right.svg");
    pub const FULLSCREEN: Icon = crate::icon_from_path!("ui/fullscreen.svg");
    pub const THEME_TOGGLE: Icon = crate::icon_from_path!("ui/theme-toggle.svg");
    pub const QUICK_SEARCH: Icon = crate::icon_from_path!("ui/search.svg");
    pub const WIDGET_BAR_CALENDAR: Icon = crate::icon_from_path!("ui/calendar.svg");
    pub const GO_TO_DATE: Icon = crate::icon_from_path!("ui/go-to-date.svg");
    pub const WIDGET_BAR_HELP: Icon = crate::icon_from_path!("ui/help.svg");
    pub const INFO: Icon = crate::icon_from_path!("ui/info.svg");
    pub const SUCCESS: Icon = crate::icon_from_path!("ui/success.svg");
    pub const WARNING: Icon = crate::icon_from_path!("ui/warning.svg");
    pub const STATUS_ERROR: Icon = crate::icon_from_path!("ui/error.svg");
    /// Stacked layers for the widget bar object panel.
    pub const WIDGET_BAR_LAYERS: Icon = crate::icon_from_path!("ui/widget-bar-layers.svg");
    pub const OBJECT_TREE: Icon = crate::icon_from_path!("ui/object-tree.svg");
    pub const EYE: Icon = crate::icon_from_path!("ui/eye.svg");
    pub const EYE_HIDE: Icon = crate::icon_from_path!("ui/eye-off.svg");
    pub const LOCK: Icon = crate::icon_from_path!("ui/lock.svg");
    pub const UNLOCK: Icon = crate::icon_from_path!("ui/unlock.svg");
    pub const ZOOM_IN: Icon = crate::icon_from_path!("ui/zoom-in.svg");
    pub const ZOOM_OUT: Icon = crate::icon_from_path!("ui/zoom-out.svg");
    pub const MOVE_VERTICAL: Icon = crate::icon_from_path!("ui/move-vertical.svg");
    pub const PERCENT: Icon = crate::icon_from_path!("ui/percent.svg");
    pub const ROTATE_CCW: Icon = crate::icon_from_path!("ui/rotate-ccw.svg");
    /// Pencil and lock for keeping the drawing tool active.
    pub const DRAWING_KEEP: Icon = crate::icon_from_path!("ui/drawing-keep.svg");
    /// Upright horseshoe magnet for the drawing rail.
    pub const DRAWING_MAGNET: Icon = crate::icon_from_path!("ui/drawing-magnet.svg");
    pub const MAGNET: Icon = crate::icon_from_path!("ui/magnet.svg");
    pub const TRASH: Icon = crate::icon_from_path!("ui/trash.svg");
    pub const DUPLICATE: Icon = crate::icon_from_path!("ui/duplicate.svg");
    pub const STAR_EMPTY: Icon = crate::icon_from_path!("ui/star.svg");
    pub const STAR_FILLED: Icon = crate::icon_from_path!("ui/star-filled.svg");
    pub const LAYOUT_GRID: Icon = crate::icon_from_path!("ui/layout-grid.svg");
    /// Single chart pane for choosing the chart layout.
    pub const LAYOUT_SINGLE: Icon = crate::icon_from_path!("ui/layout-single.svg");
    pub const LAYOUT_SETUP: Icon = crate::icon_from_path!("ui/layout-setup.svg");
    pub const CLOSE: Icon = crate::icon_from_path!("ui/close.svg");
    pub const MOVE_PANE: Icon = crate::icon_from_path!("ui/move-pane.svg");
    /// An arrow into a price axis, for pinning a study to a scale.
    pub const PIN_TO_SCALE: Icon = crate::icon_from_path!("ui/pin-scale.svg");
    pub const WATCHLIST: Icon = crate::icon_from_path!("ui/watchlist.svg");
    pub const UI_DRAG_HANDLE: Icon = crate::icon_from_path!("ui/drag-handle.svg");
    pub const UI_MORE_HORIZONTAL: Icon = crate::icon_from_path!("ui/more.svg");
    pub const UI_BUFFERING: Icon = crate::icon_from_path!("ui/loading.svg");

    // Settings dialog tabs
    pub const SETTINGS_SYMBOL: Icon = crate::icon_from_path!("ui/symbol.svg");
    pub const SETTINGS_STATUS_LINE: Icon = crate::icon_from_path!("ui/status-line.svg");
    pub const SETTINGS_SCALES_LINES: Icon = crate::icon_from_path!("ui/scales-lines.svg");
    pub const SETTINGS_CANVAS: Icon = crate::icon_from_path!("ui/canvas.svg");
    pub const SETTINGS_EVENTS: Icon = crate::icon_from_path!("ui/events.svg");

    // Indicator categories and object tree groups
    pub const CODE: Icon = crate::icon_from_path!("ui/code.svg");
    pub const USERS: Icon = crate::icon_from_path!("ui/users.svg");
    pub const CROWN: Icon = crate::icon_from_path!("ui/crown.svg");
    pub const DATABASE: Icon = crate::icon_from_path!("ui/database.svg");
    pub const PENCIL: Icon = crate::icon_from_path!("ui/pencil.svg");
    pub const TEMPLATE: Icon = crate::icon_from_path!("ui/template.svg");

    // Connection status
    pub const STATUS_CONNECTED: Icon = crate::icon_from_path!("ui/connected.svg");
    pub const STATUS_CONNECTING: Icon = crate::icon_from_path!("ui/connecting.svg");
    pub const STATUS_DISCONNECTED: Icon = crate::icon_from_path!("ui/disconnected.svg");

    // Media and replay
    pub const MEDIA_PLAY: Icon = crate::icon_from_path!("ui/play.svg");
    pub const MEDIA_PAUSE: Icon = crate::icon_from_path!("ui/pause.svg");
    pub const MEDIA_STOP: Icon = crate::icon_from_path!("ui/stop.svg");
    pub const MEDIA_SKIP_BACK: Icon = crate::icon_from_path!("ui/skip-back.svg");
    pub const MEDIA_SKIP_FORWARD: Icon = crate::icon_from_path!("ui/skip-forward.svg");
    pub const UI_ARROW_BACK: Icon = crate::icon_from_path!("ui/step-back.svg");
    pub const UI_ARROW_FORWARD: Icon = crate::icon_from_path!("ui/step-forward.svg");

    // Aliases
    pub const BELL: Icon = ALERTS;
    pub const WIDGET_BAR_ALERTS: Icon = ALERTS;
    pub const CANDLESTICK: Icon = CHART_CANDLES;
    pub const SETTINGS_GEAR: Icon = SETTINGS;
    pub const BAR_CHART: Icon = CHART_BARS;
    pub const REMOVE: Icon = TRASH;
    pub const LONG_POS: Icon = LONG_POSITION;
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
