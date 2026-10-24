//! Each symbol's drawings kept in the host's storage.

use super::Chart;
use crate::services::document::{Received, StoredDocument};
use crate::services::{Storage, StoreKind};
use std::collections::HashMap;
use std::sync::Arc;

pub(super) struct DrawingStore {
    storage: Arc<dyn Storage>,
    /// The document of each symbol shown since storage was attached.
    documents: HashMap<String, StoredDocument>,
    /// Drawings read for a symbol that was no longer shown when they arrived.
    loaded: HashMap<String, String>,
    /// The chart's drawing edit count when the shown symbol's drawings were last saved.
    saved: u64,
    error: Option<String>,
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
                    Received::Loaded(Some(json)) => {
                        store.loaded.insert(name.clone(), json);
                    }
                    Received::Loaded(None) => {}
                    Received::Failed(error) => store.error = Some(error),
                }
            }
        }
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
        let edits = self.history.drawing_edits;
        // Edits made while the drawings are read are saved once the document is read.
        if self.drawing_store.as_ref().is_none_or(|store| {
            store.saved == edits
                || store
                    .documents
                    .get(&self.symbol)
                    .is_none_or(StoredDocument::reading)
        }) {
            return;
        }
        let json = self.save_drawings().and_then(|drawings| drawings.to_json());
        let store = self.drawing_store.as_mut().expect("drawing storage");
        store.saved = edits;
        match json {
            Ok(json) => {
                if let Some(document) = store.documents.get_mut(&self.symbol) {
                    document.save(&*store.storage, json);
                }
            }
            Err(error) => store.error = Some(format!("Could not save drawings: {error}")),
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
    }

    impl Storage for Host {
        fn list(&self, _: StoreKind, reply: Reply<Vec<String>>) {
            reply.send(Ok(Vec::new()));
        }
        fn load(&self, kind: StoreKind, name: &str, reply: Reply<Option<String>>) {
            assert_eq!(kind, StoreKind::Drawings);
            reply.send(Ok(self.documents.lock().unwrap().get(name).cloned()));
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
    }
}
