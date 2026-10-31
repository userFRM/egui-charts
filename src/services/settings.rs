//! The user's settings, kept in one document of the host's storage.

use super::document::{Received, StoredDocument};
use super::{ConflictChoice, Storage, StoreKind};
use crate::config::DrawingToolType;
use crate::model::{ReplaySession, Timeframe, Watchlist};
use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;
use std::time::Duration;

/// What the library keeps of the user between sessions, in the [`StoreKind::Settings`] document
/// `settings` that [`SettingsStore`] reads and writes.
///
/// Build it with `Default` and assign fields: later releases add fields without a breaking change.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
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
    /// Whether the user hid the favourites strip with its close or toggle button.
    pub drawing_favorites_hidden: bool,
    /// The top-left corner of the favourites strip, in logical points, where the user left it;
    /// `None` until it has been shown, when it starts beside the drawing toolbar.
    pub drawing_favorites_pos: Option<(f32, f32)>,
    /// Whether the user dismissed the drawing toolbar's card that explains the magnet; a document
    /// without it reads as not dismissed.
    pub magnet_hint_dismissed: bool,
    /// The layout open last, to open again on start.
    pub layout: Option<String>,
    /// Whether the open layout is saved after each change.
    pub layout_autosave: bool,
    /// The names of the starred layouts.
    pub layout_favorites: Vec<String>,
    /// Factory names of the starred studies of the indicator dialog, in order.
    pub indicator_favorites: Vec<String>,
    /// The intervals starred in the interval menu, in order.
    pub interval_favorites: Vec<Timeframe>,
    /// The properties each drawing tool starts with, keyed by tool, as the JSON of
    /// its `DrawingTemplate`. A tool without an entry starts with
    /// the built-in defaults.
    pub drawing_defaults: BTreeMap<String, serde_json::Value>,
    /// The watchlists, in order.
    pub watchlists: Vec<Watchlist>,
    /// The market heatmap's universe, size metric and colour metric.
    pub market_heatmap: MarketHeatmapSelection,
    /// The replay position to resume, with the interval its history was fetched at.
    pub replay_session: Option<(ReplaySession, Timeframe)>,
}

/// What the market heatmap shows, by the identifiers the host gave its universes and metrics.
/// `None` is the panel's default. An identifier the host no longer offers stays chosen, so the
/// choice returns when the host offers it again.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
#[non_exhaustive]
pub struct MarketHeatmapSelection {
    /// The universe shown.
    pub universe: Option<String>,
    /// The metric that sizes the tiles.
    pub size: Option<String>,
    /// The metric that colours the tiles.
    pub colour: Option<String>,
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
/// [`take_error`](Self::take_error); a later poll or explicit flush retries it. Settings that
/// could not be read are never written over. If the host keeps revisions of documents (see
/// [`Storage`]), a change by another client is read once nothing is unsaved and returned like the
/// first read, and a write over a newer version is refused: [`conflict`](Self::conflict) then
/// asks for the user's choice and [`resolve_conflict`](Self::resolve_conflict) applies it.
pub struct SettingsStore {
    storage: Arc<dyn Storage>,
    document: StoredDocument,
    /// The settings confirmed in storage; `None` until read.
    written: Option<UserSettings>,
    /// Settings that differ from `written`, with the input time they were first seen.
    changed: Option<(UserSettings, f64)>,
    /// Stored settings read since the last poll, for the host to apply.
    restored: Option<UserSettings>,
    error: Option<String>,
}

impl SettingsStore {
    /// How long settings stay the same before they are written.
    pub const DELAY: Duration = Duration::from_secs(1);

    /// Starts reading the settings from `storage`.
    pub fn new(storage: Arc<dyn Storage>) -> Self {
        let mut document = StoredDocument::open(&*storage, StoreKind::Settings, "settings");
        document.follow_changes(&*storage);
        Self {
            document,
            storage,
            written: None,
            changed: None,
            restored: None,
            error: None,
        }
    }

    /// Call every frame with the settings as they are. Returns the stored settings in the frame
    /// they arrive, for the host to apply: once read, and again after a change by another client
    /// or a conflict settled by reloading; until the first read nothing is written. Writes
    /// `current` once it has stayed the same for [`DELAY`](Self::DELAY), and at once while a
    /// conflict is unsettled, so that the choice applies to the newest settings.
    pub fn poll(&mut self, ctx: &egui::Context, current: &UserSettings) -> Option<UserSettings> {
        self.document.hold(
            self.written
                .as_ref()
                .is_some_and(|written| written != current),
        );
        let received = self.document.poll(&*self.storage, ctx);
        self.receive(received);
        if let Some(settings) = self.restored.take() {
            self.changed = None;
            // Remote changes wait until the host has applied these.
            ctx.request_repaint();
            return Some(settings);
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
        if wait > 0.0 && !self.document.conflicted() {
            ctx.request_repaint_after(Duration::from_secs_f64(wait));
        } else {
            self.document.save(&*self.storage, current.to_json());
        }
        None
    }

    /// Writes `current` now if it differs from the stored settings, such as when the
    /// application exits. Nothing is written while stored settings that [`poll`](Self::poll)
    /// has not returned yet are pending, nor while a conflict is unsettled.
    pub fn flush(&mut self, current: &UserSettings) {
        if self.written.is_none() {
            return;
        }
        // `current` may hold changes made after the last poll.
        self.document.hold(self.written.as_ref() != Some(current));
        let received = self.document.receive(&*self.storage);
        self.receive(received);
        if self.restored.is_some() {
            return;
        }
        if self
            .written
            .as_ref()
            .is_some_and(|written| written != current || self.document.saving())
        {
            self.changed = None;
            self.document.save(&*self.storage, current.to_json());
        }
        let received = self.document.flush(&*self.storage);
        self.receive(received);
    }

    /// Takes in what the settings document received.
    fn receive(&mut self, received: Vec<Received>) {
        for received in received {
            match received {
                Received::Loaded(document) => {
                    match document.as_deref().map(UserSettings::from_json).transpose() {
                        Ok(settings) => {
                            // Before the first read, a missing document leaves the host's own.
                            if settings.is_some() || self.written.is_some() {
                                self.restored = Some(settings.clone().unwrap_or_default());
                            }
                            self.written = Some(settings.unwrap_or_default());
                        }
                        Err(error) => {
                            self.document.refuse_writes();
                            self.error = Some(format!("Could not read settings: {error}"));
                        }
                    }
                }
                Received::Failed(error) => self.error = Some(error),
                // Reported by `conflict`.
                Received::Conflict => {}
                Received::Saved(document) => {
                    self.written =
                        Some(UserSettings::from_json(&document).expect("saved settings"));
                }
            }
        }
    }

    /// Whether the host refused to write the settings because another client changed them, and
    /// the user's choice is awaited. The settings stay in memory and nothing is written until
    /// [`resolve_conflict`](Self::resolve_conflict) settles it.
    pub fn conflict(&self) -> bool {
        self.document.conflicted() && !self.document.copying()
    }

    /// Settles a refused write with the user's choice. Reloading the stored settings returns them
    /// from the next [`poll`](Self::poll), like the first read; keeping a local copy saves the
    /// unsaved settings as a new document first, named `settings (local copy)`, or
    /// `settings (local copy 2)` and so on if that name is taken.
    pub fn resolve_conflict(&mut self, choice: ConflictChoice) {
        self.document.resolve(&*self.storage, choice);
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
    use crate::services::document::tests::Revised;
    use std::sync::Mutex;

    /// The default settings with the layout `name` open.
    fn layout(name: &str) -> UserSettings {
        UserSettings {
            layout: Some(name.into()),
            ..UserSettings::default()
        }
    }

    /// Polls `store` at input time `time` with the host's settings `current`.
    fn poll_at(
        ctx: &egui::Context,
        store: &mut SettingsStore,
        time: f64,
        current: &UserSettings,
    ) -> Option<UserSettings> {
        let input = egui::RawInput {
            time: Some(time),
            ..Default::default()
        };
        let mut restored = None;
        ctx.run_ui(input, |ui| restored = store.poll(ui.ctx(), current))
            .drop_without_applying_deltas();
        restored
    }

    #[test]
    fn remote_changes_are_returned_but_never_replace_unsaved_settings() {
        // Whether this client's change is flushed before a poll has seen it.
        for flush in [false, true] {
            let ctx = egui::Context::default();
            let host = Arc::new(Revised::default());
            host.put("settings", &layout("base").to_json());
            let mut store = SettingsStore::new(host.clone());
            // Another client changes them before the first read is taken in.
            host.put("settings", &layout("theirs").to_json());
            assert_eq!(
                poll_at(&ctx, &mut store, 0.0, &UserSettings::default()),
                Some(layout("theirs"))
            );
            // And again while this client waits to write its own.
            let mine = layout("mine");
            if flush {
                host.put("settings", &layout("newest").to_json());
                store.flush(&mine);
            } else {
                assert_eq!(poll_at(&ctx, &mut store, 0.1, &mine), None);
                host.put("settings", &layout("newest").to_json());
            }
            for time in [0.2, 1.2, 1.3] {
                assert_eq!(poll_at(&ctx, &mut store, time, &mine), None, "{flush}");
            }
            assert!(store.conflict(), "{flush}");
            let stored = UserSettings::from_json(&host.get("settings").unwrap()).unwrap();
            assert_eq!(stored, layout("newest"), "{flush}");
        }
    }

    #[test]
    fn settings_read_before_the_next_poll_are_returned_and_not_written_over() {
        // Whether another client deletes the settings rather than changing them, whether this
        // client had unsaved settings and reloads, and whether it flushes before the next poll.
        for (deleted, reload, flush) in [
            (true, true, false),
            (false, true, true),
            (false, false, true),
        ] {
            let ctx = egui::Context::default();
            let host = Arc::new(Revised::default());
            host.put("settings", &layout("base").to_json());
            let mut store = SettingsStore::new(host.clone());
            poll_at(&ctx, &mut store, 0.0, &UserSettings::default());
            let current = layout(if reload { "mine" } else { "base" });
            if deleted {
                host.remove("settings");
            } else {
                host.put("settings", &layout("theirs").to_json());
            }
            if reload {
                for time in [0.1, 1.2, 1.3] {
                    assert_eq!(poll_at(&ctx, &mut store, time, &current), None);
                }
                assert!(store.conflict());
                store.resolve_conflict(ConflictChoice::ReloadRemote);
            }
            if flush {
                store.flush(&current);
            }
            let theirs = if deleted {
                UserSettings::default()
            } else {
                layout("theirs")
            };
            let case = (deleted, reload, flush);
            assert_eq!(
                poll_at(&ctx, &mut store, 1.4, &current),
                Some(theirs.clone()),
                "{case:?}"
            );
            assert_eq!(poll_at(&ctx, &mut store, 3.0, &theirs), None, "{case:?}");
            let stored = host
                .get("settings")
                .map(|json| UserSettings::from_json(&json).unwrap());
            assert_eq!(stored, (!deleted).then_some(theirs), "{case:?}");
        }
    }

    #[derive(Default)]
    struct Host(Mutex<Option<String>>);

    #[derive(Default)]
    struct DelayedHost(Mutex<Vec<(String, Reply<()>)>>);

    impl Storage for DelayedHost {
        fn list(&self, _: StoreKind, _: Reply<Vec<String>>) {
            unreachable!()
        }
        fn load(&self, _: StoreKind, _: &str, reply: Reply<Option<String>>) {
            reply.send(Ok(None));
        }
        fn save(&self, _: StoreKind, _: &str, document: String, reply: Reply<()>) {
            self.0.lock().unwrap().push((document, reply));
        }
        fn delete(&self, _: StoreKind, _: &str, _: Reply<()>) {
            unreachable!()
        }
    }

    #[test]
    fn failed_saves_remain_dirty_and_pending_edits_can_return_to_stored_settings() {
        let host = Arc::new(DelayedHost::default());
        let mut store = SettingsStore::new(host.clone());
        let ctx = egui::Context::default();
        let original = UserSettings::default();
        store.poll(&ctx, &original);
        let mut current = original.clone();
        current.symbol = Some("A".into());
        for attempt in 0..3 {
            store.flush(&current);
            store.flush(&current);
            let (json, reply) = host
                .0
                .lock()
                .unwrap()
                .pop()
                .expect("dirty settings must be retried");
            assert_eq!(UserSettings::from_json(&json).unwrap(), current);
            if attempt < 2 {
                reply.send(Err(crate::data::FeedError::Network("temporary".into())));
                store.poll(&ctx, &current);
                assert_eq!(store.take_error().is_some(), attempt == 0);
            } else {
                reply.send(Ok(()));
            }
        }
        store.flush(&current);
        assert!(
            host.0.lock().unwrap().is_empty(),
            "confirmed settings need no write"
        );
        current.symbol = Some("B".into());
        store.flush(&current);
        let (_, reply) = host.0.lock().unwrap().pop().unwrap();
        current.symbol = Some("A".into());
        store.flush(&current);
        reply.send(Ok(()));
        store.flush(&current);
        let (json, reply) = host.0.lock().unwrap().pop().unwrap();
        assert_eq!(UserSettings::from_json(&json).unwrap(), current);
        reply.send(Ok(()));
        store.flush(&current);
        assert!(host.0.lock().unwrap().is_empty());
    }

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
        settings.market_heatmap.size = Some("cap".into());
        settings.drawing_favorites_pos = Some((12.0, 34.0));
        settings.magnet_hint_dismissed = true;
        for time in [0.5, 1.0, 1.4] {
            assert_eq!(poll(&mut store, time, &settings), None);
        }
        assert!(host.0.lock().unwrap().is_none());
        poll(&mut store, 1.5, &settings);
        let written = host.0.lock().unwrap().clone().unwrap();
        assert!(written.contains("\"TrendLine\""));
        assert_eq!(
            UserSettings::from_json(&written)
                .unwrap()
                .market_heatmap
                .size,
            Some("cap".into())
        );

        // Exit can follow an inline save before another frame polls its acknowledgement.
        settings.symbol = Some("MSFT".into());
        store.flush(&settings);
        let written = host.0.lock().unwrap().clone().unwrap();
        assert_eq!(UserSettings::from_json(&written).unwrap(), settings);

        // Settings saved before the magnet card existed restore unchanged, with the card not
        // dismissed.
        let older = written.replace(",\"magnet_hint_dismissed\":true", "");
        assert_ne!(older, written);
        let mut expected = settings.clone();
        expected.magnet_hint_dismissed = false;
        assert_eq!(UserSettings::from_json(&older).unwrap(), expected);

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
