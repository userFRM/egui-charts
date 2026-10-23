//! Session-only alert and preset storage for the demo host.

use std::sync::Mutex;

use egui_charts::data::Reply;
use egui_charts::model::{Alert, AlertId};
use egui_charts::services::{AlertEvent, Alerts, Events};

/// Session-only storage used by the demo alerts panel.
#[derive(Default)]
pub struct DemoAlerts(Mutex<State>);

#[derive(Default)]
struct State {
    alerts: Vec<Alert>,
    events: Option<Events<AlertEvent>>,
    next_id: u64,
    documents: std::collections::HashMap<(egui_charts::services::StoreKind, String), String>,
}

impl Alerts for DemoAlerts {
    fn watch(&self, events: Events<AlertEvent>) {
        let mut state = self.0.lock().unwrap();
        for alert in &state.alerts {
            events.push(AlertEvent::Upsert(alert.clone()));
        }
        state.events = Some(events);
    }

    fn save(&self, mut alert: Alert, reply: Reply<()>) {
        let mut state = self.0.lock().unwrap();
        if alert.id.is_none() {
            state.next_id += 1;
            alert.id = Some(AlertId(state.next_id.to_string()));
        }
        state.alerts.retain(|a| a.id != alert.id);
        state.alerts.push(alert.clone());
        if let Some(events) = &state.events {
            events.push(AlertEvent::Upsert(alert));
        }
        reply.send(Ok(()));
    }

    fn remove(&self, id: AlertId, reply: Reply<()>) {
        let mut state = self.0.lock().unwrap();
        state.alerts.retain(|a| a.id.as_ref() != Some(&id));
        if let Some(events) = &state.events {
            events.push(AlertEvent::Remove(id));
        }
        reply.send(Ok(()));
    }
}

impl egui_charts::services::Storage for DemoAlerts {
    fn list(&self, kind: egui_charts::services::StoreKind, reply: Reply<Vec<String>>) {
        let state = self.0.lock().unwrap();
        reply.send(Ok(state
            .documents
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
            .0
            .lock()
            .unwrap()
            .documents
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
        self.0
            .lock()
            .unwrap()
            .documents
            .insert((kind, name.into()), document);
        reply.send(Ok(()));
    }
    fn delete(&self, kind: egui_charts::services::StoreKind, name: &str, reply: Reply<()>) {
        self.0
            .lock()
            .unwrap()
            .documents
            .remove(&(kind, name.into()));
        reply.send(Ok(()));
    }
}
