//! Propagate drawing changes without consuming the host's persistence queue, and list and delete
//! the layout's drawings by symbol.

use super::ChartLayout;
use crate::config::{DrawingSort, DrawingToolType};
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
    /// The [`DrawingLinks::frame`] in which the layout last saw the drawing change, or wrote the
    /// change of another drawing to it.
    seen: u64,
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

/// What a drawing and its copies have in common: [`live_key`] and the notes.
fn content(chart: &Chart, drawing: &Drawing) -> String {
    format!("{}{:?}", live_key(chart, drawing), drawing.notes)
}

type Change = (Key, Option<Drawing>, Option<usize>);

#[derive(Clone, Default)]
pub(super) struct DrawingLinks {
    previous: HashMap<Key, Snapshot>,
    origins: HashMap<Key, Key>,
    retry: HashSet<Key>,
    /// Stored drawings that a gesture in progress changed through a linked drawing on another
    /// chart, whose charts write them once it ends.
    unwritten: HashSet<Key>,
    /// Drawings a chart put aside with a symbol, by slot, symbol and id, with their
    /// [`Snapshot::seen`] and the keys of the drawings of that symbol they are linked to: the
    /// chart takes them up again when it returns.
    aside: HashMap<(usize, String, usize), (u64, Vec<Key>)>,
    /// The symbol each chart showed at the last frame, to see a chart come to a symbol.
    shown: Vec<Option<String>>,
    /// Charts that came to a symbol and still wait for the mirrored drawings of it.
    arriving: HashSet<usize>,
    /// Whether the layout-wide switch was on at the last frame.
    wide: bool,
    /// The frames `sync` ran in, which order the changes it sees: edit times cannot, as an undo
    /// puts back the time of the state it restores and a copy is stamped when it is written.
    frame: u64,
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
        self.unwritten = self.unwritten.drain().map(moved).collect();
        self.aside = self
            .aside
            .drain()
            .map(|((slot, symbol, id), (seen, partners))| {
                let partners = partners.into_iter().map(moved).collect();
                ((moved((slot, id)).0, symbol, id), (seen, partners))
            })
            .collect();
        if a.max(b) < self.shown.len() {
            self.shown.swap(a, b);
        }
        self.arriving = self
            .arriving
            .drain()
            .map(|slot| moved((slot, 0)).0)
            .collect();
    }

    /// Forgets the drawings that the layout-wide switch mirrors, as when it is first turned on.
    /// Drawings whose folder mirrors them stay linked, on show or put aside.
    pub(super) fn forget_switched(&mut self, charts: &[&mut Chart], count: usize) {
        self.previous.retain(|_, old| old.sync.is_some());
        self.origins
            .retain(|key, _| self.previous.contains_key(key));
        self.retry.retain(|key| self.previous.contains_key(key));
        self.unwritten.retain(|key| self.previous.contains_key(key));
        self.aside.retain(|(slot, symbol, id), _| {
            *slot < count && {
                let chart = &charts[*slot];
                let manager = if chart.symbol() == symbol {
                    Some(chart.drawings())
                } else {
                    chart
                        .symbol_drawings
                        .get(symbol)
                        .map(|(manager, _)| manager)
                };
                manager.is_some_and(|manager| {
                    manager
                        .drawings
                        .iter()
                        .any(|d| d.id == *id && d.folder_sync_mode().is_some())
                })
            }
        });
    }

    /// Forgets the drawings of the chart in `slot`, supplied or restored there anew: its ids may
    /// name other drawings now. They are new to the layout, and the chart receives the mirrored
    /// drawings of its symbol as a chart that comes to it does.
    pub(super) fn forget_slot(&mut self, slot: usize) {
        self.previous.retain(|key, _| key.0 != slot);
        self.origins.retain(|key, _| key.0 != slot);
        self.retry.retain(|key| key.0 != slot);
        self.unwritten.retain(|key| key.0 != slot);
        self.aside.retain(|entry, _| entry.0 != slot);
        for (_, partners) in self.aside.values_mut() {
            partners.retain(|key| key.0 != slot);
        }
        self.arriving.insert(slot);
        self.rename_links(|origin, _| origin.0 == slot);
    }

    /// Gives each link that `gone` picks by name and symbol the name of one of its drawings on
    /// show: the drawing it is named after is gone, and a new drawing that takes its id starts a
    /// link of its own. A link keeps the name of a drawing deleted on its chart, which an undo
    /// there brings back.
    fn rename_links(&mut self, gone: impl Fn(Key, &str) -> bool) {
        // Links of different symbols may have one name.
        let mut names: HashMap<(Key, &str), Key> = HashMap::new();
        for (&key, &origin) in &self.origins {
            if let Some(old) = self.previous.get(&key)
                && gone(origin, &old.symbol)
            {
                let name = names.entry((origin, &old.symbol)).or_insert(key);
                *name = (*name).min(key);
            }
        }
        for (key, origin) in &mut self.origins {
            if let Some(&name) = self
                .previous
                .get(key)
                .and_then(|old| names.get(&(*origin, old.symbol.as_str())))
            {
                *origin = name;
            }
        }
    }

    fn origin_of(&self, key: Key) -> Key {
        self.origins.get(&key).copied().unwrap_or(key)
    }

    /// The id of the drawing on show in `slot` that is linked under `origin`.
    fn linked_on(&self, charts: &[&mut Chart], slot: usize, origin: Key) -> Option<usize> {
        self.origins
            .iter()
            .find_map(|(&(target_slot, id), &value)| {
                (target_slot == slot
                    && value == origin
                    && charts[slot].drawings().drawings.iter().any(|d| d.id == id))
                .then_some(id)
            })
    }

    /// Links `key` to the first of `candidates`, linked ones first, on a chart with no drawing in
    /// its link yet, and to the others that fit that link. Returns the keys it linked.
    fn join(&mut self, charts: &[&mut Chart], key: Key, mut candidates: Vec<Key>) -> Vec<Key> {
        // A drawing taken up again in the same frame then joins the link on show instead of
        // starting another beside it.
        candidates.sort_by_key(|candidate| !self.origins.contains_key(candidate));
        let mut origin = None;
        let mut linked = Vec::new();
        for candidate in candidates {
            let theirs = self.origin_of(candidate);
            let fits = match origin {
                None => self.linked_on(charts, key.0, theirs).is_none(),
                Some(origin) => theirs == origin || !self.origins.contains_key(&candidate),
            };
            if fits {
                let origin = *origin.get_or_insert(theirs);
                self.origins.entry(candidate).or_insert(origin);
                linked.push(candidate);
            }
        }
        if let Some(origin) = origin {
            self.origins.insert(key, origin);
        }
        linked
    }

    /// The drawings on other charts of the symbol and group of `key` that are not linked to a
    /// drawing of its chart and equal `drawing` in content, one per chart: what a restored
    /// original and its restored copy are.
    fn equal_drawings(
        &self,
        charts: &[&mut Chart],
        count: usize,
        groups: &[Option<String>; 16],
        key: Key,
        drawing: &Drawing,
        layout_wide: bool,
    ) -> Vec<Key> {
        if drawing.pane.is_some() {
            return Vec::new();
        }
        let own = content(charts[key.0], drawing);
        (0..count)
            .filter(|&slot| {
                slot != key.0
                    && groups[slot] == groups[key.0]
                    && charts[slot].symbol() == charts[key.0].symbol()
                    // Charts on one storage read each other's drawings from it.
                    && !(drawing.saveable && charts[slot].shares_stored_drawings(charts[key.0]))
            })
            .filter_map(|slot| {
                let other = charts[slot].drawings().drawings.iter().find(|d| {
                    d.pane.is_none()
                        && d.tool_type == drawing.tool_type
                        && d.chart_points.len() == drawing.chart_points.len()
                        && mirrored(d.folder_sync_mode(), layout_wide)
                        && self
                            .linked_on(charts, key.0, self.origin_of((slot, d.id)))
                            .is_none()
                        && content(charts[slot], d) == own
                })?;
                Some((slot, other.id))
            })
            .collect()
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
        self.frame += 1;
        // Turning the switch on is the first time for the drawings it mirrors: nothing made while
        // it was off is a change.
        if layout_wide != self.wide {
            if layout_wide {
                self.forget_switched(charts, count);
            }
            self.wide = layout_wide;
        }
        self.shown.resize(count, None);
        for (slot, chart) in charts.iter().take(count).enumerate() {
            let before = self.shown[slot].replace(chart.symbol().to_owned());
            if before.is_some_and(|before| before != chart.symbol()) {
                self.arriving.insert(slot);
            }
        }
        // A drawing a chart put aside with its symbol keeps the drawings of that symbol it was
        // linked to: those on show, and those put aside that keep it.
        let put_aside: Vec<_> = self
            .previous
            .iter()
            .filter(|(key, old)| key.0 < count && old.symbol != charts[key.0].symbol())
            .map(|(&key, old)| {
                // An edit made before the symbol change in this frame is seen now.
                let chart = &charts[key.0];
                let edited = chart
                    .symbol_drawings
                    .get(&old.symbol)
                    .and_then(|(manager, _)| manager.drawings.iter().find(|d| d.id == key.1))
                    .is_some_and(|d| {
                        serde_json::to_string(d).is_ok_and(|style| style != old.style)
                            || timed_points(chart, d) != old.points
                            || d.notes != old.notes
                    });
                let seen = if edited { self.frame } else { old.seen };
                (key, old.symbol.clone(), seen)
            })
            .collect();
        for (key, symbol, seen) in &put_aside {
            let mut partners: Vec<Key> = self
                .aside
                .iter()
                .filter(|((_, aside_symbol, _), (_, linked))| {
                    aside_symbol == symbol && linked.contains(key)
                })
                .map(|((slot, _, id), _)| (*slot, *id))
                .collect();
            if let Some(origin) = self.origins.get(key) {
                for (other, value) in &self.origins {
                    if value == origin
                        && other.0 != key.0
                        && self
                            .previous
                            .get(other)
                            .is_some_and(|old| old.symbol == *symbol)
                        && !partners.contains(other)
                    {
                        partners.push(*other);
                    }
                }
            }
            // A chart that put a copy aside earlier also takes the drawings this one is linked
            // to, such as the copy of a chart that came to the symbol after it left.
            for ((slot, aside_symbol, id), (_, linked)) in &mut self.aside {
                if aside_symbol == symbol && linked.contains(key) {
                    for partner in &partners {
                        if *partner != (*slot, *id) && !linked.contains(partner) {
                            linked.push(*partner);
                        }
                    }
                }
            }
            if !partners.is_empty() {
                self.aside
                    .insert((key.0, symbol.clone(), key.1), (*seen, partners));
            }
        }
        // Ids belong to a symbol's drawings, so the records of a symbol a chart left go before
        // anything is matched by key.
        for (key, ..) in &put_aside {
            self.origins.remove(key);
            self.previous.remove(key);
            self.retry.remove(key);
            self.unwritten.remove(key);
        }
        let mut changes: Vec<Change> = Vec::new();
        // The `Snapshot::seen` of each drawing on show.
        let mut seen = HashMap::new();
        for (slot, chart) in charts.iter().take(count).enumerate() {
            for drawing in &chart.drawings().drawings {
                let Ok(style) = serde_json::to_string(drawing) else {
                    continue;
                };
                let key = (slot, drawing.id);
                let old = self.previous.get(&key);
                let retry = self.retry.remove(&key);
                let points = timed_points(chart, drawing);
                let edited = old.is_none_or(|old| {
                    old.style != style || old.points != points || old.notes != drawing.notes
                });
                if edited || retry {
                    changes.push((key, Some(drawing.clone()), None));
                }
                let frame = match old {
                    Some(old) if !edited => old.seen,
                    Some(_) => self.frame,
                    // A drawing taken up again keeps the frame it was put aside with.
                    None => self
                        .aside
                        .get(&(slot, chart.symbol().to_owned(), drawing.id))
                        .map_or(self.frame, |(seen, _)| *seen),
                };
                seen.insert(key, frame);
            }
        }
        for (&key, old) in &self.previous {
            if key.0 < count && !seen.contains_key(&key) && old.symbol == charts[key.0].symbol() {
                changes.push((key, None, None));
            }
        }
        let mut joined = self.join_read_drawings(charts, count, groups, &changes);
        let mut extra = self.join_new_drawings(
            charts,
            count,
            groups,
            &changes,
            &seen,
            layout_wide,
            &mut joined,
        );
        let arrived = self.arrived_drawings(charts, count, groups, &changes, &joined, layout_wide);
        let arrived: Vec<_> = arrived
            .into_iter()
            .filter(|a| !extra.iter().any(|e| e.0 == a.0 && e.2 == a.2))
            .collect();
        extra.extend(arrived);
        let changes = changes
            .into_iter()
            .filter(|(key, ..)| !joined.contains(key))
            .chain(extra);
        for (key, drawing, only) in changes {
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
            // A copy carries the change it copies, not one of its own.
            let frame = seen.get(&key).copied().unwrap_or(self.frame);
            let symbol = charts[key.0].symbol().to_owned();
            let mut saveable = false;
            let mut writers: Vec<usize> = Vec::new();
            let mut deleted = vec![key];
            for slot in 0..count {
                // Charts on one storage read each other's stored drawings instead.
                let stored = drawing.as_ref().is_some_and(|drawing| drawing.saveable);
                if slot == key.0
                    || only.is_some_and(|only| only != slot)
                    || groups[slot] != groups[key.0]
                    || charts[slot].symbol() != symbol
                    || (stored && charts[slot].shares_stored_drawings(charts[key.0]))
                {
                    continue;
                }
                let linked = self.linked_on(charts, slot, origin);
                if let Some(drawing) = drawing.as_ref().filter(|_| mirror) {
                    let Some(mut copy) =
                        map_drawing(drawing, charts[key.0], charts[slot], linked.is_some())
                    else {
                        // A new copy without bars yet: a change tries again, an arrival waits
                        // for them.
                        if only.is_some() {
                            self.arriving.insert(slot);
                        } else {
                            self.retry.insert(key);
                        }
                        continue;
                    };
                    changed = true;
                    if let Some(id) = linked {
                        unlock(charts[slot], id);
                        if writes(charts, &writers, key.0, slot, id) {
                            writers.push(slot);
                            if pointer_down {
                                self.unwritten.insert((slot, id));
                            } else {
                                charts[slot].history.drawing_edits += 1;
                            }
                        }
                        // A copy, as one added below, while the chart of the changed drawing
                        // stores it: an undo on that chart can bring back a stored original that
                        // this chart stores too.
                        let copy_only = charts[slot].keeps_stored_drawings()
                            && charts[key.0].keeps_stored_drawings()
                            && charts[key.0]
                                .drawings()
                                .drawings
                                .iter()
                                .any(|d| d.id == key.1 && d.saveable);
                        charts[slot].drawings_mut().drag_edit_drawing(id, |target| {
                            copy.id = target.id;
                            copy.stored_id.clone_from(&target.stored_id);
                            copy.created_at = target.created_at;
                            copy.saveable = target.saveable && !copy_only;
                            *target = copy;
                        });
                        seen.insert((slot, id), frame);
                    } else {
                        copy.stored_id = None;
                        copy.created_at = None;
                        // Not saved where it is mirrored: the chart it was drawn on keeps it, and
                        // a drawing that is not saveable stays when this chart reads its storage.
                        copy.saveable &= !charts[slot].keeps_stored_drawings();
                        if let Ok(added) = charts[slot].drawings_mut().add_synced_drawing(copy) {
                            self.origins.insert((slot, added), origin);
                            seen.insert((slot, added), frame);
                        }
                    }
                } else if let Some(id) = linked {
                    changed = true;
                    unlock(charts[slot], id);
                    if writes(charts, &writers, key.0, slot, id) {
                        saveable = true;
                        writers.push(slot);
                        charts[slot].history.drawing_edits += 1;
                    }
                    charts[slot].drawings_mut().delete_drawing(id);
                    deleted.push((slot, id));
                }
            }
            if !mirror {
                // A drawing deleted, or confined to its chart, also leaves the charts that put a
                // copy aside, which may be linked to any of the drawings removed here.
                for gone in deleted {
                    self.delete_aside_copies(charts, count, gone);
                }
            }
            // The copy that stays, or the one deleted, is now the only instance, so it is stored
            // where it is, also when an undo or redo on its chart brings it back.
            if saveable {
                charts[key.0].history.keep_saveable(key.1);
                if let Some(kept) = charts[key.0]
                    .drawings_mut()
                    .drawings
                    .iter_mut()
                    .find(|d| d.id == key.1 && !d.saveable)
                {
                    kept.saveable = true;
                    charts[key.0].history.drawing_edits += 1;
                }
            }
        }
        if !pointer_down {
            // Written once, as a gesture on the chart itself is.
            for (slot, _) in self.unwritten.drain() {
                if slot < count {
                    charts[slot].history.drawing_edits += 1;
                }
            }
            for chart in charts.iter_mut().take(count) {
                chart.drawings_mut().end_drag_handle();
            }
        }
        let gone: Vec<_> = self
            .aside
            .keys()
            .filter(|(slot, symbol, _)| {
                *slot >= count || !charts[*slot].symbol_drawings.contains_key(symbol)
            })
            .cloned()
            .collect();
        let mut forgotten: Vec<(usize, String)> = Vec::new();
        for (slot, symbol, id) in gone {
            self.aside.remove(&(slot, symbol.clone(), id));
            // A chart away from the symbol that holds it no more forgot its drawings there, whose
            // ids a new drawing of the symbol may take.
            if slot < count
                && charts[slot].symbol() != symbol
                && !forgotten.contains(&(slot, symbol.clone()))
            {
                forgotten.push((slot, symbol));
            }
        }
        for ((_, symbol, _), (_, partners)) in &mut self.aside {
            partners.retain(|key| {
                !forgotten
                    .iter()
                    .any(|(slot, gone)| *slot == key.0 && gone == symbol)
            });
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
                            seen: seen.get(&(slot, drawing.id)).copied().unwrap_or(self.frame),
                        },
                    );
                }
            }
        }
        self.origins
            .retain(|key, _| self.previous.contains_key(key));
        for (slot, symbol) in forgotten {
            self.rename_links(|origin, shown| origin.0 == slot && shown == symbol);
        }
        self.retry.retain(|key| self.previous.contains_key(key));
        self.unwritten.retain(|key| self.previous.contains_key(key));
        changed
    }

    /// Links each drawing new to the layout that a chart took up again, or that equals one on
    /// another chart, to that drawing instead of mirroring it. Returns the changes that give each
    /// chart of the symbol of a returning drawing the version of its link that the layout saw
    /// change last, by the `seen` frame of each drawing on show.
    fn join_new_drawings(
        &mut self,
        charts: &[&mut Chart],
        count: usize,
        groups: &[Option<String>; 16],
        changes: &[Change],
        seen: &HashMap<Key, u64>,
        layout_wide: bool,
        joined: &mut HashSet<Key>,
    ) -> Vec<Change> {
        let mut returned = Vec::new();
        for (key, drawing, _) in changes {
            let Some(drawing) = drawing else { continue };
            let symbol = charts[key.0].symbol().to_owned();
            if joined.contains(key)
                || self.origins.get(key).is_some_and(|origin| origin != key)
                || self.previous.contains_key(key)
                || !mirrored(drawing.folder_sync_mode(), layout_wide)
            {
                continue;
            }
            let kept: Vec<Key> = self
                .aside
                .remove(&(key.0, symbol.clone(), key.1))
                .map(|(_, partners)| partners)
                .unwrap_or_default()
                .into_iter()
                .filter(|&(slot, _)| {
                    slot < count
                        && slot != key.0
                        && groups[slot] == groups[key.0]
                        && charts[slot].symbol() == symbol
                })
                .collect();
            // A drawing it is linked to and that is deleted in this frame takes it along.
            if let Some(&gone) = kept.iter().find(|&&partner| {
                changes
                    .iter()
                    .any(|(changed, drawing, _)| *changed == partner && drawing.is_none())
            }) {
                self.origins.insert(*key, self.origin_of(gone));
                joined.insert(*key);
                continue;
            }
            let kept = kept
                .into_iter()
                .filter(|&(slot, id)| charts[slot].drawings().drawings.iter().any(|d| d.id == id));
            let mut linked = self.join(charts, *key, kept.collect());
            if linked.is_empty() {
                let equal = self.equal_drawings(charts, count, groups, *key, drawing, layout_wide);
                linked = self.join(charts, *key, equal);
            }
            if linked.is_empty() {
                continue;
            }
            joined.insert(*key);
            joined.extend(linked);
            returned.push(*key);
        }
        // Once every drawing that came back in this frame is linked, the version of each link
        // that the layout saw change last wins, the one on show on a tie: a chart may have left
        // again after its change. It goes as one change to each chart of the symbol whose drawing
        // of the link differs from it, or that has none yet, as a new drawing's copies do.
        let mut extra = Vec::new();
        let mut done = HashSet::new();
        for key in returned {
            if done.contains(&key) {
                continue;
            }
            let symbol = charts[key.0].symbol();
            let origin = self.origin_of(key);
            let held: Vec<(usize, Option<&Drawing>)> = (0..count)
                .filter(|&slot| groups[slot] == groups[key.0] && charts[slot].symbol() == symbol)
                .map(|slot| {
                    let id = if slot == key.0 {
                        Some(key.1)
                    } else {
                        self.linked_on(charts, slot, origin)
                    };
                    let drawings = &charts[slot].drawings().drawings;
                    (slot, id.and_then(|id| drawings.iter().find(|d| d.id == id)))
                })
                .collect();
            done.extend(
                held.iter()
                    .filter_map(|&(slot, held)| held.map(|d| (slot, d.id))),
            );
            let newest = held
                .iter()
                .filter_map(|&(slot, held)| held.map(|d| ((slot, d.id), d)))
                .max_by_key(|(member, _)| (seen.get(member), self.previous.contains_key(member)));
            if let Some((winner, version)) = newest {
                let won = content(charts[winner.0], version);
                for &(slot, held) in &held {
                    if slot != winner.0 && held.is_none_or(|d| content(charts[slot], d) != won) {
                        extra.push((winner, Some(version.clone()), Some(slot)));
                    }
                }
            }
        }
        extra
    }

    /// The mirrored drawings of its symbol that other charts hold, for each chart that came to the
    /// symbol and has no copy of them: each goes to that chart alone, once. A drawing on a chart
    /// that is equal to one of them is linked to it instead.
    fn arrived_drawings(
        &mut self,
        charts: &[&mut Chart],
        count: usize,
        groups: &[Option<String>; 16],
        changes: &[Change],
        joined: &HashSet<Key>,
        layout_wide: bool,
    ) -> Vec<Change> {
        let sending: HashSet<Key> = changes
            .iter()
            .filter(|(key, ..)| !joined.contains(key))
            .map(|(key, ..)| *key)
            .collect();
        let mut arrivals = Vec::new();
        let mut slots: Vec<usize> = std::mem::take(&mut self.arriving).into_iter().collect();
        slots.sort_unstable();
        for slot in slots.into_iter().filter(|&slot| slot < count) {
            for source in (0..count).filter(|&source| {
                source != slot
                    && groups[source] == groups[slot]
                    && charts[source].symbol() == charts[slot].symbol()
            }) {
                for drawing in &charts[source].drawings().drawings {
                    let key = (source, drawing.id);
                    if drawing.pane.is_some()
                        || sending.contains(&key)
                        || !mirrored(drawing.folder_sync_mode(), layout_wide)
                        || self.linked_on(charts, slot, self.origin_of(key)).is_some()
                    {
                        continue;
                    }
                    let equal =
                        self.equal_drawings(charts, count, groups, key, drawing, layout_wide);
                    let equal: Vec<Key> =
                        equal.into_iter().filter(|other| other.0 == slot).collect();
                    if self.join(charts, key, equal).is_empty() {
                        arrivals.push((key, Some(drawing.clone()), Some(slot)));
                    }
                }
            }
        }
        arrivals
    }

    /// Takes the copies of a drawing deleted or confined to its chart out of the charts that put
    /// them aside with their symbol, so that they do not come back with the chart.
    fn delete_aside_copies(&mut self, charts: &mut [&mut Chart], count: usize, key: Key) {
        let gone: Vec<_> = self
            .aside
            .iter()
            .filter(|((_, symbol, _), (_, partners))| {
                symbol == charts[key.0].symbol() && partners.contains(&key)
            })
            .map(|(entry, _)| entry.clone())
            .collect();
        for (slot, symbol, id) in gone {
            self.aside.remove(&(slot, symbol.clone(), id));
            if slot >= count {
                continue;
            }
            // A drawing the chart stores comes back from its storage, which this does not write.
            let stores = charts[slot].keeps_stored_drawings();
            if let Some((manager, _)) = charts[slot].symbol_drawings.get_mut(&symbol)
                && manager
                    .drawings
                    .iter()
                    .any(|d| d.id == id && !(stores && d.saveable))
            {
                manager.remove_drawings(&[id]);
            }
        }
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
        changes: &[Change],
    ) -> HashSet<Key> {
        let mut joined = HashSet::new();
        for (key, drawing, _) in changes {
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
        self.sort_by_shown_names(sort, |name| name.to_owned());
    }

    /// Orders the drawings as [`Self::sort`] does, by the names `shown` gives them.
    pub(crate) fn sort_by_shown_names(
        &mut self,
        sort: DrawingSort,
        shown: impl Fn(&str) -> String,
    ) {
        self.drawings.sort_by_cached_key(|drawing| {
            let time = match sort {
                DrawingSort::Name => None,
                DrawingSort::LastModified => Some(Reverse(drawing.modified_at)),
            };
            (
                time,
                shown(drawing.name()).to_lowercase(),
                drawing.id.clone(),
            )
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

fn live_row(drawing: &Drawing) -> SymbolDrawing {
    SymbolDrawing {
        id: sync_id(drawing),
        tool_type: drawing.tool_type,
        label: drawing.label.clone(),
        modified_at: drawing.modified_at,
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
                hold(chart.symbol(), live_row(drawing), live_key(chart, drawing));
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
                    hold(symbol, live_row(drawing), live_key(chart, drawing));
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

/// Whether the chart in `slot` writes the change that the chart in `from` made to their linked
/// drawing `id`, which the chart in `slot` stores. Charts on one storage read the change, so one
/// write per storage is enough: a second would replace the first, or be refused as a conflict
/// where storage keeps revisions.
fn writes(charts: &[&mut Chart], writers: &[usize], from: usize, slot: usize, id: usize) -> bool {
    !charts[slot].shares_stored_drawings(charts[from])
        && !writers
            .iter()
            .any(|&other| charts[slot].shares_stored_drawings(charts[other]))
        && charts[slot]
            .drawings()
            .drawings
            .iter()
            .any(|d| d.id == id && d.saveable)
}

/// `drawing` as a drawing of `target`, at the times its points have on `source`. Without bars on
/// `target`, a copy that `replaces` a drawing there keeps those times, at which the chart puts its
/// points once the bars load; a new copy waits for them.
fn map_drawing(
    drawing: &Drawing,
    source: &Chart,
    target: &Chart,
    replaces: bool,
) -> Option<Drawing> {
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
            target.drawing_point_at(time, point.price).or_else(|| {
                replaces.then_some(crate::drawings::ChartPoint {
                    time_anchor: Some((time, point.bar_idx)),
                    ..*point
                })
            })
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

    /// A symbol-wide deletion stays one edit when a clone replaces its layout.
    #[test]
    fn drawing_deletion_before_a_layout_clone_reverses_on_every_chart() {
        for slot in [0, 1] {
            let mut charts = [bars(60), bars(60)];
            for chart in &mut charts {
                chart.set_symbol("AAA");
                chart.drawings_mut().add_drawing(line("Line")).unwrap();
            }
            let mut layout = ChartLayout {
                mode: super::super::ChartLayoutMode::TwoHorizontal,
                ..Default::default()
            };
            assert!(layout.delete_symbol_drawings(&mut charts.each_mut(), "AAA"));
            layout = layout.clone();
            for redo in [false, true] {
                assert!(if redo {
                    charts[slot].redo()
                } else {
                    charts[slot].undo()
                });
                egui::Context::default()
                    .run_ui(egui::RawInput::default(), |ui| {
                        layout.show(ui, &mut charts.each_mut());
                    })
                    .drop_without_applying_deltas();
                assert_eq!(
                    charts
                        .each_ref()
                        .map(|chart| chart.drawings().drawings.len()),
                    if redo { [0, 0] } else { [1, 1] }
                );
            }
        }
    }

    /// Through the layout, without storage: a drawing and its copy on a chart of another
    /// interval are listed once, also once neither chart shows them, and deleting a symbol
    /// leaves the others, takes locked drawings along and comes back in one undo. With a storage
    /// per chart, that undo also stores the copy it brings back.
    #[test]
    fn inventory_lists_each_drawing_once_and_deletes_one_symbol_with_undo() {
        use crate::services::document::tests::Revised;
        use crate::widget::ChartLayoutMode;

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

        // Each chart with its own storage, the copy on the active chart: deleting the symbol
        // deletes the copy and leaves the original to the mirror, so the stored drawing moves to
        // the copy, and the undo there brings it back stored.
        let storages = [Arc::new(Revised::default()), Arc::new(Revised::default())];
        let charts = || {
            storages.each_ref().map(|storage| {
                let mut chart = bars(60);
                chart.set_symbol("AAA");
                chart.set_storage(storage.clone());
                chart
            })
        };
        let layout = || ChartLayout {
            mode: ChartLayoutMode::TwoHorizontal,
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let frames = |layout: &mut ChartLayout, charts: &mut [Chart; 2]| {
            for _ in 0..4 {
                ctx.run_ui(egui::RawInput::default(), |ui| {
                    layout.show(ui, &mut charts.each_mut());
                })
                .drop_without_applying_deltas();
            }
        };
        let counts = |charts: &[Chart; 2]| {
            charts
                .each_ref()
                .map(|chart| chart.drawings().drawings.len())
        };
        let top = ["Top".to_owned()];
        let mut session = layout();
        let mut shown = charts();
        frames(&mut session, &mut shown);
        shown[1].user_action(|chart| {
            let id = chart.drawings_mut().add_drawing(line("Drawn")).unwrap();
            assert!(chart.drawings_mut().move_to_folder(&[id], &[], &top));
            assert!(
                chart
                    .drawings_mut()
                    .set_folder_sync(&top, Some(FolderSync::Layout))
            );
        });
        frames(&mut session, &mut shown);
        assert_eq!(counts(&shown), [1, 1], "mirrored");
        assert!(session.delete_symbol_drawings(&mut shown.each_mut(), "AAA"));
        frames(&mut session, &mut shown);
        assert_eq!(counts(&shown), [0, 0], "deleted");
        assert!(shown[0].undo());
        frames(&mut session, &mut shown);
        let mut restarted = charts();
        frames(&mut layout(), &mut restarted);
        assert_eq!(
            (counts(&shown), counts(&restarted)),
            ([1, 1], [1, 1]),
            "after the undo and after a restart"
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
        let mapped = map_drawing(drawing, &source, &target, false).unwrap();
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

    /// Each chart keeps its drawings in its own storage, or both in one. A drawing in a folder
    /// mirrored across the layout is changed on the chart that shows its copy, by an edit, a drag,
    /// This chart only, a move out of the folder or a deletion, and undone and redone there, or
    /// undone on the chart it was drawn on. After each, a restart shows what the charts showed,
    /// with or without a later edit on the first chart: the drawing is stored on one chart, never
    /// on none or both. A drag is written once, when it ends.
    #[test]
    fn changes_to_a_mirrored_copy_survive_a_restart() {
        use crate::data::Reply;
        use crate::services::{Storage, StoreKind};
        use crate::widget::{ChartLayout, ChartLayoutMode};
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::sync::{Arc, Mutex};

        /// Documents held in memory, answered inline, and the number of writes.
        #[derive(Default)]
        struct InMemory(Mutex<HashMap<(StoreKind, String), String>>, AtomicUsize);
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
                self.1.fetch_add(1, Ordering::Relaxed);
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
        /// One change on a chart's first drawing, as one user action, or an undo or redo there.
        #[derive(Clone, Copy)]
        enum Step {
            Keep,
            ToRoot,
            Group,
            Delete,
            Edit,
            HeldEdit,
            File,
            Undo,
            Redo,
        }
        use Step::*;
        type Shown<'a> = [&'a [(&'a str, f64)]; 2];
        type Row<'a> = (&'a str, &'a [(usize, Step)], Shown<'a>, Shown<'a>);
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
        let frame =
            |input: egui::RawInput, layout: &mut ChartLayout, a: &mut Chart, b: &mut Chart| {
                ctx.run_ui(input, |ui| {
                    layout.show(ui, &mut [&mut *a, &mut *b]);
                })
                .drop_without_applying_deltas();
            };
        let frames = |layout: &mut ChartLayout, a: &mut Chart, b: &mut Chart| {
            for _ in 0..4 {
                frame(egui::RawInput::default(), layout, a, b);
            }
        };
        // Outside every chart, so that no chart reacts to the button.
        let pointer = |pressed: bool| egui::RawInput {
            events: vec![egui::Event::PointerButton {
                pos: egui::pos2(-5.0, -5.0),
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            }],
            ..Default::default()
        };
        let layout = || ChartLayout {
            mode: ChartLayoutMode::TwoHorizontal,
            ..Default::default()
        };
        let top = ["Top".to_owned()];
        // Folder and first price of each drawing per chart, without the later edit's line.
        let lines = |a: &Chart, b: &Chart| {
            [a, b].map(|chart| {
                chart
                    .drawings()
                    .drawings
                    .iter()
                    .map(|d| (d.folder.join("/"), d.chart_points[0].price))
                    .filter(|(_, price)| *price != 20.0)
                    .collect::<Vec<_>>()
            })
        };
        let first = |chart: &Chart| chart.drawings().drawings[0].id;
        let edit = |chart: &mut Chart, price: f64| {
            let id = first(chart);
            chart.user_action(|chart| {
                chart.drawings_mut().edit_drawing(id, |drawing| {
                    drawing.chart_points[0].price = price;
                    drawing.chart_points[1].price = price + 2.0;
                });
            });
        };
        let (a_side, b_side) = (0, 1);
        let (none, top_10, top_50): (&[_], &[_], &[_]) = (&[], &[("Top", 10.0)], &[("Top", 50.0)]);
        let (root_10, new_10): (&[_], &[_]) = (&[("", 10.0)], &[("New", 10.0)]);
        // Per chart, with a storage for each chart and with one for both, worked out by hand: a
        // chart that keeps a copy stores the drawing alone, and charts on one storage read it.
        let rows: &[Row] = &[
            ("keep", &[(b_side, Keep)], [none, top_10], [top_10, top_10]),
            (
                "keep, undo",
                &[(b_side, Keep), (b_side, Undo)],
                [top_10, top_10],
                [top_10, top_10],
            ),
            (
                "root, undo",
                &[(b_side, ToRoot), (b_side, Undo)],
                [top_10, top_10],
                [top_10, top_10],
            ),
            (
                "group, undo",
                &[(b_side, Group), (b_side, Undo)],
                [top_10, top_10],
                [top_10, top_10],
            ),
            (
                "delete, undo",
                &[(b_side, Delete), (b_side, Undo)],
                [top_10, top_10],
                [top_10, top_10],
            ),
            (
                "file on A, keep, undo on A",
                &[(a_side, File), (b_side, Keep), (a_side, Undo)],
                [top_10, top_10],
                [&[("Top/Sub", 10.0)], &[("Top/Sub", 10.0)]],
            ),
            (
                "keep, undo, redo",
                &[(b_side, Keep), (b_side, Undo), (b_side, Redo)],
                [none, top_10],
                [top_10, top_10],
            ),
            (
                "root, undo, redo",
                &[(b_side, ToRoot), (b_side, Undo), (b_side, Redo)],
                [none, root_10],
                [root_10, root_10],
            ),
            (
                "group, undo, redo",
                &[(b_side, Group), (b_side, Undo), (b_side, Redo)],
                [none, new_10],
                [new_10, new_10],
            ),
            (
                "delete, undo, redo",
                &[(b_side, Delete), (b_side, Undo), (b_side, Redo)],
                [none, none],
                [none, none],
            ),
            (
                "edit",
                &[(b_side, Edit)],
                [top_50, top_50],
                [top_50, top_50],
            ),
            (
                "edit, undo",
                &[(b_side, Edit), (b_side, Undo)],
                [top_10, top_10],
                [top_10, top_10],
            ),
            (
                "edit while the pointer is held",
                &[(b_side, HeldEdit)],
                [top_50, top_50],
                [top_50, top_50],
            ),
        ];
        let mut failures = Vec::new();
        for (name, steps, own, one) in rows {
            for (shared, expected) in [(false, own), (true, one)] {
                for edit_first in [false, true] {
                    let storages = {
                        let storage = Arc::new(InMemory::default());
                        let other = if shared {
                            storage.clone()
                        } else {
                            Arc::new(InMemory::default())
                        };
                        [storage, other]
                    };
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
                    let mirrored = vec![("Top".to_owned(), 10.0)];
                    assert_eq!(
                        lines(&a, &b),
                        [mirrored.clone(), mirrored],
                        "{name}, set up"
                    );
                    for &(side, step) in *steps {
                        let chart = if side == a_side { &mut a } else { &mut b };
                        let id = chart.drawings().drawings.first().map(|d| d.id);
                        let filed = |folder: &[&str]| -> Vec<String> {
                            folder.iter().map(|name| (*name).to_owned()).collect()
                        };
                        match step {
                            Keep => chart.user_action(|chart| {
                                chart
                                    .drawings_mut()
                                    .set_folder_sync(&top, Some(FolderSync::Chart));
                            }),
                            ToRoot => chart.user_action(|chart| {
                                chart
                                    .drawings_mut()
                                    .move_to_folder(&[id.unwrap()], &[], &[]);
                            }),
                            Group => chart.user_action(|chart| {
                                chart.drawings_mut().move_to_folder(
                                    &[id.unwrap()],
                                    &[],
                                    &filed(&["New"]),
                                );
                            }),
                            File => chart.user_action(|chart| {
                                chart.drawings_mut().move_to_folder(
                                    &[id.unwrap()],
                                    &[],
                                    &filed(&["Top", "Sub"]),
                                );
                            }),
                            Delete => chart.user_action(|chart| {
                                chart.drawings_mut().delete_drawing(id.unwrap());
                            }),
                            Edit => edit(chart, 50.0),
                            HeldEdit => {
                                frame(pointer(true), &mut session, &mut a, &mut b);
                                let writes = storages[0].1.load(Ordering::Relaxed);
                                // A drag changes the drawing on several frames of one gesture.
                                for price in [40.0, 50.0] {
                                    edit(if side == a_side { &mut a } else { &mut b }, price);
                                    frames(&mut session, &mut a, &mut b);
                                }
                                assert!(ctx.input(|i| i.pointer.any_down()), "{name}");
                                frame(pointer(false), &mut session, &mut a, &mut b);
                                frames(&mut session, &mut a, &mut b);
                                // With its own storage, the first chart writes the change once,
                                // when the gesture ends, as it writes a gesture of its own.
                                let written = storages[0].1.load(Ordering::Relaxed) - writes;
                                if !shared && written != 1 {
                                    failures.push(format!(
                                        "{name}, edit_first {edit_first}: the first chart wrote \
                                         its storage {written} times during the gesture"
                                    ));
                                }
                            }
                            // A chart that read a storage has nothing to undo, as a row expects.
                            Undo => {
                                chart.undo();
                            }
                            Redo => {
                                chart.redo();
                            }
                        }
                        frames(&mut session, &mut a, &mut b);
                    }
                    if edit_first {
                        a.user_action(|chart| {
                            chart.drawings_mut().add_drawing(line(20.0)).unwrap();
                        });
                        frames(&mut session, &mut a, &mut b);
                    }
                    let shown = lines(&a, &b);
                    let mut restarted = layout();
                    let (mut a, mut b) = (chart(&storages[0]), chart(&storages[1]));
                    frames(&mut restarted, &mut a, &mut b);
                    let restored = lines(&a, &b);
                    let expected = expected.map(|chart| {
                        chart
                            .iter()
                            .map(|(folder, price)| ((*folder).to_owned(), *price))
                            .collect::<Vec<_>>()
                    });
                    if shown != expected || restored != expected {
                        failures.push(format!(
                            "{name}, shared {shared}, edit_first {edit_first}: shown {shown:?}, \
                             restored {restored:?}, expected {expected:?}"
                        ));
                    }
                }
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// Two charts of one symbol in a layout with drawing sync on, each holding a line at 104.
    fn mirrored_pair(sync_symbol: bool) -> (ChartLayout, [Chart; 2]) {
        let mut charts = [bars(60), bars(60)];
        for chart in &mut charts {
            chart.set_symbol("SAMPLE");
        }
        let mut layout = ChartLayout {
            mode: super::super::ChartLayoutMode::TwoHorizontal,
            ..Default::default()
        };
        layout.sync.sync_drawings = true;
        layout.sync.sync_symbol = sync_symbol;
        draw(&mut charts[0], 104.0);
        let mut pair = (layout, charts);
        frames(&mut pair.0, &mut pair.1);
        assert_eq!(prices(&pair.1), [vec![104.0], vec![104.0]]);
        pair
    }

    /// Adds a horizontal trend line at `price` and returns its id.
    fn draw(chart: &mut Chart, price: f64) -> usize {
        let mut drawing = line("");
        drawing.chart_points = vec![ChartPoint::new(2.0, price), ChartPoint::new(4.0, price)];
        chart.drawings_mut().add_drawing(drawing).unwrap()
    }

    fn frames<const N: usize>(layout: &mut ChartLayout, charts: &mut [Chart; N]) {
        let ctx = egui::Context::default();
        for _ in 0..4 {
            ctx.run_ui(egui::RawInput::default(), |ui| {
                layout.show(ui, &mut charts.each_mut());
            })
            .drop_without_applying_deltas();
        }
    }

    /// The first price of each drawing of each chart, in order.
    fn prices<const N: usize>(charts: &[Chart; N]) -> [Vec<f64>; N] {
        charts.each_ref().map(|chart| {
            let mut prices: Vec<f64> = chart
                .drawings()
                .drawings
                .iter()
                .map(|d| d.chart_points[0].price)
                .collect();
            prices.sort_by(f64::total_cmp);
            prices
        })
    }

    fn move_first(chart: &mut Chart, price: f64) {
        let id = chart.drawings().drawings[0].id;
        chart.drawings_mut().edit_drawing(id, |drawing| {
            drawing.chart_points[0].price = price;
            drawing.chart_points[1].price = price;
        });
    }

    /// Puts the first drawing of `chart` in a folder synchronized as `sync`.
    fn into_folder(chart: &mut Chart, sync: FolderSync) {
        let folder = ["Folder".to_owned()];
        let id = chart.drawings().drawings[0].id;
        assert!(chart.drawings_mut().move_to_folder(&[id], &[], &folder));
        assert!(chart.drawings_mut().set_folder_sync(&folder, Some(sync)));
    }

    /// Three charts on `symbols` in a layout with drawing sync on, the first holding a line at
    /// 104 that the others of its symbol hold too.
    fn three_charts(symbols: [&str; 3]) -> (ChartLayout, [Chart; 3]) {
        let mut charts = [bars(60), bars(60), bars(60)];
        for (chart, symbol) in charts.iter_mut().zip(symbols) {
            chart.set_symbol(symbol);
        }
        let mut layout = ChartLayout {
            mode: super::super::ChartLayoutMode::ThreeHorizontal,
            ..Default::default()
        };
        layout.sync.sync_drawings = true;
        draw(&mut charts[0], 104.0);
        frames(&mut layout, &mut charts);
        (layout, charts)
    }

    /// A chart that leaves a symbol and returns takes its mirrored drawings up again: each chart
    /// holds one line however often it happens, also when the chart that stayed changed the line
    /// meanwhile. A line deleted on another chart of the symbol, also in the frame of the return,
    /// or confined to that chart by its folder, does not come back, and a chart that forgot the
    /// drawings of the symbol draws lines of its own there.
    #[test]
    fn mirrored_drawings_stay_single_through_a_symbol_round_trip() {
        for sync_symbol in [true, false] {
            let (mut layout, mut charts) = mirrored_pair(sync_symbol);
            // With the symbol synced the active chart takes the second along, otherwise only the
            // second leaves.
            let leaver = usize::from(!sync_symbol);
            for trip in 1..=3 {
                charts[leaver].set_symbol("OTHER");
                if !sync_symbol {
                    // The first chart edits the line while the second is away.
                    move_first(&mut charts[0], 104.0 - f64::from(trip));
                }
                frames(&mut layout, &mut charts);
                charts[leaver].set_symbol("SAMPLE");
                frames(&mut layout, &mut charts);
                let price = if sync_symbol {
                    104.0
                } else {
                    104.0 - f64::from(trip)
                };
                assert_eq!(
                    prices(&charts),
                    [vec![price], vec![price]],
                    "sync_symbol {sync_symbol}, round trip {trip}"
                );
            }
        }
        let delete: fn(&mut Chart) = |chart| {
            let id = chart.drawings().drawings[0].id;
            chart.drawings_mut().delete_drawing(id);
        };
        let confine: fn(&mut Chart) = |chart| into_folder(chart, FolderSync::Chart);
        for (name, edit, kept, at_return) in [
            ("deleted while away", delete, vec![], false),
            (
                "confined to its chart while away",
                confine,
                vec![104.0],
                false,
            ),
            ("deleted in the frame of the return", delete, vec![], true),
        ] {
            let (mut layout, mut charts) = mirrored_pair(false);
            charts[1].set_symbol("OTHER");
            frames(&mut layout, &mut charts);
            edit(&mut charts[0]);
            if !at_return {
                frames(&mut layout, &mut charts);
            }
            charts[1].set_symbol("SAMPLE");
            frames(&mut layout, &mut charts);
            assert_eq!(prices(&charts), [kept, vec![]], "{name}");
        }

        // Ids belong to a symbol's drawings: a drawing made on the other symbol meanwhile may
        // have the id of the copy, and the link of the copy still follows its line.
        let (mut layout, mut charts) = mirrored_pair(false);
        charts[1].set_symbol("OTHER");
        frames(&mut layout, &mut charts);
        draw(&mut charts[1], 70.0);
        frames(&mut layout, &mut charts);
        charts[1].set_symbol("SAMPLE");
        frames(&mut layout, &mut charts);
        move_first(&mut charts[0], 95.0);
        frames(&mut layout, &mut charts);
        assert_eq!(
            prices(&charts),
            [vec![95.0], vec![95.0]],
            "after the return"
        );
        let id = charts[0].drawings().drawings[0].id;
        charts[0].drawings_mut().delete_drawing(id);
        frames(&mut layout, &mut charts);
        charts[1].set_symbol("OTHER");
        frames(&mut layout, &mut charts);
        assert_eq!(prices(&charts), [vec![], vec![70.0]], "other symbol kept");

        // Each chart draws a line on the other symbol while the other chart shows the first, each
        // under the id of its line on the first symbol: they stay two lines, each once on each
        // chart.
        let (mut layout, mut charts) = mirrored_pair(false);
        charts[0].set_symbol("OTHER");
        frames(&mut layout, &mut charts);
        draw(&mut charts[0], 70.0);
        frames(&mut layout, &mut charts);
        charts[0].set_symbol("SAMPLE");
        frames(&mut layout, &mut charts);
        charts[1].set_symbol("OTHER");
        frames(&mut layout, &mut charts);
        draw(&mut charts[1], 60.0);
        frames(&mut layout, &mut charts);
        charts[0].set_symbol("OTHER");
        frames(&mut layout, &mut charts);
        assert_eq!(
            prices(&charts),
            [vec![60.0, 70.0], vec![60.0, 70.0]],
            "two lines on the other symbol"
        );

        // A chart that forgot the symbol while away draws a line there under the id of the line it
        // had: the line of the other chart stays a line of its own.
        let (mut layout, mut charts) = mirrored_pair(false);
        let old = charts[0].drawings().drawings[0].id;
        charts[1].set_symbol("OTHER");
        frames(&mut layout, &mut charts);
        charts[0].set_symbol("OTHER");
        frames(&mut layout, &mut charts);
        assert!(charts[0].forget_symbol("SAMPLE"));
        frames(&mut layout, &mut charts);
        charts[0].set_symbol("SAMPLE");
        frames(&mut layout, &mut charts);
        assert_eq!(draw(&mut charts[0], 70.0), old, "the id comes back");
        frames(&mut layout, &mut charts);
        charts[1].set_symbol("SAMPLE");
        frames(&mut layout, &mut charts);
        assert_eq!(
            prices(&charts),
            [vec![70.0, 104.0], vec![70.0, 104.0]],
            "symbol forgotten"
        );
        // Also while the other chart shows the symbol: the new line takes the id of a line whose
        // copy the other chart still shows, and that copy stays.
        let (mut layout, mut charts) = mirrored_pair(false);
        let kept = draw(&mut charts[0], 70.0);
        frames(&mut layout, &mut charts);
        let id = charts[0].drawings().drawings[0].id;
        charts[0].drawings_mut().delete_drawing(id);
        frames(&mut layout, &mut charts);
        charts[0].set_symbol("OTHER");
        frames(&mut layout, &mut charts);
        assert!(charts[0].forget_symbol("SAMPLE"));
        frames(&mut layout, &mut charts);
        charts[0].set_symbol("SAMPLE");
        frames(&mut layout, &mut charts);
        assert_eq!(draw(&mut charts[0], 60.0), kept, "the id comes back");
        frames(&mut layout, &mut charts);
        assert_eq!(
            prices(&charts),
            [vec![60.0, 70.0], vec![60.0, 70.0]],
            "symbol forgotten, other chart on it"
        );

        // Three charts: the third comes to the symbol after the second left, the first stays or
        // leaves too, and the copy of the third is deleted or moved. The charts that were away
        // keep the line from coming back and take the move.
        let lower: fn(&mut Chart) = |chart| move_first(chart, 95.0);
        for (name, away, edit, line) in [
            ("deleted through a later copy", false, delete, vec![]),
            ("deleted with the first away", true, delete, vec![]),
            ("moved with the first away", true, lower, vec![95.0]),
        ] {
            let (mut layout, mut charts) = three_charts(["SAMPLE", "SAMPLE", "OTHER"]);
            charts[1].set_symbol("OTHER");
            frames(&mut layout, &mut charts);
            charts[2].set_symbol("SAMPLE");
            frames(&mut layout, &mut charts);
            assert_eq!(
                prices(&charts),
                [vec![104.0], vec![], vec![104.0]],
                "{name}: copy on arrival"
            );
            if away {
                charts[0].set_symbol("THIRD");
                frames(&mut layout, &mut charts);
            }
            edit(&mut charts[2]);
            frames(&mut layout, &mut charts);
            for slot in [1, 0] {
                charts[slot].set_symbol("SAMPLE");
                frames(&mut layout, &mut charts);
            }
            assert_eq!(
                prices(&charts),
                [line.clone(), line.clone(), line],
                "{name}"
            );
        }

        // A line deleted on another symbol under the key of the line here leaves the copy that the
        // second chart put aside.
        let (mut layout, mut charts) = mirrored_pair(false);
        charts[1].set_symbol("OTHER");
        frames(&mut layout, &mut charts);
        charts[0].set_symbol("THIRD");
        frames(&mut layout, &mut charts);
        let id = draw(&mut charts[0], 70.0);
        frames(&mut layout, &mut charts);
        charts[0].drawings_mut().delete_drawing(id);
        frames(&mut layout, &mut charts);
        charts[1].set_symbol("SAMPLE");
        frames(&mut layout, &mut charts);
        assert_eq!(
            prices(&charts),
            [Vec::<f64>::new(), vec![104.0]],
            "deleted on another symbol"
        );
    }

    /// A chart that comes back to a symbol takes the last change made to its mirrored line while
    /// it was away: by the chart that stayed, by one that came back and left again or by one that
    /// came to the symbol meanwhile, also when the change is an undo or was made in the frame in
    /// which its chart left, when the returning chart still waits for its bars, and when several
    /// charts come back in one frame.
    #[test]
    fn mirrored_drawings_take_the_last_change_on_return() {
        // A move made on the chart that leaves, in the frame it leaves in, is the last change.
        let (mut layout, mut charts) = mirrored_pair(false);
        move_first(&mut charts[1], 95.0);
        charts[1].set_symbol("OTHER");
        frames(&mut layout, &mut charts);
        charts[1].set_symbol("SAMPLE");
        frames(&mut layout, &mut charts);
        assert_eq!(
            prices(&charts),
            [vec![95.0], vec![95.0]],
            "moved in the frame it left"
        );
        // A move undone while the other chart is away, on either chart, is the last change, also
        // when the undo gives the line back the edit time it had before the move.
        for undo_on in [0, 1] {
            let (mut layout, mut charts) = mirrored_pair(false);
            move_first(&mut charts[0], 95.0);
            frames(&mut layout, &mut charts);
            let away = 1 - undo_on;
            charts[away].set_symbol("OTHER");
            frames(&mut layout, &mut charts);
            assert!(
                charts[undo_on].drawings_mut().undo(),
                "undo on chart {undo_on}"
            );
            frames(&mut layout, &mut charts);
            assert_eq!(
                prices(&charts)[undo_on],
                vec![104.0],
                "undone on chart {undo_on}"
            );
            charts[away].set_symbol("SAMPLE");
            frames(&mut layout, &mut charts);
            assert_eq!(
                prices(&charts),
                [vec![104.0], vec![104.0]],
                "move undone on chart {undo_on} while the other was away"
            );
        }

        // Both charts away: the one that comes back first changes the line, and the other takes
        // that version when it comes back, whichever left first, also when the first left again
        // before the other came back, and for a line its folder mirrors with the switch off.
        for first in [0, 1] {
            for (again, by_folder) in [(false, false), (true, false), (false, true)] {
                let (mut layout, mut charts) = mirrored_pair(false);
                if by_folder {
                    into_folder(&mut charts[0], FolderSync::Layout);
                    layout.sync.sync_drawings = false;
                    frames(&mut layout, &mut charts);
                }
                charts[first].set_symbol("OTHER");
                frames(&mut layout, &mut charts);
                charts[1 - first].set_symbol("OTHER");
                frames(&mut layout, &mut charts);
                charts[first].set_symbol("SAMPLE");
                frames(&mut layout, &mut charts);
                move_first(&mut charts[first], 95.0);
                frames(&mut layout, &mut charts);
                if again {
                    charts[first].set_symbol("OTHER");
                    frames(&mut layout, &mut charts);
                }
                charts[1 - first].set_symbol("SAMPLE");
                frames(&mut layout, &mut charts);
                if again {
                    charts[first].set_symbol("SAMPLE");
                    frames(&mut layout, &mut charts);
                }
                assert_eq!(
                    prices(&charts),
                    [vec![95.0], vec![95.0]],
                    "both away, chart {first} first, again {again}, by folder {by_folder}"
                );
            }
        }
        // A chart that came to the symbol while the line was moved away from it takes the move.
        let (mut layout, mut charts) = three_charts(["SAMPLE", "SAMPLE", "OTHER"]);
        for chart in &mut charts[..2] {
            chart.set_symbol("OTHER");
        }
        frames(&mut layout, &mut charts);
        charts[0].set_symbol("SAMPLE");
        frames(&mut layout, &mut charts);
        move_first(&mut charts[0], 95.0);
        frames(&mut layout, &mut charts);
        charts[0].set_symbol("OTHER");
        frames(&mut layout, &mut charts);
        for chart in &mut charts[1..] {
            chart.set_symbol("SAMPLE");
        }
        frames(&mut layout, &mut charts);
        charts[0].set_symbol("SAMPLE");
        frames(&mut layout, &mut charts);
        assert_eq!(
            prices(&charts),
            [vec![95.0], vec![95.0], vec![95.0]],
            "a chart came meanwhile"
        );
        // Three charts leave in turn, the second and the third each moving the line before it
        // leaves, and come back in the same order: each return takes the last move, also over
        // the copy that the layout wrote to the first chart when the second came back.
        let (mut layout, mut charts) = three_charts(["SAMPLE"; 3]);
        charts[0].set_symbol("OTHER");
        frames(&mut layout, &mut charts);
        for (slot, price) in [(1, 95.0), (2, 80.0)] {
            move_first(&mut charts[slot], price);
            frames(&mut layout, &mut charts);
            charts[slot].set_symbol("OTHER");
            frames(&mut layout, &mut charts);
        }
        for (slot, expected) in [
            (0, [vec![104.0], vec![], vec![]]),
            (1, [vec![95.0], vec![95.0], vec![]]),
            (2, [vec![80.0], vec![80.0], vec![80.0]]),
        ] {
            charts[slot].set_symbol("SAMPLE");
            frames(&mut layout, &mut charts);
            assert_eq!(
                prices(&charts),
                expected,
                "chart {slot} back after two moves"
            );
        }

        // The chart that comes back waits for its bars, as with a feed, and the chart that moved
        // the line leaves meanwhile: the first takes the move once its bars are in, and the other
        // finds the line moved when it returns.
        let (mut layout, mut charts) = mirrored_pair(false);
        charts[1].set_symbol("OTHER");
        frames(&mut layout, &mut charts);
        move_first(&mut charts[0], 95.0);
        frames(&mut layout, &mut charts);
        let data = charts[1].bars(SeriesId::MAIN).to_vec();
        charts[1].set_symbol("SAMPLE");
        charts[1].set_data(SeriesId::MAIN, Vec::new());
        frames(&mut layout, &mut charts);
        charts[0].set_symbol("OTHER");
        frames(&mut layout, &mut charts);
        charts[1].set_data(SeriesId::MAIN, data);
        frames(&mut layout, &mut charts);
        assert_eq!(prices(&charts), [vec![], vec![95.0]], "bars in, mover away");
        charts[0].set_symbol("SAMPLE");
        frames(&mut layout, &mut charts);
        assert_eq!(prices(&charts), [vec![95.0], vec![95.0]], "mover back");

        // The second of three charts comes back to the line the first moved, which the third took,
        // with its bars in place, or waiting for them while the first leaves. The second takes the
        // move, and the third gets no edit for it.
        for later in [false, true] {
            let (mut layout, mut charts) = three_charts(["SAMPLE", "SAMPLE", "OTHER"]);
            charts[1].set_symbol("OTHER");
            frames(&mut layout, &mut charts);
            charts[2].set_symbol("SAMPLE");
            frames(&mut layout, &mut charts);
            move_first(&mut charts[0], 95.0);
            frames(&mut layout, &mut charts);
            let data = charts[1].bars(SeriesId::MAIN).to_vec();
            charts[1].set_symbol("SAMPLE");
            if later {
                charts[1].set_data(SeriesId::MAIN, Vec::new());
                frames(&mut layout, &mut charts);
                charts[0].set_symbol("OTHER");
                frames(&mut layout, &mut charts);
                charts[1].set_data(SeriesId::MAIN, data);
            }
            frames(&mut layout, &mut charts);
            let first = if later { vec![] } else { vec![95.0] };
            assert_eq!(
                prices(&charts),
                [first, vec![95.0], vec![95.0]],
                "bars later {later}"
            );
            // The copy it received on its arrival, and the move.
            let mut edits = 0;
            while charts[2].drawings_mut().undo() {
                edits += 1;
            }
            assert_eq!(edits, 2, "edits of the third chart, bars later {later}");
        }

        // Two charts that left one after the other come back in one frame while the third, which
        // stayed, moved the line: both take the move into the link of the third.
        let (mut layout, mut charts) = three_charts(["SAMPLE"; 3]);
        assert_eq!(
            prices(&charts),
            [vec![104.0], vec![104.0], vec![104.0]],
            "mirrored"
        );
        charts[1].set_symbol("OTHER");
        frames(&mut layout, &mut charts);
        charts[0].set_symbol("OTHER");
        frames(&mut layout, &mut charts);
        move_first(&mut charts[2], 95.0);
        frames(&mut layout, &mut charts);
        for chart in &mut charts[..2] {
            chart.set_symbol("SAMPLE");
        }
        frames(&mut layout, &mut charts);
        assert_eq!(
            prices(&charts),
            [vec![95.0], vec![95.0], vec![95.0]],
            "back in one frame"
        );
        // The third chart comes to the symbol after the second left, and moves the line in the
        // frame in which it leaves again. Both come back in one frame, neither knowing the copy of
        // the other: all three take the move.
        let (mut layout, mut charts) = three_charts(["SAMPLE", "SAMPLE", "OTHER"]);
        charts[1].set_symbol("OTHER");
        frames(&mut layout, &mut charts);
        charts[2].set_symbol("SAMPLE");
        frames(&mut layout, &mut charts);
        move_first(&mut charts[2], 95.0);
        charts[2].set_symbol("OTHER");
        frames(&mut layout, &mut charts);
        for chart in &mut charts[1..] {
            chart.set_symbol("SAMPLE");
        }
        frames(&mut layout, &mut charts);
        assert_eq!(
            prices(&charts),
            [vec![95.0], vec![95.0], vec![95.0]],
            "moved in the frame it left, back in one frame"
        );
    }

    /// A chart that comes to a symbol receives the mirrored drawings its group holds for it, those
    /// made while it showed another symbol too, once its bars are in place, and not the ones local
    /// to their chart.
    #[test]
    fn chart_arriving_at_a_symbol_receives_its_mirrored_drawings() {
        let (mut layout, mut charts) = mirrored_pair(false);
        charts[1].set_symbol("OTHER");
        frames(&mut layout, &mut charts);
        assert_eq!(
            prices(&charts),
            [vec![104.0], vec![]],
            "second chart on OTHER"
        );
        // Made in its absence, one mirrored and one local to its folder.
        let id = draw(&mut charts[0], 90.0);
        let local = draw(&mut charts[0], 80.0);
        let folder = ["Local".to_owned()];
        assert!(
            charts[0]
                .drawings_mut()
                .move_to_folder(&[local], &[], &folder)
        );
        assert!(
            charts[0]
                .drawings_mut()
                .set_folder_sync(&folder, Some(FolderSync::Chart))
        );
        assert_ne!(id, local);
        frames(&mut layout, &mut charts);
        // As with a feed, the bars go with the symbol change until those of the new one load.
        let data = charts[1].bars(SeriesId::MAIN).to_vec();
        charts[1].set_symbol("SAMPLE");
        charts[1].set_data(SeriesId::MAIN, Vec::new());
        frames(&mut layout, &mut charts);
        assert_eq!(prices(&charts)[1], vec![104.0], "before its bars");
        charts[1].set_data(SeriesId::MAIN, data);
        frames(&mut layout, &mut charts);
        assert_eq!(
            prices(&charts),
            [vec![80.0, 90.0, 104.0], vec![90.0, 104.0]],
            "second chart back on SAMPLE"
        );
    }

    /// What was done to the drawings of a chart while the switch was off stays on that chart when
    /// it is turned on, and the charts end with the same drawings.
    #[test]
    fn turning_drawing_sync_on_keeps_changes_made_while_off() {
        type Edit = fn(&mut [Chart; 2]);
        let rows: [(&str, Edit, [Vec<f64>; 2]); 3] = [
            (
                "copy deleted",
                |charts| {
                    let id = charts[1].drawings().drawings[0].id;
                    charts[1].drawings_mut().delete_drawing(id);
                },
                [vec![104.0], vec![104.0]],
            ),
            (
                "copy moved",
                |charts| move_first(&mut charts[1], 95.0),
                [vec![95.0, 104.0], vec![95.0, 104.0]],
            ),
            (
                "both moved",
                |charts| {
                    move_first(&mut charts[0], 108.0);
                    move_first(&mut charts[1], 95.0);
                },
                [vec![95.0, 108.0], vec![95.0, 108.0]],
            ),
        ];
        for (name, edit, expected) in rows {
            let (mut layout, mut charts) = mirrored_pair(false);
            layout.sync.sync_drawings = false;
            frames(&mut layout, &mut charts);
            edit(&mut charts);
            frames(&mut layout, &mut charts);
            layout.sync.sync_drawings = true;
            frames(&mut layout, &mut charts);
            assert_eq!(prices(&charts), expected, "{name}");
        }

        // A line its folder mirrors stays linked when the switch is turned on: the chart that was
        // away takes the move made meanwhile.
        let (mut layout, mut charts) = mirrored_pair(false);
        into_folder(&mut charts[0], FolderSync::Layout);
        layout.sync.sync_drawings = false;
        frames(&mut layout, &mut charts);
        charts[1].set_symbol("OTHER");
        frames(&mut layout, &mut charts);
        move_first(&mut charts[0], 95.0);
        frames(&mut layout, &mut charts);
        layout.sync.sync_drawings = true;
        frames(&mut layout, &mut charts);
        charts[1].set_symbol("SAMPLE");
        frames(&mut layout, &mut charts);
        assert_eq!(
            prices(&charts),
            [vec![95.0], vec![95.0]],
            "mirrored by its folder"
        );
    }

    /// Charts saved as snapshots and restored into a new layout show each mirrored drawing once,
    /// still linked, however often that happens.
    #[cfg(feature = "ui")]
    #[test]
    fn mirrored_drawings_restored_from_snapshots_stay_single_and_linked() {
        let (mut layout, mut charts) = mirrored_pair(false);
        for restart in 1..=3 {
            let saved = charts.each_ref().map(|chart| chart.snapshot().unwrap());
            charts = [bars(60), bars(60)];
            for (chart, saved) in charts.iter_mut().zip(&saved) {
                chart.set_symbol("SAMPLE");
                chart.restore(saved).unwrap();
            }
            layout = ChartLayout {
                mode: super::super::ChartLayoutMode::TwoHorizontal,
                ..Default::default()
            };
            layout.sync.sync_drawings = true;
            frames(&mut layout, &mut charts);
            assert_eq!(
                prices(&charts),
                [vec![104.0], vec![104.0]],
                "restart {restart}"
            );
        }
        move_first(&mut charts[0], 95.0);
        frames(&mut layout, &mut charts);
        assert_eq!(prices(&charts), [vec![95.0], vec![95.0]], "still linked");

        // One chart restored from a snapshot of the symbol holding another line, whether it
        // shows the symbol or is away from it, and whether it drew the line here or holds its
        // copy: both lines end on both charts.
        let saved = {
            let mut chart = bars(60);
            chart.set_symbol("SAMPLE");
            draw(&mut chart, 70.0);
            chart.snapshot().unwrap()
        };
        for (restored, away) in [(1, false), (1, true), (0, false), (0, true)] {
            let (mut layout, mut charts) = mirrored_pair(false);
            if away {
                charts[restored].set_symbol("OTHER");
                frames(&mut layout, &mut charts);
            }
            let data = charts[restored].bars(SeriesId::MAIN).to_vec();
            charts[restored].restore(&saved).unwrap();
            // Restoring another symbol empties the bars until the new ones load.
            charts[restored].set_data(SeriesId::MAIN, data);
            frames(&mut layout, &mut charts);
            assert_eq!(
                prices(&charts),
                [vec![70.0, 104.0], vec![70.0, 104.0]],
                "chart {restored} restored, away {away}"
            );
        }
    }

    /// Two charts keep their drawings in one storage that keeps revisions, a third in its own.
    /// Keeping or editing a mirrored drawing on the third chart writes the shared storage once,
    /// which the other chart there reads: a second write would be refused as a conflict for the
    /// user to settle, and that chart would write nothing until then.
    #[test]
    fn a_mirrored_copy_writes_a_shared_storage_once() {
        use crate::services::document::tests::Revised;
        use crate::widget::{ChartLayout, ChartLayoutMode};

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
        for edit in [false, true] {
            let (shared, own) = (Arc::new(Revised::default()), Arc::new(Revised::default()));
            let charts = || {
                [&shared, &own, &shared].map(|storage| {
                    let mut chart = bars(60);
                    chart.set_symbol("SAMPLE");
                    chart.set_storage(storage.clone());
                    chart
                })
            };
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
            let copy = shown[1].drawings().drawings[0].id;
            shown[1].user_action(|chart| {
                if edit {
                    chart.drawings_mut().edit_drawing(copy, |drawing| {
                        drawing.chart_points[0].price = 50.0;
                        drawing.chart_points[1].price = 52.0;
                    });
                } else {
                    assert!(
                        chart
                            .drawings_mut()
                            .set_folder_sync(&top, Some(FolderSync::Chart))
                    );
                }
            });
            frames(&mut session, &mut shown);
            assert_eq!(shown.each_ref().map(Chart::storage_conflict), [None; 3]);
            let mut restarted = charts();
            frames(&mut layout(), &mut restarted);
            if edit {
                assert_eq!((counts(&shown), counts(&restarted)), ([1; 3], [1; 3]));
                assert_eq!(
                    restarted
                        .each_ref()
                        .map(|chart| chart.drawings().drawings[0].chart_points[0].price),
                    [50.0; 3],
                    "the edit reached the shared storage"
                );
            } else {
                assert_eq!((counts(&shown), counts(&restarted)), ([0, 1, 0], [0, 1, 0]));
            }
        }
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
