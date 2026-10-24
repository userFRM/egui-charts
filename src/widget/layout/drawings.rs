//! Propagate drawing changes without consuming the host's persistence queue.

use crate::drawings::{ChartPoint, Drawing};
use crate::widget::{
    Chart, SeriesId,
    sync::{index_at, time_at},
};
use std::collections::{HashMap, HashSet};

type Key = (usize, usize);

#[derive(Clone)]
struct Snapshot {
    style: String,
    points: Vec<ChartPoint>,
    notes: Option<String>,
    symbol: String,
}

#[derive(Clone, Default)]
pub(super) struct DrawingLinks {
    previous: HashMap<Key, Snapshot>,
    origins: HashMap<Key, Key>,
    retry: HashSet<Key>,
}

impl DrawingLinks {
    pub(super) fn sync(
        &mut self,
        charts: &mut [&mut Chart],
        count: usize,
        groups: &[Option<String>; 16],
        pointer_down: bool,
    ) -> bool {
        let mut changed = false;
        let mut changes = Vec::new();
        let mut current = HashSet::new();
        for (slot, chart) in charts.iter().take(count).enumerate() {
            for drawing in &chart.drawings().drawings {
                let Ok(style) = serde_json::to_string(drawing) else {
                    continue;
                };
                let key = (slot, drawing.id);
                let old = self.previous.get(&key);
                let retry = self.retry.remove(&key);
                if (old.is_none_or(|old| {
                    old.style != style
                        || old.points != drawing.chart_points
                        || old.notes != drawing.notes
                }) || retry)
                    && old.is_none_or(|old| old.symbol == chart.symbol())
                {
                    changes.push((key, Some(drawing.clone())));
                }
                current.insert(key);
            }
        }
        for (&key, old) in &self.previous {
            if key.0 < count && !current.contains(&key) && old.symbol == charts[key.0].symbol() {
                changes.push((key, None));
            }
        }
        for (key, drawing) in changes {
            let origin = *self.origins.entry(key).or_insert(key);
            let symbol = charts[key.0].symbol().to_owned();
            for slot in 0..count {
                if slot == key.0 || groups[slot] != groups[key.0] || charts[slot].symbol() != symbol
                {
                    continue;
                }
                let linked = self
                    .origins
                    .iter()
                    .find_map(|(&(target_slot, id), &value)| {
                        (target_slot == slot
                            && value == origin
                            && charts[slot].drawings().drawings.iter().any(|d| d.id == id))
                        .then_some(id)
                    });
                if let Some(drawing) = &drawing {
                    let Some(mut copy) = map_drawing(drawing, charts[key.0], charts[slot]) else {
                        self.retry.insert(key);
                        continue;
                    };
                    changed = true;
                    if let Some(id) = linked {
                        charts[slot].drawings_mut().drag_edit_drawing(id, |target| {
                            copy.id = target.id;
                            copy.stored_id.clone_from(&target.stored_id);
                            copy.created_at = target.created_at;
                            *target = copy;
                        });
                    } else {
                        copy.stored_id = None;
                        copy.created_at = None;
                        charts[slot].drawings_mut().add_synced_drawing(copy);
                        if let Some(added) = charts[slot].drawings().drawings.last() {
                            self.origins.insert((slot, added.id), origin);
                        }
                    }
                } else if let Some(id) = linked {
                    changed = true;
                    charts[slot].drawings_mut().delete_drawing(id);
                }
            }
        }
        if !pointer_down {
            for chart in charts.iter_mut().take(count) {
                chart.drawings_mut().end_drag_handle();
            }
        }
        // Cache the propagated state too, so receiver updates cannot echo back next frame.
        self.previous.clear();
        for (slot, chart) in charts.iter().take(count).enumerate() {
            for drawing in &chart.drawings().drawings {
                if let Ok(style) = serde_json::to_string(drawing) {
                    self.previous.insert(
                        (slot, drawing.id),
                        Snapshot {
                            style,
                            points: drawing.chart_points.clone(),
                            notes: drawing.notes.clone(),
                            symbol: chart.symbol().to_owned(),
                        },
                    );
                }
            }
        }
        self.origins
            .retain(|key, _| self.previous.contains_key(key));
        self.retry.retain(|key| self.previous.contains_key(key));
        changed
    }
}

fn map_drawing(drawing: &Drawing, source: &Chart, target: &Chart) -> Option<Drawing> {
    let mut copy = drawing.clone();
    copy.chart_points = drawing
        .chart_points
        .iter()
        .map(|point| {
            let time = time_at(source.bars(SeriesId::MAIN), point.bar_idx as f64)?;
            let index = index_at(target.bars(SeriesId::MAIN), time)?;
            Some(ChartPoint::new(index as f32, point.price))
        })
        .collect::<Option<_>>()?;
    copy.points.clear();
    Some(copy)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::DrawingToolType;
    use crate::model::{Bar, BarData};
    use chrono::DateTime;

    #[test]
    fn drawing_changes_map_time_preserve_local_work_and_never_echo() {
        let chart = |step: i64| {
            let mut chart = Chart::new(BarData::from_bars(
                (0..10)
                    .map(|i| {
                        Bar::new(
                            DateTime::from_timestamp(i * step, 0).unwrap(),
                            10.0,
                            11.0,
                            9.0,
                            10.0,
                            1.0,
                        )
                    })
                    .collect(),
            ));
            chart.set_symbol("SAMPLE");
            chart
        };
        let mut charts = [chart(60), chart(120), chart(60), chart(60)];
        charts[3].set_symbol("OTHER");
        let mut groups = std::array::from_fn(|_| None);
        groups[2] = Some("isolated".into());
        let mut links = DrawingLinks::default();
        let mut local = Drawing::new(0, DrawingToolType::HorizontalLine);
        local.chart_points.push(ChartPoint::new(0.0, 42.0));
        local.completed = true;
        charts[1].drawings_mut().add_synced_drawing(local);
        let mut drawing = Drawing::new(0, DrawingToolType::TrendLine);
        drawing.chart_points = vec![ChartPoint::new(2.0, 10.0), ChartPoint::new(4.0, 12.0)];
        drawing.completed = true;
        charts[0].drawings_mut().add_synced_drawing(drawing);
        let mut refs: Vec<_> = charts.iter_mut().collect();
        links.sync(&mut refs, 4, &groups, false);
        assert_eq!(refs[1].drawings().drawings.len(), 2);
        assert!(refs[2].drawings().drawings.is_empty());
        assert!(refs[3].drawings().drawings.is_empty());
        let copied = &refs[1].drawings().drawings[1];
        let id = copied.id;
        assert_eq!(
            copied.chart_points,
            vec![ChartPoint::new(1.0, 10.0), ChartPoint::new(2.0, 12.0)]
        );
        let queued = refs[1].drawings().pending_sync.lock().unwrap().len();
        links.sync(&mut refs, 4, &groups, false);
        assert_eq!(
            refs[1].drawings().pending_sync.lock().unwrap().len(),
            queued
        );
        refs[1]
            .drawings_mut()
            .edit_drawing(id, |d| d.chart_points[0].bar_idx = 1.5);
        links.sync(&mut refs, 4, &groups, false);
        assert_eq!(refs[0].drawings().drawings[0].chart_points[0].bar_idx, 3.0);
        refs[1]
            .drawings_mut()
            .edit_drawing(id, |d| d.notes = Some("shared note".into()));
        links.sync(&mut refs, 4, &groups, false);
        assert_eq!(
            refs[0].drawings().drawings[0].notes.as_deref(),
            Some("shared note")
        );
        let queued = refs[1].drawings().pending_sync.lock().unwrap().len();
        let source_id = refs[0]
            .drawings()
            .drawings
            .iter()
            .find(|d| d.tool_type == DrawingToolType::TrendLine)
            .unwrap()
            .id;
        for index in [4.0, 5.0] {
            refs[0]
                .drawings_mut()
                .edit_drawing(source_id, |d| d.chart_points[0].bar_idx = index);
            links.sync(&mut refs, 4, &groups, true);
        }
        assert_eq!(
            refs[1].drawings().pending_sync.lock().unwrap().len(),
            queued,
            "a mirrored drag must not queue an update for every frame"
        );
        links.sync(&mut refs, 4, &groups, false);
        assert_eq!(
            refs[1].drawings().pending_sync.lock().unwrap().len(),
            queued + 1
        );
        assert!(refs[1].drawings_mut().undo());
        assert_eq!(
            refs[1]
                .drawings()
                .drawings
                .iter()
                .find(|d| d.id == id)
                .unwrap()
                .chart_points[0]
                .bar_idx,
            1.5
        );
        links.sync(&mut refs, 4, &groups, false);
        refs[1].drawings_mut().delete_drawing(id);
        links.sync(&mut refs, 4, &groups, false);
        assert!(
            refs[0]
                .drawings()
                .drawings
                .iter()
                .all(|d| d.tool_type != DrawingToolType::TrendLine)
        );
        assert_eq!(refs[1].drawings().drawings[0].chart_points[0].price, 42.0);
        assert!(refs[1].drawings_mut().undo());
        links.sync(&mut refs, 4, &groups, false);
        assert_eq!(
            refs[0]
                .drawings()
                .drawings
                .iter()
                .filter(|d| d.tool_type == DrawingToolType::TrendLine)
                .count(),
            1
        );
        // A receiver may still be loading its history when a drawing is first shared.
        let mut source = chart(60);
        let mut target = Chart::new(BarData::new());
        target.set_symbol("SAMPLE");
        source
            .drawings_mut()
            .add_synced_drawing(refs[0].drawings().drawings[0].clone());
        let mut pending = DrawingLinks::default();
        pending.sync(&mut [&mut source, &mut target], 2, &groups, false);
        assert!(target.drawings().drawings.is_empty());
        target.set_data(SeriesId::MAIN, source.bars(SeriesId::MAIN).to_vec());
        pending.sync(&mut [&mut source, &mut target], 2, &groups, false);
        assert_eq!(target.drawings().drawings.len(), 1);
    }
}
