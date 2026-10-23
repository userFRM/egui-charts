//! Session-only alert storage for the demo panel. No prices are evaluated here.

use std::sync::Mutex;

use egui_charts::data::Reply;
use egui_charts::model::{Alert, AlertId};
use egui_charts::services::{AlertEvent, Alerts, Events};

#[derive(Default)]
pub struct DemoAlerts(Mutex<State>);

#[derive(Default)]
struct State {
    alerts: Vec<Alert>,
    events: Option<Events<AlertEvent>>,
    next_id: u64,
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
