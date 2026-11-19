//! Reusable drawing properties without geometry or record identity.

/// Drawing properties without anchors or record identity. Apply to an existing drawing of the
/// same tool. Its coordinates, closed outline, name, notes, layer, post link and persistence
/// identity stay intact. So do its lock, user edit permission, visibility, modification time and image frame size, and,
/// on a tool that shows no text, the picture, sticker or icon its text holds.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
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
        settings.post = None;
        settings.points.clear();
        settings.chart_points.clear();
        settings.level_ys.clear();
        settings.screen_anchor = None;
        settings.text_zoom_spacing = None;
        settings.pane = None;
        settings.group = None;
        settings.folder.clear();
        settings.folder_sync.clear();
        settings.selectable = true;
        settings.saveable = true;
        settings.listed = true;
        settings.user_edit_enabled = true;
        settings.completed = false;
        Self { settings }
    }

    /// Apply the template's appearance without moving or replacing the drawing. Hosts apply it to
    /// every selected drawing that accepts user edits of the template's tool in one
    /// `DrawingManager::edit_drawings` call, which keeps it one undoable step with persistence
    /// notifications.
    pub fn apply_to(&self, drawing: &mut crate::drawings::Drawing) -> Result<(), &'static str> {
        if self.settings.tool_type != drawing.tool_type
            || self.settings.custom_tool != drawing.custom_tool
        {
            return Err("Select a drawing of the template's type");
        }
        // The checks a saved drawing passes on load, as defaults come from saved settings.
        if crate::drawings::persistence::drawing_to_stored(&self.settings, |_| 0)
            .invalid_field()
            .is_some()
        {
            return Err("Invalid drawing template dimensions");
        }
        *drawing = crate::drawings::Drawing {
            id: drawing.id,
            stored_id: drawing.stored_id.clone(),
            created_at: drawing.created_at,
            notes: drawing.notes.clone(),
            label: drawing.label.clone(),
            post: drawing.post.clone(),
            points: drawing.points.clone(),
            chart_points: drawing.chart_points.clone(),
            level_ys: drawing.level_ys.clone(),
            completed: drawing.completed,
            screen_anchor: drawing.screen_anchor,
            text_zoom_spacing: drawing.text_zoom_spacing,
            pane: drawing.pane,
            group: drawing.group.clone(),
            folder: drawing.folder.clone(),
            folder_sync: drawing.folder_sync.clone(),
            selectable: drawing.selectable,
            saveable: drawing.saveable,
            listed: drawing.listed,
            user_edit_enabled: drawing.user_edit_enabled,
            closed: drawing.closed,
            z_order: drawing.z_order,
            locked: drawing.locked,
            visible: drawing.visible,
            modified_at: drawing.modified_at,
            // A tool that shows no text holds its picture, sticker or icon there.
            text: if drawing.tool_type.supports_text() {
                self.settings.text.clone()
            } else {
                drawing.text.clone()
            },
            // Only the opacity: each picture keeps its own frame size.
            image: match (drawing.image, self.settings.image) {
                (None, None) => None,
                (own, template) => Some(crate::drawings::ImageConfig {
                    size: own.and_then(|image| image.size),
                    opacity: template.map_or(255, |image| image.opacity),
                }),
            },
            ..self.settings.clone()
        };
        Ok(())
    }
}
