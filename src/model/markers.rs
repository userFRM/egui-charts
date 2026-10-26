//! Marker API for custom chart annotations
//!
//! Markers are visual indicators that can be placed at specific data points on the chart.
//! Common use cases: buy/sell signals, events, annotations, alerts, etc.

use crate::tokens::DESIGN_TOKENS;
use chrono::{DateTime, Utc};
use egui::Color32;
use serde::{Deserialize, Serialize};

/// Marker shape
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MarkerShape {
    /// Circle marker - filled circle
    #[default]
    Circle,
    /// Square marker - filled square
    Square,
    /// Arrow up marker - points upward
    ArrowUp,
    /// Arrow down marker - points downward
    ArrowDown,
    /// Triangle up marker - points upward (hollow)
    TriangleUp,
    /// Triangle down marker - points downward (hollow)
    TriangleDown,
    /// Diamond marker - filled diamond shape
    Diamond,
    /// Star marker
    Star,
    /// Cross marker (X shape)
    Cross,
    /// Flag marker - for marking important events
    Flag,
}

/// Marker position relative to the data point
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MarkerPos {
    /// Above the high price
    #[default]
    AboveBar,
    /// Below the low price
    BelowBar,
    /// At the data point (on the candle/bar)
    InBar,
    /// At the top of the chart pane
    Top,
    /// At the bottom of the chart pane
    Bottom,
    /// At the left edge of the chart
    Left,
    /// At the right edge of the chart
    Right,
    /// Absolute pixel position (up arrow)
    AbsoluteUp,
    /// Absolute pixel position (down arrow)
    AbsoluteDown,
    /// Absolute pixel position (centered)
    Absolute,
    /// Just above [`Marker::price`], the shape's lower edge on the price.
    AtPriceTop,
    /// Just below [`Marker::price`], the shape's upper edge on the price.
    AtPriceBottom,
    /// Centred on [`Marker::price`].
    AtPriceMiddle,
}

/// Where a series' markers are painted among the other content of the price pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MarkerZOrder {
    /// With their series: over it, under the series added after it and under the studies.
    #[default]
    Normal,
    /// Over every series and study, under the price lines and drawings.
    AboveSeries,
    /// Over the series, studies, price lines and drawings.
    Top,
}

/// How the markers of one series are fitted and layered; see `Chart::set_marker_options`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MarkerOptions {
    /// Whether the prices of the [`MarkerPos::AtPriceTop`], [`AtPriceBottom`](MarkerPos::AtPriceBottom)
    /// and [`AtPriceMiddle`](MarkerPos::AtPriceMiddle) markers in view widen the autoscaled price
    /// range, so that they are not cut off. Default true.
    pub auto_scale: bool,
    /// Default [`MarkerZOrder::Normal`].
    pub z_order: MarkerZOrder,
}

impl Default for MarkerOptions {
    fn default() -> Self {
        Self {
            auto_scale: true,
            z_order: MarkerZOrder::default(),
        }
    }
}

/// Marker for chart annotations
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Marker {
    /// Time of the marker (matches a data point)
    pub time: DateTime<Utc>,
    /// Pos relative to the bar
    pub position: MarkerPos,
    /// Shape of the marker
    pub shape: MarkerShape,
    /// Color of the marker
    #[serde(skip)]
    pub color: Color32,
    /// Optional text label
    pub text: Option<String>,
    /// Optional tooltip (shown on hover)
    pub tooltip: Option<String>,
    /// Size multiplier (1.0 = normal size)
    pub size: f32,
    /// Unique ID for the marker (for removal/editing)
    pub id: Option<String>,
    /// Price of the `AtPrice` positions, in the series' units; without one they use the close.
    pub price: Option<f64>,
}

impl Marker {
    /// Create a new marker at the given time
    pub fn new(time: DateTime<Utc>) -> Self {
        Self {
            time,
            position: MarkerPos::default(),
            shape: MarkerShape::default(),
            color: Color32::WHITE,
            text: None,
            tooltip: None,
            size: 1.0,
            id: None,
            price: None,
        }
    }

    /// Set the position
    #[must_use]
    pub fn with_pos(mut self, position: MarkerPos) -> Self {
        self.position = position;
        self
    }

    /// Set the shape
    #[must_use]
    pub fn with_shape(mut self, shape: MarkerShape) -> Self {
        self.shape = shape;
        self
    }

    /// Set the color
    #[must_use]
    pub fn with_color(mut self, color: Color32) -> Self {
        self.color = color;
        self
    }

    /// Set the text label
    #[must_use]
    pub fn with_text(mut self, text: impl Into<String>) -> Self {
        self.text = Some(text.into());
        self
    }

    /// Set the tooltip
    #[must_use]
    pub fn with_tooltip(mut self, tooltip: impl Into<String>) -> Self {
        self.tooltip = Some(tooltip.into());
        self
    }

    /// Set the size
    #[must_use]
    pub fn with_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Set the ID
    #[must_use]
    pub fn with_id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }

    /// Anchors the marker to `price` at `position`, one of the `AtPrice` positions.
    #[must_use]
    pub fn at_price(mut self, price: f64, position: MarkerPos) -> Self {
        self.price = Some(price);
        self.position = position;
        self
    }

    /// Quick constructor for a buy signal marker (green arrow up)
    pub fn buy_signal(time: DateTime<Utc>) -> Self {
        Self::new(time)
            .with_shape(MarkerShape::ArrowUp)
            .with_pos(MarkerPos::BelowBar)
            .with_color(DESIGN_TOKENS.semantic.extended.bullish)
            .with_text("BUY")
    }

    /// Quick constructor for a sell signal marker (red arrow down)
    pub fn sell_signal(time: DateTime<Utc>) -> Self {
        Self::new(time)
            .with_shape(MarkerShape::ArrowDown)
            .with_pos(MarkerPos::AboveBar)
            .with_color(DESIGN_TOKENS.semantic.extended.bearish)
            .with_text("SELL")
    }

    /// Quick constructor for an event marker (blue flag)
    pub fn event(time: DateTime<Utc>, text: impl Into<String>) -> Self {
        Self::new(time)
            .with_shape(MarkerShape::Flag)
            .with_pos(MarkerPos::AboveBar)
            .with_color(DESIGN_TOKENS.semantic.extended.info)
            .with_text(text)
    }
}

/// Colours and lifetime of the marks that show where the close of a series rose or fell
/// between live updates; see `Chart::set_change_markers`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChangeMarkers {
    /// Colour of a rise.
    pub up_color: Color32,
    /// Colour of a fall.
    pub down_color: Color32,
    /// How long a mark stays after the update that made it. Default 5 seconds.
    pub duration: std::time::Duration,
}

impl Default for ChangeMarkers {
    fn default() -> Self {
        Self {
            up_color: DESIGN_TOKENS.semantic.extended.bullish,
            down_color: DESIGN_TOKENS.semantic.extended.bearish,
            duration: std::time::Duration::from_secs(5),
        }
    }
}

/// Line style of drawings, series, grid and price lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash, Serialize, Deserialize)]
pub enum LineStyle {
    /// A continuous line.
    #[default]
    Solid,
    /// Short dashes.
    Dashed,
    /// Closely spaced dots.
    Dotted,
    /// Dashes twice as long as [`Dashed`](Self::Dashed), with twice the gap.
    LargeDashed,
    /// Dots with twice the spacing of [`Dotted`](Self::Dotted).
    SparseDotted,
}

impl LineStyle {
    /// Get the dash pattern for rendering
    /// Returns (dash_len, gap_len) or None for solid
    #[inline]
    pub fn dash_pattern(&self) -> Option<(f32, f32)> {
        match self {
            LineStyle::Solid => None,
            LineStyle::Dashed => Some((8.0, 4.0)),
            LineStyle::Dotted => Some((2.0, 3.0)),
            LineStyle::LargeDashed => Some((16.0, 8.0)),
            LineStyle::SparseDotted => Some((2.0, 8.0)),
        }
    }

    /// The dash pattern of one-pixel lines such as price, grid and crosshair lines, or `None`
    /// for solid. Their dots are one pixel long.
    #[cfg(feature = "widget")]
    #[inline]
    pub(crate) fn hairline_pattern(&self) -> Option<(f32, f32)> {
        match self {
            LineStyle::Solid => None,
            LineStyle::Dashed => Some((4.0, 4.0)),
            LineStyle::Dotted => Some((1.0, 3.0)),
            LineStyle::LargeDashed => Some((8.0, 8.0)),
            LineStyle::SparseDotted => Some((1.0, 7.0)),
        }
    }

    /// All available line styles
    pub fn all() -> &'static [LineStyle] {
        &[
            LineStyle::Solid,
            LineStyle::Dashed,
            LineStyle::Dotted,
            LineStyle::LargeDashed,
            LineStyle::SparseDotted,
        ]
    }

    /// The style's name in settings menus.
    pub fn name(&self) -> &'static str {
        match self {
            LineStyle::Solid => "Solid",
            LineStyle::Dashed => "Dashed",
            LineStyle::Dotted => "Dotted",
            LineStyle::LargeDashed => "Large dashed",
            LineStyle::SparseDotted => "Sparse dotted",
        }
    }
}

/// A horizontal line at a price on a series' scale, with an optional title and a price axis label.
#[derive(Clone, Debug, PartialEq)]
pub struct PriceLine {
    /// Price in the series' units.
    pub price: f64,
    /// Colour of the line, title and axis tag.
    pub color: Color32,
    /// Width in points. Default 1.0.
    pub width: f32,
    /// Default [`LineStyle::Dashed`].
    pub style: LineStyle,
    /// Drawn at the left end of the line, above it.
    pub title: Option<String>,
    /// Price label on the axis, on a tag in the line's colour. Default true.
    pub axis_label: bool,
    /// Fill of the axis tag; `None` uses [`color`](Self::color).
    pub axis_label_color: Option<Color32>,
    /// Text of the axis tag; `None` picks white or black, whichever reads on the fill.
    pub axis_label_text_color: Option<Color32>,
    /// Colour of the title; `None` uses [`color`](Self::color).
    pub title_color: Option<Color32>,
}

impl PriceLine {
    /// Creates a dashed price line with an axis label.
    pub fn new(price: f64, color: Color32) -> Self {
        Self {
            price,
            color,
            width: 1.0,
            style: LineStyle::Dashed,
            title: None,
            axis_label: true,
            axis_label_color: None,
            axis_label_text_color: None,
            title_color: None,
        }
    }

    /// Sets the width in points.
    #[must_use]
    pub fn with_width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }

    /// Sets the line style.
    #[must_use]
    pub fn with_style(mut self, style: LineStyle) -> Self {
        self.style = style;
        self
    }

    /// Sets the title drawn just above the line's left end.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Sets whether to draw the price axis label.
    #[must_use]
    pub fn with_axis_label(mut self, show: bool) -> Self {
        self.axis_label = show;
        self
    }

    /// Sets the fill and the text colour of the axis tag.
    #[must_use]
    pub fn with_axis_label_colors(mut self, fill: Color32, text: Color32) -> Self {
        self.axis_label_color = Some(fill);
        self.axis_label_text_color = Some(text);
        self
    }

    /// Sets the colour of the title.
    #[must_use]
    pub fn with_title_color(mut self, color: Color32) -> Self {
        self.title_color = Some(color);
        self
    }
}
