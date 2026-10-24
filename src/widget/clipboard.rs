//! Drawing clipboard uses the existing validated, timestamp-based storage format.

use crate::drawings::persistence::{DrawingStorage, SaveError, stored_to_drawing};
use crate::widget::{Chart, sync::index_at};

const PREFIX: &str = "egui-charts-drawings\n";

impl Chart {
    /// Copy the selected drawings as versioned text for the host's system clipboard.
    /// Returns `None` for an empty selection. Anchors retain timestamps across intervals.
    pub fn copy_selected_drawings(&self) -> Result<Option<String>, SaveError> {
        let ids: Vec<_> = self.drawings.selected_ids().collect();
        if ids.is_empty() {
            return Ok(None);
        }
        let storage = self.store_drawings(
            self.drawings
                .drawings
                .iter()
                .filter(|d| ids.contains(&d.id)),
        )?;
        Ok(Some(format!("{PREFIX}{}", storage.to_json()?)))
    }

    /// Paste drawing clipboard text as one undoable addition, with new identities and a
    /// three-bar offset. Selects the copies. Rejects invalid or partially unreadable data,
    /// another symbol, or a chart without a time mapping before changing any drawing.
    /// The host handles the system clipboard and forwards its paste event here.
    pub fn paste_drawings(&mut self, text: &str) -> Result<(), serde_json::Error> {
        let json = text
            .strip_prefix(PREFIX)
            .ok_or_else(|| serde::de::Error::custom("not a drawing clipboard"))?;
        let (storage, skipped) = DrawingStorage::from_json(json)?;
        if storage.symbol != self.symbol() || !skipped.is_empty() {
            return Err(serde::de::Error::custom(
                "clipboard has another symbol or unreadable drawings",
            ));
        }
        let mut copies = Vec::new();
        for stored in storage.drawings() {
            for point in &stored.points {
                if chrono::DateTime::from_timestamp_millis(point.ts_ms)
                    .and_then(|time| index_at(self.state.shown(), time))
                    .is_none()
                {
                    return Err(serde::de::Error::custom(
                        "clipboard needs loaded chart history",
                    ));
                }
            }
            copies.push(stored_to_drawing(
                stored,
                |ts| {
                    index_at(
                        self.state.shown(),
                        chrono::DateTime::from_timestamp_millis(ts).expect("validated timestamp"),
                    )
                    .expect("validated time mapping") as f32
                },
                0,
            ));
        }
        self.user_action(|chart| chart.drawings.paste_drawings(&copies));
        Ok(())
    }
}
