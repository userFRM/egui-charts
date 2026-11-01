//! Shared validation for host drawing edits, and the sizes drawings are drawn at.

use super::{Drawing, DrawingToolType};

/// The widest stroke in points a drawing is drawn with; the properties editor offers 1 to 10.
pub(crate) const MAX_STROKE_WIDTH: f32 = 100.0;

/// The largest pitchfork level, as a multiple of the tines' distance from the median, that is
/// drawn. Larger levels lie far outside the chart, and near `f32::MAX` overflow its geometry.
pub(crate) const MAX_PITCHFORK_LEVEL: f32 = 100.0;

/// The distance from bar 0 at which no chart with fewer than 2^35 bars maps a bar index to a
/// time: even one nanosecond shared by every bar puts it more than `i64::MAX` nanoseconds past
/// the outermost bar, and a calendar interval more than `u32::MAX` months. Saving refuses such
/// an anchor unless it still holds the time it was restored with.
const UNMAPPABLE_BAR_INDEX: f32 = 1e30;

/// Checks a drawing created through `add_drawing` or moved through `set_drawing_points`: what
/// saving needs, with exactly the anchors the tool places.
pub(super) fn validate(drawing: &Drawing) -> Result<(), &'static str> {
    check(drawing, true)
}

/// Checks what saving needs: a drawing tool rather than a cursor, at least the anchors the tool
/// draws with, finite coordinates that can map to a time, and settings in range.
pub(super) fn validate_stored(drawing: &Drawing) -> Result<(), &'static str> {
    check(drawing, false)
}

fn check(drawing: &Drawing, exact: bool) -> Result<(), &'static str> {
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
    // An anchor that still holds its restored time needs no mapping, as when saving.
    if drawing.chart_points.iter().any(|p| {
        p.bar_idx.abs() >= UNMAPPABLE_BAR_INDEX
            && p.time_anchor.is_none_or(|(_, index)| index != p.bar_idx)
    }) {
        return Err("Drawing anchors must map to a time");
    }
    let stored = super::persistence::drawing_to_stored(drawing, |_| 0);
    if stored.invalid_field().is_some() {
        return Err("Drawing settings must be finite and in range");
    }
    Ok(())
}
