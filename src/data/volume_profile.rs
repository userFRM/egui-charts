//! Price-row volume distributions shared by studies and drawings.

use super::FootprintBar;
use crate::model::Bar;
use serde::{Deserialize, Serialize};
use std::ops::{Range, RangeInclusive};
use std::sync::Arc;

/// One price-row profile placed over a half-open span of chart bars. Exhaustive, so that a custom
/// study can build it in `studies::Indicator::volume_profiles`.
///
/// Build a profile beside its bars, as a custom study can return from
/// `Indicator::volume_profiles`. Set `strip: Some(0.3)` with `right_aligned: true` to
/// place it in the plot's rightmost 30% instead. A study that writes row values keeps the
/// volume decimals of the instrument `Indicator::set_instrument` gives it.
///
/// ```
/// use egui_charts::data::{StudyProfile, VolumeProfile, VolumeProfileConfig};
/// use egui_charts::model::Bar;
/// use std::sync::Arc;
///
/// let bars = [Bar::new(chrono::DateTime::UNIX_EPOCH, 10.0, 12.0, 9.0, 11.0, 100.0)];
/// let config = VolumeProfileConfig::default();
/// let profile = StudyProfile {
///     bars: 0..bars.len(),
///     profile: Arc::new(VolumeProfile::from_bars(&bars, config).unwrap()),
///     config,
///     color: [255, 255, 255, 255],
///     right_aligned: false,
///     strip: None,
///     width: 0.8,
///     volume_decimals: None,
/// };
/// assert_eq!(profile.bars, 0..1);
/// assert_eq!(profile.strip, None);
/// assert!(!profile.right_aligned);
/// assert_eq!(profile.width, 0.8);
/// let edge_profile = StudyProfile { right_aligned: true, strip: Some(0.3), ..profile };
/// assert!(edge_profile.right_aligned);
/// assert_eq!(edge_profile.strip, Some(0.3));
/// ```
#[derive(Clone, Debug)]
pub struct StudyProfile {
    /// Full source span for bar profiles, including offscreen bars in fixed periods,
    /// or the visible span for an options overlay.
    pub bars: Range<usize>,
    /// Immutable row volumes and price boundaries.
    pub profile: Arc<VolumeProfile>,
    /// Row layout, value area and display settings.
    pub config: VolumeProfileConfig,
    /// Colour of the point of control and value-area lines, `Color32::PLACEHOLDER` as bytes
    /// following the theme's foreground.
    pub color: [u8; 4],
    /// Grow rows from the right edge of their span instead of its left edge.
    pub right_aligned: bool,
    /// Share of the plot width for an edge strip, right with `right_aligned`, left otherwise.
    /// `None` places the profile beside its source bars. A strip over bars carries its point
    /// of control across the plot; an options strip measures strikes instead.
    pub strip: Option<f32>,
    /// Share of the span, from 0 to 1, that the row with the most volume takes. An options
    /// exposure overlay's largest bar takes half of it on each side of its centre.
    pub width: f32,
    /// Decimals of the row values that [`VolumeProfileConfig::show_values`] writes, abbreviated
    /// K, M and B values included, as
    /// [`Instrument::volume_decimals`](crate::model::Instrument::volume_decimals) sets them;
    /// `None` writes two decimals.
    pub volume_decimals: Option<u8>,
}

/// A level of a finished study profile carried right past the profile's bars: from the bar
/// after its last to the first later bar that trades in the level's row, or on the line for the
/// value area high, or to the last bar. Prices fall in rows as the profile binned its volume.
/// Exhaustive, so that a custom study can build it in `studies::Indicator::profile_levels`.
#[derive(Clone, Debug, PartialEq)]
pub struct ProfileLevel {
    /// Chart bars the line spans. It starts at the left edge of the first, which follows the
    /// profile, and ends at the centre of the last: the bar that trades in the level's row, or
    /// on the line for the value area high, or the last bar when none has.
    pub bars: Range<usize>,
    /// Price of the line: the middle of the point of control row, or the top or bottom edge of
    /// the value area.
    pub price: f64,
    /// Which level of the profile this is.
    pub kind: ProfileLevelKind,
    /// Colour of the profile's point of control, `Color32::PLACEHOLDER` as bytes following the
    /// theme's foreground. The value area high and low are drawn at the opacity of the profile's
    /// value area lines.
    pub color: [u8; 4],
}

/// A level of a volume profile.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProfileLevelKind {
    /// The middle of the row with the most volume.
    PointOfControl,
    /// The top edge of the value area.
    ValueAreaHigh,
    /// The bottom edge of the value area.
    ValueAreaLow,
}

/// Volume shown in each price row of a profile built from bars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ProfileDisplay {
    /// Total volume, with the point of control and value area highlighted.
    #[default]
    Total,
    /// Up-bar and down-bar volume in adjacent segments. A bar that closes at or above its open
    /// is an up bar, except in a fixed range or anchored volume profile, drawing or study,
    /// estimated from a single chart bar, where it is up only when it closes above its open.
    UpDown,
    /// Absolute up minus down volume, coloured by its sign.
    Delta,
}

/// Side of its range a fixed range volume profile drawing's rows grow from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub enum ProfilePlacement {
    /// From the range's left edge, growing right.
    #[default]
    Left,
    /// From the range's right edge, growing left.
    Right,
}

fn profile_up_color() -> [u8; 4] {
    [38, 166, 154, 180]
}
fn profile_down_color() -> [u8; 4] {
    [239, 83, 80, 180]
}

fn profile_shown() -> bool {
    true
}
fn histogram_width() -> u8 {
    30
}
fn profile_line_colors() -> [[u8; 4]; 2] {
    [ecolor::Color32::PLACEHOLDER.to_srgba_unmultiplied(); 2]
}
fn developing_poc_color() -> [u8; 4] {
    [255, 152, 0, 255]
}
fn developing_value_area_colors() -> [[u8; 4]; 2] {
    [[33, 150, 243, 255]; 2]
}

/// Row layout and display settings shared by volume profile studies and drawings.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(try_from = "VolumeProfileFields")]
pub struct VolumeProfileConfig {
    row_size: Option<f64>,
    row_ticks: Option<usize>,
    rows: usize,
    value_area: f64,
    /// Volume displayed in each row. Totals still determine POC and value area.
    pub display: ProfileDisplay,
    /// Colour of up volume in split and delta modes, as [`ProfileDisplay::UpDown`] counts it.
    pub up_color: [u8; 4],
    /// Colour of falling bars in split and delta modes.
    pub down_color: [u8; 4],
    /// Optional colours inside the value area; absent uses the ordinary side colours.
    pub value_area_colors: Option<[[u8; 4]; 2]>,
    /// Draw the high and low boundaries of the value area.
    pub show_value_area_lines: bool,
    /// Draw the price rows; hiding them leaves the profile's levels.
    pub show_rows: bool,
    /// Side of a fixed range drawing's span its rows grow from, Left by default and in older
    /// saves. Extend right moves the span's right edge to the last bar. Anchored drawings keep
    /// rows at the pane's right edge; studies use their Placement input. Both ignore this.
    pub placement: ProfilePlacement,
    /// Widest drawing row as a whole percent of its range, or of the pane when anchored.
    /// Defaults to 30, including older saves, and is drawn between 1 and 100. Studies use
    /// their Width input and ignore this.
    pub histogram_width: u8,
    /// Write each row's volume beside it, following `display`, in [`Self::values_color`], or
    /// inside the value area in [`Self::value_area_values_color`].
    pub show_values: bool,
    /// Colour of the row values that [`Self::show_values`] writes, unmultiplied RGBA; `None`
    /// uses the theme's foreground.
    pub values_color: Option<[u8; 4]>,
    /// Colour of the row values inside the value area, unmultiplied RGBA; `None` writes them in
    /// [`Self::values_color`], as profiles saved before it did. Volume profile drawings offer it;
    /// studies leave it unset.
    pub value_area_values_color: Option<[u8; 4]>,
    /// Draw the point of control line.
    pub show_poc: bool,
    /// Colour of the point of control line, unmultiplied RGBA. `None` draws a volume profile
    /// drawing's in the theme's foreground and a study's in the study's colour.
    pub poc_color: Option<[u8; 4]>,
    /// Draw the value area high even without `show_value_area_lines`.
    pub show_value_area_high: bool,
    /// Draw the value area low even without `show_value_area_lines`.
    pub show_value_area_low: bool,
    /// High and low line colours; placeholder bytes take the profile colour at alpha 200.
    pub value_area_line_colors: [[u8; 4]; 2],
    /// Fill the price and time range of a volume profile drawing behind its rows. On by
    /// default. Studies draw no box and ignore this.
    pub show_box: bool,
    /// Fill of a volume profile drawing's box, unmultiplied RGBA whose alpha is the box's
    /// opacity; `None` takes the drawing's colour at alpha 15. Studies ignore this.
    pub box_color: Option<[u8; 4]>,
    /// Keep a fixed range drawing growing to the last bar, past its second anchor.
    pub extend_right: bool,
    /// Draw a volume profile drawing's developing point of control: a step line through its bars
    /// at the point of control of the bars up to each one, in the rows the drawing draws. Volume
    /// profile studies draw theirs as plots and ignore this. Off by default.
    pub show_developing_poc: bool,
    /// Draw a volume profile drawing's developing value area: step lines at the high and low of
    /// the value area of the bars up to each one. Volume profile studies ignore this. Off by
    /// default.
    pub show_developing_value_area: bool,
    /// Colour of the developing point of control line, unmultiplied RGBA; orange `#FF9800` by
    /// default.
    pub developing_poc_color: [u8; 4],
    /// High and low colours of the developing value area lines, unmultiplied RGBA; blue `#2196F3`
    /// by default.
    pub developing_value_area_colors: [[u8; 4]; 2],
    /// Paint study rows in their colours' own opacity beneath price bars. Levels and values
    /// stay above them. Off by default; volume profile drawings ignore this.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub exact_row_opacity: bool,
}

/// Stored settings of a volume profile, checked by [`VolumeProfileConfig::new`] on load.
#[derive(Deserialize)]
struct VolumeProfileFields {
    #[serde(default)]
    row_size: Option<f64>,
    #[serde(default)]
    row_ticks: Option<usize>,
    rows: usize,
    value_area: f64,
    #[serde(default)]
    display: ProfileDisplay,
    #[serde(default = "profile_up_color")]
    up_color: [u8; 4],
    #[serde(default = "profile_down_color")]
    down_color: [u8; 4],
    #[serde(default)]
    value_area_colors: Option<[[u8; 4]; 2]>,
    #[serde(default)]
    show_value_area_lines: bool,
    #[serde(default = "profile_shown")]
    show_rows: bool,
    #[serde(default)]
    placement: ProfilePlacement,
    #[serde(default = "histogram_width")]
    histogram_width: u8,
    #[serde(default)]
    show_values: bool,
    #[serde(default)]
    values_color: Option<[u8; 4]>,
    #[serde(default)]
    value_area_values_color: Option<[u8; 4]>,
    #[serde(default = "profile_shown")]
    show_poc: bool,
    #[serde(default)]
    poc_color: Option<[u8; 4]>,
    #[serde(default)]
    show_value_area_high: bool,
    #[serde(default)]
    show_value_area_low: bool,
    #[serde(default = "profile_line_colors")]
    value_area_line_colors: [[u8; 4]; 2],
    #[serde(default = "profile_shown")]
    show_box: bool,
    #[serde(default)]
    box_color: Option<[u8; 4]>,
    #[serde(default)]
    extend_right: bool,
    #[serde(default)]
    show_developing_poc: bool,
    #[serde(default)]
    show_developing_value_area: bool,
    #[serde(default = "developing_poc_color")]
    developing_poc_color: [u8; 4],
    #[serde(default = "developing_value_area_colors")]
    developing_value_area_colors: [[u8; 4]; 2],
    #[serde(default)]
    exact_row_opacity: bool,
}

impl TryFrom<VolumeProfileFields> for VolumeProfileConfig {
    type Error = &'static str;

    fn try_from(fields: VolumeProfileFields) -> Result<Self, Self::Error> {
        let mut config = Self::new(fields.rows, fields.value_area)
            .ok_or("volume profile settings out of bounds")?;
        config.display = fields.display;
        config.up_color = fields.up_color;
        config.down_color = fields.down_color;
        config.value_area_colors = fields.value_area_colors;
        config.show_value_area_lines = fields.show_value_area_lines;
        config.show_rows = fields.show_rows;
        config.placement = fields.placement;
        config.histogram_width = fields.histogram_width;
        config.show_values = fields.show_values;
        config.values_color = fields.values_color;
        config.value_area_values_color = fields.value_area_values_color;
        config.show_poc = fields.show_poc;
        config.poc_color = fields.poc_color;
        config.show_value_area_high = fields.show_value_area_high;
        config.show_value_area_low = fields.show_value_area_low;
        config.value_area_line_colors = fields.value_area_line_colors;
        config.show_box = fields.show_box;
        config.box_color = fields.box_color;
        config.extend_right = fields.extend_right;
        config.show_developing_poc = fields.show_developing_poc;
        config.show_developing_value_area = fields.show_developing_value_area;
        config.developing_poc_color = fields.developing_poc_color;
        config.developing_value_area_colors = fields.developing_value_area_colors;
        config.exact_row_opacity = fields.exact_row_opacity;
        config
            .with_row_size(fields.row_size)
            .ok_or("volume profile row height must be finite and positive")?
            .with_row_ticks(fields.row_ticks)
            .ok_or("volume profile ticks per row out of bounds")
    }
}

impl VolumeProfileConfig {
    /// Price rows a profile may have.
    pub const ROWS: RangeInclusive<usize> = 1..=1000;

    /// Ticks per row a volume profile drawing may have, as the studies' Height in ticks input.
    pub const ROW_TICKS: RangeInclusive<usize> = 1..=100_000;

    /// A profile of `rows` equal price rows whose value area holds the
    /// `value_area` share of its volume (above 0, at most 1); `None` when
    /// either is out of bounds. A volume profile drawing on an instrument with a tick size draws
    /// the whole number of ticks per row whose row total lies closest to `rows`, as the volume
    /// profile studies do with Row count; [`VolumeProfile::from_bars`] splits the span into exactly
    /// `rows`.
    pub fn new(rows: usize, value_area: f64) -> Option<Self> {
        (Self::ROWS.contains(&rows) && value_area > 0.0 && value_area <= 1.0).then_some(Self {
            rows,
            value_area,
            ..Self::default()
        })
    }

    /// Fix each row to a price height, aligned to multiples of that height.
    /// `None` restores equal rows over the span. Rejects nonpositive or nonfinite heights.
    /// Profiles spanning more than 10,000 fixed rows are not drawn. Clears [`Self::row_ticks`].
    pub fn with_row_size(mut self, height: Option<f64>) -> Option<Self> {
        if height.is_some_and(|h| !h.is_finite() || h <= 0.0) {
            return None;
        }
        self.row_size = height;
        self.row_ticks = None;
        Some(self)
    }

    /// Fixed price height per row, or equal rows over the span when absent.
    pub fn row_size(&self) -> Option<f64> {
        self.row_size
    }

    /// Rows `ticks` ticks of the chart instrument high, on multiples of that height, for a
    /// volume profile drawing, which on an instrument without a tick size draws [`Self::rows`]
    /// equal rows instead. [`VolumeProfile::from_bars`] and the volume profile studies, which
    /// size rows in ticks through their own inputs, use the row count too. Clears
    /// [`Self::row_size`]; `None` leaves the row count or price height. Rejects a count outside
    /// [`Self::ROW_TICKS`].
    ///
    /// ```
    /// use egui_charts::data::VolumeProfileConfig;
    ///
    /// let ticks = VolumeProfileConfig::default().with_row_ticks(Some(4)).unwrap();
    /// assert_eq!((ticks.row_ticks(), ticks.row_size()), (Some(4), None));
    /// let height = ticks.with_row_size(Some(2.0)).unwrap();
    /// assert_eq!((height.row_ticks(), height.row_size()), (None, Some(2.0)));
    /// let ticks = height.with_row_ticks(Some(4)).unwrap();
    /// assert_eq!((ticks.row_ticks(), ticks.row_size()), (Some(4), None));
    /// assert!(ticks.with_row_ticks(Some(0)).is_none());
    /// ```
    pub fn with_row_ticks(mut self, ticks: Option<usize>) -> Option<Self> {
        if ticks.is_some_and(|ticks| !Self::ROW_TICKS.contains(&ticks)) {
            return None;
        }
        self.row_ticks = ticks;
        if ticks.is_some() {
            self.row_size = None;
        }
        Some(self)
    }

    /// Ticks of the chart instrument per row of a volume profile drawing, when set.
    pub fn row_ticks(&self) -> Option<usize> {
        self.row_ticks
    }

    /// Price height of [`Self::row_ticks`] ticks of `tick_size`; `None` without row ticks,
    /// without a tick size, or when the height is not finite and positive.
    #[cfg(feature = "drawings")]
    pub(crate) fn tick_height(&self, tick_size: Option<f64>) -> Option<f64> {
        let height = self.row_ticks? as f64 * tick_size?;
        (height.is_finite() && height > 0.0).then_some(height)
    }

    /// `self` in rows a whole number of `unit`s high over the span `low` to `high`, given in
    /// `unit`s: the count whose row total lies closest to [`Self::rows`], as the volume profile
    /// studies lay a row count on whole ticks. Unchanged with a price height, and for a span or
    /// height that cannot be represented.
    #[cfg(feature = "drawings")]
    pub(crate) fn with_whole_rows(self, unit: f64, low: f64, high: f64) -> Self {
        if self.row_size.is_some() {
            return self;
        }
        whole_tick_rows(low, high, self.rows)
            .and_then(|units| self.with_row_size(Some(units as f64 * unit)))
            .unwrap_or(self)
    }

    /// `self` with the row layout and value area of `layout`, keeping every display setting.
    #[cfg(feature = "ui")]
    pub(crate) fn with_layout(self, layout: Self) -> Self {
        Self {
            row_size: layout.row_size,
            row_ticks: layout.row_ticks,
            rows: layout.rows,
            value_area: layout.value_area,
            ..self
        }
    }

    /// Number of price rows asked for.
    pub fn rows(&self) -> usize {
        self.rows
    }

    /// Share of the volume inside the value area.
    pub fn value_area(&self) -> f64 {
        self.value_area
    }

    /// Settings a new volume profile drawing starts with: up and down volume side by side, which
    /// on a white chart show as cyan `#7CD2E1` and pink `#DE779B` inside the value area, with the
    /// bars still visible through them, and paler outside it. Studies keep [`Self::default`].
    pub fn drawing_default() -> Self {
        Self {
            display: ProfileDisplay::UpDown,
            up_color: [83, 196, 216, 191],
            down_color: [214, 74, 122, 191],
            ..Self::default()
        }
    }
}

impl Default for VolumeProfileConfig {
    /// 24 rows and a value area of 70% of the volume.
    fn default() -> Self {
        Self {
            row_size: None,
            row_ticks: None,
            rows: 24,
            value_area: 0.7,
            display: ProfileDisplay::Total,
            up_color: profile_up_color(),
            down_color: profile_down_color(),
            value_area_colors: None,
            show_value_area_lines: false,
            show_rows: true,
            placement: ProfilePlacement::Left,
            histogram_width: histogram_width(),
            show_values: false,
            values_color: None,
            value_area_values_color: None,
            show_poc: true,
            poc_color: None,
            show_value_area_high: false,
            show_value_area_low: false,
            value_area_line_colors: profile_line_colors(),
            show_box: true,
            box_color: None,
            extend_right: false,
            show_developing_poc: false,
            show_developing_value_area: false,
            developing_poc_color: developing_poc_color(),
            developing_value_area_colors: developing_value_area_colors(),
            exact_row_opacity: false,
        }
    }
}

/// Where a [`VolumeProfile`]'s volumes come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ProfileSource {
    /// Bar volume spread over each bar's high-low range, split by candle direction, from the
    /// chart's bars or from finer bars where they cover the chart bars.
    Estimated,
    /// Executed volume in each price row, split by aggressor side, with unclassified volume
    /// apart. `complete` is false when the span starts before held coverage or includes a bar
    /// whose executions may be missing. Prepared bars before a later gap can still form a complete span.
    Executions {
        /// Whether the executions of every bar in the span are held.
        complete: bool,
    },
    /// Call and put contract volume at each strike, from [`VolumeProfile::from_options`].
    Options,
}

/// The executions behind an exact profile, whose POC and value area follow [`FootprintBar`].
#[derive(Clone, Debug, PartialEq)]
struct Executions {
    first_level: i64,
    bar: FootprintBar,
    complete: bool,
}

#[cfg(any(feature = "studies", feature = "widget"))]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct StrikeExposure {
    pub strike: f64,
    pub calls: Option<f64>,
    pub puts: Option<f64>,
    pub net: Option<f64>,
}

#[cfg(any(feature = "studies", feature = "widget"))]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExposureProfile {
    pub snapshot: super::OptionExposure,
    pub rows: Vec<StrikeExposure>,
    pub note: String,
}

/// Calls plus puts of each selected expiry by row, for an options profile drawn by expiry.
#[cfg(any(feature = "studies", feature = "widget"))]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ExpiryParts {
    /// Each expiry's date label and colour, in the order its part is drawn from the base.
    pub labels: Vec<(String, [u8; 4])>,
    /// `rows[part][row]`, the expiry's calls plus puts in each row. A row's parts add up to its
    /// volume.
    pub rows: Vec<Vec<f64>>,
}

/// The call and put walls of an options profile drawn at its expiration, named beside their rows.
#[cfg(any(feature = "studies", feature = "widget"))]
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct OptionWalls {
    /// Call and put wall strikes; `None` for a side without one.
    pub strikes: [Option<f64>; 2],
    /// Call and put wall colours, unmultiplied RGBA.
    pub colors: [[u8; 4]; 2],
}

/// Volume traded at each price row of bars or host-supplied option strikes.
#[derive(Clone, Debug, PartialEq)]
pub struct VolumeProfile {
    /// Price at the bottom of row 0.
    pub(crate) low: f64,
    /// Highest price of the bars.
    pub(crate) high: f64,
    pub(crate) row_height: f64,
    fixed_height: bool,
    /// Original price of an all-flat span, before padding its grid.
    flat_price: Option<f64>,
    /// Volume per row, lowest prices first.
    pub(crate) volume: Vec<f64>,
    pub(crate) up: Vec<f64>,
    pub(crate) down: Vec<f64>,
    executions: Option<Executions>,
    /// Built from option strikes by [`Self::from_options`].
    options: bool,
    // Signed figures and their readout for an options exposure overlay; row volumes bound it.
    #[cfg(any(feature = "studies", feature = "widget"))]
    pub(crate) exposure: Option<Arc<ExposureProfile>>,
    // Each row split by expiry, for an options profile drawn by expiry.
    #[cfg(any(feature = "studies", feature = "widget"))]
    pub(crate) expiry_parts: Option<ExpiryParts>,
    // Each row holds its strike's puts over calls, for an options profile of put/call ratios.
    #[cfg(any(feature = "studies", feature = "widget"))]
    pub(crate) ratio_rows: bool,
    // The walls of an options profile drawn at its expiration.
    #[cfg(any(feature = "studies", feature = "widget"))]
    pub(crate) walls: Option<OptionWalls>,
    // The expiration date an options profile is drawn at and the colour of its time scale tag.
    #[cfg(any(feature = "studies", feature = "widget"))]
    pub(crate) expiry_tag: Option<(chrono::NaiveDate, [u8; 4])>,
}

fn valid_bar(bar: &Bar) -> bool {
    [bar.open, bar.high, bar.low, bar.close, bar.volume]
        .iter()
        .all(|v| v.is_finite())
        && (bar.high - bar.low).is_finite()
        && bar.high >= bar.low
        && bar.volume >= 0.0
}

/// Lowest low and highest high of the bars a profile distributes.
pub(crate) fn price_span(bars: &[Bar]) -> Option<(f64, f64)> {
    bars.iter()
        .filter(|bar| valid_bar(bar))
        .map(|bar| (bar.low, bar.high))
        .reduce(|(low, high), (lo, hi)| (low.min(lo), high.max(hi)))
}

/// Source-bar cumulative volume, independent of the histogram's row layout. Shared tree
/// paths keep the prefix before a forming bar without copying the price history on each tick.
#[cfg(feature = "studies")]
#[derive(Clone, Debug, Default)]
pub(crate) struct DevelopingVolume {
    origin: Option<f64>,
    root: Option<Arc<VolumeEdge>>,
    volume: f64,
    density_total: f64,
    density_min: f64,
    unrepresentable: bool,
    flat_high: Option<f64>,
}

#[cfg(feature = "studies")]
#[derive(Clone, Debug)]
struct VolumeEdge {
    price: f64,
    height: usize,
    changes: usize,
    // Density change, its price moment, and point mass, respectively.
    value: [f64; 3],
    sum: [f64; 3],
    children: [Option<Arc<Self>>; 2],
}

#[cfg(feature = "studies")]
impl VolumeEdge {
    fn height(root: &Option<Arc<Self>>) -> usize {
        root.as_ref().map_or(0, |node| node.height)
    }

    fn refresh(&mut self) {
        self.sum = self.value;
        self.height = 1;
        self.changes = usize::from(self.value[0] != 0.0 || self.value[1] != 0.0);
        for child in self.children.iter().flatten() {
            self.height = self.height.max(child.height + 1);
            self.changes += child.changes;
            for (sum, value) in self.sum.iter_mut().zip(child.sum) {
                *sum += value;
            }
        }
    }

    fn insert(root: &mut Option<Arc<Self>>, price: f64, value: [f64; 3]) {
        let Some(edge) = root else {
            *root = Some(Arc::new(Self {
                price,
                height: 1,
                changes: usize::from(value[0] != 0.0 || value[1] != 0.0),
                value,
                sum: value,
                children: [None, None],
            }));
            return;
        };
        let node = Arc::make_mut(edge);
        if price == node.price {
            for (current, added) in node.value.iter_mut().zip(value) {
                *current += added;
            }
        } else {
            let side = usize::from(price > node.price);
            Self::insert(&mut node.children[side], price, value);
        }
        node.refresh();
        let [left, right] = node.children.each_ref().map(Self::height);
        if left.abs_diff(right) > 1 {
            let side = usize::from(right > left);
            let child = node.children[side].as_ref().unwrap();
            if Self::height(&child.children[1 - side]) > Self::height(&child.children[side]) {
                Self::rotate(&mut node.children[side], 1 - side);
            }
            Self::rotate(root, side);
        }
    }

    fn rotate(root: &mut Option<Arc<Self>>, side: usize) {
        let mut old = root.take().unwrap();
        let mut next = Arc::make_mut(&mut old).children[side].take().unwrap();
        Arc::make_mut(&mut old).children[side] = Arc::make_mut(&mut next).children[1 - side].take();
        Arc::make_mut(&mut old).refresh();
        Arc::make_mut(&mut next).children[1 - side] = Some(old);
        Arc::make_mut(&mut next).refresh();
        *root = Some(next);
    }
}

#[cfg(feature = "studies")]
impl DevelopingVolume {
    pub(crate) fn add(&mut self, bars: &[Bar]) {
        for bar in bars.iter().filter(|bar| valid_bar(bar) && bar.volume > 0.0) {
            let origin = *self.origin.get_or_insert(bar.low);
            if bar.high == bar.low {
                self.flat_high = Some(self.flat_high.map_or(bar.low, |high| high.max(bar.low)));
                VolumeEdge::insert(&mut self.root, bar.low, [0.0, 0.0, bar.volume]);
            } else {
                let density = bar.volume / (bar.high - bar.low);
                self.volume += bar.volume;
                self.density_total += density;
                self.density_min = if self.density_min == 0.0 {
                    density
                } else {
                    self.density_min.min(density)
                };
                self.unrepresentable |= !density.is_normal();
                self.unrepresentable |= !self.volume.is_finite()
                    || !self.density_total.is_finite()
                    || self.density_total + self.density_min == self.density_total;
                for (price, change) in [(bar.low, density), (bar.high, -density)] {
                    let value = [change, change * (price - origin), 0.0];
                    self.unrepresentable |= value.iter().any(|v| !v.is_finite());
                    VolumeEdge::insert(&mut self.root, price, value);
                }
            }
            self.unrepresentable |= self
                .root
                .as_ref()
                .unwrap()
                .sum
                .iter()
                .any(|v| !v.is_finite());
            // Cancellation must not destroy the source volume represented by the moments.
            self.unrepresentable |= (self.root.as_ref().unwrap().sum[1] + self.volume).abs()
                > self.volume * f64::EPSILON.sqrt();
        }
    }

    fn before(&self, includes: impl Fn(f64) -> bool) -> ([f64; 3], usize) {
        let mut sum = [0.0; 3];
        let mut changes = 0;
        let mut node = self.root.as_deref();
        while let Some(edge) = node {
            if includes(edge.price) {
                changes += usize::from(edge.value[0] != 0.0 || edge.value[1] != 0.0)
                    + edge.children[0].as_ref().map_or(0, |left| left.changes);
                for (i, total) in sum.iter_mut().enumerate() {
                    *total +=
                        edge.value[i] + edge.children[0].as_ref().map_or(0.0, |left| left.sum[i]);
                }
                node = edge.children[1].as_deref();
            } else {
                node = edge.children[0].as_deref();
            }
        }
        (sum, changes)
    }

    /// Highest price a bar without range traded volume at.
    pub(crate) fn flat_high(&self) -> Option<f64> {
        self.flat_high
    }

    /// Only total volume is needed for developing POC and value area. Continuous bars integrate
    /// their original density; point masses follow the histogram's rounding at row edges.
    pub(crate) fn fill(&self, profile: &mut VolumeProfile) -> Option<()> {
        if self.unrepresentable {
            return None;
        }
        let changes = self.root.as_ref().map_or(0, |root| root.changes);
        let cumulative = |price: f64, values: [f64; 3], through| {
            if through == 0 {
                0.0
            } else if through == changes {
                self.volume
            } else {
                values[0] * (price - self.origin.unwrap_or(0.0)) - values[1]
            }
        };
        let continuous = |low, high| {
            let (below, before) = self.before(|edge| edge <= low);
            let (above, through) = self.before(|edge| edge < high);
            if before == through {
                // Integrate linear spans directly so equal rows do not acquire POC noise.
                if before == 0 || before == changes || below[0] == 0.0 {
                    0.0
                } else {
                    below[0] * (high - low)
                }
            } else {
                cumulative(high, above, through) - cumulative(low, below, before)
            }
        };
        let total_points = self.root.as_ref().map_or(0.0, |root| root.sum[2]);
        let points = |row| {
            if total_points == 0.0 {
                0.0
            } else {
                self.before(|price| grid_row(profile.low, profile.row_height, price).0 < row)
                    .0[2]
            }
        };
        let mut point_low = points(0.0);
        for row in 0..profile.volume.len() {
            let low = profile.low + row as f64 * profile.row_height;
            let high = low + profile.row_height;
            let point_high = if row + 1 == profile.volume.len() {
                total_points
            } else {
                points((row + 1) as f64)
            };
            let volume = continuous(low, high) + point_high - point_low;
            if !volume.is_finite() {
                return None;
            }
            profile.volume[row] = volume.max(0.0);
            point_low = point_high;
        }
        Some(())
    }
}

impl VolumeProfile {
    /// Group host options turnover into rows centered on multiples of `row_size`.
    /// A row at strike K includes [K - row_size/2, K + row_size/2); a boundary
    /// goes to the higher row. Calls occupy [`Self::up`], puts [`Self::down`].
    /// Duplicate strikes are summed. Empty, invalid, overflowing or unrepresentable
    /// snapshots, including grids exceeding 10,000 rows, produce no profile.
    pub fn from_options(data: &[crate::data::OptionsVolume], row_size: f64) -> Option<Self> {
        Self::options_rows(data, row_size).map(|(profile, _)| profile)
    }

    /// [`Self::from_options`], with each row also split by expiry: `part[i]` is the index in
    /// `labels` of `data[i]`'s expiry. `None` where `from_options` is, or when `part` does not
    /// give every datum a label.
    // Only the options study calls it; under `widget` alone it would be dead code.
    #[cfg(feature = "studies")]
    pub(crate) fn from_option_parts(
        data: &[crate::data::OptionsVolume],
        part: &[usize],
        labels: Vec<(String, [u8; 4])>,
        row_size: f64,
    ) -> Option<Self> {
        if part.len() != data.len() || part.iter().any(|part| *part >= labels.len()) {
            return None;
        }
        let (mut profile, rows) = Self::options_rows(data, row_size)?;
        let mut parts = vec![vec![0.0; profile.volume.len()]; labels.len()];
        for ((value, part), row) in data.iter().zip(part).zip(rows) {
            parts[*part][row] += value.calls + value.puts;
        }
        profile.expiry_parts = Some(ExpiryParts {
            labels,
            rows: parts,
        });
        Some(profile)
    }

    /// [`Self::from_options`] and the row of each datum.
    fn options_rows(
        data: &[crate::data::OptionsVolume],
        row_size: f64,
    ) -> Option<(Self, Vec<usize>)> {
        if data.is_empty()
            || !row_size.is_finite()
            || row_size <= 0.0
            || data.iter().any(|v| {
                !v.strike.is_finite()
                    || !v.calls.is_finite()
                    || !v.puts.is_finite()
                    || v.calls < 0.0
                    || v.puts < 0.0
            })
        {
            return None;
        }
        let key = |strike| grid_floor(strike / row_size + 0.5);
        let first = data
            .iter()
            .map(|v| key(v.strike))
            .fold(f64::INFINITY, f64::min);
        let last = data
            .iter()
            .map(|v| key(v.strike))
            .fold(f64::NEG_INFINITY, f64::max);
        let count = last - first + 1.0;
        if !first.is_finite()
            || !last.is_finite()
            || first.abs() >= 2_f64.powi(52)
            || last.abs() >= 2_f64.powi(52)
            || !(1.0..=10_000.0).contains(&count)
        {
            return None;
        }
        let low = (first - 0.5) * row_size;
        let high = low + count * row_size;
        if !low.is_finite() || !high.is_finite() || high <= low {
            return None;
        }
        let mut profile = Self {
            low,
            high,
            row_height: row_size,
            fixed_height: false,
            flat_price: None,
            volume: vec![0.0; count as usize],
            up: vec![0.0; count as usize],
            down: vec![0.0; count as usize],
            executions: None,
            options: true,
            #[cfg(any(feature = "studies", feature = "widget"))]
            exposure: None,
            #[cfg(any(feature = "studies", feature = "widget"))]
            expiry_parts: None,
            #[cfg(any(feature = "studies", feature = "widget"))]
            ratio_rows: false,
            #[cfg(any(feature = "studies", feature = "widget"))]
            walls: None,
            #[cfg(any(feature = "studies", feature = "widget"))]
            expiry_tag: None,
        };
        let mut total = 0.0;
        let rows = data
            .iter()
            .map(|v| {
                let row = (key(v.strike) - first) as usize;
                profile.up[row] += v.calls;
                profile.down[row] += v.puts;
                profile.volume[row] = profile.up[row] + profile.down[row];
                total += v.calls + v.puts;
                row
            })
            .collect();
        total.is_finite().then_some((profile, rows))
    }

    /// Distribute finite, nonnegative-volume bars over their high-low ranges.
    /// Invalid bars are ignored. Returns no profile for an empty or unrepresentable grid.
    pub fn from_bars(bars: &[Bar], config: VolumeProfileConfig) -> Option<Self> {
        Self::of(bars, config.rows(), config.row_size())
    }

    /// Group the executions of one span into a row per traded price step, from the lowest
    /// traded row to the highest, so untraded rows between them hold zero. `step` is the
    /// footprint's [`price_step`](super::Footprint::price_step), `tick_size` its
    /// [`tick_size`](super::Footprint::tick_size), and `complete` whether executions are held
    /// from the start of the span. A row spans from half a tick below its first tick to half
    /// a tick below the next row's, so a trade at 100 with a tick of 1 spans 99.5 to 100.5.
    /// Buys occupy [`Self::up`] and sells [`Self::down`]; unclassified volume is in
    /// [`Self::volume`] only. Returns no profile without executed volume, or for more than
    /// 10,000 rows.
    pub fn from_executions(
        bar: &FootprintBar,
        step: f64,
        tick_size: f64,
        complete: bool,
    ) -> Option<Self> {
        let (&first, &last) = (bar.levels.keys().next()?, bar.levels.keys().next_back()?);
        let count = last as i128 - first as i128 + 1;
        let half_tick = tick_size * 0.5;
        let (low, high) = (
            first as f64 * step - half_tick,
            (last as f64 + 1.0) * step - half_tick,
        );
        if bar.volume() <= 0.0
            || !step.is_finite()
            || step <= 0.0
            || !tick_size.is_finite()
            || !(1..=10_000).contains(&count)
            || !low.is_finite()
            || !high.is_finite()
        {
            return None;
        }
        let mut profile = Self {
            low,
            high,
            row_height: step,
            fixed_height: true,
            flat_price: None,
            volume: vec![0.0; count as usize],
            up: vec![0.0; count as usize],
            down: vec![0.0; count as usize],
            executions: Some(Executions {
                first_level: first,
                bar: bar.clone(),
                complete,
            }),
            options: false,
            #[cfg(any(feature = "studies", feature = "widget"))]
            exposure: None,
            #[cfg(any(feature = "studies", feature = "widget"))]
            expiry_parts: None,
            #[cfg(any(feature = "studies", feature = "widget"))]
            ratio_rows: false,
            #[cfg(any(feature = "studies", feature = "widget"))]
            walls: None,
            #[cfg(any(feature = "studies", feature = "widget"))]
            expiry_tag: None,
        };
        for (&level, row) in &bar.levels {
            let i = (level - first) as usize;
            profile.volume[i] = row.volume();
            profile.up[i] = row.ask;
            profile.down[i] = row.bid;
        }
        Some(profile)
    }

    /// [`Self::from_executions`] of `bar` in the rows `config` lays out, as volume profile
    /// studies and drawings group executions. `bar`'s levels are rows of `step`, a footprint's
    /// [`price_step`](super::Footprint::price_step), so the rows here are whole steps: the row
    /// height in steps, rounded, at least one, or the traded steps over the row count, rounded
    /// up, counted from multiples of the row as a footprint groups them and widened where that
    /// would exceed the row count. `None` without executed volume, with a volume that is not
    /// finite, or past 10,000 rows.
    #[cfg(any(feature = "studies", feature = "drawings"))]
    pub(crate) fn of_executions(
        bar: &FootprintBar,
        config: &VolumeProfileConfig,
        step: f64,
        tick_size: f64,
        complete: bool,
    ) -> Option<Self> {
        let (&first, &last) = (bar.levels.keys().next()?, bar.levels.keys().next_back()?);
        if !bar.volume().is_finite() {
            return None;
        }
        let steps = steps_per_row(config, step, first, last)?;
        let mut rows = FootprintBar::empty(bar.time);
        rows.absorb_grouped(bar, steps);
        Self::from_executions(&rows, steps as f64 * step, tick_size, complete)
    }

    /// Whether the volumes are estimated from bars, executed or option turnover.
    pub fn source(&self) -> ProfileSource {
        if self.options {
            return ProfileSource::Options;
        }
        self.executions
            .as_ref()
            .map_or(ProfileSource::Estimated, |e| ProfileSource::Executions {
                complete: e.complete,
            })
    }

    /// Bottom edge of the first row.
    pub fn low(&self) -> f64 {
        self.low
    }
    /// Top edge of the last row.
    pub fn high(&self) -> f64 {
        self.low + self.volume.len() as f64 * self.row_height
    }
    /// Price height of a row.
    pub fn row_height(&self) -> f64 {
        self.row_height
    }
    /// Total volume per row, lowest price first.
    pub fn volume(&self) -> &[f64] {
        &self.volume
    }
    /// Up-bar volume per row (unchanged closes count as up, except in a fixed range or anchored
    /// profile estimated from one chart bar), or calls for options.
    pub fn up(&self) -> &[f64] {
        &self.up
    }
    /// Down-bar volume per row, or puts for options.
    pub fn down(&self) -> &[f64] {
        &self.down
    }

    /// Spreads each bar's volume evenly over its high-low range across
    /// `rows` equal price rows (a bar without range puts it in the row of its
    /// price). A flat span uses one narrow row; `None` for an unrepresentable grid.
    /// Fixed rows end at the span's top row edge; a price traded without range on that edge
    /// keeps the row above it.
    pub(crate) fn of(bars: &[Bar], rows: usize, row_size: Option<f64>) -> Option<Self> {
        let (low, high) = price_span(bars)?;
        let flat_high = bars
            .iter()
            .filter(|bar| valid_bar(bar) && bar.volume > 0.0 && bar.low == bar.high)
            .map(|bar| bar.low)
            .reduce(f64::max);
        let mut profile = Self::empty(low, high, flat_high, rows, row_size)?;
        bars.iter().for_each(|bar| profile.add(bar));
        Some(profile)
    }

    pub(crate) fn empty(
        low: f64,
        high: f64,
        flat_high: Option<f64>,
        rows: usize,
        row_size: Option<f64>,
    ) -> Option<Self> {
        if rows == 0 || rows > 10_000 {
            return None;
        }
        let flat_price = (low == high).then_some(low);
        let (low, high, row_height, rows) = if let Some(height) = row_size {
            let (low, high, rows) = fixed_grid(low, high, flat_high, height)?;
            (low, high, height, rows)
        } else if low == high && low.is_finite() {
            let height = (low.abs() * 1e-6).max(1e-6);
            (low - height * 0.5, high + height * 0.5, height, 1)
        } else {
            (low, high, (high - low) / rows as f64, rows)
        };
        if !low.is_finite() || !high.is_finite() || !row_height.is_finite() || row_height <= 0.0 {
            return None;
        }
        Some(Self {
            low,
            high,
            row_height,
            fixed_height: row_size.is_some(),
            flat_price,
            volume: vec![0.0; rows],
            up: vec![0.0; rows],
            down: vec![0.0; rows],
            executions: None,
            options: false,
            #[cfg(any(feature = "studies", feature = "widget"))]
            exposure: None,
            #[cfg(any(feature = "studies", feature = "widget"))]
            expiry_parts: None,
            #[cfg(any(feature = "studies", feature = "widget"))]
            ratio_rows: false,
            #[cfg(any(feature = "studies", feature = "widget"))]
            walls: None,
            #[cfg(any(feature = "studies", feature = "widget"))]
            expiry_tag: None,
        })
    }

    /// Extend developing total volume, integrating source bars when row height changes and
    /// retaining existing total volumes when a fixed grid grows.
    #[cfg(feature = "studies")]
    pub(crate) fn extend_developing(
        &mut self,
        span: (f64, f64),
        rows: usize,
        row_size: Option<f64>,
        bars: &[Bar],
        source: &DevelopingVolume,
    ) -> Option<()> {
        let same_grid = if let Some(height) = row_size {
            self.fixed_height
                && self.row_height == height
                && fixed_grid(span.0, span.1, source.flat_high(), height)
                    .is_some_and(|(low, high, _)| (low, high) == (self.low, self.high))
        } else {
            !self.fixed_height
                && if let Some(price) = self.flat_price {
                    span == (price, price)
                } else {
                    span == (self.low, self.high) && rows == self.volume.len()
                }
        };
        if same_grid {
            bars.iter().for_each(|bar| self.add(bar));
            return Some(());
        }
        let mut next = Self::empty(span.0, span.1, source.flat_high(), rows, row_size)?;
        if (self.low, self.high, self.row_height, self.volume.len())
            != (next.low, next.high, next.row_height, next.volume.len())
        {
            let offset = ((self.low - next.low) / self.row_height).round() as usize;
            if self.fixed_height
                && next.fixed_height
                && self.row_height == next.row_height
                && offset + self.volume.len() <= next.volume.len()
            {
                next.volume[offset..offset + self.volume.len()].copy_from_slice(&self.volume);
            } else {
                source.fill(&mut next)?;
                *self = next;
                return Some(());
            }
            *self = next;
        }
        bars.iter().for_each(|bar| self.add(bar));
        Some(())
    }

    /// The sorted `finer` bars opening inside each of `bars`, up to the next chart bar's open or
    /// `end` after the last; `None` for a chart bar without any, which keeps its estimate.
    #[cfg(any(feature = "studies", feature = "drawings"))]
    pub(crate) fn finer_parts<'a>(
        bars: &'a [Bar],
        end: chrono::DateTime<chrono::Utc>,
        finer: &'a [Bar],
    ) -> impl Iterator<Item = Option<&'a [Bar]>> + 'a {
        bars.iter().enumerate().map(move |(i, bar)| {
            let next = bars.get(i + 1).map_or(end, |next| next.time);
            let inside = &finer[finer.partition_point(|f| f.time < bar.time)
                ..finer.partition_point(|f| f.time < next)];
            (!inside.is_empty()).then_some(inside)
        })
    }

    /// Replaces each of `bars` by the sorted `finer` bars opening inside it, up to the next
    /// chart bar's open or `end` after the last. Bars without finer inputs keep their estimate.
    /// Returns the profile and the number of chart bars with finer inputs.
    #[cfg(any(feature = "studies", feature = "drawings"))]
    pub(crate) fn of_finer(
        bars: &[Bar],
        end: chrono::DateTime<chrono::Utc>,
        finer: &[Bar],
        rows: usize,
        row_size: Option<f64>,
    ) -> (Option<Self>, usize) {
        let mut inputs = Vec::new();
        let mut covered = 0;
        for (bar, part) in bars.iter().zip(Self::finer_parts(bars, end, finer)) {
            if let Some(part) = part {
                covered += 1;
                inputs.extend_from_slice(part);
            } else {
                inputs.push(bar.clone());
            }
        }
        (Self::of(&inputs, rows, row_size), covered)
    }

    /// Add one completed bar without redistributing earlier bars. Fixed-height grids grow
    /// at the edges while keeping their original row origin; a grid over one price, and
    /// equal-row grids, return `None` when the bar moves a boundary.
    /// A changed forming bar must be added to the profile before that bar, not added twice.
    pub fn with_bar(&self, bar: &Bar) -> Option<Self> {
        let mut profile = self.clone();
        if !valid_bar(bar) {
            return Some(profile);
        }
        if self
            .flat_price
            .is_some_and(|price| bar.low != price || bar.high != price)
        {
            return None;
        }
        if self.fixed_height {
            let origin = if self.executions.is_some() {
                self.low
            } else {
                0.0
            };
            let row_of = |price| grid_row(origin, self.row_height, price);
            let base = row_of(self.low).0;
            let end = base + self.volume.len() as f64;
            let first = row_of(bar.low).0.min(base);
            let (last, edge) = row_of(bar.high);
            let flat = bar.low == bar.high && bar.volume > 0.0;
            let top = if edge && !flat { last } else { last + 1.0 };
            let top = top.max(end);
            let count = top - first;
            if first.abs() >= 2_f64.powi(52)
                || top.abs() >= 2_f64.powi(52)
                || !(1.0..=10_000.0).contains(&count)
            {
                return None;
            }
            let low = if first < base {
                origin + first * self.row_height
            } else {
                self.low
            };
            let high = if top > end {
                origin + top * self.row_height
            } else {
                self.high
            };
            if !low.is_finite() || !high.is_finite() || !(high - low).is_finite() || high <= low {
                return None;
            }
            let prepend = (base - first) as usize;
            for rows in [&mut profile.volume, &mut profile.up, &mut profile.down] {
                let old_len = rows.len();
                rows.resize(count as usize, 0.0);
                if prepend > 0 {
                    rows.copy_within(..old_len, prepend);
                    rows[..prepend].fill(0.0);
                }
            }
            profile.low = low;
            profile.high = high;
        } else if bar.low < self.low || bar.high > self.high {
            return None;
        }
        profile.add(bar);
        Some(profile)
    }

    /// Adds the volume of `bar` to the rows that its high-low range covers,
    /// in proportion to the overlap.
    pub(crate) fn add(&mut self, bar: &Bar) {
        if !valid_bar(bar) {
            return;
        }
        let (low, row_height) = (self.low, self.row_height);
        let direction = if bar.close >= bar.open {
            &mut self.up
        } else {
            &mut self.down
        };
        let top_row = self.volume.len() - 1;
        let row_of = |price: f64| (grid_row(low, row_height, price).0 as usize).min(top_row);
        if bar.high > bar.low {
            let (first, last) = (row_of(bar.low), row_of(bar.high));
            for (row, row_volume) in self
                .volume
                .iter_mut()
                .enumerate()
                .take(last + 1)
                .skip(first)
            {
                let row_low = low + row as f64 * row_height;
                let overlap = bar.high.min(row_low + row_height) - bar.low.max(row_low);
                let amount = bar.volume * (overlap.max(0.0) / (bar.high - bar.low));
                *row_volume += amount;
                direction[row] += amount;
            }
        } else {
            self.volume[row_of(bar.low)] += bar.volume;
            direction[row_of(bar.low)] += bar.volume;
        }
    }

    /// Count the volume of this estimate of `bar` alone as the volume of one bar: up only when
    /// the bar closes above its open, so the volume of a bar that closes at its open moves to the
    /// down side. [`Self::add`] counts an unchanged close as up, the rule over several bars.
    #[cfg(any(feature = "studies", feature = "drawings"))]
    pub(crate) fn count_single_bar(&mut self, bar: &Bar) {
        if bar.close == bar.open {
            for (up, down) in self.up.iter_mut().zip(&mut self.down) {
                *down += std::mem::take(up);
            }
        }
    }

    /// The row with the most volume (point of control), and the lowest and
    /// highest row of the value area, which holds `share` of the volume;
    /// `None` when no volume traded. POC ties take the lower row. The value
    /// area grows as [`FootprintBar::value_area`] describes, whatever the source.
    pub fn poc_and_value_area(&self, share: f64) -> Option<(usize, (usize, usize))> {
        if !share.is_finite() || share <= 0.0 || share > 1.0 {
            return None;
        }
        let volume = &self.volume;
        let total: f64 = volume.iter().sum();
        if !total.is_finite() || total <= 0.0 {
            return None;
        }
        if let Some(e) = &self.executions {
            let row = |level: i64| (level - e.first_level) as usize;
            let (low, high) = e.bar.value_area(share)?;
            return Some((row(e.bar.poc()?), (row(low), row(high))));
        }
        let poc = (0..volume.len())
            .max_by(|&a, &b| volume[a].total_cmp(&volume[b]).then_with(|| b.cmp(&a)))?;
        let (low, high) = value_area(volume.len(), |i| (i as i64, volume[i]), poc, total * share);
        Some((poc, (low as usize, high as usize)))
    }

    /// The point of control and value area holding `share` of the volume, in this profile's rows,
    /// after each of `parts`, this profile's bars split by chart bar in time order and spread as
    /// [`Self::add`] spreads them; `None` before volume trades. The last entry is this profile's
    /// own, so a developing line ends on the levels it shows. Empty for an executed or options
    /// profile.
    #[cfg(feature = "drawings")]
    pub(crate) fn developing_estimate<'a>(
        &self,
        share: f64,
        parts: impl IntoIterator<Item = &'a [Bar]>,
    ) -> Vec<Option<(usize, (usize, usize))>> {
        if self.executions.is_some() || self.options {
            return Vec::new();
        }
        let mut running = self.clone();
        for rows in [&mut running.volume, &mut running.up, &mut running.down] {
            rows.fill(0.0);
        }
        let mut levels: Vec<_> = parts
            .into_iter()
            .map(|part| {
                part.iter().for_each(|bar| running.add(bar));
                running.poc_and_value_area(share)
            })
            .collect();
        if let Some(last) = levels.last_mut() {
            *last = self.poc_and_value_area(share);
        }
        levels
    }

    /// [`Self::developing_estimate`] of an executed profile: `parts` holds the executions of each
    /// chart bar of the span, as [`FootprintView::profile`](super::FootprintView::profile) sums
    /// them in rows of the footprint's price `step`, which this profile groups into its own rows.
    /// Empty for a profile not built from executions.
    #[cfg(feature = "drawings")]
    pub(crate) fn developing_executions(
        &self,
        share: f64,
        step: f64,
        parts: impl IntoIterator<Item = FootprintBar>,
    ) -> Vec<Option<(usize, (usize, usize))>> {
        let Some(e) = &self.executions else {
            return Vec::new();
        };
        // The profile's rows are a whole number of steps high.
        let group = (self.row_height / step).round();
        if !group.is_finite() || group < 1.0 {
            return Vec::new();
        }
        let row = |level: i64| (level - e.first_level) as usize;
        let mut running = FootprintBar::empty(e.bar.time);
        let mut levels: Vec<_> = parts
            .into_iter()
            .map(|part| {
                running.absorb_grouped(&part, group as i64);
                let (low, high) = running.value_area(share)?;
                Some((row(running.poc()?), (row(low), row(high))))
            })
            .collect();
        if let Some(last) = levels.last_mut() {
            *last = self.poc_and_value_area(share);
        }
        levels
    }
}

// Decimal tick heights can put an exact boundary one rounding step below its integer row.
fn grid_floor(value: f64) -> f64 {
    grid_edge(value, value.abs()).unwrap_or_else(|| value.floor())
}

/// The integer nearest `value` when `value` lies within the rounding of figures of size up to
/// `scale` it was computed from.
fn grid_edge(value: f64, scale: f64) -> Option<f64> {
    let nearest = value.round();
    ((value - nearest).abs() <= 4.0 * f64::EPSILON * scale).then_some(nearest)
}

/// First row and exclusive top edge of a span in unit rows. A high within rounding of an
/// edge closes the grid there; a span on one edge keeps the row above it.
fn grid_rows(low: f64, high: f64) -> (f64, f64) {
    let first = grid_floor(low);
    let top = grid_edge(high, high.abs()).unwrap_or_else(|| high.ceil());
    (first, if top > first { top } else { first + 1.0 })
}

/// Whole ticks per row closest to the requested row count on a zero-aligned grid.
/// Compares the span per row rounded down and up, taking the larger tick count on a tie.
/// Rejects nonfinite, reversed or unrepresentable spans and zero rows.
#[cfg(any(feature = "studies", feature = "drawings"))]
pub(crate) fn whole_tick_rows(low: f64, high: f64, rows: usize) -> Option<u64> {
    let span = high - low;
    if !low.is_finite() || !high.is_finite() || !span.is_finite() || span < 0.0 || rows == 0 {
        return None;
    }
    let raw = span / rows as f64;
    [raw.floor().max(1.0), raw.ceil().max(1.0)]
        .into_iter()
        .filter_map(|ticks| {
            let (lo, hi) = (low / ticks, high / ticks);
            if ticks >= 2_f64.powi(53) || lo.abs() >= 2_f64.powi(52) || hi.abs() >= 2_f64.powi(52) {
                return None;
            }
            let (first, top) = grid_rows(lo, hi);
            let count = top - first;
            Some((ticks as u64, (count - rows as f64).abs()))
        })
        .min_by(|(a, error_a), (b, error_b)| error_a.total_cmp(error_b).then_with(|| b.cmp(a)))
        .map(|(ticks, _)| ticks)
}

/// The row of a grid from `low` in rows of `height` that `price` lies in, counted from its first
/// row and not clamped to the grid, and whether the price lies on the row's bottom edge. A price
/// within rounding of an edge lies on it, in the row above, as where `fixed_grid` starts a grid.
/// Profiles bin volume with this, so a level of a profile ends at the prices in its row.
pub(crate) fn grid_row(low: f64, height: f64, price: f64) -> (f64, bool) {
    let row = (price - low) / height;
    // The difference rounds as its prices do, not as its own size.
    grid_edge(row, (price.abs() + low.abs()) / height)
        .map_or((row.floor(), false), |edge| (edge, true))
}

/// Zero-aligned rows `height` high over `low` to `high`, ending at the span's top row edge.
/// `flat_high` keeps the row holding the highest price traded without range. Rejects reversed,
/// nonfinite or unrepresentable spans and grids exceeding 10,000 rows.
fn fixed_grid(
    low: f64,
    high: f64,
    flat_high: Option<f64>,
    height: f64,
) -> Option<(f64, f64, usize)> {
    let span = high - low;
    if !height.is_finite() || height <= 0.0 || !span.is_finite() || span < 0.0 {
        return None;
    }
    let (first, top) = grid_rows(low / height, high / height);
    let top = flat_high.map_or(top, |price| top.max(grid_floor(price / height) + 1.0));
    let count = top - first;
    if !first.is_finite()
        || !top.is_finite()
        || first.abs() >= 2_f64.powi(52)
        || top.abs() >= 2_f64.powi(52)
        || !(1.0..=10_000.0).contains(&count)
    {
        return None;
    }
    let bounds = (first * height, top * height);
    (bounds.0.is_finite() && bounds.1.is_finite() && bounds.1 > bounds.0).then_some((
        bounds.0,
        bounds.1,
        count as usize,
    ))
}

/// Steps per row of an executed profile over the traded levels `first..=last`, levels `step`
/// apart: the row height in whole steps, at least one. With equal rows, the levels over the row
/// count, rounded up, when rows of that many levels, counted from its multiples as a footprint
/// groups them, stay within the row count; else the levels over one row fewer, which always do.
/// Asked for one row, levels that straddle a multiple keep two.
#[cfg(any(feature = "studies", feature = "drawings"))]
fn steps_per_row(config: &VolumeProfileConfig, step: f64, first: i64, last: i64) -> Option<i64> {
    if let Some(height) = config.row_size() {
        let steps = (height / step).round();
        return (steps < i64::MAX as f64).then(|| (steps as i64).max(1));
    }
    let (first, last, rows) = (i128::from(first), i128::from(last), config.rows() as i128);
    let count = |steps: i128| last.div_euclid(steps) - first.div_euclid(steps) + 1;
    let span = last - first + 1;
    let even = (span + rows - 1) / rows;
    let steps = if rows == 1 || count(even) <= rows {
        even
    } else {
        (span + rows - 2) / (rows - 1)
    };
    i64::try_from(steps).ok()
}

/// Lowest and highest key of the rows around the point of control that hold `target` volume.
/// Each step compares the next two rows above with the next two below and adds the heavier pair
/// one row at a time, stopping once the area holds `target`; equal sums add one row on each
/// side, and a side with no rows left gives way to the other. `row(i)` is the key and volume of
/// row `i` of `len`, at least one, in ascending key order, and `poc` the index of the point of
/// control. Keys between rows are untraded rows of zero volume, crossed in one step.
pub(crate) fn value_area(
    len: usize,
    row: impl Fn(usize) -> (i64, f64),
    poc: usize,
    target: f64,
) -> (i64, i64) {
    let key = |i: usize| i128::from(row(i).0);
    let (first, last) = (key(0), key(len - 1));
    // Rows `low..=high` lie inside the area, which spans the keys `bottom..=top`.
    let (mut low, mut high, mut bottom, mut top) = (poc, poc, key(poc), key(poc));
    let mut sum = row(poc).1;
    while sum < target && (bottom > first || top < last) {
        let (down, up) = (bottom > first, top < last);
        // Untraded rows hold no volume: until a traded row is within two keys of an edge, both
        // sides tie at zero and grow a row each, and a side growing alone adds nothing before
        // its next traded row.
        match (
            down.then(|| bottom - key(low - 1)),
            up.then(|| key(high + 1) - top),
        ) {
            (Some(gap_below), Some(gap_above)) => {
                let skip = (gap_below.min(gap_above) - 2).max(0);
                bottom -= skip;
                top += skip;
            }
            (Some(gap_below), None) => bottom -= gap_below - 1,
            (None, Some(gap_above)) => top += gap_above - 1,
            (None, None) => {}
        }
        let below: f64 = (low.saturating_sub(2)..low)
            .filter(|&i| key(i) >= bottom - 2)
            .map(|i| row(i).1)
            .sum();
        let above: f64 = (high + 1..len.min(high + 3))
            .filter(|&i| key(i) <= top + 2)
            .map(|i| row(i).1)
            .sum();
        let tie = down && up && below == above;
        let rising = up && (!down || above > below);
        let steps = if tie { 1 } else { 2 };
        for _ in 0..steps {
            // A step's rows join the sum together, so a tie's pair rounds as one addition.
            let mut added = 0.0;
            if (tie || !rising) && bottom > first {
                bottom -= 1;
                if key(low - 1) == bottom {
                    low -= 1;
                    added += row(low).1;
                }
            }
            if (tie || rising) && top < last {
                top += 1;
                if key(high + 1) == top {
                    high += 1;
                    added += row(high).1;
                }
            }
            sum += added;
            if sum >= target {
                break;
            }
        }
    }
    (bottom as i64, top as i64)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sample_bars() -> Vec<Bar> {
        [
            (10.0, 12.0, 9.0, 11.0, 100.0),
            (11.0, 13.0, 10.0, 12.0, 200.0),
            (12.0, 12.5, 11.0, 11.0, 150.0),
            (11.5, 14.0, 11.0, 13.5, 300.0),
            (13.5, 15.0, 13.0, 14.0, 250.0),
            (14.0, 14.5, 12.0, 12.5, 400.0),
            (13.0, 13.0, 13.0, 13.0, 50.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (o, h, l, c, v))| {
            let time = chrono::DateTime::from_timestamp(i as i64 * 60, 0).unwrap();
            Bar::new(time, o, h, l, c, v)
        })
        .collect()
    }
    fn assert_close(got: f64, want: f64) {
        assert!((got - want).abs() < 1e-9, "{got} != {want}");
    }
    /// Reference: rows of 1.5 from 9.0, each bar's volume split by how much of
    /// its range falls in the row, the zero-range bar whole in the row of 13.0.
    /// For 70% the value area takes the two rows below the POC (433.3) over
    /// the one row above it (397.5); comparing single rows would take the row
    /// above.
    #[test]
    fn volume_profile_matches_reference() {
        let profile = VolumeProfile::of(&sample_bars(), 4, None).unwrap();
        for (price, representable) in [(13.0, true), (f64::MAX, false), (-f64::MAX, false)] {
            let flat = Bar::new(
                chrono::DateTime::UNIX_EPOCH,
                price,
                price,
                price,
                price,
                50.0,
            );
            let flat_profile = VolumeProfile::of(&[flat], 4, None);
            assert_eq!(flat_profile.is_some(), representable, "flat price {price}");
            if let Some(flat_profile) = flat_profile {
                assert_eq!(flat_profile.volume(), [50.0]);
                assert_eq!(flat_profile.up(), [50.0]);
            }
        }
        assert_close(profile.low, 9.0);
        assert_close(profile.row_height, 1.5);
        for (got, want) in
            profile
                .volume
                .iter()
                .zip([83.33333333333334, 350.0, 619.1666666666667, 397.5])
        {
            assert_close(*got, want);
        }
        // The down bars are 150 over 11..12.5 and 400 over 12..14.5.
        for (row, (up, down)) in [
            (250.0 / 3.0, 0.0),
            (250.0, 100.0),
            (3950.0 / 12.0, 290.0),
            (237.5, 160.0),
        ]
        .into_iter()
        .enumerate()
        {
            assert_close(profile.up[row], up);
            assert_close(profile.down[row], down);
            assert_close(profile.up[row] + profile.down[row], profile.volume[row]);
        }
        for (share, value_area) in [(0.5, (1, 2)), (0.7, (0, 2)), (1.0, (0, 3))] {
            assert_eq!(
                profile.poc_and_value_area(share),
                Some((2, value_area)),
                "share {share}"
            );
        }
    }

    /// Equal-volume POCs take the lowest row, and both builders grow the value area by one rule:
    /// the heavier of the next two rows above and the next two below, one row on each side when
    /// they tie, with untraded rows at zero. Executed rows are centred on their price: the trade
    /// at 99 with a tick of 1 spans 98.5 to 99.5.
    #[test]
    fn estimated_and_executed_profiles_agree_on_poc_and_value_area() {
        use crate::data::Footprint;
        use crate::model::{BarClock, OrderSide, Tick, Timeframe};
        let time = chrono::DateTime::UNIX_EPOCH;
        // One bar of 24 over 24 rows from 99, and unit-high bars from 99 holding each row's volume.
        let single = vec![Bar::new(time, 99.0, 123.0, 99.0, 123.0, 24.0)];
        let unit_bars = |volumes: &[f64]| -> Vec<Bar> {
            volumes
                .iter()
                .enumerate()
                .map(|(i, &volume)| {
                    let low = 99.0 + i as f64;
                    Bar::new(time, low, low + 1.0, low, low + 1.0, volume)
                })
                .collect()
        };
        let peak = vec![1.0, 1.0, 2.0, 1.0, 1.0];
        let pairs = vec![5.0, 5.0, 6.0, 10.0, 4.0, 4.0];
        let gaps = vec![5.0, 3.0, 0.0, 0.0, 0.0, 10.0, 0.0, 0.0, 4.0, 4.0];
        for (bars, volumes, share, expected) in [
            // 70% of 24 needs 17 unit rows.
            (single, vec![1.0; 24], 0.7, (0, (0, 16))),
            // 60% of 6 needs the POC's 2 plus both tied 1s.
            (unit_bars(&peak), peak, 0.6, (2, (1, 3))),
            // 70% of 34 is 23.8. Below the POC at 102 the two rows hold 11 against 8 above,
            // giving 21; then the two above hold 8 against 5 below, and 103 brings 25. The value
            // area runs from 100 to 103.
            (unit_bars(&pairs), pairs, 0.7, (3, (1, 4))),
            // 50% of 26 is 13. The untraded rows on either side of the POC tie at zero and come
            // in together; then the two rows above hold 4 against 0, and the second brings 14.
            (unit_bars(&gaps), gaps, 0.5, (5, (4, 8))),
        ] {
            let estimated = VolumeProfile::of(&bars, volumes.len(), None).unwrap();
            let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
            for (i, &volume) in volumes.iter().enumerate() {
                flow.push(&Tick::new(
                    time,
                    99.0 + i as f64,
                    volume,
                    Some(OrderSide::Buy),
                ))
                .unwrap();
            }
            let executed =
                VolumeProfile::from_executions(flow.bar_at(time).unwrap(), 1.0, 1.0, true).unwrap();
            assert_eq!(estimated.volume(), executed.volume());
            assert_eq!(
                (executed.low(), executed.high()),
                (98.5, 98.5 + volumes.len() as f64)
            );
            assert_eq!(
                (
                    estimated.poc_and_value_area(share),
                    executed.poc_and_value_area(share)
                ),
                (Some(expected), Some(expected)),
                "rows {volumes:?}, share {share}"
            );
        }
    }

    /// A flat span's padding is not its traded range: even a bar inside it needs a new grid.
    #[test]
    fn with_bar_rebuilds_when_a_flat_span_gains_range() {
        let time = chrono::DateTime::UNIX_EPOCH;
        for price in [0.000_02, 0.5, 2.0, 100.0] {
            for rows in [1, 24] {
                let flat = Bar::new(time, price, price, price, price, 100.0);
                let profile = VolumeProfile::of(std::slice::from_ref(&flat), rows, None).unwrap();
                assert_eq!(
                    profile.with_bar(&flat),
                    VolumeProfile::of(&[flat.clone(), flat.clone()], rows, None)
                );
                let range = (price.abs() * 1e-6).max(1e-6) * 0.4;
                let next = Bar::new(time, price, price + range, price - range, price, 100.0);
                assert!(next.low > profile.low() && next.high < profile.high());
                assert!(
                    profile.with_bar(&next).is_none(),
                    "price {price}, rows {rows}"
                );
                // A different flat price also changes the span, even by one representable step.
                let moved = Bar::new(time, price, price.next_up(), price, price, 100.0);
                assert!(profile.with_bar(&moved).is_none());
                let bars = [flat.clone(), next.clone()];
                let rebuilt = VolumeProfile::of(&bars, rows, None).unwrap();
                assert_eq!(rebuilt.volume().len(), rows);
                assert_eq!((rebuilt.low(), rebuilt.high()), (next.low, next.high));
                assert_close(rebuilt.row_height(), 2.0 * range / rows as f64);
                // The flat bar lies in the middle row; the second bar spreads 100 evenly over all rows.
                assert!((rebuilt.volume()[rows / 2] - (100.0 + 100.0 / rows as f64)).abs() < 1e-6);
                let third = Bar::new(
                    time,
                    price,
                    price + range / 2.0,
                    price - range / 2.0,
                    price,
                    100.0,
                );
                let updated = rebuilt.with_bar(&third).unwrap();
                assert_eq!(
                    Some(updated.clone()),
                    VolumeProfile::of(&[flat, next, third], rows, None)
                );
                assert!(
                    (updated.volume()[rows / 2] - (100.0 + if rows == 1 { 200.0 } else { 12.5 }))
                        .abs()
                        < 1e-6
                );
            }
        }
        let flat = Bar::new(time, 2.0, 2.0, 2.0, 2.0, 0.0);
        let next = Bar::new(time, 1.0, 2.0, 1.0, 2.0, 4.0);
        let profile = VolumeProfile::of(std::slice::from_ref(&flat), 24, Some(1.0)).unwrap();
        assert!(profile.with_bar(&next).is_none());
        let rebuilt = VolumeProfile::of(&[flat, next], 24, Some(1.0)).unwrap();
        assert_eq!((rebuilt.low(), rebuilt.high()), (1.0, 2.0));
        assert_eq!(rebuilt.volume(), [4.0]);
    }

    /// Existing rows keep their prices and volume when a fixed grid grows at either edge.
    #[test]
    fn fixed_profile_grows_without_rebinning() {
        use crate::data::Footprint;
        use crate::model::{BarClock, OrderSide, Tick, Timeframe};

        let time = chrono::DateTime::UNIX_EPOCH;
        let first = Bar::new(time, -0.5, 1.0, -1.0, 0.5, 20.0);
        let mut bars = vec![first];
        let mut profile = VolumeProfile::of(&bars, 24, Some(1.0)).unwrap();
        assert_eq!(profile.volume, [10.0, 10.0]);
        for (low, high, volume, expected) in [
            (-2.0, -1.0, 6.0, vec![6.0, 10.0, 10.0]),
            (1.0, 2.0, 8.0, vec![6.0, 10.0, 10.0, 8.0]),
            (2.0, 2.0, 3.0, vec![6.0, 10.0, 10.0, 8.0, 3.0]),
        ] {
            let bar = Bar::new(time, low, high, low, high, volume);
            // Extension must succeed: a fallback rebuild would conceal the regression.
            profile = profile.with_bar(&bar).expect("extend fixed grid");
            bars.push(bar);
            assert_eq!(profile.volume, expected);
            assert_eq!(
                Some(profile.clone()),
                VolumeProfile::of(&bars, 24, Some(1.0))
            );
        }
        assert!(VolumeProfile::of(&bars, 24, Some(1e-12)).is_none());
        let wide = [
            Bar::new(time, -1e308, -5e307, -1e308, -5e307, 1.0),
            Bar::new(time, 5e307, 1e308, 5e307, 1e308, 1.0),
        ];
        assert!(VolumeProfile::of(&wide, 24, Some(1e308)).is_none());
        let wide_profile = VolumeProfile::of(&wide[..1], 24, Some(1e308)).unwrap();
        assert!(wide_profile.with_bar(&wide[1]).is_none());
        let boundary = Bar::new(time, 0.3, 0.3, 0.3, 0.3, 7.0);
        let decimal = VolumeProfile::of(&[boundary], 24, Some(0.1)).unwrap();
        assert_close(decimal.low(), 0.3);
        assert_eq!(decimal.volume(), [7.0]);
        // A price on an inner edge of cent rows lies in the row the grid starts there.
        let cents = [32.16, 32.18].map(|price| Bar::new(time, price, price, price, price, 1.0));
        let cents = VolumeProfile::of(&cents, 24, Some(0.01)).unwrap();
        assert_eq!(cents.volume(), [1.0, 0.0, 1.0]);
        // Executed rows start half a tick below traded levels, including when extended by bars.
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        for (price, size) in [(99.0, 7.0), (102.0, 3.0)] {
            flow.push(&Tick::new(time, price, size, Some(OrderSide::Buy)))
                .unwrap();
        }
        let executed =
            VolumeProfile::from_executions(flow.bar_at(time).unwrap(), 1.0, 1.0, true).unwrap();
        assert_eq!((executed.low(), executed.high()), (98.5, 102.5));
        assert_eq!(executed.volume(), [7.0, 0.0, 0.0, 3.0]);
        for (low, high, volume, bounds, expected) in [
            (100.0, 100.0, 0.0, (98.5, 102.5), vec![7.0, 0.0, 0.0, 3.0]),
            (102.5, 102.5, 0.0, (98.5, 102.5), vec![7.0, 0.0, 0.0, 3.0]),
            (
                102.5,
                102.5,
                5.0,
                (98.5, 103.5),
                vec![7.0, 0.0, 0.0, 3.0, 5.0],
            ),
            (
                102.5,
                103.5,
                5.0,
                (98.5, 103.5),
                vec![7.0, 0.0, 0.0, 3.0, 5.0],
            ),
            (
                97.5,
                98.5,
                5.0,
                (97.5, 102.5),
                vec![5.0, 7.0, 0.0, 0.0, 3.0],
            ),
        ] {
            let bar = Bar::new(
                time + chrono::TimeDelta::minutes(1),
                low,
                high,
                low,
                high,
                volume,
            );
            let appended = executed.with_bar(&bar).expect("extend executed grid");
            assert_eq!((appended.low(), appended.high()), bounds);
            assert_eq!(appended.volume(), expected);
            assert_eq!(appended.up(), expected);
            assert!(appended.down().iter().all(|&volume| volume == 0.0));
        }
        // A range ends at its upper edge; only volume traded at one price keeps the row above.
        for (high, flat_volume, rows, top, last, total) in [
            (11.0, None, 25, 11.0, 4.0, 100.0),
            (11.0_f64.next_down(), None, 25, 11.0, 4.0, 100.0),
            (11.0_f64.next_up(), None, 25, 11.0, 4.0, 100.0),
            (11.01, None, 26, 11.04, 100.0 * 0.01 / 1.01, 100.0),
            (11.0, Some(0.0), 25, 11.0, 4.0, 100.0),
            (11.0, Some(5.0), 26, 11.04, 5.0, 105.0),
        ] {
            let mut bars = vec![Bar::new(time, 10.0, high, 10.0, high, 100.0)];
            if let Some(volume) = flat_volume {
                bars.push(Bar::new(time, 11.0, 11.0, 11.0, 11.0, volume));
            }
            let profile = VolumeProfile::of(&bars, 25, Some(0.04)).unwrap();
            assert_eq!(
                profile.volume().len(),
                rows,
                "high {high}, flat {flat_volume:?}"
            );
            assert_close(profile.low(), 10.0);
            assert_close(profile.high(), top);
            assert_close(profile.volume()[rows - 1], last);
            assert_close(profile.volume().iter().sum(), total);
            let streamed = bars[1..].iter().fold(
                VolumeProfile::of(&bars[..1], 25, Some(0.04)).unwrap(),
                |profile, bar| profile.with_bar(bar).expect("extend fixed grid"),
            );
            assert_eq!(streamed, profile);
            #[cfg(feature = "studies")]
            {
                // Splitting the range in half gives the cumulative path the same density.
                let middle = (10.0 + high) / 2.0;
                let mut inputs = vec![
                    Bar::new(time, 10.0, middle, 10.0, middle, 50.0),
                    Bar::new(time, middle, high, middle, high, 50.0),
                ];
                inputs.extend_from_slice(&bars[1..]);
                let mut source = DevelopingVolume::default();
                source.add(&inputs[..1]);
                let mut developed = VolumeProfile::of(&inputs[..1], 25, Some(0.04)).unwrap();
                for end in 2..=inputs.len() {
                    source.add(&inputs[end - 1..end]);
                    developed
                        .extend_developing(
                            price_span(&inputs[..end]).unwrap(),
                            25,
                            Some(0.04),
                            &inputs[end - 1..end],
                            &source,
                        )
                        .unwrap();
                }
                assert_eq!(developed.volume().len(), rows);
                assert_close(developed.high(), top);
                source.fill(&mut developed).unwrap();
                for (&got, &want) in developed.volume().iter().zip(profile.volume()) {
                    assert_close(got, want);
                }
            }
        }
    }

    /// Four-unit rows centered at 100, 104 and 108 collect [98,102), [102,106)
    /// and [106,110). Totals are 90+50=140, 60+140=200 and 75+25=100.
    #[test]
    fn options_profile_groups_strikes_and_scales_by_largest_total() {
        use crate::data::OptionsVolume;
        let data: Vec<_> = [
            (99.0, 20.0, 10.0),
            (100.0, 10.0, 5.0),
            (100.0, 20.0, 15.0),
            (101.0, 40.0, 20.0),
            (102.0, 10.0, 40.0),
            (103.0, 20.0, 50.0),
            (104.0, 30.0, 50.0),
            (107.0, 75.0, 25.0),
        ]
        .into_iter()
        .map(|(strike, calls, puts)| OptionsVolume {
            strike,
            calls,
            puts,
        })
        .collect();
        let profile = VolumeProfile::from_options(&data, 4.0).unwrap();
        assert_eq!(profile.source(), ProfileSource::Options);
        assert_eq!(
            (profile.low(), profile.high(), profile.row_height()),
            (98.0, 110.0, 4.0)
        );
        let max = profile.volume().iter().copied().fold(0.0, f64::max);
        for (row, (calls, puts, share)) in
            [(90.0, 50.0, 0.7), (60.0, 140.0, 1.0), (75.0, 25.0, 0.5)]
                .into_iter()
                .enumerate()
        {
            assert_close(profile.up()[row], calls);
            assert_close(profile.down()[row], puts);
            assert_close(profile.volume()[row] / max, share);
        }
        #[cfg(feature = "widget")]
        for right_aligned in [false, true] {
            for direction in [-1.0, 1.0] {
                let config = VolumeProfileConfig {
                    display: ProfileDisplay::UpDown,
                    ..VolumeProfileConfig::default()
                };
                let rgba = |c: [u8; 4]| {
                    egui::Color32::from_rgba_unmultiplied(c[0], c[1], c[2], c[3])
                        .gamma_multiply(0.35)
                };
                let mut output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                    let painter = egui::Painter::new(
                        ui.ctx().clone(),
                        egui::LayerId::background(),
                        egui::Rect::EVERYTHING,
                    );
                    crate::painting::volume_profile::paint_profile(
                        &painter,
                        &profile,
                        config,
                        [0, 0, 255, 255],
                        100.0..300.0,
                        crate::painting::volume_profile::ProfileStyle::Study {
                            right_aligned,
                            strip: right_aligned,
                            walls_visible: [true; 2],
                            width: 0.9,
                            volume_decimals: None,
                        },
                        &|price| price as f32 * direction,
                    );
                });
                let shapes = std::mem::take(&mut output.shapes);
                output.drop_without_applying_deltas();
                let rows: Vec<_> = shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Rect(r)
                            if r.fill == rgba(config.up_color)
                                || r.fill == rgba(config.down_color) =>
                        {
                            Some(r)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(rows.len(), 6);
                // Of 180 available points, the rows use 126, 180 and 90; each segment
                // uses its own contract volume divided by the largest combined total, 200.
                for (row, (call_width, put_width, strike)) in [
                    (81.0, 45.0, 100.0),
                    (54.0, 126.0, 104.0),
                    (67.5, 22.5, 108.0),
                ]
                .into_iter()
                .enumerate()
                {
                    let (calls, puts) = (rows[2 * row], rows[2 * row + 1]);
                    assert!((calls.rect.width() - call_width).abs() < 1e-4);
                    assert!((puts.rect.width() - put_width).abs() < 1e-4);
                    assert!((calls.rect.top() - (strike * direction - 2.0)).abs() < 1e-4);
                    assert_eq!(calls.fill, rgba(config.up_color));
                    assert_eq!(puts.fill, rgba(config.down_color));
                    if right_aligned {
                        assert!((calls.rect.right() - 300.0).abs() < 1e-4);
                        assert!((puts.rect.right() - calls.rect.left()).abs() < 1e-4);
                    } else {
                        assert!((calls.rect.left() - 100.0).abs() < 1e-4);
                        assert!((puts.rect.left() - calls.rect.right()).abs() < 1e-4);
                    }
                }
            }
        }
        // Exact and decimal strikes stay at row centers; half-row boundaries go up.
        for (strike, size, center) in [
            (100.0, 1.0, 100.0),
            (0.3, 0.1, 0.3),
            (0.35, 0.1, 0.4),
            (-0.15, 0.1, -0.1),
        ] {
            let point = OptionsVolume {
                strike,
                calls: 3.0,
                puts: 7.0,
            };
            let p = VolumeProfile::from_options(&[point], size).unwrap();
            assert_close(p.low() + p.row_height() / 2.0, center);
            assert_eq!(p.volume(), [10.0]);
        }
        for bad in [f64::NAN, f64::INFINITY, -1.0] {
            let invalid = OptionsVolume {
                strike: 100.0,
                calls: bad,
                puts: 0.0,
            };
            assert!(VolumeProfile::from_options(&[invalid], 1.0).is_none());
            assert!(VolumeProfile::from_options(&data, bad).is_none());
        }
        assert!(VolumeProfile::from_options(&data, 0.0).is_none());
        assert!(VolumeProfile::from_options(&data, 1e-12).is_none());
        assert!(VolumeProfile::from_options(&[], 4.0).is_none());
        assert!(
            VolumeProfile::from_options(
                &[-1e308, 1e308].map(|strike| OptionsVolume {
                    strike,
                    calls: 1.0,
                    puts: 0.0,
                }),
                1e308,
            )
            .is_none()
        );
        assert!(
            VolumeProfile::from_options(
                &[OptionsVolume {
                    strike: 100.0,
                    calls: f64::MAX,
                    puts: f64::MAX
                }],
                1.0
            )
            .is_none()
        );
    }
}
