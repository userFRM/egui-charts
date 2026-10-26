//! Shared validation for host drawing edits.

use super::{Drawing, DrawingToolType};

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
    if drawing.stroke_width <= 0.0 || drawing.font_size <= 0.0 {
        return Err("Drawing dimensions must be positive");
    }
    let stored = super::persistence::drawing_to_stored(drawing, |_| 0);
    if stored.non_finite_field().is_some() {
        return Err("Drawing settings must be finite");
    }
    Ok(())
}
