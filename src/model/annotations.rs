//! Chart annotations -- markers and labels at specific price/time coordinates.
//!
//! Annotations are persistent visual overlays (distinct from the transient
//! [`Marker`](super::Marker) API used for signal arrows). They are
//! serializable and typically managed through the chart's drawing tools.

/// Chart Annotations and Markers.
///
/// Allows placing visual markers and labels at specific price/time points on the chart.
/// Useful for highlighting important events, price levels, or analysis points.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt;

/// Shape of an annotation marker drawn on the chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum MarkerType {
    /// Circle marker
    #[default]
    Circle,
    /// Triangle pointing up
    TriangleUp,
    /// Triangle pointing down
    TriangleDown,
    /// Square marker
    Square,
    /// Diamond marker
    Diamond,
    /// Star marker
    Star,
    /// Arrow pointing up
    ArrowUp,
    /// Arrow pointing down
    ArrowDown,
}

impl MarkerType {
    /// Returns a human-readable name for this marker type.
    pub fn as_str(&self) -> &str {
        match self {
            MarkerType::Circle => "Circle",
            MarkerType::TriangleUp => "Triangle Up",
            MarkerType::TriangleDown => "Triangle Down",
            MarkerType::Square => "Square",
            MarkerType::Diamond => "Diamond",
            MarkerType::Star => "Star",
            MarkerType::ArrowUp => "Arrow Up",
            MarkerType::ArrowDown => "Arrow Down",
        }
    }

    /// Returns all available marker type variants.
    pub fn all() -> Vec<MarkerType> {
        vec![
            MarkerType::Circle,
            MarkerType::TriangleUp,
            MarkerType::TriangleDown,
            MarkerType::Square,
            MarkerType::Diamond,
            MarkerType::Star,
            MarkerType::ArrowUp,
            MarkerType::ArrowDown,
        ]
    }
}

impl fmt::Display for MarkerType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Position of the text label relative to the annotation marker.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AnnotationPos {
    /// Label above the marker.
    #[default]
    Above,
    /// Label below the marker.
    Below,
    /// Label to the left of the marker.
    Left,
    /// Label to the right of the marker.
    Right,
}

/// Chart annotation with marker and optional text label
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotation {
    /// Unique identifier
    pub id: usize,
    /// Ts for x-axis position
    pub ts: DateTime<Utc>,
    /// Price for y-axis position
    pub price: f64,
    /// Marker type
    pub marker: MarkerType,
    /// Optional text label
    pub text: Option<String>,
    /// Label position relative to marker
    pub position: AnnotationPos,
    /// Color in RGBA format
    pub color: [u8; 4],
    /// Marker size
    pub size: f32,
    /// Whether annotation is visible
    pub visible: bool,
}

impl Annotation {
    /// Creates a new annotation at the specified price and time
    pub fn new(id: usize, ts: DateTime<Utc>, price: f64) -> Self {
        Self {
            id,
            ts,
            price,
            marker: MarkerType::Circle,
            text: None,
            position: AnnotationPos::Above,
            color: [100, 149, 237, 255], // Cornflower blue
            size: 8.0,
            visible: true,
        }
    }

    /// Creates a new annotation with text label
    pub fn with_text(id: usize, ts: DateTime<Utc>, price: f64, text: String) -> Self {
        Self {
            id,
            ts,
            price,
            marker: MarkerType::Circle,
            text: Some(text),
            position: AnnotationPos::Above,
            color: [255, 255, 255, 255], // White
            size: 8.0,
            visible: true,
        }
    }

    /// Sets the marker type
    pub fn with_marker(mut self, marker: MarkerType) -> Self {
        self.marker = marker;
        self
    }

    /// Sets the color
    pub fn with_color(mut self, color: [u8; 4]) -> Self {
        self.color = color;
        self
    }

    /// Sets the marker size
    pub fn with_size(mut self, size: f32) -> Self {
        self.size = size;
        self
    }

    /// Sets the label position
    pub fn with_pos(mut self, position: AnnotationPos) -> Self {
        self.position = position;
        self
    }
}

impl fmt::Display for Annotation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Annotation[{}, {}, Price: {:.2}{}]",
            self.ts.format("%Y-%m-%d %H:%M:%S"),
            self.marker,
            self.price,
            self.text
                .as_ref()
                .map(|t| format!(", Text: {t}"))
                .unwrap_or_default()
        )
    }
}
