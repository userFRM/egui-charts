//! Each symbol's drawings kept in the host's storage.

use super::Chart;
use crate::drawings::persistence::DrawingStorage;
use crate::services::document::{Received, StoredDocument};
use crate::services::{Storage, StoreKind};
use std::collections::HashMap;
use std::sync::Arc;

pub(super) struct DrawingStore {
    storage: Arc<dyn Storage>,
    /// The document of each symbol shown since storage was attached.
    documents: HashMap<String, StoredDocument>,
    /// Drawings read for a symbol that was no longer shown when they arrived.
    loaded: HashMap<String, (String, bool)>,
    /// The chart's drawing edit count when the shown symbol's drawings were last saved.
    saved: u64,
    /// Edits made before a read finishes. The flag distinguishes edits from a restored layout.
    pending: HashMap<String, (DrawingStorage, bool)>,
    error: Option<String>,
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
    /// the symbol changes.
    /// Drawings that could not all be read are never written over. Charts that show the same
    /// symbol write the same document, so share their drawings through
    /// [`ChartLayout`](super::ChartLayout) drawing sync or give storage to one of them.
    pub fn set_storage(&mut self, storage: Arc<dyn Storage>) {
        self.drawing_store = Some(DrawingStore {
            storage,
            documents: HashMap::new(),
            loaded: HashMap::new(),
            saved: self.history.drawing_edits,
            pending: HashMap::new(),
            error: None,
        });
    }

    /// The error of a failed read or write of drawings since the last call, to show to the
    /// user. A failed write keeps the drawings on the chart and is reported once.
    pub fn take_storage_error(&mut self) -> Option<String> {
        self.drawing_store.as_mut()?.error.take()
    }

    /// Reads the shown symbol's drawings when it is first shown and saves edited drawings.
    pub(super) fn sync_stored_drawings(&mut self, ctx: &egui::Context) {
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
            for received in document.poll(&*store.storage, ctx) {
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
                                            store.loaded.insert(name.clone(), (json, merge));
                                        }
                                        Err(error) => {
                                            store.error = Some(format!(
                                                "Could not save drawings of {name}: {error}"
                                            ))
                                        }
                                    }
                                } else if let Some(json) = json {
                                    store.loaded.insert(name.clone(), (json, true));
                                }
                            }
                        }
                    }
                    Received::Failed(error) => store.error = Some(error),
                }
            }
        }
        if let Some((json, remap)) = store.loaded.remove(&symbol) {
            self.restore_stored_drawings(&json, remap);
        }
        self.save_stored_drawings();
    }

    /// Replaces the drawings with the stored ones in `json`.
    fn restore_stored_drawings(&mut self, json: &str, remap: bool) {
        let refused = match self.load_drawings(json) {
            Ok(skipped) if skipped.is_empty() => None,
            Ok(skipped) => Some(format!("{} drawings could not be read", skipped.len())),
            Err(error) => Some(error.to_string()),
        };
        if remap {
            self.remap_drawing_panes();
        }
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
        let edits = self.history.drawing_edits;
        if self.symbol.is_empty()
            || self
                .drawing_store
                .as_ref()
                .is_none_or(|store| store.saved == edits)
        {
            return;
        }
        let drawings = self.save_drawings();
        let store = self.drawing_store.as_mut().expect("drawing storage");
        store.saved = edits;
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
            }
            Ok(drawings) => match drawings.to_json() {
                Ok(json) => document.save(&*store.storage, json),
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
            reply.send(Ok(()));
        }
        fn delete(&self, _: StoreKind, _: &str, reply: Reply<()>) {
            reply.send(Ok(()));
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
        let mut chart = new_chart(&host);
        frame(&mut chart);
        draw(&mut chart);
        frame(&mut chart);
        assert!(host.documents.lock().unwrap().contains_key("A"));

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
