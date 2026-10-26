//! The user's settings, kept in one document of the host's storage.

use super::document::{Received, StoredDocument};
use super::{Storage, StoreKind};
use crate::config::DrawingToolType;
use crate::model::Timeframe;
use serde::{Deserialize, Deserializer, Serialize};
use std::sync::Arc;
use std::time::Duration;

/// What the library keeps of the user between sessions, in the [`StoreKind::Settings`] document
/// `settings` that [`SettingsStore`] reads and writes.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UserSettings {
    /// Name of the theme preset, as listed by [`presets`](crate::theme::presets).
    pub theme: Option<String>,
    /// The symbol shown last.
    pub symbol: Option<String>,
    /// The interval shown last.
    pub interval: Option<Timeframe>,
    /// The drawing tools on the drawing toolbar's favourites bar, in order. A tool this release
    /// does not know is left out when reading.
    #[serde(deserialize_with = "known_tools")]
    pub drawing_favorites: Vec<DrawingToolType>,
    /// The layout open last, to open again on start.
    pub layout: Option<String>,
    /// Whether the open layout is saved after each change.
    pub layout_autosave: bool,
}

fn known_tools<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<DrawingToolType>, D::Error> {
    Ok(Vec::<serde_json::Value>::deserialize(deserializer)?
        .into_iter()
        .filter_map(|tool| serde_json::from_value(tool).ok())
        .collect())
}

#[derive(Serialize, Deserialize)]
struct Document<T> {
    version: u32,
    #[serde(flatten)]
    settings: T,
}

impl UserSettings {
    /// The version of the document [`to_json`](Self::to_json) writes.
    pub const VERSION: u32 = 1;

    /// The settings as a versioned JSON document.
    pub fn to_json(&self) -> String {
        serde_json::to_string(&Document {
            version: Self::VERSION,
            settings: self,
        })
        .expect("settings always serialize")
    }

    /// Reads a document [`to_json`](Self::to_json) wrote, migrating older versions. Fails on
    /// invalid JSON and on a version this release does not know. A missing field takes its
    /// default.
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let document: Document<serde_json::Value> = serde_json::from_str(json)?;
        match document.version {
            Self::VERSION => serde_json::from_value(document.settings),
            version => Err(serde::de::Error::custom(format!(
                "unsupported settings version {version}, expected {}",
                Self::VERSION
            ))),
        }
    }
}

/// Keeps [`UserSettings`] in the host's [`Storage`]: reads them once, then writes them a second
/// after they last changed. A failed write keeps the settings in memory and is reported once by
/// [`take_error`](Self::take_error); the next change writes again. Settings that could not be
/// read are never written over.
pub struct SettingsStore {
    storage: Arc<dyn Storage>,
    document: StoredDocument,
    /// The settings in storage, or being written; `None` until read.
    written: Option<UserSettings>,
    /// Settings that differ from `written`, with the input time they were first seen.
    changed: Option<(UserSettings, f64)>,
    error: Option<String>,
}

impl SettingsStore {
    /// How long settings stay the same before they are written.
    pub const DELAY: Duration = Duration::from_secs(1);

    /// Starts reading the settings from `storage`.
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        Self {
            document: StoredDocument::open(&*storage, StoreKind::Settings, "settings"),
            storage,
            written: None,
            changed: None,
            error: None,
        }
    }

    /// Call every frame with the settings as they are. Returns the stored settings once, in the
    /// frame they arrive, for the host to apply; until then nothing is written. Afterwards writes
    /// `current` once it has stayed the same for [`DELAY`](Self::DELAY).
    pub fn poll(&mut self, ctx: &egui::Context, current: &UserSettings) -> Option<UserSettings> {
        for received in self.document.poll(&*self.storage, ctx) {
            match received {
                Received::Loaded(document) => {
                    match document.as_deref().map(UserSettings::from_json).transpose() {
                        Ok(settings) => {
                            self.written = Some(settings.clone().unwrap_or_default());
                            if settings.is_some() {
                                return settings;
                            }
                        }
                        Err(error) => {
                            self.document.refuse_writes();
                            self.error = Some(format!("Could not read settings: {error}"));
                        }
                    }
                }
                Received::Failed(error) => self.error = Some(error),
            }
        }
        if self
            .written
            .as_ref()
            .is_none_or(|written| written == current)
        {
            self.changed = None;
            return None;
        }
        let now = ctx.input(|i| i.time);
        let since = match &self.changed {
            Some((settings, since)) if settings == current => *since,
            _ => {
                self.changed = Some((current.clone(), now));
                now
            }
        };
        let wait = Self::DELAY.as_secs_f64() - (now - since);
        if wait > 0.0 {
            ctx.request_repaint_after(Duration::from_secs_f64(wait));
        } else {
            self.flush(current);
        }
        None
    }

    /// Writes `current` now if it differs from the stored settings, such as when the
    /// application exits.
    pub fn flush(&mut self, current: &UserSettings) {
        if self.written.is_some() {
            for received in self.document.receive(&*self.storage) {
                if let Received::Failed(error) = received {
                    self.error = Some(error);
                }
            }
        }
        if self
            .written
            .as_ref()
            .is_some_and(|written| written != current)
        {
            self.changed = None;
            self.written = Some(current.clone());
            self.document.save(&*self.storage, current.to_json());
        }
    }

    /// The error of a failed read or write since the last call, to show to the user.
    pub fn take_error(&mut self) -> Option<String> {
        self.error.take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Reply;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Host(Mutex<Option<String>>);

    impl Storage for Host {
        fn list(&self, _: StoreKind, reply: Reply<Vec<String>>) {
            reply.send(Ok(Vec::new()));
        }
        fn load(&self, kind: StoreKind, name: &str, reply: Reply<Option<String>>) {
            assert_eq!((kind, name), (StoreKind::Settings, "settings"));
            reply.send(Ok(self.0.lock().unwrap().clone()));
        }
        fn save(&self, _: StoreKind, _: &str, document: String, reply: Reply<()>) {
            *self.0.lock().unwrap() = Some(document);
            reply.send(Ok(()));
        }
        fn delete(&self, _: StoreKind, _: &str, reply: Reply<()>) {
            reply.send(Ok(()));
        }
    }

    #[test]
    fn settings_restore_once_wait_a_second_and_keep_unreadable_documents() {
        let ctx = egui::Context::default();
        let poll = |store: &mut SettingsStore, time: f64, current: &UserSettings| {
            let input = egui::RawInput {
                time: Some(time),
                ..Default::default()
            };
            let mut restored = None;
            ctx.run_ui(input, |ui| restored = store.poll(ui.ctx(), current))
                .drop_without_applying_deltas();
            restored
        };
        let host = Arc::new(Host::default());
        let mut store = SettingsStore::new(host.clone());
        let mut settings = UserSettings::default();
        assert_eq!(poll(&mut store, 0.0, &settings), None);
        settings.symbol = Some("AAPL".into());
        settings.drawing_favorites = vec![DrawingToolType::TrendLine];
        for time in [0.5, 1.0, 1.4] {
            assert_eq!(poll(&mut store, time, &settings), None);
        }
        assert!(host.0.lock().unwrap().is_none());
        poll(&mut store, 1.5, &settings);
        let written = host.0.lock().unwrap().clone().unwrap();
        assert!(written.contains("\"TrendLine\""));

        // Exit can follow an inline save before another frame polls its acknowledgement.
        settings.symbol = Some("MSFT".into());
        store.flush(&settings);
        let written = host.0.lock().unwrap().clone().unwrap();
        assert_eq!(UserSettings::from_json(&written).unwrap(), settings);

        // A favourite tool from a newer release is left out; the rest is restored.
        let document = written.replace("\"TrendLine\"", "\"TrendLine\",\"FutureTool\"");
        *host.0.lock().unwrap() = Some(document);
        let mut store = SettingsStore::new(host.clone());
        assert_eq!(
            poll(&mut store, 0.0, &UserSettings::default()),
            Some(settings.clone())
        );

        let newer = written.replacen("\"version\":1", "\"version\":2", 1);
        *host.0.lock().unwrap() = Some(newer.clone());
        let mut store = SettingsStore::new(host.clone());
        assert_eq!(poll(&mut store, 0.0, &settings), None);
        assert!(store.take_error().is_some());
        poll(&mut store, 5.0, &UserSettings::default());
        store.flush(&UserSettings::default());
        assert_eq!(host.0.lock().unwrap().as_deref(), Some(newer.as_str()));
    }
}
