//! Propagate drawing changes without consuming the host's persistence queue, and list and delete
//! the layout's drawings by symbol.

use super::ChartLayout;
use crate::config::{DrawingSort, DrawingToolType};
use crate::drawings::DrawingManager;
use crate::drawings::manager::sync_id;
use crate::drawings::persistence::{DrawingStorage, StoredCoord, StoredDrawing};
use crate::drawings::{Drawing, FolderSync};
use crate::widget::{Chart, PaneId};
use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, HashSet};

type Key = (usize, usize);

#[derive(Clone)]
struct Snapshot {
    style: String,
    /// See [`timed_points`].
    points: Vec<(Option<chrono::DateTime<chrono::Utc>>, f64)>,
    notes: Option<String>,
    symbol: String,
    sync: Option<FolderSync>,
}

/// Whether a drawing with this folder synchronization is mirrored to the other charts.
fn mirrored(sync: Option<FolderSync>, layout_wide: bool) -> bool {
    match sync {
        Some(FolderSync::Chart) => false,
        Some(FolderSync::Layout) => true,
        None => layout_wide,
    }
}

/// The time and price of each point, which is what a linked copy follows: bars evicted or loaded
/// in front renumber the points but keep their times.
fn timed_points(
    chart: &Chart,
    drawing: &Drawing,
) -> Vec<(Option<chrono::DateTime<chrono::Utc>>, f64)> {
    drawing
        .chart_points
        .iter()
        .map(|point| (chart.drawing_point_time(point), point.price))
        .collect()
}

#[derive(Clone, Default)]
pub(super) struct DrawingLinks {
    previous: HashMap<Key, Snapshot>,
    origins: HashMap<Key, Key>,
    retry: HashSet<Key>,
}

impl DrawingLinks {
    pub(super) fn swap_slots(&mut self, a: usize, b: usize) {
        let moved = |(slot, id): Key| {
            let slot = match slot {
                slot if slot == a => b,
                slot if slot == b => a,
                slot => slot,
            };
            (slot, id)
        };
        self.previous = self.previous.drain().map(|(k, v)| (moved(k), v)).collect();
        self.origins = self
            .origins
            .drain()
            .map(|(k, v)| (moved(k), moved(v)))
            .collect();
        self.retry = self.retry.drain().map(moved).collect();
    }

    pub(super) fn sync(
        &mut self,
        charts: &mut [&mut Chart],
        count: usize,
        groups: &[Option<String>; 16],
        pointer_down: bool,
        layout_wide: bool,
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
                let points = timed_points(chart, drawing);
                if (old.is_none_or(|old| {
                    old.style != style || old.points != points || old.notes != drawing.notes
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
            // A drawing that stops being mirrored leaves the charts its copies are on.
            let was_mirrored = self
                .previous
                .get(&key)
                .is_some_and(|old| mirrored(old.sync, layout_wide));
            let mirror = drawing
                .as_ref()
                .is_some_and(|d| mirrored(d.folder_sync_mode(), layout_wide));
            if !mirror && !was_mirrored {
                continue;
            }
            let origin = *self.origins.entry(key).or_insert(key);
            let symbol = charts[key.0].symbol().to_owned();
            let mut saveable = false;
            let mut writers: Vec<usize> = Vec::new();
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
                if let Some(drawing) = drawing.as_ref().filter(|_| mirror) {
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
                        if let Ok(added) = charts[slot].drawings_mut().add_synced_drawing(copy) {
                            self.origins.insert((slot, added), origin);
                        }
                    }
                } else if let Some(id) = linked {
                    changed = true;
                    unlock(charts[slot], id);
                    // Charts on one storage read the change, so one write per storage is enough:
                    // a second would replace the first, or be refused as a conflict where storage
                    // keeps revisions.
                    if !charts[slot].shares_stored_drawings(charts[key.0])
                        && !writers
                            .iter()
                            .any(|&other| charts[slot].shares_stored_drawings(charts[other]))
                        && charts[slot]
                            .drawings()
                            .drawings
                            .iter()
                            .any(|d| d.id == id && d.saveable)
                    {
                        saveable = true;
                        writers.push(slot);
                        charts[slot].history.drawing_edits += 1;
                    }
                    charts[slot].drawings_mut().delete_drawing(id);
                }
            }
            // The copy that stays is now the only instance, so it is stored where it is.
            if saveable
                && let Some(kept) = charts[key.0]
                    .drawings_mut()
                    .drawings
                    .iter_mut()
                    .find(|d| d.id == key.1 && !d.saveable)
            {
                kept.saveable = true;
                charts[key.0].history.drawing_edits += 1;
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
                            points: timed_points(chart, drawing),
                            notes: drawing.notes.clone(),
                            symbol: chart.symbol().to_owned(),
                            sync: drawing.folder_sync_mode(),
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
    /// same id on another chart of that storage, group and symbol, and returns their keys: the
    /// read drawing is that drawing, whose copies exist or are made from it. A read drawing
    /// without such a drawing starts a link, which the same drawing read on another chart joins.
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
                    // Not linked yet if it appeared this frame too: it starts its link below.
                    let other = (slot, other.id);
                    Some(self.origins.get(&other).copied().unwrap_or(other))
                })
                .unwrap_or(*key);
            if origin != *key {
                joined.insert(*key);
            }
            self.origins.insert(*key, origin);
        }
        joined
    }
}

/// A drawing of a symbol in [`ChartLayout::drawing_inventory`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct SymbolDrawing {
    /// The id the drawing is stored under.
    pub id: String,
    /// The drawing's tool.
    pub tool_type: DrawingToolType,
    /// The name given to the drawing, if any.
    pub label: Option<String>,
    /// Time of the last edit in Unix milliseconds, once an edit reached the drawing.
    pub modified_at: Option<i64>,
}

impl SymbolDrawing {
    /// The name given to the drawing, otherwise its tool's.
    pub fn name(&self) -> &str {
        self.label
            .as_deref()
            .filter(|label| !label.trim().is_empty())
            .unwrap_or(self.tool_type.as_str())
    }
}

/// The drawings of one symbol in [`ChartLayout::drawing_inventory`].
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct SymbolDrawings {
    /// The symbol the drawings are saved under.
    pub symbol: String,
    /// Its drawings, in the order the charts hold them.
    pub drawings: Vec<SymbolDrawing>,
}

impl SymbolDrawings {
    /// Orders the drawings by `sort`. Equal names and edit times fall back to the stored
    /// identity of the drawings, so that the order is the same after a layout is restored.
    pub fn sort(&mut self, sort: DrawingSort) {
        self.drawings.sort_by_cached_key(|drawing| {
            let time = match sort {
                DrawingSort::Name => None,
                DrawingSort::LastModified => Some(Reverse(drawing.modified_at)),
            };
            (time, drawing.name().to_lowercase(), drawing.id.clone())
        });
    }
}

/// One chart's deletion of drawings from a symbol it did not show.
pub(in crate::widget) struct UnshownDeletion {
    symbol: String,
    /// Whether the undo history of the symbol's retained drawings holds the deletion.
    retained: bool,
    /// The stored drawings at the other side of this step, when this chart wrote them.
    stored: Option<DrawingStorage>,
}

/// Whether `drawing` is listed in the inventory: what a chart saves and offers to object lists.
fn inventoried(drawing: &Drawing) -> bool {
    drawing.listed && drawing.saveable
}

/// What a drawing and its copies on other charts have in common, whatever each chart keeps
/// locally: its settings and its times and prices.
fn copy_key(drawing: &Drawing, points: impl IntoIterator<Item = (i64, u32, f64)>) -> String {
    let mut settings = drawing.clone();
    (
        settings.saveable,
        settings.locked,
        settings.pane,
        settings.z_order,
    ) = (true, false, None, 0);
    let style = serde_json::to_string(&settings).unwrap_or_default();
    let points: Vec<_> = points
        .into_iter()
        .map(|(ms, ns, price)| (ms, ns, price.to_bits()))
        .collect();
    format!("{style}{points:?}")
}

fn live_key(chart: &Chart, drawing: &Drawing) -> String {
    copy_key(
        drawing,
        drawing.chart_points.iter().map(|point| {
            let coord = chart
                .drawing_point_time(point)
                .map(|time| StoredCoord::from_time(time, point.price))
                .unwrap_or(StoredCoord::new(i64::MIN, point.price));
            (coord.ts_ms, coord.subms_ns, coord.price)
        }),
    )
}

fn stored_key(record: &StoredDrawing) -> String {
    copy_key(
        &record.drawing,
        record.points.iter().map(|p| (p.ts_ms, p.subms_ns, p.price)),
    )
}

fn live_row(manager: &DrawingManager, drawing: &Drawing) -> SymbolDrawing {
    SymbolDrawing {
        id: sync_id(drawing),
        tool_type: drawing.tool_type,
        label: drawing.label.clone(),
        modified_at: manager.modified_at(drawing),
    }
}

fn stored_row(record: &StoredDrawing) -> SymbolDrawing {
    SymbolDrawing {
        id: record.id.clone(),
        tool_type: record.drawing.tool_type,
        label: record.drawing.label.clone(),
        modified_at: Some(record.modified_at),
    }
}

/// Whether the drawings of `symbol` that the chart in `slot` read from storage are the ones to
/// list for that storage: no chart on it shows the symbol, and none read them later.
fn lists_stored(
    charts: &[&mut Chart],
    slot: usize,
    symbol: &str,
    drawings: &DrawingStorage,
) -> bool {
    charts.iter().enumerate().all(|(other, chart)| {
        !chart.shares_stored_drawings(charts[slot])
            || (chart.symbol() != symbol
                && (other == slot
                    || chart.stored_symbol_drawings(symbol).is_none_or(|theirs| {
                        (theirs.metadata.modified_at, Reverse(other))
                            < (drawings.metadata.modified_at, Reverse(slot))
                    })))
    })
}

/// Each symbol's drawings with the key of each, a copy on another chart counted once.
fn inventory(charts: &[&mut Chart]) -> BTreeMap<String, Vec<(SymbolDrawing, String)>> {
    let mut symbols: BTreeMap<String, Vec<(SymbolDrawing, String)>> = BTreeMap::new();
    let mut listed: HashMap<(String, String), usize> = HashMap::new();
    for (slot, chart) in charts.iter().enumerate() {
        let mut held: HashMap<(String, String), usize> = HashMap::new();
        let mut hold = |symbol: &str, row: SymbolDrawing, key: String| {
            let id = (symbol.to_owned(), key.clone());
            let count = held.entry(id.clone()).or_default();
            *count += 1;
            let most = listed.entry(id).or_default();
            if *count > *most {
                *most = *count;
                symbols
                    .entry(symbol.to_owned())
                    .or_default()
                    .push((row, key));
            }
        };
        if !chart.symbol().is_empty() {
            for drawing in chart.drawings().drawings.iter().filter(|d| inventoried(d)) {
                hold(
                    chart.symbol(),
                    live_row(chart.drawings(), drawing),
                    live_key(chart, drawing),
                );
            }
        }
        if chart.keeps_stored_drawings() {
            for (symbol, drawings) in chart.stored_symbols() {
                if lists_stored(charts, slot, symbol, drawings) {
                    for record in drawings.drawings().iter().filter(|r| r.drawing.listed) {
                        hold(symbol, stored_row(record), stored_key(record));
                    }
                }
            }
        } else {
            for (symbol, (manager, _)) in &chart.symbol_drawings {
                for drawing in manager.drawings.iter().filter(|d| inventoried(d)) {
                    hold(symbol, live_row(manager, drawing), live_key(chart, drawing));
                }
            }
        }
    }
    symbols
}

impl ChartLayout {
    /// The drawings of the layout grouped by the symbol they are saved under, by symbol name.
    /// Lists the drawings that object lists include and charts save, across every supplied chart:
    /// those of its shown symbol, of the other symbols it retains and, for a chart that keeps
    /// its drawings in storage, of each symbol stored there once
    /// [`Self::read_drawing_inventory`] was answered. A drawing and its copies on other charts
    /// appear once. Call while a list is shown; the cost grows with the drawings.
    pub fn drawing_inventory(&self, charts: &[&mut Chart]) -> Vec<SymbolDrawings> {
        inventory(charts)
            .into_iter()
            .map(|(symbol, rows)| SymbolDrawings {
                symbol,
                drawings: rows.into_iter().map(|(row, _)| row).collect(),
            })
            .collect()
    }

    /// Asks the storage of each chart for the drawings of every symbol, for
    /// [`Self::drawing_inventory`]. Afterwards the chart follows its own reads and writes; call
    /// this again when a list opens to take in what other clients changed.
    pub fn read_drawing_inventory(&self, charts: &mut [&mut Chart]) {
        for chart in charts.iter_mut() {
            chart.read_stored_symbols();
        }
    }

    /// Deletes the drawings [`Self::drawing_inventory`] lists under `symbol`, locked ones
    /// included, and leaves those of other symbols. Stored drawings are written without them.
    /// Undoing the edit on the active chart or any chart it changed restores them on all of
    /// them. Returns whether the symbol had drawings. Call it outside [`Chart::user_action`]:
    /// each chart records its part as its own action.
    pub fn delete_symbol_drawings(&mut self, charts: &mut [&mut Chart], symbol: &str) -> bool {
        let keys: HashSet<String> = inventory(charts)
            .remove(symbol)
            .into_iter()
            .flatten()
            .map(|(_, key)| key)
            .collect();
        if keys.is_empty() {
            return false;
        }
        let active = self.active.min(charts.len().saturating_sub(1));
        // Charts on one storage read each other's writes, so that one of them deletes what they
        // store: one that shows the symbol keeps its undo through the read.
        let writes: Vec<bool> = (0..charts.len())
            .map(|slot| {
                let sharing: Vec<usize> = (0..charts.len())
                    .filter(|&other| {
                        other == slot || charts[other].shares_stored_drawings(charts[slot])
                    })
                    .collect();
                let shows = |at: &usize| charts[*at].symbol() == symbol;
                let writer = if sharing.iter().any(shows) {
                    Some(active)
                        .filter(|at| sharing.contains(at) && shows(at))
                        .or_else(|| sharing.iter().copied().find(|at| shows(at)))
                } else {
                    Some(active).filter(|at| sharing.contains(at))
                };
                writer.unwrap_or(sharing[0]) == slot
            })
            .collect();
        let edit = self.edits.start();
        let mut linked = HashSet::new();
        // Writers first: of a linked drawing and its copies, one goes and takes the others along.
        let mut slots: Vec<usize> = (0..charts.len()).collect();
        slots.sort_by_key(|slot| !writes[*slot]);
        for slot in slots {
            let writes = writes[slot];
            let doomed = |chart: &Chart, drawing: &Drawing| {
                if writes && inventoried(drawing) {
                    return true;
                }
                !drawing.saveable && keys.contains(&live_key(chart, drawing))
            };
            let chart = &mut *charts[slot];
            let ids: Vec<usize> = if chart.symbol() == symbol {
                chart
                    .drawings()
                    .drawings
                    .iter()
                    .filter(|drawing| doomed(chart, drawing))
                    .filter(|drawing| {
                        let origin = self.drawings.origins.get(&(slot, drawing.id));
                        !mirrored(drawing.folder_sync_mode(), self.sync.sync_drawings)
                            || origin.is_none_or(|origin| linked.insert(*origin))
                    })
                    .map(|drawing| drawing.id)
                    .collect()
            } else {
                Vec::new()
            };
            let retained: Vec<usize> = chart
                .symbol_drawings
                .get(symbol)
                .map(|(manager, _)| {
                    let stored = !chart.keeps_stored_drawings()
                        || chart.stored_symbol_drawings(symbol).is_some();
                    manager
                        .drawings
                        .iter()
                        .filter(|drawing| (stored || !drawing.saveable) && doomed(chart, drawing))
                        .map(|drawing| drawing.id)
                        .collect()
                })
                .unwrap_or_default();
            chart.user_action(|chart| {
                let changed = chart.delete_symbol_part(symbol, writes, &ids, &retained);
                if changed || slot == active {
                    chart.history.join_layout_edit(edit.clone());
                }
            });
        }
        true
    }
}

impl Chart {
    /// Removes this chart's part of a deletion from `symbol`: `ids` of the shown drawings when it
    /// shows the symbol, otherwise the `retained` drawings and, if it writes them, the stored
    /// ones. Returns whether anything changed.
    fn delete_symbol_part(
        &mut self,
        symbol: &str,
        writes: bool,
        ids: &[usize],
        retained: &[usize],
    ) -> bool {
        if self.symbol == symbol {
            self.drawings.remove_drawings(ids);
            return !ids.is_empty();
        }
        if let Some((manager, _)) = self.symbol_drawings.get_mut(symbol) {
            manager.remove_drawings(retained);
        }
        let stored = self
            .stored_symbol_drawings(symbol)
            .filter(|_| writes)
            .cloned();
        if let Some(stored) = &stored {
            let mut left = stored.clone();
            left.drawings.retain(|record| !record.drawing.listed);
            self.write_unshown_drawings(symbol, left);
        }
        if retained.is_empty() && stored.is_none() {
            return false;
        }
        self.history.record_unshown_deletion(UnshownDeletion {
            symbol: symbol.to_owned(),
            retained: !retained.is_empty(),
            stored,
        });
        true
    }

    /// Takes a deletion from an unshown symbol through an undo or redo.
    pub(in crate::widget) fn step_unshown_deletion(
        &mut self,
        deletion: &mut UnshownDeletion,
        redo: bool,
    ) {
        if deletion.retained
            && let Some((manager, _)) = self.symbol_drawings.get_mut(&deletion.symbol)
        {
            if redo {
                manager.redo();
            } else {
                manager.undo();
            }
        }
        if let Some(stored) = &mut deletion.stored
            && let Some(current) = self.stored_symbol_drawings(&deletion.symbol).cloned()
        {
            let other = std::mem::replace(stored, current);
            self.write_unshown_drawings(&deletion.symbol, other);
        }
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
            .owner_pane(drawing)
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
    use crate::drawings::{ChartPoint, DrawingToolType};
    use crate::model::{Bar, BarData};
    use crate::widget::SeriesId;
    use chrono::DateTime;
    use std::sync::Arc;

    fn bars(step: i64) -> Chart {
        Chart::new(BarData::from_bars(
            (0..10)
                .map(|i| {
                    let time = DateTime::from_timestamp(i * step, 0).unwrap();
                    Bar::new(time, 10.0, 11.0, 9.0, 10.0, 1.0)
                })
                .collect(),
        ))
    }

    fn line(label: &str) -> Drawing {
        let mut drawing = Drawing::new(0, DrawingToolType::TrendLine);
        drawing.label = Some(label.into());
        drawing.chart_points = vec![ChartPoint::new(2.0, 10.0), ChartPoint::new(4.0, 12.0)];
        drawing.completed = true;
        drawing
    }

    /// Symbol and drawing names as the inventory lists them.
    fn names(layout: &ChartLayout, charts: &[&mut Chart]) -> Vec<(String, Vec<String>)> {
        layout
            .drawing_inventory(charts)
            .into_iter()
            .map(|symbol| {
                let rows = symbol.drawings.iter().map(|d| d.name().to_owned());
                (symbol.symbol.clone(), rows.collect())
            })
            .collect()
    }

    fn listed(entries: &[(&str, &[&str])]) -> Vec<(String, Vec<String>)> {
        entries
            .iter()
            .map(|(symbol, rows)| {
                let rows = rows.iter().map(|row| (*row).to_owned()).collect();
                ((*symbol).to_owned(), rows)
            })
            .collect()
    }

    /// Through the layout, without storage: a drawing and its copy on a chart of another
    /// interval are listed once, also once neither chart shows them, and deleting a symbol
    /// leaves the others, takes locked drawings along and comes back in one undo.
    #[test]
    fn inventory_lists_each_drawing_once_and_deletes_one_symbol_with_undo() {
        let (mut a, mut b) = (bars(60), bars(120));
        a.set_symbol("AAA");
        b.set_symbol("AAA");
        let mut layout = ChartLayout::default();
        layout.sync.sync_drawings = true;
        let mut locked = line("Locked");
        locked.locked = true;
        a.drawings_mut().add_drawing(locked).unwrap();
        // Host annotations that are neither saved nor offered to lists stay out of it.
        let mut host = line("Host");
        host.saveable = false;
        a.drawings_mut().add_synced_drawing(host).unwrap();
        let mut unlisted = line("Unlisted");
        unlisted.listed = false;
        a.drawings_mut().add_synced_drawing(unlisted).unwrap();
        let sync = |layout: &mut ChartLayout, a: &mut Chart, b: &mut Chart| {
            let groups = layout.groups.clone();
            layout
                .drawings
                .sync(&mut [a, b], 2, &groups, false, layout.sync.sync_drawings);
        };
        sync(&mut layout, &mut a, &mut b);
        assert_eq!(b.drawings().drawings.len(), 3, "mirrored");
        let aaa = listed(&[("AAA", &["Locked"])]);
        assert_eq!(names(&layout, &[&mut a, &mut b]), aaa);

        // On another symbol the chart retains them, and the copy still counts once.
        a.set_symbol("BBB");
        a.drawings_mut().add_drawing(line("Other")).unwrap();
        sync(&mut layout, &mut a, &mut b);
        let both = listed(&[("AAA", &["Locked"]), ("BBB", &["Other"])]);
        assert_eq!(names(&layout, &[&mut a, &mut b]), both);
        b.set_symbol("BBB");
        sync(&mut layout, &mut a, &mut b);
        b.set_symbol("AAA");
        sync(&mut layout, &mut a, &mut b);
        assert_eq!(names(&layout, &[&mut a, &mut b]), both);

        // The symbol the active chart shows goes alone, in one undo.
        assert!(layout.delete_symbol_drawings(&mut [&mut a, &mut b], "BBB"));
        assert_eq!(names(&layout, &[&mut a, &mut b]), aaa);
        assert!(a.undo());
        layout.step_layout_edits(&mut [&mut a, &mut b]);
        assert_eq!(names(&layout, &[&mut a, &mut b]), both);
        assert!(!layout.delete_symbol_drawings(&mut [&mut a, &mut b], "ZZZ"));

        // A symbol the active chart does not show goes from every chart, locked or not, and the
        // undo of the active chart brings it back on each.
        assert!(layout.delete_symbol_drawings(&mut [&mut a, &mut b], "AAA"));
        let bbb = listed(&[("BBB", &["Other"])]);
        assert_eq!(names(&layout, &[&mut a, &mut b]), bbb);
        sync(&mut layout, &mut a, &mut b);
        assert_eq!(names(&layout, &[&mut a, &mut b]), bbb);
        assert!(a.undo());
        layout.step_layout_edits(&mut [&mut a, &mut b]);
        sync(&mut layout, &mut a, &mut b);
        assert_eq!(names(&layout, &[&mut a, &mut b]), both);
        assert!(a.redo());
        layout.step_layout_edits(&mut [&mut a, &mut b]);
        assert_eq!(names(&layout, &[&mut a, &mut b]), bbb);

        // A copy takes the top layer of its chart, which is no difference in the drawing.
        let (mut c, mut d) = (bars(60), bars(60));
        c.set_symbol("AAA");
        d.set_symbol("AAA");
        let mut layout = ChartLayout::default();
        layout.sync.sync_drawings = true;
        let mut top = line("Top");
        top.z_order = 5;
        c.drawings_mut().add_drawing(top).unwrap();
        sync(&mut layout, &mut c, &mut d);
        d.drawings_mut().add_drawing(line("Under")).unwrap();
        sync(&mut layout, &mut c, &mut d);
        assert_eq!(
            names(&layout, &[&mut c, &mut d]),
            listed(&[("AAA", &["Top", "Under"])])
        );
    }

    /// Through shared storage: a symbol no chart shows is read from its document and listed once,
    /// kept copies stay out, and deleting it rewrites that document alone, keeping what lists do
    /// not include, until the undo writes it back with its times.
    #[test]
    fn stored_symbols_are_listed_and_deleted_with_undo() {
        use crate::drawings::persistence::{DrawingStorage, drawing_to_stored};
        use crate::services::document::tests::Revised;
        let host = Arc::new(Revised::default());
        let document = |symbol: &str, labels: &[(&str, bool)]| {
            let mut stored = DrawingStorage::new(symbol);
            for (at, (label, shown)) in labels.iter().enumerate() {
                let mut drawing = line(label);
                drawing.listed = *shown;
                let mut record = drawing_to_stored(&drawing, |bar| i64::from(bar as i32) * 60_000);
                record.id = (*label).to_owned();
                record.created_at = 1_000 + at as i64;
                record.modified_at = 2_000 + at as i64;
                stored.add_drawing(record);
            }
            stored.to_json().unwrap()
        };
        host.put("CCC", &document("CCC", &[("One", true), ("Hidden", false)]));
        host.put("DDD", &document("DDD", &[("Two", true)]));
        host.put("CCC (local copy)", &document("CCC", &[("Copy", true)]));
        let (mut a, mut b) = (bars(60), bars(60));
        for chart in [&mut a, &mut b] {
            chart.set_symbol("AAA");
            chart.set_storage(host.clone());
            chart.flush_stored_drawings();
        }
        let mut layout = ChartLayout::default();
        assert!(names(&layout, &[&mut a, &mut b]).is_empty());
        layout.read_drawing_inventory(&mut [&mut a, &mut b]);
        for chart in [&mut a, &mut b] {
            chart.flush_stored_drawings();
        }
        assert_eq!(
            names(&layout, &[&mut a, &mut b]),
            listed(&[("CCC", &["One"]), ("DDD", &["Two"])])
        );

        // A write, its acknowledgement, and the other chart's read of it.
        let settle = |a: &mut Chart, b: &mut Chart| {
            a.flush_stored_drawings();
            b.flush_stored_drawings();
            Chart::share_stored_drawings(&mut [&mut *a, &mut *b]);
            a.flush_stored_drawings();
            b.flush_stored_drawings();
        };
        let records = |name: &str| {
            let json = host.get(name).unwrap();
            let stored = DrawingStorage::from_json(&json).unwrap().0;
            stored
                .drawings()
                .iter()
                .map(|r| (r.id.clone(), r.modified_at))
                .collect::<Vec<_>>()
        };
        let revision = host.get("DDD");
        assert!(layout.delete_symbol_drawings(&mut [&mut a, &mut b], "CCC"));
        settle(&mut a, &mut b);
        assert_eq!(records("CCC"), [("Hidden".to_owned(), 2_001)]);
        assert_eq!(host.get("DDD"), revision, "another symbol's document");
        assert!(host.get("CCC (local copy)").is_some());
        assert_eq!(
            names(&layout, &[&mut a, &mut b]),
            listed(&[("DDD", &["Two"])])
        );
        assert_eq!(a.take_storage_error().map(|e| e.to_string()), None);
        assert_eq!(a.storage_conflict(), None);

        assert!(a.undo());
        settle(&mut a, &mut b);
        assert_eq!(
            records("CCC"),
            [("One".to_owned(), 2_000), ("Hidden".to_owned(), 2_001)]
        );
        assert_eq!(
            names(&layout, &[&mut a, &mut b]),
            listed(&[("CCC", &["One"]), ("DDD", &["Two"])])
        );
        assert!(a.redo());
        settle(&mut a, &mut b);
        assert_eq!(records("CCC"), [("Hidden".to_owned(), 2_001)]);
        assert_eq!(a.storage_conflict(), None);

        // A chart that holds no such document open still follows the write of another.
        layout.set_active(1);
        assert!(layout.delete_symbol_drawings(&mut [&mut a, &mut b], "DDD"));
        settle(&mut a, &mut b);
        assert!(names(&layout, &[&mut a, &mut b]).is_empty());
        assert!(b.undo());
        settle(&mut a, &mut b);
        assert_eq!(
            names(&layout, &[&mut a, &mut b]),
            listed(&[("DDD", &["Two"])])
        );

        // A document that no longer reads leaves the list, so a deletion cannot write over it.
        host.put("DDD", "unreadable");
        layout.read_drawing_inventory(&mut [&mut a, &mut b]);
        settle(&mut a, &mut b);
        assert!(a.take_storage_error().is_some());
        assert!(!layout.delete_symbol_drawings(&mut [&mut a, &mut b], "DDD"));
        settle(&mut a, &mut b);
        assert_eq!(host.get("DDD").as_deref(), Some("unreadable"));
    }

    /// The order by name ignores case and the order by edit time starts at the latest; equal
    /// keys fall back to the stored id, which is the same after a layout is restored. An edit
    /// time comes with the edit alone: live bars, saving and reading leave it, and an undo puts
    /// back the one before.
    #[test]
    fn drawings_sort_by_name_and_edit_time_across_saving_and_restoring() {
        let row = |id: &str, label: &str, modified_at| SymbolDrawing {
            id: id.into(),
            tool_type: DrawingToolType::TrendLine,
            label: Some(label.into()),
            modified_at,
        };
        let rows = vec![
            row("4", "beta", Some(5)),
            row("3", "Alpha", Some(5)),
            row("2", "alpha", None),
            row("1", "Gamma", Some(9)),
        ];
        for (sort, order) in [
            (DrawingSort::Name, ["2", "3", "4", "1"]),
            (DrawingSort::LastModified, ["1", "3", "4", "2"]),
        ] {
            let mut symbol = SymbolDrawings {
                symbol: "AAA".into(),
                drawings: rows.clone(),
            };
            symbol.sort(sort);
            let ids: Vec<_> = symbol.drawings.iter().map(|d| d.id.as_str()).collect();
            assert_eq!(ids, order, "{sort:?}");
        }
        // A row without a name goes by its tool's.
        assert_eq!(
            row("5", " ", None).name(),
            DrawingToolType::TrendLine.as_str()
        );

        let mut chart = bars(60);
        chart.set_symbol("AAA");
        for label in ["Older", "Newer"] {
            chart.drawings_mut().add_drawing(line(label)).unwrap();
        }
        let ids: Vec<_> = chart.drawings().drawings.iter().map(|d| d.id).collect();
        for (id, time) in ids.iter().zip([1_000, 2_000]) {
            chart.drawings.apply_edit_times();
            chart
                .drawings
                .drawings
                .iter_mut()
                .find(|d| d.id == *id)
                .unwrap()
                .modified_at = Some(time);
        }
        let times = |chart: &Chart| {
            let saved = chart.save_drawings().unwrap();
            let mut times: Vec<_> = saved
                .drawings()
                .iter()
                .map(|r| (r.drawing.label.clone().unwrap(), r.modified_at))
                .collect();
            times.sort();
            times
        };
        let before = vec![("Newer".to_owned(), 2_000), ("Older".to_owned(), 1_000)];
        assert_eq!(times(&chart), before);
        chart.set_data(SeriesId::MAIN, bars(60).bars(SeriesId::MAIN).to_vec());
        assert_eq!(times(&chart), before, "bars are no edit, nor is a save");

        // A restored chart reads the times, and gives them back when saved again.
        let json = chart.save_drawings().unwrap().to_json().unwrap();
        let mut restored = bars(60);
        restored.set_symbol("AAA");
        restored.load_drawings_remapped(&json, true).unwrap();
        assert_eq!(times(&restored), before);

        // A layer move edits the moved drawing, not those renumbered around it.
        chart.user_action(|chart| {
            chart
                .drawings_mut()
                .reorder_drawings(ids[0], &[(ids[1], 0), (ids[0], 1)])
        });
        let moved = times(&chart);
        assert_eq!(moved[0], before[0], "renumbered");
        assert!(moved[1].1 > 2_000, "edited now: {moved:?}");
        assert!(chart.undo());
        assert_eq!(times(&chart), before, "undo restores the time before");
        assert!(chart.redo());
        assert_eq!(times(&chart)[0], before[0]);

        // A drawing mirrored to another chart is no edit: its copy keeps the time.
        let mut layout = ChartLayout::default();
        layout.sync.sync_drawings = true;
        let mut other = bars(60);
        other.set_symbol("AAA");
        let groups = layout.groups.clone();
        layout
            .drawings
            .sync(&mut [&mut other, &mut restored], 2, &groups, false, true);
        let inventory = layout.drawing_inventory(&[&mut other, &mut restored]);
        let rows = inventory[0].drawings.iter();
        let mut copied: Vec<_> = rows.map(|d| d.modified_at).collect();
        copied.sort();
        assert_eq!(copied, [Some(1_000), Some(2_000)]);

        // Bulk edits are edits.
        for bulk in [
            |chart: &mut Chart| chart.drawings_mut().set_visibility_all(false),
            |chart: &mut Chart| chart.drawings_mut().toggle_lock_all(),
        ] {
            let mut chart = bars(60);
            chart.set_symbol("AAA");
            chart.load_drawings_remapped(&json, true).unwrap();
            bulk(&mut chart);
            assert!(times(&chart).iter().all(|(_, time)| *time > 2_000));
        }
    }

    /// The top-level folder decides whether its drawings mirror, over the layout-wide switch, and
    /// the charts lose the copies once its drawings stop mirroring.
    #[test]
    fn folder_sync_overrides_the_layout_switch() {
        let chart = || {
            let mut chart = Chart::new(BarData::from_bars(
                (0..10)
                    .map(|i| {
                        Bar::new(
                            DateTime::from_timestamp(i * 60, 0).unwrap(),
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
        let groups = std::array::from_fn(|_| None);
        let top = ["Top".to_owned()];
        use FolderSync::{Chart as Local, Layout};
        for (mode, layout_wide, copies) in [
            (Some(Layout), false, 1),
            (Some(Local), true, 0),
            (None, true, 1),
            (None, false, 0),
        ] {
            let (mut a, mut b) = (chart(), chart());
            let mut links = DrawingLinks::default();
            let mut drawing = Drawing::new(0, DrawingToolType::TrendLine);
            drawing.chart_points = vec![ChartPoint::new(2.0, 10.0), ChartPoint::new(4.0, 12.0)];
            drawing.completed = true;
            // Nested, so the mode comes from the top-level folder.
            drawing.folder = vec![top[0].clone(), "Inner".into()];
            a.drawings_mut().add_synced_drawing(drawing).unwrap();
            assert!(a.drawings_mut().set_folder_sync(&top, mode));
            links.sync(&mut [&mut a, &mut b], 2, &groups, false, layout_wide);
            assert_eq!(
                b.drawings().drawings.len(),
                copies,
                "{mode:?} {layout_wide}"
            );
        }

        // Mirrored, then confined to the chart: the copy leaves and an undo brings it back.
        let (mut a, mut b) = (chart(), chart());
        let mut links = DrawingLinks::default();
        let mut drawing = Drawing::new(0, DrawingToolType::TrendLine);
        drawing.chart_points = vec![ChartPoint::new(2.0, 10.0), ChartPoint::new(4.0, 12.0)];
        drawing.completed = true;
        drawing.folder = top.to_vec();
        a.drawings_mut().add_synced_drawing(drawing).unwrap();
        a.drawings_mut().set_folder_sync(&top, Some(Layout));
        let mut sync = |a: &mut Chart, b: &mut Chart| {
            links.sync(&mut [a, b], 2, &groups, false, false);
        };
        sync(&mut a, &mut b);
        assert_eq!(b.drawings().drawings.len(), 1);
        a.drawings_mut().set_folder_sync(&top, Some(Local));
        sync(&mut a, &mut b);
        assert!(b.drawings().drawings.is_empty());
        assert_eq!(a.drawings().drawings.len(), 1);
        assert!(a.drawings_mut().undo());
        sync(&mut a, &mut b);
        assert_eq!(b.drawings().drawings.len(), 1);
        // A mirrored drawing deleted here goes from the other chart too, with the switch off.
        let id = a.drawings().drawings[0].id;
        a.drawings_mut().delete_drawing(id);
        sync(&mut a, &mut b);
        assert!(b.drawings().drawings.is_empty());
    }

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
        charts[1].drawings_mut().add_synced_drawing(local).unwrap();
        let mut drawing = Drawing::new(0, DrawingToolType::TrendLine);
        drawing.chart_points = vec![ChartPoint::new(2.0, 10.0), ChartPoint::new(4.0, 12.0)];
        drawing.completed = true;
        charts[0]
            .drawings_mut()
            .add_synced_drawing(drawing)
            .unwrap();
        let mut refs: Vec<_> = charts.iter_mut().collect();
        links.sync(&mut refs, 4, &groups, false, true);
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
        links.sync(&mut refs, 4, &groups, false, true);
        assert_eq!(
            refs[1].drawings().pending_sync.lock().unwrap().len(),
            queued
        );
        refs[1]
            .drawings_mut()
            .edit_drawing(id, |d| d.chart_points[0].bar_idx = 1.5);
        links.sync(&mut refs, 4, &groups, false, true);
        assert_eq!(refs[0].drawings().drawings[0].chart_points[0].bar_idx, 3.0);
        refs[1]
            .drawings_mut()
            .edit_drawing(id, |d| d.notes = Some("shared note".into()));
        links.sync(&mut refs, 4, &groups, false, true);
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
            links.sync(&mut refs, 4, &groups, true, true);
        }
        assert_eq!(
            refs[1].drawings().pending_sync.lock().unwrap().len(),
            queued,
            "a mirrored drag must not queue an update for every frame"
        );
        links.sync(&mut refs, 4, &groups, false, true);
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
        links.sync(&mut refs, 4, &groups, false, true);
        refs[1].drawings_mut().delete_drawing(id);
        links.sync(&mut refs, 4, &groups, false, true);
        assert!(
            refs[0]
                .drawings()
                .drawings
                .iter()
                .all(|d| d.tool_type != DrawingToolType::TrendLine)
        );
        assert_eq!(refs[1].drawings().drawings[0].chart_points[0].price, 42.0);
        assert!(refs[1].drawings_mut().undo());
        links.sync(&mut refs, 4, &groups, false, true);
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
            .add_synced_drawing(refs[0].drawings().drawings[0].clone())
            .unwrap();
        let mut pending = DrawingLinks::default();
        pending.sync(&mut [&mut source, &mut target], 2, &groups, false, true);
        assert!(target.drawings().drawings.is_empty());
        target.set_data(SeriesId::MAIN, source.bars(SeriesId::MAIN).to_vec());
        pending.sync(&mut [&mut source, &mut target], 2, &groups, false, true);
        assert_eq!(target.drawings().drawings.len(), 1);
        // Element coordinates and retained timestamps must not be read as source bar indices.
        source.state.set_elements(
            Some(source.bars(SeriesId::MAIN)[..4].to_vec().into()),
            false,
        );
        let mut drawing = Drawing::new(0, DrawingToolType::TrendLine);
        drawing.chart_points = vec![ChartPoint::new(1.5, 10.0), ChartPoint::new(3.5, 12.0)];
        source.drawings_mut().add_synced_drawing(drawing).unwrap();
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
        source.drawings_mut().add_synced_drawing(owned).unwrap();
        let mut local = DrawingLinks::default();
        local.sync(&mut [&mut source, &mut target], 2, &groups, false, true);
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
        a.drawings_mut().add_synced_drawing(drawing).unwrap();
        let source = a.drawings().drawings[0].id;
        let mut sync =
            |a: &mut Chart, b: &mut Chart| links.sync(&mut [a, b], 2, &groups, false, true);
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

    /// Each chart keeps its drawings in its own storage. A drawing in a folder mirrored across the
    /// layout, then kept on the chart that shows its copy, is still shown there and only there
    /// after a restart, with or without later work on the first chart: the user moved it to that
    /// chart, they did not delete it.
    #[test]
    fn keeping_a_mirrored_copy_on_its_chart_survives_a_restart() {
        use crate::data::Reply;
        use crate::services::{Storage, StoreKind};
        use crate::widget::{ChartLayout, ChartLayoutMode};
        use std::sync::{Arc, Mutex};

        /// Documents held in memory, answered inline.
        #[derive(Default)]
        struct InMemory(Mutex<HashMap<(StoreKind, String), String>>);
        impl Storage for InMemory {
            fn list(&self, kind: StoreKind, reply: Reply<Vec<String>>) {
                let names = self
                    .0
                    .lock()
                    .unwrap()
                    .keys()
                    .filter(|(k, _)| *k == kind)
                    .map(|(_, name)| name.clone())
                    .collect();
                reply.send(Ok(names));
            }
            fn load(&self, kind: StoreKind, name: &str, reply: Reply<Option<String>>) {
                let document = self
                    .0
                    .lock()
                    .unwrap()
                    .get(&(kind, name.to_owned()))
                    .cloned();
                reply.send(Ok(document));
            }
            fn save(&self, kind: StoreKind, name: &str, document: String, reply: Reply<()>) {
                self.0
                    .lock()
                    .unwrap()
                    .insert((kind, name.to_owned()), document);
                reply.send(Ok(()));
            }
            fn delete(&self, kind: StoreKind, name: &str, reply: Reply<()>) {
                self.0.lock().unwrap().remove(&(kind, name.to_owned()));
                reply.send(Ok(()));
            }
        }
        let chart = |storage: &Arc<InMemory>| {
            let mut chart = Chart::new(BarData::from_bars(
                (0..10)
                    .map(|i| {
                        Bar::new(
                            DateTime::from_timestamp(i * 60, 0).unwrap(),
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
            chart.set_storage(storage.clone());
            chart
        };
        let line = |price: f64| {
            let mut drawing = Drawing::new(0, DrawingToolType::TrendLine);
            drawing.chart_points = vec![
                ChartPoint::new(2.0, price),
                ChartPoint::new(4.0, price + 2.0),
            ];
            drawing
        };
        let ctx = egui::Context::default();
        let frames = |layout: &mut ChartLayout, a: &mut Chart, b: &mut Chart| {
            for _ in 0..4 {
                ctx.run_ui(egui::RawInput::default(), |ui| {
                    layout.show(ui, &mut [&mut *a, &mut *b]);
                })
                .drop_without_applying_deltas();
            }
        };
        let layout = || ChartLayout {
            mode: ChartLayoutMode::TwoHorizontal,
            ..Default::default()
        };
        let top = ["Top".to_owned()];
        let in_top = |a: &Chart, b: &Chart| {
            [a, b].map(|chart| {
                chart
                    .drawings()
                    .drawings
                    .iter()
                    .filter(|d| d.folder == top)
                    .count()
            })
        };
        for edit_first in [false, true] {
            let storages = [Arc::new(InMemory::default()), Arc::new(InMemory::default())];
            let mut session = layout();
            let (mut a, mut b) = (chart(&storages[0]), chart(&storages[1]));
            frames(&mut session, &mut a, &mut b);
            a.user_action(|chart| {
                let id = chart.drawings_mut().add_drawing(line(10.0)).unwrap();
                assert!(chart.drawings_mut().move_to_folder(&[id], &[], &top));
                assert!(
                    chart
                        .drawings_mut()
                        .set_folder_sync(&top, Some(FolderSync::Layout))
                );
            });
            frames(&mut session, &mut a, &mut b);
            assert_eq!(in_top(&a, &b), [1, 1], "mirrored");
            // On the chart that shows the copy, the folder is kept on that chart.
            b.user_action(|chart| {
                assert!(
                    chart
                        .drawings_mut()
                        .set_folder_sync(&top, Some(FolderSync::Chart))
                );
            });
            frames(&mut session, &mut a, &mut b);
            if edit_first {
                a.user_action(|chart| {
                    chart.drawings_mut().add_drawing(line(20.0)).unwrap();
                });
                frames(&mut session, &mut a, &mut b);
            }
            let shown = in_top(&a, &b);
            let mut restarted = layout();
            let (mut a, mut b) = (chart(&storages[0]), chart(&storages[1]));
            frames(&mut restarted, &mut a, &mut b);
            let restored = in_top(&a, &b);
            assert_eq!(
                (shown, restored),
                ([0, 1], [0, 1]),
                "drawings in Top per chart before and after the restart, edit_first \
                 {edit_first}: {shown:?} {restored:?}"
            );
        }
    }

    /// Two charts keep their drawings in one storage that keeps revisions, a third in its own.
    /// Keeping a mirrored drawing on the third chart takes it out of the shared storage in one
    /// write, which the other chart there reads: a second write would be refused as a conflict
    /// for the user to settle, and that chart would write nothing until then.
    #[test]
    fn keeping_a_mirrored_copy_writes_a_shared_storage_once() {
        use crate::services::document::tests::Revised;
        use crate::widget::{ChartLayout, ChartLayoutMode};

        let (shared, own) = (Arc::new(Revised::default()), Arc::new(Revised::default()));
        let charts = || {
            [&shared, &own, &shared].map(|storage| {
                let mut chart = bars(60);
                chart.set_symbol("SAMPLE");
                chart.set_storage(storage.clone());
                chart
            })
        };
        let layout = || ChartLayout {
            mode: ChartLayoutMode::ThreeHorizontal,
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let frames = |layout: &mut ChartLayout, charts: &mut [Chart; 3]| {
            for _ in 0..4 {
                ctx.run_ui(egui::RawInput::default(), |ui| {
                    layout.show(ui, &mut charts.each_mut());
                })
                .drop_without_applying_deltas();
            }
        };
        let counts = |charts: &[Chart; 3]| {
            charts
                .each_ref()
                .map(|chart| chart.drawings().drawings.len())
        };
        let top = ["Top".to_owned()];
        let mut session = layout();
        let mut shown = charts();
        frames(&mut session, &mut shown);
        shown[0].user_action(|chart| {
            let id = chart.drawings_mut().add_drawing(line("Kept")).unwrap();
            assert!(chart.drawings_mut().move_to_folder(&[id], &[], &top));
            assert!(
                chart
                    .drawings_mut()
                    .set_folder_sync(&top, Some(FolderSync::Layout))
            );
        });
        frames(&mut session, &mut shown);
        assert_eq!(counts(&shown), [1, 1, 1], "mirrored");
        shown[1].user_action(|chart| {
            assert!(
                chart
                    .drawings_mut()
                    .set_folder_sync(&top, Some(FolderSync::Chart))
            );
        });
        frames(&mut session, &mut shown);
        assert_eq!(shown.each_ref().map(Chart::storage_conflict), [None; 3]);
        let mut restarted = charts();
        frames(&mut layout(), &mut restarted);
        assert_eq!((counts(&shown), counts(&restarted)), ([0, 1, 0], [0, 1, 0]));
    }

    /// Guards #1622: bars evicted from a chart or loaded in front of its bars renumber its
    /// drawings but keep their times, which is no edit to mirror: the copy queues nothing and its
    /// first undo step is still its addition.
    #[test]
    fn bars_evicted_or_loaded_in_front_are_not_a_drawing_edit() {
        let bar = |minute: i64| {
            Bar::new(
                DateTime::from_timestamp(minute * 60, 0).unwrap(),
                10.0,
                11.0,
                9.0,
                10.0,
                1.0,
            )
        };
        for evict in [true, false] {
            // Minutes 1 to 8 with room for 9 bars, linked to minutes 0 to 11.
            let mut source = Chart::new(BarData::from_bars((1..9).map(bar).collect()));
            let mut target = Chart::new(BarData::from_bars((0..12).map(bar).collect()));
            source.set_symbol("SAMPLE");
            target.set_symbol("SAMPLE");
            source.set_bar_limit(SeriesId::MAIN, std::num::NonZeroUsize::new(9));
            let groups = std::array::from_fn(|_| None);
            let mut links = DrawingLinks::default();
            let mut drawing = Drawing::new(0, DrawingToolType::TrendLine);
            drawing.chart_points = vec![ChartPoint::new(4.0, 10.0), ChartPoint::new(6.0, 12.0)];
            drawing.completed = true;
            source.drawings_mut().add_synced_drawing(drawing).unwrap();
            links.sync(&mut [&mut source, &mut target], 2, &groups, false, true);
            target.drawings().pending_sync.lock().unwrap().clear();
            if evict {
                // Ten bars exceed the limit: the oldest two go, and minute 5 moves from index 4
                // to 2.
                source.update(SeriesId::MAIN, bar(9));
                source.update(SeriesId::MAIN, bar(10));
                assert_eq!(source.bars(SeriesId::MAIN).len(), 8);
            } else {
                // An older bar in front moves minute 5 from index 4 to 5.
                source.set_bar_limit(SeriesId::MAIN, None);
                source.set_data(SeriesId::MAIN, (0..10).map(bar).collect());
            }
            links.sync(&mut [&mut source, &mut target], 2, &groups, false, true);
            assert!(
                target.drawings().pending_sync.lock().unwrap().is_empty(),
                "evict {evict}"
            );
            assert!(target.drawings_mut().undo(), "evict {evict}");
            assert!(target.drawings().drawings.is_empty(), "evict {evict}");
        }
    }
}
