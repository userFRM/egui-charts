//! Validated host creation and editing of timestamp-based drawings.

use crate::Chart;
use crate::drawings::{ChartPoint, Drawing};
use chrono::{DateTime, Utc};

impl Chart {
    /// Creates a completed drawing from UTC times and prices, with the supplied drawing's
    /// properties, pane, group and layer. Its id and storage identity are replaced.
    /// Returns an error without changing history or drawings for invalid anchors or settings.
    /// Times can precede loaded history; their projection is updated when bars arrive.
    /// Wrap this call in [`Self::user_action`] to include it in chart undo history.
    ///
    /// ```
    /// use egui_charts::{Chart, drawings::{Drawing, DrawingToolType}};
    /// let mut chart = Chart::new(egui_charts::model::BarData::default());
    /// let time = chrono::DateTime::from_timestamp(1_700_000_000, 0).unwrap();
    /// let mut line = Drawing::new(0, DrawingToolType::HorizontalLine);
    /// line.locked = true;
    /// let id = chart.create_drawing(line, &[(time, 100.0)]).unwrap();
    /// assert_eq!(chart.drawing_points(id).unwrap(), vec![(time, 100.0)]);
    /// ```
    pub fn create_drawing(
        &mut self,
        mut drawing: Drawing,
        points: &[(DateTime<Utc>, f64)],
    ) -> Result<usize, &'static str> {
        drawing.chart_points = self.map_drawing_points(points)?;
        self.drawings_mut().add_drawing(drawing)
    }

    /// Returns a drawing's UTC anchors and prices, or `None` for an unknown id or an index
    /// without a time mapping. Retains exact supplied times between bars and in whitespace.
    pub fn drawing_points(&self, id: usize) -> Option<Vec<(DateTime<Utc>, f64)>> {
        self.drawings()
            .drawings
            .iter()
            .find(|d| d.id == id)?
            .chart_points
            .iter()
            .map(|p| Some((self.drawing_point_time(p)?, p.price)))
            .collect()
    }

    /// Replaces a drawing's anchors as one undoable edit. Invalid points and locked drawings
    /// leave the original untouched. Screen positions are rebuilt on the next frame.
    pub fn set_drawing_points(
        &mut self,
        id: usize,
        points: &[(DateTime<Utc>, f64)],
    ) -> Result<(), &'static str> {
        let mut drawing = self
            .drawings()
            .drawings
            .iter()
            .find(|d| d.id == id)
            .ok_or("Unknown drawing")?
            .clone();
        if drawing.locked {
            return Err("Drawing is locked");
        }
        drawing.chart_points = self.map_drawing_points(points)?;
        self.drawings().validate_drawing(&drawing)?;
        self.drawings_mut().edit_drawing(id, |target| {
            target.chart_points = drawing.chart_points;
        });
        Ok(())
    }

    /// Reveals an offscreen drawing by panning, retaining zoom and the auto/manual scale state.
    pub(crate) fn reveal_drawing(&mut self, id: usize) {
        let Some(drawing) = self.drawings.drawings.iter().find(|d| d.id == id) else {
            return;
        };
        if !drawing.selectable || drawing.screen_anchor.is_some() || self.state.shown().is_empty() {
            return;
        }
        let pane = self.pane_layout.panes.iter().position(|pane| {
            drawing.pane == Some(pane.id.0)
                || pane.members.iter().any(|id| drawing.pane == Some(id.0))
        });
        let price = match pane {
            Some(at) => &self.pane_layout.panes[at].scale,
            None => self.state.price_scale(),
        };
        let transform = price.transform();
        let bounds = drawing
            .chart_points
            .iter()
            .filter_map(|point| {
                let price = transform.to_scale(point.price);
                (point.bar_idx.is_finite() && price.is_finite()).then_some((point.bar_idx, price))
            })
            .fold(None, |bounds, (bar, price)| {
                Some(match bounds {
                    None => (bar, bar, price, price),
                    Some((left, right, low, high)) => (
                        bar.min(left),
                        bar.max(right),
                        price.min(low),
                        price.max(high),
                    ),
                })
            });
        let Some((left, right, low, high)) = bounds else {
            return;
        };
        let range = self.state.time_scale().visible_logical_range();
        let padding = ((range.right - range.left) * 0.05).clamp(0.0, 2.0);
        let shift = if right < range.left {
            right - range.left - padding
        } else if left > range.right {
            left - range.right + padding
        } else {
            0.0
        };
        if shift != 0.0 {
            self.cancel_view_motion();
            self.tracking_mode_active = false;
            let scale = self.state.time_scale_mut();
            scale.set_right_offset(scale.right_offset_precise() + f64::from(shift));
        }
        let scale = match pane {
            Some(at) => &mut self.pane_layout.panes[at].scale,
            None => self.state.price_scale_mut(),
        };
        if scale.is_manual() {
            let range = scale.range();
            let padding = (range.max - range.min) * 0.05;
            let shift = if high < range.min {
                high - range.min - padding
            } else if low > range.max {
                low - range.max + padding
            } else {
                0.0
            };
            if shift != 0.0 && (range.min + shift).is_finite() && (range.max + shift).is_finite() {
                scale.set_range(range.min + shift, range.max + shift);
            }
        }
    }

    fn map_drawing_points(
        &self,
        points: &[(DateTime<Utc>, f64)],
    ) -> Result<Vec<ChartPoint>, &'static str> {
        points
            .iter()
            .map(|&(time, price)| {
                if !price.is_finite() {
                    return Err("Drawing prices must be finite");
                }
                Ok(self.drawing_point_at(time, price).unwrap_or_else(|| {
                    let mut point = ChartPoint::new(0.0, price);
                    point.time_anchor = Some((time, 0.0));
                    point
                }))
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::DrawingToolType;
    use crate::drawings::manager::DrawingSyncAction;
    use crate::model::{Bar, BarData};

    #[test]
    fn timestamp_creation_validates_before_mutation_and_round_trips_edits() {
        let time = |seconds| DateTime::from_timestamp(seconds, 0).unwrap();
        let bars = [0, 60, 180]
            .map(|seconds| Bar::new(time(seconds), 10., 12., 8., 10., 1.))
            .to_vec();
        let mut chart = Chart::new(BarData::from_bars(bars));
        let points = [(time(-30), 9.), (time(120), 11.)];
        let id = chart
            .user_action(|chart| {
                chart.create_drawing(Drawing::new(99, DrawingToolType::TrendLine), &points)
            })
            .unwrap();
        let drawing = &chart.drawings().drawings[0];
        // -30/60 = -0.5; 1 + (120-60)/(180-60) = 1.5.
        assert_eq!(
            drawing
                .chart_points
                .iter()
                .map(|p| p.bar_idx)
                .collect::<Vec<_>>(),
            [-0.5, 1.5]
        );
        assert_eq!(chart.drawing_points(id).unwrap(), points);
        assert!(chart.undo());
        assert!(chart.drawings().drawings.is_empty());
        assert!(chart.redo());
        chart.drawings().pending_sync.lock().unwrap().clear();
        for invalid in [
            vec![],
            vec![points[0]],
            vec![points[0]; 3],
            vec![(time(0), f64::NAN); 2],
        ] {
            assert!(
                chart
                    .create_drawing(Drawing::new(0, DrawingToolType::TrendLine), &invalid)
                    .is_err()
            );
            assert!(chart.set_drawing_points(id, &invalid).is_err());
            assert_eq!(chart.drawing_points(id).unwrap(), points);
        }
        assert!(chart.drawings().pending_sync.lock().unwrap().is_empty());
        let edited = [(time(30), 10.), (time(360), 12.)];
        chart.set_drawing_points(id, &edited).unwrap();
        let json = chart.save_drawings().unwrap().to_json().unwrap();
        let mut empty = Chart::new(crate::model::BarData::default());
        empty.load_drawings(&json).unwrap();
        let restored_id = empty.drawings().drawings[0].id;
        assert_eq!(empty.drawing_points(restored_id).unwrap(), edited);
        chart.drawings_mut().edit_drawing(id, |d| {
            d.saveable = false;
            d.listed = false;
        });
        assert!(chart.save_drawings().unwrap().drawings().is_empty());
        assert!(chart.drawings().listed_drawings().next().is_none());
        assert!(matches!(
            chart.drawings().pending_sync.lock().unwrap().last(),
            Some(DrawingSyncAction::Delete(_))
        ));
        assert!(chart.drawings_mut().undo());
        assert_eq!(chart.save_drawings().unwrap().drawings().len(), 1);
        assert!(matches!(
            chart.drawings().pending_sync.lock().unwrap().last(),
            Some(DrawingSyncAction::SaveDrawing(_))
        ));
        let mut volatile = Drawing::new(0, DrawingToolType::HorizontalLine);
        volatile.saveable = false;
        chart.drawings().pending_sync.lock().unwrap().clear();
        let volatile = chart.create_drawing(volatile, &points[..1]).unwrap();
        chart.drawings_mut().delete_drawing(volatile);
        assert!(chart.drawings().pending_sync.lock().unwrap().is_empty());
    }
}
