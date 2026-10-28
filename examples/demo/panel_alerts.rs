//! Example host: the crate's alert engine behind the demo's symbol catalog, and alert presets in storage.

use std::sync::Mutex;

use egui_charts::data::Reply;
use egui_charts::model::{Alert, AlertId};
use egui_charts::services::{AlertEngine, AlertEvent, Alerts, Events};

/// The demo's alerts: an [`AlertEngine`] that only takes alerts on symbols of the catalog and
/// tells `notify` the symbol of each trigger, and the storage of the alert presets.
pub struct DemoAlerts {
    engine: AlertEngine,
    documents: Mutex<std::collections::HashMap<(egui_charts::services::StoreKind, String), String>>,
}

impl DemoAlerts {
    /// Starts with `alerts` from the previous session.
    pub fn new(alerts: Vec<Alert>, notify: impl Fn(&str) + Send + Sync + 'static) -> Self {
        Self {
            engine: AlertEngine::new()
                .with_alerts(alerts)
                .on_trigger(move |_, event| {
                    if let AlertEvent::TriggeredForSymbol { symbol, .. } = event {
                        notify(symbol);
                    }
                }),
            documents: Mutex::default(),
        }
    }

    /// The engine that evaluates the alerts.
    pub fn engine(&self) -> &AlertEngine {
        &self.engine
    }
}

impl Alerts for DemoAlerts {
    fn watch(&self, events: Events<AlertEvent>) {
        self.engine.watch(events);
    }

    fn save(&self, alert: Alert, reply: Reply<()>) {
        if alert.watchlist.is_none()
            && !crate::sampledata::DemoSymbol::catalog()
                .iter()
                .any(|symbol| symbol.symbol == alert.symbol)
        {
            reply.send(Err(egui_charts::data::FeedError::NotSupported));
            return;
        }
        self.engine.save(alert, reply);
    }

    fn remove(&self, id: AlertId, reply: Reply<()>) {
        self.engine.remove(id, reply);
    }
}

impl egui_charts::services::Storage for DemoAlerts {
    fn list(&self, kind: egui_charts::services::StoreKind, reply: Reply<Vec<String>>) {
        reply.send(Ok(self
            .documents
            .lock()
            .unwrap()
            .keys()
            .filter(|(k, _)| *k == kind)
            .map(|(_, name)| name.clone())
            .collect()));
    }
    fn load(
        &self,
        kind: egui_charts::services::StoreKind,
        name: &str,
        reply: Reply<Option<String>>,
    ) {
        reply.send(Ok(self
            .documents
            .lock()
            .unwrap()
            .get(&(kind, name.into()))
            .cloned()));
    }
    fn save(
        &self,
        kind: egui_charts::services::StoreKind,
        name: &str,
        document: String,
        reply: Reply<()>,
    ) {
        self.documents
            .lock()
            .unwrap()
            .insert((kind, name.into()), document);
        reply.send(Ok(()));
    }
    fn delete(&self, kind: egui_charts::services::StoreKind, name: &str, reply: Reply<()>) {
        self.documents.lock().unwrap().remove(&(kind, name.into()));
        reply.send(Ok(()));
    }
}
