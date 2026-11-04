//! Synthetic answers for the market panels, derived from the symbol catalog so every symbol gets
//! stable, distinct content. A host answers these from its own data vendors.

use chrono::{DateTime, Datelike, DurationRound, NaiveTime, TimeDelta, TimeZone, Utc, Weekday};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use egui_charts::data::{
    CalendarEvent, CalendarKind, CalendarQuery, FeedError, HistoryPage, HotlistEntry, HotlistQuery,
    Importance, NewsQuery, NewsStory, Reply, ScreenerColumn, ScreenerColumnSet, ScreenerOp,
    ScreenerQuery, ScreenerRow,
};
use egui_charts::model::{Bar, Quote};
use egui_charts::services::{
    ChatEvent, ChatMessage, Chats, Events, Notification, Notifications, Quotes,
};

use egui_charts::ui::widget_bar::{PeriodReturn, Seasonality, SeasonalityMonth};

use super::live::LiveFeed;
use crate::sampledata::DemoSymbol;

/// Supplies search metadata and menu choices independently of historical price generation.
pub(super) fn search_catalog(
    dialog: &mut egui_charts::ui::top_toolbar::SymbolSearchDialog,
) -> Vec<egui_charts::model::Instrument> {
    let mut symbols = DemoSymbol::symbols();
    for symbol in &mut symbols {
        symbol.source = Some("Synthetic".into());
        if matches!(symbol.exchange.as_deref(), Some("NASDAQ" | "CME" | "Index")) {
            symbol.country = Some("US".into());
        }
        symbol.sector = match symbol.symbol.as_str() {
            "AAPL" | "MSFT" | "NVDA" => Some("Technology".into()),
            "TSLA" => Some("Consumer discretionary".into()),
            _ => None,
        };
        if symbol.symbol == "AAPL" {
            symbol.root = Some("AAPL".into());
        }
    }
    // A second synthetic provider serves the same security and historical series.
    let mut mirror = symbols
        .iter()
        .find(|symbol| symbol.symbol == "AAPL")
        .unwrap()
        .clone();
    mirror.source = Some("Synthetic mirror".into());
    symbols.push(mirror);
    dialog.set_countries(vec!["US".into()]);
    dialog.set_sectors(vec!["Technology".into(), "Consumer discretionary".into()]);
    dialog.set_data_sources(vec!["Synthetic".into(), "Synthetic mirror".into()]);
    symbols
}

/// The newest requested bars inside the inclusive start and exclusive end of a history range.
pub(super) fn history_page(
    bars: &[Bar],
    since: Option<DateTime<Utc>>,
    before: Option<DateTime<Utc>>,
    count: usize,
) -> HistoryPage {
    let end = bars.partition_point(|bar| before.is_none_or(|before| bar.time < before));
    let first = bars[..end].partition_point(|bar| since.is_some_and(|since| bar.time < since));
    let start = end.saturating_sub(count).max(first);
    HistoryPage::new(bars[start..end].to_vec(), start == 0)
}

/// Retained symbol timelines shared by a demo's chart slots and market panels.
/// Each symbol generates its archive once; subsequent samples advance only its live executions.
pub(super) struct DemoMarket {
    start: DateTime<Utc>,
    feeds: BTreeMap<String, LiveFeed>,
    quotes: BTreeMap<String, (DateTime<Utc>, Quote)>,
}

impl DemoMarket {
    pub(super) fn new(start: DateTime<Utc>, source: LiveFeed) -> Self {
        Self {
            start,
            feeds: [(source.instrument().symbol, source)].into(),
            quotes: BTreeMap::new(),
        }
    }

    /// Keeps retained executions moving even while their panels are hidden.
    pub(super) fn advance(&mut self, now: DateTime<Utc>) -> std::time::Duration {
        self.feeds
            .values()
            .map(|feed| feed.advance(now.max(feed.now())))
            .min()
            .unwrap_or(std::time::Duration::MAX)
    }

    pub(super) fn loading(&self) -> bool {
        self.feeds.values().any(LiveFeed::loading)
    }

    fn source(&mut self, symbol: &str, now: DateTime<Utc>) -> &LiveFeed {
        let feed = self
            .feeds
            .entry(symbol.to_owned())
            .or_insert_with(|| LiveFeed::generate(symbol, self.start));
        feed.advance(now.max(feed.now()));
        feed
    }

    pub(super) fn feed(&mut self, symbol: &str, now: DateTime<Utc>) -> LiveFeed {
        self.source(symbol, now).fork()
    }

    pub(super) fn quote(&mut self, symbol: &str, now: DateTime<Utc>) -> Option<Quote> {
        let symbol = DemoSymbol::catalog()
            .iter()
            .find(|spec| spec.symbol.eq_ignore_ascii_case(symbol))?
            .symbol;
        if let Some((at, quote)) = self.quotes.get(symbol)
            && *at == now
        {
            return Some(quote.clone());
        }
        let feed = self.source(symbol, now);
        if feed.loading() {
            return None;
        }
        let quote = feed.quote(now)?;
        self.quotes.insert(symbol.to_owned(), (now, quote.clone()));
        Some(quote)
    }
}

/// The watchlist's quote service: the chart symbol's quote from the live feed, with its regular
/// and extended-hours prices, session volume and best bid and ask, and retained executions for
/// other catalog symbols. The quotes follow the feed's simulation clock, which the pause freezes.
/// The demo has no background worker, so the controller publishes once per frame. Repeating a
/// quote only wakes the UI, so it is pushed again only when a displayed value changes.
pub(super) struct DemoQuotes {
    live: Mutex<Arc<LiveFeed>>,
    stream: Mutex<Option<(Vec<String>, Events<(String, Quote)>)>>,
    /// The last quote pushed per symbol, with times at display precision.
    last_pushed: Mutex<BTreeMap<String, Quote>>,
}

impl DemoQuotes {
    pub(super) fn new(live: Arc<LiveFeed>) -> Self {
        Self {
            live: Mutex::new(live),
            stream: Mutex::default(),
            last_pushed: Mutex::default(),
        }
    }

    /// Follows the selected chart slot and reports a changed feed so query panels can follow it.
    pub(super) fn set_live(&self, live: Arc<LiveFeed>) -> bool {
        let mut current = self.live.lock().unwrap_or_else(|e| e.into_inner());
        let changed = !Arc::ptr_eq(&current, &live);
        *current = live;
        changed
    }

    /// Pushes the current quote of every subscribed symbol.
    pub(super) fn publish(&self, now: DateTime<Utc>) {
        let stream = self.stream.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((symbols, events)) = stream.as_ref().filter(|(_, e)| e.is_active()) {
            self.push(symbols, events, now);
        }
    }

    fn push(&self, symbols: &[String], events: &Events<(String, Quote)>, now: DateTime<Utc>) {
        let live = self.live.lock().unwrap_or_else(|e| e.into_inner());
        let mut last_pushed = self.last_pushed.lock().unwrap_or_else(|e| e.into_inner());
        for symbol in symbols {
            let Some(quote) = live.market_quote(symbol, now) else {
                continue;
            };
            // Panels use the whole quote, but details shows both update times only to the minute.
            let mut pushed = quote.clone();
            pushed.time = pushed.time.duration_trunc(TimeDelta::minutes(1)).unwrap();
            if let Some(extended) = &mut pushed.extended {
                extended.time = extended.time.duration_trunc(TimeDelta::minutes(1)).unwrap();
            }
            if last_pushed.get(symbol) == Some(&pushed) {
                continue;
            }
            last_pushed.insert(symbol.clone(), pushed);
            events.push((symbol.clone(), quote));
        }
    }
}

impl Quotes for DemoQuotes {
    fn subscribe(&self, symbols: &[String], events: Events<(String, Quote)>) {
        // A new stream starts from the current quotes, whatever was pushed to the one before.
        self.last_pushed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        let now = self.live.lock().unwrap_or_else(|e| e.into_inner()).now();
        self.push(symbols, &events, now);
        *self.stream.lock().unwrap_or_else(|e| e.into_inner()) = Some((symbols.to_vec(), events));
    }
}

impl super::DemoApp {
    /// Returns and seasonality use daily history independently of the chart's interval or replay.
    pub(super) fn market_history(&self) -> Vec<Bar> {
        use egui_charts::model::{BarClock, Timeframe};
        let mut clock = BarClock::new(Timeframe::Day1);
        if self.chart.bar_clock().has_session() {
            clock = clock.with_session(
                crate::sampledata::us_equity_zone(),
                crate::sampledata::us_equity_session(),
                self.chart.extended_hours(),
            );
        }
        self.live
            .snapshot(Timeframe::Day1, clock, self.chart.extended_hours())
    }

    /// Range presets fit available bars and state a limit when the selected detail cannot reach it.
    pub(super) fn explain_history_limit(
        &mut self,
        range: egui_charts::ui::timeframe_toolbar::DateRange,
    ) {
        let bars = self.chart.bars(egui_charts::widget::SeriesId::MAIN);
        if let (Some(first), Some(last)) = (bars.first(), bars.last())
            && crate::sampledata::range_start(range, last.time)
                .is_some_and(|start| start < first.time)
        {
            self.notify(format!(
                "Synthetic {} history starts {}. Daily and longer intervals provide 26 years.",
                self.state.timeframe,
                first.time.format("%Y-%m-%d %H:%M UTC")
            ));
        } else if bars.is_empty() {
            self.notify("No synthetic bars at this interval yet. Wait for chart loading or choose a longer interval.");
        }
    }

    /// Prices every demo instrument independently of chart focus and watchlist membership.
    pub(super) fn feed_paper_prices(&self) {
        if self.replay.is_none() {
            for spec in DemoSymbol::catalog() {
                if let Some(quote) = self.live.market_quote(spec.symbol, self.live.now()) {
                    self.paper
                        .set_price(spec.symbol, quote.extended.map_or(quote.last, |q| q.last));
                }
            }
        }
    }
}

/// Headlines and their text, with `{name}` and `{symbol}` filled in per symbol.
const HEADLINES: &[(&str, &str)] = &[
    (
        "{name} extends gains as buyers return",
        "{symbol} rose through the morning on steady volume, recovering the losses of the previous \
         session. Buyers held the price above the opening range into the afternoon.",
    ),
    (
        "{name} slips after an early rally fades",
        "{symbol} opened higher but gave back its gains by midday as sellers met the move near \
         last week's high.",
    ),
    (
        "Volume in {symbol} runs above its 30-day average",
        "Turnover in {symbol} reached about one and a half times its 30-day average, with most of \
         it in the first hour of trading.",
    ),
    (
        "{name} holds near support ahead of the close",
        "{symbol} traded in a narrow range above the level that held twice this month.",
    ),
    (
        "Options activity in {symbol} picks up",
        "Call volume in {symbol} outpaced puts by two to one, concentrated in contracts expiring \
         this month.",
    ),
];

/// Highlights: one-line summaries of what moved the symbol.
const HIGHLIGHTS: &[(&str, &str)] = &[
    (
        "Momentum in {symbol}",
        "{name} closed higher in three of the last five sessions.",
    ),
    (
        "{symbol} near the top of its range",
        "{symbol} trades in the upper half of its 52-week range.",
    ),
    (
        "Rising activity in {symbol}",
        "Volume in {symbol} has risen for two sessions in a row.",
    ),
];

/// Up to `query.limit` synthetic stories, newest first, published on the hour before `now`.
pub(super) fn news(query: &NewsQuery, now: DateTime<Utc>) -> Vec<NewsStory> {
    let symbols: Vec<&DemoSymbol> = if query.symbols.is_empty() {
        DemoSymbol::catalog().iter().collect()
    } else {
        DemoSymbol::catalog()
            .iter()
            .filter(|spec| {
                query
                    .symbols
                    .iter()
                    .any(|symbol| spec.symbol.eq_ignore_ascii_case(symbol))
            })
            .collect()
    };
    let templates = if query.highlights {
        HIGHLIGHTS
    } else {
        HEADLINES
    };
    let hour = now.duration_trunc(TimeDelta::hours(1)).unwrap_or(now);
    let mut stories: Vec<NewsStory> = symbols
        .iter()
        .flat_map(|spec| {
            templates
                .iter()
                .enumerate()
                .map(move |(i, (headline, body))| {
                    let fill = |text: &str| {
                        text.replace("{name}", spec.company_name)
                            .replace("{symbol}", spec.symbol)
                    };
                    let age = TimeDelta::hours(i as i64 * 7 + (spec.seed % 5) as i64 + 1);
                    let mut story =
                        NewsStory::new(format!("{}-{i}", spec.symbol), hour - age, fill(headline));
                    story.source = "Demo Wire".into();
                    story.body = fill(body);
                    story.symbols = vec![spec.symbol.into()];
                    story.url = Some(format!("https://example.com/news/{}", story.id));
                    story
                })
        })
        .collect();
    stories.sort_by(|a, b| b.time.cmp(&a.time));
    stories.truncate(query.limit);
    stories
}

/// The countries the demo calendar knows.
pub(super) const COUNTRIES: [&str; 4] = ["US", "EU", "GB", "JP"];

/// Economic releases: country, release, importance, UTC hour, and the base value in percent.
const RELEASES: &[(&str, &str, Importance, u32, f64)] = &[
    ("US", "CPI (YoY)", Importance::High, 12, 3.1),
    (
        "US",
        "Initial jobless claims (K)",
        Importance::Medium,
        12,
        221.0,
    ),
    ("EU", "Manufacturing PMI", Importance::Medium, 8, 47.6),
    ("GB", "Retail sales (MoM)", Importance::Low, 6, 0.3),
    (
        "JP",
        "Industrial production (YoY)",
        Importance::Low,
        23,
        1.2,
    ),
    ("US", "Interest rate decision", Importance::High, 18, 5.25),
];

/// Companies listing in the demo IPO calendar, with their offer price.
const OFFERINGS: &[(&str, &str, f64)] = &[
    ("NWRB", "Northwind Robotics", 18.0),
    ("CLRW", "Clearwater Energy", 24.5),
    ("FBLK", "Fieldblock Software", 31.0),
];

/// The synthetic events of `query`'s calendar on the weekdays of its range, filtered as asked.
/// Released figures show for events before `now`.
pub(super) fn calendar(query: &CalendarQuery, now: DateTime<Utc>) -> Vec<CalendarEvent> {
    let mut events = Vec::new();
    let mut day = query.from.date_naive();
    while day.and_time(NaiveTime::MIN).and_utc() < query.to {
        let n = day.num_days_from_ce() as usize;
        if !matches!(day.weekday(), Weekday::Sat | Weekday::Sun) {
            let at = |hour: u32| day.and_hms_opt(hour, 30, 0).unwrap().and_utc();
            let id = |kind: &str, i: usize| format!("{kind}-{day}-{i}");
            match query.kind {
                CalendarKind::Economic => {
                    for i in 0..3 {
                        let (country, title, importance, hour, base) =
                            RELEASES[(n + i * 2) % RELEASES.len()];
                        let mut event =
                            CalendarEvent::new(id("eco", i), query.kind, at(hour), title);
                        let drift = ((n * 7 + i) % 5) as f64 / 10.0 - 0.2;
                        event.country = country.into();
                        event.importance = Some(importance);
                        event.forecast = Some(format!("{base:.1}"));
                        event.prior = Some(format!("{:.1}", base - drift / 2.0));
                        event.actual = (event.time < now).then(|| format!("{:.1}", base + drift));
                        events.push(event);
                    }
                }
                CalendarKind::Earnings | CalendarKind::Dividends => {
                    let stocks: Vec<_> = DemoSymbol::catalog()
                        .iter()
                        .filter(|spec| spec.exchange == "NASDAQ")
                        .collect();
                    let spec = stocks[n % stocks.len()];
                    let dividends = query.kind == CalendarKind::Dividends;
                    let mut event = CalendarEvent::new(
                        id(if dividends { "div" } else { "eps" }, 0),
                        query.kind,
                        at(if dividends { 13 } else { 20 }),
                        spec.company_name,
                    );
                    let eps = spec.start_price / 120.0;
                    event.country = "US".into();
                    event.symbol = Some(spec.symbol.into());
                    if dividends {
                        event.actual = Some(format!("{:.2}", eps / 4.0));
                        event.prior = Some(format!("{:.2}", eps / 4.2));
                    } else {
                        event.forecast = Some(format!("{eps:.2}"));
                        event.prior = Some(format!("{:.2}", eps * 0.94));
                        event.actual = (event.time < now).then(|| format!("{:.2}", eps * 1.03));
                    }
                    events.push(event);
                }
                CalendarKind::Ipos if n % 2 == 0 => {
                    let (symbol, name, price) = OFFERINGS[n / 2 % OFFERINGS.len()];
                    let mut event = CalendarEvent::new(id("ipo", 0), query.kind, at(14), name);
                    event.country = "US".into();
                    event.symbol = Some(symbol.into());
                    event.forecast = Some(format!("{price:.2}"));
                    events.push(event);
                }
                _ => {}
            }
        }
        day = day.succ_opt().unwrap();
    }
    events.retain(|event| {
        (query.countries.is_empty() || query.countries.contains(&event.country))
            && query
                .importance
                .is_none_or(|least| event.importance >= Some(least))
            && event.time >= query.from
    });
    events.sort_by_key(|event| event.time);
    events
}

/// The demo screener's column sets.
pub(super) fn screener_sets() -> Vec<ScreenerColumnSet> {
    let column = ScreenerColumn::new;
    vec![
        ScreenerColumnSet::new(
            "Overview",
            vec![
                column("price", "Price"),
                column("change_percent", "Chg %"),
                column("volume", "Volume"),
            ],
        ),
        ScreenerColumnSet::new(
            "Valuation",
            vec![column("price", "Price"), column("market_cap", "Mkt cap")],
        ),
    ]
}

/// The catalog symbols that pass `query`, from their shared execution quotes.
pub(super) fn screen(
    query: &ScreenerQuery,
    live: &LiveFeed,
) -> Result<Vec<ScreenerRow>, FeedError> {
    let value = |spec: &DemoSymbol, column: &str| {
        let quote = live.market_quote(spec.symbol, live.now())?;
        Some(match column {
            "price" => quote.last,
            "change_percent" => quote.change_percent(),
            "volume" => 0.5e6 + (spec.seed % 10) as f64 * 0.35e6,
            "market_cap" => spec.start_price * (spec.seed as f64 + 1.0) * 1.0e8,
            _ => return None,
        })
    };
    let known = |column: &String| value(&DemoSymbol::catalog()[0], column).is_some();
    if let Some(filter) = query.filters.iter().find(|filter| !known(&filter.column)) {
        return Err(FeedError::InvalidData(format!(
            "no column {} to filter",
            filter.column
        )));
    }
    let mut rows: Vec<(f64, ScreenerRow)> = DemoSymbol::catalog()
        .iter()
        .filter(|spec| {
            query.filters.iter().all(|filter| {
                value(spec, &filter.column).is_some_and(|v| match filter.op {
                    ScreenerOp::Above => v > filter.value,
                    ScreenerOp::Below => v < filter.value,
                })
            })
        })
        .map(|spec| {
            let decimals = usize::from(super::live::price_decimals(spec.symbol).unwrap());
            let values = query
                .columns
                .iter()
                .map(|column| match (column.as_str(), value(spec, column)) {
                    ("price", Some(v)) => format!("{v:.decimals$}"),
                    ("change_percent", Some(v)) => format!("{v:+.2}%"),
                    ("volume" | "market_cap", Some(v)) => compact(v),
                    _ => String::new(),
                })
                .collect();
            let key = query
                .sort_by
                .as_deref()
                .and_then(|column| value(spec, column))
                .unwrap_or(0.0);
            (key, ScreenerRow::new(spec.symbol, values))
        })
        .collect();
    if query.sort_by.is_some() {
        rows.sort_by(|a, b| a.0.total_cmp(&b.0));
        if query.descending {
            rows.reverse();
        }
    }
    Ok(rows
        .into_iter()
        .take(query.limit)
        .map(|(_, row)| row)
        .collect())
}

/// `value` with a K, M, B or T suffix and two decimals.
fn compact(value: f64) -> String {
    [(1e12, "T"), (1e9, "B"), (1e6, "M"), (1e3, "K")]
        .into_iter()
        .find(|(scale, _)| value.abs() >= *scale)
        .map_or_else(
            || format!("{value:.0}"),
            |(scale, suffix)| format!("{:.2}{suffix}", value / scale),
        )
}

/// The demo's hotlists.
pub(super) const HOTLISTS: [&str; 3] = ["Top gainers", "Top losers", "Most volatile"];

/// The catalog symbols ranked for `query.list` on their shared execution quotes.
pub(super) fn hotlist(query: &HotlistQuery, live: &LiveFeed) -> Vec<HotlistEntry> {
    let mut entries: Vec<HotlistEntry> = DemoSymbol::catalog()
        .iter()
        .filter_map(|spec| {
            let quote = live.market_quote(spec.symbol, live.now())?;
            let change = quote.change_percent();
            let mut entry = HotlistEntry::new(spec.symbol, quote.last, change);
            entry.price_decimals = super::live::price_decimals(spec.symbol);
            Some(entry)
        })
        .collect();
    match query.list.as_str() {
        "Top gainers" => {
            entries.retain(|entry| entry.change_percent > 0.0);
            entries.sort_by(|a, b| b.change_percent.total_cmp(&a.change_percent));
        }
        "Top losers" => {
            entries.retain(|entry| entry.change_percent < 0.0);
            entries.sort_by(|a, b| a.change_percent.total_cmp(&b.change_percent));
        }
        _ => entries.sort_by(|a, b| b.change_percent.abs().total_cmp(&a.change_percent.abs())),
    }
    entries.truncate(query.limit);
    entries
}

/// The demo's event log: what it reported to the user, newest last, kept for a new stream.
#[derive(Default)]
pub(super) struct DemoNotifications(
    Mutex<(Vec<Notification>, Option<Events<Notification>>)>,
    Mutex<Vec<std::sync::Weak<egui_charts::services::OrderFlowDetectors>>>,
    /// News delivery must not depend on whether the bounded log still contains the headline.
    Mutex<BTreeMap<String, DateTime<Utc>>>,
);

impl DemoNotifications {
    /// Include the chart's detector notifications in the shared log.
    pub(super) fn watch_detectors(
        &self,
        detectors: Arc<egui_charts::services::OrderFlowDetectors>,
    ) {
        let mut sources = self.1.lock().unwrap();
        sources.retain(|source| source.strong_count() > 0);
        let source = Arc::downgrade(&detectors);
        if sources.iter().any(|kept| kept.ptr_eq(&source)) {
            return;
        }
        if let Some(events) = &self.0.lock().unwrap().1 {
            detectors.watch(events.clone());
        }
        sources.push(source);
    }
    /// Logs `title` about `symbol`, if any, and pushes it to the panel.
    pub(super) fn notify(&self, title: impl Into<String>, symbol: Option<&str>) {
        let mut entry = Notification::new(Utc::now(), title);
        entry.symbol = symbol.map(str::to_owned);
        self.publish(entry);
    }

    /// Keep supplied headlines quiet on repeated viewport requests.
    pub(super) fn notify_event(&self, symbol: &str, event: &egui_charts::model::TimeScaleEvent) {
        let mut latest = self.2.lock().unwrap_or_else(|e| e.into_inner());
        if latest.get(symbol).is_some_and(|time| *time >= event.time) {
            return;
        }
        latest.insert(symbol.to_owned(), event.time);
        let mut entry = Notification::new(event.time, &event.title);
        entry.body = event.details.clone();
        entry.symbol = Some(symbol.into());
        self.publish(entry);
    }

    fn publish(&self, entry: Notification) {
        let mut state = self.0.lock().unwrap();
        if state.0.contains(&entry) {
            return;
        }
        if let Some(events) = &state.1 {
            events.push(entry.clone());
        }
        state.0.push(entry);
        let extra = state.0.len().saturating_sub(100);
        state.0.drain(..extra);
    }
}

impl Notifications for DemoNotifications {
    fn watch(&self, events: Events<Notification>) {
        for detectors in self
            .1
            .lock()
            .unwrap()
            .iter()
            .filter_map(std::sync::Weak::upgrade)
        {
            detectors.watch(events.clone());
        }
        let mut state = self.0.lock().unwrap();
        for entry in &state.0 {
            events.push(entry.clone());
        }
        state.1 = Some(events);
    }
}

/// Two rooms with a few synthetic messages, echoing what the user posts.
pub(super) struct DemoChats(Mutex<(Vec<ChatMessage>, Option<Events<ChatEvent>>)>);

impl DemoChats {
    pub(super) fn new(now: DateTime<Utc>) -> Self {
        let message = |room, author, minutes, text| {
            ChatMessage::new(room, author, now - TimeDelta::minutes(minutes), text)
        };
        Self(Mutex::new((
            vec![
                message("General", "Ana", 42, "Morning. Anyone watching the open?"),
                message(
                    "General",
                    "Ravi",
                    37,
                    "SPX holding above yesterday's close so far.",
                ),
                message(
                    "General",
                    "Ana",
                    12,
                    "Volume looks light ahead of the CPI release.",
                ),
                message("Crypto", "Lee", 25, "BTCUSD testing the range high again."),
                message(
                    "Crypto",
                    "Mia",
                    9,
                    "Watching for a close above it on the hourly.",
                ),
            ],
            None,
        )))
    }
}

impl Chats for DemoChats {
    fn watch(&self, events: Events<ChatEvent>) {
        let mut state = self.0.lock().unwrap();
        for message in &state.0 {
            events.push(ChatEvent::Message(message.clone()));
        }
        state.1 = Some(events);
    }

    fn send(&self, room: String, text: String, reply: Reply<()>) {
        let message = ChatMessage::new(room, "You", Utc::now(), text);
        let mut state = self.0.lock().unwrap();
        if let Some(events) = &state.1 {
            events.push(ChatEvent::Message(message.clone()));
        }
        state.0.push(message);
        reply.send(Ok(()));
    }
}

/// Synthetic universes for a host-supplied market heatmap, with live quote values filled later.
pub(super) fn heatmap_universes() -> Vec<egui_charts::ui::widget_bar::MarketHeatmapUniverse> {
    use egui_charts::ui::widget_bar::{MarketHeatmapEntry, MarketHeatmapUniverse};
    let entries = |stocks_only: bool| {
        DemoSymbol::catalog()
            .iter()
            .filter(|spec| {
                !stocks_only || !matches!(spec.exchange, "Crypto" | "Index" | "Forex" | "Synthetic")
            })
            .map(|spec| MarketHeatmapEntry {
                symbol: spec.symbol.into(),
                group: match spec.symbol {
                    "AAPL" | "MSFT" | "NVDA" => "Technology",
                    "TSLA" => "Consumer",
                    _ => spec.exchange,
                }
                .into(),
                values: [("weight", (spec.seed % 4 + 1) as f64)]
                    .map(|(id, value)| (id.to_owned(), value))
                    .into(),
            })
            .collect()
    };
    vec![
        MarketHeatmapUniverse::new("catalog", "Demo catalog", entries(false)),
        MarketHeatmapUniverse::new("stocks", "Demo stocks", entries(true)),
    ]
}

/// The metrics of [`heatmap_universes`]: a synthetic weight, and the live change and price.
pub(super) fn heatmap_metrics() -> Vec<egui_charts::ui::widget_bar::MarketHeatmapMetric> {
    use egui_charts::ui::widget_bar::MarketHeatmapMetric;
    let mut change = MarketHeatmapMetric::new("change", "Change", "%");
    change.saturation = Some(5.0);
    vec![
        MarketHeatmapMetric::new("weight", "Weight", ""),
        change,
        MarketHeatmapMetric::new("price", "Price", ""),
    ]
}

fn financials_symbol(symbol: &str) -> Option<&'static DemoSymbol> {
    DemoSymbol::catalog()
        .iter()
        .find(|spec| spec.symbol.eq_ignore_ascii_case(symbol))
        .filter(|spec| !matches!(spec.exchange, "Crypto" | "Index" | "Forex" | "Synthetic"))
}

impl super::DemoApp {
    /// Keeps Financials available only where the demo supplies figures.
    pub(super) fn feed_financials(&mut self) {
        let symbol = self.chart.symbol();
        self.symbol_details
            .set_financials_visible(financials_symbol(symbol).is_some());
        if self.symbol_details.take_financials_request()
            || self
                .financials
                .as_ref()
                .is_some_and(|(shown, _)| shown != symbol)
        {
            self.open_financials();
        }
    }

    /// Opens the current symbol's supplied pages, or returns to the chart when unavailable.
    pub(super) fn open_financials(&mut self) {
        let symbol = self.chart.symbol();
        self.financials = financials(symbol).map(|data| {
            let title = self
                .chart
                .instrument()
                .and_then(|instrument| instrument.description.clone())
                .unwrap_or_else(|| symbol.to_owned());
            let panel = egui_charts::ui::financials::FinancialsPanel::new(title, Some(data));
            (symbol.to_owned(), panel)
        });
        if self.financials.is_some() {
            self.open_panel = None;
        }
    }
}

/// Synthetic financials for the catalog's stocks: five fiscal years scaled from the catalog price,
/// one of them a loss, and dividends from the third year. Other symbols have none.
pub(super) fn financials(symbol: &str) -> Option<egui_charts::ui::financials::CompanyFinancials> {
    use egui_charts::ui::financials::{CompanyFinancials, FinancialPage};
    let spec = financials_symbol(symbol)?;
    let growth = 1.0 + (spec.seed % 7) as f64 / 40.0;
    let margin = 0.08 + (spec.seed % 5) as f64 / 50.0;
    let revenue: Vec<f64> = (0..5)
        .map(|year| spec.start_price * 4.0e8 * growth.powi(year))
        .collect();
    let net: Vec<f64> = revenue
        .iter()
        .enumerate()
        .map(|(year, revenue)| revenue * if year == 1 { -0.04 } else { margin })
        .collect();
    let shares = 1.5e11 / spec.start_price;
    let of = |values: &[f64], scale: f64| values.iter().map(|v| Some(v * scale)).collect();
    let years: Vec<String> = (2021..=2025).map(|year| format!("FY {year}")).collect();
    let income = FinancialPage::new("Income statement", years.clone())
        .with_row("Total revenue", of(&revenue, 1.0))
        .with_row("Gross profit", of(&revenue, 0.42))
        .with_row("Net income", of(&net, 1.0))
        .with_row("Diluted EPS", of(&net, 1.0 / shares));
    let balance = FinancialPage::new("Balance sheet", years.clone())
        .with_row("Total assets", of(&revenue, 1.3))
        .with_row("Total liabilities", of(&revenue, 0.8))
        .with_row("Total equity", of(&revenue, 0.5));
    let statistics = FinancialPage::new("Statistics", years)
        .with_row(
            "Net margin %",
            net.iter()
                .zip(&revenue)
                .map(|(n, r)| Some(n / r * 100.0))
                .collect(),
        )
        .with_row(
            "Dividends per share",
            (0..5)
                .map(|year| (year >= 2).then_some(0.2 * year as f64))
                .collect(),
        );
    Some(CompanyFinancials::new(
        Some("USD".to_owned()),
        vec![income, balance, statistics],
    ))
}

/// The return in percent from the close at or before each look-back date to the last close.
/// A period the history does not reach back to has no return.
pub(super) fn period_returns(bars: &[Bar]) -> Vec<PeriodReturn> {
    let Some(last) = bars.last() else {
        return Vec::new();
    };
    let year_start = Utc
        .with_ymd_and_hms(last.time.year(), 1, 1, 0, 0, 0)
        .single()
        .unwrap_or(last.time);
    [
        ("1W", last.time - TimeDelta::days(7)),
        ("1M", last.time - chrono::Months::new(1)),
        ("3M", last.time - chrono::Months::new(3)),
        ("6M", last.time - chrono::Months::new(6)),
        ("YTD", year_start),
        ("1Y", last.time - chrono::Months::new(12)),
    ]
    .into_iter()
    .map(|(period, since)| {
        let base = bars.iter().rev().find(|bar| {
            if period == "YTD" {
                bar.time < since
            } else {
                bar.time <= since
            }
        });
        let percent = base
            .filter(|base| base.close > 0.0)
            .map(|base| (last.close / base.close - 1.0) * 100.0);
        PeriodReturn::new(period, percent)
    })
    .collect()
}

/// The average return of each calendar month, from its first open to its last close, over the
/// complete months of `bars`; the first and last months may be partial and are left out.
pub(super) fn seasonality(bars: &[Bar]) -> Seasonality {
    let mut months: BTreeMap<(i32, u32), (f64, f64)> = BTreeMap::new();
    for bar in bars {
        months
            .entry((bar.time.year(), bar.time.month()))
            .and_modify(|month| month.1 = bar.close)
            .or_insert((bar.open, bar.close));
    }
    let months: Vec<_> = months.into_iter().collect();
    let complete = months
        .get(1..months.len().saturating_sub(1))
        .unwrap_or_default();
    let years = complete
        .iter()
        .map(|((year, _), _)| *year)
        .collect::<std::collections::BTreeSet<_>>()
        .len();
    let summaries = (1..=12u8)
        .filter_map(|month| {
            let returns: Vec<f64> = complete
                .iter()
                .filter(|((_, m), (open, _))| *m == u32::from(month) && *open > 0.0)
                .map(|(_, (open, close))| (close / open - 1.0) * 100.0)
                .collect();
            let count = returns.len() as f64;
            (count > 0.0).then(|| {
                SeasonalityMonth::new(
                    month,
                    Some(returns.iter().sum::<f64>() / count),
                    Some(returns.iter().filter(|r| **r > 0.0).count() as f64 / count),
                )
            })
        })
        .collect();
    Seasonality::new(years as u32, summaries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn long_intervals_warm_up_and_page_without_execution_history() {
        use egui_charts::model::{BarClock, Timeframe};
        use egui_charts::studies::{Indicator, SMA};
        // This path falls 1/hour until 06:00, then rises 3/hour until 18:00.
        // Splitting at 04:00 gives close 96 and volume 400; at 05:00, 95 and 500.
        for (day, previous, split, hours) in [
            (
                "2026-03-08T00:00:00Z",
                "2026-03-07T05:00:00Z",
                "2026-03-08T05:00:00Z",
                5,
            ),
            (
                "2026-03-09T00:00:00Z",
                "2026-03-08T05:00:00Z",
                "2026-03-09T04:00:00Z",
                4,
            ),
            (
                "2026-11-01T00:00:00Z",
                "2026-10-31T04:00:00Z",
                "2026-11-01T04:00:00Z",
                4,
            ),
            (
                "2026-11-02T00:00:00Z",
                "2026-11-01T04:00:00Z",
                "2026-11-02T05:00:00Z",
                5,
            ),
        ] {
            let bar = Bar::new(day.parse().unwrap(), 100.0, 130.0, 94.0, 106.0, 2400.0);
            let clock =
                BarClock::new(Timeframe::Day1).with_timezone(crate::sampledata::us_equity_zone());
            let split_price = 100.0 - f64::from(hours);
            assert_eq!(
                crate::sampledata::calendar_history(&[bar], &[], clock),
                vec![
                    Bar::new(
                        previous.parse().unwrap(),
                        100.0,
                        100.0,
                        split_price,
                        split_price,
                        f64::from(hours) * 100.0
                    ),
                    Bar::new(
                        split.parse().unwrap(),
                        split_price,
                        130.0,
                        94.0,
                        106.0,
                        2400.0 - f64::from(hours) * 100.0
                    ),
                ]
            );
        }
        let now = "2026-11-03T15:00:00Z".parse().unwrap();
        let daily = Bar::new(
            "2026-11-02T00:00:00Z".parse().unwrap(),
            100.0,
            130.0,
            94.0,
            106.0,
            2400.0,
        );
        let clock = BarClock::new(Timeframe::Day1)
            .with_timezone(crate::sampledata::us_equity_zone())
            .with_session(
                crate::sampledata::us_equity_zone(),
                crate::sampledata::us_equity_session(),
                false,
            );
        assert_eq!(
            crate::sampledata::calendar_history(&[daily.clone()], &[], clock.clone()),
            vec![Bar {
                time: "2026-11-02T05:00:00Z".parse().unwrap(),
                ..daily.clone()
            }]
        );
        assert!(
            crate::sampledata::calendar_history(
                &[Bar {
                    time: "2026-11-01T00:00:00Z".parse().unwrap(),
                    ..daily
                }],
                &[],
                clock
            )
            .is_empty(),
            "a shifted stock calendar must not invent Sunday bars"
        );
        let feed = Arc::new(LiveFeed::new("SAMPLE", now));
        super::super::tests::finish_feed(&feed);
        for timeframe in [Timeframe::Day1, Timeframe::Week1, Timeframe::Month1] {
            let bars = feed.snapshot(timeframe, BarClock::new(timeframe), false);
            assert!(bars.len() > 300, "{timeframe} must warm a 200-period study");
            let mut sma = SMA::new(200);
            sma.calculate(&bars);
            let expected = bars[..200].iter().map(|bar| bar.close).sum::<f64>() / 200.0;
            assert!((sma.values().value(0, 199).unwrap() - expected).abs() < 1e-8);
            let latest = history_page(&bars, None, None, 100);
            let older = history_page(&bars, None, Some(latest.bars[0].time), 100);
            assert_eq!(older.bars, bars[bars.len() - 200..bars.len() - 100]);
            assert!(!older.end_of_history);
            let ctx = egui::Context::default();
            let mut chart = egui_charts::ChartBuilder::new()
                .with_feed(feed.clone())
                .with_symbol("SAMPLE")
                .with_timeframe(timeframe)
                .with_initial_bars(256)
                .with_visible_candles(16)
                .build();
            for _ in 0..4 {
                super::super::tests::finish_feed(&feed);
                ctx.run_ui(Default::default(), |ui| {
                    chart.show(ui);
                })
                .drop_without_applying_deltas();
            }
            let held = chart.bars(egui_charts::widget::SeriesId::MAIN).len();
            assert!(held >= 256);
            // Initial layout may fetch another page per frame to fill the viewport.
            if bars.len() > 4 * 256 {
                assert!(
                    held < bars.len(),
                    "the live subscription must not bypass history paging"
                );
            }
        }
        let daily = feed.snapshot(Timeframe::Day1, BarClock::new(Timeframe::Day1), false);
        assert!(
            period_returns(&daily)
                .iter()
                .all(|value| value.percent.is_some())
        );
        // Twenty-six years of daily bars leave the minute archive and executions bounded.
        assert!(
            feed.snapshot(Timeframe::Min1, BarClock::new(Timeframe::Min1), false)
                .len()
                < 11_000
        );
    }

    #[test]
    fn period_returns_use_calendar_lookbacks_and_the_previous_year_close() {
        let bars: Vec<_> = [
            ("2023-12-31", 50.0),
            ("2024-02-28", 80.0),
            ("2024-02-29", 100.0),
            ("2024-08-28", 125.0),
            ("2024-11-28", 160.0),
            ("2024-12-31", 100.0),
            ("2025-01-01", 150.0),
            ("2025-01-28", 160.0),
            ("2025-01-29", 180.0),
            ("2025-02-21", 125.0),
            ("2025-02-28", 200.0),
        ]
        .into_iter()
        .map(|(day, close)| {
            Bar::new(
                format!("{day}T00:00:00Z").parse().unwrap(),
                close,
                close,
                close,
                close,
                1.0,
            )
        })
        .collect();
        for (actual, expected) in period_returns(&bars)
            .iter()
            .zip([60.0, 25.0, 25.0, 60.0, 100.0, 150.0])
        {
            assert!(
                (actual.percent.unwrap() - expected).abs() < 1e-9,
                "{}",
                actual.period
            );
        }
    }

    #[test]
    fn evicted_news_stays_quiet_when_history_is_reloaded_or_replayed() {
        use egui_charts::model::{EventKind, TimeScaleEvent};
        let notifications = DemoNotifications::default();
        let event = |minute| TimeScaleEvent {
            id: format!("sample-{minute}"),
            time: DateTime::UNIX_EPOCH + TimeDelta::minutes(minute),
            kind: EventKind::News,
            title: format!("Headline {minute}"),
            details: "Synthetic news".into(),
        };
        for minute in 0..120 {
            notifications.notify_event("SAMPLE", &event(minute));
        }
        notifications.notify_event("SAMPLE", &event(0));
        notifications.notify_event("SAMPLE", &event(119));
        let state = notifications.0.lock().unwrap();
        assert_eq!(state.0.len(), 100);
        assert_eq!(state.0.last().unwrap().time, event(119).time);
        drop(state);
        notifications.notify_event("BTCUSD", &event(0));
        assert_eq!(
            notifications
                .0
                .lock()
                .unwrap()
                .0
                .last()
                .unwrap()
                .symbol
                .as_deref(),
            Some("BTCUSD")
        );
    }

    fn demo_frame(
        app: &mut super::super::DemoApp,
        ctx: &egui::Context,
        events: Vec<egui::Event>,
    ) -> Vec<(String, egui::Rect)> {
        super::super::tests::finish_feed(&app.live);
        app.feed_market_panels(ctx);
        super::super::tests::finish_feed(&app.live);
        app.feed_market_panels(ctx);
        app.trading.poll(ctx, &mut app.toasts);
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1440.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ui| {
                app.trading.show_ticket(ui, &app.chart);
                egui::Panel::right("feed_test_details")
                    .exact_size(340.0)
                    .show(ui, |ui| {
                        app.symbol_details.show(ui);
                    });
                app.chart.show(ui);
            },
        );
        let texts = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some((
                    text.galley.text().to_owned(),
                    text.galley.rect.translate(text.pos.to_vec2()),
                )),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        texts
    }

    #[test]
    fn chart_quotes_follow_slots_tabs_and_symbols_outside_the_watchlist() {
        use egui_charts::ui::top_toolbar::LayoutAction;
        use egui_charts::ui::widget_bar::{Watchlist, WatchlistPanel};
        let ctx = egui::Context::default();
        let now = "2026-10-23T15:00:00Z".parse().unwrap();
        let mut app = super::super::DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            now,
            true,
        );
        app.handle_layout_action(LayoutAction::SelectGrid(
            egui_charts::widget::ChartLayoutMode::TwoHorizontal,
        ));
        app.activate_chart_slot(1);
        app.switch_symbol("MSFT");
        for slot in [1, 0, 1] {
            app.activate_chart_slot(slot);
            app.live.set_paused(true);
            for _ in 0..4 {
                demo_frame(&mut app, &ctx, vec![]);
            }
            let expected = app.live.quote(app.live.now()).unwrap();
            assert_eq!(app.watchlist.quote(app.chart.symbol()), Some(&expected));
            app.live.advance(now + TimeDelta::seconds(2));
            demo_frame(&mut app, &ctx, vec![]);
            assert_eq!(
                app.watchlist.quote(app.chart.symbol()),
                Some(&expected),
                "the selected pause freezes its quote"
            );
        }
        let frozen = app.live.quote(app.live.now()).unwrap();
        app.activate_chart_slot(0);
        app.live.set_paused(false);
        (1..=40)
            .find(|step| {
                app.live.advance(now + TimeDelta::milliseconds(step * 100));
                app.live.market_quote("MSFT", app.live.now()).unwrap().last != frozen.last
            })
            .expect("the other slot advances the market beyond the paused quote");
        for _ in 0..4 {
            demo_frame(&mut app, &ctx, vec![]);
        }
        assert_ne!(app.watchlist.quote("MSFT").unwrap().last, frozen.last);
        app.activate_chart_slot(1);
        let texts = demo_frame(&mut app, &ctx, vec![]);
        assert!(
            texts.iter().any(|(text, rect)| rect.left() >= 1100.0
                && text.starts_with(&format!("{:.2}", frozen.last))),
            "a paused slot must retain its own quote even when the list holds a newer quote"
        );
        app.activate_chart_slot(0);
        app.live.set_paused(true);
        let now = app.live.now();
        app.handle_layout_action(LayoutAction::CreateNew);
        app.switch_symbol("EURUSD");
        app.watchlist = WatchlistPanel::new(vec![Watchlist::from_text("Other", "AAPL")]);
        app.watchlist.set_service(app.quotes.clone());
        super::super::tests::finish_feed(&app.live);
        let before = app.live.quote(app.live.now()).unwrap();
        for _ in 0..4 {
            demo_frame(&mut app, &ctx, vec![]);
        }
        let texts = demo_frame(&mut app, &ctx, vec![]);
        assert_eq!(
            app.watchlist.quote("EURUSD"),
            app.live.quote(app.live.now()).as_ref()
        );
        assert!(
            texts.iter().any(|(text, rect)| rect.left() >= 1100.0
                && text.starts_with(&format!("{:.4}", before.last))),
            "details must quote an unlisted chart symbol: {texts:?}"
        );
        app.live.set_paused(false);
        let after = (1..=40)
            .find_map(|step| {
                app.live.advance(now + TimeDelta::milliseconds(step * 100));
                let quote = app.live.quote(app.live.now()).unwrap();
                (quote.last != before.last).then_some(quote)
            })
            .expect("the unlisted symbol receives another execution price");
        let texts = demo_frame(&mut app, &ctx, vec![]);
        assert!(
            texts.iter().any(|(text, rect)| rect.left() >= 1100.0
                && text.starts_with(&format!("{:.4}", after.last)))
        );
        app.screener.refresh();
        app.screener.poll(&ctx);
        let sample = app.live.market_quote("SAMPLE", app.live.now()).unwrap();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            app.screener.show(ui);
        });
        assert!(
            output.shapes.iter().any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == format!("{:.2}", sample.last))),
            "the screener must follow the selected layout's market clock"
        );
        output.drop_without_applying_deltas();
    }

    #[test]
    fn market_panel_prices_do_not_change_when_the_symbol_opens() {
        for at in [
            "2026-10-23T14:59:59Z",
            "2026-10-23T13:00:00Z",
            "2026-10-23T15:00:00Z",
            "2026-10-23T22:00:00Z",
        ] {
            let start = at.parse().unwrap();
            let live = Arc::new(LiveFeed::new("SAMPLE", start));
            super::super::tests::finish_feed(&live);
            let now = start + TimeDelta::seconds(2);
            live.advance(now);
            live.set_view(egui_charts::ui::timeframe_toolbar::SessionType::RTH, false);
            for symbol in ["MSFT", "BTCUSD"] {
                if symbol == "MSFT" {
                    assert!(live.market_quote(symbol, now).is_none());
                    assert!(
                        live.loading(),
                        "panel histories must load in scheduled batches"
                    );
                }
                super::super::tests::finish_feed(&live);
                let before = live.market_quote(symbol, now).unwrap();
                live.set_symbol(symbol);
                let after = live.quote(now).unwrap();
                assert_eq!(
                    (before.last, before.prev_close, before.extended),
                    (after.last, after.prev_close, after.extended),
                    "{symbol} at {at}"
                );
                let bars = live.snapshot(
                    egui_charts::model::Timeframe::Min1,
                    egui_charts::model::BarClock::new(egui_charts::model::Timeframe::Min1),
                    false,
                );
                assert_eq!(
                    bars.last().unwrap().close,
                    after.last,
                    "the regular chart close must price the panel"
                );
                let ctx = egui::Context::default();
                let mut panel = egui_charts::ui::widget_bar::ScreenerPanel::new(screener_sets());
                panel.set_feed(live.clone());
                let mut displayed = false;
                for _ in 0..2 {
                    panel.poll(&ctx);
                    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                        panel.show(ui);
                    });
                    displayed |= output.shapes.iter().any(|shape| matches!(&shape.shape,
                        egui::Shape::Text(text) if text.galley.text() == format!("{:.2}", after.last)));
                    output.drop_without_applying_deltas();
                }
                assert!(displayed, "the screener must share {symbol}'s price basis");
                if symbol == "MSFT" {
                    assert_eq!(
                        after.extended.is_some(),
                        at == "2026-10-23T13:00:00Z" || at == "2026-10-23T22:00:00Z"
                    );
                }
            }
        }
    }

    #[test]
    fn paper_orders_fill_after_switching_away_and_stay_frozen_during_replay() {
        use egui_charts::model::{OrderRequest, OrderSide, OrderType};
        use egui_charts::ui::widget_bar::{Watchlist, WatchlistPanel};
        let ctx = egui::Context::default();
        let now = "2026-10-23T15:00:00Z".parse().unwrap();
        let mut app = super::super::DemoApp::new_at(
            &eframe::CreationContext::_new_kittest(ctx.clone()),
            now,
            true,
        );
        app.switch_symbol("BTCUSD");
        app.trading.set_service(app.paper.clone());
        app.trading.display.instant_orders = true;
        demo_frame(&mut app, &ctx, vec![]);
        let price = app.live.quote(now).unwrap().last;
        app.paper.set_price("BTCUSD", price);
        let reference = LiveFeed::new("BTCUSD", now);
        super::super::tests::finish_feed(&reference);
        reference.set_paused(false);
        let (at, trigger) = (1..=40)
            .find_map(|step| {
                let at = now + TimeDelta::milliseconds(step * 100);
                reference.advance(at);
                let next = reference.quote(at)?.last;
                (next != price).then_some((at, next))
            })
            .expect("the execution price moves");
        let kind = if trigger > price {
            OrderType::Stop(trigger)
        } else {
            OrderType::Limit(trigger)
        };
        app.trading
            .open_ticket(OrderRequest::new("BTCUSD", OrderSide::Buy, 1.0, kind));
        let mut texts = Vec::new();
        for _ in 0..3 {
            texts = demo_frame(&mut app, &ctx, vec![]);
        }
        let point = texts
            .iter()
            .find(|(text, _)| {
                text.contains("BTCUSD @") && (text.contains("STOP") || text.contains("LIMIT"))
            })
            .expect("the pending order submit button")
            .1
            .center();
        let click = [true, false].map(|pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        });
        demo_frame(&mut app, &ctx, vec![egui::Event::PointerMoved(point)]);
        demo_frame(&mut app, &ctx, click.to_vec());
        demo_frame(&mut app, &ctx, vec![]);
        assert!(
            !app.toasts
                .toasts()
                .iter()
                .any(|toast| toast.message.starts_with("Filled:"))
        );
        app.switch_symbol("MSFT");
        super::super::tests::finish_feed(&app.live);
        app.watchlist = WatchlistPanel::new(vec![Watchlist::from_text("Other", "AAPL")]);
        app.watchlist.set_service(app.quotes.clone());
        app.live.set_paused(false);
        super::super::tests::finish_feed(&app.live);
        app.live.advance(at);
        demo_frame(&mut app, &ctx, vec![]);
        assert!(
            app.toasts
                .toasts()
                .iter()
                .any(|toast| toast.message.starts_with("Filled: Buy 1 BTCUSD")),
            "the off-chart order must fill: {:?}",
            app.toasts.toasts()
        );
        let (next_time, next_price) = (1..=40)
            .find_map(|step| {
                let time = at + TimeDelta::milliseconds(step * 100);
                reference.advance(time);
                let next = reference.quote(time)?.last;
                (next != trigger).then_some((time, next))
            })
            .expect("the position price moves again");
        super::super::tests::finish_feed(&app.live);
        app.live.advance(next_time);
        demo_frame(&mut app, &ctx, vec![]);
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            let _ = app.trading.show_account_panel(ui, "MSFT");
        });
        // One unit bought at the trigger has unrealized P&L = current price - trigger.
        let pnl = format!("{:+.2}", next_price - trigger);
        let texts: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.text().replace(',', "")),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        assert!(
            texts.contains(&pnl),
            "the off-chart position must be revalued to {pnl}: {texts:?}"
        );
        // A known replay close must survive the catalog's unrelated live price sample.
        app.chart.set_data(
            egui_charts::widget::SeriesId::MAIN,
            [100.0, 80.0, 110.0]
                .into_iter()
                .enumerate()
                .map(|(i, price)| {
                    Bar::new(
                        now - TimeDelta::minutes(10 - i as i64),
                        price,
                        price,
                        price,
                        price,
                        1.0,
                    )
                })
                .collect(),
        );
        app.start_replay();
        assert!(app.replay.is_some());
        app.feed_market_panels(&ctx);
        app.trading.poll(&ctx, &mut app.toasts);
        for _ in 0..2 {
            app.trading.trade("MSFT".into(), OrderSide::Buy);
            app.trading.poll(&ctx, &mut app.toasts);
        }
        assert!(
            app.toasts
                .toasts()
                .iter()
                .any(|toast| toast.message == "Filled: Buy 1 MSFT at 100.00"),
            "the broker must keep the replay price: {:?}",
            app.toasts.toasts()
        );
    }

    #[test]
    fn history_range_selects_the_newest_bars_inside_both_bounds() {
        let time = |minute| Utc.with_ymd_and_hms(2026, 10, 23, 12, minute, 0).unwrap();
        let bars: Vec<_> = (0..6)
            .map(|minute| Bar::new(time(minute), 1.0, 1.0, 1.0, 1.0, 1.0))
            .collect();
        // [02, 05) contains 02, 03, 04; its newest two are 03, 04.
        for (since, before, count, expected, end_of_history) in [
            (Some(2), Some(5), 10, vec![2, 3, 4], false),
            (Some(2), Some(5), 2, vec![3, 4], false),
            (Some(6), None, 10, vec![], false),
            (Some(5), Some(2), 10, vec![], false),
            (None, Some(0), 10, vec![], true),
            (None, None, 10, vec![0, 1, 2, 3, 4, 5], true),
            (Some(2), Some(5), 0, vec![], false),
        ] {
            let page = history_page(&bars, since.map(time), before.map(time), count);
            assert_eq!(
                page.bars.iter().map(|bar| bar.time).collect::<Vec<_>>(),
                expected.into_iter().map(time).collect::<Vec<_>>(),
                "since={since:?}, before={before:?}, count={count}"
            );
            assert_eq!(
                page.end_of_history, end_of_history,
                "since={since:?}, before={before:?}, count={count}"
            );
        }
    }
}
