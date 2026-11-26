//! Propagate drawing changes without consuming the host's persistence queue, and list and delete
//! the layout's drawings by symbol.

use super::{ChartLayout, MAX_CHARTS};
use crate::config::{DrawingSort, DrawingToolType};
use crate::drawings::manager::sync_id;
use crate::drawings::persistence::{DrawingStorage, StoredCoord, StoredDrawing};
use crate::drawings::{Drawing, DrawingManager, DrawingToolRegistry, FolderSync};
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
    /// Whether the chart stored the drawing (see [`Chart::stores`]).
    stored: bool,
    /// The [`DrawingLinks::frame`] in which the layout last saw the drawing change, or wrote the
    /// change of another drawing to it.
    seen: u64,
}

/// Whether a drawing with this folder synchronization is mirrored to the other charts.
fn mirrored(sync: Option<FolderSync>, layout_wide: bool) -> bool {
    match sync {
        Some(FolderSync::Chart) => false,
        Some(FolderSync::Layout | FolderSync::AllLayouts) => true,
        None => layout_wide,
    }
}

/// The drawings of `symbol` that `chart` holds: on show, or put aside with the symbol.
fn drawings_of<'a>(chart: &'a Chart, symbol: &str) -> Option<&'a DrawingManager> {
    if chart.symbol() == symbol {
        Some(chart.drawings())
    } else {
        chart
            .symbol_drawings
            .get(symbol)
            .map(|(manager, _)| manager)
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

/// Whether `drawing` changed since `old`, the layout's record of it.
fn differs(chart: &Chart, drawing: &Drawing, old: &Snapshot) -> bool {
    serde_json::to_string(drawing).is_ok_and(|style| style != old.style)
        || timed_points(chart, drawing) != old.points
        || drawing.notes != old.notes
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
    /// The `drawing_resets` of each chart at the last frame.
    resets: Vec<u64>,
    /// The `identity` of the drawings of each symbol each chart held at the last frame, on show
    /// or put aside, by slot and symbol: drawings a load or a restore put in their place are new
    /// to the layout, also under the same ids.
    managers: HashMap<(usize, String), u64>,
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
        if a.max(b) < self.resets.len() {
            self.resets.swap(a, b);
        }
        self.managers = self
            .managers
            .drain()
            .map(|((slot, symbol), identity)| ((moved((slot, 0)).0, symbol), identity))
            .collect();
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
            *slot < count
                && drawings_of(charts[*slot], symbol).is_some_and(|manager| {
                    manager
                        .drawings
                        .iter()
                        .any(|d| d.id == *id && d.folder_sync_mode().is_some())
                })
        });
    }

    /// Forgets the drawings of the chart in `slot` of each symbol that `gone` picks, which the
    /// chart no longer holds: drawings in their place are new to the layout, also under their ids.
    fn forget(&mut self, slot: usize, gone: impl Fn(&str) -> bool) {
        let keys: Vec<Key> = self
            .previous
            .iter()
            .filter(|(key, old)| key.0 == slot && gone(&old.symbol))
            .map(|(key, _)| *key)
            .collect();
        for key in &keys {
            self.previous.remove(key);
            self.origins.remove(key);
            self.retry.remove(key);
            self.unwritten.remove(key);
        }
        self.aside
            .retain(|(owner, symbol, _), _| *owner != slot || !gone(symbol));
        for ((_, symbol, _), (_, partners)) in &mut self.aside {
            if gone(symbol) {
                partners.retain(|key| key.0 != slot);
            }
        }
        self.managers
            .retain(|(owner, symbol), _| *owner != slot || !gone(symbol));
        self.rename_links(|origin, symbol| origin.0 == slot && gone(symbol));
    }

    /// Forgets the drawings of the chart in `slot`, supplied or restored there anew, of each
    /// symbol whose drawings it no longer holds as at the last frame: a new chart holds none of
    /// them, and a restore replaces those of the symbol it shows, whose ids may name other
    /// drawings now. The drawings a restored chart kept stay linked. The chart receives the
    /// mirrored drawings of its symbol as a chart that comes to it does.
    pub(super) fn forget_slot(&mut self, slot: usize, chart: &Chart) {
        let kept: HashSet<String> = self
            .managers
            .iter()
            .filter(|((owner, symbol), identity)| {
                *owner == slot
                    && drawings_of(chart, symbol)
                        .is_some_and(|manager| manager.identity == **identity)
            })
            .map(|((_, symbol), _)| symbol.clone())
            .collect();
        self.forget(slot, |symbol| !kept.contains(symbol));
        self.arriving.insert(slot);
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

    /// Whether the stored drawing `key` went because its chart read storage after another chart
    /// of that storage took the drawing into a folder kept on the chart or in the layout, which
    /// is no deletion: that chart's change reaches the others.
    fn left_storage(&self, charts: &[&mut Chart], count: usize, key: Key) -> bool {
        let origin = self.origin_of(key);
        (0..count).any(|slot| {
            slot != key.0
                && charts[slot].shares_stored_drawings(charts[key.0])
                && self.linked_on(charts, slot, origin).is_some_and(|id| {
                    charts[slot]
                        .drawings()
                        .drawings
                        .iter()
                        .any(|d| d.id == id && !charts[slot].stores(charts[slot].symbol(), d))
                })
        })
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
    /// drawing of its chart and equal `drawing` in content and pane, one per chart: what a
    /// restored original and its restored copy are. A drawing in a study pane equals one in the
    /// linked pane of the other chart (see [`Chart::linked_pane`]).
    fn equal_drawings(
        &self,
        charts: &[&mut Chart],
        count: usize,
        groups: &[Option<String>; MAX_CHARTS],
        key: Key,
        drawing: &Drawing,
        layout_wide: bool,
    ) -> Vec<Key> {
        let own = content(charts[key.0], drawing);
        (0..count)
            .filter(|&slot| {
                slot != key.0
                    && groups[slot] == groups[key.0]
                    && charts[slot].symbol() == charts[key.0].symbol()
                    // Charts on one storage read each other's stored drawings from it.
                    && !(charts[key.0].stores(charts[key.0].symbol(), drawing)
                        && charts[slot].shares_stored_drawings(charts[key.0]))
            })
            .filter_map(|slot| {
                // A study-pane drawing equals one in the linked pane only.
                let pane = match drawing.pane {
                    Some(_) => Some(linked_study_pane(charts[key.0], drawing, charts[slot])?),
                    None => None,
                };
                let in_pane = |d: &Drawing| match pane {
                    Some(pane) => charts[slot].owner_pane(d) == Some(pane),
                    None => d.pane.is_none(),
                };
                let other = charts[slot].drawings().drawings.iter().find(|d| {
                    in_pane(d)
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
        groups: &[Option<String>; MAX_CHARTS],
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
        // Drawings a load put in place of those of a symbol are new to the layout, and the chart
        // receives the mirrored drawings of its symbol again, as after a restore.
        let replaced: Vec<(usize, String)> = self
            .managers
            .iter()
            .filter(|((slot, symbol), identity)| {
                *slot < count
                    && drawings_of(charts[*slot], symbol)
                        .is_some_and(|manager| manager.identity != **identity)
            })
            .map(|(entry, _)| entry.clone())
            .collect();
        for (slot, symbol) in replaced {
            if charts[slot].symbol() == symbol {
                self.arriving.insert(slot);
            }
            self.forget(slot, |shown| shown == symbol);
        }
        self.shown.resize(count, None);
        for (slot, chart) in charts.iter().take(count).enumerate() {
            let before = self.shown[slot].replace(chart.symbol().to_owned());
            if before.is_some_and(|before| before != chart.symbol()) {
                self.arriving.insert(slot);
            }
        }
        // A drawing a chart put aside with its symbol keeps the drawings of that symbol it was
        // linked to: those on show, and those put aside that keep it. A mirrored drawing that the
        // chart deleted, or confined to it and saves, in the frame in which it left the symbol is
        // a change of that frame instead, unless the chart dropped or replaced the drawings of a
        // symbol at once since the last frame. A confined drawing that the chart does not save,
        // such as a copy on a chart that stores drawings, is put aside like the others: the chart
        // cannot store it while it shows another symbol, so the copies on show stay, as one of
        // them may be the stored one.
        let mut put_aside = Vec::new();
        let mut left = Vec::new();
        // Charts that dropped the drawings of a symbol with the last of them.
        let mut dropped: Vec<(usize, String)> = Vec::new();
        for (&key, old) in &self.previous {
            if key.0 >= count || old.symbol == charts[key.0].symbol() {
                continue;
            }
            let chart = &charts[key.0];
            let manager = chart
                .symbol_drawings
                .get(&old.symbol)
                .map(|(manager, _)| manager);
            let drawing =
                manager.and_then(|manager| manager.drawings.iter().find(|d| d.id == key.1));
            // shortcut: a host that puts an empty manager in place of the shown one through
            // `drawings_mut` and changes the symbol before the frame reads as deletions; count
            // such replacements too if hosts need them.
            if mirrored(old.sync, layout_wide)
                && drawing
                    .is_none_or(|d| !mirrored(d.folder_sync_mode(), layout_wide) && d.saveable)
                && self.resets.get(key.0) == Some(&chart.drawing_resets)
            {
                if manager.is_none() && !dropped.contains(&(key.0, old.symbol.clone())) {
                    dropped.push((key.0, old.symbol.clone()));
                }
                left.push((key, old.symbol.clone(), self.origin_of(key)));
                continue;
            }
            // An edit made before the symbol change in this frame is seen now.
            let edited = drawing.is_some_and(|d| differs(chart, d, old));
            put_aside.push((
                key,
                old.symbol.clone(),
                if edited { self.frame } else { old.seen },
            ));
        }
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
        // The copies on show of a drawing deleted or confined as its chart left go too, as do the
        // copies charts put aside, also in this frame: the change counts in this frame, and only
        // a change of that frame on show wins the tie.
        for (key, symbol, origin) in &left {
            let linked: Vec<Key> = (0..count)
                .filter(|&slot| {
                    slot != key.0
                        && groups[slot] == groups[key.0]
                        && charts[slot].symbol() == symbol
                })
                .filter_map(|slot| Some((slot, self.linked_on(charts, slot, *origin)?)))
                .collect();
            let newer = linked.iter().any(|&(slot, id)| {
                let chart = &charts[slot];
                chart
                    .drawings()
                    .drawings
                    .iter()
                    .find(|d| d.id == id)
                    .zip(self.previous.get(&(slot, id)))
                    .is_some_and(|(drawing, old)| differs(chart, drawing, old))
            });
            if newer {
                // The link goes on without this copy: a chart that put a copy aside linked to it
                // takes the copies on show instead.
                for ((_, aside_symbol, _), (_, partners)) in &mut self.aside {
                    if aside_symbol == symbol && partners.contains(key) {
                        for other in &linked {
                            if !partners.contains(other) {
                                partners.push(*other);
                            }
                        }
                    }
                }
                continue;
            }
            changed = true;
            let mut writers = Vec::new();
            for (slot, id) in linked {
                delete_copy(charts, &mut writers, key.0, slot, id);
            }
            self.delete_aside_copies(charts, count, groups, symbol, *key);
        }
        // Ids belong to a symbol's drawings, so the records of a symbol a chart left go before
        // anything is matched by key.
        for key in put_aside
            .iter()
            .map(|(key, ..)| key)
            .chain(left.iter().map(|(key, ..)| key))
        {
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
            if key.0 < count
                && !seen.contains_key(&key)
                && old.symbol == charts[key.0].symbol()
                && !(old.stored && self.left_storage(charts, count, key))
            {
                changes.push((key, None, None));
            }
        }
        let mut joined = self.join_read_drawings(charts, count, groups, &changes);
        let extra = self.join_new_drawings(
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
            // The version a returning link takes goes to an arriving chart too.
            .filter(|a| {
                !extra
                    .iter()
                    .any(|e| e.2 == a.2 && self.origin_of(e.0) == self.origin_of(a.0))
            })
            .collect();
        let changes: Vec<Change> = changes
            .into_iter()
            .filter(|(key, ..)| !joined.contains(key))
            .collect();
        let changes = self
            .one_version(charts, groups, &seen, layout_wide, changes, extra)
            .into_iter()
            .chain(arrived);
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
            // Charts on one storage read each other's stored drawings instead.
            let stored = drawing
                .as_ref()
                .is_some_and(|drawing| charts[key.0].stores(&symbol, drawing));
            for slot in 0..count {
                if slot == key.0
                    || only.is_some_and(|only| only != slot)
                    || groups[slot] != groups[key.0]
                    || charts[slot].symbol() != symbol
                {
                    continue;
                }
                if stored && charts[slot].shares_stored_drawings(charts[key.0]) {
                    // A copy the chart does not store, as of a drawing taken out of a folder kept
                    // in the layout, gives way to the stored drawing it reads.
                    let copies: Vec<usize> = self
                        .origins
                        .iter()
                        .filter(|&(&(at, _), &linked)| at == slot && linked == origin)
                        .map(|(&(_, id), _)| id)
                        .filter(|&id| {
                            charts[slot]
                                .drawings()
                                .drawings
                                .iter()
                                .any(|d| d.id == id && !charts[slot].stores(&symbol, d))
                        })
                        .collect();
                    for id in copies {
                        changed = true;
                        delete_copy(charts, &mut writers, key.0, slot, id);
                    }
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
                    if let Some(id) = linked {
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
                        let Some(target) =
                            charts[slot].drawings().drawings.iter().find(|d| d.id == id)
                        else {
                            continue;
                        };
                        copy.id = target.id;
                        copy.stored_id.clone_from(&target.stored_id);
                        copy.created_at = target.created_at;
                        copy.saveable = target.saveable && !copy_only;
                        seen.insert((slot, id), frame);
                        // A copy that holds the change already, such as one written while another
                        // chart waits for its bars, is not written again: each write is an undo
                        // step, and a storage update where the chart stores the copy.
                        if serde_json::to_string(target).ok() == serde_json::to_string(&copy).ok()
                            && timed_points(charts[slot], target)
                                == timed_points(charts[slot], &copy)
                            && target.notes == copy.notes
                        {
                            continue;
                        }
                        changed = true;
                        unlock(charts[slot], id);
                        if writes(charts, &writers, key.0, slot, id) {
                            writers.push(slot);
                            if pointer_down {
                                self.unwritten.insert((slot, id));
                            } else {
                                charts[slot].history.drawing_edits += 1;
                            }
                        }
                        // A copy that saving would refuse leaves the target as it was, so this
                        // chart keeps saving.
                        let _ = charts[slot]
                            .drawings_mut()
                            .drag_edit_drawing(id, |target| *target = copy);
                    } else {
                        changed = true;
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
                    saveable |= delete_copy(charts, &mut writers, key.0, slot, id);
                    deleted.push((slot, id));
                }
            }
            if !mirror {
                // A drawing deleted, or confined to its chart, also leaves the charts that put a
                // copy aside, which may be linked to any of the drawings removed here.
                for gone in deleted {
                    self.delete_aside_copies(charts, count, groups, &symbol, gone);
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
        let mut forgotten: Vec<(usize, String)> = dropped;
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
                            stored: chart.stores(chart.symbol(), drawing),
                            seen: seen.get(&(slot, drawing.id)).copied().unwrap_or(self.frame),
                        },
                    );
                }
            }
        }
        self.resets = charts
            .iter()
            .take(count)
            .map(|chart| chart.drawing_resets)
            .collect();
        self.managers.clear();
        for (slot, chart) in charts.iter().take(count).enumerate() {
            self.managers
                .insert((slot, chart.symbol().to_owned()), chart.drawings().identity);
            for (symbol, (manager, _)) in &chart.symbol_drawings {
                self.managers
                    .insert((slot, symbol.clone()), manager.identity);
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
        groups: &[Option<String>; MAX_CHARTS],
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

    /// Keeps one version of each link for the frame. Among the changes of a link, the one the
    /// layout saw in the latest frame wins (see [`Snapshot::seen`]); on a tie, the one on show,
    /// then a removal (a deletion, or a confinement to its chart) over an edit, then the one of
    /// the later slot. A removal that wins keeps the other removals of its link, each of which
    /// takes its put-aside copies along. The version `join_new_drawings` chose for a returning
    /// link goes instead only when it is newer than every change of the link.
    fn one_version(
        &self,
        charts: &[&mut Chart],
        groups: &[Option<String>; MAX_CHARTS],
        seen: &HashMap<Key, u64>,
        layout_wide: bool,
        changes: Vec<Change>,
        extra: Vec<Change>,
    ) -> Vec<Change> {
        let link = |key: Key| (self.origin_of(key), charts[key.0].symbol(), &groups[key.0]);
        let removal = |drawing: &Option<Drawing>| {
            drawing
                .as_ref()
                .is_none_or(|d| !mirrored(d.folder_sync_mode(), layout_wide))
        };
        // A deleted drawing has no `seen`: its deletion is a change of this frame.
        let rank = |key: Key, removes: bool| {
            (
                seen.get(&key).copied().unwrap_or(self.frame),
                self.previous.contains_key(&key),
                removes,
                key.0,
            )
        };
        let mut newest = HashMap::new();
        for (key, drawing, _) in &changes {
            let change = rank(*key, removal(drawing));
            newest
                .entry(link(*key))
                .and_modify(|best| {
                    if change > *best {
                        *best = change;
                    }
                })
                .or_insert(change);
        }
        // Pass 2 sends one version per returning link, from one drawing kept on its chart.
        let returned: HashSet<_> = extra
            .iter()
            .filter(|(key, ..)| {
                newest
                    .get(&link(*key))
                    .is_none_or(|best| rank(*key, false) > *best)
            })
            .map(|(key, ..)| link(*key))
            .collect();
        changes
            .into_iter()
            .filter(|(key, drawing, _)| {
                let link = link(*key);
                let removes = removal(drawing);
                !returned.contains(&link)
                    && newest
                        .get(&link)
                        .is_some_and(|best| rank(*key, removes) == *best || (best.2 && removes))
            })
            .chain(
                extra
                    .into_iter()
                    .filter(|(key, ..)| returned.contains(&link(*key))),
            )
            .collect()
    }

    /// The mirrored drawings of its symbol that other charts hold, for each chart that came to the
    /// symbol and has no copy of them: each goes to that chart alone, once, unless a change of its
    /// link in this frame reaches that chart anyway. A drawing on a chart that is equal to one of
    /// them is linked to it instead. A drawing in a study pane goes only to a chart with the
    /// linked pane.
    fn arrived_drawings(
        &mut self,
        charts: &[&mut Chart],
        count: usize,
        groups: &[Option<String>; MAX_CHARTS],
        changes: &[Change],
        joined: &HashSet<Key>,
        layout_wide: bool,
    ) -> Vec<Change> {
        // The link and the chart of each change sent in this frame: it goes to every chart of
        // that chart's symbol and group, an arriving one too.
        let sent: Vec<(Key, usize)> = changes
            .iter()
            .filter(|(key, ..)| !joined.contains(key))
            .map(|(key, ..)| (self.origin_of(*key), key.0))
            .collect();
        // Each link an arriving chart is given, by the first chart that holds it.
        let mut given: HashSet<(Key, usize)> = HashSet::new();
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
                    let origin = self.origin_of(key);
                    if (drawing.pane.is_some()
                        && linked_study_pane(charts[source], drawing, charts[slot]).is_none())
                        || !mirrored(drawing.folder_sync_mode(), layout_wide)
                        || sent.iter().any(|&(changed, from)| {
                            changed == origin
                                && groups[from] == groups[slot]
                                && charts[from].symbol() == charts[slot].symbol()
                        })
                        // Read from the storage they share, as the loop leaves it.
                        || (charts[source].stores(charts[source].symbol(), drawing)
                            && charts[slot].shares_stored_drawings(charts[source]))
                        || given.contains(&(origin, slot))
                        || self.linked_on(charts, slot, origin).is_some()
                    {
                        continue;
                    }
                    let equal =
                        self.equal_drawings(charts, count, groups, key, drawing, layout_wide);
                    let equal: Vec<Key> =
                        equal.into_iter().filter(|other| other.0 == slot).collect();
                    if self.join(charts, key, equal).is_empty() {
                        given.insert((origin, slot));
                        arrivals.push((key, Some(drawing.clone()), Some(slot)));
                    }
                }
            }
        }
        arrivals
    }

    /// Takes the copies of a drawing of `symbol` deleted or confined to its chart out of the
    /// charts of its group that put them aside with the symbol, or took them up again in this
    /// frame, so that they do not come back with the chart.
    fn delete_aside_copies(
        &mut self,
        charts: &mut [&mut Chart],
        count: usize,
        groups: &[Option<String>; MAX_CHARTS],
        symbol: &str,
        key: Key,
    ) {
        let gone: Vec<_> = self
            .aside
            .iter()
            .filter(|((slot, aside_symbol, _), (_, partners))| {
                aside_symbol == symbol && groups[*slot] == groups[key.0] && partners.contains(&key)
            })
            .map(|(entry, _)| entry.clone())
            .collect();
        for (slot, symbol, id) in gone {
            self.aside.remove(&(slot, symbol.clone(), id));
            if slot >= count {
                continue;
            }
            let chart = &mut *charts[slot];
            // A drawing the chart stores comes back from its storage, which this does not write.
            let gone = drawings_of(chart, &symbol).is_some_and(|manager| {
                manager
                    .drawings
                    .iter()
                    .any(|d| d.id == id && !chart.stores(&symbol, d))
            });
            let manager = if chart.symbol() == symbol {
                Some(&mut chart.drawings)
            } else {
                chart
                    .symbol_drawings
                    .get_mut(&symbol)
                    .map(|(manager, _)| manager)
            };
            if let Some(manager) = manager
                && gone
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
        groups: &[Option<String>; MAX_CHARTS],
        changes: &[Change],
    ) -> HashSet<Key> {
        let mut joined = HashSet::new();
        for (key, drawing, _) in changes {
            let Some(drawing) = drawing else { continue };
            if !charts[key.0].stores(charts[key.0].symbol(), drawing)
                || drawing.stored_id.is_none()
                || self.origins.contains_key(key)
            {
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
                    let other = charts[slot].drawings().drawings.iter().find(|d| {
                        charts[slot].stores(charts[slot].symbol(), d) && sync_id(d) == record
                    })?;
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
    /// The tool name given by [`DrawingToolRegistry::tool_name`].
    tool: String,
    /// The name given to the drawing, if any.
    pub label: Option<String>,
    /// Time of the last edit in Unix milliseconds, once an edit reached the drawing.
    pub modified_at: Option<i64>,
}

impl SymbolDrawing {
    /// The name given to the drawing, otherwise its tool's.
    pub fn name(&self) -> &str {
        self.given_name().unwrap_or(&self.tool)
    }

    /// [`Self::name`] as lists show it: the given name as saved, the tool's through `translate`.
    pub(crate) fn shown_name(&self, translate: impl Fn(&str) -> String) -> String {
        self.given_name()
            .map_or_else(|| translate(&self.tool), str::to_owned)
    }

    fn given_name(&self) -> Option<&str> {
        self.label
            .as_deref()
            .filter(|label| !label.trim().is_empty())
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

    /// Orders the drawings as [`Self::sort`] does, by the names the rows show, with `shown`
    /// translating tool names.
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
                drawing.shown_name(&shown).to_lowercase(),
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
    settings.series_layer = None;
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

fn live_row(drawing: &Drawing, registry: &DrawingToolRegistry) -> SymbolDrawing {
    SymbolDrawing {
        id: sync_id(drawing),
        tool_type: drawing.tool_type,
        tool: registry.tool_name(drawing).to_owned(),
        label: drawing.label.clone(),
        modified_at: drawing.modified_at,
    }
}

fn stored_row(record: &StoredDrawing, registry: &DrawingToolRegistry) -> SymbolDrawing {
    SymbolDrawing {
        id: record.id.clone(),
        tool_type: record.drawing.tool_type,
        tool: registry.tool_name(&record.drawing).to_owned(),
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
                    live_row(drawing, &chart.drawings().registry),
                    live_key(chart, drawing),
                );
            }
        }
        if chart.keeps_stored_drawings() {
            for (symbol, drawings) in chart.stored_symbols() {
                if lists_stored(charts, slot, symbol, drawings) {
                    for record in drawings.drawings().iter().filter(|r| r.drawing.listed) {
                        hold(
                            symbol,
                            stored_row(record, &chart.drawings().registry),
                            stored_key(record),
                        );
                    }
                }
            }
        }
        for (symbol, (manager, _)) in &chart.symbol_drawings {
            // Storage lists the drawings it holds; those kept on the chart or in the layout only
            // the chart does.
            for drawing in manager
                .drawings
                .iter()
                .filter(|d| inventoried(d) && !chart.stores(symbol, d))
            {
                hold(
                    symbol,
                    live_row(drawing, &manager.registry),
                    live_key(chart, drawing),
                );
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
    /// included, and leaves those whose user edits are off and those of other symbols. Stored
    /// drawings are written without them.
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
        // A symbol with only protected drawings has no deletion to record on any chart.
        let editable = |d: &Drawing| inventoried(d) && d.user_edit_enabled;
        if !charts.iter().any(|chart| {
            (chart.symbol() == symbol && chart.drawings().drawings.iter().any(editable))
                || chart
                    .symbol_drawings
                    .get(symbol)
                    .is_some_and(|(manager, _)| manager.drawings.iter().any(editable))
                || chart.stored_symbol_drawings(symbol).is_some_and(|stored| {
                    stored
                        .drawings()
                        .iter()
                        .any(|record| editable(&record.drawing))
                })
        }) {
            return true;
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
                if !drawing.user_edit_enabled {
                    return false;
                }
                if writes && inventoried(drawing) {
                    return true;
                }
                !chart.stores(symbol, drawing) && keys.contains(&live_key(chart, drawing))
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
                        .filter(|drawing| {
                            (stored || !chart.stores(symbol, drawing)) && doomed(chart, drawing)
                        })
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
            .filter(|stored| {
                writes
                    && stored
                        .drawings()
                        .iter()
                        .any(|record| record.drawing.listed && record.drawing.user_edit_enabled)
            })
            .cloned();
        if let Some(stored) = &stored {
            let mut left = stored.clone();
            left.drawings
                .retain(|record| !record.drawing.listed || !record.drawing.user_edit_enabled);
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
        let _ = chart.drawings_mut().edit_drawing(id, |d| d.locked = false);
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

/// Deletes the copy `id` on the chart in `slot` of a drawing deleted on the chart in `from`, and
/// returns whether the chart in `slot` saves the deletion: it writes it to its storage, or the
/// copy is kept on that chart or in the layout, which the chart saves with itself.
fn delete_copy(
    charts: &mut [&mut Chart],
    writers: &mut Vec<usize>,
    from: usize,
    slot: usize,
    id: usize,
) -> bool {
    unlock(charts[slot], id);
    let written = writes(charts, writers, from, slot, id);
    let kept = charts[slot]
        .drawings()
        .drawings
        .iter()
        .any(|d| d.id == id && d.saveable && !charts[slot].stores(charts[slot].symbol(), d));
    if written {
        writers.push(slot);
        charts[slot].history.drawing_edits += 1;
    }
    charts[slot].drawings_mut().delete_drawing(id);
    written || kept
}

/// The study pane of `target` linked to the one of `source` that draws `drawing`: study ids are
/// local to each chart, so a copy goes to the corresponding pane. `None` for a drawing that no
/// study pane draws, and where `target` has no such pane.
fn linked_study_pane(source: &Chart, drawing: &Drawing, target: &Chart) -> Option<PaneId> {
    let pane = source
        .owner_pane(drawing)
        .filter(|pane| *pane != PaneId::MAIN)?;
    source.linked_pane(pane, target)
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
    let manager = source.drawings();
    // Only removal while filtered leaves the drawing with an absent owner. A file loaded
    // without its study has no local owner and mirrors as a price-pane drawing.
    let missing = drawing.pane.is_some() && !manager.removed_panes.is_empty();
    if let Some((pane, _)) = missing
        .then(|| sync_id(drawing))
        .and_then(|id| source.unshown_pane(manager, drawing, &id))
    {
        copy.pane = Some(target.stored_pane_owner(pane)?);
    } else if drawing.pane.is_some() {
        let pane = linked_study_pane(source, drawing, target)?;
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

    /// Symbol deletion leaves protected drawings on both the shown and retained symbol paths.
    #[test]
    fn deleting_a_symbol_keeps_drawings_without_user_edits() {
        for shown in [true, false] {
            for free in [true, false] {
                let mut a = bars(60);
                a.set_symbol("AAA");
                let mut held = line("Held");
                held.user_edit_enabled = false;
                // Initial host data is outside the user's undo history.
                a.drawings_mut().drawings.push(held);
                if free {
                    let mut drawing = line("Free");
                    drawing.id = 1;
                    a.drawings_mut().drawings.push(drawing);
                }
                if !shown {
                    a.set_symbol("BBB");
                }
                let mut layout = ChartLayout::default();
                assert!(layout.delete_symbol_drawings(&mut [&mut a], "AAA"));
                assert_eq!(names(&layout, &[&mut a]), listed(&[("AAA", &["Held"])]));
                assert_eq!(a.can_undo(), free, "shown {shown}, free {free}");
            }
        }
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
                    .set_folder_sync(&top, Some(FolderSync::AllLayouts))
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

        // A drawing kept on a chart away from its symbol is listed and deleted with the symbol,
        // though storage does not hold it.
        b.set_symbol("CCC");
        settle(&mut a, &mut b);
        b.user_action(|chart| {
            let id = chart.drawings_mut().add_drawing(line("Kept")).unwrap();
            let folder = ["Mine".to_owned()];
            assert!(chart.drawings_mut().move_to_folder(&[id], &[], &folder));
            assert!(
                chart
                    .drawings_mut()
                    .set_folder_sync(&folder, Some(FolderSync::Chart))
            );
        });
        b.set_symbol("AAA");
        settle(&mut a, &mut b);
        assert_eq!(records("CCC"), [("Hidden".to_owned(), 2_001)]);
        assert_eq!(
            names(&layout, &[&mut a, &mut b]),
            listed(&[("CCC", &["Kept"]), ("DDD", &["Two"])])
        );
        assert!(layout.delete_symbol_drawings(&mut [&mut a, &mut b], "CCC"));
        settle(&mut a, &mut b);
        assert_eq!(
            names(&layout, &[&mut a, &mut b]),
            listed(&[("DDD", &["Two"])])
        );

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

    /// Guards #2918: study ids are local to each chart. A chart that restored its own snapshot
    /// reads the drawings document of another chart, whose RSI has the saved id of the MACD here,
    /// and shows the line on its RSI. Built again after a restart from a layout that leaves its
    /// drawings to storage, a chart maps the study ids of the document the layout's chart wrote,
    /// as written now and as written before documents named their writer, to the studies the
    /// restore added: an SMA of the RSI in its pane keeps that pane from matching by its studies.
    #[cfg(feature = "ui")]
    #[test]
    fn stored_drawing_of_another_chart_keeps_its_study_pane() {
        use crate::services::document::tests::Revised;
        use crate::studies::{MACD, RSI, SMA, StudySource};
        // The study of the one drawing `chart` shows once it read `document` from its storage.
        let read = |mut chart: Chart, document: &str| {
            let host = Arc::new(Revised::default());
            host.put("AAA", document);
            chart.set_storage(host);
            chart.flush_stored_drawings();
            let drawings = &chart.drawings().drawings;
            assert_eq!(drawings.len(), 1);
            let pane = drawings[0].pane;
            let study = chart.indicators().find(|(id, _)| Some(id.0) == pane);
            study.map(|(_, study)| study.name().to_owned())
        };
        let draw_on = |chart: &mut Chart, study: SeriesId| {
            let mut drawing = line("On RSI");
            drawing.pane = Some(study.0);
            chart.drawings_mut().add_drawing(drawing).unwrap();
            chart.save_drawings().unwrap().to_json().unwrap()
        };

        let mut other = bars(60);
        other.set_symbol("AAA");
        let rsi = other.add_indicator(Box::new(RSI::new(14)));
        let document = draw_on(&mut other, rsi);
        let mut chart = bars(60);
        chart.set_symbol("AAA");
        let macd = chart.add_indicator(Box::new(MACD::new(12, 26, 9)));
        chart.add_indicator(Box::new(RSI::new(14)));
        assert_eq!(macd, rsi, "each chart gives its first study the same id");
        chart.restore(&chart.snapshot().unwrap()).unwrap();
        assert_eq!(read(chart, &document).as_deref(), Some("RSI"));

        let mut saved = bars(60);
        saved.set_symbol("AAA");
        let rsi = saved.add_indicator(Box::new(RSI::new(14)));
        let mean = saved.add_indicator(Box::new(SMA::new(5)));
        let source = StudySource::Study { id: rsi.0, plot: 0 };
        saved.set_indicator_source(mean, source).unwrap();
        assert!(saved.move_indicator_to_pane(mean, rsi));
        let written = draw_on(&mut saved, rsi);
        let mut snapshot = saved.snapshot().unwrap();
        snapshot.drawings = None;
        let mut older: serde_json::Value = serde_json::from_str(&written).unwrap();
        assert!(older.as_object_mut().unwrap().remove("saved_by").is_some());
        for document in [written, older.to_string()] {
            let mut restarted = bars(60);
            restarted.restore(&snapshot).unwrap();
            assert_eq!(read(restarted, &document).as_deref(), Some("RSI"));
        }
    }

    /// Guards #3286 and #3436: opening a layout shows the symbol's stored drawings. One moved or
    /// deleted elsewhere since the layout was saved stays so, one drawn elsewhere shows, and one
    /// the layout keeps on a chart or in a layout comes back as the layout holds it, which storage
    /// does not hold. As a document of an earlier version holds such a drawing, it shows elsewhere
    /// as stored and stays stored until it is changed. The opening chart's own drawing is kept
    /// whether it was stored before the layout opened, unsaved then, or drawn before the stored
    /// drawings were read again.
    #[cfg(feature = "ui")]
    #[test]
    fn opening_a_layout_keeps_the_drawings_stored_since_it_was_saved() {
        use crate::services::document::tests::Revised;
        use FolderSync::{AllLayouts, Chart as Local, Layout};
        #[derive(Debug)]
        enum Drawn {
            Before,
            Unsaved,
            After,
        }
        /// What another layout does to the layout's drawing, if it shows there.
        #[derive(Debug, PartialEq)]
        enum Elsewhere {
            Moved,
            Deleted,
            Untouched,
        }
        // (folder of the layout's drawing, stored there as by an earlier version, what another
        // layout does to it, when the opening chart draws its own, prices shown, prices stored)
        type Row = (
            Option<FolderSync>,
            bool,
            Elsewhere,
            Drawn,
            &'static [f64],
            &'static [f64],
        );
        let rows: [Row; 12] = [
            (
                None,
                false,
                Elsewhere::Moved,
                Drawn::Before,
                &[105.0, 108.0],
                &[105.0, 108.0],
            ),
            (
                Some(AllLayouts),
                false,
                Elsewhere::Moved,
                Drawn::Before,
                &[105.0, 108.0],
                &[105.0, 108.0],
            ),
            (
                Some(Layout),
                false,
                Elsewhere::Untouched,
                Drawn::Before,
                &[104.0, 108.0],
                &[108.0],
            ),
            (
                Some(Local),
                false,
                Elsewhere::Untouched,
                Drawn::Before,
                &[104.0, 108.0],
                &[108.0],
            ),
            (
                Some(Layout),
                true,
                Elsewhere::Moved,
                Drawn::Before,
                &[104.0, 108.0],
                &[108.0],
            ),
            (
                Some(Local),
                true,
                Elsewhere::Moved,
                Drawn::Before,
                &[104.0, 108.0],
                &[108.0],
            ),
            (
                Some(Layout),
                true,
                Elsewhere::Untouched,
                Drawn::Before,
                &[104.0, 108.0],
                &[104.0, 108.0],
            ),
            (
                Some(Local),
                true,
                Elsewhere::Untouched,
                Drawn::Before,
                &[104.0, 108.0],
                &[104.0, 108.0],
            ),
            (
                None,
                false,
                Elsewhere::Deleted,
                Drawn::Before,
                &[108.0],
                &[108.0],
            ),
            (
                Some(Layout),
                true,
                Elsewhere::Deleted,
                Drawn::Before,
                &[104.0, 108.0],
                &[108.0],
            ),
            (
                None,
                false,
                Elsewhere::Moved,
                Drawn::Unsaved,
                &[105.0, 108.0],
                &[105.0, 108.0],
            ),
            (
                None,
                false,
                Elsewhere::Moved,
                Drawn::After,
                &[105.0, 108.0],
                &[105.0, 108.0],
            ),
        ];
        for (folder, earlier, elsewhere, drawn, shown_expected, stored_expected) in rows {
            let host = Arc::new(Revised::default());
            let chart = || {
                let mut chart = bars(60);
                chart.set_symbol("SAMPLE");
                chart.set_storage(host.clone());
                assert!(chart.flush_stored_drawings());
                chart
            };
            let mut saved = chart();
            saved.user_action(|chart| {
                draw(chart, 104.0);
                if let Some(folder) = folder {
                    into_folder(chart, folder);
                }
            });
            assert!(saved.flush_stored_drawings());
            let layout = saved.snapshot().unwrap();
            if earlier {
                // An earlier version stored every saveable drawing, as the layout's chart saves.
                host.put("SAMPLE", &saved.save_drawings().unwrap().to_json().unwrap());
            }
            // Another layout of the symbol moves or deletes that drawing and draws one at 108.
            let mut other = chart();
            if elsewhere != Elsewhere::Untouched {
                other.user_action(|chart| {
                    if elsewhere == Elsewhere::Deleted {
                        let id = chart.drawings().drawings[0].id;
                        chart.drawings_mut().delete_drawing(id);
                    } else {
                        move_first(chart, 105.0);
                    }
                });
            }
            assert!(other.flush_stored_drawings());
            if !matches!(drawn, Drawn::After) {
                other.user_action(|chart| draw(chart, 108.0));
            }
            if matches!(drawn, Drawn::Before) {
                assert!(other.flush_stored_drawings());
            }
            other.restore(&layout).unwrap();
            if matches!(drawn, Drawn::After) {
                other.user_action(|chart| draw(chart, 108.0));
            }
            // A write the restore started is settled before the stored drawings are read again.
            assert!((0..3).any(|_| other.flush_stored_drawings()));
            let [shown] = prices(std::array::from_ref(&other));
            // Each drawing keeps a stored id of its own, so the layout saved now holds them all.
            let resaved = other.snapshot().unwrap().drawings.unwrap().to_string();
            let held = DrawingStorage::from_json(&resaved)
                .unwrap()
                .0
                .drawings()
                .len();
            assert_eq!(
                held,
                shown.len(),
                "{folder:?}, earlier {earlier}, {elsewhere:?}"
            );
            let (document, _) = DrawingStorage::from_json(&host.get("SAMPLE").unwrap()).unwrap();
            let mut stored: Vec<f64> = document
                .drawings()
                .iter()
                .map(|d| d.points[0].price)
                .collect();
            stored.sort_by(f64::total_cmp);
            assert_eq!(
                (shown, stored),
                (shown_expected.to_vec(), stored_expected.to_vec()),
                "{folder:?}, earlier {earlier}, {elsewhere:?}, drawn {drawn:?}"
            );
        }
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
            tool: DrawingToolType::TrendLine.as_str().to_owned(),
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

        // A bulk edit is an edit.
        {
            let mut chart = bars(60);
            chart.set_symbol("AAA");
            chart.load_drawings_remapped(&json, true).unwrap();
            let ids: Vec<_> = chart.drawings().drawings.iter().map(|d| d.id).collect();
            chart
                .drawings_mut()
                .edit_drawings(&ids, |d| d.locked = true)
                .unwrap();
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
        use FolderSync::{AllLayouts, Chart as Local, Layout};
        for (mode, layout_wide, copies) in [
            (Some(Layout), false, 1),
            (Some(AllLayouts), false, 1),
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
            .edit_drawing(id, |d| d.chart_points[0].bar_idx = 1.5)
            .unwrap();
        links.sync(&mut refs, 4, &groups, false, true);
        assert_eq!(refs[0].drawings().drawings[0].chart_points[0].bar_idx, 3.0);
        refs[1]
            .drawings_mut()
            .edit_drawing(id, |d| d.notes = Some("shared note".into()))
            .unwrap();
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
                .edit_drawing(source_id, |d| d.chart_points[0].bar_idx = index)
                .unwrap();
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
        a.drawings_mut()
            .edit_drawing(source, |d| d.locked = true)
            .unwrap();
        sync(&mut a, &mut b);
        assert!(copy(&b).unwrap().locked);
        a.drawings_mut()
            .edit_drawing(source, |d| {
                d.locked = false;
                d.chart_points[0].bar_idx = 3.0;
            })
            .unwrap();
        sync(&mut a, &mut b);
        let moved = copy(&b).unwrap();
        assert!(!moved.locked, "an unlock reaches a locked copy");
        assert_eq!(moved.chart_points[0].bar_idx, 1.5);
        a.drawings_mut()
            .edit_drawing(source, |d| d.locked = true)
            .unwrap();
        sync(&mut a, &mut b);
        b.drawings_mut()
            .edit_drawing(id, |d| d.chart_points[0].bar_idx = 0.0)
            .unwrap();
        assert_eq!(copy(&b).unwrap().chart_points[0].bar_idx, 1.5);
        a.drawings_mut()
            .edit_drawing(source, |d| d.locked = false)
            .unwrap();
        a.drawings_mut().delete_drawing(source);
        sync(&mut a, &mut b);
        assert!(copy(&b).is_none(), "a deletion reaches a locked copy");
    }

    /// Each chart keeps its drawings in its own storage, or both in one. A drawing in a folder
    /// shared across layouts is changed on the chart that shows its copy, by an edit, a drag,
    /// This chart only, a move out of the folder or a deletion, and undone and redone there, or
    /// undone on the chart it was drawn on. After each, opening the saved charts again shows what
    /// the charts showed, with or without a later edit on the first chart: the drawing is kept by
    /// one chart, never by none or both, and one kept on its chart shows on no other chart of its
    /// storage (#3436). A drag is written once, when it ends. Set to This chart only in the frame
    /// in which its chart leaves the symbol, a copy is not stored there, and the first chart keeps
    /// the drawing stored. Deleted in that frame, as the chart reads the stored drawings of the
    /// symbol it goes to, the copy is deleted on the first chart too: a read is no load.
    // Saving and opening a layout needs `ui`.
    #[cfg(feature = "ui")]
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
                chart
                    .drawings_mut()
                    .edit_drawing(id, |drawing| {
                        drawing.chart_points[0].price = price;
                        drawing.chart_points[1].price = price + 2.0;
                    })
                    .unwrap();
            });
        };
        let (a_side, b_side) = (0, 1);
        let (none, top_10, top_50): (&[_], &[_], &[_]) = (&[], &[("Top", 10.0)], &[("Top", 50.0)]);
        let (root_10, new_10): (&[_], &[_]) = (&[("", 10.0)], &[("New", 10.0)]);
        // Per chart, with a storage for each chart and with one for both, worked out by hand: a
        // chart that keeps a copy stores the drawing alone, and charts on one storage read it.
        let rows: &[Row] = &[
            ("keep", &[(b_side, Keep)], [none, top_10], [none, top_10]),
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
                [none, &[("Top/Sub", 10.0)]],
            ),
            (
                "keep, undo, redo",
                &[(b_side, Keep), (b_side, Undo), (b_side, Redo)],
                [none, top_10],
                [none, top_10],
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
                                .set_folder_sync(&top, Some(FolderSync::AllLayouts))
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
                    let saved = [&a, &b].map(|chart| chart.snapshot().unwrap());
                    let mut restarted = layout();
                    let (mut a, mut b) = (chart(&storages[0]), chart(&storages[1]));
                    a.restore(&saved[0]).unwrap();
                    b.restore(&saved[1]).unwrap();
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

        // This chart only, set on the chart that shows the copy in the frame in which that chart
        // leaves the symbol: the chart cannot store the copy while it shows another symbol, so
        // the first chart keeps the drawing stored.
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
                    .set_folder_sync(&top, Some(FolderSync::AllLayouts))
            );
        });
        frames(&mut session, &mut a, &mut b);
        b.user_action(|chart| {
            chart
                .drawings_mut()
                .set_folder_sync(&top, Some(FolderSync::Chart));
        });
        b.set_symbol("OTHER");
        frames(&mut session, &mut a, &mut b);
        let saved = a.snapshot().unwrap();
        let mut restarted = layout();
        let (mut a, mut b) = (chart(&storages[0]), chart(&storages[1]));
        a.restore(&saved).unwrap();
        frames(&mut restarted, &mut a, &mut b);
        let mirrored = vec![("Top".to_owned(), 10.0)];
        assert_eq!(
            lines(&a, &b),
            [mirrored.clone(), mirrored],
            "This chart only as the chart left"
        );

        // Deleted on the chart that shows the copy in the frame in which that chart leaves for a
        // symbol whose drawings an earlier session stored, which it reads in that frame.
        let storages = [Arc::new(InMemory::default()), Arc::new(InMemory::default())];
        let mut session = layout();
        let (mut a, mut b) = (chart(&storages[0]), chart(&storages[1]));
        b.set_symbol("OTHER");
        frames(&mut session, &mut a, &mut b);
        b.user_action(|chart| {
            chart.drawings_mut().add_drawing(line(30.0)).unwrap();
        });
        frames(&mut session, &mut a, &mut b);
        let mut session = layout();
        let (mut a, mut b) = (chart(&storages[0]), chart(&storages[1]));
        frames(&mut session, &mut a, &mut b);
        a.user_action(|chart| {
            let id = chart.drawings_mut().add_drawing(line(10.0)).unwrap();
            assert!(chart.drawings_mut().move_to_folder(&[id], &[], &top));
            assert!(
                chart
                    .drawings_mut()
                    .set_folder_sync(&top, Some(FolderSync::AllLayouts))
            );
        });
        frames(&mut session, &mut a, &mut b);
        let copy = first(&b);
        b.user_action(|chart| {
            chart.drawings_mut().delete_drawing(copy);
        });
        b.set_symbol("OTHER");
        frames(&mut session, &mut a, &mut b);
        assert_eq!(
            lines(&a, &b),
            [vec![], vec![(String::new(), 30.0)]],
            "deleted as the chart left for stored drawings"
        );
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
        chart
            .drawings_mut()
            .edit_drawing(id, |drawing| {
                drawing.chart_points[0].price = price;
                drawing.chart_points[1].price = price;
            })
            .unwrap();
    }

    /// Puts the first drawing of `chart` in a folder synchronized as `sync`.
    fn into_folder(chart: &mut Chart, sync: FolderSync) {
        let folder = ["Folder".to_owned()];
        let id = chart.drawings().drawings[0].id;
        assert!(chart.drawings_mut().move_to_folder(&[id], &[], &folder));
        assert!(chart.drawings_mut().set_folder_sync(&folder, Some(sync)));
    }

    /// Two charts in a layout with drawing sync on, the first on SAMPLE with RSI(14) and a line at
    /// 104 in its pane, the second on `symbol`, with RSI(14) too where `rsi`.
    #[cfg(feature = "studies")]
    fn rsi_pair(symbol: &str, rsi: bool) -> (ChartLayout, [Chart; 2]) {
        let mut charts = [bars(60), bars(60)];
        let study = charts[0].add_indicator(Box::new(crate::studies::RSI::new(14)));
        if rsi {
            charts[1].add_indicator(Box::new(crate::studies::RSI::new(14)));
        }
        charts[0].set_symbol("SAMPLE");
        charts[1].set_symbol(symbol);
        let mut layout = ChartLayout {
            mode: super::super::ChartLayoutMode::TwoHorizontal,
            ..Default::default()
        };
        layout.sync.sync_drawings = true;
        let mut drawing = line("");
        drawing.pane = Some(study.0);
        drawing.chart_points = vec![ChartPoint::new(2.0, 104.0), ChartPoint::new(4.0, 104.0)];
        charts[0].drawings_mut().add_drawing(drawing).unwrap();
        frames(&mut layout, &mut charts);
        (layout, charts)
    }

    /// Whether `chart` draws a drawing at `price` in a study pane.
    #[cfg(feature = "studies")]
    fn in_study_pane(chart: &Chart, price: f64) -> bool {
        chart.drawings().drawings.iter().any(|d| {
            d.chart_points[0].price == price
                && chart.drawing_pane(d.id) != Some(chart.main_series_pane())
        })
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
    /// drawings of the symbol draws lines of its own there. A deletion in another group leaves the
    /// copy put aside. A line deleted or confined as its chart leaves leaves the other charts too,
    /// unless one of them changed it in that frame, while drawings a chart forgets or replaces
    /// stay.
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
        for (name, edit, expected, at_return, apart) in [
            ("deleted while away", delete, [vec![], vec![]], false, false),
            (
                "confined to its chart while away",
                confine,
                [vec![104.0], vec![]],
                false,
                false,
            ),
            (
                "deleted in the frame of the return",
                delete,
                [vec![], vec![]],
                true,
                false,
            ),
            (
                "deleted in another group while away",
                delete,
                [vec![], vec![104.0]],
                false,
                true,
            ),
        ] {
            let (mut layout, mut charts) = mirrored_pair(false);
            charts[1].set_symbol("OTHER");
            frames(&mut layout, &mut charts);
            if apart {
                layout.set_sync_group(0, Some("Second".into()));
            }
            edit(&mut charts[0]);
            if !at_return {
                frames(&mut layout, &mut charts);
            }
            charts[1].set_symbol("SAMPLE");
            frames(&mut layout, &mut charts);
            assert_eq!(prices(&charts), expected, "{name}");
        }

        // A copy deleted, or confined to its chart, in the frame in which its chart leaves: the
        // line leaves the other chart too and does not come back, unless that chart changed it in
        // the same frame. Drawings a chart forgets or replaces as it leaves stay on the other
        // chart.
        type Leave = fn(&mut ChartLayout, &mut [Chart; 2]);
        type LeaveRow = (&'static str, Leave, [Vec<f64>; 2], [Vec<f64>; 2]);
        let rows: [LeaveRow; 7] = [
            (
                "deleted as it left",
                |_, charts| {
                    let id = charts[1].drawings().drawings[0].id;
                    charts[1].drawings_mut().delete_drawing(id);
                    charts[1].set_symbol("OTHER");
                },
                [vec![], vec![]],
                [vec![], vec![]],
            ),
            (
                "deleted as it left, another line kept",
                |layout, charts| {
                    draw(&mut charts[1], 70.0);
                    frames(layout, charts);
                    let drawings = &charts[1].drawings().drawings;
                    let line = drawings.iter().find(|d| d.chart_points[0].price == 104.0);
                    let id = line.unwrap().id;
                    charts[1].drawings_mut().delete_drawing(id);
                    charts[1].set_symbol("OTHER");
                },
                [vec![70.0], vec![]],
                [vec![70.0], vec![70.0]],
            ),
            (
                "deleted as it left, its notice taken",
                |_, charts| {
                    let id = charts[1].drawings().drawings[0].id;
                    charts[1].drawings_mut().delete_drawing(id);
                    charts[1]
                        .drawings_mut()
                        .pending_sync
                        .lock()
                        .unwrap()
                        .clear();
                    charts[1].set_symbol("OTHER");
                },
                [vec![], vec![]],
                [vec![], vec![]],
            ),
            (
                "confined as it left",
                |_, charts| {
                    into_folder(&mut charts[1], FolderSync::Chart);
                    charts[1].set_symbol("OTHER");
                },
                [vec![], vec![]],
                [vec![], vec![104.0]],
            ),
            (
                "deleted as it left, moved on show",
                |_, charts| {
                    let id = charts[1].drawings().drawings[0].id;
                    charts[1].drawings_mut().delete_drawing(id);
                    charts[1].set_symbol("OTHER");
                    move_first(&mut charts[0], 95.0);
                },
                [vec![95.0], vec![]],
                [vec![95.0], vec![95.0]],
            ),
            (
                "forgotten as it left",
                |_, charts| {
                    charts[1].set_symbol("OTHER");
                    assert!(charts[1].forget_symbol("SAMPLE"));
                },
                [vec![104.0], vec![]],
                [vec![104.0], vec![104.0]],
            ),
            (
                "replaced as it left",
                |_, charts| {
                    let mut chart = bars(60);
                    chart.set_symbol("SAMPLE");
                    let empty = chart.save_drawings().unwrap().to_json().unwrap();
                    charts[1].load_drawings(&empty).unwrap();
                    charts[1].set_symbol("OTHER");
                },
                [vec![104.0], vec![]],
                [vec![104.0], vec![104.0]],
            ),
        ];
        for (name, leave, away, back) in rows {
            let (mut layout, mut charts) = mirrored_pair(false);
            leave(&mut layout, &mut charts);
            frames(&mut layout, &mut charts);
            assert_eq!(prices(&charts), away, "{name}, left");
            charts[1].set_symbol("SAMPLE");
            frames(&mut layout, &mut charts);
            assert_eq!(prices(&charts), back, "{name}, back");
        }
        // The same with a third chart that leaves, comes back or comes to the symbol in that
        // frame, or that came meanwhile and moves its copy in that frame.
        type Third = fn(&mut ChartLayout, &mut [Chart; 3]);
        type ThirdRow = (
            &'static str,
            [&'static str; 3],
            Third,
            [Vec<f64>; 3],
            [Vec<f64>; 3],
        );
        let rows: [ThirdRow; 4] = [
            (
                "deleted and moved as both left",
                ["SAMPLE"; 3],
                |_, charts| {
                    let id = charts[1].drawings().drawings[0].id;
                    charts[1].drawings_mut().delete_drawing(id);
                    move_first(&mut charts[2], 95.0);
                    for chart in &mut charts[1..] {
                        chart.set_symbol("OTHER");
                    }
                },
                [vec![], vec![], vec![]],
                [vec![], vec![], vec![]],
            ),
            (
                "deleted as it left while another came back",
                ["SAMPLE"; 3],
                |layout, charts| {
                    charts[2].set_symbol("OTHER");
                    frames(layout, charts);
                    let id = charts[1].drawings().drawings[0].id;
                    charts[1].drawings_mut().delete_drawing(id);
                    charts[1].set_symbol("OTHER");
                    charts[2].set_symbol("SAMPLE");
                },
                [vec![], vec![], vec![]],
                [vec![], vec![], vec![]],
            ),
            (
                "deleted as it left while another came",
                ["SAMPLE", "SAMPLE", "OTHER"],
                |_, charts| {
                    let id = charts[1].drawings().drawings[0].id;
                    charts[1].drawings_mut().delete_drawing(id);
                    charts[1].set_symbol("OTHER");
                    charts[2].set_symbol("SAMPLE");
                },
                [vec![], vec![], vec![]],
                [vec![], vec![], vec![]],
            ),
            (
                "deleted as it left while a later copy moved",
                ["SAMPLE", "SAMPLE", "OTHER"],
                |layout, charts| {
                    charts[0].set_symbol("THIRD");
                    frames(layout, charts);
                    charts[2].set_symbol("SAMPLE");
                    frames(layout, charts);
                    let id = charts[1].drawings().drawings[0].id;
                    charts[1].drawings_mut().delete_drawing(id);
                    charts[1].set_symbol("OTHER");
                    move_first(&mut charts[2], 95.0);
                },
                [vec![], vec![], vec![95.0]],
                [vec![95.0], vec![95.0], vec![95.0]],
            ),
        ];
        for (name, symbols, leave, away, back) in rows {
            let (mut layout, mut charts) = three_charts(symbols);
            leave(&mut layout, &mut charts);
            frames(&mut layout, &mut charts);
            assert_eq!(prices(&charts), away, "{name}, left");
            for chart in &mut charts {
                chart.set_symbol("SAMPLE");
            }
            frames(&mut layout, &mut charts);
            assert_eq!(prices(&charts), back, "{name}, back");
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
        // leaves too, and the copy of the third is deleted or moved, also in the frame in which
        // the second comes back. The charts that were away keep the line from coming back and
        // take the move.
        let lower: fn(&mut Chart) = |chart| move_first(chart, 95.0);
        for (name, away, edit, line, at_return) in [
            ("deleted through a later copy", false, delete, vec![], false),
            ("deleted with the first away", true, delete, vec![], false),
            ("moved with the first away", true, lower, vec![95.0], false),
            (
                "deleted in the frame of a return",
                false,
                delete,
                vec![],
                true,
            ),
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
            if at_return {
                charts[1].set_symbol("SAMPLE");
            }
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

    /// The copies of a mirrored line end every frame equal. Two copies changed in one frame end
    /// with the later chart's version, and a deletion or a confinement wins over a move of the
    /// same frame and takes the copies put aside along. A change waiting for the bars of a chart
    /// gives way to a newer one, and the copies that hold it take it once.
    #[test]
    fn mirrored_copies_end_each_frame_equal() {
        let delete: fn(&mut Chart) = |chart| {
            let id = chart.drawings().drawings[0].id;
            chart.drawings_mut().delete_drawing(id);
        };
        let lower: fn(&mut Chart) = |chart| move_first(chart, 95.0);
        let lowest: fn(&mut Chart) = |chart| move_first(chart, 80.0);
        let confine: fn(&mut Chart) = |chart| into_folder(chart, FolderSync::Chart);
        // Both before one frame, as only a host can.
        for (name, first, second, expected) in [
            ("both moved", lower, lowest, [vec![80.0], vec![80.0]]),
            ("confined and moved", confine, lower, [vec![104.0], vec![]]),
        ] {
            let (mut layout, mut charts) = mirrored_pair(false);
            first(&mut charts[0]);
            second(&mut charts[1]);
            frames(&mut layout, &mut charts);
            assert_eq!(prices(&charts), expected, "{name}");
        }
        // The second chart is away with its copy, the third came meanwhile; the first deletes
        // as the third moves or deletes, in one frame.
        for (name, edit) in [("deleted and moved", lower), ("deleted twice", delete)] {
            let (mut layout, mut charts) = three_charts(["SAMPLE", "SAMPLE", "OTHER"]);
            charts[1].set_symbol("OTHER");
            frames(&mut layout, &mut charts);
            charts[2].set_symbol("SAMPLE");
            frames(&mut layout, &mut charts);
            delete(&mut charts[0]);
            edit(&mut charts[2]);
            frames(&mut layout, &mut charts);
            charts[1].set_symbol("SAMPLE");
            frames(&mut layout, &mut charts);
            assert_eq!(
                prices(&charts),
                [Vec::<f64>::new(), vec![], vec![]],
                "{name}"
            );
        }
        // The third chart shows the symbol without its bars, as while a feed loads them, so the
        // copy of a new line waits for them, while the second keeps, moves or deletes its copy.
        // The second takes the line as one undo step, however long the third waits.
        let keep: fn(&mut Chart) = |_| {};
        for (name, edit, waiting, loaded, steps) in [
            (
                "kept",
                keep,
                [vec![104.0], vec![104.0], vec![]],
                [vec![104.0], vec![104.0], vec![104.0]],
                1,
            ),
            (
                "moved",
                lower,
                [vec![95.0], vec![95.0], vec![]],
                [vec![95.0], vec![95.0], vec![95.0]],
                2,
            ),
            (
                "deleted",
                delete,
                [vec![], vec![], vec![]],
                [vec![], vec![], vec![]],
                2,
            ),
        ] {
            let mut charts = [bars(60), bars(60), bars(60)];
            for chart in &mut charts {
                chart.set_symbol("SAMPLE");
            }
            let data = charts[2].bars(SeriesId::MAIN).to_vec();
            charts[2].set_data(SeriesId::MAIN, Vec::new());
            let mut layout = ChartLayout {
                mode: super::super::ChartLayoutMode::ThreeHorizontal,
                ..Default::default()
            };
            layout.sync.sync_drawings = true;
            frames(&mut layout, &mut charts);
            draw(&mut charts[0], 104.0);
            frames(&mut layout, &mut charts);
            edit(&mut charts[1]);
            frames(&mut layout, &mut charts);
            assert_eq!(prices(&charts), waiting, "{name}, bars loading");
            charts[2].set_data(SeriesId::MAIN, data);
            frames(&mut layout, &mut charts);
            assert_eq!(prices(&charts), loaded, "{name}, bars in");
            let mut undone = 0;
            while charts[1].drawings_mut().undo() {
                undone += 1;
            }
            assert_eq!(undone, steps, "undo steps of the second chart, {name}");
        }
    }

    /// A chart that comes to a symbol receives the mirrored drawings its group holds for it, those
    /// made while it showed another symbol too, once its bars are in place, and not the ones local
    /// to their chart. It receives each once, in the version the frame ends with, never one
    /// deleted in that frame, also when another chart of the group brings the drawing back in
    /// that frame. A drawing in a study pane arrives in the linked pane, and not on a chart
    /// without one.
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

        // A chart that comes to the symbol where two charts hold the line receives it once, in
        // the version the frame ends with, also when the line changes, goes or comes back in
        // that frame on another chart of the group, but not for a change in another group.
        type Step = fn(&mut ChartLayout, &mut [Chart; 3]);
        let rows: [(&str, Step, [Vec<f64>; 3], usize); 5] = [
            (
                "held by two charts",
                |_, charts| charts[2].set_symbol("SAMPLE"),
                [vec![104.0], vec![104.0], vec![104.0]],
                1,
            ),
            (
                "moved in the frame",
                |_, charts| {
                    charts[2].set_symbol("SAMPLE");
                    move_first(&mut charts[1], 95.0);
                },
                [vec![95.0], vec![95.0], vec![95.0]],
                1,
            ),
            (
                "deleted in the frame",
                |_, charts| {
                    charts[2].set_symbol("SAMPLE");
                    let id = charts[0].drawings().drawings[0].id;
                    charts[0].drawings_mut().delete_drawing(id);
                },
                [vec![], vec![], vec![]],
                0,
            ),
            (
                "with a return in the frame",
                |layout, charts| {
                    charts[1].set_symbol("OTHER");
                    frames(layout, charts);
                    move_first(&mut charts[0], 95.0);
                    frames(layout, charts);
                    for chart in &mut charts[1..] {
                        chart.set_symbol("SAMPLE");
                    }
                },
                [vec![95.0], vec![95.0], vec![95.0]],
                1,
            ),
            (
                "moved in another group",
                |layout, charts| {
                    layout.set_sync_group(0, Some("Second".into()));
                    frames(layout, charts);
                    move_first(&mut charts[0], 95.0);
                    charts[2].set_symbol("SAMPLE");
                },
                [vec![95.0], vec![104.0], vec![104.0]],
                1,
            ),
        ];
        for (name, step, expected, edits) in rows {
            let (mut layout, mut charts) = three_charts(["SAMPLE", "SAMPLE", "OTHER"]);
            step(&mut layout, &mut charts);
            frames(&mut layout, &mut charts);
            assert_eq!(prices(&charts), expected, "{name}");
            let mut steps = 0;
            while charts[2].drawings_mut().undo() {
                steps += 1;
            }
            assert_eq!(steps, edits, "edits of the arriving chart, {name}");
        }

        // A chart that comes to the symbol receives a line of an RSI pane in its own RSI pane,
        // and nothing without one.
        #[cfg(feature = "studies")]
        for rsi in [true, false] {
            let (mut layout, mut charts) = rsi_pair("OTHER", rsi);
            draw(&mut charts[0], 90.0);
            frames(&mut layout, &mut charts);
            charts[1].set_symbol("SAMPLE");
            frames(&mut layout, &mut charts);
            let expected = if rsi { vec![90.0, 104.0] } else { vec![90.0] };
            assert_eq!(prices(&charts), [vec![90.0, 104.0], expected], "RSI {rsi}");
            assert_eq!(in_study_pane(&charts[1], 104.0), rsi, "RSI {rsi}");
            assert!(!in_study_pane(&charts[1], 90.0), "RSI {rsi}");
            // The RSI line, drawn first, stays linked.
            move_first(&mut charts[0], 95.0);
            frames(&mut layout, &mut charts);
            let expected = if rsi { vec![90.0, 95.0] } else { vec![90.0] };
            assert_eq!(
                prices(&charts),
                [vec![90.0, 95.0], expected],
                "linked, RSI {rsi}"
            );
        }
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
    /// in the price pane or a linked study pane, still linked, however often that happens. A
    /// restore or a load deletes no drawing of the other charts: without storage a restore
    /// replaces the copies a chart put aside on other symbols, and the chart takes the mirrored
    /// drawings again when it returns to them. A chart new to a slot starts without links.
    #[cfg(feature = "ui")]
    #[test]
    fn mirrored_drawings_restored_from_snapshots_stay_single_and_linked() {
        type Start = fn() -> (ChartLayout, [Chart; 2]);
        let starts: [(bool, Start); 2] = [
            (false, || mirrored_pair(false)),
            (true, || rsi_pair("SAMPLE", true)),
        ];
        for (study, start) in starts {
            let (mut layout, mut charts) = start();
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
                    "restart {restart}, study pane {study}"
                );
                assert_eq!(
                    charts.each_ref().map(|c| in_study_pane(c, 104.0)),
                    [study; 2],
                    "restart {restart}"
                );
            }
            move_first(&mut charts[0], 95.0);
            frames(&mut layout, &mut charts);
            assert_eq!(
                prices(&charts),
                [vec![95.0], vec![95.0]],
                "still linked, study pane {study}"
            );
        }

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

        // Restored from a snapshot of a third symbol, away or on show: the other chart keeps its
        // line, and the restored chart takes it with the move made meanwhile when it returns.
        let third = {
            let mut chart = bars(60);
            chart.set_symbol("THIRD");
            draw(&mut chart, 70.0);
            chart.snapshot().unwrap()
        };
        for away in [true, false] {
            let (mut layout, mut charts) = mirrored_pair(false);
            if away {
                charts[1].set_symbol("OTHER");
                frames(&mut layout, &mut charts);
            }
            let data = charts[1].bars(SeriesId::MAIN).to_vec();
            charts[1].restore(&third).unwrap();
            charts[1].set_data(SeriesId::MAIN, data);
            frames(&mut layout, &mut charts);
            move_first(&mut charts[0], 95.0);
            frames(&mut layout, &mut charts);
            charts[1].set_symbol("SAMPLE");
            frames(&mut layout, &mut charts);
            assert_eq!(
                prices(&charts),
                [vec![95.0], vec![95.0]],
                "third symbol, away {away}"
            );
        }

        // A chart new to the slot starts without links, also when its line takes the id of the
        // copy the chart before it put aside.
        let (mut layout, mut charts) = mirrored_pair(false);
        let copy = charts[1].drawings().drawings[0].id;
        charts[1].set_symbol("OTHER");
        frames(&mut layout, &mut charts);
        let mut chart = bars(60);
        chart.set_symbol("SAMPLE");
        assert_eq!(draw(&mut chart, 60.0), copy, "the id comes back");
        chart.set_symbol("OTHER");
        charts[1] = chart;
        frames(&mut layout, &mut charts);
        move_first(&mut charts[0], 95.0);
        frames(&mut layout, &mut charts);
        charts[1].set_symbol("SAMPLE");
        frames(&mut layout, &mut charts);
        assert_eq!(
            prices(&charts),
            [vec![60.0, 95.0], vec![60.0, 95.0]],
            "new chart"
        );

        // Drawings loaded into a chart are new: on show they join the others' drawings, which
        // stay as they are; away, the copy put aside on the left symbol stays linked.
        let file = |symbol: &str| {
            let mut chart = bars(60);
            chart.set_symbol(symbol);
            draw(&mut chart, 70.0);
            chart.save_drawings().unwrap().to_json().unwrap()
        };
        let (mut layout, mut charts) = mirrored_pair(false);
        let line = charts[0].drawings().drawings[0].id;
        charts[1].load_drawings(&file("SAMPLE")).unwrap();
        frames(&mut layout, &mut charts);
        assert_eq!(
            prices(&charts),
            [vec![70.0, 104.0], vec![70.0, 104.0]],
            "loaded on show"
        );
        assert!(
            charts[0]
                .drawings()
                .drawings
                .iter()
                .any(|d| d.id == line && d.chart_points[0].price == 104.0),
            "the first chart's line stays"
        );
        let (mut layout, mut charts) = mirrored_pair(false);
        charts[1].set_symbol("OTHER");
        frames(&mut layout, &mut charts);
        charts[1].load_drawings(&file("OTHER")).unwrap();
        frames(&mut layout, &mut charts);
        move_first(&mut charts[0], 95.0);
        frames(&mut layout, &mut charts);
        charts[1].set_symbol("SAMPLE");
        frames(&mut layout, &mut charts);
        assert_eq!(prices(&charts), [vec![95.0], vec![95.0]], "loaded away");
    }

    /// Two charts keep their drawings in one storage that keeps revisions, a third in its own.
    /// Keeping or editing a mirrored drawing on the third chart writes the shared storage once,
    /// which the other chart there reads: a second write would be refused as a conflict for the
    /// user to settle, and that chart would write nothing until then.
    // Saving and opening a layout needs `ui`.
    #[cfg(feature = "ui")]
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
                        .set_folder_sync(&top, Some(FolderSync::AllLayouts))
                );
            });
            frames(&mut session, &mut shown);
            assert_eq!(counts(&shown), [1, 1, 1], "mirrored");
            let copy = shown[1].drawings().drawings[0].id;
            shown[1].user_action(|chart| {
                if edit {
                    chart
                        .drawings_mut()
                        .edit_drawing(copy, |drawing| {
                            drawing.chart_points[0].price = 50.0;
                            drawing.chart_points[1].price = 52.0;
                        })
                        .unwrap();
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
            // The copy kept on its chart comes back with the chart's saved layout.
            let saved = shown.each_ref().map(|chart| chart.snapshot().unwrap());
            let mut restarted = charts();
            for (chart, saved) in restarted.iter_mut().zip(&saved) {
                chart.restore(saved).unwrap();
            }
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

    /// A chart reads its drawings again when they change in its storage elsewhere, here on a
    /// chart of another layout, as in another window. A read is no load: the change reaches the
    /// copy on the other chart of the layout as an edit, and each chart keeps one line.
    #[test]
    fn a_mirrored_drawing_changed_elsewhere_stays_single() {
        use crate::services::document::tests::Revised;
        use crate::widget::{ChartLayout, ChartLayoutMode};

        let (own, shared) = (Arc::new(Revised::default()), Arc::new(Revised::default()));
        let stored = |storage: &Arc<Revised>| {
            let mut chart = bars(60);
            chart.set_symbol("SAMPLE");
            chart.set_storage(storage.clone());
            chart
        };
        let mut layout = ChartLayout {
            mode: ChartLayoutMode::TwoHorizontal,
            ..Default::default()
        };
        layout.sync.sync_drawings = true;
        let mut window = ChartLayout::default();
        let mut charts = [stored(&own), stored(&shared)];
        let mut elsewhere = [stored(&shared)];
        frames(&mut layout, &mut charts);
        // The chart on the shared storage stores the line there, where the window reads it.
        charts[1].user_action(|chart| draw(chart, 104.0));
        frames(&mut layout, &mut charts);
        frames(&mut window, &mut elsewhere);
        assert_eq!(
            prices(&elsewhere),
            [vec![104.0]],
            "read from the shared storage"
        );
        elsewhere[0].user_action(|chart| move_first(chart, 95.0));
        frames(&mut window, &mut elsewhere);
        frames(&mut layout, &mut charts);
        assert_eq!(
            (prices(&charts), prices(&elsewhere)),
            ([vec![95.0], vec![95.0]], [vec![95.0]])
        );
    }

    #[cfg(feature = "studies")]
    struct StudyWindows {
        shared: Arc<crate::services::document::tests::Revised>,
        charts: [Chart; 2],
        writer: [Chart; 1],
        pair: ChartLayout,
        window: ChartLayout,
        owners: [SeriesId; 3],
        id: usize,
    }

    #[cfg(feature = "studies")]
    impl StudyWindows {
        fn new(peer_has_storage: bool) -> Self {
            use crate::services::document::tests::Revised;
            use crate::studies::RSI;
            let shared = Arc::new(Revised::default());
            let mut charts = [bars(60), bars(60)];
            let mut writer = [bars(60)];
            for c in charts.iter_mut().chain(&mut writer) {
                c.set_symbol("SAMPLE");
            }
            charts[0].set_storage(shared.clone());
            writer[0].set_storage(shared.clone());
            if peer_has_storage {
                charts[1].set_storage(Arc::new(Revised::default()));
            }
            let [bridge, peer] = charts
                .each_mut()
                .map(|c| c.add_indicator(Box::new(RSI::new(14))));
            // The bridge's numeric RSI(14) id instead names RSI(7) on the writer.
            writer[0].add_indicator(Box::new(RSI::new(7)));
            let study = writer[0].add_indicator(Box::new(RSI::new(14)));
            let mut pair = ChartLayout {
                mode: super::super::ChartLayoutMode::TwoHorizontal,
                ..Default::default()
            };
            pair.sync.sync_drawings = true;
            let mut window = ChartLayout::default();
            frames(&mut pair, &mut charts);
            frames(&mut window, &mut writer);
            let mut drawing = line("RSI line");
            drawing.pane = Some(study.0);
            drawing.folder = vec!["Plan".into()];
            drawing.folder_sync = vec![Some(FolderSync::AllLayouts)];
            let id = writer[0].user_action(|c| c.drawings_mut().add_drawing(drawing).unwrap());
            frames(&mut window, &mut writer);
            frames(&mut pair, &mut charts);
            Self {
                shared,
                charts,
                writer,
                pair,
                window,
                owners: [bridge, peer, study],
                id,
            }
        }

        fn settle(&mut self) {
            frames(&mut self.pair, &mut self.charts);
            frames(&mut self.window, &mut self.writer);
            frames(&mut self.pair, &mut self.charts);
        }

        fn hide_and_remove(&mut self, owner: SeriesId) {
            self.charts[0]
                .drawings_mut()
                .set_shared_drawings_hidden(true);
            assert!(self.charts[0].user_action(|c| c.remove_series(owner)));
            self.settle();
        }

        fn check_peers(&self, price: f64) {
            for (chart, owner) in [
                (&self.charts[1], self.owners[1]),
                (&self.writer[0], self.owners[2]),
            ] {
                let mut lines = chart
                    .drawings()
                    .drawings
                    .iter()
                    .filter(|d| d.label.as_deref() == Some("RSI line"));
                let line = lines.next().expect("one RSI line");
                assert!(
                    lines.next().is_none(),
                    "duplicate RSI line at {price} for {owner:?}"
                );
                assert_eq!(line.chart_points[0].price, price);
                assert_eq!(
                    chart.drawing_pane(line.id).unwrap(),
                    chart.series_pane(owner).unwrap()
                );
            }
        }
    }

    #[cfg(feature = "studies")]
    #[test]
    fn shared_drawing_filter_keeps_the_peers_pane_across_storage_reads() {
        for peer_has_storage in [false, true] {
            let StudyWindows {
                shared,
                mut charts,
                mut writer,
                mut pair,
                mut window,
                owners: [bridge, peer, _],
                id,
            } = StudyWindows::new(peer_has_storage);
            let peer_pane = charts[1].series_pane(peer).unwrap();
            let peer_id = charts[1].drawings().drawings[0].id;
            let check = |c: &Chart, pane, price| {
                let mut lines = c
                    .drawings()
                    .drawings
                    .iter()
                    .filter(|d| d.label.as_deref() == Some("RSI line"));
                let line = lines.next().expect("mirrored RSI line");
                assert!(lines.next().is_none());
                assert_eq!(line.id, peer_id);
                assert_eq!(c.drawing_pane(line.id), Some(pane));
                assert_eq!(line.chart_points[0].price, price);
            };
            let original = shared_records(&shared)[0].clone();
            assert!(original["pane"].is_object());
            charts[0].drawings_mut().set_shared_drawings_hidden(true);
            // A line of the bridge's own whose study goes before the line is first written.
            charts[0].user_action(|c| {
                let mut drawing = line("Bridge line");
                drawing.pane = Some(bridge.0);
                drawing.folder = vec!["Plan".into()];
                drawing.folder_sync = vec![Some(FolderSync::AllLayouts)];
                c.drawings_mut().add_drawing(drawing).unwrap();
            });
            assert!(charts[0].user_action(|c| c.remove_series(bridge)));
            frames(&mut pair, &mut charts);
            check(&charts[1], peer_pane, 10.);
            // The writer reads that line before it writes, so that its writes are not refused.
            frames(&mut window, &mut writer);

            // An unrelated write replaces the bridge's manager while it is still filtered.
            writer[0].user_action(|c| draw(c, 11.));
            frames(&mut window, &mut writer);
            frames(&mut pair, &mut charts);
            check(&charts[1], peer_pane, 10.);
            let own = |c: &Chart| {
                let manager = c.drawings();
                let line = manager
                    .drawings
                    .iter()
                    .find(|d| d.label.as_deref() == Some("Bridge line"));
                c.drawing_pane(line.expect("bridge line").id)
            };
            assert_eq!(own(&charts[1]), Some(peer_pane));
            charts[0].drawings_mut().set_shared_drawings_hidden(false);
            frames(&mut pair, &mut charts);
            // The read kept the removed study, whose lines the price pane draws.
            let manager = charts[0].drawings();
            for label in ["RSI line", "Bridge line"] {
                let line = manager
                    .drawings
                    .iter()
                    .find(|d| d.label.as_deref() == Some(label));
                let painted = manager.painted_range(line.unwrap().id);
                assert!(painted.is_some_and(|r| !r.is_empty()), "{label}");
            }
            writer[0].user_action(|c| {
                c.drawings_mut()
                    .edit_drawing(id, |d| d.chart_points[0].price = 9.5)
                    .unwrap()
            });
            frames(&mut window, &mut writer);
            frames(&mut pair, &mut charts);
            check(&charts[1], peer_pane, 9.5);
            let mut document: serde_json::Value =
                serde_json::from_str(&shared.get("SAMPLE").unwrap()).unwrap();
            let record = document["drawings"]
                .as_array_mut()
                .unwrap()
                .iter_mut()
                .find(|r| r["id"] == original["id"])
                .unwrap();
            assert_eq!(record["pane"], original["pane"]);
            assert_eq!(record["created_at"], original["created_at"]);
            // Neither a new drawing under the same id nor a move of the original inherits
            // the local removal. Both must reach the peer as a price-pane drawing.
            if peer_has_storage {
                record["created_at"] = (original["created_at"].as_i64().unwrap() + 1).into();
            } else {
                record["pane"]["rank"] = 1.into();
            }
            shared.put("SAMPLE", &document.to_string());
            frames(&mut pair, &mut charts);
            check(&charts[1], charts[1].main_series_pane(), 9.5);
        }
    }

    #[cfg(all(feature = "ui", feature = "studies"))]
    #[test]
    fn shared_drawing_filter_keeps_removed_panes_through_copy_undo_and_snapshot_edits() {
        for scenario in [
            "kept copy",
            "different pane in kept copy",
            "snapshot",
            "snapshot after removal",
        ] {
            let mut w = StudyWindows::new(false);
            let original = shared_records(&w.shared)[0].clone();
            let expected = if scenario == "snapshot after removal" {
                w.hide_and_remove(w.owners[0]);
                let saved = w.charts[0].snapshot().unwrap();
                assert!(w.charts[0].restore(&saved).unwrap().is_empty());
                w.settle();
                10.
            } else if scenario == "snapshot" {
                let saved = w.charts[0].snapshot().unwrap();
                assert!(w.charts[0].restore(&saved).unwrap().is_empty());
                let restored = w.charts[0]
                    .indicators()
                    .find(|(_, s)| s.name() == "RSI")
                    .unwrap()
                    .0;
                // Edit between the snapshot restore and the first frame that reads storage.
                w.charts[0].user_action(|c| move_first(c, 9.5));
                w.hide_and_remove(restored);
                9.5
            } else {
                w.hide_and_remove(w.owners[0]);
                let name = "SAMPLE (local copy)";
                let mut copy: serde_json::Value =
                    serde_json::from_str(&w.shared.get("SAMPLE").unwrap()).unwrap();
                copy["drawings"][0]["points"][0]["price"] = 11.5.into();
                if scenario == "different pane in kept copy" {
                    copy["drawings"][0]["pane"]["rank"] = 1.into();
                }
                w.shared.put(name, &copy.to_string());
                w.charts[0].list_local_copies();
                w.settle();
                w.charts[0].load_local_copy(name);
                w.settle();
                assert_eq!(prices(&w.charts)[0], [11.5]);
                assert!(w.charts[0].undo());
                w.settle();
                // The study comes back and goes again while the copy's pane is still unshown.
                assert!(w.charts[0].undo());
                w.settle();
                assert!(w.charts[0].redo());
                w.settle();
                10.
            };
            w.check_peers(expected);
            w.charts[0].drawings_mut().set_shared_drawings_hidden(false);
            // Both the first unfiltered write and the subsequent write from the chart
            // with RSI(7) must store the canonical RSI(14) pane, not the bridge's id.
            for from_bridge in [true, false] {
                let c = if from_bridge {
                    &mut w.charts[0]
                } else {
                    &mut w.writer[0]
                };
                c.user_action(|c| draw(c, 11.));
                w.settle();
                w.check_peers(expected);
                let written = shared_records(&w.shared);
                let r = written
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|r| r["id"] == original["id"])
                    .unwrap();
                for key in ["pane", "created_at"] {
                    assert_eq!(
                        r[key], original[key],
                        "{key}, {scenario}, bridge={from_bridge}"
                    );
                }
            }
        }
    }

    #[cfg(feature = "studies")]
    #[test]
    fn shared_drawing_filter_keeps_removed_panes_across_remote_deletion_and_undo() {
        for (remove_study, trip) in [
            (false, None),
            (true, None),
            (false, Some("OTHER")),
            (false, Some("")),
        ] {
            let mut w = StudyWindows::new(false);
            w.hide_and_remove(w.owners[0]);
            w.check_peers(10.);
            w.writer[0].user_action(|c| {
                if remove_study {
                    assert!(c.remove_series(w.owners[2]));
                } else {
                    c.drawings_mut().delete_drawing(w.id);
                }
            });
            w.settle();
            assert!(shared_records(&w.shared).as_array().unwrap().is_empty());
            assert!(w.charts.iter().all(|c| c.drawings().drawings.is_empty()));
            // The bridge leaves the symbol while it holds none of its drawings.
            if let Some(symbol) = trip {
                w.charts[0].set_symbol(symbol);
                w.settle();
                w.charts[0].set_symbol("SAMPLE");
                w.settle();
            }
            assert!(w.writer[0].undo());
            w.settle();
            w.check_peers(10.);
            w.charts[0].drawings_mut().set_shared_drawings_hidden(false);
            w.settle();
            w.check_peers(10.);
        }
    }

    #[cfg(feature = "studies")]
    #[test]
    fn a_study_removed_on_another_symbol_keeps_the_stored_pane_of_its_drawings() {
        for hidden in [false, true] {
            let mut w = StudyWindows::new(false);
            let original = shared_records(&w.shared)[0].clone();
            let record = |w: &StudyWindows| {
                let written = shared_records(&w.shared);
                let found =
                    (written.as_array().unwrap().iter()).find(|r| r["id"] == original["id"]);
                found.expect("stored RSI line").clone()
            };
            w.charts[0]
                .drawings_mut()
                .set_shared_drawings_hidden(hidden);
            w.charts[0].set_symbol("OTHER");
            w.settle();
            assert!(w.charts[0].user_action(|c| c.remove_series(w.owners[0])));
            w.settle();
            w.charts[0].set_symbol("SAMPLE");
            w.settle();
            // Any change of the symbol's drawings writes the line's record again.
            w.charts[0].user_action(|c| draw(c, 11.));
            w.settle();
            w.check_peers(10.);
            let written = record(&w);
            for key in ["pane", "created_at"] {
                assert_eq!(written[key], original[key], "{key}, hidden {hidden}");
            }
            let local = &written["drawing"]["pane"];
            assert!(local.is_null(), "hidden {hidden}: {local}");
            if hidden {
                continue;
            }
            let manager = w.charts[0].drawings();
            let line = manager
                .drawings
                .iter()
                .find(|d| d.label.as_deref() == Some("RSI line"));
            let line = line.expect("bridge RSI line");
            // The price pane, the bridge's only pane now, draws it.
            assert!(manager.owns_pane(line));
            let line = line.id;
            w.charts[0].user_action(|c| {
                c.drawings_mut()
                    .edit_drawing(line, |d| d.chart_points[0].price = 9.5)
                    .unwrap()
            });
            w.settle();
            w.check_peers(9.5);
            let edited = record(&w);
            assert_eq!(edited["pane"], original["pane"]);
            assert!(edited["drawing"]["pane"].is_null());
        }
    }

    #[cfg(feature = "ui")]
    #[test]
    fn chart_local_placement_stays_private_through_undo_and_redo_until_shared() {
        use crate::services::document::tests::Revised;

        let host = Arc::new(Revised::default());
        let mut charts: [Chart; 2] = std::array::from_fn(|_| {
            let mut chart = bars(60);
            chart.set_symbol("SAMPLE");
            chart.set_storage(host.clone());
            chart
        });
        let mut layouts = [ChartLayout::default(), ChartLayout::default()];
        let mut settle = |charts: &mut [Chart; 2]| {
            for _ in 0..3 {
                for (layout, chart) in layouts.iter_mut().zip(&mut *charts) {
                    frames(layout, std::array::from_mut(chart));
                }
            }
        };
        let place = |chart: &mut Chart, price| {
            chart.user_action(|chart| {
                chart.drawings_mut().start_drawing_with_coords(
                    DrawingToolType::HorizontalLine,
                    egui::pos2(2.0, price),
                    |x| x,
                    f64::from,
                );
            });
        };
        charts[0].drawings_mut().options.local_placement_folder = Some("Notes".into());
        settle(&mut charts);
        place(&mut charts[0], 104.0);
        let mut check_private = |charts: &mut [Chart; 2], local: &[f64]| {
            settle(charts);
            let [mine, other] = prices(charts);
            assert_eq!(mine, local);
            assert!(other.is_empty(), "visible on the other chart: {other:?}");
            if let Some(json) = host.get("SAMPLE") {
                let (stored, skipped) = DrawingStorage::from_json(&json).unwrap();
                assert!(skipped.is_empty() && stored.drawings().is_empty());
            }
        };
        check_private(&mut charts, &[104.0]);
        assert!(charts[0].undo());
        check_private(&mut charts, &[]);
        assert!(charts[0].redo());
        check_private(&mut charts, &[104.0]);

        // Explicit sharing takes effect. It must not make the next placement shared.
        charts[0].user_action(|chart| {
            assert!(
                chart
                    .drawings_mut()
                    .set_folder_sync(&["Notes".to_owned()], Some(FolderSync::AllLayouts))
            );
        });
        settle(&mut charts);
        assert_eq!(prices(&charts), [vec![104.0], vec![104.0]]);
        place(&mut charts[0], 108.0);
        place(&mut charts[0], 110.0);
        settle(&mut charts);
        for (price, folder, scope) in [
            (104.0, "Notes", FolderSync::AllLayouts),
            (108.0, "Notes (2)", FolderSync::Chart),
            (110.0, "Notes (2)", FolderSync::Chart),
        ] {
            let line = charts[0]
                .drawings()
                .drawings
                .iter()
                .find(|d| d.chart_points[0].price == price)
                .unwrap();
            assert_eq!(line.folder, [folder], "line at {price}");
            assert_eq!(line.folder_sync_mode(), Some(scope), "line at {price}");
        }
        assert_eq!(prices(&charts)[1], [104.0]);

        let original = charts[0]
            .drawings()
            .drawings
            .iter()
            .find(|d| d.folder == ["Notes"])
            .unwrap()
            .id;
        // Copies of the shared original stay in the numbered chart-only folder.
        let mut check_local = |charts: &mut [Chart; 2], local: &[f64]| {
            settle(charts);
            assert_eq!(prices(charts), [local.to_vec(), vec![104.0]]);
            let (stored, skipped) =
                DrawingStorage::from_json(&host.get("SAMPLE").unwrap()).unwrap();
            assert!(skipped.is_empty());
            let [record] = stored.drawings() else {
                panic!("expected only the shared original")
            };
            assert_eq!(record.points[0].price, 104.0);
            assert_eq!(record.drawing.folder, ["Notes"]);
            for line in &charts[0].drawings().drawings {
                let (folder, scope) = if line.id == original {
                    ("Notes", FolderSync::AllLayouts)
                } else {
                    ("Notes (2)", FolderSync::Chart)
                };
                assert_eq!(line.folder, [folder]);
                assert_eq!(line.folder_sync_mode(), Some(scope));
            }
        };
        let shared = charts[1].drawings().drawings[0].id;
        let local = charts[0]
            .drawings()
            .drawings
            .iter()
            .find(|d| d.chart_points[0].price == 108.0)
            .unwrap()
            .id;
        let drag = |chart: &mut Chart, ids: &[usize], copy| {
            chart.user_action(|chart| {
                let manager = chart.drawings_mut();
                manager.select_drawings(ids.iter().copied());
                manager.start_drag_body(ids[0], egui::Pos2::ZERO);
                if copy {
                    manager.copy_dragged_drawings();
                }
                manager.update_drag_handle(
                    egui::pos2(0.0, 1.0),
                    |x| x,
                    f64::from,
                    |p, from, to| p + f64::from(to - from),
                );
                manager.end_drag_handle();
            });
        };
        // In the group the shared line is not the grabbed one: every copy needs placement.
        for (ids, expected) in [
            (&[original][..], &[104.0, 105.0, 108.0, 110.0][..]),
            (&[local, original], &[104.0, 105.0, 108.0, 109.0, 110.0]),
        ] {
            drag(&mut charts[0], ids, true);
            check_local(&mut charts, expected);
            assert!(charts[0].undo());
            check_local(&mut charts, &[104.0, 108.0, 110.0]);
            assert!(charts[0].redo());
            check_local(&mut charts, expected);
            assert!(charts[0].undo());
            check_local(&mut charts, &[104.0, 108.0, 110.0]);
        }
        charts[1].drawings_mut().select(shared);
        let copied = charts[1].copy_selected_drawings().unwrap().unwrap();
        charts[0].paste_drawings(&copied).unwrap();
        let pasted = charts[0].drawings().drawings.last().unwrap();
        assert_eq!(pasted.folder, ["Notes (2)"]);
        assert_eq!(pasted.folder_sync_mode(), Some(FolderSync::Chart));
        check_local(&mut charts, &[104.0, 104.0, 108.0, 110.0]);
        assert!(charts[0].undo());
        check_local(&mut charts, &[104.0, 108.0, 110.0]);
        assert!(charts[0].redo());
        check_local(&mut charts, &[104.0, 104.0, 108.0, 110.0]);
        charts[0].user_action(|chart| chart.drawings_mut().duplicate_drawing(original));
        let cloned = charts[0].drawings().drawings.last().unwrap();
        assert_eq!(cloned.folder, ["Notes (2)"]);
        assert_eq!(cloned.folder_sync_mode(), Some(FolderSync::Chart));
        check_local(&mut charts, &[104.0, 104.0, 104.0, 108.0, 110.0]);

        // Moving the original still syncs; a drag-copy on a chart without the option does too.
        drag(&mut charts[0], &[original], false);
        settle(&mut charts);
        assert_eq!(prices(&charts)[1], [105.0]);
        assert!(charts[0].undo());
        settle(&mut charts);
        drag(&mut charts[1], &[shared], true);
        settle(&mut charts);
        assert_eq!(
            prices(&charts),
            [
                vec![104.0, 104.0, 104.0, 105.0, 108.0, 110.0],
                vec![104.0, 105.0]
            ]
        );
        assert!(charts[1].undo());
        settle(&mut charts);

        charts[1].user_action(|chart| chart.drawings_mut().delete_drawing(shared));
        settle(&mut charts);
        assert_eq!(prices(&charts), [vec![104.0, 104.0, 108.0, 110.0], vec![]]);

        // Same symbol, no opt-in: placement on the other chart still goes to both.
        place(&mut charts[1], 102.0);
        settle(&mut charts);
        assert_eq!(
            prices(&charts),
            [vec![102.0, 104.0, 104.0, 108.0, 110.0], vec![102.0]]
        );
    }

    fn shared_layouts() -> (
        Arc<crate::services::document::tests::Revised>,
        [Chart; 2],
        [ChartLayout; 2],
    ) {
        use crate::services::document::tests::Revised;

        let host = Arc::new(Revised::default());
        let charts = std::array::from_fn(|_| {
            let mut chart = bars(60);
            chart.set_symbol("SAMPLE");
            chart.set_storage(host.clone());
            chart
        });
        (host, charts, Default::default())
    }

    fn settle_pair(layouts: &mut [ChartLayout; 2], charts: &mut [Chart; 2]) {
        for _ in 0..2 {
            for (layout, chart) in layouts.iter_mut().zip(&mut *charts) {
                frames(layout, std::array::from_mut(chart));
            }
        }
    }

    #[test]
    fn shared_drawing_filter_follows_peer_moves_and_deletions_without_writing() {
        let (host, mut charts, mut layouts) = shared_layouts();
        let mut settle = |charts: &mut [Chart; 2]| settle_pair(&mut layouts, charts);
        let painted = |chart: &Chart| {
            let manager = chart.drawings();
            let mut prices: Vec<_> = manager
                .drawings
                .iter()
                .filter(|d| manager.painted_range(d.id).is_some_and(|r| !r.is_empty()))
                .map(|d| d.chart_points[0].price)
                .collect();
            prices.sort_by(f64::total_cmp);
            prices
        };
        settle(&mut charts);
        charts[0].user_action(|chart| {
            for (folder, scope, price) in [
                ("Notes", FolderSync::Chart, 9.25),
                ("Layout", FolderSync::Layout, 9.5),
            ] {
                let id = draw(chart, price);
                let path = [folder.to_owned()];
                assert!(chart.drawings_mut().move_to_folder(&[id], &[], &path));
                assert!(chart.drawings_mut().set_folder_sync(&path, Some(scope)));
            }
            draw(chart, 10.0);
        });
        settle(&mut charts);
        assert_eq!(prices(&charts), [vec![9.25, 9.5, 10.0], vec![10.0]]);
        assert_eq!(painted(&charts[0]), [9.25, 9.5, 10.0]);
        assert_eq!(painted(&charts[1]), [10.0]);

        let revision = host.revision("SAMPLE").unwrap();
        charts[0].user_action(|c| c.drawings_mut().set_shared_drawings_hidden(true));
        settle(&mut charts);
        assert_eq!(painted(&charts[0]), [9.25, 9.5]);
        assert_eq!(painted(&charts[1]), [10.0]);
        assert_eq!(host.revision("SAMPLE"), Some(revision));
        for redo in [false, true] {
            assert!(if redo {
                charts[0].redo()
            } else {
                charts[0].undo()
            });
            settle(&mut charts);
            assert_eq!(painted(&charts[0]).contains(&10.0), !redo);
            assert_eq!(painted(&charts[1]), [10.0]);
            assert_eq!(host.revision("SAMPLE"), Some(revision));
        }

        // A host can also set the filter outside history. A mixed drawing action waits
        // intact (including a redo of a drawing no longer present) until it is shown.
        assert!(charts[0].undo()); // show the drawings again
        charts[0].drawings_mut().set_shared_drawings_hidden(true);
        assert!(!charts[0].undo());
        charts[0].drawings_mut().set_shared_drawings_hidden(false);
        assert!(charts[0].undo()); // undo placement
        settle(&mut charts);
        assert!(prices(&charts).iter().all(Vec::is_empty));
        let empty = host.get("SAMPLE");
        charts[0].drawings_mut().set_shared_drawings_hidden(true);
        assert!(!charts[0].redo());
        settle(&mut charts);
        assert!(prices(&charts)[1].is_empty());
        assert_eq!(host.get("SAMPLE"), empty);
        charts[0].drawings_mut().set_shared_drawings_hidden(false);
        assert!(charts[0].redo()); // restore placement
        assert!(charts[0].redo()); // hide the shared line
        settle(&mut charts);
        assert_eq!(painted(&charts[0]), [9.25, 9.5]);
        assert_eq!(painted(&charts[1]), [10.0]);

        charts[1].user_action(|chart| move_first(chart, 10.5));
        settle(&mut charts);
        assert_eq!(prices(&charts)[0], [9.25, 9.5, 10.5]);
        assert_eq!(painted(&charts[0]), [9.25, 9.5]);
        let moved = host.revision("SAMPLE").unwrap();
        assert!(moved > revision);
        charts[0].drawings_mut().set_shared_drawings_hidden(false);
        settle(&mut charts);
        assert_eq!(painted(&charts[0]), [9.25, 9.5, 10.5]);
        assert_eq!(host.revision("SAMPLE"), Some(moved));

        charts[0].drawings_mut().set_shared_drawings_hidden(true);
        let shared = charts[1].drawings().drawings[0].id;
        charts[1].user_action(|chart| chart.drawings_mut().delete_drawing(shared));
        settle(&mut charts);
        assert!(charts[0].drawings().shared_drawings_hidden());
        charts[0].drawings_mut().set_shared_drawings_hidden(false);
        settle(&mut charts);
        assert_eq!(prices(&charts), [vec![9.25, 9.5], vec![]]);
        assert_eq!(painted(&charts[0]), [9.25, 9.5]);
    }

    fn shared_records(host: &crate::services::document::tests::Revised) -> serde_json::Value {
        let (doc, skipped) = DrawingStorage::from_json(&host.get("SAMPLE").unwrap()).unwrap();
        assert!(skipped.is_empty());
        serde_json::to_value(doc.drawings()).unwrap()
    }

    #[cfg(feature = "ui")]
    #[test]
    fn shared_drawing_filter_keeps_bulk_tree_edits_local() {
        use crate::ui::widget_bar::{ObjectTreeAction as Action, ObjectTreePanel, SourceItem};

        let (host, mut charts, mut layouts) = shared_layouts();
        settle_pair(&mut layouts, &mut charts);
        let notes = vec!["Notes".to_owned()];
        let shared = vec!["Shared".to_owned()];
        charts[1].user_action(|chart| {
            let filed = draw(chart, 10.5);
            let root = draw(chart, 10.0);
            draw(chart, 9.75); // unlocked, unfiled: Remove All would otherwise delete it
            let m = chart.drawings_mut();
            assert!(m.move_to_folder(&[filed], &[], &shared));
            assert!(m.set_folder_sync(&shared, Some(FolderSync::AllLayouts)));
            m.edit_drawing(root, |d| {
                d.locked = true;
                d.visible = false;
            })
            .unwrap();
        });
        settle_pair(&mut layouts, &mut charts);
        // The writer and document timestamps may change; the peer's complete records must not.
        let stored = shared_records(&host);
        let layers = |c: &Chart| {
            c.drawings()
                .drawings
                .iter()
                .map(|d| (d.chart_points[0].price, d.series_layer, d.z_order))
                .collect::<Vec<_>>()
        };
        let peer_layers = layers(&charts[1]);
        charts[0].user_action(|chart| {
            let note = draw(chart, 9.5);
            let m = chart.drawings_mut();
            assert!(m.move_to_folder(&[note], &[], &notes));
            assert!(m.set_folder_sync(&notes, Some(FolderSync::Chart)));
        });
        settle_pair(&mut layouts, &mut charts);
        charts[0].drawings_mut().set_shared_drawings_hidden(true);
        let mut tree = ObjectTreePanel::new();
        tree.state_mut()
            .folders
            .extend([notes.clone(), shared.clone()]);
        let note = charts[0].drawings().listed_drawings().next().unwrap().id;
        let top_series = charts[0].main_series_order().len().checked_sub(1).unwrap();
        for (action, visible, locked) in [
            (Action::MoveDown(note), true, false),
            (Action::MoveUp(note), true, false),
            (Action::LockAll, true, true),
            (Action::UnlockAll, true, false),
            (Action::HideAll, false, false),
            (Action::ShowAll, true, false),
            (Action::SetFolderVisible(shared.clone(), false), true, false),
            (Action::SetFolderLocked(shared, true), true, false),
            (Action::SetFolderVisible(notes.clone(), false), false, false),
            (Action::SetFolderLocked(notes.clone(), true), false, true),
            (Action::SetFolderLocked(notes, false), false, false),
            (Action::RemoveAllDrawings, false, false),
        ] {
            let rows: Vec<_> = charts[0]
                .drawings()
                .listed_drawings()
                .map(|d| SourceItem::drawing(d.id, d.tool_type, d.color32()))
                .collect();
            assert_eq!(rows.len(), 1, "{action:?}");
            assert!(charts[0].user_action(|chart| tree.apply_to_chart(chart, &action, &rows)));
            settle_pair(&mut layouts, &mut charts);
            let mut listed = charts[0].drawings().listed_drawings();
            if action == Action::RemoveAllDrawings {
                assert!(listed.next().is_none());
            } else {
                let note = listed.next().unwrap();
                assert_eq!(
                    (note.chart_points[0].price, note.visible, note.locked),
                    (9.5, visible, locked),
                    "{action:?}"
                );
                if matches!(action, Action::MoveDown(_) | Action::MoveUp(_)) {
                    // A single step crosses the top series, not one of the hidden lines.
                    assert_eq!(
                        note.series_layer,
                        matches!(action, Action::MoveDown(_)).then_some(top_series)
                    );
                }
            }
            let peer = charts[1].drawings();
            assert_eq!(prices(&charts)[1], [9.75, 10.0, 10.5], "{action:?}");
            for d in &peer.drawings {
                let root = d.chart_points[0].price == 10.0;
                assert_eq!((d.visible, d.locked), (!root, root), "{action:?}");
            }
            assert_eq!(layers(&charts[1]), peer_layers, "{action:?}");
            assert_eq!(shared_records(&host), stored, "{action:?}");
        }
        assert!(charts[0].undo());
        settle_pair(&mut layouts, &mut charts);
        assert_eq!(
            prices(&charts),
            [vec![9.5, 9.75, 10.0, 10.5], vec![9.75, 10.0, 10.5]]
        );
        assert_eq!(shared_records(&host), stored);
    }

    #[cfg(feature = "studies")]
    #[test]
    fn shared_drawing_filter_keeps_a_removed_studys_line_in_storage() {
        use crate::drawings::Drawing;
        use crate::studies::RSI;

        for remove_pane in [false, true] {
            let (host, mut charts, mut layouts) = shared_layouts();
            let [mine, peer] = charts
                .each_mut()
                .map(|c| c.add_indicator(Box::new(RSI::new(14))));
            settle_pair(&mut layouts, &mut charts);
            let pane = charts[0].series_pane(mine).unwrap();
            // Removing the study must still delete its local drawing. That deletion also
            // forces a write from the chart on which the peer's drawing is hidden.
            charts[0].user_action(|c| {
                let id = draw(c, 45.0);
                assert!(c.move_drawing_to_pane(id, pane));
                let path = ["Notes".to_owned()];
                assert!(c.drawings_mut().move_to_folder(&[id], &[], &path));
                assert!(
                    c.drawings_mut()
                        .set_folder_sync(&path, Some(FolderSync::Chart))
                );
            });
            settle_pair(&mut layouts, &mut charts);
            charts[1].user_action(|c| {
                let id = draw(c, 55.0);
                assert!(c.move_drawing_to_pane(id, c.series_pane(peer).unwrap()));
            });
            settle_pair(&mut layouts, &mut charts);
            let before = shared_records(&host);
            let stored_line = &before[0];
            assert!(stored_line["pane"].is_object());
            charts[0].drawings_mut().set_shared_drawings_hidden(true);
            for step in 0..3 {
                match step {
                    0 => assert!(charts[0].user_action(|c| if remove_pane {
                        c.remove_pane(pane)
                    } else {
                        c.remove_series(mine)
                    })),
                    1 => assert!(charts[0].undo()),
                    _ => assert!(charts[0].redo()),
                }
                settle_pair(&mut layouts, &mut charts);
                let restored = step == 1;
                assert_eq!(charts[0].series_pane(mine), restored.then_some(pane));
                let [local, other] = prices(&charts);
                assert_eq!(
                    local.as_slice(),
                    if restored {
                        &[45., 55.][..]
                    } else {
                        &[55.][..]
                    }
                );
                assert_eq!(other, [55.]);
                let line = |d: &&Drawing| d.chart_points[0].price == 55.0;
                let local_line = charts[0].drawings().drawings.iter().find(line).unwrap();
                assert_eq!(local_line.pane, Some(mine.0));
                assert_eq!(
                    charts[0].drawing_pane(local_line.id),
                    Some(if restored {
                        pane
                    } else {
                        charts[0].main_series_pane()
                    })
                );
                assert_eq!(charts[1].drawings().drawings[0].pane, Some(peer.0));
                let written = shared_records(&host);
                assert_eq!(written.as_array().unwrap().len(), 1);
                // Numeric study ids belong to the writer; the stored study identity, the
                // drawing's own id and the anchors must still describe the peer's RSI line.
                for key in ["pane", "id", "points", "created_at", "modified_at"] {
                    assert_eq!(written[0][key], stored_line[key], "{key}, step {step}");
                }
            }
        }
    }

    #[cfg(feature = "studies")]
    #[test]
    fn shared_drawing_filter_never_writes_a_removed_price_pane_owner() {
        use crate::studies::SMA;

        let (host, mut charts, mut layouts) = shared_layouts();
        let average = charts[0].add_indicator(Box::new(SMA::new(5)));
        assert_eq!(
            charts[0].series_pane(average),
            Some(charts[0].main_series_pane())
        );
        charts[0].user_action(|c| {
            let mut drawing = line("average");
            drawing.pane = Some(average.0);
            c.drawings_mut().add_drawing(drawing).unwrap();
        });
        settle_pair(&mut layouts, &mut charts);
        assert_eq!(shared_records(&host)[0]["drawing"]["pane"], average.0);
        charts[0].drawings_mut().set_shared_drawings_hidden(true);
        // The price pane has no stored identity, so no removed pane is remembered for the line.
        assert!(charts[0].user_action(|c| c.remove_series(average)));
        charts[0].user_action(|c| {
            let mut note = line("local");
            note.folder = vec!["Notes".into()];
            note.folder_sync = vec![Some(FolderSync::Chart)];
            c.drawings_mut().add_drawing(note).unwrap();
        });
        settle_pair(&mut layouts, &mut charts);
        let written = shared_records(&host);
        assert_eq!(written.as_array().unwrap().len(), 1);
        assert_eq!(written[0]["drawing"]["label"], "average");
        assert!(written[0]["drawing"]["pane"].is_null());
        assert!(written[0].get("pane").is_none());
    }

    #[cfg(feature = "studies")]
    #[test]
    fn shared_drawing_filter_keeps_the_last_stored_pane_after_study_edits() {
        use crate::studies::{InputValue, RSI, SMA};
        use crate::widget::VisualOrder;

        for edit in [
            "length",
            "move in",
            "move out",
            "order",
            "swap",
            "remove above",
        ] {
            let (host, mut charts, mut layouts) = shared_layouts();
            let studies = charts.each_mut().map(|c| {
                let above = c.add_indicator(Box::new(RSI::new(14)));
                let owner = c.add_indicator(Box::new(RSI::new(14)));
                let other = c.add_indicator(Box::new(SMA::new(5)));
                if matches!(edit, "move out" | "order") {
                    assert!(c.move_indicator_to_pane(other, owner));
                }
                (above, owner, other)
            });
            settle_pair(&mut layouts, &mut charts);
            // Exercise both a pane read from the peer and one first written here.
            let source = usize::from(edit != "swap");
            charts[source].user_action(|c| {
                let id = draw(c, 55.0);
                assert!(c.move_drawing_to_pane(id, c.series_pane(studies[source].1).unwrap()));
            });
            settle_pair(&mut layouts, &mut charts);
            let before = shared_records(&host);
            let revision = host.revision("SAMPLE").unwrap();
            let (above, owner, other) = studies[0];
            let pane = charts[0].series_pane(owner).unwrap();
            charts[0].drawings_mut().set_shared_drawings_hidden(true);
            charts[0].user_action(|c| {
                match edit {
                    "length" => c
                        .indicator_mut(owner)
                        .unwrap()
                        .set_input("period", InputValue::Length(21))
                        .unwrap(),
                    "move in" => assert!(c.move_indicator_to_pane(other, owner)),
                    "move out" => assert!(c.move_indicator_to_pane(other, other)),
                    "order" => assert!(c.reorder_indicator(other, VisualOrder::SendToBack)),
                    "swap" => assert!(c.swap_panes(c.series_pane(above).unwrap(), pane)),
                    "remove above" => assert!(c.remove_series(above)),
                    _ => unreachable!(),
                }
                assert_ne!(
                    serde_json::to_value(c.stored_pane(pane)).unwrap(),
                    before[0]["pane"],
                    "{edit}"
                );
                let mut note = line("local");
                note.folder = vec!["Notes".into()];
                note.folder_sync = vec![Some(FolderSync::Chart)];
                c.drawings_mut().add_drawing(note).unwrap();
            });
            settle_pair(&mut layouts, &mut charts);
            assert!(
                host.revision("SAMPLE").unwrap() > revision,
                "{edit}: filtered chart wrote"
            );
            let written = shared_records(&host);
            assert_eq!(written.as_array().unwrap().len(), 1);
            for key in ["pane", "points", "id"] {
                assert_eq!(written[0][key], before[0][key], "{edit}: {key}");
            }
            assert_eq!(prices(&charts)[1], [55.]);
            let peer_line = &charts[1].drawings().drawings[0];
            assert_eq!(
                charts[1].drawing_pane(peer_line.id),
                charts[1].series_pane(studies[1].1),
                "{edit}"
            );
        }
    }

    #[cfg(feature = "studies")]
    #[test]
    fn shared_drawing_filter_restores_the_original_pane_after_a_write_and_undo() {
        use crate::studies::{InputValue, RSI};

        for setup in ["never filtered", "removed", "length changed"] {
            let (host, mut charts, mut layouts) = shared_layouts();
            // The removed local id would name RSI(7), not RSI(14), on the peer.
            let mine = (setup != "never filtered")
                .then(|| charts[0].add_indicator(Box::new(RSI::new(14))));
            let local_seven = charts[0].add_indicator(Box::new(RSI::new(7)));
            let peer_seven = charts[1].add_indicator(Box::new(RSI::new(7)));
            let peer_rsi = charts[1].add_indicator(Box::new(RSI::new(14)));
            settle_pair(&mut layouts, &mut charts);
            charts[1].user_action(|c| {
                let id = draw(c, 55.);
                assert!(c.move_drawing_to_pane(id, c.series_pane(peer_rsi).unwrap()));
            });
            settle_pair(&mut layouts, &mut charts);
            let original = shared_records(&host)[0].clone();
            let local = charts[0].drawings().drawings[0].id;
            if let Some(mine) = mine {
                charts[0].drawings_mut().set_shared_drawings_hidden(true);
                if setup == "removed" {
                    assert!(charts[0].user_action(|c| c.remove_series(mine)));
                    charts[0].drawings_mut().set_shared_drawings_hidden(false);
                } else {
                    charts[0].user_action(|c| {
                        c.indicator_mut(mine)
                            .unwrap()
                            .set_input("period", InputValue::Length(21))
                            .unwrap()
                    });
                }
            }
            if setup != "length changed" {
                assert_eq!(
                    charts[0].drawing_pane(local),
                    Some(charts[0].main_series_pane())
                );
            }
            for op in ["delete", "clear", "move"] {
                if setup == "length changed" && op != "delete" {
                    continue;
                }
                charts[0].user_action(|c| match op {
                    "delete" => c.drawings_mut().delete_drawing(local),
                    "clear" => c.drawings_mut().clear_all(),
                    _ => {
                        assert!(c.move_drawing_to_pane(local, c.series_pane(local_seven).unwrap()))
                    }
                });
                settle_pair(&mut layouts, &mut charts);
                if op == "move" {
                    let target = charts[1].series_pane(peer_seven).unwrap();
                    assert_eq!(
                        charts[1].drawing_pane(charts[1].drawings().drawings[0].id),
                        Some(target)
                    );
                    assert_eq!(
                        shared_records(&host)[0]["pane"],
                        serde_json::to_value(charts[1].stored_pane(target)).unwrap()
                    );
                } else {
                    assert!(charts[1].drawings().drawings.is_empty(), "{setup}: {op}");
                    assert!(shared_records(&host).as_array().unwrap().is_empty());
                }
                assert!(charts[0].undo());
                settle_pair(&mut layouts, &mut charts);
                let written = shared_records(&host);
                assert_eq!(written.as_array().unwrap().len(), 1);
                for key in ["pane", "id", "points", "created_at"] {
                    assert_eq!(written[0][key], original[key], "{setup}: {op}: {key}");
                }
                assert_eq!(prices(&charts)[1], [55.]);
                assert_eq!(
                    charts[1].drawing_pane(charts[1].drawings().drawings[0].id),
                    charts[1].series_pane(peer_rsi)
                );
            }
        }
    }

    #[cfg(feature = "studies")]
    #[test]
    fn shared_drawing_filter_keeps_study_removal_local_without_storage() {
        use crate::studies::RSI;
        let mut charts = [bars(60), bars(60)];
        // Restore allocates the surviving study's pane before the study. Reserve one id
        // on the original chart so its removed study gets the surviving study's future id.
        let prior = charts[0].add_pane();
        assert!(charts[0].remove_pane(prior));
        let owners = charts.each_mut().map(|c| {
            c.set_symbol("SAMPLE");
            c.add_indicator(Box::new(RSI::new(14)))
        });
        charts[0].add_indicator(Box::new(RSI::new(7)));
        let mut layout = ChartLayout {
            mode: super::super::ChartLayoutMode::TwoHorizontal,
            ..Default::default()
        };
        layout.sync.sync_drawings = true;
        let mut drawing = line("shared");
        drawing.folder = vec!["Plan".into()];
        drawing.folder_sync = vec![Some(FolderSync::AllLayouts)];
        drawing.pane = Some(owners[1].0);
        charts[1].user_action(|c| c.drawings_mut().add_drawing(drawing).unwrap());
        frames(&mut layout, &mut charts);
        let ids = charts.each_ref().map(|c| c.drawings().drawings[0].id);
        let panes = std::array::from_fn::<_, 2, _>(|i| charts[i].series_pane(owners[i]).unwrap());
        charts[0].drawings_mut().set_shared_drawings_hidden(true);
        for step in 0..3 {
            match step {
                0 => assert!(charts[0].user_action(|c| c.remove_series(owners[0]))),
                1 => assert!(charts[0].undo()),
                _ => assert!(charts[0].redo()),
            }
            frames(&mut layout, &mut charts);
            let local = if step == 1 {
                panes[0]
            } else {
                charts[0].main_series_pane()
            };
            assert_eq!(charts[0].drawing_pane(ids[0]), Some(local));
            assert_eq!(charts[1].drawing_pane(ids[1]), Some(panes[1]));
            assert!(
                charts[1]
                    .drawings()
                    .painted_range(ids[1])
                    .is_some_and(|r| !r.is_empty())
            );
        }
        charts[0].drawings_mut().set_shared_drawings_hidden(false);
        frames(&mut layout, &mut charts);
        assert!(
            charts[0]
                .drawings()
                .painted_range(ids[0])
                .is_some_and(|r| !r.is_empty())
        );
        assert_eq!(charts[1].drawing_pane(ids[1]), Some(panes[1]));
        let original = charts[1].save_drawings().unwrap();
        let original = &original.drawings()[0];
        charts[0].user_action(|c| c.drawings_mut().delete_drawing(ids[0]));
        frames(&mut layout, &mut charts);
        assert!(charts[1].drawings().drawings.is_empty());
        assert!(charts[0].undo());
        frames(&mut layout, &mut charts);
        assert_eq!(charts[1].drawings().drawings.len(), 1);
        assert_eq!(
            charts[1].drawing_pane(charts[1].drawings().drawings[0].id),
            Some(panes[1])
        );
        let saved = charts[0].save_drawings().unwrap();
        assert_eq!(saved.drawings()[0].pane, original.pane);
        assert_eq!(saved.drawings()[0].drawing.pane, None);
        let json = saved.to_json().unwrap();
        #[cfg(feature = "ui")]
        let snapshot = charts[0].snapshot().unwrap();
        assert!(charts[0].load_drawings(&json).unwrap().is_empty());
        let check = |chart: &mut Chart| {
            frames(&mut ChartLayout::default(), std::array::from_mut(chart));
            let id = chart.drawings().drawings[0].id;
            assert_eq!(chart.drawing_pane(id), Some(chart.main_series_pane()));
            assert!(
                chart
                    .drawings()
                    .painted_range(id)
                    .is_some_and(|r| !r.is_empty())
            );
            assert_eq!(
                chart.save_drawings().unwrap().drawings()[0].pane,
                original.pane
            );
        };
        check(&mut charts[0]);
        #[cfg(feature = "ui")]
        {
            let mut fresh = bars(60);
            assert!(fresh.restore(&snapshot).unwrap().is_empty());
            assert_eq!(
                fresh
                    .indicators()
                    .find(|(_, study)| study.name() == "RSI")
                    .unwrap()
                    .0,
                owners[0],
                "the old id now belongs to the surviving RSI(7)"
            );
            check(&mut fresh);
        }
    }

    #[cfg(all(feature = "studies", feature = "ui"))]
    #[test]
    fn shared_drawing_filter_off_mirrors_an_unmatched_loaded_line_in_the_price_pane() {
        use crate::studies::RSI;

        let mut author = bars(60);
        author.set_symbol("SAMPLE");
        let rsi = author.add_indicator(Box::new(RSI::new(14)));
        let line = draw(&mut author, 10.);
        assert!(author.move_drawing_to_pane(line, author.series_pane(rsi).unwrap()));
        let file = author.save_drawings().unwrap().to_json().unwrap();
        let layout = || {
            let mut layout = ChartLayout {
                mode: super::super::ChartLayoutMode::TwoHorizontal,
                ..Default::default()
            };
            layout.sync.sync_drawings = true;
            layout
        };
        let check = |charts: &[Chart; 2], price| {
            for chart in charts {
                let [line] = chart.drawings().drawings.as_slice() else {
                    panic!("expected exactly one mirrored line");
                };
                assert_eq!(chart.drawing_pane(line.id), Some(chart.main_series_pane()));
                assert!(line.chart_points.iter().all(|p| p.price == price));
            }
        };
        let edit = |chart: &mut Chart, price| {
            let id = chart.drawings().drawings[0].id;
            chart.user_action(|c| {
                c.drawings_mut()
                    .edit_drawing(id, |d| {
                        for p in &mut d.chart_points {
                            p.price = price;
                        }
                    })
                    .unwrap()
            });
        };
        for peer_has_rsi in [false, true] {
            let mut charts = [bars(60), bars(60)];
            for chart in &mut charts {
                chart.set_symbol("SAMPLE");
            }
            charts[0].add_indicator(Box::new(RSI::new(7)));
            if peer_has_rsi {
                charts[1].add_indicator(Box::new(RSI::new(14)));
            }
            let mut pair = layout();
            frames(&mut pair, &mut charts);
            assert!(charts[0].load_drawings(&file).unwrap().is_empty());
            frames(&mut pair, &mut charts);
            check(&charts, 10.);
            edit(&mut charts[1], 10.25);
            frames(&mut pair, &mut charts);
            check(&charts, 10.25);

            let saved = charts.each_ref().map(|c| c.snapshot().unwrap());
            let mut restored = [bars(60), bars(60)];
            for (c, snapshot) in restored.iter_mut().zip(&saved) {
                assert!(c.restore(snapshot).unwrap().is_empty());
            }
            let mut restarted = layout();
            frames(&mut restarted, &mut restored);
            check(&restored, 10.25);
            edit(&mut restored[0], 9.75);
            frames(&mut restarted, &mut restored);
            check(&restored, 9.75);
        }
    }

    #[cfg(feature = "studies")]
    #[test]
    fn shared_drawing_filter_leaves_the_peers_series_layer_alone() {
        use crate::studies::SMA;
        use crate::widget::VisualOrder;

        let (host, mut charts, mut layouts) = shared_layouts();
        charts[1].add_indicator(Box::new(SMA::new(5)));
        settle_pair(&mut layouts, &mut charts);
        let below = charts[1].user_action(|c| {
            let id = draw(c, 10.5);
            assert!(c.reorder_drawing(id, VisualOrder::SendBackward));
            c.drawings().drawings[0].series_layer.unwrap()
        });
        settle_pair(&mut layouts, &mut charts);
        let stored = shared_records(&host);
        assert_eq!(stored[0]["drawing"]["series_layer"], below);
        charts[0].drawings_mut().set_shared_drawings_hidden(true);
        for step in 0..3 {
            match step {
                0 => {
                    charts[0].user_action(|c| {
                        c.add_indicator(Box::new(SMA::new(5)));
                    });
                }
                1 => assert!(charts[0].undo()),
                _ => assert!(charts[0].redo()),
            }
            settle_pair(&mut layouts, &mut charts);
            assert_eq!(
                charts[0]
                    .indicators()
                    .any(|(_, study)| study.name() == "SMA"),
                step != 1
            );
            for c in &charts {
                assert_eq!(c.drawings().drawings[0].series_layer, Some(below));
            }
            assert_eq!(shared_records(&host), stored);
        }
    }

    #[cfg(feature = "ui")]
    #[test]
    fn shared_drawing_filter_keeps_same_named_peer_folder_intact() {
        use crate::ui::widget_bar::{ObjectTreeAction as Action, ObjectTreePanel, SourceItem};

        let (host, mut charts, mut layouts) = shared_layouts();
        settle_pair(&mut layouts, &mut charts);
        let plan = vec!["Plan".to_owned()];
        let archive = vec!["Archive".to_owned()];
        let place = |c: &mut Chart, path: &[String], scope, price| {
            let mut d = line("");
            d.chart_points = vec![ChartPoint::new(2.0, price), ChartPoint::new(4.0, price)];
            d.folder = path.to_owned();
            d.folder_sync = vec![Some(scope)];
            c.user_action(|c| c.drawings_mut().add_drawing(d).unwrap())
        };
        // Load the hidden peer folder first so an unfiltered folder-mode lookup picks it.
        settle_pair(&mut layouts, &mut charts);
        place(&mut charts[1], &plan, FolderSync::AllLayouts, 10.5);
        settle_pair(&mut layouts, &mut charts);
        let stored = shared_records(&host);
        let local = place(&mut charts[0], &plan, FolderSync::Layout, 9.5);
        let archived = place(&mut charts[0], &archive, FolderSync::Chart, 9.0);
        settle_pair(&mut layouts, &mut charts);
        charts[0].drawings_mut().set_shared_drawings_hidden(true);
        let mut tree = ObjectTreePanel::new();
        tree.state_mut()
            .folders
            .extend([plan.clone(), archive.clone()]);
        tree.state_mut().select(local);
        for (action, id, expected, scope) in [
            (
                Action::MoveToFolder(vec![archived], plan.clone()),
                archived,
                vec!["Plan"],
                FolderSync::Layout,
            ),
            (
                Action::SetFolderSync(plan.clone(), Some(FolderSync::Chart)),
                local,
                vec!["Plan"],
                FolderSync::Chart,
            ),
            (
                Action::MoveFolder(plan.clone(), archive.clone()),
                local,
                vec!["Archive", "Plan"],
                FolderSync::Chart,
            ),
            (
                Action::CreateFolder(archive.clone(), "New".into()),
                local,
                vec!["Archive", "New", "Plan"],
                FolderSync::Chart,
            ),
        ] {
            let rows: Vec<_> = charts[0]
                .drawings()
                .listed_drawings()
                .map(|d| {
                    let mut row = SourceItem::drawing(d.id, d.tool_type, d.color32());
                    row.folder.clone_from(&d.folder);
                    row
                })
                .collect();
            assert!(charts[0].user_action(|c| tree.apply_to_chart(c, &action, &rows)));
            settle_pair(&mut layouts, &mut charts);
            let check_peer = |charts: &[Chart; 2]| {
                assert_eq!(prices(charts)[1], [10.5]);
                let d = &charts[1].drawings().drawings[0];
                assert_eq!(d.folder, plan, "{action:?}");
                assert_eq!(d.folder_sync_mode(), Some(FolderSync::AllLayouts));
                assert_eq!(shared_records(&host), stored, "{action:?}");
            };
            let note = charts[0]
                .drawings()
                .listed_drawings()
                .find(|d| d.id == id)
                .unwrap();
            assert_eq!(note.folder, expected, "{action:?}");
            assert_eq!(note.folder_sync_mode(), Some(scope));
            check_peer(&charts);
            assert!(charts[0].undo());
            settle_pair(&mut layouts, &mut charts);
            let note = charts[0]
                .drawings()
                .listed_drawings()
                .find(|d| d.id == id)
                .unwrap();
            let (path, scope) = if id == local {
                (&plan, FolderSync::Layout)
            } else {
                (&archive, FolderSync::Chart)
            };
            assert_eq!(&note.folder, path);
            assert_eq!(note.folder_sync_mode(), Some(scope));
            check_peer(&charts);
        }
    }

    #[test]
    fn shared_drawing_filter_keeps_first_text_edit_until_commit() {
        use crate::widget::test_frames::frame;
        use egui::{Event, Key, Modifiers, PointerButton};

        let (host, mut charts, mut layouts) = shared_layouts();
        settle_pair(&mut layouts, &mut charts);
        let chart = &mut charts[0];
        chart.user_action(|c| c.drawings_mut().set_shared_drawings_hidden(true));
        chart.user_action(|c| draw(c, 9.5));
        let ctx = egui::Context::default();
        let mut time = 0.0;
        let mut step = |chart: &mut Chart, events| {
            time += 0.1;
            frame(&ctx, chart, time, events);
        };
        step(chart, vec![]);
        chart
            .drawings_mut()
            .set_active_tool(Some(DrawingToolType::TextLabel));
        let pos = chart.get_rendered_price_rect().center();
        step(chart, vec![Event::PointerMoved(pos)]);
        for pressed in [true, false] {
            step(
                chart,
                vec![Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                }],
            );
        }
        step(chart, vec![]);
        step(chart, vec![Event::Text("Session high".into())]);
        step(chart, vec![]);
        let id = chart
            .drawings()
            .drawings
            .iter()
            .find(|d| d.tool_type == DrawingToolType::TextLabel)
            .unwrap()
            .id;
        assert!(chart.drawings().painted_text(id, "Session high").is_some());
        step(
            chart,
            vec![Event::Key {
                key: Key::Enter,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::COMMAND,
            }],
        );
        settle_pair(&mut layouts, &mut charts);
        let peer_label = charts[1]
            .drawings()
            .drawings
            .iter()
            .find(|d| d.tool_type == DrawingToolType::TextLabel)
            .unwrap();
        assert_eq!(peer_label.text.as_deref(), Some("Session high"));
        assert!(charts[0].drawings().painted_range(id).is_none());
        let committed = shared_records(&host);
        assert!(
            committed
                .as_array()
                .unwrap()
                .iter()
                .any(|d| d["drawing"]["text"] == "Session high")
        );
        // Undo and redo the text placement (including its first edit), the line and the
        // filter toggle in order, checking both the peer and the shared document each time.
        for (redo, count, filtered) in [
            (false, 1, true),
            (false, 0, true),
            (false, 0, false),
            (true, 0, true),
            (true, 1, true),
            (true, 2, true),
        ] {
            assert!(if redo {
                charts[0].redo()
            } else {
                charts[0].undo()
            });
            settle_pair(&mut layouts, &mut charts);
            assert_eq!(charts[0].drawings().shared_drawings_hidden(), filtered);
            let records = shared_records(&host);
            let records = records.as_array().unwrap();
            assert_eq!(records.len(), count);
            assert_eq!(charts[1].drawings().drawings.len(), count);
            if count == 1 {
                assert_eq!(prices(&charts)[1], [9.5]);
                assert_eq!(records[0]["points"][0]["price"], 9.5);
            } else if count == 2 {
                assert!(
                    charts[1]
                        .drawings()
                        .drawings
                        .iter()
                        .any(|d| d.text.as_deref() == Some("Session high"))
                );
                assert!(
                    records
                        .iter()
                        .any(|d| d["drawing"]["text"] == "Session high")
                );
            }
        }
    }

    /// Guards #3436: with drawing storage, a drawing kept on one chart shows on that chart only,
    /// one kept in the layout on the charts of that layout only, and one shared across layouts on
    /// every chart of the symbol, also after the layouts are saved and opened again on new charts
    /// of the storage, and also when its chart shows another symbol as the layout is saved. Moving
    /// a drawing into another folder moves it between the charts, and the undo on its chart moves
    /// it back on all of them. Another layout opened on the chart leaves its drawings out.
    // Saving and opening a layout needs `ui`.
    #[cfg(feature = "ui")]
    #[test]
    fn folder_scope_decides_which_charts_of_a_storage_show_a_drawing() {
        use crate::services::document::tests::Revised;
        use crate::widget::ChartLayoutMode;

        /// The two charts of one layout and a chart of another, as in another window.
        fn settle(layouts: &mut [ChartLayout; 2], pair: &mut [Chart; 2], other: &mut [Chart; 1]) {
            for _ in 0..3 {
                frames(&mut layouts[0], pair);
                frames(&mut layouts[1], other);
            }
        }
        let host = Arc::new(Revised::default());
        let chart = || {
            let mut chart = bars(60);
            chart.set_symbol("SAMPLE");
            chart.set_storage(host.clone());
            chart
        };
        let layouts = || {
            [
                ChartLayout {
                    mode: ChartLayoutMode::TwoHorizontal,
                    ..Default::default()
                },
                ChartLayout::default(),
            ]
        };
        let folders = [
            ("Chart", FolderSync::Chart, 104.0),
            ("Layout", FolderSync::Layout, 105.0),
            ("All", FolderSync::AllLayouts, 106.0),
        ];
        let (mut session, mut pair, mut other) = (layouts(), [chart(), chart()], [chart()]);
        settle(&mut session, &mut pair, &mut other);
        pair[0].user_action(|chart| {
            for (folder, sync, price) in folders {
                let id = draw(chart, price);
                let folder = [folder.to_owned()];
                assert!(chart.drawings_mut().move_to_folder(&[id], &[], &folder));
                assert!(chart.drawings_mut().set_folder_sync(&folder, Some(sync)));
            }
        });
        settle(&mut session, &mut pair, &mut other);
        let scoped = (
            [vec![104.0, 105.0, 106.0], vec![105.0, 106.0]],
            [vec![106.0]],
        );
        assert_eq!((prices(&pair), prices(&other)), scoped, "drawn");

        // Saved, and opened again on new charts of the storage.
        let saved = [&pair[0], &pair[1], &other[0]].map(|chart| chart.snapshot().unwrap());
        let (mut session, mut pair, mut other) = (layouts(), [chart(), chart()], [chart()]);
        for (chart, saved) in pair.iter_mut().chain(&mut other).zip(&saved) {
            chart.restore(saved).unwrap();
        }
        settle(&mut session, &mut pair, &mut other);
        assert_eq!((prices(&pair), prices(&other)), scoped, "opened again");

        // (drawing moved, folder it moves to, then shown on the pair and on the other chart)
        type Row = (f64, &'static str, [&'static [f64]; 2], &'static [f64]);
        let rows: [Row; 6] = [
            (
                104.0,
                "All",
                [&[104.0, 105.0, 106.0], &[104.0, 105.0, 106.0]],
                &[104.0, 106.0],
            ),
            (
                104.0,
                "Layout",
                [&[104.0, 105.0, 106.0], &[104.0, 105.0, 106.0]],
                &[106.0],
            ),
            (
                105.0,
                "All",
                [&[104.0, 105.0, 106.0], &[105.0, 106.0]],
                &[105.0, 106.0],
            ),
            (105.0, "Chart", [&[104.0, 105.0, 106.0], &[106.0]], &[106.0]),
            (106.0, "Chart", [&[104.0, 105.0, 106.0], &[105.0]], &[]),
            (
                106.0,
                "Layout",
                [&[104.0, 105.0, 106.0], &[105.0, 106.0]],
                &[],
            ),
        ];
        for (price, folder, shown, elsewhere) in rows {
            let id = pair[0]
                .drawings()
                .drawings
                .iter()
                .find(|d| d.chart_points[0].price == price)
                .unwrap()
                .id;
            pair[0].user_action(|chart| {
                let folder = [folder.to_owned()];
                assert!(chart.drawings_mut().move_to_folder(&[id], &[], &folder));
            });
            settle(&mut session, &mut pair, &mut other);
            assert_eq!(
                (prices(&pair), prices(&other)),
                (shown.map(<[f64]>::to_vec), [elsewhere.to_vec()]),
                "{price} moved to {folder}"
            );
            assert!(pair[0].undo(), "{price} moved to {folder}");
            settle(&mut session, &mut pair, &mut other);
            assert_eq!(
                (prices(&pair), prices(&other)),
                scoped,
                "{price} moved to {folder}, undone"
            );
        }

        // Kept on the chart that shows the copy, the drawing leaves the first chart, and comes
        // back on that chart alone with its saved layout.
        pair[1].user_action(|chart| {
            let layout = ["Layout".to_owned()];
            assert!(
                chart
                    .drawings_mut()
                    .set_folder_sync(&layout, Some(FolderSync::Chart))
            );
        });
        settle(&mut session, &mut pair, &mut other);
        let kept = ([vec![104.0, 106.0], vec![105.0, 106.0]], [vec![106.0]]);
        assert_eq!(
            (prices(&pair), prices(&other)),
            kept,
            "kept on the copy's chart"
        );
        let saved = [&pair[0], &pair[1], &other[0]].map(|chart| chart.snapshot().unwrap());
        let (mut session, mut pair, mut other) = (layouts(), [chart(), chart()], [chart()]);
        for (chart, saved) in pair.iter_mut().chain(&mut other).zip(&saved) {
            chart.restore(saved).unwrap();
        }
        settle(&mut session, &mut pair, &mut other);
        assert_eq!((prices(&pair), prices(&other)), kept, "kept, opened again");

        // Kept aside while its chart shows another symbol, the drawing comes back with the saved
        // layout once the chart shows the symbol again.
        pair[0].set_symbol("OTHER");
        settle(&mut session, &mut pair, &mut other);
        let saved = [&pair[0], &pair[1], &other[0]].map(|chart| chart.snapshot().unwrap());
        let (mut session, mut pair, mut other) = (layouts(), [chart(), chart()], [chart()]);
        for (chart, saved) in pair.iter_mut().chain(&mut other).zip(&saved) {
            chart.restore(saved).unwrap();
        }
        settle(&mut session, &mut pair, &mut other);
        pair[0].set_symbol("SAMPLE");
        settle(&mut session, &mut pair, &mut other);
        assert_eq!(
            (prices(&pair), prices(&other)),
            kept,
            "kept aside, opened again"
        );

        // Another layout opened on that chart leaves out the drawing the chart kept aside.
        pair[0].set_symbol("OTHER");
        settle(&mut session, &mut pair, &mut other);
        let mut elsewhere = bars(60);
        elsewhere.set_symbol("THIRD");
        pair[0].restore(&elsewhere.snapshot().unwrap()).unwrap();
        settle(&mut session, &mut pair, &mut other);
        pair[0].set_symbol("SAMPLE");
        settle(&mut session, &mut pair, &mut other);
        assert_eq!(prices(&pair)[0], [106.0], "another layout opened");
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
