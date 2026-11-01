//! Time anchors behind editable chart coordinates.

use super::Chart;
use crate::drawings::persistence::{
    DrawingStorage, LoadError, SaveError, SkipReason, SkippedDrawing, StoredCoord, StoredDrawing,
    drawing_to_stored_with, stored_to_drawing_with,
};
use crate::drawings::{ChartPoint, Drawing, DrawingManager};
use crate::model::{index_at_with_timeframe, time_at_with_timeframe};

impl Chart {
    /// UTC time behind a drawing anchor, retaining fractional and repeated-time positions.
    /// Uses the current rendered bars when its index was edited.
    pub fn drawing_point_time(&self, point: &ChartPoint) -> Option<chrono::DateTime<chrono::Utc>> {
        point
            .time_anchor
            .filter(|(_, index)| *index == point.bar_idx)
            .map(|(time, _)| time)
            .or_else(|| self.index_to_time(point.bar_idx as f64))
    }

    /// Map a UTC time and price into drawing coordinates, including times outside loaded bars.
    /// Returns `None` without displayed bars or for a non-finite price or projected index.
    pub fn drawing_point_at(
        &self,
        time: chrono::DateTime<chrono::Utc>,
        price: f64,
    ) -> Option<ChartPoint> {
        let index = self.time_to_index(time)? as f32;
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
        let timeframe = self.timeframe();
        let timezone = self.calendar_timezone();
        let mut anchor = |point: &mut ChartPoint| {
            if point
                .time_anchor
                .is_none_or(|(_, projected)| projected != point.bar_idx)
            {
                point.time_anchor = time_at_with_timeframe(
                    bars,
                    point.bar_idx as f64,
                    timeframe,
                    timezone.as_ref(),
                )
                .map(|time| (time, point.bar_idx));
            }
        };
        self.drawings.visit_chart_points(&mut anchor);
        self.history.visit_chart_points(&mut anchor);
    }

    /// Projects retained timestamps against the current rendered bars, including whitespace.
    pub(super) fn project_drawing_points(&mut self) {
        let bars = self.state.shown();
        let timeframe = self.timeframe();
        let timezone = self.calendar_timezone();
        let state = &self.state;
        let chart_type = self.chart_type;
        let params = &self.chart_type_params;
        let older_bars_drawn = self.older_bars_drawn;
        let mut project = |point: &mut ChartPoint| {
            if let Some((time, _)) = point.time_anchor {
                let index = crate::chart::rendering::candles::price_based::element_index_at(
                    state,
                    chart_type,
                    params,
                    older_bars_drawn,
                    time,
                )
                .map(|index| index as f64)
                .or_else(|| index_at_with_timeframe(bars, time, timeframe, timezone.as_ref()));
                if let Some(index) = index {
                    // Keep the rank of an anchor already on one of several equal-time elements.
                    if time_at_with_timeframe(
                        bars,
                        point.bar_idx as f64,
                        timeframe,
                        timezone.as_ref(),
                    ) != Some(time)
                    {
                        point.bar_idx = index as f32;
                        point.time_anchor = Some((time, point.bar_idx));
                    }
                }
            }
        };
        self.drawings.visit_chart_points(&mut project);
        self.history.visit_chart_points(&mut project);
    }

    /// Exports the current symbol's drawings with timestamps, using the chart's own mapping.
    /// Between bars time is interpolated; outside them the known timeframe duration or most
    /// common positive spacing among the outermost 16 bars is extended (shorter spacing breaks
    /// ties, one second if none), retaining repeated-edge ranks. Known month and year intervals
    /// step by calendar months in the display time zone. Empty charts export retained
    /// anchors; an index with no loaded mapping returns an error rather than clamping or
    /// inventing time.
    pub fn save_drawings(&self) -> Result<DrawingStorage, SaveError> {
        self.store_drawings(
            self.drawings
                .drawings
                .iter()
                .filter(|drawing| drawing.saveable),
        )
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
                    && self.index_to_time(point.bar_idx as f64).is_none()
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
            let mut stored = drawing_to_stored_with(drawing, |index, price| {
                StoredCoord::from_time(
                    self.index_to_time(index as f64).expect("validated anchor"),
                    price,
                )
            });
            stored.pane = self
                .drawing_pane(drawing.id)
                .and_then(|pane| self.stored_pane(pane));
            storage.add_drawing(stored);
        }
        // Validate all settings too, before a host replaces its last good save.
        storage.to_json()?;
        Ok(storage)
    }

    /// Replaces the current symbol's drawings from a saved file, clears chart undo history
    /// and reports skipped records.
    /// Rejects a different symbol or an unreadable file without changing the current drawings,
    /// with a [`LoadError`] that tells a file from a newer release
    /// ([`LoadError::UnsupportedVersion`]) from damaged data; each skipped record carries its
    /// [`SkipReason`].
    /// Can run before history arrives: timestamp anchors are projected when bars load.
    /// Restored drawings do not queue writes or enter undo history. Screen coordinates follow
    /// on the next frame. Account, filesystem and cloud storage remain the host's responsibility.
    pub fn load_drawings(&mut self, json: &str) -> Result<Vec<SkippedDrawing>, LoadError> {
        self.load_drawings_remapped(json, false)
    }

    /// [`Self::load_drawings`]; with `remap`, a drawing whose study was replaced when a snapshot
    /// was restored takes the replacement before its stored pane is matched, since a study's
    /// input naming another study as its source holds that study's local id.
    pub(super) fn load_drawings_remapped(
        &mut self,
        json: &str,
        remap: bool,
    ) -> Result<Vec<SkippedDrawing>, LoadError> {
        let (storage, mut skipped) = DrawingStorage::from_json(json)?;
        if storage.symbol != self.symbol() {
            return Err(LoadError::OtherSymbol);
        }
        let mut manager = DrawingManager::new();
        manager.set_sync_enabled(self.drawings.sync_enabled);
        manager.registry = self.drawings.registry.clone();
        manager.options = self.drawings.options.clone();
        manager.tool_defaults = self.drawings.tool_defaults.clone();
        manager.active_tool = self.drawings.active_tool;
        if let Some(id) = self.drawings.active_custom_tool() {
            let _ = manager.set_custom_tool(id);
        }
        manager.stay_in_drawing_mode = self.drawings.stay_in_drawing_mode;
        manager.eraser_mode = self.drawings.eraser_mode;
        manager.curr_timeframe = self.drawings.curr_timeframe.clone();
        let mut refused = Vec::new();
        // The n-th loaded drawing is the n-th entry of the file that was not skipped.
        let read = (0..).filter(|index| skipped.iter().all(|s| s.index != *index));
        for (index, stored) in read.zip(storage.drawings()) {
            if let Err(error) = manager.add_synced_drawing(self.restored_drawing(stored, remap)) {
                refused.push(SkippedDrawing {
                    index,
                    id: Some(stored.id.clone()),
                    error: SkipReason::Invalid(serde::de::Error::custom(error)),
                });
            }
        }
        skipped.extend(refused);
        skipped.sort_by_key(|skipped| skipped.index);
        self.drawings = manager;
        self.history.clear();
        Ok(skipped)
    }

    /// `stored` as a drawing of this chart: anchors at the bars of their times, exact once bars
    /// load, and in the pane of its study when the chart shows that study, else in the price pane.
    /// With `remap`, see [`Self::load_drawings_remapped`].
    pub(super) fn restored_drawing(&self, stored: &StoredDrawing, remap: bool) -> Drawing {
        let mut drawing = stored_to_drawing_with(
            stored,
            |point| {
                point
                    .time()
                    .and_then(|time| self.time_to_index(time))
                    .unwrap_or(0.0) as f32
            },
            0,
        );
        if let Some(id) = drawing
            .pane
            .filter(|_| remap)
            .and_then(|pane| self.restored_studies.get(&pane))
        {
            drawing.pane = Some(*id);
        } else if let Some(pane) = &stored.pane {
            drawing.pane = self.stored_pane_owner(pane);
        }
        drawing
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
    fn whitespace_anchors_keep_their_slots_and_axis_dates() {
        use crate::model::Timeframe;
        use chrono::Duration;

        let date = |y, m, d| Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap();
        let hourly = [
            (4, 9, 30),
            (4, 10, 0),
            (4, 11, 0),
            (4, 12, 0),
            (4, 13, 0),
            (4, 14, 0),
            (4, 15, 0),
            (5, 9, 30),
            (5, 10, 0),
            (5, 11, 0),
        ]
        .map(|(day, hour, minute)| Utc.with_ymd_and_hms(2024, 3, day, hour, minute, 0).unwrap())
        .to_vec();
        let bar = |time| Bar::new(time, 10.0, 12.0, 9.0, 11.0, 1.0);
        for (times, timeframe, timezone, expected_left, expected_right, next) in [
            // Thursday, Friday, Monday: the weekend cannot turn one slot into three days.
            (
                vec![date(2024, 2, 8), date(2024, 2, 9), date(2024, 2, 12)],
                None,
                "UTC",
                date(2024, 2, 7),
                date(2024, 2, 13),
                date(2024, 2, 13),
            ),
            // Friday, Monday, Tuesday: the same rule applies to prepended history.
            (
                vec![date(2024, 2, 2), date(2024, 2, 5), date(2024, 2, 6)],
                None,
                "UTC",
                date(2024, 2, 1),
                date(2024, 2, 7),
                date(2024, 2, 7),
            ),
            (
                vec![
                    date(2024, 1, 2) + Duration::minutes(1258),
                    date(2024, 1, 2) + Duration::minutes(1259),
                    date(2024, 1, 3) + Duration::minutes(870),
                ],
                Some(Timeframe::Min1),
                "UTC",
                date(2024, 1, 2) + Duration::minutes(1257),
                date(2024, 1, 3) + Duration::minutes(871),
                date(2024, 1, 3) + Duration::minutes(871),
            ),
            (
                vec![date(2024, 2, 1), date(2024, 3, 1), date(2024, 4, 1)],
                Some(Timeframe::Month1),
                "UTC",
                date(2024, 1, 1),
                date(2024, 5, 1),
                date(2024, 5, 1),
            ),
            (
                vec![date(2024, 7, 1), date(2024, 10, 1), date(2025, 1, 1)],
                Some("3month".parse().unwrap()),
                "UTC",
                date(2024, 4, 1),
                date(2025, 4, 1),
                date(2025, 4, 1),
            ),
            (
                vec![date(2023, 12, 31), date(2024, 1, 31), date(2024, 2, 29)],
                Some(Timeframe::Month1),
                "UTC",
                date(2023, 11, 30),
                date(2024, 3, 31),
                date(2024, 3, 31),
            ),
            (
                vec![date(2023, 1, 1), date(2024, 1, 1), date(2025, 1, 1)],
                Some("12month".parse().unwrap()),
                "UTC",
                date(2022, 1, 1),
                date(2026, 1, 1),
                date(2026, 1, 1),
            ),
            // Local month starts follow the clock change in November.
            #[cfg(feature = "timezones")]
            (
                vec![
                    date(2024, 9, 1) + Duration::hours(4),
                    date(2024, 10, 1) + Duration::hours(4),
                    date(2024, 11, 1) + Duration::hours(4),
                ],
                Some(Timeframe::Month1),
                "America/New_York",
                date(2024, 8, 1) + Duration::hours(4),
                date(2024, 12, 1) + Duration::hours(5),
                date(2024, 12, 1) + Duration::hours(5),
            ),
            // Months keyed at UTC midnight stay at UTC midnight across the clock change.
            #[cfg(feature = "timezones")]
            (
                vec![date(2024, 1, 1), date(2024, 2, 1), date(2024, 3, 1)],
                Some(Timeframe::Month1),
                "America/New_York",
                date(2023, 12, 1),
                date(2024, 4, 1),
                date(2024, 4, 1),
            ),
            // The half-hour opening bar as the last bar keeps the slots on the hourly grid.
            (
                hourly[..8].to_vec(),
                Some(Timeframe::Hour1),
                "UTC",
                Utc.with_ymd_and_hms(2024, 3, 4, 9, 0, 0).unwrap(),
                Utc.with_ymd_and_hms(2024, 3, 5, 10, 0, 0).unwrap(),
                Utc.with_ymd_and_hms(2024, 3, 5, 10, 0, 0).unwrap(),
            ),
            (
                hourly[..8].to_vec(),
                None,
                "UTC",
                Utc.with_ymd_and_hms(2024, 3, 4, 9, 0, 0).unwrap(),
                Utc.with_ymd_and_hms(2024, 3, 5, 10, 0, 0).unwrap(),
                Utc.with_ymd_and_hms(2024, 3, 5, 10, 0, 0).unwrap(),
            ),
            // Clock-aligned hours keep the half-hour opening bar out of the step.
            (
                hourly.clone(),
                Some(Timeframe::Hour1),
                "UTC",
                Utc.with_ymd_and_hms(2024, 3, 4, 9, 0, 0).unwrap(),
                Utc.with_ymd_and_hms(2024, 3, 5, 12, 0, 0).unwrap(),
                Utc.with_ymd_and_hms(2024, 3, 5, 12, 0, 0).unwrap(),
            ),
            (
                hourly,
                None,
                "UTC",
                Utc.with_ymd_and_hms(2024, 3, 4, 9, 0, 0).unwrap(),
                Utc.with_ymd_and_hms(2024, 3, 5, 12, 0, 0).unwrap(),
                Utc.with_ymd_and_hms(2024, 3, 5, 12, 0, 0).unwrap(),
            ),
            // A holiday shifts the weekly keys without changing the known duration.
            (
                vec![date(2024, 1, 8), date(2024, 1, 16), date(2024, 1, 22)],
                Some(Timeframe::Week1),
                "UTC",
                date(2024, 1, 1),
                date(2024, 1, 29),
                date(2024, 1, 29),
            ),
        ] {
            let last = times.len() - 1;
            let right_index = (last + 1) as f32;
            let mut chart = Chart::new(BarData::from_bars(times.into_iter().map(bar).collect()));
            if let Some(timeframe) = timeframe {
                chart.set_timeframe(timeframe);
            }
            chart.chart_options.time_scale.timezone =
                crate::scales::TimezoneMode::Timezone(timezone.parse().unwrap());
            let mut drawing = Drawing::new(7, DrawingToolType::TrendLine);
            drawing.chart_points = vec![
                ChartPoint::new(-1.0, 10.0),
                ChartPoint::new(right_index, 11.0),
            ];
            drawing.completed = true;
            chart.drawings_mut().drawings.push(drawing);
            for (index, expected) in [(right_index, expected_right), (-1.0, expected_left)] {
                assert_eq!(
                    chart.drawing_point_time(&ChartPoint::new(index, 10.0)),
                    Some(expected),
                    "{timeframe:?}"
                );
                assert_eq!(
                    chart.drawing_point_at(expected, 10.0).unwrap().bar_idx,
                    index
                );
                assert_eq!(chart.index_to_time(index as f64), Some(expected));
            }
            for (index, left, right) in [
                (-0.5, expected_left, chart.state.shown()[0].time),
                (
                    last as f64 + 0.5,
                    chart.state.shown()[last].time,
                    expected_right,
                ),
            ] {
                let midpoint = left + (right - left) / 2;
                assert_eq!(chart.index_to_time(index), Some(midpoint));
                assert_eq!(chart.time_to_index(midpoint), Some(index));
            }
            // The label keys must agree even if only the final bar is visible.
            let ctx = egui::Context::default();
            let painter =
                egui::Painter::new(ctx, egui::LayerId::background(), egui::Rect::EVERYTHING);
            let rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(1000.0, 300.0));
            let price = crate::scales::PriceMap::new(
                crate::scales::PriceTransform::default(),
                crate::scales::PriceRange::new(9.0, 12.0),
                0.0,
                300.0,
                false,
            );
            for start in [0, last] {
                let coords = crate::chart::renderers::ChartMapping::new(
                    rect, 100.0, start, last, 5.0, price,
                );
                let (_, keys) = crate::chart::renderers::time_axis_marks(
                    &crate::chart::renderers::RenderContext::new(&painter, rect),
                    &chart.state.shown()[start..],
                    &coords,
                    &chart.build_time_formatter(),
                    &chart.chart_options.time_scale,
                    |index| chart.index_to_time(index),
                );
                assert_eq!(
                    keys.iter()
                        .find(|(_, index)| *index == right_index as usize)
                        .map(|(time, _)| *time),
                    Some(expected_right)
                );
            }
            let saved = chart.save_drawings().unwrap().to_json().unwrap();
            chart.load_drawings(&saved).unwrap();
            chart.update(SeriesId::MAIN, bar(next));
            assert_eq!(
                chart.drawings().drawings[0].chart_points[1].bar_idx,
                right_index
            );
            let mut extended = vec![bar(expected_left)];
            extended.extend_from_slice(&chart.state.data().bars);
            chart.set_data(SeriesId::MAIN, extended);
            assert_eq!(chart.drawings().drawings[0].chart_points[0].bar_idx, 0.0);
            assert_eq!(
                chart.drawings().drawings[0].chart_points[1].bar_idx,
                right_index + 1.0
            );
        }
    }

    /// Millisecond JSON fields must not collapse execution bars one nanosecond apart.
    #[test]
    fn drawing_storage_preserves_submillisecond_anchors() {
        for start in [
            Utc.timestamp_opt(1_704_067_200, 0).unwrap(),
            Utc.timestamp_opt(-1, 999_999_999).unwrap(),
        ] {
            let bars = crate::data::aggregate_trades(
                &(0..3)
                    .map(|id| crate::model::Tick::new(start, 10.0, 1.0, None).with_id(id))
                    .collect::<Vec<_>>(),
                "1t".parse::<crate::model::Timeframe>().unwrap(),
            )
            .unwrap();
            let mut chart = Chart::new(BarData::from_bars(bars.clone()));
            let mut drawing = Drawing::new(7, DrawingToolType::TrendLine);
            drawing.chart_points = vec![ChartPoint::new(1.0, 10.0), ChartPoint::new(2.0, 11.0)];
            drawing.completed = true;
            chart.drawings_mut().drawings.push(drawing);
            let saved = chart.save_drawings().unwrap().to_json().unwrap();
            chart.load_drawings(&saved).unwrap();
            assert_eq!(
                chart.drawings().drawings[0]
                    .chart_points
                    .iter()
                    .map(|p| p.bar_idx)
                    .collect::<Vec<_>>(),
                [1.0, 2.0]
            );
            assert_eq!(
                chart.drawing_point_time(&chart.drawings().drawings[0].chart_points[1]),
                Some(start + chrono::Duration::nanoseconds(2))
            );
            let id = chart.drawings().drawings[0].id;
            chart.drawings_mut().select(id);
            let copied = chart.copy_selected_drawings().unwrap().unwrap();
            chart.paste_drawings(&copied).unwrap();
            assert_eq!(
                chart
                    .drawings()
                    .drawings
                    .last()
                    .unwrap()
                    .chart_points
                    .iter()
                    .map(|point| point.bar_idx)
                    .collect::<Vec<_>>(),
                [4.0, 5.0]
            );
            assert!(chart.undo());
            // Retained anchors also survive a save while history is absent.
            chart.set_data(SeriesId::MAIN, Vec::new());
            let saved = chart.save_drawings().unwrap().to_json().unwrap();
            chart.load_drawings(&saved).unwrap();
            chart.set_data(SeriesId::MAIN, bars);
            assert_eq!(
                chart.drawings().drawings[0]
                    .chart_points
                    .iter()
                    .map(|p| p.bar_idx)
                    .collect::<Vec<_>>(),
                [1.0, 2.0]
            );
        }
    }

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
        // -0.5 * 60 = -30 s; 60 + 0.5 * 120 = 120 s; 180 + 1.5 * 60 = 270 s.
        assert_eq!(
            saved.drawings[0]
                .points
                .iter()
                .map(|p| p.ts_ms)
                .collect::<Vec<_>>(),
            [-30_000, 120_000, 270_000]
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
        assert!(matches!(
            restored.load_drawings(&json.to_string()),
            Err(LoadError::OtherSymbol)
        ));
        assert_eq!(restored.drawings().drawings.len(), 1);
        let mut empty = Chart::new(BarData::new());
        empty.drawings_mut().drawings = chart.drawings().drawings.clone();
        assert!(empty.save_drawings().is_err());
    }

    /// A paste refuses other text, drawings of another symbol, drawings that could not all be
    /// read and a chart without bars to place them, each with its own reason, before changing
    /// any drawing.
    #[test]
    fn a_refused_paste_reports_why() {
        use crate::drawings::persistence::{LoadError, SkipReason};
        use crate::widget::test_frames;
        let mut source = test_frames::chart(&[100.0], &[110.0]);
        source.set_symbol("A");
        let mut line = Drawing::new(0, DrawingToolType::TrendLine);
        line.chart_points = vec![ChartPoint::new(1.0, 105.0), ChartPoint::new(2.0, 106.0)];
        let id = source.drawings_mut().add_drawing(line).unwrap();
        source.drawings_mut().select(id);
        let copied = source.copy_selected_drawings().unwrap().unwrap();
        let newer = copied.replace("\"TrendLine\"", "\"NewerTool\"");
        type Refused = fn(&LoadError) -> bool;
        let cases: [(&str, &str, bool, Refused); 4] = [
            ("hello", "A", true, |e| matches!(e, LoadError::NotClipboard)),
            (&copied, "B", true, |e| matches!(e, LoadError::OtherSymbol)),
            (&newer, "A", true, |e| {
                matches!(e, LoadError::Skipped(s) if s.len() == 1
                    && matches!(&s[0].error, SkipReason::UnknownTool(t) if t.input == "NewerTool"))
            }),
            (&copied, "A", false, |e| {
                matches!(e, LoadError::NeedsHistory)
            }),
        ];
        for (text, symbol, bars, refused) in cases {
            let mut target = if bars {
                test_frames::chart(&[100.0], &[110.0])
            } else {
                Chart::new(BarData::new())
            };
            target.set_symbol(symbol);
            let error = target.paste_drawings(text).unwrap_err();
            assert!(refused(&error), "{symbol}: {error:?}");
            assert!(target.drawings().drawings.is_empty(), "{error}");
        }
    }
}
