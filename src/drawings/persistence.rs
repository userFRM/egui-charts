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
//! A restored drawing keeps the id, creation time and notes of its record in
//! `Drawing::stored_id`, `created_at` and `notes`, and is saved back under
//! them. Restore drawings through
//! [`DrawingManager::add_synced_drawing`](crate::drawings::DrawingManager::add_synced_drawing),
//! which refuses a drawing that saving would refuse; the manager then gives a new drawing an
//! id that no restored drawing uses. Two managers restoring the same storage can still hand
//! out the same id: one storage per manager is the supported use.
//!
//! # Storage format
//!
//! Drawings are grouped by symbol into a `DrawingStorage` container that
//! includes a format version, metadata, and an optional timeframe filter.
//! Loading migrates versions 1 to 4, rejects versions it does not know and
//! skips any drawing it cannot read
//! (see [`from_json`](crate::drawings::persistence::DrawingStorage::from_json)).

use crate::drawings::{ChartPoint, Drawing, DrawingToolType, ParseToolTypeError, PositionSize};
use serde::de::IntoDeserializer;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

/// A coordinate stored as a `(timestamp, price)` pair for persistence.
///
/// Unlike screen-space coordinates ([`Pos2`](egui::Pos2)) and bar indices,
/// these values are independent of chart pan/zoom and of the loaded history,
/// making them suitable for long-term storage and cross-device synchronization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredCoord {
    /// Timestamp in Unix epoch milliseconds.
    pub ts_ms: i64,
    /// Nanoseconds after `ts_ms`, less than 1,000,000. Older documents default to zero.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub subms_ns: u32,
    /// Price level at this coordinate.
    pub price: f64,
}

impl StoredCoord {
    /// Creates a new stored coordinate from a timestamp and price.
    pub fn new(ts_ms: i64, price: f64) -> Self {
        Self {
            ts_ms,
            subms_ns: 0,
            price,
        }
    }

    pub(crate) fn from_time(time: chrono::DateTime<chrono::Utc>, price: f64) -> Self {
        Self {
            ts_ms: time.timestamp_millis(),
            subms_ns: time.timestamp_subsec_nanos() % 1_000_000,
            price,
        }
    }

    pub(crate) fn time(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        if self.subms_ns >= 1_000_000 {
            return None;
        }
        chrono::DateTime::from_timestamp_millis(self.ts_ms)?
            .checked_add_signed(chrono::Duration::nanoseconds(i64::from(self.subms_ns)))
    }
}

fn is_zero(value: &u32) -> bool {
    *value == 0
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
    /// The study pane holding the drawing, which a chart resolves to its own pane with the same
    /// studies, or to the price pane when it has none, since study ids are local to each chart.
    /// `None` for the price pane, and in records written before it was stored, which keep the
    /// study id in `drawing.pane`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pane: Option<StoredPane>,
}

/// A study pane identified by its studies rather than by a chart's study ids.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StoredPane {
    /// The name of each study in the pane, in legend order, with its input ids and their values
    /// as serialized [`InputValue`](crate::studies::InputValue)s.
    pub studies: Vec<(String, Vec<(String, serde_json::Value)>)>,
    /// The pane's rank, from the top, among the chart's panes holding the same studies.
    pub rank: usize,
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
            pane: None,
        }
    }

    /// Add a point
    pub fn add_point(&mut self, point: StoredCoord) {
        self.points.push(point);
        self.modified_at = chrono::Utc::now().timestamp_millis();
    }

    /// The anchors the tool needs when this drawing has fewer, as `(needed, held)`.
    fn missing_anchors(&self) -> Option<(usize, usize)> {
        let needed = self.drawing.tool_type.anchors_needed();
        (self.points.len() < needed).then_some((needed, self.points.len()))
    }

    /// The first field holding NaN or an infinity, which JSON cannot store, or a size or
    /// pitchfork level outside the range drawings are drawn at.
    pub(super) fn invalid_field(&self) -> Option<String> {
        use super::validation::{MAX_PITCHFORK_LEVEL, MAX_STROKE_WIDTH};
        use crate::styles::typography::FONT_SIZES;
        let d = &self.drawing;
        let bad = |value: f64| !value.is_finite();
        if !(d.stroke_width > 0.0 && d.stroke_width <= MAX_STROKE_WIDTH) {
            return Some("stroke_width".into());
        }
        if let Some(i) = d
            .pitchfork
            .iter()
            .flat_map(|c| &c.levels)
            .position(|l| l.width.is_some_and(|w| !(w > 0.0 && w <= MAX_STROKE_WIDTH)))
        {
            return Some(format!("pitchfork.levels[{i}].width"));
        }
        if d.screen_anchor
            .is_some_and(|anchor| anchor.into_iter().any(|value| !value.is_finite()))
        {
            return Some("screen_anchor".into());
        }
        if !FONT_SIZES.contains(&d.font_size) {
            return Some("font_size".into());
        }
        if let Some(i) = self.points.iter().position(|p| bad(p.price)) {
            return Some(format!("points[{i}].price"));
        }
        if d.ghost_feed.is_some_and(|c| !c.is_valid()) {
            return Some("ghost_feed".into());
        }
        let sizing = d.position_sizing;
        if [
            sizing.point_value,
            sizing.lot_size,
            sizing.quantity_step,
            sizing.leverage,
        ]
        .into_iter()
        .any(bad)
        {
            return Some("position_sizing".into());
        }
        let size = match d.position_size {
            Some(PositionSize::Quantity(value) | PositionSize::RiskAmount(value)) => [value, 0.0],
            Some(PositionSize::RiskPercent {
                account_size,
                percent,
            }) => [account_size, percent],
            None => [0.0; 2],
        };
        if size.into_iter().any(bad) {
            return Some("position_size".into());
        }
        if d.fib_config.as_ref().is_some_and(|c| {
            !c.level_width.is_finite()
                || c.level_width <= 0.0
                || c.level_width > super::validation::MAX_STROKE_WIDTH
        }) {
            return Some("fib_config.level_width".into());
        }
        for (name, values) in [
            (
                "fib_config.levels",
                d.fib_config
                    .iter()
                    .flat_map(|c| c.levels.iter().map(|l| l.value))
                    .collect::<Vec<_>>(),
            ),
            (
                "pitchfork.levels",
                d.pitchfork
                    .iter()
                    .flat_map(|c| c.levels.iter().map(|l| l.value))
                    .collect(),
            ),
            (
                "gann.price_levels",
                d.gann
                    .iter()
                    .flat_map(|c| c.price_levels.iter().map(|l| l.value))
                    .collect(),
            ),
            (
                "gann.time_levels",
                d.gann
                    .iter()
                    .flat_map(|c| c.time_levels.iter().map(|l| l.value))
                    .collect(),
            ),
            (
                "gann.fan_ratios",
                d.gann
                    .iter()
                    .flat_map(|c| c.fan_ratios.iter().map(|l| l.value))
                    .collect(),
            ),
            (
                "gann.arc_ratios",
                d.gann
                    .iter()
                    .flat_map(|c| c.arc_ratios.iter().map(|l| l.value))
                    .collect(),
            ),
        ] {
            // Pitchfork tines are drawn at their multiple of the median's distance.
            let limit = if name == "pitchfork.levels" {
                MAX_PITCHFORK_LEVEL
            } else {
                f32::MAX
            };
            if let Some(i) = values.iter().position(|v| !(-limit..=limit).contains(v)) {
                return Some(format!("{name}[{i}].value"));
            }
        }
        None
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
    /// Current storage format version. Version 5 gives Elliott double and triple combinations an
    /// unlabelled start point; earlier versions load with a start point on their first anchor.
    /// Version 4 draws brushes and highlighters at their stored width and opacity; earlier
    /// versions load at the width and opacity their strokes were drawn at. Version 3 preserves
    /// editable ray and extended-line extensions; versions 1 and 2 migrate to their former
    /// rendered reach when loaded.
    pub const CURRENT_VERSION: u32 = 5;

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

    /// Add a drawing, or replace the drawing with the same id in place, so a
    /// drawing saved again after an edit is stored once and the ids stay
    /// unique, as [`Self::from_json`] requires. The replacement keeps the
    /// position and the `created_at` of the drawing it replaces and takes
    /// every other field, `notes` included, from `drawing`.
    pub fn add_drawing(&mut self, drawing: StoredDrawing) {
        match self.get_drawing_mut(&drawing.id) {
            Some(existing) => {
                *existing = StoredDrawing {
                    created_at: existing.created_at,
                    ..drawing
                }
            }
            None => self.drawings.push(drawing),
        }
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

    /// Serialize to JSON, refusing what would lose a drawing on the next load:
    /// a non-finite number, which JSON cannot store and which would otherwise be
    /// written as `null`, an id an earlier drawing already has, a cursor tool
    /// such as the eraser, an unsupported timestamp and fewer anchors than the tool draws with.
    /// [`Self::from_json`] skips these records.
    pub fn to_json(&self) -> Result<String, SaveError> {
        let mut ids = HashSet::new();
        for stored in &self.drawings {
            if stored.drawing.tool_type.is_cursor() {
                return Err(SaveError::Cursor {
                    id: stored.id.clone(),
                });
            }
            if !ids.insert(stored.id.as_str()) {
                return Err(SaveError::DuplicateId {
                    id: stored.id.clone(),
                });
            }
            if let Some(field) = stored.invalid_field() {
                return Err(SaveError::NonFinite {
                    id: stored.id.clone(),
                    field,
                });
            }
            if let Some(point) = stored
                .points
                .iter()
                .position(|point| point.time().is_none())
            {
                return Err(SaveError::UnmappedAnchor {
                    id: stored.id.clone(),
                    point,
                });
            }
            if let Some((needed, anchors)) = stored.missing_anchors() {
                return Err(SaveError::MissingAnchors {
                    id: stored.id.clone(),
                    tool: stored.drawing.tool_type,
                    anchors,
                    needed,
                });
            }
        }
        serde_json::to_string_pretty(self).map_err(SaveError::Json)
    }

    /// Deserialize from JSON, migrating versions 1 to 4 to the current format.
    /// Fails as a whole with a [`LoadError`]: [`LoadError::Json`] for invalid JSON or a missing
    /// or invalid version, symbol, timeframe or metadata, and [`LoadError::UnsupportedVersion`]
    /// for a version this release does not know. A
    /// drawing that cannot be read (a missing field, a `null` an earlier release
    /// wrote for a non-finite number, a number out of the range of its field, a
    /// tool type from a newer release, a cursor, fewer anchors than the tool
    /// draws with or a setting out of bounds) is
    /// left out and returned as a [`SkippedDrawing`] with its [`SkipReason`], so one bad drawing does
    /// not lose the others. A drawing whose id an
    /// earlier loaded drawing already has is skipped too, so the loaded ids are
    /// unique and the first one in the file wins. Saving the returned storage writes
    /// only the drawings that loaded; keep the original file if the skipped
    /// ones must be recovered. A drawing whose timeframe filter names no
    /// timeframe loads as visible on every timeframe.
    pub fn from_json(json: &str) -> Result<(Self, Vec<SkippedDrawing>), LoadError> {
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
        let version = serde_json::from_str::<Version>(json)
            .map_err(LoadError::Json)?
            .version;
        let load: fn(serde_json::Value) -> Result<StoredDrawing, SkipReason> = match version {
            1 => v1::load,
            2..=Self::CURRENT_VERSION => |value| {
                // Checked here, not in `Drawing`'s serde, which other formats share:
                // serde's own error lists every variant.
                if let Some(name) = value.pointer("/drawing/tool_type").and_then(|t| t.as_str()) {
                    DrawingToolType::deserialize(name.into_deserializer()).map_err(
                        |_: serde::de::value::Error| {
                            SkipReason::UnknownTool(ParseToolTypeError { input: name.into() })
                        },
                    )?;
                }
                serde_json::from_value(value).map_err(SkipReason::Invalid)
            },
            version => {
                return Err(LoadError::UnsupportedVersion {
                    found: version,
                    supported: 1..=Self::CURRENT_VERSION,
                });
            }
        };
        let container: Container = serde_json::from_str(json).map_err(LoadError::Json)?;
        let mut skipped = Vec::new();
        let mut ids = HashSet::new();
        let drawings = container
            .drawings
            .into_iter()
            .enumerate()
            .filter_map(|(index, value)| {
                let id = value.get("id").and_then(|id| id.as_str()).map(String::from);
                load(value)
                    .and_then(|mut drawing| {
                        // Before the id check, so that a skipped drawing does not claim its id.
                        let tool = drawing.drawing.tool_type;
                        if version < 3 {
                            // Earlier renderers ignored these flags for rays and extended lines.
                            match tool {
                                DrawingToolType::Ray => {
                                    let reversed = drawing.points.get(..2).is_some_and(|p| {
                                        (p[0].ts_ms, p[0].subms_ns) > (p[1].ts_ms, p[1].subms_ns)
                                    });
                                    drawing.drawing.extend_left = reversed;
                                    drawing.drawing.extend_right = !reversed;
                                }
                                DrawingToolType::ExtendedLine => {
                                    drawing.drawing.extend_left = true;
                                    drawing.drawing.extend_right = true;
                                }
                                _ => {}
                            }
                        }
                        if tool.is_cursor() {
                            return Err(SkipReason::Cursor { tool });
                        }
                        if drawing.points.iter().any(|point| point.time().is_none()) {
                            return Err(SkipReason::Invalid(serde::de::Error::custom(
                                "anchor timestamp is outside the supported range",
                            )));
                        }
                        if let Some(field) = drawing.invalid_field() {
                            return Err(SkipReason::Invalid(serde::de::Error::custom(format!(
                                "non-finite or out-of-range {field}"
                            ))));
                        }
                        if version < 4 {
                            // Earlier renderers drew brushes at twice and highlighters at eight
                            // times the stored width, highlighters at alpha 100 whatever the
                            // colour's.
                            let settings = &mut drawing.drawing;
                            let factor = match tool {
                                DrawingToolType::Brush => 2.0,
                                DrawingToolType::Highlighter => {
                                    settings.color[3] = 100;
                                    8.0
                                }
                                _ => 1.0,
                            };
                            settings.stroke_width = (settings.stroke_width * factor)
                                .min(super::validation::MAX_STROKE_WIDTH);
                        }
                        // Earlier placements stored fewer anchors. Keep their endpoints and
                        // supply editable controls before checking the current construction.
                        use DrawingToolType as T;
                        // Combinations saved before version 5 had no start point: their first
                        // anchor ends wave W. A combination finished early since then can hold
                        // as many anchors, so only the version tells them apart.
                        if version < 5
                            && matches!(tool, T::ElliottDoubleCombo | T::ElliottTripleCombo)
                            && let Some(first) = drawing.points.first().cloned()
                        {
                            drawing.points.insert(0, first);
                        }
                        match (tool, drawing.points.as_slice()) {
                            (T::PriceNote | T::ArrowMarker, [a]) => {
                                drawing.points.push(a.clone());
                            }
                            (T::RotatedRect, [a, b]) => {
                                drawing.points = vec![
                                    a.clone(),
                                    StoredCoord {
                                        price: a.price,
                                        ..b.clone()
                                    },
                                    b.clone(),
                                ];
                            }
                            (T::DoubleCurve, [a, b]) => {
                                drawing.points = vec![a.clone(), a.clone(), b.clone(), b.clone()];
                            }
                            (T::FibonacciWedge | T::Pitchfan, [a, b]) => {
                                drawing.points.push(StoredCoord {
                                    price: a.price,
                                    ..b.clone()
                                });
                            }
                            _ => {}
                        }
                        if let Some((needed, anchors)) = drawing.missing_anchors() {
                            return Err(SkipReason::MissingAnchors {
                                tool,
                                anchors,
                                needed,
                            });
                        }
                        // A filter naming no interval and holding no range would show the drawing on none.
                        let visibility = &mut drawing.drawing.timeframe_visibility;
                        visibility.show_on_all |= visibility.visible_timeframes.is_empty()
                            && visibility.ranges.is_empty();
                        if ids.insert(drawing.id.clone()) {
                            Ok(drawing)
                        } else {
                            Err(SkipReason::DuplicateId {
                                id: drawing.id.clone(),
                            })
                        }
                    })
                    .map_err(|error| skipped.push(SkippedDrawing { index, id, error }))
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

/// Why [`DrawingStorage::to_json`] did not save.
#[derive(Debug)]
pub enum SaveError {
    /// A drawing holds NaN or an infinity, which JSON cannot store, or a size or pitchfork
    /// level out of the range drawings are drawn at, and loading skips it.
    NonFinite {
        /// The id of the drawing.
        id: String,
        /// The field, such as `"stroke_width"` or `"points[1].price"`.
        field: String,
    },
    /// Two drawings have the same id, and loading keeps only the first.
    DuplicateId {
        /// The id of the drawing.
        id: String,
    },
    /// A drawing's tool is a cursor, such as the eraser, and loading skips it.
    Cursor {
        /// The id of the drawing.
        id: String,
    },
    /// A drawing has fewer anchors than its tool draws with, and loading skips it.
    MissingAnchors {
        /// The id of the drawing.
        id: String,
        /// The tool of the drawing.
        tool: DrawingToolType,
        /// The anchors the drawing has.
        anchors: usize,
        /// The anchors the tool needs.
        needed: usize,
    },
    /// An anchor has no time mapping or its timestamp is outside the supported range.
    UnmappedAnchor {
        /// Drawing id.
        id: String,
        /// Anchor index within the drawing.
        point: usize,
    },
    /// Serialization failed.
    Json(serde_json::Error),
}

impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFinite { id, field } => {
                write!(f, "drawing {id:?} has a non-finite or out-of-range {field}")
            }
            Self::DuplicateId { id } => write!(f, "duplicate drawing id {id:?}"),
            Self::Cursor { id } => write!(f, "drawing {id:?} is a cursor, not a drawing"),
            Self::MissingAnchors {
                id,
                tool,
                anchors,
                needed,
            } => write!(
                f,
                "drawing {id:?} has {anchors} anchors, {tool} needs {needed}"
            ),
            Self::UnmappedAnchor { id, point } => {
                write!(f, "drawing {id:?} anchor {point} has no time mapping")
            }
            Self::Json(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for SaveError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::NonFinite { .. }
            | Self::DuplicateId { .. }
            | Self::Cursor { .. }
            | Self::MissingAnchors { .. }
            | Self::UnmappedAnchor { .. } => None,
            Self::Json(error) => Some(error),
        }
    }
}

/// A drawing [`DrawingStorage::from_json`] left out because it could not be
/// read or its id was already taken.
#[derive(Debug)]
pub struct SkippedDrawing {
    /// The position of the entry in the file's list of drawings, counted from 0.
    pub index: usize,
    /// The stored id, or `None` if the entry has no id or a non-string one.
    pub id: Option<String>,
    /// Why the drawing could not be read.
    pub error: SkipReason,
}

/// Why [`DrawingStorage::from_json`] left a drawing out.
///
/// An [`UnknownTool`](Self::UnknownTool) in a file of version 2 or later was most likely written
/// by a newer release that added the tool: keep the file. The other reasons describe damaged or
/// hand-edited data.
#[derive(Debug)]
#[non_exhaustive]
pub enum SkipReason {
    /// The tool is not one this release knows; [`ParseToolTypeError::input`] holds its name.
    UnknownTool(ParseToolTypeError),
    /// The tool is a cursor, such as the eraser, which creates no drawing.
    Cursor {
        /// The cursor tool.
        tool: DrawingToolType,
    },
    /// The drawing has fewer anchors than its tool draws with.
    MissingAnchors {
        /// The tool of the drawing.
        tool: DrawingToolType,
        /// The anchors the drawing has.
        anchors: usize,
        /// The anchors the tool needs.
        needed: usize,
    },
    /// An earlier drawing in the file has the same id; the first one is loaded.
    DuplicateId {
        /// The repeated id.
        id: String,
    },
    /// A field is missing or has the wrong type, or a value is out of range, such as a `null`
    /// written for a non-finite number, a timestamp outside the supported range or a setting
    /// out of bounds.
    Invalid(serde_json::Error),
}

impl fmt::Display for SkipReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownTool(error) => error.fmt(f),
            Self::Cursor { tool } => write!(f, "{tool} is a cursor, not a drawing"),
            Self::MissingAnchors {
                tool,
                anchors,
                needed,
            } => write!(f, "{tool} needs {needed} anchors, has {anchors}"),
            Self::DuplicateId { id } => write!(f, "duplicate drawing id {id:?}"),
            Self::Invalid(error) => error.fmt(f),
        }
    }
}

impl std::error::Error for SkipReason {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::UnknownTool(error) => Some(error),
            Self::Invalid(error) => Some(error),
            Self::Cursor { .. } | Self::MissingAnchors { .. } | Self::DuplicateId { .. } => None,
        }
    }
}

/// Why drawings were not loaded, by [`DrawingStorage::from_json`],
/// [`Chart::load_drawings`](crate::widget::Chart::load_drawings) or
/// [`Chart::paste_drawings`](crate::widget::Chart::paste_drawings), before any drawing changed.
///
/// [`UnsupportedVersion`](Self::UnsupportedVersion) above `supported` means a newer release
/// wrote the data: keep it and update. [`Json`](Self::Json) means damaged data.
#[derive(Debug)]
#[non_exhaustive]
pub enum LoadError {
    /// The text is not valid JSON, or its version, symbol, timeframe or metadata is missing or
    /// invalid.
    Json(serde_json::Error),
    /// The storage version is not one this release reads.
    UnsupportedVersion {
        /// The version the data declares.
        found: u32,
        /// The versions this release reads.
        supported: std::ops::RangeInclusive<u32>,
    },
    /// The drawings belong to another symbol than the chart's.
    OtherSymbol,
    /// The chart has no bars to place the pasted anchors on; paste once history has loaded.
    NeedsHistory,
    /// The pasted text is not drawing clipboard text from
    /// [`Chart::copy_selected_drawings`](crate::widget::Chart::copy_selected_drawings).
    NotClipboard,
    /// Drawings that could not be read, where every drawing is needed: a paste, or a document
    /// that automatic storage then never writes over.
    Skipped(Vec<SkippedDrawing>),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Json(error) => error.fmt(f),
            Self::UnsupportedVersion { found, supported } => write!(
                f,
                "unsupported drawing storage version {found}, expected {} to {}",
                supported.start(),
                supported.end()
            ),
            Self::OtherSymbol => f.write_str("drawing storage belongs to a different symbol"),
            Self::NeedsHistory => f.write_str("clipboard needs loaded chart history"),
            Self::NotClipboard => f.write_str("not a drawing clipboard"),
            Self::Skipped(skipped) if skipped.len() == 1 => {
                f.write_str("1 drawing could not be read")
            }
            Self::Skipped(skipped) => write!(f, "{} drawings could not be read", skipped.len()),
        }
    }
}

impl std::error::Error for LoadError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Json(error) => Some(error),
            _ => None,
        }
    }
}

/// A failed read or write of automatic drawing storage, from
/// [`Chart::take_storage_error`](crate::widget::Chart::take_storage_error).
#[derive(Debug)]
#[non_exhaustive]
pub enum StorageError {
    /// The stored drawings of `symbol`, or its kept copy, could not all be read. The document
    /// is kept and never written over; `error` tells a newer release from damaged data.
    Read {
        /// The symbol or kept copy whose document was read.
        symbol: String,
        /// Why it could not be read.
        error: LoadError,
    },
    /// The drawings of `symbol` could not be saved. They stay on the chart.
    Save {
        /// The symbol whose drawings were saved.
        symbol: String,
        /// Why they could not be saved.
        error: SaveError,
    },
    /// The storage service or a kept-copy operation failed; the message names it. A failed
    /// write keeps the drawings on the chart and is retried.
    Storage(String),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { symbol, error } => {
                write!(f, "Could not read drawings of {symbol}: {error}")
            }
            Self::Save { symbol, error } => {
                write!(f, "Could not save drawings of {symbol}: {error}")
            }
            Self::Storage(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for StorageError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Read { error, .. } => Some(error),
            Self::Save { error, .. } => Some(error),
            Self::Storage(_) => None,
        }
    }
}

/// The version 1 format, read only to migrate it: the tool by display name,
/// the colour, width, lock, visibility and text, and nothing else.
mod v1 {
    use super::StoredCoord;
    use crate::drawings::{Drawing, DrawingToolType};
    use serde::Deserialize;
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
    ) -> Result<super::StoredDrawing, super::SkipReason> {
        let stored: StoredDrawing =
            serde_json::from_value(value).map_err(super::SkipReason::Invalid)?;
        // The previous loader also accepted the other names `FromStr` parses,
        // such as "trendline".
        let tool_type: DrawingToolType = stored
            .tool_type
            .parse()
            .map_err(super::SkipReason::UnknownTool)?;
        let Color { r, g, b, a } = stored.color;
        let mut drawing = Drawing::with_color(0, tool_type, [r, g, b, a]);
        drawing.fib_config = None;
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
            pane: None,
        })
    }
}

/// Convert a Drawing to StoredDrawing, its anchors mapped from bar index to timestamp, under
/// the drawing's stored id and creation time when it was restored from storage, and under its
/// id and the current time otherwise.
///
/// Retained timestamps are used for unchanged projected indices. For new or edited indices,
/// `bar_to_time` must accept fractional bar indices and extrapolate by the bar
/// interval before index 0 and after the last bar instead of clamping: anchors
/// can sit between bars, in the history not yet loaded or in the future area,
/// and a clamping mapping would move them onto the first or last bar. It must
/// be the inverse of the `time_to_bar` passed to [`stored_to_drawing`].
pub fn drawing_to_stored(drawing: &Drawing, bar_to_time: impl Fn(f32) -> i64) -> StoredDrawing {
    drawing_to_stored_with(drawing, |index, price| {
        StoredCoord::new(bar_to_time(index), price)
    })
}

pub(crate) fn drawing_to_stored_with(
    drawing: &Drawing,
    bar_to_coord: impl Fn(f32, f64) -> StoredCoord,
) -> StoredDrawing {
    let mut settings = drawing.clone();
    settings.points.clear();
    settings.chart_points.clear();
    let id = drawing.stored_id.clone();
    let mut stored = StoredDrawing::new(id.unwrap_or_else(|| drawing.id.to_string()), settings);
    stored.created_at = drawing.created_at.unwrap_or(stored.created_at);
    stored.notes.clone_from(&drawing.notes);
    stored.points = drawing
        .chart_points
        .iter()
        .map(|p| {
            p.time_anchor
                .filter(|(_, index)| *index == p.bar_idx)
                .map_or_else(
                    || bar_to_coord(p.bar_idx, p.price),
                    |(time, _)| StoredCoord::from_time(time, p.price),
                )
        })
        .collect();
    stored
}

/// Convert StoredDrawing back to a completed Drawing with id `next_id`, its anchors mapped from
/// timestamp to bar index and the record's id, creation time and notes kept; the screen points
/// follow on the next [`Drawing::update_screen_coords`].
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
    stored_to_drawing_with(stored, |point| time_to_bar(point.ts_ms), next_id)
}

pub(crate) fn stored_to_drawing_with(
    stored: &StoredDrawing,
    time_to_bar: impl Fn(&StoredCoord) -> f32,
    next_id: usize,
) -> Drawing {
    let mut drawing = stored.drawing.clone();
    drawing.id = next_id;
    drawing.stored_id = Some(stored.id.clone());
    drawing.created_at = Some(stored.created_at);
    drawing.notes.clone_from(&stored.notes);
    drawing.chart_points = stored
        .points
        .iter()
        .map(|p| {
            let mut point = ChartPoint::new(time_to_bar(p), p.price);
            point.time_anchor = p.time().map(|time| (time, point.bar_idx));
            point
        })
        .collect();
    drawing.completed = true;
    drawing
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::VolumeProfileConfig;
    use crate::drawings::*;
    use crate::model::LineStyle;

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

    /// The drawing `stored` restores to, without the record's identity, which
    /// `a_later_session_keeps_the_stored_drawings_and_their_ids` checks.
    fn restore(stored: &StoredDrawing, id: usize) -> Drawing {
        let mut drawing = stored_to_drawing(stored, time_to_bar, id);
        drawing.update_screen_coords(bar_to_x, price_to_y);
        (drawing.stored_id, drawing.created_at, drawing.notes) = (None, None, None);
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
        d.color = [1, 2, 3, 4];
        d.stroke_width = 3.5;
        d.completed = true;
        d.text = Some("note".into());
        d.label = Some("Support".into());
        d.line_style = LineStyle::Dotted;
        d.fill_color = Some([5, 6, 7, 8]);
        d.locked = true;
        d.visible = false;
        d.font_size = 18.0;
        d.font_weight = FontWeight::Bold;
        d.extend_left = !d.extend_left;
        d.extend_right = !d.extend_right;
        d.arrow_start_style = ArrowStyle::Diamond;
        d.arrow_end_style = ArrowStyle::Circle;
        d.timeframe_visibility = TimeframeVisibility::specific(&["1h"]);
        d.position_sizing = PositionSizing {
            point_value: 50.0,
            lot_size: 10.0,
            quantity_step: 0.25,
            leverage: 5.0,
        };
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
            level_width: 3.5,
            level_style: LineStyle::Dashed,
            show_background: true,
            background_opacity: 77,
            extend_left: true,
            extend_right: true,
            reverse: true,
            logarithmic: true,
            ..FibonacciConfig::default()
        });
        d.volume_profile = VolumeProfileConfig::new(50, 0.68);
        d.regression_trend = RegressionTrendConfig::new(1.5);
        let mut vwap = VwapConfig::default();
        vwap.bands[0].enabled = false;
        vwap.bands[1] = VwapBand::new(true, 2.5).unwrap();
        d.vwap = Some(vwap);
        if tool == DrawingToolType::DoubleCurve {
            d.chart_points.push(ChartPoint::new(12.0, 115.0));
        }
        if tool == DrawingToolType::ElliottTriangle {
            d.chart_points.extend([
                ChartPoint::new(12.0, 115.0),
                ChartPoint::new(14.0, 104.0),
                ChartPoint::new(16.0, 112.0),
            ]);
        }
        d.italic = true;
        d.text_color = Some([13, 14, 15, 16]);
        d.background = false;
        d.border = false;
        d.border_color = Some([17, 18, 19, 20]);
        d.z_order = -3;
        d.update_screen_coords(bar_to_x, price_to_y);
        d
    }

    #[test]
    fn every_tool_round_trips_with_all_settings() {
        use crate::data::ProfileDisplay::{Delta, Total, UpDown};
        for (tool, custom_anchors) in DrawingToolType::all()
            .iter()
            .copied()
            .filter(|t| !t.is_cursor())
            .map(|tool| (tool, None))
            .chain((1..=3).map(|anchors| (DrawingToolType::Polyline, Some(anchors))))
        {
            let mut drawing = non_default_drawing(tool);
            if let Some(anchors) = custom_anchors {
                drawing.custom_tool = Some("host-shape".into());
                drawing.custom_settings = serde_json::json!({"radius": 10});
                drawing.chart_points.truncate(anchors);
                drawing.update_screen_coords(bar_to_x, price_to_y);
            }
            drawing.gann = Some(crate::drawings::GannConfig {
                price_levels: vec![
                    GannLevel::new(0.25, [1, 2, 3, 255]),
                    GannLevel::new(0.75, [4, 5, 6, 255]),
                ],
                time_levels: vec![GannLevel::new(0.5, [7, 8, 9, 255])],
                show_grid: false,
                show_fan: false,
                fan_ratios: vec![
                    GannLevel::new(0.5, [10, 11, 12, 255]),
                    GannLevel {
                        visible: false,
                        ..GannLevel::new(2.0, [13, 14, 15, 255])
                    },
                ],
                fan_direction: crate::drawings::GannFanDirection::Reverse,
                show_arcs: false,
                arc_ratios: vec![GannLevel::new(0.75, [16, 17, 18, 255])],
                show_background: false,
                background_opacity: 7,
                show_angles: true,
            });
            drawing.ghost_feed = Some(crate::drawings::GhostFeedConfig {
                average_range_ticks: 2.5,
                noise_ticks: 0.5,
            });
            drawing.update_screen_coords(bar_to_x, price_to_y);
            let mut fork = crate::drawings::PitchforkConfig::default();
            fork.levels[0].line_style = LineStyle::Dotted;
            fork.levels[1].visible = false;
            fork.show_background = false;
            drawing.pitchfork = Some(fork);
            drawing.channel_midline = true;
            drawing.elliott_degree = ElliottDegree::Minuette;
            drawing.selectable = false;
            drawing.listed = false;
            drawing.group = Some("levels".into());
            drawing.folder = vec!["Analysis".into(), "Support / resistance".into()];
            if matches!(
                tool,
                DrawingToolType::AnchoredText | DrawingToolType::AnchoredNote
            ) {
                drawing.screen_anchor = Some([0.25, 0.75]);
            }

            let modes = if matches!(
                tool,
                DrawingToolType::FixedRangeVolumeProfile | DrawingToolType::AnchoredVolumeProfile
            ) {
                &[Total, UpDown, Delta][..]
            } else {
                &[Total][..]
            };
            for &display in modes {
                let config = drawing.volume_profile.as_mut().unwrap();
                *config = config.with_row_size(Some(0.25)).unwrap();
                config.display = display;
                config.up_color = [1, 2, 3, 240];
                config.down_color = [4, 5, 6, 200];
                let mut storage = DrawingStorage::new("AAPL");
                storage.add_drawing(drawing_to_stored(&drawing, bar_to_time));
                let (loaded, skipped) =
                    DrawingStorage::from_json(&storage.to_json().unwrap()).unwrap();
                assert!(skipped.is_empty(), "{tool:?}: {skipped:?}");
                let back = restore(&loaded.drawings()[0], drawing.id);
                assert_eq!(
                    format!("{back:?}"),
                    format!("{drawing:?}"),
                    "{tool:?} {display:?}"
                );
            }
        }
    }

    /// A later session that loads the storage, restores its drawings, copies one, adds one and
    /// saves them all keeps every stored record as it was, with its layer, and stores the copy
    /// and the new drawing under ids of their own, even where a stored id is a number the new
    /// session could hand out.
    #[test]
    fn a_later_session_keeps_the_stored_drawings_and_their_ids() {
        let mut storage = DrawingStorage::new("AAPL");
        for (id, notes, z_order) in [("a1b2", Some("breakout level"), 1), ("2", None, 0)] {
            let mut drawing = Drawing::new(0, DrawingToolType::HorizontalLine);
            drawing.z_order = z_order;
            let mut stored = StoredDrawing::new(id, drawing);
            stored.points = vec![StoredCoord::new(T0, 100.0)];
            stored.created_at = 1_700_000_000_000;
            stored.notes = notes.map(String::from);
            storage.add_drawing(stored);
        }
        let (mut storage, _) = DrawingStorage::from_json(&storage.to_json().unwrap()).unwrap();

        let mut manager = DrawingManager::new();
        for stored in storage.drawings() {
            manager
                .add_synced_drawing(stored_to_drawing(stored, time_to_bar, 0))
                .unwrap();
        }
        manager
            .add_synced_drawing(manager.drawings[0].clone())
            .unwrap();
        let (x_to_bar, y_to_price) = (|x: f32| x, |y: f32| f64::from(y));
        let at = egui::Pos2::new(5.0, 90.0);
        manager.start_drawing_with_coords(
            DrawingToolType::HorizontalLine,
            at,
            x_to_bar,
            y_to_price,
        );
        for drawing in &manager.drawings {
            storage.add_drawing(drawing_to_stored(drawing, bar_to_time));
        }
        let (saved, _) = DrawingStorage::from_json(&storage.to_json().unwrap()).unwrap();

        let records: Vec<_> = saved
            .drawings()
            .iter()
            .map(|d| {
                (
                    d.id.as_str(),
                    d.created_at,
                    d.notes.as_deref(),
                    d.drawing.z_order,
                )
            })
            .collect();
        assert_eq!(records.len(), 4, "{records:?}");
        assert_eq!(
            records[0],
            ("a1b2", 1_700_000_000_000, Some("breakout level"), 1)
        );
        assert_eq!(records[1], ("2", 1_700_000_000_000, None, 0));
        let ids: HashSet<_> = records.iter().map(|r| r.0).collect();
        assert_eq!(ids.len(), 4, "{records:?}");
    }

    /// The version 1 fixture was written by the previous release and stands
    /// for files already saved, so do not rewrite it. The version 2 fixture was
    /// written by this format's first implementation, which was never released.
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
            // Old files without Fibonacci settings retain their absent config.
            want.fib_config = None;
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
            let mut want = non_default_drawing(tool);
            want.label = None; // Earlier files have no object name.
            want.position_sizing = PositionSizing::default(); // Earlier files use unit sizing.
            want.vwap = None; // Earlier files use the default bands.
            // Earlier files draw text and borders in the drawing colour, boxed and upright.
            want.italic = false;
            want.text_color = None;
            want.background = true;
            want.border = true;
            want.border_color = None;
            let config = want.fib_config.as_mut().unwrap();
            config.logarithmic = false;
            config.level_width = 2.0;
            config.level_style = LineStyle::Solid;
            config.levels_as_percent = true;
            config.labels_on_left = false;
            assert_eq!(
                format!("{:?}", restore(stored, want.id)),
                format!("{want:?}")
            );
        }

        // New level defaults apply to new drawings, never to either saved format's fixed levels.
        for tool in [
            DrawingToolType::FibonacciTimeZones,
            DrawingToolType::TrendBasedFibTime,
            DrawingToolType::FibonacciSpeedFan,
            DrawingToolType::FibonacciCircles,
            DrawingToolType::FibonacciSpeedResistanceArcs,
        ] {
            for (fixture, legacy) in [
                (v1_json, true),
                (include_str!("fixtures/storage_v2.json"), false),
            ] {
                let mut value: serde_json::Value = serde_json::from_str(fixture).unwrap();
                let records = value["drawings"].as_array_mut().unwrap();
                records.truncate(1);
                let fields = if legacy {
                    &mut records[0]
                } else {
                    &mut records[0]["drawing"]
                };
                fields["tool_type"] = serde_json::to_value(tool).unwrap();
                if !legacy {
                    fields["fib_config"] = serde_json::Value::Null;
                }
                if tool == DrawingToolType::TrendBasedFibTime {
                    let points = value["drawings"][0]["points"].as_array_mut().unwrap();
                    points.push(points[0].clone());
                }
                let (loaded, skipped) = DrawingStorage::from_json(&value.to_string()).unwrap();
                assert!(skipped.is_empty(), "{tool:?}: {skipped:?}");
                assert!(
                    loaded.drawings[0].drawing.fib_config.is_none(),
                    "{tool:?}, legacy={legacy}"
                );
                let (again, skipped) =
                    DrawingStorage::from_json(&loaded.to_json().unwrap()).unwrap();
                assert!(skipped.is_empty());
                assert!(
                    again.drawings[0].drawing.fib_config.is_none(),
                    "{tool:?}: retain fixed levels after saving"
                );
            }
        }

        // Earlier rays and extended lines ignored the saved Extend flags. Adopt their old
        // reach once, then retain explicit changes on the next save and load.
        for (tool, name) in [
            (DrawingToolType::Ray, "Ray"),
            (DrawingToolType::ExtendedLine, "Extended Line"),
        ] {
            for (fixture, tool_path, tool_name) in [
                (
                    include_str!("fixtures/storage_v1.json"),
                    "/tool_type",
                    name.to_owned(),
                ),
                (
                    include_str!("fixtures/storage_v2.json"),
                    "/drawing/tool_type",
                    serde_json::to_value(tool)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ),
            ] {
                for reversed in [false, true] {
                    let mut saved: serde_json::Value = serde_json::from_str(fixture).unwrap();
                    let records = saved["drawings"].as_array_mut().unwrap();
                    records.truncate(1);
                    *records[0].pointer_mut(tool_path).unwrap() = tool_name.clone().into();
                    records[0]["points"].as_array_mut().unwrap().truncate(2);
                    if reversed {
                        records[0]["points"].as_array_mut().unwrap().reverse();
                    }
                    if let Some(settings) = records[0].get_mut("drawing") {
                        settings["extend_left"] = true.into();
                        settings["extend_right"] = false.into();
                    }
                    let (mut loaded, skipped) =
                        DrawingStorage::from_json(&saved.to_string()).unwrap();
                    assert!(skipped.is_empty(), "{skipped:?}");
                    let drawing = &mut loaded.drawings[0].drawing;
                    assert_eq!(
                        drawing.extend_left,
                        reversed || tool == DrawingToolType::ExtendedLine
                    );
                    assert_eq!(
                        drawing.extend_right,
                        !reversed || tool == DrawingToolType::ExtendedLine
                    );
                    drawing.extend_left = false;
                    drawing.extend_right = false;
                    let (again, skipped) =
                        DrawingStorage::from_json(&loaded.to_json().unwrap()).unwrap();
                    assert!(skipped.is_empty());
                    assert!(!again.drawings[0].drawing.extend_left);
                    assert!(!again.drawings[0].drawing.extend_right);
                }
            }
        }

        // Strokes saved before version 4 were drawn at twice (brush) and eight times
        // (highlighter) their width, the highlighter at alpha 100. They load as drawn, once.
        for (version, tool, width, alpha, want_width, want_alpha) in [
            (2, DrawingToolType::Brush, 3.0, 200, 6.0, 200),
            (3, DrawingToolType::Brush, 3.0, 200, 6.0, 200),
            (3, DrawingToolType::Highlighter, 2.0, 255, 16.0, 100),
            (3, DrawingToolType::Highlighter, 20.0, 40, 100.0, 100),
            (4, DrawingToolType::Highlighter, 2.0, 40, 2.0, 40),
        ] {
            let mut saved: serde_json::Value =
                serde_json::from_str(include_str!("fixtures/storage_v2.json")).unwrap();
            saved["version"] = version.into();
            let records = saved["drawings"].as_array_mut().unwrap();
            records.truncate(1);
            let settings = &mut records[0]["drawing"];
            settings["tool_type"] = serde_json::to_value(tool).unwrap();
            settings["stroke_width"] = width.into();
            settings["color"] = serde_json::json!([10, 20, 30, alpha]);
            let (loaded, skipped) = DrawingStorage::from_json(&saved.to_string()).unwrap();
            assert!(skipped.is_empty(), "{tool:?} v{version}: {skipped:?}");
            let drawing = &loaded.drawings[0].drawing;
            assert_eq!(
                (drawing.stroke_width, drawing.color),
                (want_width, [10, 20, 30, want_alpha]),
                "{tool:?} v{version}"
            );
            let (again, _) = DrawingStorage::from_json(&loaded.to_json().unwrap()).unwrap();
            assert_eq!(
                again.drawings[0].drawing.stroke_width, want_width,
                "{tool:?} v{version}: converted twice"
            );
        }

        // Settings from a file saved by version 4, before the new fields and placement counts.
        for (tool, count, origins) in [
            (DrawingToolType::Note, 1, &[0][..]),
            (DrawingToolType::Signpost, 1, &[0][..]),
            (DrawingToolType::PriceNote, 1, &[0, 0][..]),
            (DrawingToolType::ArrowMarker, 1, &[0, 0][..]),
            (DrawingToolType::RotatedRect, 2, &[0, 1, 1][..]),
            (DrawingToolType::DoubleCurve, 2, &[0, 0, 1, 1][..]),
            (DrawingToolType::FibonacciWedge, 2, &[0, 1, 1][..]),
            (DrawingToolType::Pitchfan, 2, &[0, 1, 1][..]),
            (DrawingToolType::BarsPattern, 2, &[0, 1][..]),
            (DrawingToolType::ElliottDoubleCombo, 3, &[0, 0, 1, 2][..]),
            (
                DrawingToolType::ElliottTripleCombo,
                5,
                &[0, 0, 1, 2, 3, 4][..],
            ),
            (DrawingToolType::ElliottDoubleCombo, 2, &[0, 0, 1][..]),
        ] {
            let mut saved: serde_json::Value =
                serde_json::from_str(include_str!("fixtures/storage_v2.json")).unwrap();
            saved["version"] = 4.into();
            let records = saved["drawings"].as_array_mut().unwrap();
            records.truncate(1);
            records[0]["drawing"]["tool_type"] = serde_json::to_value(tool).unwrap();
            let points = records[0]["points"].as_array_mut().unwrap();
            let first = points[0].clone();
            points.resize(count, first);
            for (at, point) in records[0]["points"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .enumerate()
            {
                point["subms_ns"] = (at + 1).into();
            }
            let original = records[0]["points"].as_array().unwrap().clone();
            let (loaded, skipped) = DrawingStorage::from_json(&saved.to_string()).unwrap();
            assert!(skipped.is_empty(), "{tool:?}: {skipped:?}");
            let points = &loaded.drawings[0].points;
            assert_eq!(points.len(), origins.len(), "{tool:?}");
            for (point, &origin) in points.iter().zip(origins) {
                assert_eq!(
                    point.ts_ms,
                    original[origin]["ts_ms"].as_i64().unwrap(),
                    "{tool:?}"
                );
                assert_eq!(
                    u64::from(point.subms_ns),
                    original[origin]["subms_ns"].as_u64().unwrap(),
                    "{tool:?}"
                );
            }
            let points = serde_json::to_value(points).unwrap();
            for anchor in original {
                assert!(points.as_array().unwrap().contains(&anchor), "{tool:?}");
            }
            let (again, skipped) = DrawingStorage::from_json(&loaded.to_json().unwrap()).unwrap();
            assert!(skipped.is_empty());
            assert_eq!(again.drawings[0].points.len(), origins.len());
        }

        let mut first = Drawing::new(1, DrawingToolType::FixedRangeVolumeProfile);
        first.volume_profile = VolumeProfileConfig::new(24, 0.7);
        first.chart_points = vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(10.0, 110.0)];
        let mut storage = DrawingStorage::new("AAPL");
        storage.add_drawing(drawing_to_stored(&first, bar_to_time));
        let mut second = Drawing::new(2, DrawingToolType::TrendLine);
        second.chart_points = first.chart_points.clone();
        storage.add_drawing(drawing_to_stored(&second, bar_to_time));
        let current = storage.to_json().unwrap();
        // The first drawing as `tool` with only `anchors` anchors, written as `to_json` would
        // without its checks.
        let short_of_anchors = |tool, anchors| {
            let mut storage = storage.clone();
            storage.drawings[0].drawing.tool_type = tool;
            storage.drawings[0].points.truncate(anchors);
            serde_json::to_string_pretty(&storage).unwrap()
        };
        let with_level_width = |width| {
            let mut value = serde_json::to_value(&storage).unwrap();
            value["drawings"][0]["drawing"]["pitchfork"] = serde_json::json!({
                "levels":[{"value":1.0,"visible":true,"color":[1,2,3,255],"line_style":"Solid","width":width}],
                "show_background":false,"background_opacity":51
            });
            serde_json::to_string(&value).unwrap()
        };
        let with_vwap_multiplier = |multiplier| {
            let mut value = serde_json::to_value(&storage).unwrap();
            let band = |m| serde_json::json!({ "enabled": true, "multiplier": m });
            value["drawings"][0]["drawing"]["vwap"] = serde_json::json!({
                "bands": [band(multiplier), band(2.0), band(3.0)]
            });
            serde_json::to_string(&value).unwrap()
        };
        let unreadable = [
            (
                with_vwap_multiplier(0.0),
                "anchored VWAP band multiplier out of bounds",
                &["2"][..],
                0,
                "invalid",
            ),
            (
                with_vwap_multiplier(10.5),
                "anchored VWAP band multiplier out of bounds",
                &["2"][..],
                0,
                "invalid",
            ),
            (
                with_level_width(0.0),
                "out-of-range pitchfork.levels[0].width",
                &["2"][..],
                0,
                "invalid",
            ),
            (
                with_level_width(101.0),
                "out-of-range pitchfork.levels[0].width",
                &["2"][..],
                0,
                "invalid",
            ),
            (
                current.replacen("\"ts_ms\":", "\"subms_ns\": 1000000, \"ts_ms\":", 1),
                "anchor timestamp is outside the supported range",
                &["2"][..],
                0,
                "invalid",
            ),
            (
                current.replacen("\"stroke_width\": 2.0", "\"stroke_width\": null", 1),
                "invalid type: null",
                &["2"][..],
                0,
                "invalid",
            ),
            (
                current.replacen("\"rows\": 24", "\"rows\": 0", 1),
                "volume profile settings out of bounds",
                &["2"],
                0,
                "invalid",
            ),
            (
                current.replace("FixedRangeVolumeProfile", "NewerTool"),
                "unknown drawing tool \"NewerTool\"",
                &["2"],
                0,
                "unknown tool NewerTool",
            ),
            (
                current.replace("FixedRangeVolumeProfile", "Eraser"),
                "Eraser is a cursor, not a drawing",
                &["2"],
                0,
                "cursor Eraser",
            ),
            (
                current.replacen("\"stroke_width\": 2.0", "\"stroke_width\": 1e39", 1),
                "out-of-range stroke_width",
                &["2"],
                0,
                "invalid",
            ),
            (
                current.replacen("\"stroke_width\": 2.0", "\"stroke_width\": -1.0", 1),
                "out-of-range stroke_width",
                &["2"],
                0,
                "invalid",
            ),
            (
                current.replacen("\"font_size\": 14.0", "\"font_size\": 100000.0", 1),
                "out-of-range font_size",
                &["2"],
                0,
                "invalid",
            ),
            (
                short_of_anchors(DrawingToolType::TrendLine, 0),
                "Trend Line needs 2 anchors, has 0",
                &["2"],
                0,
                "TrendLine has 0 of 2 anchors",
            ),
            (
                short_of_anchors(DrawingToolType::Rect, 1),
                "Rectangle needs 2 anchors, has 1",
                &["2"],
                0,
                "Rect has 1 of 2 anchors",
            ),
            (
                current.replacen("\"id\": \"2\"", "\"id\": \"1\"", 1),
                "duplicate drawing id \"1\"",
                &["1"],
                1,
                "duplicate 1",
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
                0,
                "unknown tool Newer Tool",
            ),
        ];
        let reason = |skipped: &SkippedDrawing| match &skipped.error {
            SkipReason::UnknownTool(error) => format!("unknown tool {}", error.input),
            SkipReason::Cursor { tool } => format!("cursor {tool:?}"),
            SkipReason::MissingAnchors {
                tool,
                anchors,
                needed,
            } => format!("{tool:?} has {anchors} of {needed} anchors"),
            SkipReason::DuplicateId { id } => format!("duplicate {id}"),
            SkipReason::Invalid(_) => "invalid".into(),
        };
        for (json, error, loaded, index, expected) in unreadable {
            let (storage, skipped) = DrawingStorage::from_json(&json).unwrap();
            let ids: Vec<_> = storage.drawings().iter().map(|d| d.id.as_str()).collect();
            assert_eq!(ids, loaded);
            assert_eq!(skipped.len(), 1);
            assert_eq!(skipped[0].index, index);
            assert_eq!(reason(&skipped[0]), expected, "{skipped:?}");
            assert_eq!(skipped[0].id.as_deref(), Some("1"));
            assert!(skipped[0].error.to_string().contains(error), "{skipped:?}");
            assert!(storage.to_json().is_ok(), "{error}");
        }

        // A pitchfork finished after one click has two anchors and draws a mark and a segment each.
        let (storage, skipped) =
            DrawingStorage::from_json(&short_of_anchors(DrawingToolType::Pitchfork, 2)).unwrap();
        assert!(skipped.is_empty(), "{skipped:?}");
        assert_eq!(storage.drawings().len(), 2);

        // A drawing saved with the former Paintbrush tool, which drew as a brush, loads as one.
        let paintbrush = current.replace("\"TrendLine\"", "\"Paintbrush\"");
        let (storage, skipped) = DrawingStorage::from_json(&paintbrush).unwrap();
        assert!(skipped.is_empty(), "{skipped:?}");
        assert_eq!(
            storage.drawings()[1].drawing.tool_type,
            DrawingToolType::Brush
        );

        // A filter naming no timeframe loads as visible on every timeframe.
        let hidden = current.replacen("\"show_on_all\": true", "\"show_on_all\": false", 1);
        let (storage, skipped) = DrawingStorage::from_json(&hidden).unwrap();
        assert!(skipped.is_empty(), "{skipped:?}");
        assert!(
            storage.drawings()[0]
                .drawing
                .timeframe_visibility
                .show_on_all
        );
        // A filter holding only a range keeps it.
        let ranged = current.replacen(
            "\"show_on_all\": true",
            "\"show_on_all\": false, \"ranges\": {\"Hour\": {\"start\": 1, \"end\": 4}}",
            1,
        );
        let (storage, skipped) = DrawingStorage::from_json(&ranged).unwrap();
        assert!(skipped.is_empty(), "{skipped:?}");
        let visibility = &storage.drawings()[0].drawing.timeframe_visibility;
        assert!(!visibility.show_on_all);
        assert!(visibility.is_visible_on("2h") && !visibility.is_visible_on("1D"));

        let error = DrawingStorage::from_json(&current.replace("\"version\": 5", "\"version\": 6"))
            .unwrap_err();
        assert!(
            matches!(&error, LoadError::UnsupportedVersion { found: 6, supported } if *supported == (1..=5)),
            "{error}"
        );
    }

    #[test]
    fn to_json_refuses_what_the_next_load_would_lose() {
        fn first(storage: &mut DrawingStorage) -> &mut StoredDrawing {
            &mut storage.drawings[0]
        }
        type Spoil = fn(&mut DrawingStorage);
        let cases: [(Spoil, &str); 24] = [
            (
                |s| {
                    first(s)
                        .drawing
                        .pitchfork
                        .get_or_insert_with(Default::default)
                        .levels[0]
                        .width = Some(f32::NAN)
                },
                "invalid pitchfork.levels[0].width",
            ),
            (
                |s| {
                    first(s)
                        .drawing
                        .pitchfork
                        .get_or_insert_with(Default::default)
                        .levels[0]
                        .width = Some(0.0)
                },
                "invalid pitchfork.levels[0].width",
            ),
            (
                |s| first(s).drawing.position_sizing.leverage = f64::NAN,
                "invalid position_sizing",
            ),
            (
                |s| first(s).drawing.screen_anchor = Some([f32::NAN, 0.5]),
                "invalid screen_anchor",
            ),
            (
                |s| first(s).points[0].ts_ms = i64::MAX,
                "anchor 0 has no time mapping",
            ),
            (
                |s| first(s).points[0].subms_ns = 1_000_000,
                "anchor 0 has no time mapping",
            ),
            (
                |s| first(s).drawing.stroke_width = f32::NAN,
                "invalid stroke_width",
            ),
            (
                |s| first(s).drawing.font_size = f32::INFINITY,
                "invalid font_size",
            ),
            (
                |s| first(s).drawing.stroke_width = 3e38,
                "invalid stroke_width",
            ),
            (|s| first(s).drawing.font_size = 0.0, "invalid font_size"),
            (
                |s| first(s).points[1].price = f64::NAN,
                "invalid points[1].price",
            ),
            (
                |s| {
                    first(s).drawing.position_size = Some(PositionSize::RiskPercent {
                        account_size: 25_000.0,
                        percent: f64::NEG_INFINITY,
                    })
                },
                "invalid position_size",
            ),
            (
                |s| first(s).drawing.fib_config.as_mut().unwrap().level_width = 0.0,
                "invalid fib_config.level_width",
            ),
            (
                |s| first(s).drawing.fib_config.as_mut().unwrap().level_width = f32::NAN,
                "invalid fib_config.level_width",
            ),
            (
                |s| first(s).drawing.fib_config.as_mut().unwrap().level_width = 101.0,
                "invalid fib_config.level_width",
            ),
            (
                |s| first(s).drawing.fib_config.as_mut().unwrap().levels[0].value = f32::NAN,
                "invalid fib_config.levels[0].value",
            ),
            (
                |s| {
                    first(s)
                        .drawing
                        .pitchfork
                        .get_or_insert_with(Default::default)
                        .levels[0]
                        .value = f32::NAN
                },
                "invalid pitchfork.levels[0].value",
            ),
            (
                |s| {
                    first(s)
                        .drawing
                        .pitchfork
                        .get_or_insert_with(Default::default)
                        .levels[0]
                        .value = 3e38
                },
                "invalid pitchfork.levels[0].value",
            ),
            (
                |s| {
                    first(s)
                        .drawing
                        .gann
                        .get_or_insert_with(Default::default)
                        .price_levels[0]
                        .value = f32::INFINITY
                },
                "invalid gann.price_levels[0].value",
            ),
            (
                |s| {
                    first(s)
                        .drawing
                        .gann
                        .get_or_insert_with(Default::default)
                        .time_levels[0]
                        .value = f32::NAN
                },
                "invalid gann.time_levels[0].value",
            ),
            (
                |s| {
                    first(s)
                        .drawing
                        .ghost_feed
                        .get_or_insert_with(Default::default)
                        .average_range_ticks = f64::NAN
                },
                "invalid ghost_feed",
            ),
            (|s| s.drawings.push(s.drawings[0].clone()), "duplicate id"),
            (|s| first(s).points.truncate(1), "Trend Line needs 2"),
            (
                |s| first(s).drawing.tool_type = DrawingToolType::Eraser,
                "a cursor",
            ),
        ];
        for (spoil, expected) in cases {
            let mut storage = DrawingStorage::new("AAPL");
            storage.add_drawing(drawing_to_stored(
                &non_default_drawing(DrawingToolType::TrendLine),
                bar_to_time,
            ));
            spoil(&mut storage);
            let got = match storage.to_json() {
                Err(SaveError::NonFinite { id, field }) => format!("{id}: invalid {field}"),
                Err(SaveError::DuplicateId { id }) => format!("{id}: duplicate id"),
                Err(SaveError::Cursor { id }) => format!("{id}: a cursor"),
                Err(SaveError::UnmappedAnchor { id, point }) => {
                    format!("{id}: anchor {point} has no time mapping")
                }
                Err(SaveError::MissingAnchors {
                    id, tool, needed, ..
                }) => {
                    format!("{id}: {tool} needs {needed}")
                }
                other => panic!("{expected}: {:?}", other.map(drop)),
            };
            assert_eq!(got, format!("7: {expected}"));
        }
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

    #[test]
    fn adding_an_existing_id_replaces_the_drawing_and_round_trips() {
        let mut storage = DrawingStorage::new("AAPL");
        for (id, tool, created_at) in [
            ("1", DrawingToolType::TrendLine, 10),
            ("2", DrawingToolType::Ray, 20),
            ("1", DrawingToolType::HorizontalLine, 30),
        ] {
            let mut stored = StoredDrawing::new(id, Drawing::new(0, tool));
            stored.points = vec![StoredCoord::new(T0, 100.0), StoredCoord::new(T0, 110.0)];
            stored.created_at = created_at;
            storage.add_drawing(stored);
        }
        let (loaded, skipped) = DrawingStorage::from_json(&storage.to_json().unwrap()).unwrap();
        assert!(skipped.is_empty(), "{skipped:?}");
        let got: Vec<_> = loaded
            .drawings()
            .iter()
            .map(|d| (d.id.as_str(), d.drawing.tool_type, d.created_at))
            .collect();
        assert_eq!(
            got,
            [
                ("1", DrawingToolType::HorizontalLine, 10),
                ("2", DrawingToolType::Ray, 20)
            ]
        );
    }
}
