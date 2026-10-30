//! A deterministic feed, in-memory storage and frame helpers for the terminal's tests.

use super::{Action, Terminal};
use chrono::{NaiveTime, TimeDelta, TimeZone, Utc, Weekday};
use egui::{Event, Key, Modifiers, PointerButton, Pos2, RawInput, Rect, Vec2};
use egui_charts::data::{Datafeed, HistoryPage, HistoryRequest, Reply};
use egui_charts::model::{Bar, Instrument, Session, SessionWindow, TimezoneId};
use egui_charts::services::{Revision, SaveOutcome, Storage, StoreKind, Versioned};
use std::collections::HashMap;
use std::sync::Mutex;

/// A window size in which every top toolbar control fits.
pub(super) const SIZE: Vec2 = Vec2::new(1440.0, 900.0);

/// History of 500 bars ending on 2 January 2026 for every symbol and interval, without live
/// updates. AAPL trades on weekdays in New York, EURUSD on weekdays around the clock and
/// BTCUSD always.
pub(super) struct TestFeed;

impl TestFeed {
    const BARS: i32 = 500;

    fn instrument(symbol: &str) -> Instrument {
        let weekdays = [
            Weekday::Mon,
            Weekday::Tue,
            Weekday::Wed,
            Weekday::Thu,
            Weekday::Fri,
        ];
        let time = |hour, minute| NaiveTime::from_hms_opt(hour, minute, 0).unwrap();
        let instrument = Instrument::new(symbol);
        match symbol {
            "AAPL" => instrument
                .with_exchange("NASDAQ")
                .with_price_decimals(2)
                .with_time_zone(
                    TimezoneId::named("America/New_York").unwrap_or_else(TimezoneId::utc),
                )
                .with_session(Session::new([SessionWindow::new(
                    weekdays,
                    time(9, 30),
                    time(16, 0),
                )])),
            "EURUSD" => instrument
                .with_exchange("FX")
                .with_price_decimals(5)
                .with_session(Session::new([SessionWindow::new(
                    weekdays,
                    time(0, 0),
                    time(0, 0),
                )])),
            "BTCUSD" => instrument
                .with_exchange("Crypto")
                .with_price_decimals(2)
                .with_session(Session::new(Vec::new())),
            _ => instrument,
        }
    }

    fn base_price(symbol: &str) -> f64 {
        match symbol {
            "EURUSD" => 1.08,
            "BTCUSD" => 60_000.0,
            _ => 180.0,
        }
    }
}

impl Datafeed for TestFeed {
    fn resolve(&self, symbol: &str, reply: Reply<Instrument>) {
        reply.send(Ok(Self::instrument(symbol)));
    }

    fn history(&self, request: HistoryRequest, reply: Reply<HistoryPage>) {
        let end = Utc.with_ymd_and_hms(2026, 1, 2, 0, 0, 0).unwrap();
        let step = TimeDelta::milliseconds(request.timeframe.duration_ms());
        let base = Self::base_price(&request.symbol);
        let close = |i: i32| base * (1.0 + 0.02 * (f64::from(i) * 0.3).sin());
        let bars: Vec<_> = (0..Self::BARS)
            .map(|i| {
                let (open, close) = (close(i - 1), close(i));
                Bar::new(
                    end - step * (Self::BARS - i),
                    open,
                    open.max(close) * 1.002,
                    open.min(close) * 0.998,
                    close,
                    1_000.0 + f64::from(i % 7) * 100.0,
                )
            })
            .filter(|bar| request.before.is_none_or(|before| bar.time < before))
            .collect();
        let start = bars.len().saturating_sub(request.count);
        reply.send(Ok(HistoryPage::new(bars[start..].to_vec(), start == 0)));
    }
}

/// Documents held in memory with a revision each, answered at once.
#[derive(Default)]
pub(super) struct MemoryStorage(Mutex<HashMap<(StoreKind, String), Versioned>>);

impl MemoryStorage {
    /// The document `name` of `kind`.
    pub(super) fn get(&self, kind: StoreKind, name: &str) -> Option<String> {
        let documents = self.0.lock().unwrap();
        documents
            .get(&(kind, name.into()))
            .map(|stored| stored.document.clone())
    }

    /// Stores `document` as `name` of `kind` at the next revision, and returns that revision.
    fn put(&self, kind: StoreKind, name: &str, document: String) -> Revision {
        let mut documents = self.0.lock().unwrap();
        let key = (kind, name.to_owned());
        let next = documents
            .get(&key)
            .map_or(1, |stored| stored.revision.0.parse::<u64>().unwrap() + 1);
        let revision = Revision(next.to_string());
        documents.insert(
            key,
            Versioned {
                document,
                revision: revision.clone(),
            },
        );
        revision
    }
}

impl Storage for MemoryStorage {
    fn list(&self, kind: StoreKind, reply: Reply<Vec<String>>) {
        let names = self
            .0
            .lock()
            .unwrap()
            .keys()
            .filter(|(k, _)| *k == kind)
            .map(|(_, name)| name.clone())
            .collect();
        reply.send(Ok(names));
    }

    fn load(&self, kind: StoreKind, name: &str, reply: Reply<Option<String>>) {
        reply.send(Ok(self.get(kind, name)));
    }

    fn save(&self, kind: StoreKind, name: &str, document: String, reply: Reply<()>) {
        self.put(kind, name, document);
        reply.send(Ok(()));
    }

    fn delete(&self, kind: StoreKind, name: &str, reply: Reply<()>) {
        self.0.lock().unwrap().remove(&(kind, name.into()));
        reply.send(Ok(()));
    }

    fn load_versioned(&self, kind: StoreKind, name: &str, reply: Reply<Option<Versioned>>) {
        let stored = self.0.lock().unwrap().get(&(kind, name.into())).cloned();
        reply.send(Ok(stored));
    }

    fn save_versioned(
        &self,
        kind: StoreKind,
        name: &str,
        document: String,
        expected: Option<Revision>,
        reply: Reply<SaveOutcome>,
    ) {
        let stored = self.0.lock().unwrap().get(&(kind, name.into())).cloned();
        if stored.as_ref().map(|stored| &stored.revision) != expected.as_ref() {
            return reply.send(Ok(SaveOutcome::Conflict(stored)));
        }
        reply.send(Ok(SaveOutcome::Saved(self.put(kind, name, document))));
    }
}

/// What one frame returned and the accessible labels it laid out.
pub(super) struct Frame {
    /// What [`Terminal::show`] returned.
    pub(super) actions: Vec<Action>,
    labels: Vec<(String, Pos2)>,
}

impl Frame {
    /// The centre of the first control or text labelled `label`.
    pub(super) fn text_at(&self, label: &str) -> Option<Pos2> {
        self.labels
            .iter()
            .find_map(|(text, pos)| (text == label).then_some(*pos))
    }
}

/// Runs one frame of `terminal` in a window of `size`, a tenth of a second after the last.
pub(super) fn frame(terminal: &mut Terminal, size: Vec2, events: Vec<Event>) -> Frame {
    frame_with(terminal, size, events, |_| {})
}

/// Runs one frame like [`frame`], with the input adjusted by `input`.
pub(super) fn frame_with(
    terminal: &mut Terminal,
    size: Vec2,
    events: Vec<Event>,
    input: impl FnOnce(&mut RawInput),
) -> Frame {
    let ctx = terminal.ctx.clone().unwrap_or_default();
    ctx.enable_accesskit();
    let mut raw = RawInput {
        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
        time: Some(ctx.input(|i| i.time) + 0.1),
        events,
        ..Default::default()
    };
    input(&mut raw);
    let mut actions = Vec::new();
    let output = ctx.run_ui(raw, |ui| actions = terminal.show(ui));
    let labels = output
        .platform_output
        .accesskit_update
        .iter()
        .flat_map(|update| &update.nodes)
        .filter_map(|(_, node)| {
            let bounds = node.bounds()?;
            let centre = Pos2::new(
                ((bounds.x0 + bounds.x1) / 2.0) as f32,
                ((bounds.y0 + bounds.y1) / 2.0) as f32,
            );
            Some((node.label()?.to_owned(), centre))
        })
        .collect();
    output.drop_without_applying_deltas();
    Frame { actions, labels }
}

/// A press of `key` with `modifiers`.
pub(super) fn key(key: Key, modifiers: Modifiers) -> Event {
    Event::Key {
        key,
        physical_key: None,
        pressed: true,
        repeat: false,
        modifiers,
    }
}

/// Moves the pointer to `pos` and clicks there, over three frames, and returns what they
/// returned.
pub(super) fn click(terminal: &mut Terminal, size: Vec2, pos: Pos2) -> Vec<Action> {
    let mut actions = frame(terminal, size, vec![Event::PointerMoved(pos)]).actions;
    for pressed in [true, false] {
        let button = Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        };
        actions.extend(frame(terminal, size, vec![button]).actions);
    }
    actions
}
