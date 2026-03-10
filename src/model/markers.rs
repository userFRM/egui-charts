/// Marker API for custom chart annotations
///
/// Markers are visual indicators that can be placed at specific data points on the chart.
/// Common use cases: buy/sell signals, events, annotations, alerts, etc.
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
