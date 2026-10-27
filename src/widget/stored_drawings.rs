//! Each symbol's drawings kept in the host's storage.

use super::Chart;
use crate::drawings::persistence::DrawingStorage;
use crate::services::document::{Received, StoredDocument};
use crate::services::{Storage, StoreKind};
use std::collections::HashMap;
use std::sync::Arc;

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
        self.documents.shrink_to_fit();
        self.loaded.shrink_to_fit();
        true
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

impl Chart {
    /// Keeps each symbol's drawings in `storage`, in the [`StoreKind::Drawings`] document named
    /// by the symbol. The drawings of a symbol are read when it is first shown and, when a
    /// document holds some, replace the chart's drawings and undo history; they are written after
    /// every drawing edit made through chart history (every user edit, undo and redo) and when
    /// the symbol changes. Failed writes retain the latest document and retry after one second
    /// of input time; [`Self::flush_stored_drawings`] retries immediately.
    /// Drawings that could not all be read are never written over. Charts that show the same
    /// symbol write the same document, so share their drawings through
    /// [`ChartLayout`](super::ChartLayout) drawing sync or give storage to one of them.
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
        });
    }

    /// Captures current drawing edits and retries failed writes for every symbol immediately.
    /// Available replies are processed, but asynchronous storage is not awaited. Returns `true`
    /// once all captured edits are acknowledged. Keep rendering or call again to finish delayed
    /// reads and writes; inspect [`Self::take_storage_error`] for failures.
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
    /// user. A failed write keeps the drawings on the chart and is reported once.
    pub fn take_storage_error(&mut self) -> Option<String> {
        self.drawing_store.as_mut()?.error.take()
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
        let Some(store) = &mut self.drawing_store else {
            return;
        };
        if !symbol.is_empty() && !store.documents.contains_key(&symbol) {
            let document = StoredDocument::open(&*store.storage, StoreKind::Drawings, &symbol);
            store.documents.insert(symbol.clone(), document);
        }
        for (name, document) in &mut store.documents {
            let received = match ctx {
                Some(ctx) => document.poll(&*store.storage, ctx),
                None => document.flush(&*store.storage),
            };
            for received in received {
                match received {
                    Received::Loaded(json) => {
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
                    Received::Saved(_) => {}
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
        self.save_stored_drawings();
    }

    /// Replaces the drawings with the stored ones in `json`.
    fn restore_stored_drawings(&mut self, json: &str) {
        let refused = match self.load_drawings(json) {
            Ok(skipped) if skipped.is_empty() => None,
            Ok(skipped) => Some(format!("{} drawings could not be read", skipped.len())),
            Err(error) => Some(error.to_string()),
        };
        self.remap_drawing_panes();
        let store = self.drawing_store.as_mut().expect("drawing storage");
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
        let drawings = self.save_drawings();
        let store = self.drawing_store.as_mut().expect("drawing storage");
        let document = store
            .documents
            .entry(self.symbol.clone())
            .or_insert_with(|| {
                StoredDocument::open(&*store.storage, StoreKind::Drawings, &self.symbol)
            });
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

    /// Points drawings in study panes at the studies that took the place of the saved ones
    /// when a snapshot was restored.
    pub(super) fn remap_drawing_panes(&mut self) {
        for drawing in &mut self.drawings.drawings {
            if let Some(id) = drawing
                .pane
                .and_then(|pane| self.restored_studies.get(&pane))
            {
                drawing.pane = Some(*id);
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
    use chrono::{TimeZone, Utc};
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(Default)]
    struct Host {
        documents: Mutex<HashMap<String, String>>,
        fail_writes: AtomicBool,
        delay_loads: AtomicBool,
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
}
