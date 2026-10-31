//! Propagate drawing changes without consuming the host's persistence queue.

use crate::drawings::manager::sync_id;
use crate::drawings::{ChartPoint, Drawing};
use crate::widget::{Chart, PaneId};
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
        let joined = self.join_read_drawings(charts, count, groups, &changes);
        for (key, drawing) in changes {
            if joined.contains(&key) {
                continue;
            }
            let origin = *self.origins.entry(key).or_insert(key);
            let symbol = charts[key.0].symbol().to_owned();
            for slot in 0..count {
                // Charts on one storage read each other's stored drawings instead.
                let stored = drawing.as_ref().is_some_and(|drawing| drawing.saveable);
                if slot == key.0
                    || groups[slot] != groups[key.0]
                    || charts[slot].symbol() != symbol
                    || (stored && charts[slot].shares_stored_drawings(charts[key.0]))
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
                        unlock(charts[slot], id);
                        charts[slot].drawings_mut().drag_edit_drawing(id, |target| {
                            copy.id = target.id;
                            copy.stored_id.clone_from(&target.stored_id);
                            copy.created_at = target.created_at;
                            copy.saveable = target.saveable;
                            *target = copy;
                        });
                    } else {
                        copy.stored_id = None;
                        copy.created_at = None;
                        // Not saved where it is mirrored: the chart it was drawn on keeps it, and
                        // a drawing that is not saveable stays when this chart reads its storage.
                        copy.saveable &= !charts[slot].keeps_stored_drawings();
                        charts[slot].drawings_mut().add_synced_drawing(copy);
                        if let Some(added) = charts[slot].drawings().drawings.last() {
                            self.origins.insert((slot, added.id), origin);
                        }
                    }
                } else if let Some(id) = linked {
                    changed = true;
                    unlock(charts[slot], id);
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

    /// Links each drawing that a chart has just read from storage to the drawing stored under the
    /// same id on another chart of that storage, group and symbol, once that one is linked, and
    /// returns their keys: the read drawing is that drawing, whose copies exist already. A read
    /// drawing without such a drawing starts a link, which the same drawing read on another
    /// chart joins.
    fn join_read_drawings(
        &mut self,
        charts: &[&mut Chart],
        count: usize,
        groups: &[Option<String>; 16],
        changes: &[(Key, Option<Drawing>)],
    ) -> HashSet<Key> {
        let mut joined = HashSet::new();
        for (key, drawing) in changes {
            let Some(drawing) = drawing else { continue };
            if !drawing.saveable || drawing.stored_id.is_none() || self.origins.contains_key(key) {
                continue;
            }
            let record = sync_id(drawing);
            let origin = (0..count)
                .filter(|&slot| {
                    slot != key.0
                        && groups[slot] == groups[key.0]
                        && charts[slot].symbol() == charts[key.0].symbol()
                        && charts[slot].shares_stored_drawings(charts[key.0])
                })
                .find_map(|slot| {
                    let other = charts[slot]
                        .drawings()
                        .drawings
                        .iter()
                        .find(|d| d.saveable && sync_id(d) == record)?;
                    self.origins.get(&(slot, other.id)).copied()
                });
            if origin.is_some() {
                joined.insert(*key);
            }
            self.origins.insert(*key, origin.unwrap_or(*key));
        }
        joined
    }
}

/// A linked copy follows its source even where a local lock holds it, which the source's own
/// lock state restores with the copy. Direct edits keep meeting the lock.
fn unlock(chart: &mut Chart, id: usize) {
    if chart
        .drawings()
        .drawings
        .iter()
        .any(|d| d.id == id && d.locked)
    {
        chart.drawings_mut().edit_drawing(id, |d| d.locked = false);
    }
}

fn map_drawing(drawing: &Drawing, source: &Chart, target: &Chart) -> Option<Drawing> {
    let mut copy = drawing.clone();
    if drawing.pane.is_some() {
        // Study ids are local to each chart, so the copy goes to the corresponding pane.
        let pane = source
            .drawing_pane(drawing.id)
            .filter(|pane| *pane != PaneId::MAIN)?;
        let pane = source.linked_pane(pane, target)?;
        copy.pane = target.pane_series(pane).first().map(|id| id.0);
    }
    copy.chart_points = drawing
        .chart_points
        .iter()
        .map(|point| {
            let time = source.drawing_point_time(point)?;
            target.drawing_point_at(time, point.price)
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
    use crate::widget::SeriesId;
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
        refs[1].drawings().pending_sync.lock().unwrap().clear();
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
        // Element coordinates and retained timestamps must not be read as source bar indices.
        source.state.set_elements(
            Some(source.bars(SeriesId::MAIN)[..4].to_vec().into()),
            false,
        );
        let mut drawing = Drawing::new(0, DrawingToolType::TrendLine);
        drawing.chart_points = vec![ChartPoint::new(1.5, 10.0), ChartPoint::new(3.5, 12.0)];
        source.drawings_mut().add_synced_drawing(drawing);
        source.anchor_drawing_points();
        source.state.set_elements(
            Some(
                source
                    .bars(SeriesId::MAIN)
                    .iter()
                    .step_by(2)
                    .cloned()
                    .collect(),
            ),
            false,
        );
        source.project_drawing_points();
        let drawing = source.drawings().drawings.last().unwrap();
        let mapped = map_drawing(drawing, &source, &target).unwrap();
        assert_eq!(mapped.chart_points[0].bar_idx, 1.5);
        assert_eq!(mapped.chart_points[1].bar_idx, 3.5);
        // A drawing of a study pane without a counterpart is not mirrored.
        let mut owned = drawing.clone();
        owned.pane = Some(7);
        source.drawings_mut().add_synced_drawing(owned);
        let mut local = DrawingLinks::default();
        local.sync(&mut [&mut source, &mut target], 2, &groups, false);
        assert!(target.drawings().drawings.iter().all(|d| d.pane.is_none()));
    }

    /// Drawings in corresponding study panes follow each other by timestamp, and a linked edit,
    /// unlock or deletion reaches a locked copy while a direct edit still meets its lock.
    #[cfg(feature = "studies")]
    #[test]
    fn study_pane_drawings_link_by_time_and_pass_locks() {
        use crate::studies::{MACD, RSI};
        let chart = |step: i64, rsi_first: bool| {
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
            if !rsi_first {
                chart.add_indicator(Box::new(MACD::new(12, 26, 9)));
            }
            let rsi = chart.add_indicator(Box::new(RSI::new(14)));
            if rsi_first {
                chart.add_indicator(Box::new(MACD::new(12, 26, 9)));
            }
            (chart, rsi)
        };
        let ((mut a, rsi_a), (mut b, rsi_b)) = (chart(60, true), chart(120, false));
        assert_eq!(
            a.linked_pane(a.series_pane(rsi_a).unwrap(), &b),
            b.series_pane(rsi_b)
        );
        let groups = std::array::from_fn(|_| None);
        let mut links = DrawingLinks::default();
        let mut drawing = Drawing::new(0, DrawingToolType::TrendLine);
        drawing.pane = Some(rsi_a.0);
        drawing.chart_points = vec![ChartPoint::new(2.0, 40.0), ChartPoint::new(4.0, 60.0)];
        drawing.completed = true;
        a.drawings_mut().add_synced_drawing(drawing);
        let source = a.drawings().drawings[0].id;
        let mut sync = |a: &mut Chart, b: &mut Chart| links.sync(&mut [a, b], 2, &groups, false);
        sync(&mut a, &mut b);
        let copy = &b.drawings().drawings[0];
        let id = copy.id;
        assert_eq!(copy.pane, Some(rsi_b.0));
        // Bars two and four are the minutes 2 and 4, which are the bars one and two at 2 minute spacing.
        assert_eq!(
            copy.chart_points,
            vec![ChartPoint::new(1.0, 40.0), ChartPoint::new(2.0, 60.0)]
        );
        let copy = |b: &Chart| b.drawings().drawings.iter().find(|d| d.id == id).cloned();
        a.drawings_mut().edit_drawing(source, |d| d.locked = true);
        sync(&mut a, &mut b);
        assert!(copy(&b).unwrap().locked);
        a.drawings_mut().edit_drawing(source, |d| {
            d.locked = false;
            d.chart_points[0].bar_idx = 3.0;
        });
        sync(&mut a, &mut b);
        let moved = copy(&b).unwrap();
        assert!(!moved.locked, "an unlock reaches a locked copy");
        assert_eq!(moved.chart_points[0].bar_idx, 1.5);
        a.drawings_mut().edit_drawing(source, |d| d.locked = true);
        sync(&mut a, &mut b);
        b.drawings_mut()
            .edit_drawing(id, |d| d.chart_points[0].bar_idx = 0.0);
        assert_eq!(copy(&b).unwrap().chart_points[0].bar_idx, 1.5);
        a.drawings_mut().edit_drawing(source, |d| d.locked = false);
        a.drawings_mut().delete_drawing(source);
        sync(&mut a, &mut b);
        assert!(copy(&b).is_none(), "a deletion reaches a locked copy");
    }
}
