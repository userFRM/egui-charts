//! Facts about the chart's symbol: its quote, the key statistics and profile the feed supplies,
//! and the user's notes, kept through the storage service.

use crate::localization as l10n;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Offset, Utc};
use chrono_tz::Tz;
use egui::text::LayoutJob;
use egui::{Align, Color32, Label, Layout, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde::{Deserialize, Serialize};

use super::object_tree::{format_price, price_decimals};
use super::watchlist::{HEADER_HEIGHT, signed, weighted};
use crate::data::inbox::Inbox;
use crate::data::{Datafeed, FeedError, SymbolDetails};
use crate::icons::icons;
use crate::model::{ExtendedSession, Instrument, Quote};
use crate::services::{Storage, StoreKind};
use crate::studies::StudyFormat;
use crate::theme::ThemeExt;
use crate::ui_kit::IconButton;
use crate::ui_kit::status_message::{MessageKind, status_message};

/// Text size of the last price, the panel's largest figure.
const PRICE_SIZE: f32 = 24.0;
/// Text size of the extended-hours price.
const EXTENDED_PRICE_SIZE: f32 = 18.0;
/// The sections the layout menu shows or hides, in panel order.
const SECTIONS: [&str; 4] = ["Volume", "Key statistics", "Profile", "Notes"];

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
/// Keep it on the chart's symbol with [`set_instrument`](Self::set_instrument), its latest quote
/// with [`set_quote`](Self::set_quote), such as the one
/// [`WatchlistPanel::quote`](super::WatchlistPanel::quote) holds, and its average volume with
/// [`set_average_volume`](Self::set_average_volume). The quote's last price, change, update time,
/// extended-hours trade and session volume are shown, the extended-hours change measured from the
/// regular last price; update times are shown in the time zone set with
/// [`set_timezone`](Self::set_timezone), UTC by default. The header's layout button shows or
/// hides sections, the edit button moves to the notes, and a feed that supplies
/// [`SymbolDetails::url`] gets a link to the instrument beside its name. For each new symbol the
/// panel asks the feed for [`Datafeed::symbol_details`] and shows the statistics and profile it
/// answers; an empty section, or a feed that does not support the request, shows nothing. Notes
/// are kept in memory per symbol, and with a storage service attached they are read from and
/// written to one [`StoreKind::SymbolNotes`] document per symbol as they are edited, one write per
/// document at a time. Call [`poll`](Self::poll) each frame, even while the panel is hidden, then
/// [`show`](Self::show) while it is open.
#[derive(Default)]
pub struct SymbolDetailsPanel {
    instrument: Option<Instrument>,
    quote: Option<Quote>,
    /// Average daily volume over the last 30 days.
    average_volume: Option<f64>,
    timezone: Tz,
    hidden: HashSet<&'static str>,
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

    /// Show `instrument`, such as the chart's. A new symbol clears the quote and the average
    /// volume and asks for its details and notes; the same symbol only takes the new name,
    /// exchange and currency.
    pub fn set_instrument(&mut self, instrument: &Instrument) {
        let new = self.symbol() != Some(instrument.symbol.as_str());
        self.instrument = Some(instrument.clone());
        if new {
            self.quote = None;
            self.average_volume = None;
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

    /// Sets the latest quote of the shown symbol. Its extended-hours trade shows below the
    /// regular price while it is set.
    pub fn set_quote(&mut self, quote: Quote) {
        self.quote = Some(quote);
    }

    /// Sets the average daily volume of the shown symbol over the last 30 days. With the quote's
    /// session volume, both are drawn as a bar split in proportion to the two.
    pub fn set_average_volume(&mut self, average: f64) {
        self.average_volume = Some(average);
    }

    /// Shows update times in `timezone`, such as the chart's.
    pub fn set_timezone(&mut self, timezone: Tz) {
        self.timezone = timezone;
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
            average_volume,
            timezone,
            hidden,
            details,
            error,
            notes,
            ..
        } = self;
        let Some(instrument) = instrument else {
            return;
        };
        let theme = ui.active_theme();
        let mut edit_notes = false;
        let header = vec2(ui.available_width(), HEADER_HEIGHT);
        ui.allocate_ui_with_layout(header, Layout::right_to_left(Align::Center), |ui| {
            ui.set_min_height(header.y);
            ui.spacing_mut().item_spacing.x = theme.spacing.sm;
            let more = IconButton::new(&icons::UI_MORE_HORIZONTAL)
                .tooltip("More")
                .show(ui);
            egui::Popup::menu(&more).show(|ui| {
                if ui.button(l10n::text(ui.ctx(), "Copy symbol")).clicked() {
                    ui.ctx().copy_text(instrument.symbol.clone());
                }
            });
            edit_notes = IconButton::new(&icons::PENCIL)
                .tooltip("Edit notes")
                .show(ui)
                .clicked();
            let layout = IconButton::new(&icons::LAYOUT_GRID)
                .tooltip("Layout")
                .show(ui);
            egui::Popup::menu(&layout)
                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                .show(|ui| {
                    for section in SECTIONS {
                        let mut shown = !hidden.contains(section);
                        if ui
                            .checkbox(&mut shown, l10n::text(ui.ctx(), section))
                            .changed()
                        {
                            if shown {
                                hidden.remove(section);
                            } else {
                                hidden.insert(section);
                            }
                        }
                    }
                });
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                let title = LayoutJob::single_section(
                    l10n::text(ui.ctx(), instrument.symbol.clone()),
                    weighted(theme.text.heading, 700.0, theme.ui.text),
                );
                ui.add(Label::new(title).truncate());
            });
        });
        if edit_notes {
            hidden.remove("Notes");
        }
        let shown = |section| !hidden.contains(section);
        let update = |time: DateTime<Utc>| {
            let local = time.with_timezone(&*timezone);
            format!(
                "Last update at {} {}",
                local.format("%H:%M"),
                zone_label(local.offset().fix().local_minus_utc())
            )
        };
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let name: Vec<&str> = [&instrument.description, &instrument.exchange]
                    .into_iter()
                    .flatten()
                    .map(String::as_str)
                    .collect();
                let url = details.as_ref().and_then(|details| details.url.as_deref());
                if !name.is_empty() || url.is_some() {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = theme.spacing.xs;
                        ui.scope(|ui| {
                            ui.set_max_width(ui.available_width() - 16.0);
                            ui.add(
                                Label::new(
                                    RichText::new(l10n::text(ui.ctx(), name.join(" · ")))
                                        .size(theme.text.label)
                                        .color(theme.ui.text),
                                )
                                .truncate(),
                            );
                        });
                        if let Some(url) = url {
                            external_link(ui, url);
                        }
                    });
                }
                if let Some(quote) = quote {
                    let decimals =
                        (instrument.price_decimals).unwrap_or_else(|| price_decimals(quote.last));
                    ui.add_space(theme.spacing.md);
                    price_line(
                        ui,
                        quote.last,
                        quote.change(),
                        quote.change_percent(),
                        decimals,
                        PRICE_SIZE,
                        instrument.currency.as_deref(),
                    );
                    ui.label(
                        RichText::new(l10n::text(ui.ctx(), update(quote.time)))
                            .size(theme.text.small)
                            .color(theme.ui.text_secondary),
                    );
                    if let Some(extended) = quote.extended {
                        let from_regular = Quote::new(extended.last, quote.last, extended.time);
                        ui.add_space(theme.spacing.md);
                        price_line(
                            ui,
                            from_regular.last,
                            from_regular.change(),
                            from_regular.change_percent(),
                            decimals,
                            EXTENDED_PRICE_SIZE,
                            instrument.currency.as_deref(),
                        );
                        let (session, color) = match extended.session {
                            ExtendedSession::PreMarket => ("Pre-market", theme.ui.warning),
                            ExtendedSession::PostMarket => ("Post-market", theme.ui.info),
                        };
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(l10n::text(ui.ctx(), session))
                                    .size(theme.text.small)
                                    .color(color),
                            );
                            ui.label(
                                RichText::new(l10n::text(ui.ctx(), update(extended.time)))
                                    .size(theme.text.small)
                                    .color(theme.ui.text_secondary),
                            );
                        });
                    }
                }
                if let Some((volume, average)) =
                    quote.as_ref().and_then(|q| q.volume).zip(*average_volume)
                    && shown("Volume")
                {
                    ui.add_space(theme.spacing.md);
                    let decimals = instrument.volume_decimals.map(usize::from);
                    for (left, right, size, color) in [
                        (
                            "Volume".to_owned(),
                            "Avg. volume (30D)".to_owned(),
                            theme.text.small,
                            theme.ui.text_secondary,
                        ),
                        (
                            StudyFormat::Volume.format(volume, decimals),
                            StudyFormat::Volume.format(average, decimals),
                            theme.text.label,
                            theme.ui.text,
                        ),
                    ] {
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(l10n::text(ui.ctx(), left))
                                    .size(size)
                                    .color(color),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(
                                    RichText::new(l10n::text(ui.ctx(), right))
                                        .size(size)
                                        .color(color),
                                );
                            });
                        });
                    }
                    volume_bar(ui, volume, average);
                }
                if let Some(details) = details {
                    for (title, rows) in [
                        ("Key statistics", &details.statistics),
                        ("Profile", &details.profile),
                    ] {
                        if rows.is_empty() || !shown(title) {
                            continue;
                        }
                        ui.add_space(theme.spacing.md);
                        ui.label(RichText::new(title).strong());
                        for (label, value) in rows {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new(l10n::text(ui.ctx(), label))
                                        .color(theme.ui.text_secondary),
                                );
                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                    ui.add(
                                        Label::new(l10n::text(ui.ctx(), value.as_str())).truncate(),
                                    );
                                });
                            });
                        }
                    }
                }
                if shown("Notes") {
                    ui.add_space(theme.spacing.md);
                    ui.label(RichText::new(l10n::text(ui.ctx(), "Notes")).strong());
                    let notes = notes.entry(instrument.symbol.clone()).or_default();
                    let field = ui.add_enabled(
                        notes.written.is_some(),
                        egui::TextEdit::multiline(&mut notes.text)
                            .hint_text(l10n::text(ui.ctx(), "Add a note"))
                            .desired_width(f32::INFINITY)
                            .desired_rows(4),
                    );
                    if edit_notes {
                        field.request_focus();
                        field.scroll_to_me(None);
                    }
                }
                if let Some(error) = error {
                    status_message(ui, MessageKind::Error, error);
                }
            });
    }
}

/// A price set large and bold with its currency, then its signed change and change percent in
/// the rising or falling colour.
fn price_line(
    ui: &mut Ui,
    price: f64,
    change: f64,
    percent: f64,
    decimals: u8,
    size: f32,
    currency: Option<&str>,
) {
    let theme = ui.active_theme();
    let color = if change > 0.0 {
        theme.chart.candle_up
    } else if change < 0.0 {
        theme.chart.candle_down
    } else {
        theme.ui.text
    };
    // One text, so the figures of different sizes share a baseline.
    let mut job = LayoutJob::single_section(
        l10n::text(ui.ctx(), format_price(price, Some(decimals))),
        weighted(size, 700.0, theme.ui.text),
    );
    if let Some(currency) = currency {
        job.append(
            currency,
            theme.spacing.sm,
            weighted(theme.text.small, 400.0, theme.ui.text),
        );
    }
    let change_size = (size * 2.0 / 3.0).max(theme.text.body);
    for text in [
        signed(change, usize::from(decimals)),
        format!("{}%", signed(percent, 2)),
    ] {
        job.append(&text, theme.spacing.sm, weighted(change_size, 600.0, color));
    }
    ui.add(Label::new(job).wrap());
}

/// "UTC" for no offset, otherwise "GMT" and the signed offset in hours, with minutes when they
/// are not zero, such as "GMT+2" or "GMT+5:30".
fn zone_label(offset_seconds: i32) -> String {
    if offset_seconds == 0 {
        return "UTC".to_owned();
    }
    let sign = if offset_seconds < 0 { '-' } else { '+' };
    let minutes = offset_seconds.unsigned_abs() / 60;
    match minutes % 60 {
        0 => format!("GMT{sign}{}", minutes / 60),
        rest => format!("GMT{sign}{}:{rest:02}", minutes / 60),
    }
}

/// A small external-link glyph opening `url` in a new tab.
fn external_link(ui: &mut Ui, url: &str) {
    let theme = ui.active_theme();
    let (rect, response) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::click());
    let response = response
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .on_hover_text(l10n::text(ui.ctx(), url));
    let translated_label = l10n::text(ui.ctx(), "Open instrument page");
    response.widget_info(|| {
        egui::WidgetInfo::labeled(egui::Role::Link, ui.is_enabled(), &translated_label)
    });
    let color = if response.hovered() {
        theme.ui.accent
    } else {
        theme.ui.text_secondary
    };
    let stroke = Stroke::new(1.0, color);
    let r = rect.shrink(1.5);
    let painter = ui.painter();
    // A box open at its top right, with an arrow leaving through the gap.
    painter.line(
        vec![
            pos2(r.center().x, r.top()),
            r.left_top(),
            r.left_bottom(),
            r.right_bottom(),
            pos2(r.right(), r.center().y),
        ],
        stroke,
    );
    painter.line_segment([r.center(), r.right_top()], stroke);
    painter.line(
        vec![
            pos2(r.right() - 3.5, r.top()),
            r.right_top(),
            pos2(r.right(), r.top() + 3.5),
        ],
        stroke,
    );
    if response.clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(url));
    }
}

/// A bar split at `volume / (volume + average)`: the session volume in the accent colour, the
/// average volume in a lighter shade.
fn volume_bar(ui: &mut Ui, volume: f64, average: f64) {
    let theme = ui.active_theme();
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 4.0), Sense::hover());
    let total = volume.max(0.0) + average.max(0.0);
    if !total.is_finite() || total <= 0.0 {
        return;
    }
    let gap = 2.0;
    let split = rect.left() + (rect.width() - gap) * (volume.max(0.0) / total) as f32;
    let painter = ui.painter();
    let radius = rect.height() / 2.0;
    let part = |left: f32, right: f32, color: Color32| {
        if right > left {
            painter.rect_filled(rect.with_min_x(left).with_max_x(right), radius, color);
        }
    };
    part(rect.left(), split, theme.ui.accent);
    part(
        split + gap,
        rect.right(),
        theme.ui.accent.gamma_multiply(0.35),
    );
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

    /// The extended-hours change is measured from the regular last price, not the previous close,
    /// and update times carry the offset of the chart's time zone on that date.
    #[test]
    fn extended_hours_change_from_the_regular_last_price() {
        let ctx = egui::Context::default();
        let mut panel = SymbolDetailsPanel::new();
        panel.set_instrument(&Instrument::new("QQQ").with_price_decimals(2));
        panel.set_timezone(chrono_tz::Europe::Berlin);
        let quote = Quote::new(704.47, 725.20, "2026-10-22T19:59:00Z".parse().unwrap());
        panel.set_quote(quote.with_extended(crate::model::ExtendedHoursQuote {
            session: ExtendedSession::PreMarket,
            last: 703.65,
            // Summer time ends in Berlin on October 25.
            time: "2026-10-26T12:30:00Z".parse().unwrap(),
        }));
        let shown = texts(&ctx, &mut panel);
        for text in [
            "704.47-20.73-2.86%",
            "Last update at 21:59 GMT+2",
            "703.65-0.82-0.12%",
            "Last update at 13:30 GMT+1",
        ] {
            assert!(shown.contains(&text.into()), "{text}: {shown:?}");
        }
        for (offset, label) in [(0, "UTC"), (-5 * 3600, "GMT-5"), (-12600, "GMT-3:30")] {
            assert_eq!(zone_label(offset), label);
        }
    }
}
