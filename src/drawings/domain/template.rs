//! Reusable drawing properties without geometry or record identity.

/// Drawing properties without anchors or record identity. Apply to an existing drawing of the
/// same type; its coordinates, name, notes, layer and persistence identity stay intact.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct DrawingTemplate {
    /// Tool and reusable properties. Serialization omits anchors and record identity.
    pub settings: crate::drawings::Drawing,
}

impl DrawingTemplate {
    /// Capture the properties of a selected drawing.
    pub fn from_drawing(drawing: &crate::drawings::Drawing) -> Self {
        let mut settings = drawing.clone();
        settings.id = 0;
        settings.stored_id = None;
        settings.created_at = None;
        settings.notes = None;
        settings.label = None;
        settings.points.clear();
        settings.chart_points.clear();
        settings.level_ys.clear();
        settings.screen_anchor = None;
        settings.completed = false;
        Self { settings }
    }

    /// Apply properties without moving or replacing the selected drawing. Hosts wrap this edit
    /// in their drawing manager's `edit_drawing` to retain undo and persistence notifications.
    pub fn apply_to(&self, drawing: &mut crate::drawings::Drawing) -> Result<(), &'static str> {
        if self.settings.tool_type != drawing.tool_type {
            return Err("Select a drawing of the template's type");
        }
        if !self.settings.stroke_width.is_finite()
            || self.settings.stroke_width <= 0.0
            || !self.settings.font_size.is_finite()
            || self.settings.font_size <= 0.0
        {
            return Err("Invalid drawing template dimensions");
        }
        *drawing = crate::drawings::Drawing {
            id: drawing.id,
            stored_id: drawing.stored_id.clone(),
            created_at: drawing.created_at,
            notes: drawing.notes.clone(),
            label: drawing.label.clone(),
            points: drawing.points.clone(),
            chart_points: drawing.chart_points.clone(),
            level_ys: drawing.level_ys.clone(),
            completed: drawing.completed,
            screen_anchor: drawing.screen_anchor,
            z_order: drawing.z_order,
            ..self.settings.clone()
        };
        Ok(())
    }
}
