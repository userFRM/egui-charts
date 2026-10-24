//! Synthetic answers for the market panels, derived from the symbol catalog so every symbol gets
//! stable, distinct content. A host answers these from its own data vendors.

use chrono::{DateTime, DurationRound, TimeDelta, Utc};
use egui_charts::data::{NewsQuery, NewsStory};

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
