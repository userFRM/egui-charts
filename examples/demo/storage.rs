//! The demo's storage service: every document in one map that eframe keeps, in a file natively
//! and in the browser's local storage on the web, written out as soon as a document changes.

use egui_charts::data::Reply;
use egui_charts::services::{Storage, StoreKind};
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

/// The eframe key of the map, where earlier releases of the demo kept templates.
const KEY: &str = "templates";

#[derive(Default)]
pub(super) struct DemoStorage {
    documents: Mutex<HashMap<String, String>>,
    /// Whether a document changed since the map was last written out.
    changed: AtomicBool,
}

impl DemoStorage {
    /// The documents eframe kept, with the drawings earlier releases kept under their own key.
    pub fn restore(storage: Option<&dyn eframe::Storage>) -> Self {
        let mut documents: HashMap<String, String> = storage
            .and_then(|storage| eframe::get_value(storage, KEY))
            .unwrap_or_default();
        let drawings: HashMap<String, String> = storage
            .and_then(|storage| eframe::get_value(storage, "drawings"))
            .unwrap_or_default();
        for (symbol, json) in drawings {
            documents
                .entry(key(StoreKind::Drawings, &symbol))
                .or_insert(json);
        }
        Self {
            documents: Mutex::new(documents),
            changed: AtomicBool::new(false),
        }
    }

    /// Puts the documents in eframe's storage, which writes them out on exit and periodically.
    pub fn persist(&self, storage: &mut dyn eframe::Storage) {
        eframe::set_value(storage, KEY, &*self.documents.lock().unwrap());
        storage.remove_string("drawings");
    }

    /// Writes the documents out now if one changed, so that a save is kept before the next
    /// periodic write.
    pub fn flush(&self, storage: Option<&mut (dyn eframe::Storage + 'static)>) {
        if let Some(storage) = storage
            && self.changed.swap(false, Ordering::Relaxed)
        {
            self.persist(storage);
            storage.flush();
        }
    }

    pub(super) fn save_comparisons(
        &self,
        name: &str,
        comparisons: &[Vec<super::comparison::SavedComparison>],
    ) -> Result<(), String> {
        let document = serde_json::to_string(comparisons).map_err(|error| error.to_string())?;
        self.documents
            .lock()
            .unwrap()
            .insert(format!("layout-comparisons/{name}"), document);
        self.changed.store(true, Ordering::Relaxed);
        Ok(())
    }

    pub(super) fn comparisons(
        &self,
        name: &str,
    ) -> Result<Vec<Vec<super::comparison::SavedComparison>>, String> {
        self.documents
            .lock()
            .unwrap()
            .get(&format!("layout-comparisons/{name}"))
            .map_or_else(
                || Ok(Vec::new()),
                |json| {
                    serde_json::from_str(json)
                        .map_err(|error| format!("Could not open comparisons: {error}"))
                },
            )
    }

    fn write(&self, kind: StoreKind, name: &str, document: Option<String>) {
        let mut documents = self.documents.lock().unwrap();
        match document {
            Some(document) => documents.insert(key(kind, name), document),
            None => documents.remove(&key(kind, name)),
        };
        self.changed.store(true, Ordering::Relaxed);
    }
}

fn key(kind: StoreKind, name: &str) -> String {
    format!("{kind:?}/{name}")
}

impl Storage for DemoStorage {
    fn list(&self, kind: StoreKind, reply: Reply<Vec<String>>) {
        let prefix = key(kind, "");
        reply.send(Ok(self
            .documents
            .lock()
            .unwrap()
            .keys()
            .filter_map(|key| key.strip_prefix(&prefix).map(str::to_owned))
            .collect()));
    }
    fn load(&self, kind: StoreKind, name: &str, reply: Reply<Option<String>>) {
        reply.send(Ok(self
            .documents
            .lock()
            .unwrap()
            .get(&key(kind, name))
            .cloned()));
    }
    fn save(&self, kind: StoreKind, name: &str, document: String, reply: Reply<()>) {
        self.write(kind, name, Some(document));
        reply.send(Ok(()));
    }
    fn delete(&self, kind: StoreKind, name: &str, reply: Reply<()>) {
        self.write(kind, name, None);
        if kind == StoreKind::Layouts {
            self.documents
                .lock()
                .unwrap()
                .remove(&format!("layout-comparisons/{name}"));
        }
        reply.send(Ok(()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::DemoApp;
    use eframe::App;
    use egui_charts::ui::dialogs::StudyContextMenuAction;
    use egui_charts::widget::SeriesId;

    #[derive(Default)]
    struct AppStorage(HashMap<String, String>);

    impl eframe::Storage for AppStorage {
        fn get_string(&self, key: &str) -> Option<String> {
            self.0.get(key).cloned()
        }
        fn set_string(&mut self, key: &str, value: String) {
            self.0.insert(key.into(), value);
        }
        fn remove_string(&mut self, key: &str) {
            self.0.remove(key);
        }
        fn flush(&mut self) {}
    }

    fn restart(ctx: &egui::Context, storage: &AppStorage) -> DemoApp {
        let mut cc = eframe::CreationContext::_new_kittest(ctx.clone());
        cc.storage = Some(storage);
        DemoApp::new_at(&cc, "2026-10-27T19:29:50Z".parse().unwrap(), true)
    }

    #[test]
    fn study_favorites_survive_restart_and_removal_with_legacy_settings() {
        let ctx = egui::Context::default();
        let mut storage = AppStorage::default();
        eframe::set_value(
            &mut storage,
            KEY,
            &HashMap::from([("Settings/settings", r#"{"version":1,"interval":"Min5"}"#)]),
        );
        let mut app = restart(&ctx, &storage);
        assert!(app.indicator_dialog.favorites.is_empty());
        assert_eq!(app.state.timeframe, egui_charts::model::Timeframe::Min5);
        app.indicator_dialog.favorites.insert("RSI".into());
        app.handle_study_context(
            &ctx,
            SeriesId::MAIN,
            StudyContextMenuAction::AddToFavorites("SMA".into()),
        );
        // Keep stars for host studies that are temporarily absent from the catalogue.
        app.indicator_dialog.favorites.insert("Host study".into());
        for expected in [["RSI", "SMA"].as_slice(), ["SMA"].as_slice()] {
            app.save(&mut storage);
            app = restart(&ctx, &storage);
            ctx.run_ui(Default::default(), |_| app.show_templates(&ctx))
                .drop_without_applying_deltas();
            let mut favorites: Vec<_> = app
                .top_toolbar
                .favorite_indicators
                .iter()
                .map(|(id, _)| id.as_str())
                .collect();
            favorites.sort_unstable();
            assert_eq!(favorites, expected);
            assert!(app.indicator_dialog.favorites.contains("Host study"));
            app.indicator_dialog.favorites.remove("RSI");
        }
    }
}
