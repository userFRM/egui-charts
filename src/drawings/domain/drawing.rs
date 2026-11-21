//! Drawing struct - represents a single drawing on the chart
//!
//! This is the core data structure for chart drawings. It contains:
//! - Dual coordinate system (screen coords + chart coords)
//! - Styling properties (color, stroke, line style)
//! - Tool-specific configuration (Fibonacci levels, etc.)
//!
//! Note: Rendering methods are in the rendering module, not here.
//! This keeps the domain layer pure and testable.

use super::{
    AnnotationMarker, ArrowStyle, BarsPatternConfig, BarsPatternMode, ChartPoint, DrawingToolType,
    ElliottDegree, FibonacciConfig, FontWeight, ForecastStyle, GANN_SQUARE_UNITS, GannConfig,
    GhostFeedConfig, ImageConfig, LineMeasurements, MiddleLineStyle, NoteTextStyle,
    PitchforkConfig, PositionReadout, PositionSize, PositionSizing, PositionStyle,
    ProjectionConfig, RangeSettings, RegressionTrendConfig, TimeframeVisibility, VwapConfig,
};

use crate::data::VolumeProfileConfig;
use crate::model::{AlertSource, LineStyle, PostLink};
use crate::scales::PriceScaleMode;
use serde::{Deserialize, Serialize};

/// How a top-level object-tree folder shares its drawings with the other charts of a layout and
/// with other layouts.
///
/// Leaving it unset follows [`ChartSyncOptions::sync_drawings`](crate::widget::ChartSyncOptions)
/// within a layout. With drawing storage
/// ([`Chart::set_storage`](crate::widget::Chart::set_storage)), the stored drawings of a symbol
/// show on every chart of that storage, in every layout. Drawings in a folder set to
/// [`Chart`](Self::Chart) or [`Layout`](Self::Layout) stay out of storage, on their chart or on
/// the charts of their layout, and come back with a saved layout that holds them. Moving a
/// drawing into such a folder takes it out of storage and off the other charts; moving it out
/// stores it again. A drawing that a stored document already holds in such a folder, as earlier
/// versions stored them, shows as stored until a chart changes it; that chart then keeps it.
///
/// A saved layout opened on a chart with drawing storage shows each drawing as stored, and one
/// deleted from storage stays deleted, except that a drawing the layout holds in a folder set to
/// [`Chart`](Self::Chart) or [`Layout`](Self::Layout) comes back as the layout holds it, unless
/// storage holds it outside such a folder.
///
/// ```
/// use egui_charts::drawings::{ChartPoint, Drawing, DrawingManager, DrawingToolType, FolderSync};
///
/// let mut drawings = DrawingManager::new();
/// let mut level = Drawing::new(0, DrawingToolType::TrendLine);
/// level.chart_points = vec![ChartPoint::new(2.0, 104.0), ChartPoint::new(4.0, 104.0)];
/// let id = drawings.add_drawing(level).unwrap();
/// let folder = ["Levels".to_owned()];
/// assert!(drawings.move_to_folder(&[id], &[], &folder));
/// assert!(drawings.set_folder_sync(&folder, Some(FolderSync::AllLayouts)));
/// assert_eq!(drawings.drawings[0].folder_sync_mode(), Some(FolderSync::AllLayouts));
/// ```
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum FolderSync {
    /// Keep the drawings on this chart, even with layout-wide drawing sync on or with drawing
    /// storage shared with other charts.
    Chart,
    /// Mirror the drawings across the layout, even with layout-wide drawing sync off, and keep
    /// them off the charts of other layouts, also on the same drawing storage.
    Layout,
    /// Mirror the drawings across the layout as [`Layout`](Self::Layout) does, and show them as
    /// stored in every layout of their symbol, also in layouts saved before they were drawn.
    AllLayouts,
}

/// A drawing on the chart
///
/// Drawings use a dual coordinate system:
/// - `points: Vec<Pos2>` - Screen coordinates, updated each frame
/// - `chart_points: Vec<ChartPoint>` - Chart coordinates (bar_idx, price), stable across pan/zoom
///
/// # Example
///
/// ```
/// use egui_charts::drawings::{Drawing, DrawingToolType};
///
/// let mut drawing = Drawing::new(1, DrawingToolType::TrendLine);
/// drawing.color = [33, 150, 243, 255]; // Blue
/// drawing.stroke_width = 2.0;
/// ```
///
/// Serde covers the tool and its settings only: the id, the points and the
/// completion state are left out, since the anchors are stored as
/// timestamps by [`persistence`](crate::drawings::persistence). A field added
/// later needs `#[serde(default)]`, so that saved version 2 files still load.
///
/// ```
/// use egui_charts::drawings::{
///     ArrowStyle, Drawing, DrawingToolType, FibonacciLevel, GhostFeedConfig,
/// };
///
/// let mut arrow = Drawing::new(1, DrawingToolType::TrendLine);
/// arrow.arrow_end_style = ArrowStyle::Arrow;
/// let mut feed = GhostFeedConfig::drawing_default();
/// feed.noise_ticks = 2.0;
/// feed.show_wick = false;
/// let mut ghost = Drawing::new(2, DrawingToolType::GhostFeed);
/// ghost.ghost_feed = Some(feed);
/// let mut fib = Drawing::new(3, DrawingToolType::FibonacciRetracement);
/// if let Some(config) = &mut fib.fib_config {
///     config.levels.push(FibonacciLevel::new(1.272, "1.272", [41, 98, 255, 255]));
/// }
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
// The trait impls below wrap the derived ones.
#[serde(remote = "Self")]
pub struct Drawing {
    /// Unique identifier for this drawing
    #[serde(skip)]
    pub id: usize,
    /// Id of the stored record this drawing was restored from; `None` for a drawing never
    /// stored, which is saved under its `id`
    #[serde(skip)]
    pub stored_id: Option<String>,
    /// Creation time, in Unix milliseconds, of the stored record this drawing was restored
    /// from; `None` for a drawing never stored, which is saved with the time of the save
    #[serde(skip)]
    pub created_at: Option<i64>,
    /// Time of the last edit, in Unix milliseconds: read from the stored record, set by the edits
    /// of a [`DrawingManager`](crate::drawings::DrawingManager) and put back by their undo.
    /// `None` for a drawing no edit reached, which is saved with the time of the save.
    #[serde(skip)]
    pub modified_at: Option<i64>,
    /// User notes, saved with the drawing and restored from its stored record
    #[serde(skip)]
    pub notes: Option<String>,
    /// Optional object-tree name, saved with the drawing.
    #[serde(default)]
    pub label: Option<String>,
    /// Owning study id, or `None` for the main price pane. Follows that study when panes move
    /// or merge; hosts restoring studies must retain or remap their ids. Older files use main.
    #[serde(default)]
    pub pane: Option<usize>,
    /// Whether pointer and programmatic selection can select this drawing.
    #[serde(default = "enabled")]
    pub selectable: bool,
    /// Whether chart saves and persistence notifications include this drawing.
    #[serde(default = "enabled")]
    pub saveable: bool,
    /// Whether host object lists should include this drawing. Rendering is independent.
    #[serde(default = "enabled")]
    pub listed: bool,
    /// Whether the user can edit this drawing. Off, the chart, its drawing menu, the floating
    /// toolbar, the properties dialog and the object tree offer no drag, nudge, style, text,
    /// anchor or settings edit of it, no deletion, no lock or unlock and no move to another
    /// pane; it can still be selected, hidden, renamed, filed, layered and copied. Host calls
    /// that name the drawing, such as
    /// [`DrawingManager::edit_drawing`](crate::drawings::DrawingManager::edit_drawing) and
    /// [`Chart::set_drawing_points`](crate::Chart::set_drawing_points), still change it, and
    /// undo and redo keep its current value. Independent of [`Self::locked`], the user's own
    /// lock. Copies the user makes have it on. Older saved drawings have it on.
    #[serde(default = "enabled")]
    pub user_edit_enabled: bool,
    /// Host-defined group identity, retained through saving and undo.
    #[serde(default)]
    pub group: Option<String>,
    /// Named object-tree folder path, from outermost to innermost; empty means the root.
    /// Independent of host-defined groups. Older saved drawings start at the root.
    #[serde(default)]
    pub folder: Vec<String>,
    /// Synchronization set on each folder of [`Self::folder`], by position; a missing entry or
    /// `None` is unset. The outermost folder decides, as [`Self::folder_sync_mode`] reads; inner
    /// entries wait for their folder to reach the root. Use
    /// [`DrawingManager::move_to_folder`](crate::drawings::DrawingManager::move_to_folder) to keep
    /// this aligned with the path.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub folder_sync: Vec<Option<FolderSync>>,
    /// Stable id of a host-registered tool; `None` uses the built-in renderer.
    #[serde(default)]
    pub custom_tool: Option<String>,
    /// Host-defined settings passed to the registered renderer and saved without interpretation.
    #[serde(default)]
    pub custom_settings: serde_json::Value,
    /// Type of drawing tool
    pub tool_type: DrawingToolType,
    /// Screen coordinates (updated each frame from chart_points)
    #[serde(skip)]
    pub points: Vec<egui::Pos2>,
    /// Persistent chart coordinates (bar_idx, price) - stable across pan/zoom
    #[serde(skip)]
    pub chart_points: Vec<ChartPoint>,
    /// Screen y of each visible level of a Fibonacci retracement or extension, in config order,
    /// or of the far corner of a fixed Gann square (empty for other tools); rebuilt with `points`
    /// by [`Self::update_screen_coords`]
    #[serde(skip)]
    pub level_ys: Vec<f32>,
    /// Position as fractions of the pane width and height for anchored text, which keeps its
    /// place in the pane while the chart scrolls and zooms. The chart keeps
    /// [`chart_points`](Self::chart_points) on the bar and price under it.
    /// `None` takes the first position the text is drawn at inside its pane, including for older
    /// saved drawings; until then the text stays on its bar and price.
    /// A pin saved with one takes the bar and price under it the first time a chart draws it,
    /// and the chart then clears it.
    #[serde(default)]
    pub screen_anchor: Option<[f32; 2]>,
    /// Drawing colour as RGBA; also the text and border colour of annotations that have none of
    /// their own.
    pub color: [u8; 4],
    /// Stroke width in pixels
    pub stroke_width: f32,
    /// Whether the drawing is complete
    #[serde(skip)]
    pub completed: bool,
    /// Optional annotation content, preserved with the drawing. Tables use one row per line
    /// and tab-separated cells. Images use an image URI resolved by the host's egui image
    /// loaders. Posts and ideas use the first line as author or title and the rest as body,
    /// when they have no [`post`](Self::post).
    pub text: Option<String>,
    /// The post or idea a post or idea drawing shows: its link and the content the host
    /// resolved it to through [`Posts`](crate::services::Posts). The content is saved with the
    /// drawing, so it shows without asking the host again. While it is set, the card shows it
    /// instead of [`text`](Self::text). Other tools ignore it.
    #[serde(default)]
    pub post: Option<PostLink>,

    // === Styling properties ===
    /// Line style (Solid, Dashed, Dotted)
    pub line_style: LineStyle,
    /// Fill colour of shapes and the background colour of annotation boxes. `None` leaves shapes
    /// unfilled and gives an annotation box its tool's default colour.
    pub fill_color: Option<[u8; 4]>,
    /// Whether the drawing is locked (prevents editing/moving)
    pub locked: bool,
    /// Whether the drawing is visible
    pub visible: bool,
    /// Font size for text annotations
    pub font_size: f32,
    /// Font weight for text annotations
    pub font_weight: FontWeight,
    /// Whether annotation text is slanted. Older drawings are upright.
    #[serde(default)]
    pub italic: bool,
    /// Text colour of annotations; `None` uses the tool's default, the drawing colour except on
    /// a solid label, which takes white or black, whichever reads on its fill.
    #[serde(default)]
    pub text_color: Option<[u8; 4]>,
    /// Whether an annotation draws a box behind its text, in `fill_color` or else the tool's
    /// default: the theme's panel colour, or the drawing colour for a solid label. Older
    /// drawings have it on.
    #[serde(default = "enabled")]
    pub background: bool,
    /// Whether an annotation's box has a 1 px border. Older drawings have it on, except price
    /// labels saved before they had a border.
    #[serde(default = "enabled")]
    pub border: bool,
    /// Border colour of annotation boxes; `None` uses the drawing colour.
    #[serde(default)]
    pub border_color: Option<[u8; 4]>,
    /// Whether a Text, Anchored text or Callout breaks its text into lines no wider than
    /// `text_wrap_width`. Older drawings keep each line of their text whole.
    #[serde(default)]
    pub text_wrap: bool,
    /// Widest line of wrapped text, in points.
    #[serde(default = "default_text_wrap_width")]
    pub text_wrap_width: f32,
    /// Whether a Text or Anchored text grows and shrinks with the bar spacing instead of keeping
    /// its font size at every zoom: the text, its wrap width and its box scale together. Older
    /// drawings keep their size.
    #[serde(default)]
    pub text_follows_zoom: bool,
    /// Bar spacing, in points per bar or per spacing unit of a numeric horizontal scale, at which
    /// text that follows the zoom has its set font size. `None` adopts the spacing a Text or
    /// Anchored text is first displayed at by the chart or
    /// [`DrawingManager::update_all_screen_coords`](crate::drawings::DrawingManager::update_all_screen_coords),
    /// including for older saved drawings.
    #[serde(default)]
    pub text_zoom_spacing: Option<f32>,
    /// Colour, size and emphasis of the text typed on a price note. `None` draws it as the price
    /// beside it, in the label's text settings, as price notes saved before this setting do. Other
    /// tools ignore it.
    #[serde(default)]
    pub note_text_style: Option<NoteTextStyle>,
    /// Extend line to left edge; a trend line, ray, extended line, info line or trend angle
    /// extends past its first anchor, whichever way it was drawn, and a curve or double curve
    /// continues along its tangent past its first anchor
    pub extend_left: bool,
    /// Extend line to right edge; a trend line, ray, extended line, info line or trend angle
    /// extends past its second anchor, whichever way it was drawn, and a curve or double curve
    /// continues along its tangent past its last anchor
    pub extend_right: bool,
    /// Arrow style at start of line
    pub arrow_start_style: ArrowStyle,
    /// Arrow style at end of line
    pub arrow_end_style: ArrowStyle,
    /// Per-timeframe visibility
    pub timeframe_visibility: TimeframeVisibility,
    /// Whether a horizontal line, horizontal ray, cross line or position shows its prices as
    /// tags on the price axis. Other drawings ignore it. Saved drawings without it show tags.
    #[serde(default = "enabled")]
    pub price_axis_label: bool,
    /// Whether a vertical line or cross line shows its time as a tag on the time axis. Other
    /// tools ignore it. Drawings saved without it show the tag.
    #[serde(default = "enabled")]
    pub time_axis_label: bool,

    // === Tool-specific configuration (None uses the defaults) ===
    /// Readouts, midpoint, price labels and text placement of a trend line, ray, extended line,
    /// info line, trend angle or arrow, the text placement of a horizontal line or ray, a
    /// vertical line, a cross line, a channel or a regression trend, the prices of a flat
    /// top/bottom or disjoint channel, and the reach of a vertical line. Other tools ignore
    /// these settings.
    #[serde(default = "LineMeasurements::left_out")]
    pub line_measurements: LineMeasurements,
    /// Readouts, card, box border and vertical reach of a date range or date and price range.
    /// Other tools ignore these settings. Older drawings keep every readout in the theme's
    /// colours at the drawing's saved font size.
    #[serde(default = "RangeSettings::left_out")]
    pub range_settings: RangeSettings,
    /// Quantity of the long and short position tools. New positions risk 25% of an account of
    /// 1,000; `None` sizes one unit, as positions saved without a size do.
    pub position_size: Option<PositionSize>,
    /// Instrument economics for long and short position sizing. Older drawings use unit sizing.
    #[serde(default)]
    pub position_sizing: PositionSizing,
    /// Statistics, account currency and quantity precision of position tools.
    #[serde(default = "PositionReadout::legacy")]
    pub position_readout: PositionReadout,
    /// Target zone and level colour of position tools, whose alpha is the zone's opacity while
    /// labels and tags stay opaque. New positions start green at 20% opacity; `None` draws
    /// the greyed green of positions saved without a colour.
    #[serde(default)]
    pub position_target_color: Option<[u8; 4]>,
    /// Stop zone and level colour of position tools, whose alpha is the zone's opacity while
    /// labels and tags stay opaque. New positions start red at 20% opacity; `None` draws
    /// the greyed red of positions saved without a colour.
    #[serde(default)]
    pub position_stop_color: Option<[u8; 4]>,
    /// Lines, labels, zone fill and border of position tools.
    #[serde(default)]
    pub position_style: PositionStyle,
    /// Text popup of a pin, glyph disc of a signpost, and the Show text switch of horizontal
    /// lines and rays, arrow marks and the parallel, flat top/bottom and disjoint channels.
    /// Drawings saved without it show their text and no glyph, as new ones do.
    #[serde(default)]
    pub marker: AnnotationMarker,
    /// Customizable Fibonacci levels for Fib tools
    pub fib_config: Option<FibonacciConfig>,
    /// Rows and value area of the volume profile tools
    pub volume_profile: Option<VolumeProfileConfig>,
    /// Source, bounds, line styles and correlation label of the regression trend tool. New
    /// trends take [`RegressionTrendConfig::drawing_default`]; `None` uses the defaults, as
    /// trends saved without settings do.
    pub regression_trend: Option<RegressionTrendConfig>,
    /// Bands of the anchored VWAP. Older drawings use the defaults.
    #[serde(default)]
    pub vwap: Option<VwapConfig>,

    /// Levels, fan lines, arcs, background and labels of the Gann tools. New drawings store
    /// [`GannConfig::drawing_default`]; `None` draws [`GannConfig::for_tool`], preserving saved
    /// boxes' left and bottom labels and squares' 10 point labels.
    #[serde(default)]
    pub gann: Option<GannConfig>,
    /// Average range and noise, in ticks, and colours of the candles a Ghost Feed projects. New
    /// feeds start from [`GhostFeedConfig::drawing_default`]; `None` uses the defaults, the
    /// range, noise and colours of feeds saved before these settings.
    #[serde(default)]
    pub ghost_feed: Option<GhostFeedConfig>,
    /// Whether a rectangle or parallel channel draws its halfway line; a channel's takes
    /// [`Self::middle_line`].
    #[serde(default)]
    pub channel_midline: bool,
    /// Colour, size and emphasis of the prices a flat top/bottom or disjoint channel writes
    /// beside the ends of its lines while `line_measurements.price_labels` is on. `None` writes
    /// them in the line colour at 12 points, regular and upright, as drawings saved before this
    /// setting do. Other tools ignore it.
    #[serde(default)]
    pub anchor_price_style: Option<NoteTextStyle>,
    /// Colour, width and pattern of a parallel channel's middle line, drawn while
    /// `channel_midline` is on. Other tools ignore it; a rectangle keeps its dashed hairline.
    #[serde(default)]
    pub middle_line: MiddleLineStyle,
    /// Whether the tool's default line, level and text colours are drawn in the nearest shade
    /// that keeps 4:1 contrast on the active theme's chart and panel. New drawings start with
    /// it on, except projections, which keep their specified greys until shading is enabled.
    /// Drawings and saved defaults stored without it load with it off and draw in their stored
    /// colours.
    #[serde(default)]
    pub theme_shade: bool,
    /// Frame size and opacity of the image tool; `None` uses the defaults.
    #[serde(default)]
    pub image: Option<ImageConfig>,
    /// Clockwise rotation of an icon, emoji or sticker about its centre, in degrees; 0 is upright.
    /// Its handles, selection frame and hit area rotate with it. Other tools ignore this field.
    /// Drawings saved before this setting load upright.
    #[serde(default)]
    pub glyph_angle: f32,
    /// Whether a polyline joins its last anchor back to its first. Clicking the first anchor
    /// while placing a polyline closes it; older saved drawings are open.
    #[serde(default)]
    pub closed: bool,
    /// Level lines and band fill of the pitchfork tools.
    #[serde(default)]
    pub pitchfork: Option<PitchforkConfig>,
    /// Notation of the wave labels of the Elliott wave tools. Older drawings use the default.
    #[serde(default)]
    pub elliott_degree: ElliottDegree,
    /// Whether the Elliott wave tools draw the legs between their wave points. Off leaves the
    /// wave labels where they are, and the drawing is selected by its labels only. New drawings
    /// and drawings saved before this setting show their legs; other tools ignore it.
    #[serde(default = "enabled")]
    pub elliott_show_wave: bool,

    /// Draw mode, mirroring and flipping of a bars pattern's copy. New patterns draw high-low
    /// bars; `None` uses the defaults, OHLC bars, as drawings saved before this setting do.
    #[serde(default)]
    pub bars_pattern: Option<BarsPatternConfig>,
    /// Sector levels, gradient, coefficient labels and outline style of the projection
    /// tool. New projections start from [`ProjectionConfig::default`]; `None`, as on projections
    /// saved before these settings, draws one dashed level at the sector's arc in the drawing
    /// colour with a flat fill and no coefficients. Other tools ignore it.
    #[serde(default)]
    pub projection: Option<ProjectionConfig>,
    /// Card and badge colours and the background transparency of the forecast tool. New
    /// forecasts start from [`ForecastStyle::default`]; `None`, as on forecasts saved before
    /// these settings, draws the theme's colours without borders, with the fill colour, when
    /// set, as the background of every card and badge. Other tools ignore it.
    #[serde(default)]
    pub forecast: Option<ForecastStyle>,

    // === Z-Order (rendering layer) ===
    /// Z-order among the drawings of the same `series_layer` (higher = drawn on top).
    pub z_order: i32,
    /// How many series and studies of the drawing's pane are drawn under it, counted from the
    /// back of the pane's draw order: `Some(0)` draws it under all of them. `None`, the default,
    /// draws it over all of them, as drawings were drawn before this setting, and so does a
    /// count of at least the number of series and studies in the pane. Drawings with the same
    /// count are layered by [`z_order`](Self::z_order). The chart updates the count when a series
    /// or study is removed, moved out of the pane or moved in its order, and, in a user action,
    /// when one joins the pane or the drawing moves to another pane with the main series or its
    /// study, so the drawing stays over and under the same other items. An item new to the
    /// drawing goes under it where its place leaves the choice.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub series_layer: Option<usize>,
}

pub(super) fn enabled() -> bool {
    true
}

fn default_text_wrap_width() -> f32 {
    200.0
}

pub(super) const BLUE: [u8; 4] = [41, 98, 255, 255];
pub(super) const RED: [u8; 4] = [242, 54, 69, 255];
pub(super) const ORANGE: [u8; 4] = [255, 152, 0, 255];
pub(super) const GREEN: [u8; 4] = [76, 175, 80, 255];
pub(super) const TEAL: [u8; 4] = [8, 153, 129, 255];
pub(super) const CYAN: [u8; 4] = [0, 188, 212, 255];
pub(super) const PINK: [u8; 4] = [233, 30, 99, 255];
pub(super) const PURPLE: [u8; 4] = [156, 39, 176, 255];
pub(super) const DEEP_PURPLE: [u8; 4] = [103, 58, 183, 255];
pub(super) const GREY: [u8; 4] = [128, 128, 128, 255];
pub(super) const WHITE: [u8; 4] = [255, 255, 255, 255];

/// `color` at `alpha` of 255.
pub(super) const fn tint(color: [u8; 4], alpha: u8) -> [u8; 4] {
    [color[0], color[1], color[2], alpha]
}

/// Where a colour that rendering may shade for the theme sits in a drawing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ColorSlot {
    /// The line colour.
    Line,
    /// The text colour, while it is the tool's default line colour, as on lines and shapes.
    Text,
    /// A level by list and value: Fibonacci levels (0) and time levels (1), pitchfork levels
    /// (2), Gann price levels (3), time levels (4), fan lines (5) and arcs (6), and the upper (7)
    /// and lower (8) lines of the anchored VWAP bands by index, and projection levels (9).
    Level(u8, f32),
}

/// Line, fill and text defaults of a new drawing, before any default the user saved for its tool.
struct DefaultStyle {
    color: [u8; 4],
    width: f32,
    line_style: LineStyle,
    fill: Option<[u8; 4]>,
    font_size: f32,
}

/// A 2 px solid blue line with 14 px text and no fill, the style most tools start from.
const LINE: DefaultStyle = DefaultStyle {
    color: BLUE,
    width: 2.0,
    line_style: LineStyle::Solid,
    fill: None,
    font_size: 14.0,
};

/// The default style of each tool, chosen for its role: areas start filled with a light tint
/// (20%, 15% for chart patterns, 40% for date ranges), small labels and chart pattern labels take 12 px text.
fn default_style(tool: DrawingToolType) -> DefaultStyle {
    use DrawingToolType as T;
    let filled = |color: [u8; 4], alpha: u8| DefaultStyle {
        color,
        fill: Some(tint(color, alpha)),
        ..LINE
    };
    let colored = |color: [u8; 4]| DefaultStyle { color, ..LINE };
    match tool {
        T::HorizontalLine | T::HorizontalRay | T::CrossLine => DefaultStyle {
            font_size: 12.0,
            ..LINE
        },
        T::ParallelChannel => filled(BLUE, 51),
        // The fitted line is lighter than its text; bands carry their own style.
        T::RegressionTrend => DefaultStyle {
            color: tint(RED, 77),
            width: 1.0,
            fill: Some(tint(BLUE, 51)),
            ..LINE
        },
        T::FlatTopBottom => DefaultStyle {
            font_size: 12.0,
            ..filled(ORANGE, 51)
        },
        T::DisjointChannel => DefaultStyle {
            font_size: 12.0,
            ..filled(TEAL, 51)
        },
        T::Triangle => filled(TEAL, 51),
        T::DoubleCurve => colored(DEEP_PURPLE),
        T::ThreeDrivesPattern => DefaultStyle {
            font_size: 12.0,
            ..colored(DEEP_PURPLE)
        },
        T::Rect => filled(PURPLE, 51),
        T::RotatedRect => filled(GREEN, 51),
        T::Circle => filled(ORANGE, 51),
        T::Ellipse => filled(RED, 51),
        T::Polyline => filled(CYAN, 51),
        T::Brush => colored(CYAN),
        T::Arc => filled(PINK, 51),
        T::Sector => DefaultStyle {
            color: [156, 156, 156, 255],
            width: 1.0,
            fill: Some(tint(BLUE, 51)),
            ..LINE
        },
        T::Highlighter => DefaultStyle {
            color: tint(RED, 51),
            width: 20.0,
            ..LINE
        },
        T::ArrowMarker => DefaultStyle {
            color: [30, 83, 229, 255],
            font_size: 16.0,
            ..LINE
        },
        T::ArrowMarkUp => colored(TEAL),
        T::ArrowMarkDown => colored([204, 47, 60, 255]),
        T::Pitchfork
        | T::SchiffPitchfork
        | T::ModifiedSchiffPitchfork
        | T::InsidePitchfork
        | T::Pitchfan => colored(RED),
        // The colour, width and pattern of the trend line; levels have their own.
        T::FibonacciRetracement
        | T::FibonacciExtension
        | T::FibonacciChannel
        | T::TrendBasedFibTime
        | T::FibonacciCircles
        | T::FibonacciSpeedResistanceArcs => DefaultStyle {
            color: GREY,
            line_style: LineStyle::Dashed,
            ..LINE
        },
        T::FibonacciTimeZones => DefaultStyle {
            color: GREY,
            width: 1.0,
            line_style: LineStyle::Dashed,
            ..LINE
        },
        T::FibonacciWedge | T::FibonacciSpeedFan => colored(GREY),
        T::FibonacciSpiral => DefaultStyle {
            color: CYAN,
            width: 1.0,
            ..LINE
        },
        T::GhostFeed => DefaultStyle {
            color: GREY,
            width: 1.0,
            ..LINE
        },
        T::AnchoredVWAP => DefaultStyle {
            color: [30, 136, 229, 255],
            width: 1.0,
            ..LINE
        },
        // The colour of the range tint, the same cyan as the rows of rising volume.
        T::FixedRangeVolumeProfile | T::AnchoredVolumeProfile => colored([83, 196, 216, 255]),
        T::XABCDPattern | T::CypherPattern => DefaultStyle {
            font_size: 12.0,
            ..filled(BLUE, 38)
        },
        T::HeadAndShoulders => DefaultStyle {
            font_size: 12.0,
            ..filled(TEAL, 38)
        },
        T::ABCDPattern => DefaultStyle {
            font_size: 12.0,
            ..colored(TEAL)
        },
        T::SineLine => colored([21, 153, 128, 255]),
        T::TrianglePattern => DefaultStyle {
            font_size: 12.0,
            ..filled(DEEP_PURPLE, 38)
        },
        T::ElliottImpulse | T::ElliottCorrection => colored([61, 133, 198, 255]),
        T::ElliottTriangle => colored(ORANGE),
        T::ElliottDoubleCombo | T::ElliottTripleCombo => colored([106, 168, 79, 255]),
        T::TrendAngle => DefaultStyle {
            font_size: 12.0,
            ..LINE
        },
        T::ProjectionTool => colored([156, 156, 156, 255]),
        T::CyclicLines => colored([144, 202, 249, 255]),
        T::TimeCycles => DefaultStyle {
            color: [21, 153, 128, 255],
            fill: Some([106, 168, 79, 128]),
            ..LINE
        },
        T::Measure | T::PriceRange => DefaultStyle {
            font_size: 12.0,
            ..filled(BLUE, 38)
        },
        T::DateRange | T::DateAndPriceRange => DefaultStyle {
            font_size: 12.0,
            ..filled(BLUE, 102)
        },
        // Text boxes: the fill is the box colour, used once the background is on.
        T::TextLabel | T::AnchoredText => DefaultStyle {
            fill: Some(tint(BLUE, 64)),
            ..LINE
        },
        T::Note => filled(BLUE, 179),
        T::Callout => filled([0, 151, 167, 255], 179),
        T::PriceNote | T::Signpost => DefaultStyle {
            font_size: 12.0,
            ..LINE
        },
        T::Comment => DefaultStyle {
            font_size: 16.0,
            ..LINE
        },
        T::Table => colored([219, 219, 219, 255]),
        T::FontIcon => DefaultStyle {
            font_size: 40.0,
            ..LINE
        },
        T::Sticker => DefaultStyle {
            font_size: 110.0,
            ..LINE
        },
        _ => LINE,
    }
}

impl Drawing {
    /// The synchronization this drawing follows: that of its top-level folder, or `None` when
    /// unset or at the root.
    pub fn folder_sync_mode(&self) -> Option<FolderSync> {
        if self.folder.is_empty() {
            return None;
        }
        self.folder_sync.first().copied().flatten()
    }

    /// Whether the top-level folder of this drawing keeps it on one chart or in one layout,
    /// which drawing storage does not hold.
    pub(crate) fn confined(&self) -> bool {
        matches!(
            self.folder_sync_mode(),
            Some(FolderSync::Chart | FolderSync::Layout)
        )
    }

    /// Whether the user can edit this drawing now: it is not [locked](Self::locked) and
    /// [user edits](Self::user_edit_enabled) are on.
    /// Lock all drawings is not part of it: see
    /// [`DrawingManager::accepts_user_edits`](crate::drawings::DrawingManager::accepts_user_edits).
    pub fn accepts_user_edits(&self) -> bool {
        self.user_edit_enabled && !self.locked
    }

    /// Create a new drawing with default properties
    pub fn new(id: usize, tool_type: DrawingToolType) -> Self {
        let style = default_style(tool_type);
        // Text starts without a box; switching one on shows the tinted fill and a grey border.
        let plain_text = matches!(
            tool_type,
            DrawingToolType::TextLabel | DrawingToolType::AnchoredText
        );
        Self {
            id,
            stored_id: None,
            created_at: None,
            modified_at: None,
            notes: None,
            label: None,
            selectable: true,
            saveable: true,
            listed: true,
            user_edit_enabled: true,
            group: None,
            folder: Vec::new(),
            folder_sync: Vec::new(),
            tool_type,
            custom_tool: None,
            custom_settings: serde_json::Value::Null,
            points: Vec::new(),
            chart_points: Vec::new(),
            level_ys: Vec::new(),
            screen_anchor: None,
            pane: None,
            color: style.color,
            stroke_width: style.width,
            completed: false,
            text: (tool_type == DrawingToolType::Sticker).then(|| "sticker:bull".into()),
            post: None,
            // Styling defaults
            line_style: style.line_style,
            fill_color: style.fill,
            locked: false,
            visible: true,
            font_size: style.font_size,
            font_weight: if matches!(
                tool_type,
                DrawingToolType::ArrowMarker | DrawingToolType::PriceLabel
            ) {
                FontWeight::Bold
            } else {
                FontWeight::Normal
            },
            italic: false,
            text_color: match tool_type {
                DrawingToolType::Note | DrawingToolType::Callout => Some(WHITE),
                // Independent of the line, which can change colour without its text.
                DrawingToolType::TrendLine
                | DrawingToolType::Ray
                | DrawingToolType::ExtendedLine
                | DrawingToolType::InfoLine
                | DrawingToolType::HorizontalLine
                | DrawingToolType::HorizontalRay
                | DrawingToolType::VerticalLine => Some(BLUE),
                // As is the text of a shape, which starts in the shape's own colour.
                DrawingToolType::Rect | DrawingToolType::Circle | DrawingToolType::Ellipse => {
                    Some(style.color)
                }
                // Opaque above its translucent line.
                DrawingToolType::RegressionTrend => Some(RED),
                _ => None,
            },
            background: !plain_text,
            border: !plain_text,
            border_color: plain_text.then_some([112, 112, 112, 255]),
            text_wrap: false,
            text_wrap_width: default_text_wrap_width(),
            text_follows_zoom: false,
            text_zoom_spacing: None,
            note_text_style: None,
            extend_left: tool_type == DrawingToolType::ExtendedLine,
            extend_right: matches!(
                tool_type,
                DrawingToolType::Ray | DrawingToolType::ExtendedLine
            ),
            arrow_start_style: ArrowStyle::None,
            arrow_end_style: if matches!(tool_type, DrawingToolType::Arrow | DrawingToolType::Path)
            {
                ArrowStyle::Arrow
            } else {
                ArrowStyle::None
            },
            timeframe_visibility: TimeframeVisibility::all(),
            price_axis_label: true,
            time_axis_label: true,
            line_measurements: LineMeasurements::for_tool(tool_type),
            range_settings: RangeSettings::for_tool(tool_type),
            position_size: matches!(
                tool_type,
                DrawingToolType::LongPos | DrawingToolType::ShortPos
            )
            .then_some(PositionSize::RiskPercent {
                account_size: 1_000.0,
                percent: 25.0,
            }),
            position_sizing: PositionSizing::default(),
            position_readout: PositionReadout::default(),
            position_target_color: matches!(
                tool_type,
                DrawingToolType::LongPos | DrawingToolType::ShortPos
            )
            .then_some(tint(TEAL, 51)),
            position_stop_color: matches!(
                tool_type,
                DrawingToolType::LongPos | DrawingToolType::ShortPos
            )
            .then_some(tint(RED, 51)),
            position_style: PositionStyle::default(),
            marker: AnnotationMarker::default(),
            fib_config: matches!(
                tool_type,
                DrawingToolType::FibonacciRetracement
                    | DrawingToolType::FibonacciExtension
                    | DrawingToolType::FibonacciChannel
                    | DrawingToolType::FibonacciWedge
                    | DrawingToolType::FibonacciTimeZones
                    | DrawingToolType::FibonacciSpeedFan
                    | DrawingToolType::TrendBasedFibTime
                    | DrawingToolType::FibonacciCircles
                    | DrawingToolType::FibonacciSpeedResistanceArcs
                    | DrawingToolType::FibonacciSpiral
            )
            .then(|| FibonacciConfig::for_tool(tool_type)),
            volume_profile: matches!(
                tool_type,
                DrawingToolType::FixedRangeVolumeProfile | DrawingToolType::AnchoredVolumeProfile
            )
            .then(VolumeProfileConfig::drawing_default),
            regression_trend: (tool_type == DrawingToolType::RegressionTrend)
                .then(RegressionTrendConfig::drawing_default),
            vwap: None,
            pitchfork: matches!(
                tool_type,
                DrawingToolType::Pitchfork
                    | DrawingToolType::SchiffPitchfork
                    | DrawingToolType::ModifiedSchiffPitchfork
                    | DrawingToolType::InsidePitchfork
                    | DrawingToolType::Pitchfan
            )
            .then(|| PitchforkConfig::for_tool(tool_type)),
            elliott_degree: if matches!(
                tool_type,
                DrawingToolType::ElliottImpulse
                    | DrawingToolType::ElliottCorrection
                    | DrawingToolType::ElliottTriangle
                    | DrawingToolType::ElliottDoubleCombo
                    | DrawingToolType::ElliottTripleCombo
            ) {
                ElliottDegree::Intermediate
            } else {
                ElliottDegree::default()
            },
            elliott_show_wave: true,
            channel_midline: tool_type == DrawingToolType::ParallelChannel,
            anchor_price_style: None,
            middle_line: MiddleLineStyle::default(),
            theme_shade: tool_type != DrawingToolType::ProjectionTool,
            image: None,
            glyph_angle: 0.0,
            closed: false,
            gann: matches!(
                tool_type,
                DrawingToolType::GannBox
                    | DrawingToolType::GannSquare
                    | DrawingToolType::GannFixed
                    | DrawingToolType::GannFan
            )
            .then(|| GannConfig::drawing_default(tool_type)),
            ghost_feed: (tool_type == DrawingToolType::GhostFeed)
                .then(GhostFeedConfig::drawing_default),
            bars_pattern: (tool_type == DrawingToolType::BarsPattern).then(|| BarsPatternConfig {
                mode: BarsPatternMode::HighLowBars,
                ..Default::default()
            }),
            projection: (tool_type == DrawingToolType::ProjectionTool)
                .then(ProjectionConfig::default),
            forecast: (tool_type == DrawingToolType::Forecast).then(ForecastStyle::default),
            // Z-order (default: 0, higher values render on top)
            z_order: 0,
            series_layer: None,
        }
    }

    /// Clockwise radians for painting; invalid host values draw upright and cannot be saved.
    pub(crate) fn glyph_radians(&self) -> f32 {
        if self.glyph_angle.is_finite() {
            self.glyph_angle.to_radians()
        } else {
            0.0
        }
    }

    /// Stores a clockwise angle in degrees in [0, 360).
    pub(crate) fn set_glyph_angle(&mut self, degrees: f32) {
        let degrees = degrees.rem_euclid(360.0);
        // A negative value just below zero can round its remainder up to 360.
        self.glyph_angle = if degrees < 360.0 { degrees } else { 0.0 };
    }

    /// Creates a host-defined drawing. The manager looks up `tool` in its registry; the
    /// built-in polyline identity supplies anchor handles and multi-click input only.
    pub fn custom(tool: impl Into<String>) -> Self {
        let mut drawing = Self::new(0, DrawingToolType::Polyline);
        drawing.custom_tool = Some(tool.into());
        drawing
    }

    /// Create a new drawing with a specific line colour; fills, borders, pitchfork levels and
    /// projection levels sharing the tool's line colour take the new RGB at their own opacity.
    pub fn with_color(id: usize, tool_type: DrawingToolType, color: [u8; 4]) -> Self {
        let mut drawing = Self::new(id, tool_type);
        drawing.set_line_color(color);
        drawing
    }

    /// Set the line colour and retint fills, borders, pitchfork levels and projection levels
    /// sharing its current RGB, keeping each one's opacity. Independently coloured levels keep
    /// their colours.
    pub fn set_line_color(&mut self, color: [u8; 4]) {
        for shared in [&mut self.fill_color, &mut self.border_color] {
            if let Some(shared) = shared
                && shared[..3] == self.color[..3]
            {
                shared[..3].copy_from_slice(&color[..3]);
            }
        }
        if let Some(config) = &mut self.pitchfork {
            for level in &mut config.levels {
                if level.color[..3] == self.color[..3] {
                    level.color[..3].copy_from_slice(&color[..3]);
                }
            }
        }
        if let Some(config) = &mut self.projection {
            for level in &mut config.levels {
                if level.color[..3] == self.color[..3] {
                    level.color[..3].copy_from_slice(&color[..3]);
                }
            }
        }
        self.color = color;
    }

    /// The projection settings this drawing is drawn with: its own, or for one saved without
    /// them, the settings that draw it as it was drawn.
    pub(crate) fn projection_config(&self) -> std::borrow::Cow<'_, ProjectionConfig> {
        match &self.projection {
            Some(config) => std::borrow::Cow::Borrowed(config),
            None => std::borrow::Cow::Owned(ProjectionConfig::legacy(self)),
        }
    }

    /// The text colour of Text and Anchored text that has one. Their colour controls show and
    /// edit it, as the drawing colour then paints at most a border left to follow it.
    #[cfg(feature = "ui")]
    pub(crate) fn text_color_override(&self) -> Option<[u8; 4]> {
        matches!(
            self.tool_type,
            DrawingToolType::TextLabel | DrawingToolType::AnchoredText
        )
        .then_some(self.text_color)
        .flatten()
    }

    /// Set the colour picked in a colour control. A price label's badge stays one colour, so its
    /// border and fill follow as in [`Self::set_line_color`]. Text with a colour of its own takes
    /// the pick there, and its drawing colour takes the RGB at its own opacity; other tools keep
    /// their other colours.
    #[cfg(feature = "ui")]
    pub(crate) fn recolor(&mut self, color: [u8; 4]) {
        if self.tool_type == DrawingToolType::PriceLabel {
            self.set_line_color(color);
        } else if self.text_color_override().is_some() {
            self.text_color = Some(color);
            self.color[..3].copy_from_slice(&color[..3]);
        } else {
            self.color = color;
        }
    }

    /// The fill a `tool` drawing takes when Fill is switched on: its own default where that is not
    /// a tint of its line, as on the regression trend's upper band, otherwise `line` at 20%.
    #[cfg(feature = "ui")]
    pub(crate) fn first_fill(tool: DrawingToolType, line: [u8; 4]) -> [u8; 4] {
        let style = default_style(tool);
        match style.fill {
            Some(fill) if fill[..3] != style.color[..3] => fill,
            _ => tint(line, 51),
        }
    }

    /// Create a new text label drawing
    pub fn with_text(id: usize, text: String) -> Self {
        let mut drawing = Self::new(id, DrawingToolType::TextLabel);
        drawing.text = Some(text);
        drawing.arrow_end_style = ArrowStyle::None;
        drawing
    }

    /// Whether the line colour is still the tool's default, which rendering may shade to read on
    /// the theme, rather than one chosen for this drawing.
    pub(crate) fn has_default_color(&self) -> bool {
        self.color == default_style(self.tool_type).color
            || Self::former_default_color(self.tool_type) == Some(self.color)
    }

    /// A tool's former creation colour, still shaded when held by a saved drawing.
    pub(crate) fn former_default_color(tool: DrawingToolType) -> Option<[u8; 4]> {
        use DrawingToolType as T;
        match tool {
            T::AnchoredVWAP => Some(BLUE),
            T::Circle | T::RegressionTrend => Some(RED),
            T::SineLine => Some(TEAL),
            T::ElliottTriangle | T::ElliottDoubleCombo | T::ElliottTripleCombo => {
                Some([61, 133, 198, 255])
            }
            _ => None,
        }
    }

    /// Calls `visit` with each line, text and level colour that rendering may shade for the
    /// theme. A Gann drawing or an anchored VWAP without a stored configuration first takes the
    /// defaults it draws. Volume profile drawings have none: their colour tints an area.
    pub(crate) fn visit_theme_colors(&mut self, mut visit: impl FnMut(ColorSlot, &mut [u8; 4])) {
        use DrawingToolType as T;
        let tool = self.tool_type;
        if matches!(tool, T::FixedRangeVolumeProfile | T::AnchoredVolumeProfile) {
            return;
        }
        let defaults = [
            Some(default_style(tool).color),
            Self::former_default_color(tool),
        ];
        let default = |color: &[u8; 4]| defaults.contains(&Some(*color));
        if let Some(text) = self.text_color.as_mut().filter(|text| default(text)) {
            visit(ColorSlot::Text, text);
        }
        visit(ColorSlot::Line, &mut self.color);
        // Prices and a middle line set in the tool's line colour take the line's shade.
        for color in [
            self.anchor_price_style
                .as_mut()
                .map(|style| &mut style.color),
            self.middle_line.color.as_mut(),
        ]
        .into_iter()
        .flatten()
        .filter(|color| default(color))
        {
            visit(ColorSlot::Line, color);
        }
        if let Some(config) = &mut self.fib_config {
            for level in &mut config.levels {
                visit(ColorSlot::Level(0, level.value), &mut level.color);
            }
            for level in config.time_levels.iter_mut().flatten() {
                visit(ColorSlot::Level(1, level.value), &mut level.color);
            }
        }
        if let Some(config) = &mut self.projection {
            for level in &mut config.levels {
                visit(ColorSlot::Level(9, level.coefficient), &mut level.color);
            }
        }
        if let Some(config) = &mut self.pitchfork {
            for level in &mut config.levels {
                visit(ColorSlot::Level(2, level.value), &mut level.color);
            }
        }
        if matches!(tool, T::GannBox | T::GannSquare | T::GannFixed | T::GannFan) {
            let config = self.gann.get_or_insert_with(|| GannConfig::for_tool(tool));
            for (list, levels) in [
                (3, &mut config.price_levels),
                (4, &mut config.time_levels),
                (5, &mut config.fan_ratios),
                (6, &mut config.arc_ratios),
            ] {
                for level in levels {
                    visit(ColorSlot::Level(list, level.value), &mut level.color);
                }
            }
        }
        if tool == T::AnchoredVWAP {
            let config = self.vwap.get_or_insert_with(VwapConfig::default);
            for (index, band) in config.bands.iter_mut().enumerate() {
                visit(ColorSlot::Level(7, index as f32), &mut band.upper.color);
                visit(ColorSlot::Level(8, index as f32), &mut band.lower.color);
            }
        }
    }

    /// Update screen coordinates from chart_points using current transforms
    ///
    /// This should be called each frame before rendering.
    pub fn update_screen_coords<F, G>(&mut self, bar_to_x: F, price_to_y: G)
    where
        F: Fn(f32) -> f32,
        G: Fn(f64) -> f32,
    {
        self.points.clear();
        for cp in &self.chart_points {
            let x = bar_to_x(cp.bar_idx);
            let y = price_to_y(cp.price);
            self.points.push(egui::Pos2::new(x, y));
        }
        self.level_ys = match (self.tool_type, &self.chart_points[..]) {
            // Five of the first units in price, which a logarithmic scale does not keep linear.
            (DrawingToolType::GannFixed, [a, b, ..]) => {
                vec![price_to_y(
                    a.price + (b.price - a.price) * f64::from(GANN_SQUARE_UNITS),
                )]
            }
            _ => self
                .fib_levels()
                .map(|(_, prices)| prices.into_iter().map(&price_to_y).collect())
                .unwrap_or_default(),
        };
    }

    /// Records `bar_spacing` as the spacing a Text or Anchored text has its set size at, when it
    /// has none: the bar spacing it is first displayed at. A spacing that is not a positive
    /// number is not recorded.
    pub(crate) fn adopt_text_zoom_spacing(&mut self, bar_spacing: f32) {
        if self.text_zoom_spacing.is_none()
            && matches!(
                self.tool_type,
                DrawingToolType::TextLabel | DrawingToolType::AnchoredText
            )
            && !self.chart_points.is_empty()
            && bar_spacing.is_finite()
            && bar_spacing > 0.0
        {
            self.text_zoom_spacing = Some(bar_spacing);
        }
    }

    /// The factor that scales text from its set size: `bar_spacing`, the chart's current bar
    /// spacing, over `text_zoom_spacing` for text that follows the zoom, and 1 otherwise or
    /// when `bar_spacing` is not a positive number.
    pub(crate) fn text_zoom(&self, bar_spacing: f32) -> f32 {
        match (self.text_follows_zoom, self.text_zoom_spacing) {
            (true, Some(reference)) if bar_spacing.is_finite() && bar_spacing > 0.0 => {
                bar_spacing / reference
            }
            _ => 1.0,
        }
    }

    /// Rebuilds anchored text coordinates relative to the pane. Call after
    /// `update_screen_coords`; other tools retain their chart coordinates. A text without a pane
    /// position, such as an older drawing or one just moved, takes one the first time its point
    /// is drawn inside `pane`, and stays on its bar and price until then. Resizing preserves the
    /// width and height fractions.
    pub fn update_pane_coords(&mut self, pane: egui::Rect) {
        if self.tool_type != DrawingToolType::AnchoredText
            || !pane.is_finite()
            || pane.width() <= 0.0
            || pane.height() <= 0.0
        {
            return;
        }
        // A position taken outside the pane would keep the text out of view whatever the
        // chart scrolls to; until it is drawn inside, the text stays on its bar and price.
        if let Some(pos) = self.points.first_mut()
            && (self.screen_anchor.is_some() || pane.contains(*pos))
        {
            let anchor = self.screen_anchor.get_or_insert_with(|| {
                [
                    (pos.x - pane.left()) / pane.width(),
                    (pos.y - pane.top()) / pane.height(),
                ]
            });
            *pos = pane.min + egui::vec2(anchor[0] * pane.width(), anchor[1] * pane.height());
        }
    }

    /// The Fibonacci config in use and the price of each of its visible levels, for a retracement
    /// or an extension projected from its third anchor (the first anchor for legacy two-point drawings).
    pub(crate) fn fib_levels(&self) -> Option<(std::borrow::Cow<'_, FibonacciConfig>, Vec<f64>)> {
        let extension = match self.tool_type {
            DrawingToolType::FibonacciRetracement => false,
            DrawingToolType::FibonacciExtension => true,
            _ => return None,
        };
        let [start, end, ..] = &self.chart_points[..] else {
            return None;
        };
        let config = self.fibonacci_config();
        let origin = self.chart_points.get(2).unwrap_or(start).price;
        let prices = config
            .level_prices(start.price, end.price, extension, origin)
            .collect();
        Some((config, prices))
    }

    /// The price levels an alert can follow at the fractional bar index `bar`, each with its
    /// level number, name and price: a line's price extended through its two anchors, the base and
    /// second lines of parallel or disjoint channels, the trend and flat lines of a flat
    /// top/bottom, a horizontal level, a rectangle's top and bottom, a position's entry, target
    /// and stop, each visible Fibonacci retracement, extension or channel level, numbered by its
    /// place in the level settings and named by its ratio as the drawing labels it, each ray of a
    /// Fibonacci speed resistance fan, the price level at place `n` numbered `2n` and the time level
    /// at place `n` numbered `2n + 1`, and each Gann fan line, numbered by its place in the fan
    /// ratios and named by its ratio, such as `2×1`. Sloping lines are straight on the price axis
    /// `scale` paints, so they are linear in price except on a logarithmic scale; channel lines run
    /// on through their anchors, and fan rays have a price only from their first anchor on, on the
    /// side they are drawn.
    /// Fork levels use two stable slots per configured level (first and second line), with
    /// only the first slot used by a median. Regression and anchored VWAP levels require
    /// [`Self::alert_levels_with_bars`]. Empty for other drawings and for
    /// geometry with no price at `bar`, such as a vertical line, a Fibonacci channel with two
    /// anchors, a vertical fan ray or a sloping line through a non-positive price on a
    /// logarithmic scale.
    pub fn alert_levels(&self, bar: f64, scale: PriceScaleMode) -> Vec<(usize, String, f64)> {
        use DrawingToolType as T;
        let named = |names: &[&str], prices: &[f64]| {
            names
                .iter()
                .zip(prices)
                .enumerate()
                .map(|(level, (name, price))| (level, (*name).to_owned(), *price))
                .collect::<Vec<_>>()
        };
        let log = scale == PriceScaleMode::Logarithmic;
        let to_axis = |price: f64| if log { price.ln() } else { price };
        // A vertical line or one through a non-positive price is minus infinity here, not zero.
        let from_axis = |value: f64| {
            if log && value.is_finite() {
                value.exp()
            } else {
                value
            }
        };
        // The line through `a` and `b` at `bar`, in axis coordinates.
        let along = |a: &ChartPoint, b: &ChartPoint, bar: f64| {
            let (from, to) = (to_axis(a.price), to_axis(b.price));
            from + (bar - f64::from(a.bar_idx)) * (to - from) / f64::from(b.bar_idx - a.bar_idx)
        };
        let levels = match (self.tool_type, &self.chart_points[..]) {
            (T::HorizontalLine | T::HorizontalRay, [level, ..]) => named(&["Line"], &[level.price]),
            (
                T::TrendLine | T::Ray | T::ExtendedLine | T::InfoLine | T::TrendAngle,
                [start, end, ..],
            ) => named(&["Line"], &[from_axis(along(start, end, bar))]),
            // The second line is the base line moved straight up or down through the third anchor.
            (T::ParallelChannel, [start, end, through, ..]) => {
                let shift = to_axis(through.price) - along(start, end, f64::from(through.bar_idx));
                named(
                    &["Line", "Parallel line"],
                    &[
                        from_axis(along(start, end, bar)),
                        from_axis(along(start, end, bar) + shift),
                    ],
                )
            }
            (T::FlatTopBottom, [start, end, flat, ..]) => named(
                &["Line", "Flat line"],
                &[from_axis(along(start, end, bar)), flat.price],
            ),
            // The second line passes through the third anchor with the opposite slope.
            (T::DisjointChannel, [start, end, from, ..]) => {
                let rise = to_axis(end.price) - to_axis(start.price);
                let run = f64::from(end.bar_idx - start.bar_idx);
                named(
                    &["Line", "Second line"],
                    &[
                        from_axis(along(start, end, bar)),
                        from_axis(
                            to_axis(from.price) - (bar - f64::from(from.bar_idx)) * rise / run,
                        ),
                    ],
                )
            }
            (tool, _) if super::alert_levels::is_fork(tool) => self.fork_alert_levels(bar, scale),
            (T::Rect, [a, b, ..]) => named(
                &["Top", "Bottom"],
                &[a.price.max(b.price), a.price.min(b.price)],
            ),
            (T::LongPos | T::ShortPos, [entry, target, rest @ ..]) => named(
                &["Entry", "Target", "Stop"],
                &[
                    entry.price,
                    target.price,
                    // Until the stop is placed it mirrors the target around the entry.
                    rest.first()
                        .map_or(2.0 * entry.price - target.price, |stop| stop.price),
                ],
            ),
            // Each line is the base translated by its ratio times the third anchor's offset.
            (T::FibonacciChannel, [start, end, through, ..]) => {
                let config = self.fibonacci_config();
                let shift = to_axis(through.price) - along(start, end, f64::from(through.bar_idx));
                let base = along(start, end, bar);
                config
                    .levels
                    .iter()
                    .enumerate()
                    .filter(|(_, level)| level.visible)
                    .map(|(index, level)| {
                        (
                            index,
                            level.label(config.levels_as_percent),
                            from_axis(base + f64::from(level.value) * shift),
                        )
                    })
                    .filter(|(.., price)| price.is_finite())
                    .collect()
            }
            (T::FibonacciSpeedFan, _) => self.speed_fan_alert_levels(bar, scale),
            (T::GannFan, _) => self.gann_fan_alert_levels(bar, scale),
            _ => self
                .fib_levels()
                .map(|(config, prices)| {
                    config
                        .levels
                        .iter()
                        .enumerate()
                        .filter(|(_, level)| level.visible)
                        .zip(prices)
                        .map(|((index, level), price)| {
                            (index, level.label(config.levels_as_percent), price)
                        })
                        .collect()
                })
                .unwrap_or_default(),
        };
        if levels.iter().all(|(_, _, price)| price.is_finite()) {
            levels
        } else {
            Vec::new()
        }
    }

    /// The alert source of level `level` of [`Self::alert_levels`]: the drawing's stored id, or
    /// its id when it was never stored, followed by `#` and the level for levels after the
    /// first. `%` and `#` in the id are escaped as `%25` and `%23` so distinct drawings cannot
    /// share a source. It names the same level after the drawing is saved and restored.
    pub fn alert_source(&self, level: usize) -> AlertSource {
        let id = self.stable_id();
        AlertSource::Drawing(if level == 0 {
            id.into_owned()
        } else {
            format!("{id}#{level}")
        })
    }

    /// The level of this drawing that `source`, from [`Self::alert_source`], names.
    pub fn alert_level(&self, source: &AlertSource) -> Option<usize> {
        let AlertSource::Drawing(source) = source else {
            return None;
        };
        let id = self.stable_id();
        if *source == id {
            return Some(0);
        }
        source
            .strip_prefix(id.as_ref())?
            .strip_prefix('#')?
            .parse()
            .ok()
            .filter(|level| *level > 0)
    }

    fn stable_id(&self) -> std::borrow::Cow<'_, str> {
        let id: std::borrow::Cow<'_, str> = self
            .stored_id
            .as_deref()
            .map_or_else(|| self.id.to_string().into(), Into::into);
        if id.contains('%') || id.contains('#') {
            id.replace('%', "%25").replace('#', "%23").into()
        } else {
            id
        }
    }

    /// Add a point with both screen and chart coordinates
    pub fn add_point_with_chart_coords<F, G>(
        &mut self,
        screen_point: egui::Pos2,
        x_to_bar: F,
        y_to_price: G,
    ) where
        F: Fn(f32) -> f32,
        G: Fn(f32) -> f64,
    {
        let bar_idx = x_to_bar(screen_point.x);
        let price = y_to_price(screen_point.y);
        self.chart_points.push(ChartPoint::new(bar_idx, price));
        self.points.push(screen_point);
        self.check_completion();
    }

    /// Update the last point with both screen and chart coordinates
    pub fn update_last_point_with_chart_coords<F, G>(
        &mut self,
        screen_point: egui::Pos2,
        x_to_bar: F,
        y_to_price: G,
    ) where
        F: Fn(f32) -> f32,
        G: Fn(f32) -> f64,
    {
        if let (Some(last_point), Some(last_chart_point)) =
            (self.points.last_mut(), self.chart_points.last_mut())
        {
            *last_point = screen_point;
            *last_chart_point =
                ChartPoint::new(x_to_bar(screen_point.x), y_to_price(screen_point.y));
        }
    }

    /// Keeps a second anchor on the first one's price for Time cycles, so it only sets the period,
    /// and on the first one's bar for a Signpost, so it only sets the label's height.
    pub(crate) fn align_second_anchor(&mut self) {
        if self.points.len() < 2 || self.chart_points.len() < 2 {
            return;
        }
        match self.tool_type {
            DrawingToolType::TimeCycles => {
                self.points[1].y = self.points[0].y;
                self.chart_points[1].price = self.chart_points[0].price;
            }
            DrawingToolType::Signpost => {
                self.points[1].x = self.points[0].x;
                self.chart_points[1].bar_idx = self.chart_points[0].bar_idx;
            }
            _ => {}
        }
    }

    /// Add a screen point (legacy method)
    pub fn add_point(&mut self, point: egui::Pos2) {
        self.points.push(point);
        self.check_completion();
    }

    /// Shift the bar indices at or after `from` by a delta (used when bars are inserted in front
    /// of them)
    pub fn shift_bar_indices(&mut self, from: f32, delta: f32) {
        for cp in &mut self.chart_points {
            if cp.bar_idx >= from {
                cp.shift_bar_idx(delta);
            }
        }
    }

    /// Index of the first anchor that moving the whole drawing moves. A bars pattern's source
    /// anchors stay on the bars it copies, so a move takes only its copy's anchor; every other
    /// drawing, and a bars pattern still without a copy anchor, moves all of its anchors.
    pub(crate) fn first_moved_anchor(&self) -> usize {
        if self.tool_type == DrawingToolType::BarsPattern && self.chart_points.len() >= 3 {
            2
        } else {
            0
        }
    }

    /// Check and set completion status based on tool type
    fn check_completion(&mut self) {
        if let Some(required) = self.tool_type.required_points()
            && self.points.len() >= required
        {
            self.completed = true;
        }
    }

    /// Get the egui Color32 from the RGBA array
    #[inline]
    pub fn color32(&self) -> egui::Color32 {
        egui::Color32::from_rgba_unmultiplied(
            self.color[0],
            self.color[1],
            self.color[2],
            self.color[3],
        )
    }

    /// Get the fill color as Color32, if set
    #[inline]
    pub fn fill_color32(&self) -> Option<egui::Color32> {
        self.fill_color
            .map(|c| egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3]))
    }

    /// Get the stroke for rendering
    #[inline]
    pub fn stroke(&self) -> egui::Stroke {
        egui::Stroke::new(self.stroke_width, self.color32())
    }

    /// Check if this drawing is visible on the given timeframe
    #[inline]
    pub fn is_visible_on_timeframe(&self, timeframe: &str) -> bool {
        self.visible && self.timeframe_visibility.is_visible_on(timeframe)
    }

    /// Whether drawings of `tool` can tag their price on the price axis and their time on the
    /// time axis: horizontal lines, rays and positions their prices, vertical lines their time,
    /// cross lines both.
    pub(crate) fn axis_labels_of(tool: DrawingToolType) -> (bool, bool) {
        use DrawingToolType as T;
        match tool {
            T::HorizontalLine | T::HorizontalRay | T::LongPos | T::ShortPos => (true, false),
            T::VerticalLine => (false, true),
            T::CrossLine => (true, true),
            _ => (false, false),
        }
    }

    /// Whether the drawing also paints its line in the chart's other panes.
    pub(crate) fn reaches_other_panes(&self) -> bool {
        self.tool_type == DrawingToolType::VerticalLine && self.line_measurements.across_panes
    }

    /// Get the z-order of this drawing
    #[inline]
    pub fn z_order(&self) -> i32 {
        self.z_order
    }

    /// Set the z-order of this drawing
    #[inline]
    pub fn set_z_order(&mut self, order: i32) {
        self.z_order = order;
    }

    /// Get the bounding rect of the drawing's points
    pub fn bounding_rect(&self) -> Option<egui::Rect> {
        if self.points.is_empty() {
            return None;
        }

        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for p in &self.points {
            min_x = min_x.min(p.x);
            min_y = min_y.min(p.y);
            max_x = max_x.max(p.x);
            max_y = max_y.max(p.y);
        }

        Some(egui::Rect::from_min_max(
            egui::pos2(min_x, min_y),
            egui::pos2(max_x, max_y),
        ))
    }

    /// Quadratic bezier curve interpolation
    ///
    /// Returns a point on the bezier curve defined by p0, p1, p2 at parameter t.
    /// Used for smooth curve rendering (e.g., arcs, spirals).
    pub fn quadratic_bezier(p0: egui::Pos2, p1: egui::Pos2, p2: egui::Pos2, t: f32) -> egui::Pos2 {
        let u = 1.0 - t;
        egui::Pos2::new(
            u * u * p0.x + 2.0 * u * t * p1.x + t * t * p2.x,
            u * u * p0.y + 2.0 * u * t * p1.y + t * t * p2.y,
        )
    }
}

impl Default for Drawing {
    fn default() -> Self {
        Self::new(0, DrawingToolType::TrendLine)
    }
}

impl Serialize for Drawing {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        Self::serialize(self, serializer)
    }
}

impl<'de> Deserialize<'de> for Drawing {
    /// A saved `gann` object takes the fields it leaves out from [`GannConfig::for_tool`] of the
    /// drawing's tool, which a drawing without one draws. Range settings it leaves out take
    /// the drawing's font size for the readouts.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        GannConfig::forget_left_out();
        LineMeasurements::forget_left_out();
        RangeSettings::forget_left_out();
        let mut drawing = Self::deserialize(deserializer)?;
        drawing.line_measurements.fill_left_out(drawing.tool_type);
        drawing.range_settings.fill_left_out(drawing.font_size);
        if let Some(gann) = &mut drawing.gann {
            gann.fill_left_out(drawing.tool_type);
        }
        Ok(drawing)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// New drawings start in their tool's style. Chart patterns, the flat top/bottom and
    /// disjoint channels and the trend angle write 12 px text; a circle is orange with an orange
    /// 20% fill and text; a sine line takes the green time cycles start in; each kind of
    /// Elliott wave has its colour and starts at the Intermediate degree; a regression trend
    /// draws a 1 px line in red at 30% opacity (77 of 255) under opaque red text, with 2 px bands
    /// in blue at 30% opacity; a price label is bold; a position risks 25% of an account of
    /// 1,000 in lots of 1, over a green target and a red stop zone at 20% opacity (51). A
    /// parallel channel, flat top/bottom or disjoint channel starts its text at the left end of
    /// its first line, and one saved without line measurements keeps it in the middle. The
    /// triangle, ellipse and double curve keep the styles they shared before.
    #[test]
    fn new_drawings_start_in_their_tool_style() {
        use DrawingToolType as T;
        type Style = (T, [u8; 4], f32, Option<[u8; 4]>, f32, Option<[u8; 4]>);
        let rows: [Style; 20] = [
            (T::ABCDPattern, [8, 153, 129, 255], 2.0, None, 12.0, None),
            (
                T::XABCDPattern,
                [41, 98, 255, 255],
                2.0,
                Some([41, 98, 255, 38]),
                12.0,
                None,
            ),
            (
                T::CypherPattern,
                [41, 98, 255, 255],
                2.0,
                Some([41, 98, 255, 38]),
                12.0,
                None,
            ),
            (
                T::HeadAndShoulders,
                [8, 153, 129, 255],
                2.0,
                Some([8, 153, 129, 38]),
                12.0,
                None,
            ),
            (
                T::TrianglePattern,
                [103, 58, 183, 255],
                2.0,
                Some([103, 58, 183, 38]),
                12.0,
                None,
            ),
            (
                T::ThreeDrivesPattern,
                [103, 58, 183, 255],
                2.0,
                None,
                12.0,
                None,
            ),
            (
                T::Circle,
                [255, 152, 0, 255],
                2.0,
                Some([255, 152, 0, 51]),
                14.0,
                Some([255, 152, 0, 255]),
            ),
            (T::SineLine, [21, 153, 128, 255], 2.0, None, 14.0, None),
            (
                T::FlatTopBottom,
                [255, 152, 0, 255],
                2.0,
                Some([255, 152, 0, 51]),
                12.0,
                None,
            ),
            (
                T::DisjointChannel,
                [8, 153, 129, 255],
                2.0,
                Some([8, 153, 129, 51]),
                12.0,
                None,
            ),
            (T::TrendAngle, [41, 98, 255, 255], 2.0, None, 12.0, None),
            (
                T::ElliottImpulse,
                [61, 133, 198, 255],
                2.0,
                None,
                14.0,
                None,
            ),
            (
                T::ElliottCorrection,
                [61, 133, 198, 255],
                2.0,
                None,
                14.0,
                None,
            ),
            (
                T::ElliottTriangle,
                [255, 152, 0, 255],
                2.0,
                None,
                14.0,
                None,
            ),
            (
                T::ElliottDoubleCombo,
                [106, 168, 79, 255],
                2.0,
                None,
                14.0,
                None,
            ),
            (
                T::ElliottTripleCombo,
                [106, 168, 79, 255],
                2.0,
                None,
                14.0,
                None,
            ),
            (
                T::RegressionTrend,
                [242, 54, 69, 77],
                1.0,
                Some([41, 98, 255, 51]),
                14.0,
                Some([242, 54, 69, 255]),
            ),
            (
                T::Triangle,
                [8, 153, 129, 255],
                2.0,
                Some([8, 153, 129, 51]),
                14.0,
                None,
            ),
            (
                T::Ellipse,
                [242, 54, 69, 255],
                2.0,
                Some([242, 54, 69, 51]),
                14.0,
                Some([242, 54, 69, 255]),
            ),
            (T::DoubleCurve, [103, 58, 183, 255], 2.0, None, 14.0, None),
        ];
        let wrong: Vec<String> = rows
            .into_iter()
            .filter_map(|want| {
                let d = Drawing::new(1, want.0);
                let got = (
                    want.0,
                    d.color,
                    d.stroke_width,
                    d.fill_color,
                    d.font_size,
                    d.text_color,
                );
                (got != want).then(|| format!("got {got:?}, want {want:?}"))
            })
            .collect();
        assert!(wrong.is_empty(), "{}", wrong.join("\n"));
        for tool in [
            T::ElliottImpulse,
            T::ElliottCorrection,
            T::ElliottTriangle,
            T::ElliottDoubleCombo,
            T::ElliottTripleCombo,
        ] {
            assert_eq!(
                Drawing::new(1, tool).elliott_degree,
                ElliottDegree::Intermediate,
                "{tool:?}"
            );
        }
        assert_eq!(Drawing::new(1, T::PriceLabel).font_weight, FontWeight::Bold);
        let placed = Drawing::new(1, T::DateAndPriceRange);
        assert_eq!(
            (
                placed.range_settings.border,
                placed.range_settings.border_width
            ),
            (false, Some(1.0))
        );
        assert_eq!(placed.range_settings.border_color, None);
        let mut saved = serde_json::to_value(&placed).unwrap();
        saved.as_object_mut().unwrap().remove("range_settings");
        let read: Drawing = serde_json::from_value(saved).unwrap();
        assert!(read.range_settings.border);
        assert_eq!(read.range_settings.border_width, None);
        // Channels write their text from the left end of their first line; a channel saved
        // without line measurements keeps it in the middle.
        use crate::drawings::LineTextHorizontalAlign as H;
        for tool in [T::ParallelChannel, T::FlatTopBottom, T::DisjointChannel] {
            let placed = Drawing::new(1, tool);
            let mut saved = serde_json::to_value(&placed).unwrap();
            saved.as_object_mut().unwrap().remove("line_measurements");
            let read: Drawing = serde_json::from_value(saved).unwrap();
            assert_eq!(
                (
                    placed.line_measurements.text_horizontal_align,
                    read.line_measurements.text_horizontal_align
                ),
                (H::Left, H::Center),
                "{tool:?}"
            );
        }
        let config = Drawing::new(1, T::RegressionTrend)
            .regression_trend
            .unwrap();
        for band in [config.upper_line, config.lower_line] {
            assert_eq!(
                (band.visible, band.color, band.width(), band.line_style),
                (true, [41, 98, 255, 77], Some(2.0), LineStyle::Solid)
            );
        }
        assert_eq!(
            (config.line_style, config.show_base_line),
            (LineStyle::Dashed, true)
        );
        for tool in [T::LongPos, T::ShortPos] {
            let d = Drawing::new(1, tool);
            assert_eq!(
                (
                    d.position_size,
                    d.position_sizing.lot_size,
                    d.position_target_color,
                    d.position_stop_color
                ),
                (
                    Some(PositionSize::RiskPercent {
                        account_size: 1_000.0,
                        percent: 25.0
                    }),
                    1.0,
                    Some([8, 153, 129, 51]),
                    Some([242, 54, 69, 51])
                ),
                "{tool:?}"
            );
        }
    }

    /// Text and Anchored text keep the first positive bar spacing they are shown at and are drawn
    /// at the ratio of the current spacing to it; other tools record none.
    #[test]
    fn text_scales_by_the_bar_spacing_it_was_sized_at() {
        let mut text = Drawing::new(1, DrawingToolType::AnchoredText);
        text.chart_points = vec![ChartPoint::new(10.0, 1.0)];
        // A spacing that saving would reject is never recorded.
        for invalid in [0.0, -3.0, f32::NAN, f32::INFINITY] {
            text.adopt_text_zoom_spacing(invalid);
            assert_eq!(text.text_zoom_spacing, None, "{invalid}");
        }
        text.adopt_text_zoom_spacing(3.0);
        text.adopt_text_zoom_spacing(6.0);
        assert_eq!(text.text_zoom_spacing, Some(3.0));
        assert_eq!(text.text_zoom(6.0), 1.0);

        text.text_follows_zoom = true;
        assert_eq!(text.text_zoom(6.0), 2.0);
        assert_eq!(text.text_zoom(f32::NAN), 1.0);
        assert_eq!(text.text_zoom(0.0), 1.0);

        let mut line = Drawing::new(2, DrawingToolType::TrendLine);
        line.chart_points = vec![ChartPoint::new(10.0, 1.0)];
        line.adopt_text_zoom_spacing(3.0);
        assert_eq!(line.text_zoom_spacing, None);
    }

    #[test]
    fn test_shift_bar_indices() {
        let mut drawing = Drawing::new(1, DrawingToolType::TrendLine);
        drawing.chart_points.push(ChartPoint::new(100.0, 150.0));
        drawing.chart_points.push(ChartPoint::new(110.0, 160.0));

        drawing.shift_bar_indices(105.0, 50.0);

        assert_eq!(
            drawing.chart_points,
            [ChartPoint::new(100.0, 150.0), ChartPoint::new(160.0, 160.0)]
        );
    }

    /// A drawing completes once it holds its tool's required number of
    /// anchors, and each anchor added from the screen gets its chart
    /// coordinates.
    #[test]
    fn test_completion() {
        let x_to_bar = |x: f32| x / 10.0;
        let y_to_price = |y: f32| 1000.0 - y as f64;
        let mut drawing = Drawing::new(1, DrawingToolType::TrendLine);

        drawing.add_point_with_chart_coords(egui::pos2(100.0, 300.0), x_to_bar, y_to_price);
        assert!(!drawing.completed);

        drawing.add_point_with_chart_coords(egui::pos2(200.0, 50.0), x_to_bar, y_to_price);
        assert!(drawing.completed); // TrendLine requires 2 points
        assert_eq!(
            drawing.chart_points,
            [ChartPoint::new(10.0, 700.0), ChartPoint::new(20.0, 950.0)]
        );
    }

    /// Alerts follow each level the drawing shows at the evaluated bar, under a source that
    /// names the same level once the drawing is stored.
    #[test]
    fn alert_levels_follow_the_drawing_geometry() {
        use super::super::{FibonacciLevel, GannLevel};
        use DrawingToolType as T;
        use PriceScaleMode::{Logarithmic, Normal as Linear};
        let drawing = |tool, points: &[(f32, f64)]| {
            let mut drawing = Drawing::new(7, tool);
            drawing.chart_points = points.iter().map(|&(b, p)| ChartPoint::new(b, p)).collect();
            drawing
        };
        let mut fib = drawing(T::FibonacciRetracement, &[(0.0, 100.0), (10.0, 200.0)]);
        let mut config = FibonacciConfig {
            levels_as_percent: true,
            ..FibonacciConfig::legacy()
        };
        config.levels[1].visible = false;
        config.levels[2].label = "100%".into();
        config.levels[3].label.clear();
        fib.fib_config = Some(config);
        let channel = |points: &[(f32, f64)]| {
            let mut d = drawing(T::FibonacciChannel, points);
            let mut config = FibonacciConfig::legacy();
            config.levels[1].visible = false;
            d.fib_config = Some(config);
            d
        };
        let fan = |points: &[(f32, f64)], time: Option<Vec<f32>>, reverse| {
            let mut d = drawing(T::FibonacciSpeedFan, points);
            let levels = |values: Vec<f32>| {
                values
                    .into_iter()
                    .map(|value| FibonacciLevel::new(value, "", [0; 4]))
                    .collect()
            };
            d.fib_config = Some(FibonacciConfig {
                levels: levels(vec![0.0, 0.5, 1.0]),
                time_levels: time.map(levels),
                levels_as_percent: true,
                reverse,
                ..FibonacciConfig::legacy()
            });
            d
        };
        let gann = |points: &[(f32, f64)]| {
            let mut d = drawing(T::GannFan, points);
            let mut config = GannConfig::for_tool(T::GannFan);
            config.fan_ratios = [2.0, 1.0, 0.5, 0.0]
                .map(|v| GannLevel::new(v, [0; 4]))
                .to_vec();
            config.fan_ratios[1].visible = false;
            d.gann = Some(config);
            d
        };
        // (drawing, bar) -> (level, name, price); prices by hand.
        for (drawing, bar, scale, expected) in [
            (
                channel(&[(10.0, 100.0), (20.0, 120.0), (14.0, 130.0)]),
                15.0,
                Linear,
                vec![
                    (0, "0", 110.0),
                    (2, "0.382", 118.404),
                    (3, "0.5", 121.0),
                    (4, "0.618", 123.596),
                    (5, "0.786", 127.292),
                    (6, "1", 132.0),
                ],
            ),
            (
                channel(&[(10.0, 100.0), (20.0, 120.0), (14.0, 130.0)]),
                18.0,
                Linear,
                vec![
                    (0, "0", 116.0),
                    (2, "0.382", 124.404),
                    (3, "0.5", 127.0),
                    (4, "0.618", 129.596),
                    (5, "0.786", 133.292),
                    (6, "1", 138.0),
                ],
            ),
            (
                channel(&[(10.0, 100.0), (20.0, 400.0), (10.0, 200.0)]),
                15.0,
                Logarithmic,
                vec![
                    (0, "0", 200.0 * 2f64.powf(0.0)),
                    (2, "0.382", 200.0 * 2f64.powf(0.382)),
                    (3, "0.5", 200.0 * 2f64.powf(0.5)),
                    (4, "0.618", 200.0 * 2f64.powf(0.618)),
                    (5, "0.786", 200.0 * 2f64.powf(0.786)),
                    (6, "1", 200.0 * 2f64.powf(1.0)),
                ],
            ),
            (
                channel(&[(10.0, 100.0), (20.0, 400.0), (10.0, 200.0)]),
                18.0,
                Logarithmic,
                vec![
                    (0, "0", 100.0 * 4f64.powf(0.8) * 2f64.powf(0.0)),
                    (2, "0.382", 100.0 * 4f64.powf(0.8) * 2f64.powf(0.382)),
                    (3, "0.5", 100.0 * 4f64.powf(0.8) * 2f64.powf(0.5)),
                    (4, "0.618", 100.0 * 4f64.powf(0.8) * 2f64.powf(0.618)),
                    (5, "0.786", 100.0 * 4f64.powf(0.8) * 2f64.powf(0.786)),
                    (6, "1", 100.0 * 4f64.powf(0.8) * 2f64.powf(1.0)),
                ],
            ),
            (
                channel(&[(10.0, 100.0), (20.0, 400.0), (10.0, 200.0)]),
                15.0,
                Linear,
                vec![
                    (0, "0", 250.0),
                    (2, "0.382", 288.2),
                    (3, "0.5", 300.0),
                    (4, "0.618", 311.8),
                    (5, "0.786", 328.6),
                    (6, "1", 350.0),
                ],
            ),
            (
                channel(&[(10.0, 100.0), (20.0, 120.0)]),
                15.0,
                Linear,
                vec![],
            ),
            (
                channel(&[(10.0, 100.0), (10.0, 120.0), (14.0, 130.0)]),
                15.0,
                Linear,
                vec![],
            ),
            (
                fan(&[(10.0, 100.0), (20.0, 200.0)], None, false),
                15.0,
                Linear,
                vec![
                    (0, "0%: price level", 150.0),
                    (2, "50%: price level", 125.0),
                    (3, "50%: time level", 200.0),
                    (4, "100%: price level", 100.0),
                ],
            ),
            (
                fan(&[(10.0, 100.0), (20.0, 200.0)], None, false),
                20.0,
                Linear,
                vec![
                    (0, "0%: price level", 200.0),
                    (2, "50%: price level", 150.0),
                    (3, "50%: time level", 300.0),
                    (4, "100%: price level", 100.0),
                ],
            ),
            (
                fan(&[(10.0, 100.0), (20.0, 200.0)], None, false),
                5.0,
                Linear,
                vec![],
            ),
            (
                fan(&[(10.0, 100.0), (20.0, 400.0)], None, false),
                15.0,
                Logarithmic,
                vec![
                    (0, "0%: price level", 200.0),
                    (2, "50%: price level", 100.0 * 2f64.sqrt()),
                    (3, "50%: time level", 400.0),
                    (4, "100%: price level", 100.0),
                ],
            ),
            (
                fan(&[(10.0, 100.0), (20.0, 400.0)], None, false),
                20.0,
                Logarithmic,
                vec![
                    (0, "0%: price level", 400.0),
                    (2, "50%: price level", 200.0),
                    (3, "50%: time level", 1600.0),
                    (4, "100%: price level", 100.0),
                ],
            ),
            (
                fan(
                    &[(10.0, 100.0), (20.0, 200.0)],
                    Some(vec![0.0, 0.5, 2.0]),
                    true,
                ),
                15.0,
                Linear,
                vec![
                    (0, "0%: price level", 150.0),
                    (1, "0%: time level", 150.0),
                    (2, "50%: price level", 175.0),
                    (3, "50%: time level", 100.0),
                    (4, "100%: price level", 200.0),
                ],
            ),
            (
                fan(
                    &[(10.0, 100.0), (20.0, 200.0)],
                    Some(vec![0.0, 0.5, 2.0]),
                    true,
                ),
                5.0,
                Linear,
                vec![(5, "200%: time level", 150.0)],
            ),
            (
                fan(&[(10.0, 100.0), (10.0, 200.0)], None, false),
                15.0,
                Linear,
                vec![(4, "100%: price level", 100.0)],
            ),
            (
                gann(&[(10.0, 100.0), (20.0, 120.0)]),
                15.0,
                Linear,
                vec![(0, "2×1", 120.0), (2, "1×2", 105.0)],
            ),
            (
                gann(&[(10.0, 100.0), (20.0, 120.0)]),
                20.0,
                Linear,
                vec![(0, "2×1", 140.0), (2, "1×2", 110.0)],
            ),
            (gann(&[(10.0, 100.0), (20.0, 120.0)]), 5.0, Linear, vec![]),
            (
                gann(&[(20.0, 100.0), (10.0, 120.0)]),
                15.0,
                Linear,
                vec![(0, "2×1", 120.0), (2, "1×2", 105.0)],
            ),
            (gann(&[(20.0, 100.0), (10.0, 120.0)]), 25.0, Linear, vec![]),
            (
                gann(&[(10.0, 100.0), (20.0, 400.0)]),
                15.0,
                Logarithmic,
                vec![(0, "2×1", 400.0), (2, "1×2", 100.0 * 2f64.sqrt())],
            ),
            (
                gann(&[(10.0, 100.0), (20.0, 400.0)]),
                20.0,
                Logarithmic,
                vec![(0, "2×1", 1600.0), (2, "1×2", 200.0)],
            ),
            (gann(&[(10.0, 100.0), (10.0, 120.0)]), 15.0, Linear, vec![]),
            (
                gann(&[(10.0, 0.0), (20.0, 10.0)]),
                15.0,
                Logarithmic,
                vec![],
            ),
            // 100 + (25 - 10) * (120 - 100) / (20 - 10).
            (
                drawing(T::TrendLine, &[(10.0, 100.0), (20.0, 120.0)]),
                25.0,
                Linear,
                vec![(0, "Line", 130.0)],
            ),
            (
                drawing(T::TrendLine, &[(10.0, 100.0), (10.0, 120.0)]),
                25.0,
                Linear,
                vec![],
            ),
            (
                drawing(T::HorizontalRay, &[(3.0, 50.0)]),
                9.0,
                Linear,
                vec![(0, "Line", 50.0)],
            ),
            (
                drawing(T::Rect, &[(5.0, 90.0), (8.0, 110.0)]),
                9.0,
                Linear,
                vec![(0, "Top", 110.0), (1, "Bottom", 90.0)],
            ),
            (
                drawing(T::LongPos, &[(5.0, 100.0), (9.0, 110.0)]),
                9.0,
                Linear,
                vec![(0, "Entry", 100.0), (1, "Target", 110.0), (2, "Stop", 90.0)],
            ),
            (
                drawing(T::ShortPos, &[(5.0, 100.0), (9.0, 90.0), (9.0, 104.0)]),
                9.0,
                Linear,
                vec![(0, "Entry", 100.0), (1, "Target", 90.0), (2, "Stop", 104.0)],
            ),
            // Level 0 is on the second anchor; the hidden 23.6% level keeps the numbers after it.
            (
                fib,
                9.0,
                Linear,
                vec![
                    (0, "0%", 200.0),
                    (2, "38.2%", 161.8),
                    (3, "50%", 150.0),
                    (4, "61.8%", 138.2),
                    (5, "78.6%", 121.4),
                    (6, "100%", 100.0),
                ],
            ),
            // Straight on the scale: the geometric mean of 100 and 400 halfway on a log scale.
            (
                drawing(T::TrendLine, &[(10.0, 100.0), (20.0, 400.0)]),
                15.0,
                Logarithmic,
                vec![(0, "Line", 200.0)],
            ),
            (
                drawing(T::TrendLine, &[(10.0, 100.0), (20.0, 400.0)]),
                15.0,
                Linear,
                vec![(0, "Line", 250.0)],
            ),
            (
                drawing(T::TrendLine, &[(10.0, 0.0), (20.0, 10.0)]),
                15.0,
                Logarithmic,
                vec![],
            ),
            // Before the anchor at zero, and beside a vertical base, the log line has no price.
            (
                drawing(T::TrendLine, &[(10.0, 0.0), (20.0, 10.0)]),
                5.0,
                Logarithmic,
                vec![],
            ),
            (
                channel(&[(10.0, 100.0), (10.0, 120.0), (14.0, 130.0)]),
                5.0,
                Logarithmic,
                vec![],
            ),
            // The base line is 110 at the third anchor's bar 15, 20 below it: 130 + 20 at bar 25.
            // On a log scale the third anchor is twice the base line there, so the parallel is
            // twice as high at every bar: 400 and 800 at bar 20.
            (
                drawing(
                    T::ParallelChannel,
                    &[(10.0, 100.0), (20.0, 120.0), (15.0, 130.0)],
                ),
                25.0,
                Linear,
                vec![(0, "Line", 130.0), (1, "Parallel line", 150.0)],
            ),
            (
                drawing(
                    T::ParallelChannel,
                    &[(10.0, 100.0), (20.0, 400.0), (15.0, 400.0)],
                ),
                20.0,
                Logarithmic,
                vec![(0, "Line", 400.0), (1, "Parallel line", 800.0)],
            ),
            (
                drawing(
                    T::ParallelChannel,
                    &[(10.0, 100.0), (10.0, 120.0), (15.0, 130.0)],
                ),
                25.0,
                Linear,
                vec![],
            ),
            (
                drawing(
                    T::FlatTopBottom,
                    &[(10.0, 100.0), (20.0, 120.0), (15.0, 90.0)],
                ),
                25.0,
                Linear,
                vec![(0, "Line", 130.0), (1, "Flat line", 90.0)],
            ),
            // Through 50 at bar 12, the mirrored line falls 2 per bar: 24 at bar 25.
            // In log price it divides by 4 over 10 bars: 12.5 at bar 22.
            (
                drawing(
                    T::DisjointChannel,
                    &[(10.0, 100.0), (20.0, 120.0), (12.0, 50.0)],
                ),
                25.0,
                Linear,
                vec![(0, "Line", 130.0), (1, "Second line", 24.0)],
            ),
            (
                drawing(
                    T::DisjointChannel,
                    &[(10.0, 100.0), (20.0, 400.0), (12.0, 50.0)],
                ),
                22.0,
                Logarithmic,
                vec![
                    (0, "Line", 100.0 * 4f64.powf(1.2)),
                    (1, "Second line", 12.5),
                ],
            ),
            (
                drawing(T::Brush, &[(1.0, 1.0), (2.0, 2.0)]),
                9.0,
                Linear,
                vec![],
            ),
        ] {
            let levels = drawing.alert_levels(bar, scale);
            // Compare the hand prices with the same screen segments the renderer paints.
            if !expected.is_empty()
                && matches!(
                    drawing.tool_type,
                    T::FibonacciChannel | T::FibonacciSpeedFan | T::GannFan
                )
            {
                let mut painted = drawing.clone();
                let x = |bar: f32| 100.0 + 10.0 * bar;
                let y = |price: f64| {
                    (1000.0
                        - if scale == Logarithmic {
                            100.0 * price.ln()
                        } else {
                            price
                        }) as f32
                };
                painted.update_screen_coords(x, y);
                let rect = egui::Rect::from_min_max(egui::pos2(-1e4, -1e4), egui::pos2(1e4, 1e4));
                let lines = if painted.tool_type == T::FibonacciChannel {
                    painted.fib_channel_level_lines(rect).unwrap().1
                } else {
                    painted.outline(rect).unwrap().lines
                };
                for (_, name, price) in &levels {
                    let px = x(bar as f32);
                    assert!(
                        lines.iter().any(|[a, b]| {
                            a.x != b.x
                                && px >= a.x.min(b.x)
                                && px <= a.x.max(b.x)
                                && (a.y + (px - a.x) * (b.y - a.y) / (b.x - a.x) - y(*price)).abs()
                                    < 0.01
                        }),
                        "{:?} {name} at {bar} is off its painted line",
                        drawing.tool_type
                    );
                }
            }
            assert_eq!(levels.len(), expected.len(), "{:?}", drawing.tool_type);
            for ((level, name, price), (want_level, want_name, want_price)) in
                levels.into_iter().zip(expected)
            {
                assert_eq!((level, name.as_str()), (want_level, want_name));
                // Fibonacci ratios are stored as `f32`, good to about 1e-5 at these prices.
                assert!((price - want_price).abs() < 1e-4, "{name}: {price}");
            }
        }

        let mut line = drawing(T::TrendLine, &[(10.0, 100.0), (20.0, 120.0)]);
        for (stored, level, source) in [
            (None, 0, "7"),
            (None, 2, "7#2"),
            (Some("a%231"), 0, "a%25231"),
            (Some("a#1"), 3, "a%231#3"),
        ] {
            line.stored_id = stored.map(Into::into);
            assert_eq!(
                line.alert_source(level),
                AlertSource::Drawing(source.into())
            );
            assert_eq!(line.alert_level(&line.alert_source(level)), Some(level));
        }
        for other in ["a", "a#1#0", "a#1#x", "7"] {
            assert_eq!(line.alert_level(&AlertSource::Drawing(other.into())), None);
        }
        assert_eq!(line.alert_level(&AlertSource::Price), None);

        // A stored id containing the level separator must not alias another drawing's level.
        let mut other = line.clone();
        other.stored_id = Some("a".into());
        assert_ne!(line.alert_source(0), other.alert_source(1));
    }

    #[test]
    fn test_bounding_rect() {
        let mut drawing = Drawing::new(1, DrawingToolType::TrendLine);
        assert!(drawing.bounding_rect().is_none());

        drawing.points.push(egui::pos2(10.0, 20.0));
        drawing.points.push(egui::pos2(30.0, 50.0));

        let rect = drawing.bounding_rect().unwrap();
        assert_eq!(rect.min, egui::pos2(10.0, 20.0));
        assert_eq!(rect.max, egui::pos2(30.0, 50.0));
    }
}
