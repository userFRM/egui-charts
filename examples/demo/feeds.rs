//! Synthetic answers for the market panels, derived from the symbol catalog so every symbol gets
//! stable, distinct content. A host answers these from its own data vendors.

use chrono::{DateTime, Datelike, DurationRound, NaiveTime, TimeDelta, Utc, Weekday};
use egui_charts::data::{
    CalendarEvent, CalendarKind, CalendarQuery, FeedError, Importance, NewsQuery, NewsStory,
    ScreenerColumn, ScreenerColumnSet, ScreenerOp, ScreenerQuery, ScreenerRow,
};

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
    let symbols: Vec<&DemoSymbol> = match &query.symbol {
        Some(symbol) => DemoSymbol::catalog()
            .iter()
            .filter(|spec| spec.symbol.eq_ignore_ascii_case(symbol))
            .collect(),
        None => DemoSymbol::catalog().iter().collect(),
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
            "change_percent" => (quote.last - quote.prev_close) / quote.prev_close * 100.0,
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
            let decimals = if spec.start_price < 10.0 { 4 } else { 2 };
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
