//! Time anchors behind editable chart coordinates.

use super::{
    Chart,
    sync::{index_at, time_at},
};
use crate::drawings::persistence::{
    DrawingStorage, SaveError, SkippedDrawing, drawing_to_stored, stored_to_drawing,
};
use crate::drawings::{ChartPoint, Drawing, DrawingManager};

impl Chart {
    /// UTC time behind a drawing anchor, retaining fractional and repeated-time positions.
    /// Uses the current rendered bars when its index was edited.
    pub fn drawing_point_time(&self, point: &ChartPoint) -> Option<chrono::DateTime<chrono::Utc>> {
        point
            .time_anchor
            .filter(|(_, index)| *index == point.bar_idx)
            .map(|(time, _)| time)
            .or_else(|| time_at(self.state.shown(), point.bar_idx as f64))
    }

    /// Map a UTC time and price into drawing coordinates, including times outside loaded bars.
    /// Returns `None` without displayed bars or for a non-finite price or projected index.
    pub fn drawing_point_at(
        &self,
        time: chrono::DateTime<chrono::Utc>,
        price: f64,
    ) -> Option<ChartPoint> {
        let index = index_at(self.state.shown(), time)? as f32;
        if !index.is_finite() || !price.is_finite() {
            return None;
        }
        let mut point = ChartPoint::new(index, price);
        point.time_anchor = Some((time, index));
        Some(point)
    }

    /// Captures edited indices before the bars or the rendered element sequence change.
    pub(super) fn anchor_drawing_points(&mut self) {
        let bars = self.state.shown();
        let mut anchor = |point: &mut ChartPoint| {
            if point
                .time_anchor
                .is_none_or(|(_, projected)| projected != point.bar_idx)
            {
                point.time_anchor =
                    time_at(bars, point.bar_idx as f64).map(|time| (time, point.bar_idx));
            }
        };
        self.drawings.visit_chart_points(&mut anchor);
        self.history.visit_chart_points(&mut anchor);
    }

    /// Projects retained timestamps against the current rendered bars, including whitespace.
    pub(super) fn project_drawing_points(&mut self) {
        let bars = self.state.shown();
        let mut project = |point: &mut ChartPoint| {
            if let Some((time, _)) = point.time_anchor
                && let Some(index) = index_at(bars, time)
            {
                // Several price-based elements can share a time; keep an already matching rank.
                if time_at(bars, point.bar_idx as f64) != Some(time) {
                    point.bar_idx = index as f32;
                    point.time_anchor = Some((time, point.bar_idx));
                }
            }
        };
        self.drawings.visit_chart_points(&mut project);
        self.history.visit_chart_points(&mut project);
    }

    /// Exports the current symbol's drawings with timestamps, using the chart's own mapping.
    /// Between bars time is interpolated; outside them the nearest distinct pair is extended
    /// (one second per index for a single bar). Empty charts can export retained time anchors;
    /// an index with no loaded mapping returns an error rather than clamping or inventing time.
    pub fn save_drawings(&self) -> Result<DrawingStorage, SaveError> {
        self.store_drawings(&self.drawings.drawings)
    }

    pub(super) fn store_drawings<'a>(
        &self,
        drawings: impl IntoIterator<Item = &'a Drawing>,
    ) -> Result<DrawingStorage, SaveError> {
        let mut storage = DrawingStorage::new(self.symbol());
        storage.timeframe = self
            .timeframe()
            .map(|timeframe| timeframe.as_str().into_owned());
        for drawing in drawings {
            for (at, point) in drawing.chart_points.iter().enumerate() {
                if point
                    .time_anchor
                    .is_none_or(|(_, index)| index != point.bar_idx)
                    && time_at(self.state.shown(), point.bar_idx as f64).is_none()
                {
                    return Err(SaveError::UnmappedAnchor {
                        id: drawing
                            .stored_id
                            .clone()
                            .unwrap_or_else(|| drawing.id.to_string()),
                        point: at,
                    });
                }
            }
            storage.add_drawing(drawing_to_stored(drawing, |index| {
                time_at(self.state.shown(), index as f64)
                    .expect("validated anchor")
                    .timestamp_millis()
            }));
        }
        // Validate all settings too, before a host replaces its last good save.
        storage.to_json()?;
        Ok(storage)
    }

    /// Replaces the current symbol's drawings from a saved file, clears chart undo history
    /// and reports skipped records.
    /// Rejects a different symbol or an unreadable file without changing the current drawings.
    /// Can run before history arrives: timestamp anchors are projected when bars load.
    /// Restored drawings do not queue writes or enter undo history. Screen coordinates follow
    /// on the next frame. Account, filesystem and cloud storage remain the host's responsibility.
    pub fn load_drawings(&mut self, json: &str) -> Result<Vec<SkippedDrawing>, serde_json::Error> {
        let (storage, skipped) = DrawingStorage::from_json(json)?;
        if storage.symbol != self.symbol() {
            return Err(serde::de::Error::custom(
                "drawing storage belongs to a different symbol",
            ));
        }
        let mut manager = DrawingManager::new();
        manager.options = self.drawings.options.clone();
        manager.tool_defaults = self.drawings.tool_defaults.clone();
        manager.active_tool = self.drawings.active_tool;
        manager.stay_in_drawing_mode = self.drawings.stay_in_drawing_mode;
        manager.eraser_mode = self.drawings.eraser_mode;
        manager.curr_timeframe = self.drawings.curr_timeframe.clone();
        for stored in storage.drawings() {
            let drawing = stored_to_drawing(
                stored,
                |ts| {
                    chrono::DateTime::from_timestamp_millis(ts)
                        .and_then(|time| index_at(self.state.shown(), time))
                        .unwrap_or(0.0) as f32
                },
                0,
            );
            manager.add_synced_drawing(drawing);
        }
        self.drawings = manager;
        self.history = Default::default();
        Ok(skipped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::{Drawing, DrawingToolType};
    use crate::model::{Bar, BarData};
    use crate::widget::SeriesId;
    use chrono::{TimeZone, Utc};

    #[test]
    fn saved_drawings_restore_before_bars_and_report_skipped_records() {
        let bars = || {
            [0, 60, 180]
                .map(|seconds| {
                    Bar::new(
                        Utc.timestamp_opt(seconds, 0).unwrap(),
                        10.0,
                        11.0,
                        9.0,
                        10.0,
                        1.0,
                    )
                })
                .to_vec()
        };
        let mut chart = Chart::new(BarData::from_bars(bars()));
        chart.set_symbol("A");
        let mut drawing = Drawing::new(7, DrawingToolType::TrendLine);
        drawing.chart_points = [-0.5, 1.5, 3.5]
            .map(|index| ChartPoint::new(index, 10.0))
            .to_vec();
        drawing.completed = true;
        chart.drawings_mut().drawings.push(drawing);
        let saved = chart.save_drawings().unwrap();
        chart.drawings_mut().select(7);
        let mut unrelated = chart.drawings().drawings[0].clone();
        unrelated.id = 8;
        unrelated.font_size = f32::NAN;
        chart.drawings_mut().drawings.push(unrelated);
        assert!(chart.save_drawings().is_err());
        let copied = chart.copy_selected_drawings().unwrap().unwrap();
        chart.drawings_mut().drawings.pop();
        chart.paste_drawings(&copied).unwrap();
        assert_eq!(chart.drawings().drawings.len(), 2);
        assert!(chart.undo());
        assert_eq!(chart.drawings().drawings.len(), 1);
        // -0.5 * 60 = -30 s; 60 + 0.5 * 120 = 120 s; 180 + 1.5 * 120 = 360 s.
        assert_eq!(
            saved.drawings[0]
                .points
                .iter()
                .map(|p| p.ts_ms)
                .collect::<Vec<_>>(),
            [-30_000, 120_000, 360_000]
        );
        let mut json = serde_json::to_value(&saved).unwrap();
        let valid = json["drawings"][0].clone();
        let mut bad_time = valid.clone();
        bad_time["id"] = "bad-time".into();
        bad_time["points"][0]["ts_ms"] = i64::MAX.into();
        json["drawings"]
            .as_array_mut()
            .unwrap()
            .extend([serde_json::json!({"id": "broken"}), bad_time]);
        let mut restored = Chart::new(BarData::new());
        restored.set_symbol("A");
        restored.drawings_mut().stay_in_drawing_mode = true;
        restored.drawings_mut().eraser_mode = true;
        let skipped = restored.load_drawings(&json.to_string()).unwrap();
        assert_eq!(skipped.iter().map(|s| s.index).collect::<Vec<_>>(), [1, 2]);
        assert!(restored.drawings().pending_sync.lock().unwrap().is_empty());
        assert!(!restored.can_undo());
        assert!(restored.drawings().stay_in_drawing_mode);
        assert!(restored.drawings().eraser_mode);
        // Timestamp-only drawings can be saved while history is still pending.
        assert_eq!(
            serde_json::to_value(&restored.save_drawings().unwrap().drawings[0].points).unwrap(),
            serde_json::to_value(&saved.drawings[0].points).unwrap()
        );
        restored.set_data(SeriesId::MAIN, bars());
        assert_eq!(
            restored.drawings().drawings[0].chart_points,
            chart.drawings().drawings[0].chart_points
        );
        json["symbol"] = "B".into();
        assert!(restored.load_drawings(&json.to_string()).is_err());
        assert_eq!(restored.drawings().drawings.len(), 1);
        let mut empty = Chart::new(BarData::new());
        empty.drawings_mut().drawings = chart.drawings().drawings.clone();
        assert!(empty.save_drawings().is_err());
    }
}
