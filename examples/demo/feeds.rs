//! Synthetic answers for the market panels, derived from the symbol catalog so every symbol gets
//! stable, distinct content. A host answers these from its own data vendors.

use chrono::{DateTime, Datelike, DurationRound, NaiveTime, TimeDelta, TimeZone, Utc, Weekday};
use std::collections::BTreeMap;
use std::sync::Mutex;

use egui_charts::data::{
    CalendarEvent, CalendarKind, CalendarQuery, FeedError, HotlistEntry, HotlistQuery, Importance,
    NewsQuery, NewsStory, Reply, ScreenerColumn, ScreenerColumnSet, ScreenerOp, ScreenerQuery,
    ScreenerRow,
};
use egui_charts::model::Bar;
use egui_charts::services::{ChatEvent, ChatMessage, Chats, Events, Notification, Notifications};

use egui_charts::ui::widget_bar::{PeriodReturn, Seasonality, SeasonalityMonth};

use crate::sampledata::DemoSymbol;

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

/// The catalog symbols that pass `query`, from their synthetic quotes at `now`.
pub(super) fn screen(
    query: &ScreenerQuery,
    now: DateTime<Utc>,
) -> Result<Vec<ScreenerRow>, FeedError> {
    let value = |spec: &DemoSymbol, column: &str| {
        let quote = super::live::watchlist_quote(spec.symbol, now)?;
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

/// The catalog symbols ranked for `query.list` on their synthetic quotes at `now`.
pub(super) fn hotlist(query: &HotlistQuery, now: DateTime<Utc>) -> Vec<HotlistEntry> {
    let mut entries: Vec<HotlistEntry> = DemoSymbol::catalog()
        .iter()
        .filter_map(|spec| {
            let quote = super::live::watchlist_quote(spec.symbol, now)?;
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
pub(super) struct DemoNotifications(Mutex<(Vec<Notification>, Option<Events<Notification>>)>);

impl DemoNotifications {
    /// Logs `title` about `symbol`, if any, and pushes it to the panel.
    pub(super) fn notify(&self, title: impl Into<String>, symbol: Option<&str>) {
        let mut entry = Notification::new(Utc::now(), title);
        entry.symbol = symbol.map(str::to_owned);
        self.publish(entry);
    }

    /// Keep supplied headlines quiet on repeated viewport requests.
    pub(super) fn notify_event(&self, symbol: &str, event: &egui_charts::model::TimeScaleEvent) {
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

/// Synthetic financials for the catalog's stocks: five fiscal years scaled from the catalog price,
/// one of them a loss, and dividends from the third year. Other symbols have none.
pub(super) fn financials(symbol: &str) -> Option<egui_charts::ui::financials::CompanyFinancials> {
    use egui_charts::ui::financials::{CompanyFinancials, FinancialPage};
    let spec = DemoSymbol::catalog()
        .iter()
        .find(|spec| spec.symbol.eq_ignore_ascii_case(symbol))
        .filter(|spec| !matches!(spec.exchange, "Crypto" | "Index" | "Forex" | "Synthetic"))?;
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
        ("1M", last.time - TimeDelta::days(30)),
        ("3M", last.time - TimeDelta::days(91)),
        ("6M", last.time - TimeDelta::days(182)),
        ("YTD", year_start),
        ("1Y", last.time - TimeDelta::days(365)),
    ]
    .into_iter()
    .map(|(period, since)| {
        let base = bars.iter().rev().find(|bar| bar.time <= since);
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
