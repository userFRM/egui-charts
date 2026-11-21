//! Facts about the chart's symbol: its quote, the key statistics and profile the feed supplies,
//! and the user's notes, kept through the storage service.

use crate::localization as l10n;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Offset, TimeDelta, Utc};
use egui::text::LayoutJob;
use egui::{Align, Color32, Label, Layout, RichText, Sense, Stroke, Ui, pos2, vec2};
use serde::{Deserialize, Serialize};

use super::market_details::{MarketDetail, MarketDetailRequest, PeriodReturn, Seasonality};
use super::news::NewsAction;
use super::price_decimals;
use super::{HEADER_HEIGHT, weighted};
use crate::data::inbox::Inbox;
use crate::data::{
    CalendarEvent, CalendarKind, CalendarQuery, CompanyFinancials, Datafeed, FeedError, NewsQuery,
    NewsStory, SymbolDetails,
};
use crate::icons::icons;
use crate::model::{Bar, ExtendedSession, Instrument, Quote, Timeframe, TimezoneId};
use crate::scales::StudyFormat;
use crate::services::revisions::{Revisions, Settled, Written};
use crate::services::{ConflictChoice, Storage, StoreKind};
use crate::studies::{Indicator, TechnicalRatings, technical_rating};
use crate::theme::ThemeExt;
use crate::ui::trading::grouped;
use crate::ui_kit::IconButton;
use crate::ui_kit::status_message::{MessageKind, status_message};

/// Text size of the last price, the panel's largest figure: digits about 19 points high.
const PRICE_SIZE: f32 = 26.0;
/// Text size of the extended-hours price.
const EXTENDED_PRICE_SIZE: f32 = 18.0;
/// Headlines the News section shows.
const HEADLINES: usize = 3;
/// Days ahead the Upcoming events section looks for the next earnings and dividend.
const EVENT_DAYS: i64 = 366;
/// The sections the layout menu shows or hides, in panel order.
const SECTIONS: [&str; 9] = [
    "Volume",
    "Key statistics",
    "Profile",
    "Performance",
    "Seasonality",
    "Technicals",
    "Upcoming events",
    "News",
    "Notes",
];
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

#[derive(Serialize, Deserialize)]
struct NotesDocument {
    version: u32,
    notes: String,
}

/// The stored form of the notes `text`.
fn document(text: &str) -> String {
    serde_json::to_string(&NotesDocument {
        version: 1,
        notes: text.to_owned(),
    })
    .expect("a string field always serializes")
}

/// The notes a stored document holds, empty without one, or why they cannot be read.
fn read_notes(document: Option<String>) -> Result<String, String> {
    let Some(document) = document else {
        return Ok(String::new());
    };
    let document: NotesDocument =
        serde_json::from_str(&document).map_err(|error| error.to_string())?;
    if document.version != 1 {
        return Err("unsupported notes version".into());
    }
    Ok(document.notes)
}

/// The notes of one symbol.
#[derive(Debug, Default)]
struct Notes {
    text: String,
    /// The text last read from or sent to storage, `None` until it is read. Writes wait for a
    /// successful read, so a failed read never lets an empty field replace the stored notes.
    written: Option<String>,
    saving: bool,
    /// Whether the last write failed, so that a run of failures is reported once.
    failing: bool,
    /// The input time from which a failed write is tried again.
    retry_at: Option<f64>,
}

/// The Technical Ratings study on the shown symbol's bars, and what it last rated.
struct Technicals {
    study: TechnicalRatings,
    timeframe: Timeframe,
    /// How many bars were rated, the first and the last bar.
    len: usize,
    first: Bar,
    last: Bar,
    /// The moving-average, oscillator and summary scores of the latest bar with values.
    scores: [Option<f64>; 3],
}

/// What an inbox answer is for.
#[derive(Debug)]
enum Request {
    Details(String),
    Financials(String),
    /// The calendar of one kind for a symbol.
    Events(String, CalendarKind),
    News(String),
    Load(String),
    Save(String, String),
}

/// Details of the chart's symbol, fed by the host's [`Datafeed`] and [`Storage`].
///
/// Keep it on the chart's symbol with [`set_instrument`](Self::set_instrument), its latest quote
/// with [`set_quote`](Self::set_quote), such as the one
/// [`WatchlistPanel::quote`](crate::ui::widget_bar::WatchlistPanel::quote) holds, and its average volume with
/// [`set_average_volume`](Self::set_average_volume). The quote's last price, change, update time,
/// extended-hours trade and session volume are shown, the extended-hours change measured from the
/// regular last price; update times are shown in the time zone set with
/// [`set_timezone`](Self::set_timezone), UTC by default. The header's layout button shows or
/// hides sections, the edit button moves to the notes, and a feed that supplies
/// [`SymbolDetails::url`] gets a link to the instrument beside its name, and one that supplies
/// [`SymbolDetails::category`] gets the category under it. For each new symbol the panel asks the
/// feed for [`Datafeed::symbol_details`] and shows the statistics, on a card, and the profile it
/// answers; an empty section, or a feed that does not support the request, shows nothing. It also
/// asks for [`Datafeed::financials`] and holds a nonempty answer in
/// [`financials`](Self::financials). It also asks [`Datafeed::calendar`] for the symbol's earnings
/// and dividends of the next year, with the symbol in
/// [`CalendarQuery::symbols`], and shows the next of each
/// under Upcoming events, keeping only events without a symbol or of the shown one, and asks
/// [`Datafeed::news`] for its three newest headlines, shown under News; opening one is reported by
/// [`take_news_action`](Self::take_news_action). A feed that does not support a request, or
/// answers nothing, shows nothing for it. Pass the chart's bars each frame to
/// [`set_bars`](Self::set_bars) for the Technicals section. Notes
/// are kept in memory per symbol, and with a storage service attached they are read from and
/// written to one [`StoreKind::SymbolNotes`] document per symbol as they are edited, one write per
/// document at a time, a failed write being tried again a second later. With a host that keeps
/// revisions they are written over the version read; see
/// [`storage_conflict`](Self::storage_conflict). Call [`poll`](Self::poll) each frame, even while
/// the panel is hidden, then [`show`](Self::show) while it is open. The More menu's Financials
/// item is reported by [`take_financials_request`](Self::take_financials_request), for the host
/// to open a [`FinancialsPanel`](crate::ui::financials::FinancialsPanel) with the company's
/// figures, such as [`financials`](Self::financials).
///
/// The Performance and Seasonality sections show what the host supplies. Call
/// [`take_market_requests`](Self::take_market_requests) each frame: the first call for a symbol
/// returns its [`MarketDetailRequest`]s and shows both sections as loading, and the host answers
/// with [`set_period_returns`](Self::set_period_returns) and
/// [`set_seasonality`](Self::set_seasonality). A host that never calls it sees neither section.
/// Each section says whether it is loading, empty, unavailable or failed.
pub struct SymbolDetailsPanel {
    instrument: Option<Instrument>,
    quote: Option<Quote>,
    /// Average daily volume over the last 30 days.
    average_volume: Option<f64>,
    timezone: TimezoneId,
    hidden: HashSet<&'static str>,
    details: Option<SymbolDetails>,
    financials: Option<CompanyFinancials>,
    returns: Option<MarketDetail<Vec<PeriodReturn>>>,
    seasonality: Option<MarketDetail<Seasonality>>,
    /// `None` until bars of the shown symbol arrive.
    technicals: Option<Technicals>,
    /// The next earnings and the next ex-dividend event of the shown symbol.
    events: [Option<CalendarEvent>; 2],
    /// The shown symbol's newest headlines, newest first.
    headlines: Vec<NewsStory>,
    news_action: NewsAction,
    error: Option<String>,
    notes: HashMap<String, Notes>,
    feed: Option<Arc<dyn Datafeed>>,
    storage: Option<Arc<dyn Storage>>,
    /// The revisions the notes were read or written at, and the writes the host refused.
    revisions: Revisions,
    inbox: Arc<Inbox>,
    requests: HashMap<u64, Request>,
    next: u64,
    financials_requested: bool,
    hide_financials: bool,
}

impl Default for SymbolDetailsPanel {
    fn default() -> Self {
        Self {
            instrument: None,
            quote: None,
            average_volume: None,
            timezone: TimezoneId::default(),
            hidden: HashSet::new(),
            details: None,
            financials: None,
            returns: None,
            seasonality: None,
            technicals: None,
            events: [None, None],
            headlines: Vec::new(),
            news_action: NewsAction::None,
            error: None,
            notes: HashMap::new(),
            feed: None,
            storage: None,
            revisions: Revisions::new(StoreKind::SymbolNotes),
            inbox: Arc::default(),
            requests: HashMap::new(),
            next: 0,
            financials_requested: false,
            hide_financials: false,
        }
    }
}

impl std::fmt::Debug for SymbolDetailsPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SymbolDetailsPanel")
            .field("instrument", &self.instrument)
            .field("quote", &self.quote)
            .field("average_volume", &self.average_volume)
            .field("timezone", &self.timezone)
            .field("hidden", &self.hidden)
            .field("details", &self.details)
            .field("financials", &self.financials)
            .field("returns", &self.returns)
            .field("seasonality", &self.seasonality)
            .field(
                "technicals",
                &self.technicals.as_ref().map(|t| (t.timeframe, t.len)),
            )
            .field("events", &self.events)
            .field("headlines", &self.headlines)
            .field("news_action", &self.news_action)
            .field("error", &self.error)
            .field("next", &self.next)
            .field("financials_requested", &self.financials_requested)
            .field("hide_financials", &self.hide_financials)
            .finish_non_exhaustive()
    }
}

impl SymbolDetailsPanel {
    /// A panel without a symbol, feed or storage.
    pub fn new() -> Self {
        Self::default()
    }

    /// Ask `feed` for the details, financials, upcoming events and headlines of each symbol from
    /// now on, starting with the shown one. Replies from the previous feed are ignored.
    pub fn set_feed(&mut self, feed: Arc<dyn Datafeed>) {
        self.requests
            .retain(|_, request| matches!(request, Request::Load(_) | Request::Save(..)));
        self.feed = Some(feed);
        if let Some(symbol) = self.symbol().map(str::to_owned) {
            self.request_details(symbol);
        }
    }

    /// Keep notes in `storage`, reading them again from it. Edits not yet written are dropped.
    pub fn set_storage(&mut self, storage: Arc<dyn Storage>) {
        self.storage = Some(storage);
        self.revisions = Revisions::new(StoreKind::SymbolNotes);
        self.notes.clear();
        self.requests
            .retain(|_, request| !matches!(request, Request::Load(_) | Request::Save(..)));
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
            self.financials = None;
            self.returns = None;
            self.seasonality = None;
            self.technicals = None;
            self.events = [None, None];
            self.headlines.clear();
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
    pub fn set_timezone(&mut self, timezone: TimezoneId) {
        self.timezone = timezone;
    }

    /// Show or hide Financials in the More menu, shown by default. Hide it when the host cannot
    /// supply company financials.
    pub fn set_financials_visible(&mut self, visible: bool) {
        self.hide_financials = !visible;
        if !visible {
            self.financials_requested = false;
        }
    }

    /// The shown symbol's [`Datafeed::financials`] once the feed answers with at least one page.
    pub fn financials(&self) -> Option<&CompanyFinancials> {
        self.financials.as_ref()
    }

    /// Whether the user chose Financials in the More menu since the last call.
    pub fn take_financials_request(&mut self) -> bool {
        std::mem::take(&mut self.financials_requested)
    }

    /// The market detail the sections still lack for the shown symbol, each returned once; it
    /// marks them loading. Call it every frame and answer with
    /// [`set_period_returns`](Self::set_period_returns) and
    /// [`set_seasonality`](Self::set_seasonality).
    pub fn take_market_requests(&mut self) -> Vec<MarketDetailRequest> {
        let Some(symbol) = self.symbol().map(str::to_owned) else {
            return Vec::new();
        };
        let mut requests = Vec::new();
        if self.returns.is_none() {
            self.returns = Some(MarketDetail::Loading);
            requests.push(MarketDetailRequest::PeriodReturns(symbol.clone()));
        }
        if self.seasonality.is_none() {
            self.seasonality = Some(MarketDetail::Loading);
            requests.push(MarketDetailRequest::Seasonality(symbol));
        }
        requests
    }

    /// Answers the [`MarketDetailRequest::PeriodReturns`] of `symbol`. An answer for a symbol the
    /// panel no longer shows is ignored.
    pub fn set_period_returns(&mut self, symbol: &str, returns: MarketDetail<Vec<PeriodReturn>>) {
        if self.symbol() == Some(symbol) {
            self.returns = Some(returns);
        }
    }

    /// Answers the [`MarketDetailRequest::Seasonality`] of `symbol`. An answer for a symbol the
    /// panel no longer shows is ignored.
    pub fn set_seasonality(&mut self, symbol: &str, seasonality: MarketDetail<Seasonality>) {
        if self.symbol() == Some(symbol) {
            self.seasonality = Some(seasonality);
        }
    }

    /// Rates `bars`, the shown symbol's bars on `timeframe`, oldest first, with the
    /// [`TechnicalRatings`] study for the Technicals section, which shows the summary,
    /// moving-average and oscillator ratings of the latest bar with a finite high, low and close,
    /// as the study's last values on the same bars. Call it each frame with the chart's bars,
    /// such as `Chart::bars(SeriesId::MAIN)` and `Chart::timeframe`: an unchanged slice costs
    /// nothing, a changed or added last bar rates from the last rated bar on, and another
    /// interval, another first bar or fewer bars rate them all again. Bars changed in place
    /// between the first and last rated ones, keeping their times, are rated again after a call
    /// without bars. Without bars, and for a new symbol until its bars arrive, the section is hidden.
    pub fn set_bars(&mut self, timeframe: Timeframe, bars: &[Bar]) {
        let (Some(first), Some(last)) = (bars.first(), bars.last()) else {
            self.technicals = None;
            return;
        };
        match &mut self.technicals {
            Some(rated)
                if rated.timeframe == timeframe
                    && same_bar(first, &rated.first)
                    && bars.len() >= rated.len
                    && bars[rated.len - 1].time == rated.last.time =>
            {
                if bars.len() == rated.len && same_bar(last, &rated.last) {
                    return;
                }
                rated.study.update(bars, rated.len - 1);
            }
            _ => {
                let mut study = TechnicalRatings::new();
                study.set_timeframe(timeframe);
                study.calculate(bars);
                self.technicals = Some(Technicals {
                    study,
                    timeframe,
                    len: 0,
                    first: first.clone(),
                    last: last.clone(),
                    scores: [None; 3],
                });
            }
        }
        let rated = self.technicals.as_mut().expect("rated above");
        rated.len = bars.len();
        rated.last = last.clone();
        let values = rated.study.values();
        rated.scores = (0..values.len())
            .rev()
            .find(|&bar| values.value(5, bar).is_some())
            .map_or([None; 3], |bar| {
                std::array::from_fn(|line| values.value(line, bar))
            });
    }

    /// What the user chose in the News section since the last call, [`NewsAction::None`]
    /// without a choice: a headline with a link gives [`NewsAction::OpenUrl`], one without
    /// [`NewsAction::ShowOnChart`] at its publication time.
    pub fn take_news_action(&mut self) -> NewsAction {
        std::mem::take(&mut self.news_action)
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
            let key = self.key(Request::Financials(symbol.clone()));
            feed.financials(&symbol, self.inbox.reply(key));
            let now = Utc::now();
            for kind in [CalendarKind::Earnings, CalendarKind::Dividends] {
                let mut query = CalendarQuery::new(kind, now, now + TimeDelta::days(EVENT_DAYS));
                query.symbols.push(symbol.clone());
                let key = self.key(Request::Events(symbol.clone(), kind));
                feed.calendar(query, self.inbox.reply(key));
            }
            let mut query = NewsQuery::new(HEADLINES);
            query.symbols.push(symbol.clone());
            let key = self.key(Request::News(symbol));
            feed.news(query, self.inbox.reply(key));
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
        self.revisions.load(&*storage, &symbol, &self.inbox, key);
    }

    /// Receive the feed's and storage's answers, write edited notes, and wake the UI when the
    /// next answer arrives.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        let now = ctx.input(|input| input.time);
        let shown = self.symbol().map(str::to_owned);
        let current = |symbol: &str| shown.as_deref() == Some(symbol);
        let storage = self.storage.clone();
        if let Some(storage) = &storage {
            for (symbol, settled) in self.revisions.poll(&**storage, ctx) {
                let Some(notes) = self.notes.get_mut(&symbol) else {
                    continue;
                };
                match settled {
                    // The notes, read only meanwhile, are written again below.
                    Settled::Overwrite(_) => notes.saving = false,
                    Settled::Copied(_) => {}
                    Settled::Reload(stored) => {
                        notes.saving = false;
                        match read_notes(stored) {
                            Ok(text) => {
                                notes.text.clone_from(&text);
                                notes.written = Some(text);
                            }
                            Err(error) => {
                                notes.written = None;
                                if current(&symbol) {
                                    self.error = Some(l10n::format_text(
                                        ctx,
                                        "Notes unavailable: {error}",
                                        &[("error", error.as_str())],
                                    ));
                                }
                            }
                        }
                    }
                    Settled::CopyFailed(reason) => {
                        self.error = Some(l10n::format_text(
                            ctx,
                            "Could not keep a copy of the notes for {symbol}: {reason}",
                            &[("symbol", symbol.as_str()), ("reason", reason.as_str())],
                        ));
                    }
                }
            }
        }
        loop {
            let answers = self.inbox.drain().answers;
            if answers.is_empty() {
                break;
            }
            for (key, answer) in answers {
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
                            Err(error) => {
                                self.error = Some(l10n::format_text(
                                    ctx,
                                    "Details unavailable: {error}",
                                    &[("error", error.to_string().as_str())],
                                ))
                            }
                        }
                    }
                    Request::Financials(symbol) => {
                        let result = *answer
                            .downcast::<Result<CompanyFinancials, FeedError>>()
                            .expect("company financials");
                        if !current(&symbol) {
                            continue;
                        }
                        match result {
                            Ok(financials) => {
                                self.financials =
                                    Some(financials).filter(|figures| !figures.pages.is_empty());
                            }
                            Err(FeedError::NotSupported) => {}
                            Err(error) => {
                                self.error = Some(l10n::format_text(
                                    ctx,
                                    "Financials unavailable: {error}",
                                    &[("error", error.to_string().as_str())],
                                ))
                            }
                        }
                    }
                    Request::Events(symbol, kind) => {
                        let result = *answer
                            .downcast::<Result<Vec<CalendarEvent>, FeedError>>()
                            .expect("calendar events");
                        if !current(&symbol) {
                            continue;
                        }
                        self.events[usize::from(kind == CalendarKind::Dividends)] = None;
                        match result {
                            // A feed that ignores the query's symbols still shows only this one.
                            Ok(events) => {
                                self.events[usize::from(kind == CalendarKind::Dividends)] = events
                                    .into_iter()
                                    .filter(|event| {
                                        event.kind == kind
                                            && event
                                                .symbol
                                                .as_deref()
                                                .is_none_or(|of| of.eq_ignore_ascii_case(&symbol))
                                    })
                                    .min_by_key(|event| event.time);
                            }
                            Err(FeedError::NotSupported) => {}
                            Err(error) => {
                                self.error = Some(l10n::format_text(
                                    ctx,
                                    "Events unavailable: {error}",
                                    &[("error", error.to_string().as_str())],
                                ))
                            }
                        }
                    }
                    Request::News(symbol) => {
                        let result = *answer
                            .downcast::<Result<Vec<NewsStory>, FeedError>>()
                            .expect("news");
                        if !current(&symbol) {
                            continue;
                        }
                        self.headlines.clear();
                        match result {
                            Ok(mut stories) => {
                                stories.sort_by_key(|story| std::cmp::Reverse(story.time));
                                stories.truncate(HEADLINES);
                                self.headlines = stories;
                            }
                            Err(FeedError::NotSupported) => {}
                            Err(error) => {
                                self.error = Some(l10n::format_text(
                                    ctx,
                                    "News unavailable: {error}",
                                    &[("error", error.to_string().as_str())],
                                ))
                            }
                        }
                    }
                    Request::Load(symbol) => {
                        let storage = storage.as_deref().expect("notes storage");
                        let Some(result) =
                            self.revisions
                                .loaded(storage, &symbol, answer, &self.inbox, key)
                        else {
                            self.requests.insert(key, Request::Load(symbol));
                            continue;
                        };
                        match result
                            .map_err(|error| error.to_string())
                            .and_then(read_notes)
                        {
                            Ok(text) => {
                                let notes = self.notes.entry(symbol).or_default();
                                notes.text = text.clone();
                                notes.written = Some(text);
                            }
                            Err(error) if current(&symbol) => {
                                self.error = Some(l10n::format_text(
                                    ctx,
                                    "Notes unavailable: {error}",
                                    &[("error", error.to_string().as_str())],
                                ));
                            }
                            Err(_) => {}
                        }
                    }
                    Request::Save(symbol, text) => {
                        let storage = storage.as_deref().expect("notes storage");
                        let Some(written) = self.revisions.saved(
                            storage,
                            &symbol,
                            &document(&text),
                            answer,
                            &self.inbox,
                            key,
                        ) else {
                            self.requests.insert(key, Request::Save(symbol, text));
                            continue;
                        };
                        let Some(notes) = self.notes.get_mut(&symbol) else {
                            continue;
                        };
                        match written {
                            Written::Saved => {
                                notes.saving = false;
                                notes.written = Some(text);
                                notes.failing = false;
                                notes.retry_at = None;
                            }
                            // The notes stay as typed, and unsaved, until the user's choice.
                            Written::Refused(stored) => {
                                notes.failing = false;
                                self.revisions.refuse(&symbol, document(&text), stored);
                            }
                            Written::Failed(error) => {
                                notes.saving = false;
                                notes.retry_at = Some(now + 1.0);
                                if !notes.failing {
                                    self.error = Some(l10n::format_text(
                                        ctx,
                                        "Notes for {symbol} not saved: {error}",
                                        &[
                                            ("symbol", symbol.as_str()),
                                            ("error", error.to_string().as_str()),
                                        ],
                                    ));
                                }
                                notes.failing = true;
                            }
                        }
                    }
                }
            }
        }
        self.save_notes(now);
        if let Some(at) = self
            .notes
            .values()
            .filter_map(|notes| notes.retry_at)
            .filter(|&at| at > now)
            .min_by(f64::total_cmp)
        {
            ctx.request_repaint_after(Duration::from_secs_f64(at - now));
        }
    }

    /// Write the notes that differ from storage, have no write in flight and are not waiting to
    /// try a failed write again.
    fn save_notes(&mut self, now: f64) {
        let Some(storage) = &self.storage else {
            return;
        };
        for (symbol, notes) in &mut self.notes {
            if notes.saving
                || notes.retry_at.is_some_and(|at| at > now)
                || notes
                    .written
                    .as_ref()
                    .is_none_or(|text| *text == notes.text)
            {
                continue;
            }
            notes.retry_at = None;
            let key = self.next;
            self.next += 1;
            self.requests
                .insert(key, Request::Save(symbol.clone(), notes.text.clone()));
            notes.saving = true;
            self.revisions
                .save(&**storage, symbol, document(&notes.text), &self.inbox, key);
        }
    }

    /// The symbol whose notes the host refused to write because another client changed them
    /// since they were read: the shown symbol first, then the others by name. `None` while no
    /// write waits, and while the choice to keep a local copy takes effect. Until it is settled
    /// the notes stay as typed and cannot be edited; show a
    /// [`StorageConflictDialog`](crate::ui::dialogs::StorageConflictDialog) and pass the choice
    /// to [`resolve_storage_conflict`](Self::resolve_storage_conflict).
    pub fn storage_conflict(&self) -> Option<&str> {
        let shown = self.symbol();
        self.revisions
            .conflicts()
            .find(|symbol| Some(*symbol) == shown)
            .or_else(|| self.revisions.conflicts().next())
    }

    /// Settles the refused write of the notes on `symbol`, taking effect at the next
    /// [`poll`](Self::poll): [`ConflictChoice::ReloadRemote`] takes the stored notes,
    /// [`ConflictChoice::KeepLocalCopy`] first saves the typed ones as `<symbol> (local copy)`,
    /// and [`ConflictChoice::Retry`] writes them over the stored ones.
    pub fn resolve_storage_conflict(&mut self, symbol: &str, choice: ConflictChoice) {
        if let Some(storage) = self.storage.clone() {
            self.revisions.resolve(&*storage, symbol, choice);
        }
    }

    /// Render the shown symbol after [`poll`](Self::poll).
    pub fn show(&mut self, ui: &mut Ui) {
        // Inset like the watchlist's symbol column, so the title and prices line up with it.
        egui::Frame::NONE
            .inner_margin(egui::Margin::symmetric(10, 0))
            .show(ui, |ui| self.contents(ui));
    }

    fn contents(&mut self, ui: &mut Ui) {
        let Self {
            instrument,
            quote,
            average_volume,
            timezone,
            hidden,
            details,
            returns,
            seasonality,
            technicals,
            events,
            headlines,
            news_action,
            error,
            notes,
            revisions,
            financials_requested,
            hide_financials,
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
                if !*hide_financials && ui.button(l10n::text(ui.ctx(), "Financials")).clicked() {
                    *financials_requested = true;
                }
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
        let ctx = ui.ctx().clone();
        let update = |time: DateTime<Utc>| {
            let local = time.with_timezone(&*timezone);
            l10n::format_text(
                &ctx,
                "Last update at {time} {zone}",
                &[
                    ("time", local.format("%H:%M").to_string().as_str()),
                    (
                        "zone",
                        zone_label(local.offset().fix().local_minus_utc()).as_str(),
                    ),
                ],
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
                if let Some(category) = details.as_ref().and_then(|d| d.category.as_deref()) {
                    ui.add(
                        Label::new(
                            RichText::new(l10n::text(ui.ctx(), category))
                                .size(theme.text.small)
                                .color(theme.ui.text_secondary),
                        )
                        .truncate(),
                    );
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
                            ExtendedSession::Overnight => ("Overnight", theme.ui.accent),
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
                    // Volume decimals stand below a thousand; K, M and B values keep two.
                    let volume_text = |value: f64| match instrument.volume_decimals {
                        Some(decimals) if value.is_finite() => crate::scales::VolumeFormatter {
                            precision: usize::from(decimals).min(12),
                        }
                        .format_with_min_unit_decimals(value, 2),
                        _ => StudyFormat::Volume.format(value, None),
                    };
                    for (left, right, size, color) in [
                        (
                            "Volume".to_owned(),
                            "Avg. volume (30D)".to_owned(),
                            theme.text.small,
                            theme.ui.text_secondary,
                        ),
                        (
                            volume_text(volume),
                            volume_text(average),
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
                        let facts = |ui: &mut Ui| {
                            ui.label(RichText::new(l10n::text(ui.ctx(), title)).strong());
                            for (label, value) in rows {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(l10n::text(ui.ctx(), label))
                                            .color(theme.ui.text_secondary),
                                    );
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        ui.add(
                                            Label::new(l10n::text(ui.ctx(), value.as_str()))
                                                .truncate(),
                                        );
                                    });
                                });
                            }
                        };
                        if title == "Key statistics" {
                            // The key facts stand apart from the profile on an accent-tinted card.
                            egui::Frame::new()
                                .fill(theme.ui.accent.gamma_multiply(0.1))
                                .corner_radius(theme.radius.md)
                                .inner_margin(theme.spacing.md)
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    facts(ui);
                                });
                        } else {
                            facts(ui);
                        }
                    }
                }
                if let Some(returns) = returns.as_ref().filter(|_| shown("Performance")) {
                    detail_section(
                        ui,
                        "Performance",
                        returns,
                        |rows| rows.is_empty(),
                        |ui, rows| {
                            for row in rows {
                                ui.horizontal(|ui| {
                                    ui.label(
                                        RichText::new(l10n::text(ui.ctx(), &row.period))
                                            .color(theme.ui.text_secondary),
                                    );
                                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                        let (text, color) = percent_text(row.percent, &theme);
                                        ui.label(
                                            RichText::new(l10n::text(ui.ctx(), text)).color(color),
                                        );
                                    });
                                });
                            }
                        },
                    );
                }
                if let Some(seasonality) = seasonality.as_ref().filter(|_| shown("Seasonality")) {
                    detail_section(
                        ui,
                        "Seasonality",
                        seasonality,
                        |seasonality| seasonality.months.is_empty(),
                        |ui, seasonality| {
                            ui.label(
                                RichText::new(l10n::format_text(
                                    ui.ctx(),
                                    "Average monthly return over {0} years",
                                    &[("0", seasonality.years.to_string())],
                                ))
                                .size(theme.text.small)
                                .color(theme.ui.text_secondary),
                            );
                            egui::Grid::new("seasonality_months")
                                .num_columns(4)
                                .spacing(vec2(theme.spacing.md, theme.spacing.sm))
                                .show(ui, |ui| {
                                    for (index, month) in seasonality
                                        .months
                                        .iter()
                                        .filter(|month| (1..=12).contains(&month.month))
                                        .enumerate()
                                    {
                                        ui.vertical(|ui| {
                                            ui.label(
                                                RichText::new(l10n::text(
                                                    ui.ctx(),
                                                    MONTHS[usize::from(month.month) - 1],
                                                ))
                                                .size(theme.text.small)
                                                .color(theme.ui.text_secondary),
                                            );
                                            let (text, color) =
                                                percent_text(month.average_percent, &theme);
                                            ui.label(
                                                RichText::new(l10n::text(ui.ctx(), text))
                                                    .color(color),
                                            );
                                            let share = month.positive_share.map_or_else(
                                                || "-".to_owned(),
                                                |share| {
                                                    l10n::format_text(
                                                        ui.ctx(),
                                                        "{0}% up",
                                                        &[("0", format!("{:.0}", share * 100.0))],
                                                    )
                                                },
                                            );
                                            ui.label(
                                                RichText::new(share)
                                                    .size(theme.text.small)
                                                    .color(theme.ui.text_secondary),
                                            );
                                        });
                                        if index % 4 == 3 {
                                            ui.end_row();
                                        }
                                    }
                                });
                        },
                    );
                }
                if let Some(technicals) = technicals.as_ref().filter(|_| shown("Technicals")) {
                    ui.add_space(theme.spacing.md);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(l10n::text(ui.ctx(), "Technicals")).strong());
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(
                                RichText::new(l10n::text(ui.ctx(), technicals.timeframe.as_str()))
                                    .size(theme.text.small)
                                    .color(theme.ui.text_secondary),
                            );
                        });
                    });
                    for (label, line) in
                        [("Summary", 2), ("Moving averages", 0), ("Oscillators", 1)]
                    {
                        let word = technicals.scores[line].map_or("N/A", technical_rating);
                        let color = match word {
                            "Strong Buy" | "Buy" => theme.chart.candle_up,
                            "Sell" | "Strong Sell" => theme.chart.candle_down,
                            "Neutral" => theme.ui.text,
                            _ => theme.ui.text_secondary,
                        };
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(l10n::text(ui.ctx(), label))
                                    .color(theme.ui.text_secondary),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                let text = RichText::new(l10n::text(ui.ctx(), word)).color(color);
                                ui.label(if word.starts_with("Strong") {
                                    text.strong()
                                } else {
                                    text
                                });
                            });
                        });
                    }
                }
                if events.iter().any(Option::is_some) && shown("Upcoming events") {
                    ui.add_space(theme.spacing.md);
                    ui.label(RichText::new(l10n::text(ui.ctx(), "Upcoming events")).strong());
                    for (label, event) in [("Earnings", &events[0]), ("Ex-dividend", &events[1])] {
                        let Some(event) = event else {
                            continue;
                        };
                        // The date only: a feed's earnings times are indicative.
                        let date = event.time.with_timezone(&*timezone).format("%b %-d, %Y");
                        ui.horizontal(|ui| {
                            ui.label(
                                RichText::new(l10n::text(ui.ctx(), label))
                                    .color(theme.ui.text_secondary),
                            );
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(l10n::text(ui.ctx(), date.to_string()));
                            });
                        });
                    }
                }
                if !headlines.is_empty() && shown("News") {
                    ui.add_space(theme.spacing.md);
                    ui.label(RichText::new(l10n::text(ui.ctx(), "News")).strong());
                    for story in headlines.iter() {
                        let response = ui
                            .scope_builder(egui::UiBuilder::new().sense(Sense::click()), |ui| {
                                ui.style_mut().interaction.selectable_labels = false;
                                ui.label(
                                    RichText::new(l10n::text(
                                        ui.ctx(),
                                        story_meta(story, timezone),
                                    ))
                                    .size(theme.text.small)
                                    .color(theme.ui.text_secondary),
                                );
                                ui.add(
                                    Label::new(
                                        RichText::new(l10n::text(ui.ctx(), &story.headline))
                                            .strong(),
                                    )
                                    .wrap(),
                                );
                            })
                            .response;
                        let translated_headline = l10n::text(ui.ctx(), &story.headline);
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(
                                egui::Role::Button,
                                true,
                                &translated_headline,
                            )
                        });
                        let hover =
                            l10n::text(ui.ctx(), story.url.as_deref().unwrap_or("Show on chart"));
                        if response
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .on_hover_text(hover)
                            .clicked()
                        {
                            *news_action = story
                                .url
                                .clone()
                                .map_or(NewsAction::ShowOnChart(story.time), NewsAction::OpenUrl);
                        }
                    }
                }
                if shown("Notes") {
                    ui.add_space(theme.spacing.md);
                    ui.label(RichText::new(l10n::text(ui.ctx(), "Notes")).strong());
                    let notes = notes.entry(instrument.symbol.clone()).or_default();
                    // Refused notes stay as typed until the user's choice settles them.
                    let field = ui.add_enabled(
                        notes.written.is_some() && !revisions.refused(&instrument.symbol),
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

/// A signed percentage in the rising or falling colour, or a dash when the host lacks it.
fn percent_text(percent: Option<f64>, theme: &crate::theme::Theme) -> (String, Color32) {
    match percent.filter(|percent| percent.is_finite()) {
        Some(percent) => (
            format!("{}%", grouped(percent, 2, true)),
            if percent > 0.0 {
                theme.chart.candle_up
            } else if percent < 0.0 {
                theme.chart.candle_down
            } else {
                theme.ui.text
            },
        ),
        None => ("-".to_owned(), theme.ui.text_secondary),
    }
}

/// A titled section that says what a host-supplied answer is: still loading, empty, not supplied
/// or failed, and otherwise draws it with `body`.
pub(super) fn detail_section<T>(
    ui: &mut Ui,
    title: &str,
    state: &MarketDetail<T>,
    is_empty: impl Fn(&T) -> bool,
    body: impl FnOnce(&mut Ui, &T),
) {
    let theme = ui.active_theme();
    ui.add_space(theme.spacing.md);
    ui.label(RichText::new(l10n::text(ui.ctx(), title)).strong());
    let note = |ui: &mut Ui, text: &str| {
        ui.label(RichText::new(l10n::text(ui.ctx(), text)).color(theme.ui.text_secondary));
    };
    match state {
        MarketDetail::Loading => note(ui, "Loading…"),
        MarketDetail::Unavailable => note(ui, "Not available"),
        MarketDetail::Failed(message) => status_message(ui, MessageKind::Error, message),
        MarketDetail::Ready(value) if is_empty(value) => note(ui, "No data"),
        MarketDetail::Ready(value) => body(ui, value),
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
        l10n::text(ui.ctx(), grouped(price, usize::from(decimals), false)),
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
        grouped(change, usize::from(decimals), true),
        format!("{}%", grouped(percent, 2, true)),
    ] {
        job.append(&text, theme.spacing.sm, weighted(change_size, 600.0, color));
    }
    ui.add(Label::new(job).wrap());
}

/// "UTC" for no offset, otherwise "GMT" and the signed offset in hours, with minutes when they
/// are not zero, such as "GMT+2" or "GMT+5:30".
pub(super) fn zone_label(offset_seconds: i32) -> String {
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

/// A story's publication time in `timezone`, with its offset, and its source when known.
fn story_meta(story: &NewsStory, timezone: &TimezoneId) -> String {
    let local = story.time.with_timezone(timezone);
    let time = format!(
        "{} {}",
        local.format("%b %-d, %H:%M"),
        zone_label(local.offset().fix().local_minus_utc())
    );
    if story.source.is_empty() {
        time
    } else {
        format!("{time} · {}", story.source)
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

/// Whether the ratings read `a` and `b` alike: the same time and prices and volume, bit for bit,
/// so a bar with NaN prices matches itself.
fn same_bar(a: &Bar, b: &Bar) -> bool {
    a.time == b.time
        && [a.open, a.high, a.low, a.close, a.volume].map(f64::to_bits)
            == [b.open, b.high, b.low, b.close, b.volume].map(f64::to_bits)
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
    use crate::data::{FinancialPage, HistoryPage, HistoryRequest, Reply};
    use std::sync::Mutex;

    /// Keeps detail requests for the test to answer in any order, and answers financials at once
    /// with a page named after the symbol.
    #[derive(Default)]
    struct Feed(Mutex<Vec<(String, Reply<SymbolDetails>)>>);

    impl Datafeed for Feed {
        fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
            reply.send(Ok(HistoryPage::default()));
        }

        fn financials(&self, symbol: &str, reply: Reply<CompanyFinancials>) {
            let page = FinancialPage::new(symbol, Vec::new());
            reply.send(Ok(CompanyFinancials::new(None, vec![page])));
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

        /// Fails the writes waiting for an answer, keeping nothing.
        fn fail_saves(&self) {
            for (_, _, reply) in std::mem::take(&mut *self.saves.lock().unwrap()) {
                reply.send(Err(FeedError::Network("offline".into())));
            }
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

    /// Keeps calendar and news requests for the test to answer.
    #[derive(Default)]
    struct Market {
        calendar: Mutex<Vec<(CalendarQuery, Reply<Vec<CalendarEvent>>)>>,
        news: Mutex<Vec<(NewsQuery, Reply<Vec<NewsStory>>)>>,
    }

    impl Datafeed for Market {
        fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
            reply.send(Ok(HistoryPage::default()));
        }

        fn calendar(&self, query: CalendarQuery, reply: Reply<Vec<CalendarEvent>>) {
            self.calendar.lock().unwrap().push((query, reply));
        }

        fn news(&self, query: NewsQuery, reply: Reply<Vec<NewsStory>>) {
            self.news.lock().unwrap().push((query, reply));
        }
    }

    /// Hourly bars from 2026-01-01 closing at `closes`, each opening at the previous close.
    fn bars(closes: &[f64]) -> Vec<Bar> {
        let start: DateTime<Utc> = "2026-01-01T00:00:00Z".parse().unwrap();
        let mut open = closes[0];
        closes
            .iter()
            .enumerate()
            .map(|(i, &close)| {
                let bar = Bar::new(
                    start + TimeDelta::hours(i as i64),
                    open,
                    open.max(close) + 0.5,
                    open.min(close) - 0.5,
                    close,
                    1000.0 + (i % 7) as f64 * 100.0,
                );
                open = close;
                bar
            })
            .collect()
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
        l10n::set_translator(&ctx, |text| {
            (text == "Key statistics").then(|| "Facts".to_owned())
        });
        let feed = Arc::new(Feed::default());
        let store = Arc::new(Store::default());
        let mut panel = SymbolDetailsPanel::new();
        panel.set_feed(feed.clone());
        panel.set_instrument(&Instrument::new("AAPL"));
        panel.set_instrument(&Instrument::new("MSFT"));
        // Storage attached while the feed's answers are on their way does not drop them.
        panel.set_storage(store.clone());
        // The answer for the previous symbol arrives last and must not replace the current one.
        let requests = std::mem::take(&mut *feed.0.lock().unwrap());
        for (symbol, reply) in requests.into_iter().rev() {
            let statistics = vec![("Market cap".into(), format!("{symbol} cap"))];
            reply.send(Ok(SymbolDetails::new(statistics, Vec::new())
                .with_category(format!("{symbol} category"))));
        }
        panel.poll(&ctx);
        let shown = texts(&ctx, &mut panel);
        assert!(shown.contains(&"MSFT cap".into()), "{shown:?}");
        assert!(!shown.contains(&"AAPL cap".into()), "{shown:?}");
        assert!(shown.contains(&"MSFT category".into()), "{shown:?}");
        assert!(!shown.contains(&"AAPL category".into()), "{shown:?}");
        assert!(shown.contains(&"Facts".into()), "{shown:?}");
        assert!(!shown.contains(&"Key statistics".into()), "{shown:?}");
        assert!(!shown.contains(&"Profile".into()), "{shown:?}");
        assert_eq!(panel.financials().unwrap().pages[0].name, "MSFT");

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

        // A failed write is tried again a second later without a new edit, and a run of
        // failures is reported once.
        let poll_at = |panel: &mut SymbolDetailsPanel, time: f64| {
            let input = egui::RawInput {
                time: Some(time),
                ..Default::default()
            };
            ctx.run_ui(input, |ui| panel.poll(ui.ctx()))
                .drop_without_applying_deltas();
        };
        restored.error = None;
        restored.notes.get_mut("MSFT").unwrap().text = "third".into();
        poll_at(&mut restored, 10.0);
        store.fail_saves();
        poll_at(&mut restored, 10.0);
        assert!(restored.error.take().is_some());
        assert!(store.saves.lock().unwrap().is_empty());
        poll_at(&mut restored, 10.5);
        assert!(store.saves.lock().unwrap().is_empty());
        poll_at(&mut restored, 11.0);
        assert_eq!(store.saves.lock().unwrap().len(), 1);
        store.fail_saves();
        poll_at(&mut restored, 11.0);
        assert!(restored.error.is_none());
        poll_at(&mut restored, 12.0);
        assert_eq!(store.answer_saves(), [r#"{"version":1,"notes":"third"}"#]);
    }

    #[test]
    fn notes_changed_elsewhere_wait_for_the_users_choice() {
        use crate::services::document::tests::Revised;
        let ctx = egui::Context::default();
        let document = |notes: &str| format!(r#"{{"version":1,"notes":"{notes}"}}"#);
        for (choice, kept) in [
            (ConflictChoice::ReloadRemote, "remote"),
            (ConflictChoice::Retry, "local"),
        ] {
            let host = Arc::new(Revised::default());
            host.put("MSFT", &document("old"));
            let mut panel = SymbolDetailsPanel::new();
            panel.set_storage(host.clone());
            panel.set_instrument(&Instrument::new("MSFT"));
            panel.poll(&ctx);
            assert_eq!(panel.notes["MSFT"].text, "old");
            // Another client changes the notes after they were read.
            host.put("MSFT", &document("remote"));
            panel.notes.get_mut("MSFT").unwrap().text = "local".into();
            panel.poll(&ctx);
            panel.poll(&ctx);
            assert_eq!(panel.storage_conflict(), Some("MSFT"));
            assert_eq!(host.get("MSFT"), Some(document("remote")));
            panel.resolve_storage_conflict("MSFT", choice);
            panel.poll(&ctx);
            panel.poll(&ctx);
            assert_eq!(panel.storage_conflict(), None);
            assert_eq!(panel.notes["MSFT"].text, kept);
            assert_eq!(host.get("MSFT"), Some(document(kept)));
        }
    }

    /// The extended-hours change is measured from the regular last price, not the previous close,
    /// and update times carry the offset of the chart's time zone on that date.
    #[cfg(feature = "timezones")]
    #[test]
    fn extended_hours_change_from_the_regular_last_price() {
        let ctx = egui::Context::default();
        let mut panel = SymbolDetailsPanel::new();
        panel.set_instrument(&Instrument::new("QQQ").with_price_decimals(2));
        panel.set_timezone("Europe/Berlin".parse().unwrap());
        let quote = Quote::new(704.47, 725.20, "2026-10-22T19:59:00Z".parse().unwrap());
        for (session, label) in [
            (ExtendedSession::PreMarket, "Pre-market"),
            (ExtendedSession::PostMarket, "Post-market"),
            (ExtendedSession::Overnight, "Overnight"),
        ] {
            panel.set_quote(
                quote
                    .clone()
                    .with_extended(crate::model::ExtendedHoursQuote {
                        session,
                        last: 703.65,
                        // Summer time ends in Berlin on October 25.
                        time: "2026-10-26T12:30:00Z".parse().unwrap(),
                    }),
            );
            let shown = texts(&ctx, &mut panel);
            for text in [
                label,
                "704.47-20.73-2.86%",
                "Last update at 21:59 GMT+2",
                "703.65-0.82-0.12%",
                "Last update at 13:30 GMT+1",
            ] {
                assert!(shown.contains(&text.into()), "{text}: {shown:?}");
            }
        }
        for (offset, label) in [(0, "UTC"), (-5 * 3600, "GMT-5"), (-12600, "GMT-3:30")] {
            assert_eq!(zone_label(offset), label);
        }
    }

    /// The sections ask once per symbol, ignore answers for another symbol, and say whether they
    /// are loading, empty, unavailable or failed instead of leaving a blank. A regression shows
    /// the previous symbol's figures, asks every frame, or hides a failure.
    #[test]
    fn market_detail_sections_ask_once_and_state_what_the_host_answered() {
        use crate::ui::widget_bar::SeasonalityMonth;
        let ctx = egui::Context::default();
        let mut panel = SymbolDetailsPanel::new();
        assert!(panel.take_market_requests().is_empty(), "no symbol");
        panel.set_instrument(&Instrument::new("AAA"));
        assert!(!texts(&ctx, &mut panel).contains(&"Performance".into()));
        assert_eq!(
            panel.take_market_requests(),
            [
                MarketDetailRequest::PeriodReturns("AAA".into()),
                MarketDetailRequest::Seasonality("AAA".into())
            ]
        );
        assert!(panel.take_market_requests().is_empty());
        let shown = texts(&ctx, &mut panel);
        assert_eq!(shown.iter().filter(|t| *t == "Loading…").count(), 2);
        panel.set_period_returns(
            "OLD",
            MarketDetail::Ready(vec![PeriodReturn::new("1M", Some(9.0))]),
        );
        panel.set_period_returns(
            "AAA",
            MarketDetail::Ready(vec![
                PeriodReturn::new("1M", Some(2.5)),
                PeriodReturn::new("1Y", Some(-10.0)),
                PeriodReturn::new("5Y", None),
            ]),
        );
        panel.set_seasonality("AAA", MarketDetail::Failed("feed offline".into()));
        let shown = texts(&ctx, &mut panel);
        for text in ["+2.50%", "-10.00%", "-", "feed offline"] {
            assert!(shown.contains(&text.into()), "{text}: {shown:?}");
        }
        assert!(!shown.contains(&"+9.00%".into()));
        panel.set_period_returns("AAA", MarketDetail::Ready(vec![]));
        panel.set_seasonality(
            "AAA",
            MarketDetail::Ready(Seasonality::new(
                20,
                vec![SeasonalityMonth::new(1, Some(1.25), Some(0.65))],
            )),
        );
        let shown = texts(&ctx, &mut panel);
        for text in [
            "No data",
            "Jan",
            "+1.25%",
            "65% up",
            "Average monthly return over 20 years",
        ] {
            assert!(shown.contains(&text.into()), "{text}: {shown:?}");
        }
        panel.set_period_returns("AAA", MarketDetail::Unavailable);
        assert!(texts(&ctx, &mut panel).contains(&"Not available".into()));
        panel.set_instrument(&Instrument::new("BBB"));
        assert_eq!(
            panel.take_market_requests().len(),
            2,
            "a new symbol asks again"
        );
    }

    /// The Technicals rows read the study's ratings of the latest bar with values, whether the
    /// bars were rated whole or from the last rated bar on. A regression shows another line in a
    /// row, keeps the rating of a changed last bar or of fewer bars, rates the bar without values
    /// at the end, or keeps the previous symbol's rating.
    #[test]
    fn technicals_show_the_study_ratings_of_the_latest_bar_with_values() {
        let ctx = egui::Context::default();
        let trend = bars(
            &(0..300)
                .map(|i| {
                    if i < 285 {
                        100.0 + 0.3 * i as f64
                    } else {
                        185.5 - 2.0 * (i as f64 - 284.0)
                    }
                })
                .collect::<Vec<_>>(),
        );
        let wave = bars(
            &(0..300)
                .map(|i| 101.0 + 0.2 * i as f64 + 6.0 * (i as f64 / 7.0).sin())
                .collect::<Vec<_>>(),
        );
        // A fresh study's words for the last of `bars`, in the order of the rows.
        let words = |bars: &[Bar]| {
            let mut study = TechnicalRatings::new();
            study.calculate(bars);
            let last = bars.len() - 1;
            [2, 0, 1].map(|line| {
                let score = study.values().value(line, last);
                score.map_or("N/A", technical_rating)
            })
        };
        let rows = |panel: &mut SymbolDetailsPanel| {
            let shown = texts(&ctx, panel);
            ["Summary", "Moving averages", "Oscillators"].map(|label| {
                let row = shown.iter().position(|text| text == label);
                row.and_then(|row| shown.get(row + 1))
                    .cloned()
                    .unwrap_or_default()
            })
        };
        // The fixtures give each row its own word, and other words after each change below.
        let [summary, averages, oscillators] = words(&trend[..250]);
        assert!(summary != averages && averages != oscillators && summary != oscillators);
        assert_ne!(words(&trend[..250]), words(&trend));
        assert_ne!(words(&wave[..299]), words(&wave));

        let mut panel = SymbolDetailsPanel::new();
        panel.set_instrument(&Instrument::new("AAA"));
        assert!(!texts(&ctx, &mut panel).contains(&"Technicals".into()));
        panel.set_bars(Timeframe::Hour1, &trend[..250]);
        let shown = texts(&ctx, &mut panel);
        assert!(shown.contains(&"Technicals".into()), "{shown:?}");
        assert!(
            shown.contains(&Timeframe::Hour1.as_str().into()),
            "{shown:?}"
        );
        assert_eq!(rows(&mut panel), words(&trend[..250]));
        panel.set_bars(Timeframe::Hour1, &trend);
        assert_eq!(rows(&mut panel), words(&trend));
        panel.set_bars(Timeframe::Hour1, &[]);
        assert!(!texts(&ctx, &mut panel).contains(&"Technicals".into()));
        let mut tail = wave[..299].to_vec();
        tail.push(Bar::whitespace(wave[299].time));
        panel.set_bars(Timeframe::Hour1, &tail);
        assert_eq!(rows(&mut panel), words(&wave[..299]));
        // A live change of the last bar.
        panel.set_bars(Timeframe::Hour1, &wave);
        assert_eq!(rows(&mut panel), words(&wave));
        panel.set_bars(Timeframe::Hour1, &trend);
        panel.set_instrument(&Instrument::new("BBB"));
        assert!(!texts(&ctx, &mut panel).contains(&"Technicals".into()));
        // The chart names the new symbol before replacing its history on the same bar times.
        panel.set_bars(Timeframe::Hour1, &trend);
        panel.set_bars(Timeframe::Hour1, &wave);
        assert_eq!(rows(&mut panel), words(&wave));
    }

    /// The panel asks for the shown symbol's earnings, dividends and headlines with the symbol in
    /// the query, and shows its next earnings and ex-dividend dates in the panel's time zone and
    /// its three newest headlines, an opened one being reported to the host. A regression asks
    /// without the symbol, shows an earlier symbol's answer, another symbol's or a later event or
    /// older headlines, drops the answers when storage is attached, or reports a feed without
    /// calendar and news.
    #[test]
    fn events_and_headlines_ask_for_the_symbol_and_hide_without_support() {
        use egui_kittest::{Harness, kittest::Queryable};
        let ctx = egui::Context::default();
        let at = |time: &str| time.parse::<DateTime<Utc>>().unwrap();
        let event = |kind, time: &str, symbol: Option<&str>| {
            let mut event = CalendarEvent::new(time, kind, at(time), "Report");
            event.symbol = symbol.map(str::to_owned);
            event
        };
        let story = |time: &str, headline: &str, source: &str, url: Option<&str>| {
            let mut story = NewsStory::new(time, at(time), headline);
            story.source = source.into();
            story.url = url.map(str::to_owned);
            story
        };
        let market = Arc::new(Market::default());
        let mut panel = SymbolDetailsPanel::new();
        panel.set_feed(market.clone());
        panel.set_instrument(&Instrument::new("OLD"));
        panel.set_instrument(&Instrument::new("AAA"));
        panel.set_storage(Arc::new(Store::default()));
        let calendar = std::mem::take(&mut *market.calendar.lock().unwrap());
        let news = std::mem::take(&mut *market.news.lock().unwrap());
        let symbols = |symbol: &str| vec![symbol.to_owned()];
        let asked: Vec<_> = calendar
            .iter()
            .map(|(query, _)| (query.kind, query.symbols.clone()))
            .collect();
        assert_eq!(
            asked,
            [
                (CalendarKind::Earnings, symbols("OLD")),
                (CalendarKind::Dividends, symbols("OLD")),
                (CalendarKind::Earnings, symbols("AAA")),
                (CalendarKind::Dividends, symbols("AAA")),
            ]
        );
        let asked: Vec<_> = news
            .iter()
            .map(|(query, _)| (query.symbols.clone(), query.limit))
            .collect();
        assert_eq!(asked, [(symbols("OLD"), 3), (symbols("AAA"), 3)]);
        let Ok(
            [
                (_, old_earnings),
                (_, old_dividends),
                (_, earnings),
                (_, dividends),
            ],
        ) = <[_; 4]>::try_from(calendar)
        else {
            unreachable!()
        };
        let Ok([(_, old_news), (_, news)]) = <[_; 2]>::try_from(news) else {
            unreachable!()
        };
        // A feed may answer events of other symbols, and stories in any order and beyond the limit.
        earnings.send(Ok(vec![
            event(CalendarKind::Earnings, "2027-01-10T21:00:00Z", Some("ZZZ")),
            event(CalendarKind::Earnings, "2027-02-03T21:00:00Z", Some("AAA")),
            event(CalendarKind::Earnings, "2027-01-28T21:00:00Z", Some("aaa")),
        ]));
        dividends.send(Ok(vec![event(
            CalendarKind::Dividends,
            "2027-02-09T13:30:00Z",
            None,
        )]));
        let url = "https://example.com/third";
        news.send(Ok(vec![
            story("2027-01-05T14:30:00Z", "Third headline", "Wire", Some(url)),
            story("2027-01-06T09:00:00Z", "Newest headline", "", None),
            story("2027-01-04T08:00:00Z", "Oldest headline", "", None),
            story("2027-01-05T18:15:00Z", "Second headline", "", None),
        ]));
        // The earlier symbol's answers arrive last.
        let old = event(CalendarKind::Earnings, "2027-01-20T21:00:00Z", Some("OLD"));
        old_earnings.send(Ok(vec![old]));
        let old = event(CalendarKind::Dividends, "2027-01-21T13:30:00Z", Some("OLD"));
        old_dividends.send(Ok(vec![old]));
        old_news.send(Ok(vec![story(
            "2027-01-07T00:00:00Z",
            "Old headline",
            "",
            None,
        )]));
        panel.poll(&ctx);
        let shown = texts(&ctx, &mut panel);
        let dates = |shown: &[String]| {
            ["Earnings", "Ex-dividend"].map(|label| {
                let row = shown.iter().position(|text| text == label);
                row.and_then(|row| shown.get(row + 1))
                    .cloned()
                    .unwrap_or_default()
            })
        };
        assert_eq!(dates(&shown), ["Jan 28, 2027", "Feb 9, 2027"]);
        for text in [
            "Upcoming events",
            "News",
            "Jan 5, 14:30 UTC · Wire",
            "Jan 6, 09:00 UTC",
        ] {
            assert!(shown.contains(&text.into()), "{text}: {shown:?}");
        }
        for text in [
            "Jan 10, 2027",
            "Feb 3, 2027",
            "Jan 20, 2027",
            "Jan 21, 2027",
        ] {
            assert!(!shown.contains(&text.into()), "{text}: {shown:?}");
        }
        let headlines: Vec<_> = shown
            .iter()
            .filter(|text| text.ends_with("headline"))
            .collect();
        assert_eq!(
            headlines,
            ["Newest headline", "Second headline", "Third headline"]
        );
        // 21:00 UTC is the next morning at UTC+10.
        panel.set_timezone(TimezoneId::fixed_offset(10 * 3600).unwrap());
        let shown = texts(&ctx, &mut panel);
        assert_eq!(dates(&shown), ["Jan 29, 2027", "Feb 9, 2027"]);
        assert!(
            shown.contains(&"Jan 6, 00:30 GMT+10 · Wire".into()),
            "{shown:?}"
        );

        let mut h = Harness::builder()
            .allow_missing_glyphs()
            .with_size(vec2(300.0, 800.0))
            .build_ui_state(|ui, panel: &mut SymbolDetailsPanel| panel.show(ui), panel);
        h.get_by_role_and_label(egui::Role::Button, "Third headline")
            .click();
        h.run();
        assert_eq!(
            h.state_mut().take_news_action(),
            NewsAction::OpenUrl(url.into())
        );
        h.get_by_role_and_label(egui::Role::Button, "Newest headline")
            .click();
        h.run();
        assert_eq!(
            h.state_mut().take_news_action(),
            NewsAction::ShowOnChart(at("2027-01-06T09:00:00Z"))
        );
        assert_eq!(h.state_mut().take_news_action(), NewsAction::None);

        // Replacing a supported feed with one without calendar and news hides its old answers.
        let panel = h.state_mut();
        panel.set_feed(market.clone());
        let pending_calendar = std::mem::take(&mut *market.calendar.lock().unwrap());
        let pending_news = std::mem::take(&mut *market.news.lock().unwrap());
        panel.set_feed(Arc::new(Feed::default()));
        // Answers from the previous feed arrive after the replacement's unsupported answers.
        for (query, reply) in pending_calendar {
            reply.send(Ok(vec![event(
                query.kind,
                "2027-01-28T21:00:00Z",
                Some("AAA"),
            )]));
        }
        for (_, reply) in pending_news {
            reply.send(Ok(vec![story(
                "2027-01-07T00:00:00Z",
                "Previous feed headline",
                "",
                None,
            )]));
        }
        panel.poll(&ctx);
        let shown = texts(&ctx, panel);
        assert!(!shown.contains(&"Upcoming events".into()), "{shown:?}");
        assert!(!shown.contains(&"News".into()), "{shown:?}");
        assert_eq!(panel.error, None);
    }

    /// The layout menu hides the Technicals, Upcoming events and News sections like the others,
    /// and they stay hidden for the next symbol until shown again. A regression leaves a section
    /// out of the menu, draws it while hidden, or shows it again for another symbol.
    #[test]
    fn layout_menu_hides_the_new_sections_across_symbols() {
        use egui_kittest::{Harness, kittest::Queryable};
        const NEW: [&str; 3] = ["Technicals", "Upcoming events", "News"];
        let market = Arc::new(Market::default());
        // Answers the shown symbol's requests with one event and one headline, and gives it bars.
        let answer = |panel: &mut SymbolDetailsPanel| {
            for (query, reply) in std::mem::take(&mut *market.calendar.lock().unwrap()) {
                let event = CalendarEvent::new("event", query.kind, query.from, "Report");
                reply.send(Ok(vec![event]));
            }
            for (_, reply) in std::mem::take(&mut *market.news.lock().unwrap()) {
                reply.send(Ok(vec![NewsStory::new("story", Utc::now(), "Headline")]));
            }
            panel.poll(&egui::Context::default());
            panel.set_bars(Timeframe::Hour1, &bars(&[100.0, 101.0]));
        };
        let shown = |panel: &mut SymbolDetailsPanel| {
            let painted = texts(&egui::Context::default(), panel);
            NEW.map(|title| painted.contains(&title.into()))
        };
        let mut panel = SymbolDetailsPanel::new();
        panel.set_feed(market.clone());
        panel.set_instrument(&Instrument::new("AAA"));
        answer(&mut panel);
        assert_eq!(shown(&mut panel), [true; 3]);

        let mut h = Harness::builder()
            .allow_missing_glyphs()
            .with_size(vec2(300.0, 800.0))
            .build_ui_state(|ui, panel: &mut SymbolDetailsPanel| panel.show(ui), panel);
        h.get_by_label("Layout").click();
        h.run();
        let toggle = |h: &mut Harness<'_, SymbolDetailsPanel>| {
            for title in NEW {
                h.get_by_role_and_label(egui::Role::CheckBox, title).click();
                h.run();
            }
        };
        toggle(&mut h);
        assert_eq!(shown(h.state_mut()), [false; 3]);
        h.state_mut().set_instrument(&Instrument::new("BBB"));
        answer(h.state_mut());
        h.run();
        assert_eq!(shown(h.state_mut()), [false; 3]);
        toggle(&mut h);
        assert_eq!(shown(h.state_mut()), [true; 3]);
    }
}
