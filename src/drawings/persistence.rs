//! Drawing persistence -- JSON serialization and storage.
//!
//! This module provides types for serializing drawings to and from JSON. The key
//! design decision is that anchors are stored as `(timestamp, price)` pairs
//! (`StoredCoord`) rather than bar indices or screen positions, so drawings
//! survive pan/zoom, resolution changes and history being prepended.
//!
//! # Conversion flow
//!
//! ```text
//! Drawing (in-memory)                    StoredDrawing (serializable)
//! ─────────────────                      ────────────────────────────
//! chart_points: Vec<ChartPoint> ──────►  points: Vec<StoredCoord>
//! (bar index + price)  drawing_to_stored()  (timestamp + price)
//! tool type and settings        ──────►  drawing: Drawing
//!
//! chart_points: Vec<ChartPoint> ◄──────  points: Vec<StoredCoord>
//!                      stored_to_drawing()
//! ```
//!
//! Screen points are not stored: [`Drawing::update_screen_coords`] rebuilds
//! them from the anchors on the next frame.
//!
//! # Storage format
//!
//! Drawings are grouped by symbol into a `DrawingStorage` container that
//! includes a format version, metadata, and an optional timeframe filter.
//! Loading migrates version 1 files, rejects versions it does not know and
//! skips any drawing it cannot read
//! (see [`from_json`](crate::drawings::persistence::DrawingStorage::from_json)).

use crate::drawings::{ChartPoint, Drawing};
use serde::{Deserialize, Serialize};

/// A coordinate stored as a `(timestamp, price)` pair for persistence.
///
/// Unlike screen-space coordinates ([`Pos2`](egui::Pos2)) and bar indices,
/// these values are independent of chart pan/zoom and of the loaded history,
/// making them suitable for long-term storage and cross-device synchronization.
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
}

/// A fully serializable representation of a drawing with all its properties.
///
/// This is the storage/wire format for drawings: the anchors as
/// `(timestamp, price)` coordinates and the [`Drawing`] itself for the tool
/// type and every setting. Convert between the two with [`drawing_to_stored`]
/// and [`stored_to_drawing`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredDrawing {
    /// Unique identifier (string for backend compatibility).
    pub id: String,
    /// Anchor points in `(timestamp, price)` coordinates.
    pub points: Vec<StoredCoord>,
    /// Tool type and settings; its id, points and completion state are not stored.
    pub drawing: Drawing,
    /// Creation ts
    pub created_at: i64,
    /// Last modified ts
    pub modified_at: i64,
    /// User notes
    pub notes: Option<String>,
}

impl StoredDrawing {
    /// Create a new stored drawing without anchor points
    pub fn new(id: impl Into<String>, drawing: Drawing) -> Self {
        let now = chrono::Utc::now().timestamp_millis();
        Self {
            id: id.into(),
            points: Vec::new(),
            drawing,
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
}

/// Drawing storage for saving/loading drawings
///
/// It is read only through [`Self::from_json`], which checks the version,
/// migrates older files and loads each drawing on its own.
#[derive(Debug, Clone, Serialize)]
pub struct DrawingStorage {
    /// Format version, [`Self::CURRENT_VERSION`] once loaded
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
    pub const CURRENT_VERSION: u32 = 2;

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

    /// Deserialize from JSON, migrating version 1 to the current format.
    ///
    /// Fails as a whole on invalid JSON, on a version this release does not
    /// know and on a missing or invalid symbol, timeframe or metadata. A
    /// drawing that cannot be read (a missing field, a `null` written for a
    /// non-finite number, a tool type from a newer release or a setting out of
    /// bounds) is left out and returned as a [`SkippedDrawing`], so one bad
    /// drawing does not lose the others. Saving the returned storage writes
    /// only the drawings that loaded; keep the original file if the skipped
    /// ones must be recovered.
    pub fn from_json(json: &str) -> Result<(Self, Vec<SkippedDrawing>), serde_json::Error> {
        #[derive(Deserialize)]
        struct Version {
            version: u32,
        }
        #[derive(Deserialize)]
        struct Container {
            symbol: String,
            timeframe: Option<String>,
            drawings: Vec<serde_json::Value>,
            metadata: StorageMetadata,
        }
        let load: fn(serde_json::Value) -> Result<StoredDrawing, serde_json::Error> =
            match serde_json::from_str::<Version>(json)?.version {
                1 => v1::load,
                Self::CURRENT_VERSION => serde_json::from_value,
                version => {
                    return Err(serde::de::Error::custom(format!(
                        "unsupported drawing storage version {version}, expected 1 to {}",
                        Self::CURRENT_VERSION
                    )));
                }
            };
        let container: Container = serde_json::from_str(json)?;
        let mut skipped = Vec::new();
        let drawings = container
            .drawings
            .into_iter()
            .filter_map(|value| {
                let id = value.get("id").and_then(|id| id.as_str()).map(String::from);
                load(value)
                    .map_err(|error| skipped.push(SkippedDrawing { id, error }))
                    .ok()
            })
            .collect();
        let storage = Self {
            version: Self::CURRENT_VERSION,
            symbol: container.symbol,
            timeframe: container.timeframe,
            drawings,
            metadata: container.metadata,
        };
        Ok((storage, skipped))
    }

    /// Clear all drawings
    pub fn clear(&mut self) {
        self.drawings.clear();
        self.metadata.modified_at = chrono::Utc::now().timestamp_millis();
    }
}

/// A drawing [`DrawingStorage::from_json`] left out because it could not be read.
#[derive(Debug)]
pub struct SkippedDrawing {
    /// The stored id, or `None` if the entry has no id or a non-string one.
    pub id: Option<String>,
    /// Why the drawing could not be read.
    pub error: serde_json::Error,
}

/// The version 1 format, read only to migrate it: the tool by display name,
/// the colour, width, lock, visibility and text, and nothing else.
mod v1 {
    use super::StoredCoord;
    use crate::drawings::{Drawing, DrawingToolType};
    use serde::Deserialize;
    use serde::de::Error;
    use std::collections::HashMap;

    #[derive(Deserialize)]
    struct StoredDrawing {
        id: String,
        tool_type: String,
        points: Vec<StoredCoord>,
        color: Color,
        line_width: f32,
        locked: bool,
        visible: bool,
        properties: HashMap<String, serde_json::Value>,
        created_at: i64,
        modified_at: i64,
        notes: Option<String>,
    }

    #[derive(Deserialize)]
    struct Color {
        r: u8,
        g: u8,
        b: u8,
        a: u8,
    }

    /// Reads one version 1 drawing as a current one.
    pub(super) fn load(
        value: serde_json::Value,
    ) -> Result<super::StoredDrawing, serde_json::Error> {
        let stored: StoredDrawing = serde_json::from_value(value)?;
        // Cursors share display names with drawing tools ("Arrow") and are never stored.
        // The previous loader also accepted the names `FromStr` parses, such as "trendline".
        let tool_type = DrawingToolType::all()
            .iter()
            .find(|tool| !tool.is_cursor() && tool.as_str() == stored.tool_type)
            .copied()
            .or_else(|| {
                stored
                    .tool_type
                    .parse()
                    .ok()
                    .filter(|tool: &DrawingToolType| !tool.is_cursor())
            })
            .ok_or_else(|| {
                serde_json::Error::custom(format!("unknown drawing tool {:?}", stored.tool_type))
            })?;
        let Color { r, g, b, a } = stored.color;
        let mut drawing = Drawing::new(0, tool_type);
        drawing.color = [r, g, b, a];
        drawing.stroke_width = stored.line_width;
        drawing.locked = stored.locked;
        drawing.visible = stored.visible;
        drawing.text = stored
            .properties
            .get("text")
            .and_then(|text| text.as_str())
            .map(String::from);
        Ok(super::StoredDrawing {
            id: stored.id,
            points: stored.points,
            drawing,
            created_at: stored.created_at,
            modified_at: stored.modified_at,
            notes: stored.notes,
        })
    }
}

/// Convert a Drawing to StoredDrawing, its anchors mapped from bar index to timestamp.
///
/// `bar_to_time` must accept fractional bar indices and extrapolate by the bar
/// interval before index 0 and after the last bar instead of clamping: anchors
/// can sit between bars, in the history not yet loaded or in the future area,
/// and a clamping mapping would move them onto the first or last bar. It must
/// be the inverse of the `time_to_bar` passed to [`stored_to_drawing`].
pub fn drawing_to_stored(drawing: &Drawing, bar_to_time: impl Fn(f32) -> i64) -> StoredDrawing {
    let mut settings = drawing.clone();
    settings.points.clear();
    settings.chart_points.clear();
    let mut stored = StoredDrawing::new(drawing.id.to_string(), settings);
    stored.points = drawing
        .chart_points
        .iter()
        .map(|p| StoredCoord::new(bar_to_time(p.bar_idx), p.price))
        .collect();
    stored
}

/// Convert StoredDrawing back to a completed Drawing, its anchors mapped from
/// timestamp to bar index; the screen points follow on the next
/// [`Drawing::update_screen_coords`].
///
/// `time_to_bar` must be the inverse of the `bar_to_time` passed to
/// [`drawing_to_stored`]: it returns fractional bar indices and extrapolates
/// before the first and after the last bar instead of clamping, or anchors
/// outside the loaded bars collapse onto the edge bar.
pub fn stored_to_drawing(
    stored: &StoredDrawing,
    time_to_bar: impl Fn(i64) -> f32,
    next_id: usize,
) -> Drawing {
    let mut drawing = stored.drawing.clone();
    drawing.id = next_id;
    drawing.chart_points = stored
        .points
        .iter()
        .map(|p| ChartPoint::new(time_to_bar(p.ts_ms), p.price))
        .collect();
    drawing.completed = true;
    drawing
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::*;

    const T0: i64 = 1_704_067_200_000;

    fn bar_to_time(bar: f32) -> i64 {
        T0 + (bar * 60_000.0) as i64
    }

    fn time_to_bar(ts_ms: i64) -> f32 {
        (ts_ms - T0) as f32 / 60_000.0
    }

    fn bar_to_x(bar: f32) -> f32 {
        100.0 + bar * 8.0
    }

    fn price_to_y(price: f64) -> f32 {
        (500.0 - price * 2.0) as f32
    }

    fn restore(stored: &StoredDrawing, id: usize) -> Drawing {
        let mut drawing = stored_to_drawing(stored, time_to_bar, id);
        drawing.update_screen_coords(bar_to_x, price_to_y);
        drawing
    }

    /// Every stored setting differs from `Drawing::new`, so a dropped field
    /// shows up as a difference.
    fn non_default_drawing(tool: DrawingToolType) -> Drawing {
        let mut d = Drawing::new(7, tool);
        d.chart_points = vec![
            ChartPoint::new(3.0, 101.25),
            ChartPoint::new(10.5, 99.5),
            ChartPoint::new(-2.0, 120.0),
        ];
        d.update_screen_coords(bar_to_x, price_to_y);
        d.color = [1, 2, 3, 4];
        d.stroke_width = 3.5;
        d.completed = true;
        d.text = Some("note".into());
        d.line_style = LineStyle::Dotted;
        d.fill_color = Some([5, 6, 7, 8]);
        d.locked = true;
        d.visible = false;
        d.font_size = 18.0;
        d.font_weight = FontWeight::Bold;
        d.extend_left = true;
        d.extend_right = true;
        d.arrow_start_style = ArrowStyle::Diamond;
        d.arrow_end_style = ArrowStyle::Circle;
        d.timeframe_visibility = TimeframeVisibility::specific(&["1h"]);
        d.position_size = Some(PositionSize::RiskPercent {
            account_size: 25_000.0,
            percent: 1.5,
        });
        d.fib_config = Some(FibonacciConfig {
            levels: vec![FibonacciLevel {
                value: 1.618,
                label: "golden".into(),
                color: [9, 10, 11, 12],
                visible: false,
                show_price: false,
            }],
            show_background: true,
            background_opacity: 77,
            extend_left: true,
            extend_right: true,
            reverse: true,
        });
        d.volume_profile = VolumeProfileConfig::new(50, 0.68);
        d.regression_trend = RegressionTrendConfig::new(1.5);
        d.z_order = -3;
        d
    }

    #[test]
    fn every_tool_round_trips_with_all_settings() {
        for &tool in DrawingToolType::all() {
            let drawing = non_default_drawing(tool);
            let mut storage = DrawingStorage::new("AAPL");
            storage.add_drawing(drawing_to_stored(&drawing, bar_to_time));
            let (loaded, _) = DrawingStorage::from_json(&storage.to_json().unwrap()).unwrap();
            let back = restore(&loaded.drawings()[0], drawing.id);
            assert_eq!(format!("{back:?}"), format!("{drawing:?}"), "{tool:?}");
        }
    }

    /// The version 1 fixture was written by the previous release and the
    /// version 2 fixture by this format's first implementation; they stand for
    /// files already saved, so do not rewrite them.
    #[test]
    fn from_json_reads_saved_files_and_skips_the_drawings_it_cannot_read() {
        let v1_json = include_str!("fixtures/storage_v1.json");
        let (v1, skipped) = DrawingStorage::from_json(v1_json).unwrap();
        assert!(skipped.is_empty(), "{skipped:?}");
        assert_eq!(v1.version, DrawingStorage::CURRENT_VERSION);
        assert_eq!(v1.timeframe.as_deref(), Some("1h"));
        let tools = [
            (DrawingToolType::TrendLine, None),
            (DrawingToolType::FibonacciRetracement, None),
            (DrawingToolType::ElliottImpulse, None),
            (DrawingToolType::Arrow, None),
            (DrawingToolType::TextLabel, Some("breakout")),
        ];
        assert_eq!(v1.drawings().len(), tools.len());
        for (stored, (tool, text)) in v1.drawings().iter().zip(tools) {
            let mut want = Drawing::with_color(1, tool, [33, 150, 243, 255]);
            want.chart_points = vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(10.0, 110.0)];
            want.update_screen_coords(bar_to_x, price_to_y);
            want.stroke_width = 3.0;
            want.completed = true;
            want.text = text.map(String::from);
            // The fixture's Arrow is the one drawing saved locked and hidden.
            want.locked = tool == DrawingToolType::Arrow;
            want.visible = tool != DrawingToolType::Arrow;
            assert_eq!(format!("{:?}", restore(stored, 1)), format!("{want:?}"));
        }

        let (v2, skipped) =
            DrawingStorage::from_json(include_str!("fixtures/storage_v2.json")).unwrap();
        assert!(skipped.is_empty(), "{skipped:?}");
        let tools = [
            DrawingToolType::TrendLine,
            DrawingToolType::LongPos,
            DrawingToolType::FixedRangeVolumeProfile,
        ];
        assert_eq!(v2.drawings().len(), tools.len());
        for (stored, tool) in v2.drawings().iter().zip(tools) {
            let want = non_default_drawing(tool);
            assert_eq!(
                format!("{:?}", restore(stored, want.id)),
                format!("{want:?}")
            );
        }

        let mut first = Drawing::new(1, DrawingToolType::FixedRangeVolumeProfile);
        first.volume_profile = VolumeProfileConfig::new(24, 0.7);
        let mut storage = DrawingStorage::new("AAPL");
        storage.add_drawing(drawing_to_stored(&first, bar_to_time));
        let second = Drawing::new(2, DrawingToolType::TrendLine);
        storage.add_drawing(drawing_to_stored(&second, bar_to_time));
        let current = storage.to_json().unwrap();
        let unreadable_first_drawing = [
            (
                current.replacen("\"stroke_width\": 2.0", "\"stroke_width\": null", 1),
                "invalid type: null",
                &["2"][..],
            ),
            (
                current.replacen("\"rows\": 24", "\"rows\": 0", 1),
                "volume profile settings out of bounds",
                &["2"],
            ),
            (
                current.replace("FixedRangeVolumeProfile", "NewerTool"),
                "unknown variant `NewerTool`",
                &["2"],
            ),
            (
                // "fib retracement" is a name the previous loader also accepted.
                v1_json.replacen("Trend Line", "Newer Tool", 1).replacen(
                    "Fib Retracement",
                    "fib retracement",
                    1,
                ),
                "unknown drawing tool \"Newer Tool\"",
                &["2", "3", "4", "5"],
            ),
        ];
        for (json, error, loaded) in unreadable_first_drawing {
            let (storage, skipped) = DrawingStorage::from_json(&json).unwrap();
            let ids: Vec<_> = storage.drawings().iter().map(|d| d.id.as_str()).collect();
            assert_eq!(ids, loaded);
            assert_eq!(skipped.len(), 1);
            assert_eq!(skipped[0].id.as_deref(), Some("1"));
            assert!(skipped[0].error.to_string().contains(error), "{skipped:?}");
        }

        let got = DrawingStorage::from_json(&current.replace("\"version\": 2", "\"version\": 3"))
            .unwrap_err()
            .to_string();
        assert!(
            got.contains("unsupported drawing storage version 3"),
            "{got}"
        );
    }

    #[test]
    fn test_remove_drawing() {
        let mut storage = DrawingStorage::new("AAPL");
        storage.add_drawing(StoredDrawing::new("1", Drawing::default()));
        storage.add_drawing(StoredDrawing::new("2", Drawing::default()));

        assert!(storage.remove_drawing("1"));
        assert_eq!(storage.drawings().len(), 1);
        assert!(!storage.remove_drawing("1")); // Already removed
    }
}
