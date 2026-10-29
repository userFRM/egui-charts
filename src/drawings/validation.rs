//! Shared validation for host drawing edits, and the sizes drawings are drawn at.

use super::{Drawing, DrawingToolType};

/// The widest stroke in points a drawing is drawn with; the properties editor offers 1 to 10.
pub(crate) const MAX_STROKE_WIDTH: f32 = 100.0;

/// The largest pitchfork level, as a multiple of the tines' distance from the median, that is
/// drawn. Larger levels lie far outside the chart, and near `f32::MAX` overflow its geometry.
pub(crate) const MAX_PITCHFORK_LEVEL: f32 = 100.0;

pub(super) fn validate(drawing: &Drawing) -> Result<(), &'static str> {
    let tool = drawing.tool_type;
    if tool.is_cursor() {
        return Err("Cursor tools do not create drawings");
    }
    let count = drawing.chart_points.len();
    let required = match tool {
        DrawingToolType::LongPos | DrawingToolType::ShortPos => Some(3),
        _ => tool.required_points(),
    };
    if drawing.custom_tool.is_none() && required.map_or(count < 2, |required| count != required) {
        return Err("Invalid drawing anchor count");
    }
    if drawing
        .chart_points
        .iter()
        .any(|p| !p.bar_idx.is_finite() || !p.price.is_finite())
    {
        return Err("Drawing coordinates must be finite");
    }
    let stored = super::persistence::drawing_to_stored(drawing, |_| 0);
    if stored.invalid_field().is_some() {
        return Err("Drawing settings must be finite and in range");
    }
    Ok(())
}
