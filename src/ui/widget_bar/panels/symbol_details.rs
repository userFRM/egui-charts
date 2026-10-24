//! Facts about the chart's symbol: its quote, the key statistics and profile the feed supplies,
//! and the user's notes, kept through the storage service.

use std::collections::HashMap;
use std::sync::Arc;

use egui::{Align, Label, Layout, RichText, Ui};
use serde::{Deserialize, Serialize};

use super::object_tree::{format_price, price_decimals};
use super::watchlist::{WatchlistQuote, signed};
use crate::data::inbox::Inbox;
use crate::data::{Datafeed, FeedError, SymbolDetails};
use crate::model::Instrument;
use crate::services::{Storage, StoreKind};
use crate::theme::ThemeExt;
use crate::ui_kit::PanelHeader;
use crate::ui_kit::status_message::{MessageKind, status_message};

/// Text size of the last price, the panel's largest figure.
const PRICE_SIZE: f32 = 24.0;

#[derive(Serialize, Deserialize)]
struct NotesDocument {
    version: u32,
    notes: String,
}

/// The notes of one symbol.
#[derive(Default)]
struct Notes {
    text: String,
    /// The text last read from or sent to storage, `None` until it is read. Writes wait for a
    /// successful read, so a failed read never lets an empty field replace the stored notes.
    written: Option<String>,
    saving: bool,
}

/// What an inbox answer is for.
enum Request {
    Details(String),
    Load(String),
    Save(String, String),
}

/// Details of the chart's symbol, fed by the host's [`Datafeed`] and [`Storage`].
///
/// Keep it on the chart's symbol with [`set_instrument`](Self::set_instrument) and its latest
/// quote with [`set_quote`](Self::set_quote). For each new symbol the panel asks the feed for
/// [`Datafeed::symbol_details`] and shows the statistics and profile it answers; an empty section,
/// or a feed that does not support the request, shows nothing. Notes are kept in memory per
/// symbol, and with a storage service attached they are read from and written to one
/// [`StoreKind::SymbolNotes`] document per symbol as they are edited, one write per document at a
/// time. Call [`poll`](Self::poll) each frame, even while the panel is hidden, then
/// [`show`](Self::show) while it is open.
#[derive(Default)]
pub struct SymbolDetailsPanel {
    instrument: Option<Instrument>,
    quote: Option<WatchlistQuote>,
    details: Option<SymbolDetails>,
    error: Option<String>,
    notes: HashMap<String, Notes>,
    feed: Option<Arc<dyn Datafeed>>,
    storage: Option<Arc<dyn Storage>>,
    inbox: Arc<Inbox>,
    requests: HashMap<u64, Request>,
    next: u64,
}

impl SymbolDetailsPanel {
    /// A panel without a symbol, feed or storage.
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask `feed` for the details of each symbol from now on, starting with the shown one.
    pub fn set_feed(&mut self, feed: Arc<dyn Datafeed>) {
        self.feed = Some(feed);
        if let Some(symbol) = self.symbol().map(str::to_owned) {
            self.request_details(symbol);
        }
    }

    /// Keep notes in `storage`, reading them again from it. Edits not yet written are dropped.
    pub fn set_storage(&mut self, storage: Arc<dyn Storage>) {
        self.storage = Some(storage);
        self.notes.clear();
        self.requests
            .retain(|_, request| matches!(request, Request::Details(_)));
        if let Some(symbol) = self.symbol().map(str::to_owned) {
            self.load_notes(symbol);
        }
    }

    /// Show `instrument`, such as the chart's. A new symbol clears the quote and asks for its
    /// details and notes; the same symbol only takes the new name, exchange and currency.
    pub fn set_instrument(&mut self, instrument: &Instrument) {
        let new = self.symbol() != Some(instrument.symbol.as_str());
        self.instrument = Some(instrument.clone());
        if new {
            self.quote = None;
            self.details = None;
            self.error = None;
            self.request_details(instrument.symbol.clone());
            // Notes not read yet and not being read, such as after a failed read, are read again.
            let loading = self.requests.values().any(
                |request| matches!(request, Request::Load(symbol) if *symbol == instrument.symbol),
            );
            if !loading
                && self
                    .notes
                    .get(&instrument.symbol)
                    .is_none_or(|notes| notes.written.is_none())
            {
                self.load_notes(instrument.symbol.clone());
            }
        }
    }

    /// Sets the latest quote of the shown symbol.
    pub fn set_quote(&mut self, quote: WatchlistQuote) {
        self.quote = Some(quote);
    }

    /// The shown symbol, if any.
    pub fn symbol(&self) -> Option<&str> {
        self.instrument
            .as_ref()
            .map(|instrument| instrument.symbol.as_str())
    }

    fn key(&mut self, request: Request) -> u64 {
        let key = self.next;
        self.next += 1;
        self.requests.insert(key, request);
        key
    }

    fn request_details(&mut self, symbol: String) {
        if let Some(feed) = self.feed.clone() {
            let key = self.key(Request::Details(symbol.clone()));
            feed.symbol_details(&symbol, self.inbox.reply(key));
        }
    }

    fn load_notes(&mut self, symbol: String) {
        let Some(storage) = self.storage.clone() else {
            // Without storage there is nothing to read: the notes start empty and stay in memory.
            self.notes.entry(symbol).or_default().written = Some(String::new());
            return;
        };
        self.notes.insert(symbol.clone(), Notes::default());
        let key = self.key(Request::Load(symbol.clone()));
        storage.load(StoreKind::SymbolNotes, &symbol, self.inbox.reply(key));
    }

    /// Receive the feed's and storage's answers, write edited notes, and wake the UI when the
    /// next answer arrives.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        let shown = self.symbol().map(str::to_owned);
        let current = |symbol: &str| shown.as_deref() == Some(symbol);
        for (key, answer) in self.inbox.drain().answers {
            let Some(request) = self.requests.remove(&key) else {
                continue;
            };
            match request {
                Request::Details(symbol) => {
                    let result = *answer
                        .downcast::<Result<SymbolDetails, FeedError>>()
                        .expect("symbol details");
                    if !current(&symbol) {
                        continue;
                    }
                    match result {
                        Ok(details) => self.details = Some(details),
                        Err(FeedError::NotSupported) => {}
                        Err(error) => self.error = Some(format!("Details unavailable: {error}")),
                    }
                }
                Request::Load(symbol) => {
                    let result = answer
                        .downcast::<Result<Option<String>, FeedError>>()
                        .expect("notes load")
                        .map_err(|error| error.to_string())
                        .and_then(|document| {
                            let Some(document) = document else {
                                return Ok(String::new());
                            };
                            let document: NotesDocument = serde_json::from_str(&document)
                                .map_err(|error| error.to_string())?;
                            if document.version != 1 {
                                return Err("unsupported notes version".into());
                            }
                            Ok(document.notes)
                        });
                    match result {
                        Ok(text) => {
                            let notes = self.notes.entry(symbol).or_default();
                            notes.text = text.clone();
                            notes.written = Some(text);
                        }
                        Err(error) if current(&symbol) => {
                            self.error = Some(format!("Notes unavailable: {error}"));
                        }
                        Err(_) => {}
                    }
                }
                Request::Save(symbol, text) => {
                    let result = *answer
                        .downcast::<Result<(), FeedError>>()
                        .expect("notes save");
                    if let Err(error) = result {
                        self.error = Some(format!("Notes for {symbol} not saved: {error}"));
                    }
                    // After a failure too, so the next edit writes again rather than every frame.
                    if let Some(notes) = self.notes.get_mut(&symbol) {
                        notes.saving = false;
                        notes.written = Some(text);
                    }
                }
            }
        }
        self.save_notes();
    }

    /// Write the notes that differ from storage and have no write in flight.
    fn save_notes(&mut self) {
        let Some(storage) = &self.storage else {
            return;
        };
        for (symbol, notes) in &mut self.notes {
            if notes.saving
                || notes
                    .written
                    .as_ref()
                    .is_none_or(|text| *text == notes.text)
            {
                continue;
            }
            let document = serde_json::to_string(&NotesDocument {
                version: 1,
                notes: notes.text.clone(),
            })
            .expect("a string field always serializes");
            let key = self.next;
            self.next += 1;
            self.requests
                .insert(key, Request::Save(symbol.clone(), notes.text.clone()));
            notes.saving = true;
            storage.save(
                StoreKind::SymbolNotes,
                symbol,
                document,
                self.inbox.reply(key),
            );
        }
    }

    /// Render the shown symbol after [`poll`](Self::poll).
    pub fn show(&mut self, ui: &mut Ui) {
        let Self {
            instrument,
            quote,
            details,
            error,
            notes,
            ..
        } = self;
        let Some(instrument) = instrument else {
            return;
        };
        let theme = ui.active_theme();
        PanelHeader::new(&instrument.symbol).show_plain(ui);
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let name: Vec<&str> = [&instrument.description, &instrument.exchange]
                    .into_iter()
                    .flatten()
                    .map(String::as_str)
                    .collect();
                if !name.is_empty() {
                    ui.add(
                        Label::new(
                            RichText::new(name.join(" · "))
                                .size(theme.text.small)
                                .color(theme.ui.text_secondary),
                        )
                        .truncate(),
                    );
                }
                if let Some(quote) = quote {
                    let decimals = quote
                        .price_decimals
                        .or(instrument.price_decimals)
                        .unwrap_or_else(|| price_decimals(quote.last));
                    let change = quote.change();
                    let color = if change > 0.0 {
                        theme.chart.candle_up
                    } else if change < 0.0 {
                        theme.chart.candle_down
                    } else {
                        theme.ui.text
                    };
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(format_price(quote.last, Some(decimals)))
                                .size(PRICE_SIZE)
                                .strong()
                                .color(theme.ui.text),
                        );
                        if let Some(currency) = &instrument.currency {
                            ui.label(
                                RichText::new(currency)
                                    .size(theme.text.small)
                                    .color(theme.ui.text_secondary),
                            );
                        }
                        for text in [
                            signed(change, usize::from(decimals)),
                            format!("{}%", signed(quote.change_percent(), 2)),
                        ] {
                            ui.label(RichText::new(text).size(theme.text.heading).color(color));
                        }
                    });
                }
                if let Some(details) = details {
                    for (title, rows) in [
                        ("Key statistics", &details.statistics),
                        ("Profile", &details.profile),
                    ] {
                        if rows.is_empty() {
                            continue;
                        }
                        ui.add_space(theme.spacing.md);
                        ui.label(RichText::new(title).strong());
                        for (label, value) in rows {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(label).color(theme.ui.text_secondary));
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.add(Label::new(value.as_str()).truncate());
                                });
                            });
                        }
                    }
                }
                ui.add_space(theme.spacing.md);
                ui.label(RichText::new("Notes").strong());
                let notes = notes.entry(instrument.symbol.clone()).or_default();
                ui.add_enabled(
                    notes.written.is_some(),
                    egui::TextEdit::multiline(&mut notes.text)
                        .hint_text("Add a note")
                        .desired_width(f32::INFINITY)
                        .desired_rows(4),
                );
                if let Some(error) = error {
                    status_message(ui, MessageKind::Error, error);
                }
            });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{HistoryPage, HistoryRequest, Reply};
    use std::sync::Mutex;

    /// Keeps detail requests for the test to answer in any order.
    #[derive(Default)]
    struct Feed(Mutex<Vec<(String, Reply<SymbolDetails>)>>);

    impl Datafeed for Feed {
        fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
            reply.send(Ok(HistoryPage::default()));
        }

        fn symbol_details(&self, symbol: &str, reply: Reply<SymbolDetails>) {
            self.0.lock().unwrap().push((symbol.into(), reply));
        }
    }

    /// Answers reads at once, failing for `BAD`, and keeps writes for the test to answer.
    #[derive(Default)]
    struct Store {
        documents: Mutex<HashMap<String, String>>,
        saves: Mutex<Vec<(String, String, Reply<()>)>>,
    }

    impl Store {
        fn answer_saves(&self) -> Vec<String> {
            let saves = std::mem::take(&mut *self.saves.lock().unwrap());
            saves
                .into_iter()
                .map(|(name, document, reply)| {
                    self.documents
                        .lock()
                        .unwrap()
                        .insert(name, document.clone());
                    reply.send(Ok(()));
                    document
                })
                .collect()
        }
    }

    impl Storage for Store {
        fn list(&self, _: StoreKind, reply: Reply<Vec<String>>) {
            reply.send(Ok(Vec::new()));
        }

        fn load(&self, kind: StoreKind, name: &str, reply: Reply<Option<String>>) {
            assert_eq!(kind, StoreKind::SymbolNotes);
            reply.send(if name == "BAD" {
                Err(FeedError::Network("offline".into()))
            } else {
                Ok(self.documents.lock().unwrap().get(name).cloned())
            });
        }

        fn save(&self, kind: StoreKind, name: &str, document: String, reply: Reply<()>) {
            assert_eq!(kind, StoreKind::SymbolNotes);
            self.saves
                .lock()
                .unwrap()
                .push((name.into(), document, reply));
        }

        fn delete(&self, _: StoreKind, _: &str, reply: Reply<()>) {
            reply.send(Ok(()));
        }
    }

    fn texts(ctx: &egui::Context, panel: &mut SymbolDetailsPanel) -> Vec<String> {
        let output = ctx.run_ui(egui::RawInput::default(), |ui| panel.show(ui));
        let texts = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        texts
    }

    #[test]
    fn details_follow_the_symbol_and_notes_are_written_one_at_a_time() {
        let ctx = egui::Context::default();
        let feed = Arc::new(Feed::default());
        let store = Arc::new(Store::default());
        let mut panel = SymbolDetailsPanel::new();
        panel.set_feed(feed.clone());
        panel.set_storage(store.clone());
        panel.set_instrument(&Instrument::new("AAPL"));
        panel.set_instrument(&Instrument::new("MSFT"));
        // The answer for the previous symbol arrives last and must not replace the current one.
        let requests = std::mem::take(&mut *feed.0.lock().unwrap());
        for (symbol, reply) in requests.into_iter().rev() {
            let statistics = vec![("Market cap".into(), format!("{symbol} cap"))];
            reply.send(Ok(SymbolDetails::new(statistics, Vec::new())));
        }
        panel.poll(&ctx);
        let shown = texts(&ctx, &mut panel);
        assert!(shown.contains(&"MSFT cap".into()), "{shown:?}");
        assert!(!shown.contains(&"AAPL cap".into()), "{shown:?}");
        assert!(!shown.contains(&"Profile".into()), "{shown:?}");

        // A second edit waits for the first write of the same document, then writes the latest text.
        let edit = |panel: &mut SymbolDetailsPanel, text: &str| {
            panel.notes.get_mut("MSFT").unwrap().text = text.into();
            panel.poll(&ctx);
        };
        edit(&mut panel, "first");
        edit(&mut panel, "second");
        assert_eq!(store.answer_saves().len(), 1);
        panel.poll(&ctx);
        assert_eq!(store.answer_saves(), [r#"{"version":1,"notes":"second"}"#]);
        panel.poll(&ctx);
        assert!(store.saves.lock().unwrap().is_empty());

        let mut restored = SymbolDetailsPanel::new();
        restored.set_storage(store.clone());
        restored.set_instrument(&Instrument::new("MSFT"));
        restored.poll(&ctx);
        assert_eq!(restored.notes["MSFT"].text, "second");

        // Notes that could not be read are never written over.
        restored.set_instrument(&Instrument::new("BAD"));
        restored.poll(&ctx);
        restored.notes.get_mut("BAD").unwrap().text = "lost".into();
        restored.poll(&ctx);
        assert!(store.saves.lock().unwrap().is_empty());
        assert!(restored.error.is_some());
        // They are read again when the symbol comes back.
        restored.set_instrument(&Instrument::new("MSFT"));
        restored.set_instrument(&Instrument::new("BAD"));
        restored.poll(&ctx);
        assert!(restored.error.is_some());
    }
}
