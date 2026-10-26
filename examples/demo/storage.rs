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
        reply.send(Ok(()));
    }
}
