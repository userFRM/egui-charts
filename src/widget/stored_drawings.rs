//! Each symbol's drawings kept in the host's storage.

use super::Chart;
use crate::data::FeedError;
use crate::data::inbox::Inbox;
use crate::drawings::persistence::{DrawingStorage, StoredPane};
use crate::services::document::{Received, StoredDocument, copy_number};
use crate::services::{ConflictChoice, Revision, Storage, StoreKind};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

/// What a chart asked its storage about the kept copies of its symbols' drawings.
#[derive(Default)]
struct LocalCopies {
    inbox: Arc<Inbox>,
    /// The symbol listed last, with the names of its copies by copy number.
    listed: Option<(String, Vec<String>)>,
    /// The request whose answer is awaited.
    sent: Option<CopyRequest>,
    /// The requests that wait for it, in order.
    queued: VecDeque<CopyRequest>,
    /// A copy read to be loaded: its symbol, name and document.
    read: Option<(String, String, String)>,
}

impl LocalCopies {
    /// Drops `name` from the list if the copies of `symbol` are the ones listed.
    fn forget(&mut self, symbol: &str, name: &str) {
        if let Some((listed, names)) = &mut self.listed
            && listed == symbol
        {
            names.retain(|listed| listed != name);
        }
    }
}

/// A request about the kept copies of a symbol's drawings.
enum CopyRequest {
    /// Lists the documents, keeping the copies of `symbol`'s drawings.
    List { symbol: String },
    /// Reads a copy, to load it or, with `discard`, to check it before deleting it.
    Read {
        symbol: String,
        name: String,
        discard: bool,
    },
    /// Deletes a copy found to hold `symbol`'s drawings.
    Delete { symbol: String, name: String },
}

impl CopyRequest {
    fn symbol(&self) -> &str {
        match self {
            Self::List { symbol } | Self::Read { symbol, .. } | Self::Delete { symbol, .. } => {
                symbol
            }
        }
    }
}

pub(super) struct DrawingStore {
    // Only attaching storage links the drawing readers and writers into an application.
    sync: fn(&mut Chart, Option<&egui::Context>),
    save: fn(&mut Chart),
    storage: Arc<dyn Storage>,
    /// The active document and inactive documents with retained drawings or edits.
    documents: HashMap<String, StoredDocument>,
    /// Drawings read for a symbol that was no longer shown when they arrived.
    loaded: HashMap<String, String>,
    /// The chart's drawing edit count last captured in a document, which may still be dirty.
    captured: u64,
    /// Edits made before a read finishes. The flag distinguishes edits from a restored layout.
    pending: HashMap<String, (DrawingStorage, bool)>,
    error: Option<String>,
    /// Documents this chart wrote, or read again at a newer revision, since the charts of its
    /// layout that keep drawings in the same storage were told; `None` for a host without
    /// revisions.
    changed: HashMap<String, Option<Revision>>,
    /// Per symbol, the stored pane of each read drawing whose study this chart does not show, by
    /// stored id with its creation time: shown in the price pane, it is written back in its own.
    unshown_panes: HashMap<String, HashMap<String, (i64, StoredPane)>>,
    /// The kept copies of the shown symbol's drawings and the requests about them.
    copies: LocalCopies,
}

impl DrawingStore {
    pub(super) fn forget(&mut self, symbol: &str) -> bool {
        if self.pending.contains_key(symbol)
            || self
                .documents
                .get(symbol)
                .is_some_and(StoredDocument::saving)
        {
            return false;
        }
        self.documents.remove(symbol);
        self.loaded.remove(symbol);
        self.unshown_panes.remove(symbol);
        self.documents.shrink_to_fit();
        self.loaded.shrink_to_fit();
        true
    }

    /// Sends the queued requests about kept copies one at a time, each once no copy of its
    /// symbol is being written and no copy read earlier waits to be loaded, and takes the
    /// answers that arrived.
    fn receive_copies(&mut self) {
        loop {
            if self.copies.sent.is_none()
                && self.copies.read.is_none()
                && let Some(request) = self.copies.queued.front()
                && !self
                    .documents
                    .get(request.symbol())
                    .is_some_and(StoredDocument::copying)
            {
                let request = self.copies.queued.pop_front().expect("queued request");
                let inbox = &self.copies.inbox;
                match &request {
                    CopyRequest::List { .. } => {
                        self.storage.list(StoreKind::Drawings, inbox.reply(0));
                    }
                    CopyRequest::Read { name, .. } => {
                        self.storage.load(StoreKind::Drawings, name, inbox.reply(0));
                    }
                    CopyRequest::Delete { name, .. } => {
                        self.storage
                            .delete(StoreKind::Drawings, name, inbox.reply(0));
                    }
                }
                self.copies.sent = Some(request);
            }
            let answers = self.copies.inbox.drain().answers;
            if answers.is_empty() {
                break;
            }
            for (_, answer) in answers {
                if let Some(request) = self.copies.sent.take() {
                    self.copy_answered(request, answer);
                }
            }
        }
    }

    /// Finishes the answer to `request`.
    fn copy_answered(&mut self, request: CopyRequest, answer: Box<dyn std::any::Any + Send>) {
        match request {
            CopyRequest::List { symbol } => {
                let answer = *answer
                    .downcast::<Result<Vec<String>, FeedError>>()
                    .expect("copy list");
                let mut copies: Vec<_> = match answer {
                    Ok(names) => names
                        .into_iter()
                        .filter_map(|name| Some((copy_number(&symbol, &name)?, name)))
                        .collect(),
                    Err(error) => {
                        self.error = Some(format!(
                            "Could not list the kept copies of {symbol} drawings: {error}"
                        ));
                        Vec::new()
                    }
                };
                copies.sort_unstable();
                let names = copies.into_iter().map(|(_, name)| name).collect();
                self.copies.listed = Some((symbol, names));
            }
            CopyRequest::Read {
                symbol,
                name,
                discard,
            } => {
                let answer = *answer
                    .downcast::<Result<Option<String>, FeedError>>()
                    .expect("copy read");
                match answer {
                    Ok(None) => {
                        self.copies.forget(&symbol, &name);
                        if !discard {
                            self.error = Some(format!("{name} no longer exists"));
                        }
                    }
                    Ok(Some(json)) if !discard => self.copies.read = Some((symbol, name, json)),
                    Ok(Some(json)) if holds_drawings_of(&json, &symbol) => {
                        self.copies
                            .queued
                            .push_front(CopyRequest::Delete { symbol, name });
                    }
                    Ok(Some(_)) => {
                        self.error = Some(format!(
                            "{name} was not discarded: it does not hold drawings of {symbol}"
                        ));
                    }
                    Err(error) if discard => {
                        self.error = Some(format!("Could not discard {name}: {error}"));
                    }
                    Err(error) => self.error = Some(format!("Could not read {name}: {error}")),
                }
            }
            CopyRequest::Delete { symbol, name } => {
                let answer = *answer
                    .downcast::<Result<(), FeedError>>()
                    .expect("copy delete");
                match answer {
                    Ok(()) => self.copies.forget(&symbol, &name),
                    Err(error) => self.error = Some(format!("Could not discard {name}: {error}")),
                }
            }
        }
    }
}

#[cfg(feature = "ui")]
impl DrawingStore {
    pub(super) fn ready(&self, symbol: &str) -> bool {
        self.pending.get(symbol).is_some_and(|(_, merge)| !merge)
            || self
                .documents
                .get(symbol)
                .is_some_and(|document| !document.reading())
    }
}

/// Starts reading the drawings of `symbol` and follows changes other clients make to them.
fn open_drawings(storage: &dyn Storage, symbol: &str) -> StoredDocument {
    let mut document = StoredDocument::open(storage, StoreKind::Drawings, symbol);
    document.follow_changes(storage);
    document
}

/// Whether the stored document `json` names `symbol` as the symbol of its drawings, in any
/// version of the format.
fn holds_drawings_of(json: &str, symbol: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(json).is_ok_and(|document| {
        document.get("symbol").and_then(serde_json::Value::as_str) == Some(symbol)
    })
}

impl Chart {
    /// Keeps each symbol's drawings in `storage`, in the [`StoreKind::Drawings`] document named
    /// by the symbol. The drawings of a symbol are read when it is first shown and, when a
    /// document holds some, replace the chart's drawings and undo history; they are written after
    /// every drawing edit made through chart history (every user edit, undo and redo) and when
    /// the symbol changes. Failed writes retain the latest document and retry after one second
    /// of input time; [`Self::flush_stored_drawings`] retries immediately.
    /// Drawings that could not all be read are never written over.
    ///
    /// Charts that show one symbol keep its drawings in one document. A chart reads them again
    /// after another chart of the same [`ChartLayout`](super::ChartLayout) wrote them to the same
    /// `storage`, and after another client changed them if `storage` keeps revisions and reports
    /// changes through [`Storage::watch`]. It does so once nothing of its own is unsaved and no
    /// drawing is being drawn, dragged, measured or edited: the stored drawings replace its
    /// saveable drawings, selection and undo history, each keeping its local id, and a document
    /// deleted elsewhere leaves none. A write waits for such a read, and one that fails is tried
    /// again a second of input time later. Drawing sync of a [`ChartLayout`](super::ChartLayout)
    /// shows drawings of charts outside `storage` on this chart without saving them here. A
    /// drawing whose study the chart does not show appears in the price pane and is written back
    /// in its study's pane. If `storage` keeps revisions, a
    /// write over drawings changed elsewhere since they were read is refused and waits, with the
    /// chart's drawings unchanged, until [`Self::resolve_storage_conflict`] settles it; without
    /// revisions, the later write replaces the earlier one, including edits on two charts within
    /// the time one write takes.
    ///
    /// Per-drawing storage notifications are disabled and cleared. To consume them alongside
    /// automatic storage, call `chart.drawings_mut().set_sync_enabled(true)` after attaching it.
    pub fn set_storage(&mut self, storage: Arc<dyn Storage>) {
        self.drawings.set_sync_enabled(false);
        for (manager, _) in self.symbol_drawings.values_mut() {
            manager.set_sync_enabled(false);
        }
        self.drawing_store = Some(DrawingStore {
            sync: Self::poll_stored_drawings,
            save: Self::write_stored_drawings,
            storage,
            documents: HashMap::new(),
            loaded: HashMap::new(),
            captured: self.history.drawing_edits,
            pending: HashMap::new(),
            error: None,
            changed: HashMap::new(),
            unshown_panes: HashMap::new(),
            copies: LocalCopies::default(),
        });
    }

    /// Captures current drawing edits and retries failed writes for every symbol immediately.
    /// Available replies are processed, but asynchronous storage is not awaited. Returns `true`
    /// once all captured edits are acknowledged. Keep rendering or call again to finish delayed
    /// reads and writes; inspect [`Self::take_storage_error`] for failures. A write refused
    /// because the drawings changed elsewhere stays unacknowledged until
    /// [`Self::resolve_storage_conflict`] settles it.
    pub fn flush_stored_drawings(&mut self) -> bool {
        if let Some(sync) = self.drawing_store.as_ref().map(|store| store.sync) {
            sync(self, None);
            // An inline load can submit a write whose acknowledgement is already available.
            sync(self, None);
        }
        self.drawing_store.as_ref().is_none_or(|store| {
            store.pending.is_empty()
                && store.captured == self.history.drawing_edits
                && store.documents.values().all(StoredDocument::flushed)
        })
    }

    /// The error of a failed read or write of drawings since the last call, to show to the
    /// user. A failed write keeps the drawings on the chart and is reported once. A failed read
    /// of drawings changed elsewhere is reported once and tried again a second of input time
    /// later until it succeeds.
    pub fn take_storage_error(&mut self) -> Option<String> {
        self.drawing_store.as_mut()?.error.take()
    }

    /// The symbol whose drawings `storage` refused to write because another client changed them
    /// after they were read: the shown symbol first, then the others by name. The chart keeps
    /// its drawings and writes nothing of that symbol until [`Self::resolve_storage_conflict`]
    /// settles it; ask the user, for example with `ui::dialogs::StorageConflictDialog`. `None`
    /// while no refused write waits, including while a kept copy is being written.
    pub fn storage_conflict(&self) -> Option<&str> {
        let store = self.drawing_store.as_ref()?;
        let waiting = |document: &StoredDocument| document.conflicted() && !document.copying();
        if store.documents.get(&self.symbol).is_some_and(waiting) {
            return Some(self.symbol.as_str());
        }
        store
            .documents
            .iter()
            .filter(|(_, document)| waiting(document))
            .map(|(name, _)| name.as_str())
            .min()
    }

    /// Settles the refused write of `symbol`'s drawings that [`Self::storage_conflict`] reports
    /// with the user's `choice`: read the stored drawings, keep the unsaved ones as the document
    /// `<symbol> (local copy)` (or the next free numbered copy) and read the stored ones, or write
    /// the unsaved ones over the stored ones. The drawings change on the next frame or
    /// [`Self::flush_stored_drawings`]. Does nothing when no write of `symbol` waits for a choice.
    /// [`Self::local_copies`] lists the copies kept, to load or discard.
    pub fn resolve_storage_conflict(&mut self, symbol: &str, choice: ConflictChoice) {
        if let Some(store) = &mut self.drawing_store
            && let Some(document) = store.documents.get_mut(symbol)
        {
            document.resolve(&*store.storage, choice);
        }
    }

    /// Asks storage for the copies of the shown symbol's drawings that
    /// [`ConflictChoice::KeepLocalCopy`] kept: the [`StoreKind::Drawings`] documents
    /// `<symbol> (local copy)`, `<symbol> (local copy 2)` and so on. [`Self::local_copies`] has
    /// them once storage answers, on a later frame or [`Self::flush_stored_drawings`]. Requests
    /// about copies are sent one at a time, each after any copy of its symbol being written.
    /// Does nothing without storage or a symbol.
    pub fn list_local_copies(&mut self) {
        if let Some(store) = &mut self.drawing_store
            && !self.symbol.is_empty()
        {
            let symbol = self.symbol.clone();
            store.copies.queued.push_back(CopyRequest::List { symbol });
        }
    }

    /// The kept copies of the shown symbol's drawings that the last [`Self::list_local_copies`]
    /// found, by copy number; `None` until storage answered it for the shown symbol. A failed
    /// listing, reported through [`Self::take_storage_error`], finds none, and a discarded copy
    /// leaves the list once storage confirms.
    pub fn local_copies(&self) -> Option<&[String]> {
        let (symbol, names) = self.drawing_store.as_ref()?.copies.listed.as_ref()?;
        (*symbol == self.symbol).then_some(names.as_slice())
    }

    /// Replaces the shown symbol's drawings with those of its kept copy `name`, one that
    /// [`Self::local_copies`] lists, as one user action: undo restores the drawings it replaced,
    /// and the result is written like any other edit. Drawings that are never saved stay, and
    /// the copy stays stored. The copy is read first and loaded once the symbol's own drawings
    /// are read and nothing is being drawn, dragged, measured or edited. A copy that no longer
    /// exists or whose drawings cannot all be read changes nothing, and neither does one that
    /// arrives after another symbol is shown; [`Self::take_storage_error`] reports each. Does
    /// nothing for a name that `local_copies` does not list.
    pub fn load_local_copy(&mut self, name: &str) {
        self.request_local_copy(name, false);
    }

    /// Deletes the kept copy `name` of the shown symbol's drawings, one that
    /// [`Self::local_copies`] lists, which then leaves that list once storage confirms. The copy
    /// is read first and kept if it does not hold drawings of the shown symbol. Failures are
    /// reported through [`Self::take_storage_error`]. Does nothing for a name that
    /// `local_copies` does not list.
    pub fn discard_local_copy(&mut self, name: &str) {
        self.request_local_copy(name, true);
    }

    fn request_local_copy(&mut self, name: &str, discard: bool) {
        if !self
            .local_copies()
            .is_some_and(|names| names.iter().any(|listed| listed == name))
        {
            return;
        }
        let symbol = self.symbol.clone();
        let store = self.drawing_store.as_mut().expect("listed copies");
        let name = name.to_owned();
        store.copies.queued.push_back(CopyRequest::Read {
            symbol,
            name,
            discard,
        });
    }

    /// Handles the requests about kept copies, then loads a copy read for the shown symbol once
    /// its drawings are read and nothing is being drawn or edited.
    fn poll_local_copies(&mut self, ctx: Option<&egui::Context>, busy: bool) {
        let Some(store) = &mut self.drawing_store else {
            return;
        };
        if let Some(ctx) = ctx {
            let repaint = ctx.clone();
            store
                .copies
                .inbox
                .set_waker(move || repaint.request_repaint());
        }
        store.receive_copies();
        let ready = !busy
            && store
                .documents
                .get(&self.symbol)
                .is_some_and(|document| !document.reading());
        let Some((symbol, name, json)) = store
            .copies
            .read
            .take_if(|(symbol, ..)| *symbol != self.symbol || ready)
        else {
            return;
        };
        let loaded = if symbol == self.symbol {
            self.apply_local_copy(&name, &json)
        } else {
            Err(format!(
                "{name} was not loaded because {symbol} is no longer shown"
            ))
        };
        if let Err(error) = loaded {
            self.drawing_store.as_mut().expect("drawing storage").error = Some(error);
        }
    }

    /// Replaces the saveable drawings with those of the kept copy `name` read as `json`, as one
    /// user action that the next write stores.
    fn apply_local_copy(&mut self, name: &str, json: &str) -> Result<(), String> {
        let copy = match DrawingStorage::from_json(json) {
            Ok((copy, skipped)) if skipped.is_empty() && copy.symbol == self.symbol => copy,
            Ok(_) => return Err(format!("Drawings of {name} could not all be read")),
            Err(error) => return Err(format!("Could not read drawings of {name}: {error}")),
        };
        // Shown in the price pane when this chart does not show their study, written back in it.
        let mut unshown = HashMap::new();
        let drawings: Vec<_> = copy
            .drawings()
            .iter()
            .map(|record| {
                let drawing = self.restored_drawing(record, true);
                if drawing.pane.is_none()
                    && let Some(pane) = &record.pane
                {
                    unshown.insert(record.id.clone(), (record.created_at, pane.clone()));
                }
                drawing
            })
            .collect();
        if !unshown.is_empty() {
            let store = self.drawing_store.as_mut().expect("drawing storage");
            let panes = store.unshown_panes.entry(self.symbol.clone()).or_default();
            panes.extend(unshown);
        }
        self.user_action(|chart| chart.drawings.replace_saved_drawings(drawings));
        Ok(())
    }

    /// Whether this chart and `other` keep their drawings in the same storage.
    pub(super) fn shares_stored_drawings(&self, other: &Chart) -> bool {
        match (&self.drawing_store, &other.drawing_store) {
            (Some(own), Some(other)) => Arc::ptr_eq(&own.storage, &other.storage),
            _ => false,
        }
    }

    /// Whether this chart keeps its drawings in a storage.
    pub(super) fn keeps_stored_drawings(&self) -> bool {
        self.drawing_store.is_some()
    }

    /// Tells each chart in `charts` which documents the others that keep drawings in the same
    /// storage wrote or read again since the last call, so that it reads them again. Returns
    /// whether any chart was told.
    pub(super) fn share_stored_drawings(charts: &mut [&mut Chart]) -> bool {
        let mut shared = false;
        for at in 0..charts.len() {
            let Some(store) = &mut charts[at].drawing_store else {
                continue;
            };
            if store.changed.is_empty() {
                continue;
            }
            let changed = std::mem::take(&mut store.changed);
            let storage = Arc::clone(&store.storage);
            for (other, chart) in charts.iter_mut().enumerate() {
                let Some(store) = chart
                    .drawing_store
                    .as_mut()
                    .filter(|store| other != at && Arc::ptr_eq(&store.storage, &storage))
                else {
                    continue;
                };
                for (name, revision) in &changed {
                    if let Some(document) = store.documents.get_mut(name) {
                        document.changed_elsewhere(revision.clone());
                        shared = true;
                    }
                }
            }
        }
        shared
    }

    /// Reads the shown symbol's drawings when it is first shown and saves edited drawings.
    pub(super) fn sync_stored_drawings(&mut self, ctx: &egui::Context) {
        if let Some(store) = &self.drawing_store {
            (store.sync)(self, Some(ctx));
        }
    }

    fn poll_stored_drawings(&mut self, ctx: Option<&egui::Context>) {
        self.save_stored_drawings();
        let symbol = self.symbol.clone();
        // A change made elsewhere is read between gestures, so that it cannot cut one short.
        let busy = ctx.is_some_and(|ctx| ctx.input(|input| input.pointer.any_down()))
            || self.drawings.curr_drawing.is_some()
            || self.drawings.measurement.is_some()
            || self.drawings.text_editor.is_some();
        let Some(store) = &mut self.drawing_store else {
            return;
        };
        if !symbol.is_empty() && !store.documents.contains_key(&symbol) {
            let document = open_drawings(&*store.storage, &symbol);
            store.documents.insert(symbol.clone(), document);
        }
        for (name, document) in &mut store.documents {
            document.hold(busy && name == &symbol);
            let mut first = document.reading();
            let received = match ctx {
                Some(ctx) => document.poll(&*store.storage, ctx),
                None => document.flush(&*store.storage),
            };
            for received in received {
                match received {
                    Received::Loaded(json) => {
                        let later = !std::mem::replace(&mut first, false);
                        if later && let Some(revision) = document.revision() {
                            store.changed.insert(name.clone(), Some(revision.clone()));
                        }
                        // Read again and missing, the document was deleted: no drawings remain.
                        let json = json.or_else(|| {
                            later
                                .then(|| DrawingStorage::new(name.clone()).to_json().ok())
                                .flatten()
                        });
                        let read = json.as_deref().map(DrawingStorage::from_json).transpose();
                        match read {
                            Ok(Some((drawings, skipped)))
                                if !skipped.is_empty() || drawings.symbol != *name =>
                            {
                                document.refuse_writes();
                                store.error =
                                    Some(format!("Drawings of {name} could not all be read"));
                            }
                            Err(error) => {
                                document.refuse_writes();
                                store.error =
                                    Some(format!("Could not read drawings of {name}: {error}"));
                            }
                            Ok(loaded) => {
                                if let Some((mut pending, merge)) = store.pending.remove(name) {
                                    let merged = merge
                                        && loaded.as_ref().is_some_and(|(drawings, _)| {
                                            !drawings.drawings().is_empty()
                                        });
                                    if merge && let Some((mut drawings, _)) = loaded {
                                        for mut drawing in pending.drawings().iter().cloned() {
                                            // Edits before the first read are new drawings, even if ids collide.
                                            while drawings.get_drawing(&drawing.id).is_some() {
                                                drawing.id.push_str("-local");
                                            }
                                            drawings.add_drawing(drawing);
                                        }
                                        pending = drawings;
                                    }
                                    match pending.to_json() {
                                        Ok(json) => {
                                            document.save(&*store.storage, json.clone());
                                            // Only a real merge needs to replace local drawings and history.
                                            if merged {
                                                store.loaded.insert(name.clone(), json);
                                            }
                                        }
                                        Err(error) => {
                                            store.error = Some(format!(
                                                "Could not save drawings of {name}: {error}"
                                            ))
                                        }
                                    }
                                } else if let Some(json) = json
                                    && (name == &symbol
                                        || self.symbol_drawings.contains_key(name)
                                        || loaded.is_some_and(|(drawings, _)| {
                                            !drawings.drawings().is_empty()
                                        }))
                                {
                                    store.loaded.insert(name.clone(), json);
                                }
                            }
                        }
                    }
                    Received::Failed(error) => store.error = Some(error),
                    // `storage_conflict` reports it until `resolve_storage_conflict` settles it.
                    Received::Conflict => {}
                    Received::Saved(_) => {
                        store
                            .changed
                            .insert(name.clone(), document.revision().cloned());
                    }
                }
            }
        }
        store.documents.retain(|name, document| {
            name == &symbol
                || self.symbol_drawings.contains_key(name)
                || store.loaded.contains_key(name)
                || store.pending.contains_key(name)
                || document.saving()
        });
        if let Some(json) = store.loaded.remove(&symbol) {
            self.restore_stored_drawings(&json);
        }
        self.poll_local_copies(ctx, busy);
        self.save_stored_drawings();
    }

    /// Replaces the drawings with the stored ones in `json`.
    fn restore_stored_drawings(&mut self, json: &str) {
        let previous = self.drawings.before_read();
        let loaded = self.load_drawings_remapped(json, true);
        if loaded.is_ok() {
            self.drawings.keep_across_read(previous);
        }
        let refused = match loaded {
            Ok(skipped) if skipped.is_empty() => None,
            Ok(skipped) => Some(format!("{} drawings could not be read", skipped.len())),
            Err(error) => Some(error.to_string()),
        };
        // Drawings whose study this chart does not show, shown in the price pane.
        let unshown: HashMap<_, _> = DrawingStorage::from_json(json)
            .map(|(stored, _)| {
                self.drawings
                    .drawings
                    .iter()
                    .filter(|drawing| drawing.pane.is_none())
                    .filter_map(|drawing| {
                        let id = drawing.stored_id.as_ref()?;
                        let record = stored.get_drawing(id)?;
                        Some((id.clone(), (record.created_at, record.pane.clone()?)))
                    })
                    .collect()
            })
            .unwrap_or_default();
        let store = self.drawing_store.as_mut().expect("drawing storage");
        if unshown.is_empty() {
            store.unshown_panes.remove(&self.symbol);
        } else {
            store.unshown_panes.insert(self.symbol.clone(), unshown);
        }
        if let Some(error) = refused {
            if let Some(document) = store.documents.get_mut(&self.symbol) {
                document.refuse_writes();
            }
            store.error = Some(format!(
                "Drawings of {} are not saved: {error}",
                self.symbol
            ));
        }
    }

    /// Writes the shown symbol's drawings if they changed since they were last written.
    pub(super) fn save_stored_drawings(&mut self) {
        if let Some(store) = &self.drawing_store {
            (store.save)(self);
        }
    }

    fn write_stored_drawings(&mut self) {
        let edits = self.history.drawing_edits;
        if self.symbol.is_empty()
            || self
                .drawing_store
                .as_ref()
                .is_none_or(|store| store.captured == edits)
        {
            return;
        }
        let mut drawings = self.save_drawings();
        let store = self.drawing_store.as_mut().expect("drawing storage");
        // A drawing whose study is not shown here stays in that study's pane for the charts
        // that show it.
        if let (Ok(drawings), Some(panes)) = (&mut drawings, store.unshown_panes.get(&self.symbol))
        {
            for record in &mut drawings.drawings {
                if record.pane.is_none()
                    && let Some((created_at, pane)) = panes.get(&record.id)
                    && *created_at == record.created_at
                {
                    record.pane = Some(pane.clone());
                }
            }
        }
        let document = store
            .documents
            .entry(self.symbol.clone())
            .or_insert_with(|| open_drawings(&*store.storage, &self.symbol));
        match drawings {
            Ok(drawings) if document.reading() => {
                let merge = store
                    .pending
                    .get(&self.symbol)
                    .is_none_or(|(_, merge)| *merge);
                store.pending.insert(self.symbol.clone(), (drawings, merge));
                store.captured = edits;
            }
            Ok(drawings) => match drawings.to_json() {
                Ok(json) => {
                    document.save(&*store.storage, json);
                    store.captured = edits;
                }
                Err(error) => store.error = Some(format!("Could not save drawings: {error}")),
            },
            Err(error) => store.error = Some(format!("Could not save drawings: {error}")),
        }
    }

    #[cfg(feature = "ui")]
    pub(super) fn save_restored_drawings(&mut self) {
        self.save_stored_drawings();
        if let Some(store) = &mut self.drawing_store {
            store.loaded.remove(&self.symbol);
            if let Some((_, merge)) = store.pending.get_mut(&self.symbol) {
                *merge = false;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{FeedError, Reply};
    use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
    use crate::model::{Bar, BarData};
    use crate::widget::{ChartLayout, ChartLayoutMode};
    use chrono::{TimeZone, Utc};
    use egui::{Pos2, RawInput, Rect, vec2};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(Default)]
    struct Host {
        documents: Mutex<HashMap<String, String>>,
        fail_writes: AtomicBool,
        delay_loads: AtomicBool,
        fail_loads: AtomicBool,
        loads: Mutex<Vec<(String, Reply<Option<String>>)>>,
        delay_writes: AtomicBool,
        writes: Mutex<Vec<Reply<()>>>,
    }

    impl Storage for Host {
        fn list(&self, _: StoreKind, reply: Reply<Vec<String>>) {
            reply.send(Ok(Vec::new()));
        }
        fn load(&self, kind: StoreKind, name: &str, reply: Reply<Option<String>>) {
            assert_eq!(kind, StoreKind::Drawings);
            if self.fail_loads.load(Ordering::Relaxed) {
                return reply.send(Err(FeedError::Network("offline".into())));
            }
            if self.delay_loads.load(Ordering::Relaxed) {
                self.loads.lock().unwrap().push((name.into(), reply));
            } else {
                reply.send(Ok(self.documents.lock().unwrap().get(name).cloned()));
            }
        }
        fn save(&self, _: StoreKind, name: &str, document: String, reply: Reply<()>) {
            if self.fail_writes.load(Ordering::Relaxed) {
                return reply.send(Err(FeedError::NotSupported));
            }
            self.documents.lock().unwrap().insert(name.into(), document);
            if self.delay_writes.load(Ordering::Relaxed) {
                self.writes.lock().unwrap().push(reply);
            } else {
                reply.send(Ok(()));
            }
        }
        fn delete(&self, _: StoreKind, _: &str, reply: Reply<()>) {
            reply.send(Ok(()));
        }
    }

    #[test]
    fn automatic_storage_does_not_fill_the_notification_queue() {
        let host = Arc::new(Host::default());
        let mut chart = Chart::new(BarData::from_bars(vec![Bar::new(
            Utc.timestamp_opt(0, 0).unwrap(),
            100.0,
            101.0,
            99.0,
            100.0,
            1.0,
        )]));
        chart.set_symbol("A");
        chart.set_storage(host.clone());
        let ctx = egui::Context::default();
        chart.sync_stored_drawings(&ctx);
        let mut line = Drawing::new(0, DrawingToolType::HorizontalLine);
        line.chart_points = vec![ChartPoint::new(0.0, 100.0)];
        let id = chart.user_action(|chart| chart.drawings_mut().add_drawing(line).unwrap());
        for i in 0..10_000 {
            chart.user_action(|chart| {
                chart.drawings_mut().edit_drawing(id, |drawing| {
                    drawing.chart_points[0].price = 100.0 + (i % 2) as f64;
                })
            });
            assert!(chart.drawings().pending_sync.lock().unwrap().is_empty());
            if i % 100 == 0 {
                chart.sync_stored_drawings(&ctx);
            }
        }
        assert!(chart.flush_stored_drawings());
        let json = host.documents.lock().unwrap()["A"].clone();
        assert_eq!(
            DrawingStorage::from_json(&json).unwrap().0.drawings()[0].points[0].price,
            101.0
        );
        for symbol in ["B", "A"] {
            chart.set_symbol(symbol);
            assert!(!chart.drawings.sync_enabled);
            chart.sync_stored_drawings(&ctx);
            assert!(
                !chart.drawings.sync_enabled,
                "loading keeps notifications disabled"
            );
        }
        chart.drawings_mut().set_sync_enabled(true);
        for symbol in ["B", "A"] {
            chart.set_symbol(symbol);
            assert!(chart.drawings.sync_enabled);
            chart.sync_stored_drawings(&ctx);
            assert!(
                chart.drawings.sync_enabled,
                "loading preserves the host's opt-in"
            );
        }
    }

    #[test]
    fn symbol_drawings_come_back_and_storage_failures_lose_nothing() {
        let ctx = egui::Context::default();
        let frame = |chart: &mut Chart| {
            ctx.run_ui(Default::default(), |ui| {
                chart.show(ui);
            })
            .drop_without_applying_deltas();
        };
        let new_chart = |host: &Arc<Host>| {
            let bars = (0..20)
                .map(|i| {
                    Bar::new(
                        Utc.timestamp_opt(i * 60, 0).unwrap(),
                        10.0,
                        11.0,
                        9.0,
                        10.0,
                        1.0,
                    )
                })
                .collect();
            let mut chart = Chart::new(BarData::from_bars(bars));
            chart.set_symbol("A");
            chart.set_storage(host.clone());
            chart
        };
        let draw = |chart: &mut Chart| {
            let mut drawing = Drawing::new(0, DrawingToolType::HorizontalLine);
            drawing.chart_points = vec![ChartPoint::new(4.0, 10.5)];
            drawing.completed = true;
            chart.user_action(|chart| chart.drawings_mut().add_synced_drawing(drawing));
        };
        let host = Arc::new(Host::default());
        host.documents
            .lock()
            .unwrap()
            .insert("B".into(), "{\"version\": 99}".into());
        let mut scanner = new_chart(&host);
        for i in 0..16 {
            let symbol = format!("EMPTY-{i}");
            if i % 2 == 0 {
                host.documents.lock().unwrap().insert(
                    symbol.clone(),
                    DrawingStorage::new(&symbol).to_json().unwrap(),
                );
            }
            scanner.set_symbol(symbol);
            frame(&mut scanner);
        }
        assert_eq!(
            scanner.drawing_store.as_ref().unwrap().documents.len(),
            1,
            "empty scans must retain only the active storage reader"
        );
        let mut chart = new_chart(&host);
        frame(&mut chart);
        draw(&mut chart);
        frame(&mut chart);
        assert!(host.documents.lock().unwrap().contains_key("A"));

        let mut transient = Drawing::new(0, DrawingToolType::HorizontalLine);
        transient.chart_points = vec![ChartPoint::new(4.0, 10.75)];
        transient.saveable = false;
        chart.user_action(|chart| chart.drawings_mut().add_drawing(transient).unwrap());
        for replay in 0..3 {
            match replay {
                1 => assert!(chart.undo()),
                2 => assert!(chart.redo()),
                _ => {}
            }
            frame(&mut chart);
            let json = host.documents.lock().unwrap()["A"].clone();
            let (saved, skipped) = DrawingStorage::from_json(&json).unwrap();
            assert!(skipped.is_empty());
            assert_eq!(saved.drawings().len(), 1, "transient drawing was saved");
        }

        chart.set_symbol("away");
        assert!(chart.forget_symbol("A"));
        assert!(
            !chart
                .drawing_store
                .as_ref()
                .unwrap()
                .documents
                .contains_key("A")
        );
        assert!(host.documents.lock().unwrap().contains_key("A"));
        chart.set_symbol("A");
        frame(&mut chart);
        assert_eq!(
            chart.drawings().drawings.len(),
            1,
            "forgotten state reloads storage"
        );

        // Empty state must keep its writer until both an edit and its deletion are acknowledged.
        chart.set_symbol("SAVING");
        frame(&mut chart);
        host.delay_writes.store(true, Ordering::Relaxed);
        draw(&mut chart);
        frame(&mut chart);
        chart.user_action(|chart| chart.drawings_mut().clear_all());
        chart.set_symbol("away");
        assert!(
            !chart.flush_stored_drawings(),
            "a delayed write is not acknowledged"
        );
        assert!(!chart.forget_symbol("SAVING"));
        for _ in 0..2 {
            for reply in std::mem::take(&mut *host.writes.lock().unwrap()) {
                reply.send(Ok(()));
            }
            frame(&mut chart);
        }
        assert!(chart.flush_stored_drawings());
        assert!(chart.forget_symbol("SAVING"));
        let (saved, skipped) =
            DrawingStorage::from_json(&host.documents.lock().unwrap()["SAVING"]).unwrap();
        assert!(saved.drawings().is_empty());
        assert!(skipped.is_empty());
        host.delay_writes.store(false, Ordering::Relaxed);

        let mut restarted = new_chart(&host);
        frame(&mut restarted);
        let restored = &restarted.drawings().drawings;
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].chart_points[0].bar_idx, 4.0);
        assert_eq!(restored[0].chart_points[0].price, 10.5);

        // A document this release cannot read is reported and never written over.
        restarted.set_symbol("B");
        frame(&mut restarted);
        assert!(restarted.take_storage_error().is_some());
        draw(&mut restarted);
        frame(&mut restarted);
        assert_eq!(host.documents.lock().unwrap()["B"], "{\"version\": 99}");

        // Failed writes keep the drawings and are reported once.
        host.fail_writes.store(true, Ordering::Relaxed);
        restarted.set_symbol("C");
        frame(&mut restarted);
        for _ in 0..2 {
            draw(&mut restarted);
            frame(&mut restarted);
            frame(&mut restarted);
        }
        assert!(restarted.take_storage_error().is_some());
        draw(&mut restarted);
        frame(&mut restarted);
        frame(&mut restarted);
        assert!(restarted.take_storage_error().is_none());
        assert_eq!(restarted.drawings().drawings.len(), 3);

        // Recovery without another edit must save the latest failed document.
        assert!(restarted.drawing_store.as_ref().unwrap().documents["C"].saving());
        host.fail_writes.store(false, Ordering::Relaxed);
        ctx.run_ui(
            egui::RawInput {
                time: Some(ctx.input(|input| input.time) + 2.0),
                ..Default::default()
            },
            |ui| {
                restarted.show(ui);
            },
        )
        .drop_without_applying_deltas();
        let json = host.documents.lock().unwrap()["C"].clone();
        let (saved, skipped) = DrawingStorage::from_json(&json).unwrap();
        assert!(skipped.is_empty());
        assert_eq!(saved.drawings().len(), 3);
        assert!(!restarted.drawing_store.as_ref().unwrap().documents["C"].saving());

        // An explicit flush retries immediately, even without another frame.
        host.fail_writes.store(true, Ordering::Relaxed);
        draw(&mut restarted);
        assert!(!restarted.flush_stored_drawings());
        host.fail_writes.store(false, Ordering::Relaxed);
        assert!(
            !restarted.flush_stored_drawings(),
            "unreadable B still prevents a complete flush"
        );
        assert!(!restarted.drawing_store.as_ref().unwrap().documents["C"].saving());
        let json = host.documents.lock().unwrap()["C"].clone();
        assert_eq!(
            DrawingStorage::from_json(&json).unwrap().0.drawings().len(),
            4
        );

        // A late read must preserve edits, including after switching away from the symbol.
        host.fail_writes.store(false, Ordering::Relaxed);
        host.delay_loads.store(true, Ordering::Relaxed);
        let mut delayed = new_chart(&host);
        frame(&mut delayed);
        #[cfg(feature = "ui")]
        assert!(
            delayed.snapshot().is_err(),
            "a layout must not capture drawings before their read completes"
        );
        draw(&mut delayed);
        delayed.set_symbol("D");
        assert!(
            !delayed.forget_symbol("A"),
            "pending edits must finish loading"
        );
        frame(&mut delayed);
        for (symbol, reply) in std::mem::take(&mut *host.loads.lock().unwrap()) {
            reply.send(Ok(host.documents.lock().unwrap().get(&symbol).cloned()));
        }
        frame(&mut delayed);
        assert!(delayed.drawings().drawings.is_empty());
        let json = host.documents.lock().unwrap()["A"].clone();
        let (saved, skipped) = DrawingStorage::from_json(&json).unwrap();
        assert!(skipped.is_empty());
        assert_eq!(saved.drawings().len(), 2);
        delayed.set_symbol("A");
        frame(&mut delayed);
        assert_eq!(delayed.drawings().drawings.len(), 2);
        // A different history window still projects the same timestamps and prices.
        let bars = delayed.bars(super::super::SeriesId::MAIN)[2..].to_vec();
        delayed.set_data(super::super::SeriesId::MAIN, bars);
        for drawing in &delayed.drawings().drawings {
            assert_eq!(drawing.chart_points[0].bar_idx, 2.0);
            assert_eq!(drawing.chart_points[0].price, 10.5);
        }
    }

    fn line_at(bar: f32, price: f64) -> Drawing {
        let mut line = Drawing::new(0, DrawingToolType::HorizontalLine);
        line.chart_points = vec![ChartPoint::new(bar, price)];
        line.completed = true;
        line
    }

    fn draw_line(chart: &mut Chart, line: Drawing) -> usize {
        chart.user_action(|chart| chart.drawings_mut().add_drawing(line).unwrap())
    }

    fn sorted(mut prices: Vec<f64>) -> Vec<f64> {
        prices.sort_by(f64::total_cmp);
        prices
    }

    fn shown_prices(chart: &Chart) -> Vec<f64> {
        let drawings = &chart.drawings().drawings;
        sorted(drawings.iter().map(|d| d.chart_points[0].price).collect())
    }

    /// A chart of symbol A on ten one-minute bars, keeping its drawings in `storage` if given.
    fn chart_on(storage: Option<Arc<dyn Storage>>) -> Chart {
        let bars = (0..10)
            .map(|i| {
                Bar::new(
                    Utc.timestamp_opt(i * 60, 0).unwrap(),
                    10.0,
                    11.0,
                    9.0,
                    10.0,
                    1.0,
                )
            })
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        chart.set_symbol("A");
        if let Some(storage) = storage {
            chart.set_storage(storage);
        }
        chart
    }

    /// Shows `charts` in `layout` for three frames over `seconds` of input time: a write, the
    /// read it causes, and a frame of rest.
    fn show_frames(
        ctx: &egui::Context,
        layout: &mut ChartLayout,
        charts: &mut [&mut Chart],
        seconds: f64,
    ) {
        for _ in 0..3 {
            let input = RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(900.0, 600.0))),
                time: Some(ctx.input(|input| input.time) + seconds / 3.0),
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                layout.show(ui, charts);
            })
            .drop_without_applying_deltas();
        }
    }

    /// The prices of the drawings stored as A.
    fn stored_prices(host: &Host) -> Vec<f64> {
        prices_in(&host.documents.lock().unwrap()["A"])
    }

    /// The prices of the drawings in a stored `document`.
    fn prices_in(document: &str) -> Vec<f64> {
        let (stored, _) = DrawingStorage::from_json(document).unwrap();
        sorted(
            stored
                .drawings()
                .iter()
                .map(|d| d.points[0].price)
                .collect(),
        )
    }

    /// Guards #1463 and #1604: two clients on a storage that keeps revisions, where the later
    /// write is refused and the user's choice settles it, and the copy kept is offered to load
    /// onto the chart or to discard.
    #[test]
    fn a_refused_drawings_write_waits_for_the_users_choice() {
        use crate::services::document::tests::Revised;
        let stored = |host: &Revised, name: &str| {
            host.get(name).map(|json| {
                let (stored, _) = DrawingStorage::from_json(&json).unwrap();
                sorted(
                    stored
                        .drawings()
                        .iter()
                        .map(|d| d.points[0].price)
                        .collect(),
                )
            })
        };
        let new_chart = |host: &Arc<Revised>| {
            let bar = Bar::new(Utc.timestamp_opt(0, 0).unwrap(), 10.0, 11.0, 9.0, 10.0, 1.0);
            let mut chart = Chart::new(BarData::from_bars(vec![bar]));
            chart.set_symbol("A");
            chart.set_storage(host.clone());
            assert!(chart.flush_stored_drawings());
            chart
        };
        // The choice, whether the other client deletes the document instead of adding to it, the
        // prices then stored, those of the kept copy, and those the chart shows.
        for (choice, deleted, expected, copy, shown) in [
            (
                ConflictChoice::ReloadRemote,
                false,
                Some(vec![10.0, 10.5]),
                None,
                vec![10.0, 10.5],
            ),
            (
                ConflictChoice::KeepLocalCopy,
                false,
                Some(vec![10.0, 10.5]),
                Some(vec![10.0, 11.0]),
                vec![10.0, 10.5],
            ),
            (
                ConflictChoice::Retry,
                false,
                Some(vec![10.0, 11.0]),
                None,
                vec![10.0, 11.0],
            ),
            (ConflictChoice::ReloadRemote, true, None, None, vec![]),
        ] {
            let host = Arc::new(Revised::default());
            let mut theirs = new_chart(&host);
            draw_line(&mut theirs, line_at(0.0, 10.0));
            assert!(theirs.flush_stored_drawings());
            let mut mine = new_chart(&host);
            assert_eq!(shown_prices(&mine), [10.0]);
            draw_line(&mut mine, line_at(0.0, 11.0));
            if deleted {
                host.remove("A");
            } else {
                draw_line(&mut theirs, line_at(0.0, 10.5));
                assert!(theirs.flush_stored_drawings());
            }
            assert!(!mine.flush_stored_drawings(), "{choice:?}");
            assert_eq!(mine.storage_conflict(), Some("A"));
            assert_eq!(shown_prices(&mine), [10.0, 11.0], "kept until the choice");
            mine.resolve_storage_conflict("A", choice);
            assert!(mine.flush_stored_drawings(), "{choice:?}");
            assert_eq!(mine.storage_conflict(), None);
            assert_eq!(stored(&host, "A"), expected, "{choice:?}");
            assert_eq!(stored(&host, "A (local copy)"), copy, "{choice:?}");
            assert_eq!(shown_prices(&mine), shown, "{choice:?}");
            // A change from elsewhere waits for the drawing in progress.
            if choice == ConflictChoice::Retry {
                theirs.drawings_mut().curr_drawing = Some(line_at(0.0, 12.0));
                theirs.flush_stored_drawings();
                assert_eq!(shown_prices(&theirs), [10.0, 10.5]);
                theirs.drawings_mut().curr_drawing = None;
                theirs.flush_stored_drawings();
                assert_eq!(shown_prices(&theirs), [10.0, 11.0]);
            }
            if choice == ConflictChoice::KeepLocalCopy {
                // Numbered copies are offered; names that are not copies of `A` are not.
                host.put("A (local copy 2)", &host.get("A").unwrap());
                let other = DrawingStorage::new("B").to_json().unwrap();
                for name in [
                    "A (local copy 3)",
                    "A (local copy 02)",
                    "AB (local copy)",
                    "A (local copy) (local copy)",
                ] {
                    host.put(name, &other);
                }
                assert_eq!(mine.local_copies(), None);
                mine.list_local_copies();
                assert!(mine.flush_stored_drawings());
                let listed = |chart: &Chart| chart.local_copies().map(<[String]>::to_vec);
                assert_eq!(
                    listed(&mine),
                    Some(
                        ["A (local copy)", "A (local copy 2)", "A (local copy 3)"]
                            .map(String::from)
                            .to_vec()
                    )
                );
                assert_eq!(mine.take_storage_error(), None);
                // Another symbol's drawings under a copy's name are neither loaded nor deleted.
                mine.load_local_copy("A (local copy 3)");
                mine.discard_local_copy("A (local copy 3)");
                assert!(mine.flush_stored_drawings());
                assert!(mine.take_storage_error().is_some());
                assert!(host.get("A (local copy 3)").is_some());
                assert_eq!(shown_prices(&mine), [10.0, 10.5]);
                // Loading replaces the saveable drawings in one undo step, written like any edit.
                let mut overlay = line_at(0.0, 9.0);
                overlay.saveable = false;
                draw_line(&mut mine, overlay);
                mine.load_local_copy("A (local copy)");
                assert!(mine.flush_stored_drawings());
                assert_eq!(shown_prices(&mine), [9.0, 10.0, 11.0]);
                assert_eq!(stored(&host, "A"), Some(vec![10.0, 11.0]));
                assert!(mine.undo());
                assert!(mine.flush_stored_drawings());
                assert_eq!(shown_prices(&mine), [9.0, 10.0, 10.5]);
                assert_eq!(stored(&host, "A"), Some(vec![10.0, 10.5]));
                // Only a listed copy is discarded.
                mine.discard_local_copy("A (local copy 02)");
                mine.discard_local_copy("A (local copy)");
                assert!(mine.flush_stored_drawings());
                assert_eq!(host.get("A (local copy)"), None);
                assert!(host.get("A (local copy 02)").is_some());
                assert_eq!(
                    listed(&mine),
                    Some(
                        ["A (local copy 2)", "A (local copy 3)"]
                            .map(String::from)
                            .to_vec()
                    )
                );
            }
        }
    }

    /// Guards #1463: two charts of one layout on one storage edit one symbol with drawing sync
    /// on, whether the host keeps revisions or not, without a refusal or a lost drawing; a
    /// drawing keeps its study's pane, and a reread keeps what an open dialog refers to.
    #[cfg(feature = "studies")]
    #[test]
    fn charts_of_one_layout_follow_each_others_stored_drawings() {
        use crate::services::document::tests::Revised;
        use crate::studies::{CustomIndicator, IndicatorValue};

        let unwatched = Revised::default();
        unwatched.2.store(true, Ordering::Relaxed);
        let hosts: [Arc<dyn Storage>; 2] = [Arc::new(Host::default()), Arc::new(unwatched)];
        for host in hosts {
            let new_chart = || {
                let bars = (0..10)
                    .map(|i| {
                        let time = Utc.timestamp_opt(i * 60, 0).unwrap();
                        Bar::new(time, 10.0, 11.0, 9.0, 10.0, 1.0)
                    })
                    .collect();
                let mut chart = Chart::new(BarData::from_bars(bars));
                chart.set_symbol("A");
                chart.set_storage(host.clone());
                chart
            };
            let (mut a, mut b) = (new_chart(), new_chart());
            let gamma = a.add_indicator(Box::new(
                CustomIndicator::new("Gamma", |bars: &[Bar]| {
                    vec![IndicatorValue::Single(40.0); bars.len()]
                })
                .with_overlay(false),
            ));
            let mut layout = ChartLayout::default();
            layout.mode = ChartLayoutMode::TwoHorizontal;
            layout.sync.sync_drawings = true;
            let ctx = egui::Context::default();
            // A write, the reread it causes, and a frame of rest.
            let mut frames = |a: &mut Chart, b: &mut Chart| {
                for _ in 0..3 {
                    let input = RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                        ..Default::default()
                    };
                    ctx.run_ui(input, |ui| {
                        layout.show(ui, &mut [&mut *a, &mut *b]);
                    })
                    .drop_without_applying_deltas();
                }
            };
            let find = |chart: &Chart, price: f64| {
                let drawings = &chart.drawings().drawings;
                drawings
                    .iter()
                    .find(|d| d.chart_points[0].price == price)
                    .cloned()
                    .unwrap()
            };
            frames(&mut a, &mut b);
            let mut study_line = line_at(4.0, 40.0);
            study_line.pane = Some(gamma.0);
            draw_line(&mut a, study_line);
            frames(&mut a, &mut b);
            assert_eq!(shown_prices(&a), [40.0]);
            assert_eq!(shown_prices(&b), [40.0]);

            draw_line(&mut b, line_at(4.0, 10.5));
            frames(&mut a, &mut b);
            assert_eq!(shown_prices(&a), [10.5, 40.0]);
            assert_eq!(shown_prices(&b), [10.5, 40.0]);
            assert_eq!(find(&a, 40.0).pane, Some(gamma.0));
            assert_eq!(find(&a, 10.5).pane, None);

            // Properties opened before a reread that removes one drawing and adds another.
            #[cfg(feature = "ui")]
            {
                use crate::ui::dialogs::{DrawingId, DrawingProps};
                let open = |chart: &Chart, price: f64| {
                    let drawing = find(chart, price);
                    let props = DrawingProps {
                        line_width: 3.5,
                        ..DrawingProps::from_drawing(&drawing, chart)
                    };
                    (DrawingId(drawing.id), props)
                };
                let (removed_id, removed) = open(&a, 40.0);
                let (kept_id, kept) = open(&a, 10.5);
                let gone = find(&b, 40.0).id;
                b.user_action(|chart| {
                    chart.drawings_mut().delete_drawing(gone);
                });
                draw_line(&mut b, line_at(4.0, 9.5));
                frames(&mut a, &mut b);
                assert_eq!(shown_prices(&a), [9.5, 10.5]);
                assert_eq!(find(&a, 10.5).id, kept_id.0);
                assert!(removed.apply_to_chart(removed_id, &mut a).is_err());
                let width = find(&a, 9.5).stroke_width;
                assert_eq!(find(&a, 10.5).stroke_width, width);
                kept.apply_to_chart(kept_id, &mut a).unwrap();
                assert_eq!(find(&a, 10.5).stroke_width, 3.5);
                assert_eq!(find(&a, 9.5).stroke_width, width);
            }

            // A drawing no document holds stays through a reread and reaches the other chart by
            // drawing sync.
            let mut volatile = line_at(4.0, 77.0);
            volatile.saveable = false;
            a.drawings_mut().add_synced_drawing(volatile);
            frames(&mut a, &mut b);
            draw_line(&mut b, line_at(4.0, 8.5));
            frames(&mut a, &mut b);
            let count = |chart: &Chart, price: f64| {
                shown_prices(chart).iter().filter(|p| **p == price).count()
            };
            for chart in [&a, &b] {
                assert_eq!(count(chart, 8.5), 1);
                assert_eq!(count(chart, 77.0), 1);
            }
        }
    }

    /// Guards #1614: a drawing read again never takes the local id of a drawing that is not
    /// saved, so removing that one by its id leaves the read drawing on both charts and stored.
    #[test]
    fn a_drawing_read_again_never_takes_the_id_of_one_that_is_not_saved() {
        let host = Arc::new(Host::default());
        let storage: Arc<dyn Storage> = host.clone();
        let (mut a, mut b) = (chart_on(Some(storage.clone())), chart_on(Some(storage)));
        let mut layout = ChartLayout::default();
        layout.mode = ChartLayoutMode::TwoHorizontal;
        let ctx = egui::Context::default();
        show_frames(&ctx, &mut layout, &mut [&mut a, &mut b], 0.1);
        // The host marks a level on B that no document holds, and gets id 0.
        let mut marker = line_at(4.0, 77.0);
        marker.saveable = false;
        let marker = b.drawings_mut().add_drawing(marker).unwrap();
        // A's first drawing is stored as "0", and B reads it.
        draw_line(&mut a, line_at(4.0, 10.5));
        show_frames(&ctx, &mut layout, &mut [&mut a, &mut b], 0.1);
        assert_eq!(shown_prices(&b), [10.5, 77.0]);
        b.drawings_mut().delete_drawing(marker);
        assert_eq!(shown_prices(&b), [10.5]);
        draw_line(&mut b, line_at(4.0, 9.0));
        show_frames(&ctx, &mut layout, &mut [&mut a, &mut b], 0.1);
        assert_eq!(
            [shown_prices(&a), shown_prices(&b)],
            [[9.0, 10.5], [9.0, 10.5]]
        );
        assert_eq!(stored_prices(&host), [9.0, 10.5]);
    }

    /// Guards #1615: an edit made while the chart reads its drawings again is not written before
    /// the read is answered, and the read answered after it does not replace the edit.
    #[test]
    fn an_edit_made_while_the_drawings_are_read_again_waits_for_the_read() {
        let host = Arc::new(Host::default());
        let storage: Arc<dyn Storage> = host.clone();
        let (mut a, mut b) = (chart_on(Some(storage.clone())), chart_on(Some(storage)));
        let mut layout = ChartLayout::default();
        layout.mode = ChartLayoutMode::TwoHorizontal;
        let ctx = egui::Context::default();
        show_frames(&ctx, &mut layout, &mut [&mut a, &mut b], 0.1);
        draw_line(&mut a, line_at(4.0, 10.0));
        show_frames(&ctx, &mut layout, &mut [&mut a, &mut b], 0.1);
        assert_eq!(shown_prices(&b), [10.0]);
        // A draws a line and undoes it, so A writes and B reads the drawings again; the host
        // has not answered that read yet.
        host.delay_loads.store(true, Ordering::Relaxed);
        draw_line(&mut a, line_at(4.0, 12.0));
        assert!(a.undo());
        show_frames(&ctx, &mut layout, &mut [&mut a, &mut b], 0.1);
        let (_, read) = host.loads.lock().unwrap().pop().expect("B reads again");
        let before = host.documents.lock().unwrap()["A"].clone();
        draw_line(&mut b, line_at(4.0, 11.0));
        show_frames(&ctx, &mut layout, &mut [&mut a, &mut b], 0.1);
        assert_eq!(stored_prices(&host), [10.0], "written during the read");
        host.delay_loads.store(false, Ordering::Relaxed);
        read.send(Ok(Some(before)));
        show_frames(&ctx, &mut layout, &mut [&mut a, &mut b], 0.1);
        assert_eq!(shown_prices(&b), [10.0, 11.0]);
        assert_eq!(stored_prices(&host), [10.0, 11.0]);
        assert!(b.can_undo(), "B read its own write again");
    }

    /// Guards #1617: a failed read of drawings another chart wrote is reported once and tried
    /// again each second, so the chart's next edit keeps them.
    #[test]
    fn a_failed_read_of_drawings_another_chart_wrote_is_reported_once_and_tried_again() {
        let host = Arc::new(Host::default());
        let storage: Arc<dyn Storage> = host.clone();
        let (mut a, mut b) = (chart_on(Some(storage.clone())), chart_on(Some(storage)));
        let mut layout = ChartLayout::default();
        layout.mode = ChartLayoutMode::TwoHorizontal;
        let ctx = egui::Context::default();
        show_frames(&ctx, &mut layout, &mut [&mut a, &mut b], 0.1);
        host.fail_loads.store(true, Ordering::Relaxed);
        draw_line(&mut a, line_at(4.0, 10.0));
        // A writes; B's read of it fails, and fails again a second later.
        show_frames(&ctx, &mut layout, &mut [&mut a, &mut b], 3.0);
        assert_eq!(
            b.take_storage_error().as_deref(),
            Some("Could not read A: network error: offline")
        );
        show_frames(&ctx, &mut layout, &mut [&mut a, &mut b], 3.0);
        assert_eq!(
            b.take_storage_error(),
            None,
            "a run of failed reads is reported once"
        );
        host.fail_loads.store(false, Ordering::Relaxed);
        show_frames(&ctx, &mut layout, &mut [&mut a, &mut b], 3.0);
        assert_eq!(shown_prices(&b), [10.0]);
        draw_line(&mut b, line_at(4.0, 11.0));
        show_frames(&ctx, &mut layout, &mut [&mut a, &mut b], 0.1);
        assert_eq!(stored_prices(&host), [10.0, 11.0]);
        // A write of B ends a run of failed reads, so the next failure is reported again.
        host.fail_loads.store(true, Ordering::Relaxed);
        draw_line(&mut a, line_at(4.0, 12.0));
        show_frames(&ctx, &mut layout, &mut [&mut a, &mut b], 0.1);
        assert!(b.take_storage_error().is_some());
        draw_line(&mut b, line_at(4.0, 13.0));
        draw_line(&mut a, line_at(4.0, 14.0));
        show_frames(&ctx, &mut layout, &mut [&mut a, &mut b], 0.1);
        assert!(b.take_storage_error().is_some(), "a new run is reported");
    }

    /// Guards #1619 and #1650: with drawing sync on, a chart outside the charts' shared storage
    /// shows each of their drawings once, and its own drawing, mirrored onto them but not
    /// stored, stays as it is while they read each other's writes. A storage that reports
    /// changes has a chart read another's write in the frame that write is made.
    #[test]
    fn a_chart_outside_the_storage_shows_each_drawing_once_and_keeps_its_own() {
        use crate::services::document::tests::Revised;

        for revised in [false, true] {
            let host = Arc::new(Host::default());
            let watched = Arc::new(Revised::default());
            let storage: Arc<dyn Storage> = if revised {
                watched.clone()
            } else {
                host.clone()
            };
            let (mut a, mut b) = (chart_on(Some(storage.clone())), chart_on(Some(storage)));
            let mut c = chart_on(None);
            let mut layout = ChartLayout::default();
            layout.mode = ChartLayoutMode::Three;
            layout.sync.sync_drawings = true;
            let ctx = egui::Context::default();
            let mut show = |a: &mut Chart, b: &mut Chart, c: &mut Chart| {
                show_frames(&ctx, &mut layout, &mut [a, b, c], 0.1);
            };
            show(&mut a, &mut b, &mut c);
            draw_line(&mut a, line_at(4.0, 10.5));
            show(&mut a, &mut b, &mut c);
            assert_eq!([shown_prices(&b), shown_prices(&c)], [[10.5], [10.5]]);
            let own = draw_line(&mut c, line_at(4.0, 9.0));
            show(&mut a, &mut b, &mut c);
            draw_line(&mut a, line_at(4.0, 8.0));
            show(&mut a, &mut b, &mut c);
            for chart in [&a, &b, &c] {
                assert_eq!(shown_prices(chart), [8.0, 9.0, 10.5]);
            }
            let own = c.drawings().drawings.iter().find(|d| d.id == own).unwrap();
            assert_eq!(own.chart_points[0].price, 9.0);
            assert_eq!(
                if revised {
                    prices_in(&watched.get("A").unwrap())
                } else {
                    stored_prices(&host)
                },
                [8.0, 10.5],
                "a mirrored drawing is not stored"
            );
        }
    }
}
