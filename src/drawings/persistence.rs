//! Drawing persistence -- JSON serialization and storage.
//!
//! This module provides types for serializing drawings to and from JSON. The key
//! design decision is that coordinates are stored as `(timestamp, price)` pairs
//! (`StoredCoord`) rather than screen pixel positions, so drawings survive
//! chart pan/zoom and resolution changes.
//!
//! # Conversion flow
//!
//! ```text
//! Drawing (in-memory)                StoredDrawing (serializable)
//! ─────────────────                  ────────────────────────────
//! points: Vec<Pos2>   ──────────►   points: Vec<StoredCoord>
//! (screen pixels)      drawing_to_stored()  (timestamp + price)
//!
//! points: Vec<Pos2>   ◄──────────   points: Vec<StoredCoord>
//! (screen pixels)      stored_to_drawing()  (timestamp + price)
//! ```
//!
//! # Storage format
//!
//! Drawings are grouped by symbol into a `DrawingStorage` container that
//! includes a version number for forward-compatible migration, metadata, and an
//! optional timeframe filter.

use crate::drawings::{Drawing, DrawingToolType};
use egui::Color32;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A coordinate stored as a `(timestamp, price)` pair for persistence.
///
/// Unlike screen-space coordinates ([`Pos2`](egui::Pos2)), these values are
/// independent of chart pan/zoom and resolution, making them suitable for
/// long-term storage and cross-device synchronization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCoord {
    /// Timestamp in Unix epoch milliseconds.
    pub ts_ms: i64,
    /// Price level at this coordinate.
    pub price: f64,
}

impl StoredCoord {
    /// Creates a new stored coordinate from a timestamp and price.
    pub fn new(ts_ms: i64, price: f64) -> Self {
        Self { ts_ms, price }
    }

    /// Creates a stored coordinate by converting screen-space `(x, y)` to
    /// `(timestamp, price)` using the provided conversion closures.
    pub fn from_screen<F, G>(x: f64, y: f64, x_to_time: F, y_to_price: G) -> Self
    where
        F: Fn(f64) -> i64,
        G: Fn(f64) -> f64,
    {
        Self {
            ts_ms: x_to_time(x),
            price: y_to_price(y),
        }
    }

    /// Converts this stored coordinate back to screen-space `(x, y)` using the
    /// provided conversion closures.
    pub fn to_screen<F, G>(&self, time_to_x: F, price_to_y: G) -> (f64, f64)
    where
        F: Fn(i64) -> f64,
        G: Fn(f64) -> f64,
    {
        (time_to_x(self.ts_ms), price_to_y(self.price))
    }
}

/// A serializable RGBA color.
///
/// Converts to/from [`Color32`] and `[u8; 4]` via `From` implementations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredColor {
    /// Red channel (0--255).
    pub r: u8,
    /// Green channel (0--255).
    pub g: u8,
    /// Blue channel (0--255).
    pub b: u8,
    /// Alpha channel (0--255, where 255 is fully opaque).
    pub a: u8,
}

impl From<Color32> for StoredColor {
    fn from(color: Color32) -> Self {
        Self {
            r: color.r(),
            g: color.g(),
            b: color.b(),
            a: color.a(),
        }
    }
}

impl From<[u8; 4]> for StoredColor {
    fn from(color: [u8; 4]) -> Self {
        Self {
            r: color[0],
            g: color[1],
            b: color[2],
            a: color[3],
        }
    }
}

impl From<StoredColor> for Color32 {
    fn from(color: StoredColor) -> Self {
        Color32::from_rgba_unmultiplied(color.r, color.g, color.b, color.a)
    }
}

impl From<StoredColor> for [u8; 4] {
    fn from(color: StoredColor) -> Self {
        [color.r, color.g, color.b, color.a]
    }
}

/// A fully serializable representation of a drawing with all its properties.
///
/// This is the storage/wire format for drawings. It uses string-based tool type
/// names and `(timestamp, price)` coordinates rather than the in-memory
/// [`Drawing`] type which uses enum variants and screen pixels. Convert between
/// the two with [`drawing_to_stored`] and [`stored_to_drawing`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredDrawing {
    /// Unique identifier (string for backend compatibility).
    pub id: String,
    /// Drawing tool type as a string (e.g., `"Trend Line"`, `"Fib Retracement"`).
    pub tool_type: String,
    /// Anchor points in `(timestamp, price)` coordinates.
    pub points: Vec<StoredCoord>,
    /// Line/stroke color.
    pub color: StoredColor,
    /// Line/stroke width in pixels.
    pub line_width: f32,
    /// Whether the drawing is selected
    pub selected: bool,
    /// Whether the drawing is locked
    pub locked: bool,
    /// Whether the drawing is visible
    pub visible: bool,
    /// Custom properties (tool-specific)
    pub properties: HashMap<String, StoredValue>,
    /// Creation ts
    pub created_at: i64,
    /// Last modified ts
    pub modified_at: i64,
    /// User notes
    pub notes: Option<String>,
}

/// Generic stored value for custom properties
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StoredValue {
    Bool(bool),
    Int(i64),
    Float(f64),
    String(String),
    Array(Vec<StoredValue>),
}

impl StoredDrawing {
    /// Create a new stored drawing
    pub fn new(id: impl Into<String>, tool_type: impl Into<String>) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id: id.into(),
            tool_type: tool_type.into(),
            points: Vec::new(),
            color: StoredColor::from(Color32::WHITE),
            line_width: 1.0,
            selected: false,
            locked: false,
            visible: true,
            properties: HashMap::new(),
            created_at: now,
            modified_at: now,
            notes: None,
        }
    }

    /// Add a point
    pub fn add_point(&mut self, point: StoredCoord) {
        self.points.push(point);
        self.modified_at = chrono::Utc::now().timestamp_millis();
    }

    /// Set a custom property
    pub fn set_property(&mut self, key: impl Into<String>, value: StoredValue) {
        self.properties.insert(key.into(), value);
        self.modified_at = chrono::Utc::now().timestamp_millis();
    }

    /// Get a custom property
    pub fn get_property(&self, key: &str) -> Option<&StoredValue> {
        self.properties.get(key)
    }
}

/// Drawing storage for saving/loading drawings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrawingStorage {
    /// Version for migration support
    pub version: u32,
    /// Symbol this storage is for
    pub symbol: String,
    /// Timeframe (optional)
    pub timeframe: Option<String>,
    /// All stored drawings
    pub drawings: Vec<StoredDrawing>,
    /// Metadata
    pub metadata: StorageMetadata,
}

/// Storage metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StorageMetadata {
    /// When the storage was created
    pub created_at: i64,
    /// When the storage was last modified
    pub modified_at: i64,
    /// Application version that created this
    pub app_version: Option<String>,
}

impl Default for StorageMetadata {
    fn default() -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            created_at: now,
            modified_at: now,
            app_version: None,
        }
    }
}

impl DrawingStorage {
    /// Current storage format version
    pub const CURRENT_VERSION: u32 = 1;

    /// Create new storage for a symbol
    pub fn new(symbol: impl Into<String>) -> Self {
        Self {
            version: Self::CURRENT_VERSION,
            symbol: symbol.into(),
            timeframe: None,
            drawings: Vec::new(),
            metadata: StorageMetadata::default(),
        }
    }

    /// Set timeframe
    pub fn with_timeframe(mut self, timeframe: impl Into<String>) -> Self {
        self.timeframe = Some(timeframe.into());
        self
    }

    /// Add a drawing
    pub fn add_drawing(&mut self, drawing: StoredDrawing) {
        self.drawings.push(drawing);
        self.metadata.modified_at = chrono::Utc::now().timestamp_millis();
    }

    /// Remove a drawing by ID
    pub fn remove_drawing(&mut self, id: &str) -> bool {
        let len_before = self.drawings.len();
        self.drawings.retain(|d| d.id != id);
        if self.drawings.len() != len_before {
            self.metadata.modified_at = chrono::Utc::now().timestamp_millis();
            true
        } else {
            false
        }
    }

    /// Get a drawing by ID
    pub fn get_drawing(&self, id: &str) -> Option<&StoredDrawing> {
        self.drawings.iter().find(|d| d.id == id)
    }

    /// Get a mutable drawing by ID
    pub fn get_drawing_mut(&mut self, id: &str) -> Option<&mut StoredDrawing> {
        self.drawings.iter_mut().find(|d| d.id == id)
    }

    /// Get all drawings
    pub fn drawings(&self) -> &[StoredDrawing] {
        &self.drawings
    }

    /// Serialize to JSON
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Deserialize from JSON
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let mut storage: Self = serde_json::from_str(json)?;
        // Handle version migrations if needed
        storage.migrate();
        Ok(storage)
    }

    /// Migrate from older versions if needed
    fn migrate(&mut self) {
        // Future: add migration logic when version changes
        self.version = Self::CURRENT_VERSION;
    }

    /// Clear all drawings
    pub fn clear(&mut self) {
        self.drawings.clear();
        self.metadata.modified_at = chrono::Utc::now().timestamp_millis();
    }
}

/// Convert a Drawing to StoredDrawing
pub fn drawing_to_stored<F, G>(drawing: &Drawing, x_to_time: F, y_to_price: G) -> StoredDrawing
where
    F: Fn(f64) -> i64,
    G: Fn(f64) -> f64,
{
    let mut stored = StoredDrawing::new(drawing.id.to_string(), drawing.tool_type.as_str());

    for point in &drawing.points {
        stored.add_point(StoredCoord::from_screen(
            point.x as f64,
            point.y as f64,
            &x_to_time,
            &y_to_price,
        ));
    }

    stored.color = drawing.color.into();
    stored.line_width = drawing.stroke_width;

    // Store tool-specific properties
    match drawing.tool_type {
        DrawingToolType::FibonacciRetracement => {
            stored.set_property("show_levels", StoredValue::Bool(true));
        }
        DrawingToolType::Measure => {
            stored.set_property("show_measurement", StoredValue::Bool(true));
        }
        _ => {}
    }

    // Store text for text labels
    if let Some(ref text) = drawing.text {
        stored.set_property("text", StoredValue::String(text.clone()));
    }

    stored
}

/// Convert StoredDrawing back to Drawing
pub fn stored_to_drawing<F, G>(
    stored: &StoredDrawing,
    time_to_x: F,
    price_to_y: G,
    next_id: usize,
) -> Option<Drawing>
where
    F: Fn(i64) -> f64,
    G: Fn(f64) -> f64,
{
    let tool_type = stored.tool_type.parse::<DrawingToolType>().ok()?;

    let points: Vec<egui::Pos2> = stored
        .points
        .iter()
        .map(|p| {
            let (x, y) = p.to_screen(&time_to_x, &price_to_y);
            egui::Pos2::new(x as f32, y as f32)
        })
        .collect();

    let mut drawing = Drawing::new(next_id, tool_type);
    drawing.points = points;
    drawing.color = stored.color.clone().into();
    drawing.stroke_width = stored.line_width;
    drawing.completed = true;

    // Restore text for text labels
    if let Some(StoredValue::String(text)) = stored.get_property("text") {
        drawing.text = Some(text.clone());
    }

    Some(drawing)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stored_coord() {
        let coord = StoredCoord::new(1704067200000, 100.50);
        assert_eq!(coord.ts_ms, 1704067200000);
        assert!((coord.price - 100.50).abs() < 0.01);
    }

    #[test]
    fn test_stored_color_conversion() {
        let color = Color32::from_rgb(255, 128, 64);
        let stored: StoredColor = color.into();
        let back: Color32 = stored.into();
        assert_eq!(color, back);
    }

    #[test]
    fn test_drawing_storage() {
        let mut storage = DrawingStorage::new("AAPL");

        let mut drawing = StoredDrawing::new("1", "TrendLine");
        drawing.add_point(StoredCoord::new(1704067200000, 100.0));
        drawing.add_point(StoredCoord::new(1704153600000, 110.0));

        storage.add_drawing(drawing);

        assert_eq!(storage.drawings().len(), 1);
        assert!(storage.get_drawing("1").is_some());
    }

    #[test]
    fn test_json_serialization() {
        let mut storage = DrawingStorage::new("AAPL");

        let mut drawing = StoredDrawing::new("1", "TrendLine");
        drawing.add_point(StoredCoord::new(1704067200000, 100.0));
        drawing.set_property("extended", StoredValue::Bool(true));

        storage.add_drawing(drawing);

        let json = storage.to_json().unwrap();
        let loaded = DrawingStorage::from_json(&json).unwrap();

        assert_eq!(loaded.symbol, "AAPL");
        assert_eq!(loaded.drawings().len(), 1);
    }

    #[test]
    fn test_remove_drawing() {
        let mut storage = DrawingStorage::new("AAPL");
        storage.add_drawing(StoredDrawing::new("1", "TrendLine"));
        storage.add_drawing(StoredDrawing::new("2", "HorizontalLine"));

        assert!(storage.remove_drawing("1"));
        assert_eq!(storage.drawings().len(), 1);
        assert!(!storage.remove_drawing("1")); // Already removed
    }
}
