//! Shared validation for host drawing edits, and the sizes drawings are drawn at.

use super::{Drawing, DrawingToolType};
use crate::model::{Timeframe, TimezoneId};
use chrono::{DateTime, Utc};
use std::ops::RangeInclusive;

/// The widest stroke in points a drawing is drawn with; the properties editor offers 1 to 10.
pub(crate) const MAX_STROKE_WIDTH: f32 = 100.0;

/// Largest side of an image frame, in points.
pub(crate) const MAX_IMAGE_SIDE: f32 = 10_000.0;

/// Narrowest column and lowest row, in points, that dragging a table or its settings sets.
pub(crate) const MIN_TABLE_CELL: f32 = 8.0;
/// Widest column and highest row of a table, in points.
pub(crate) const MAX_TABLE_CELL: f32 = 10_000.0;

/// Widest line of wrapped annotation text, in points.
pub(crate) const MAX_TEXT_WRAP_WIDTH: f32 = 10_000.0;

/// The largest pitchfork level, as a multiple of the tines' distance from the median, that is
/// drawn. Larger levels lie far outside the chart, and near `f32::MAX` overflow its geometry.
pub(crate) const MAX_PITCHFORK_LEVEL: f32 = 100.0;

/// The largest projection coefficient, as a multiple of the sector's radius, that is drawn.
/// Larger levels lie far outside the chart.
pub(crate) const MAX_PROJECTION_LEVEL: f32 = 100.0;

/// The distance from bar 0 at which no chart with fewer than 2^35 bars maps a bar index to a
/// time: even one nanosecond shared by every bar puts it more than `i64::MAX` nanoseconds past
/// the outermost bar, and a calendar interval more than `u32::MAX` months. Saving refuses such
/// an anchor unless it still holds the time it was restored with. It is the only bound for a
/// manager outside a chart and on a chart without bars.
const UNMAPPABLE_BAR_INDEX: f32 = 1e30;

/// The shown bars' count, revision and edge times, calendar interval and calendar zone from
/// which a chart worked out its mapped bar indices.
pub(crate) type MappingKey = (
    usize,
    u64,
    Option<DateTime<Utc>>,
    Option<DateTime<Utc>>,
    Option<Timeframe>,
    Option<TimezoneId>,
);

/// Finds the outermost `f32` indices that map to a time, on a chart with bars from 0 to `last`.
/// Outside the bars time is extrapolated monotonically, so each bound lies between a known
/// mapped index and [`UNMAPPABLE_BAR_INDEX`], which no chart maps.
pub(crate) fn mapped_bars(last: usize, maps: impl Fn(f64) -> bool) -> RangeInclusive<f32> {
    let farthest = |from: f32, sign: f32| {
        // Non-negative `f32` values are ordered as their bits are.
        let (mut inside, mut outside) = (from.to_bits(), UNMAPPABLE_BAR_INDEX.to_bits());
        while outside - inside > 1 {
            let middle = inside + (outside - inside) / 2;
            if maps(f64::from(sign * f32::from_bits(middle))) {
                inside = middle;
            } else {
                outside = middle;
            }
        }
        sign * f32::from_bits(inside)
    };
    farthest(0.0, -1.0)..=farthest(last as f32, 1.0)
}

/// Checks a drawing created through `add_drawing` or moved through `set_drawing_points`: what
/// saving needs, with exactly the anchors the tool places.
pub(super) fn validate(
    drawing: &Drawing,
    mapped: Option<&RangeInclusive<f32>>,
) -> Result<(), &'static str> {
    check(drawing, true, mapped)
}

/// Checks what saving needs: a drawing tool rather than a cursor, at least the anchors the tool
/// draws with, finite coordinates that can map to a time, and settings in range.
pub(super) fn validate_stored(
    drawing: &Drawing,
    mapped: Option<&RangeInclusive<f32>>,
) -> Result<(), &'static str> {
    check(drawing, false, mapped)
}

/// Checks an edit that turned `old` into `new`: refuses one after which saving would refuse a
/// drawing it accepted before, with the reason [`validate_stored`] gives. A drawing that saving
/// refuses already, such as one a host changed directly through `DrawingManager::drawings`,
/// takes any edit, so that it can still be hidden, locked, renamed or filed.
pub(super) fn validate_edit(
    old: &Drawing,
    new: &Drawing,
    mapped: Option<&RangeInclusive<f32>>,
) -> Result<(), &'static str> {
    match validate_stored(new, mapped) {
        Err(reason) if validate_stored(old, mapped).is_ok() => Err(reason),
        _ => Ok(()),
    }
}

fn check(
    drawing: &Drawing,
    exact: bool,
    mapped: Option<&RangeInclusive<f32>>,
) -> Result<(), &'static str> {
    let tool = drawing.tool_type;
    if tool.is_cursor() {
        return Err("Cursor tools do not create drawings");
    }
    let count = drawing.chart_points.len();
    let invalid_count = if exact {
        // Tools that store more anchors than the clicks that place them.
        let required = match tool {
            DrawingToolType::LongPos
            | DrawingToolType::ShortPos
            | DrawingToolType::Curve
            | DrawingToolType::BarsPattern => Some(3),
            DrawingToolType::DoubleCurve => Some(4),
            _ => tool.required_points(),
        };
        drawing.custom_tool.is_none() && required.map_or(count < 2, |required| count != required)
    } else {
        count < tool.anchors_needed()
    };
    if invalid_count {
        return Err("Invalid drawing anchor count");
    }
    if drawing
        .chart_points
        .iter()
        .any(|p| !p.bar_idx.is_finite() || !p.price.is_finite())
    {
        return Err("Drawing coordinates must be finite");
    }
    // Retained time anchors need no mapping, as when saving. Without chart bars, refuse only
    // indices that no chart maps.
    if drawing.chart_points.iter().any(|p| {
        let maps = mapped.map_or(p.bar_idx.abs() < UNMAPPABLE_BAR_INDEX, |range| {
            range.contains(&p.bar_idx)
        });
        !maps && p.time_anchor.is_none_or(|(_, index)| index != p.bar_idx)
    }) {
        return Err("Drawing anchors must map to a time");
    }
    let stored = super::persistence::drawing_to_stored(drawing, |_| 0);
    if stored.invalid_field().is_some() {
        return Err("Drawing settings must be finite and in range");
    }
    Ok(())
}
