//! Hotlists the feed supplies: the market's top movers by the measures the host names.

use crate::localization as l10n;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Offset, Utc};
use egui::{RichText, Sense, Ui, vec2};

use super::controls::{cell, dropdown, row};
use super::symbol_details::zone_label;
use super::{ROW_HEIGHT, signed};
use super::{format_price, price_decimals};
use crate::data::inbox::Inbox;
use crate::data::{Datafeed, FeedError, HotlistEntry, HotlistQuery};
use crate::icons::icons;
use crate::model::TimezoneId;
use crate::theme::ThemeExt;
use crate::ui_kit::status_message::{MessageKind, status_message, translated_status_message};
use crate::ui_kit::{EmptyState, IconButton, LoadingIndicator, PanelHeader};

/// Symbols asked for at most.
const LIMIT: usize = 50;

/// An action the host performs for the hotlists panel.
#[derive(Clone, Debug, PartialEq, Default)]
#[non_exhaustive]
pub enum HotlistsAction {
    /// No host action.
    #[default]
    None,
    /// Show this symbol, such as on the chart.
    SelectSymbol(String),
}

/// Hotlists fed by the host's [`Datafeed::hotlist`], one at a time.
///
/// The host names the lists, such as top gainers, top losers and most active; the user picks
/// one, each pick asks the feed, and only the answer to the latest request is shown. The panel asks
/// again on its Refresh button and, with [`set_refresh_interval`](Self::set_refresh_interval), on
/// a timer while it is shown, keeping the rows until the answer and naming the time they were
/// received. Call [`poll`](Self::poll) each frame, then [`show`](Self::show) while the panel is
/// open.
pub struct HotlistsPanel {
    lists: Vec<String>,
    list: usize,
    feed: Option<Arc<dyn Datafeed>>,
    entries: Vec<HotlistEntry>,
    error: Option<FeedError>,
    loading: bool,
    inbox: Arc<Inbox>,
    request: u64,
    exchanges: Vec<(String, bool)>,
    countries: Vec<(String, bool)>,
    interval: Option<Duration>,
    timezone: TimezoneId,
    /// egui time of the answer on screen, from which the next ask is timed.
    answered_at: Option<f64>,
    /// When the rows on screen were received.
    as_of: Option<DateTime<Utc>>,
}

impl std::fmt::Debug for HotlistsPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HotlistsPanel")
            .field("lists", &self.lists)
            .field("list", &self.list)
            .field("error", &self.error)
            .field("loading", &self.loading)
            .field("request", &self.request)
            .field("exchanges", &self.exchanges)
            .field("countries", &self.countries)
            .field("interval", &self.interval)
            .field("timezone", &self.timezone)
            .field("as_of", &self.as_of)
            .finish_non_exhaustive()
    }
}

impl HotlistsPanel {
    /// A panel of the hotlists named `lists`, showing the first, without a feed.
    pub fn new(lists: Vec<String>) -> Self {
        Self {
            lists,
            list: 0,
            feed: None,
            entries: Vec::new(),
            error: None,
            loading: false,
            inbox: Arc::default(),
            request: 0,
            exchanges: Vec::new(),
            countries: Vec::new(),
            interval: None,
            timezone: TimezoneId::default(),
            answered_at: None,
            as_of: None,
        }
    }

    /// Ask the feed again `interval` after each answer while the panel is shown, such as every
    /// minute; with `None`, the default, only when the host or the user asks. A zero interval
    /// counts as `None`. The rows stay on screen until the new answer arrives.
    pub fn set_refresh_interval(&mut self, interval: Option<Duration>) {
        self.interval = interval.filter(|interval| !interval.is_zero());
    }

    /// Shows the time of the answer in `timezone`, such as the chart's. UTC by default.
    pub fn set_timezone(&mut self, timezone: TimezoneId) {
        self.timezone = timezone;
    }

    /// Offer `exchanges`, in the codes the feed uses, as a filter kept across lists. None is
    /// selected at first, which lists every exchange. Asks the feed again.
    pub fn set_exchanges(&mut self, exchanges: impl IntoIterator<Item = String>) {
        self.exchanges = exchanges.into_iter().map(|code| (code, false)).collect();
        self.reload();
    }

    /// Offer `countries`, in the codes the feed uses, as a filter kept across lists. None is
    /// selected at first, which lists every country. Asks the feed again.
    pub fn set_countries(&mut self, countries: impl IntoIterator<Item = String>) {
        self.countries = countries.into_iter().map(|code| (code, false)).collect();
        self.reload();
    }

    /// Ask `feed` for the lists from now on, starting at once. The rows of the previous feed are
    /// cleared.
    pub fn set_feed(&mut self, feed: Arc<dyn Datafeed>) {
        self.feed = Some(feed);
        self.reload();
    }

    /// Ask the feed again, such as to update the figures. The rows stay on screen until the
    /// answer.
    pub fn refresh(&mut self) {
        let (Some(feed), Some(list)) = (&self.feed, self.lists.get(self.list)) else {
            return;
        };
        self.request += 1;
        self.loading = true;
        let mut query = HotlistQuery::new(list.clone(), LIMIT);
        query.exchanges = chosen(&self.exchanges);
        query.countries = chosen(&self.countries);
        feed.hotlist(query, self.inbox.reply(self.request));
    }

    /// Ask for another list, market or feed, whose rows the ones on screen are not.
    fn reload(&mut self) {
        self.entries.clear();
        self.error = None;
        self.as_of = None;
        self.refresh();
    }

    /// Receive the feed's answer and wake the UI when the next one arrives.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        let now = ctx.input(|i| i.time);
        for (key, answer) in self.inbox.drain().answers {
            if key != self.request {
                continue;
            }
            self.loading = false;
            self.answered_at = Some(now);
            match *answer
                .downcast::<Result<Vec<HotlistEntry>, FeedError>>()
                .expect("hotlist")
            {
                Ok(entries) => {
                    self.entries = entries;
                    self.error = None;
                    self.as_of = Some(Utc::now());
                }
                Err(error) => {
                    self.entries.clear();
                    self.error = Some(error);
                    self.as_of = None;
                }
            }
        }
    }

    /// Render the chosen list after [`poll`](Self::poll).
    pub fn show(&mut self, ui: &mut Ui) -> HotlistsAction {
        let mut action = HotlistsAction::None;
        let theme = ui.active_theme();
        let mut refresh = false;
        PanelHeader::new("Hotlists").show(ui, |ui| {
            refresh = IconButton::new(&icons::ROTATE_CCW)
                .tooltip("Refresh")
                .enabled(self.feed.is_some())
                .show(ui)
                .clicked();
        });
        let before = (self.list, self.exchanges.clone(), self.countries.clone());
        ui.horizontal_wrapped(|ui| {
            if let Some(name) = self.lists.get(self.list) {
                dropdown(ui, "hotlist", name, |ui| {
                    for (index, name) in self.lists.iter().enumerate() {
                        ui.selectable_value(&mut self.list, index, l10n::text(ui.ctx(), name));
                    }
                });
            }
            choices(
                ui,
                "hotlist_exchanges",
                "All exchanges",
                "{count} exchanges",
                &mut self.exchanges,
            );
            choices(
                ui,
                "hotlist_countries",
                "All countries",
                "{count} countries",
                &mut self.countries,
            );
        });
        if before != (self.list, self.exchanges.clone(), self.countries.clone()) {
            self.reload();
        } else if refresh {
            self.refresh();
        } else if let (Some(interval), Some(at)) = (self.interval, self.answered_at)
            && !self.loading
            && !matches!(self.error, Some(FeedError::NotSupported))
        {
            let now = ui.input(|i| i.time);
            // A clock behind the answer, or NaN, waits a full interval.
            let elapsed = Duration::try_from_secs_f64((now - at).max(0.0)).unwrap_or(Duration::MAX);
            let wait = interval.saturating_sub(elapsed);
            if wait.is_zero() {
                self.refresh();
            } else {
                ui.ctx().request_repaint_after(wait);
            }
        }
        if let Some(time) = self.as_of {
            let local = time.with_timezone(&self.timezone);
            let text = l10n::format_text(
                ui.ctx(),
                "Last update at {time} {zone}",
                &[
                    ("time", local.format("%H:%M:%S").to_string().as_str()),
                    (
                        "zone",
                        zone_label(local.offset().fix().local_minus_utc()).as_str(),
                    ),
                ],
            );
            ui.label(
                RichText::new(text)
                    .size(theme.text.small)
                    .color(theme.ui.text_secondary),
            );
        }
        match &self.error {
            Some(FeedError::NotSupported) => {
                status_message(ui, MessageKind::Info, "This feed supplies no hotlists.");
            }
            Some(error) => {
                translated_status_message(
                    ui,
                    MessageKind::Error,
                    l10n::format_text(
                        ui.ctx(),
                        "Hotlist unavailable: {error}",
                        &[("error", error.to_string().as_str())],
                    ),
                );
            }
            None if self.loading && self.entries.is_empty() => {
                LoadingIndicator::new().message("Loading hotlist").show(ui);
            }
            None if self.entries.is_empty() => {
                EmptyState::new("No symbols in this list")
                    .icon(&icons::WIDGET_BAR_HOTLISTS)
                    .fill_height()
                    .show(ui);
            }
            None => {}
        }
        let heading = self
            .entries
            .iter()
            .find_map(|entry| entry.figure.as_ref().map(|(heading, _)| heading.clone()));
        let mut headings = vec!["Symbol".to_owned(), "Last".to_owned(), "Chg%".to_owned()];
        headings.extend(heading.clone());
        let rows: Vec<Vec<String>> = self
            .entries
            .iter()
            .map(|entry| {
                let decimals = entry
                    .price_decimals
                    .unwrap_or_else(|| price_decimals(entry.last));
                let mut texts = vec![
                    entry.symbol.clone(),
                    format_price(entry.last, Some(decimals)),
                    format!("{}%", signed(entry.change_percent, 2)),
                ];
                if heading.is_some() {
                    texts.push(
                        entry
                            .figure
                            .as_ref()
                            .map_or_else(|| "-".to_owned(), |(_, value)| value.clone()),
                    );
                }
                texts
            })
            .collect();
        // Each value column is as wide as its widest text, up to 96 px, so four fit a narrow panel.
        let font = egui::TextStyle::Body.resolve(ui.style());
        let widths: Vec<f32> = (1..headings.len())
            .map(|column| {
                std::iter::once(&headings[column])
                    .chain(rows.iter().map(|texts| &texts[column]))
                    .map(|text| {
                        let text = l10n::text(ui.ctx(), text.as_str());
                        let galley = ui
                            .painter()
                            .layout_no_wrap(text, font.clone(), theme.ui.text);
                        galley.size().x
                    })
                    .fold(0.0, f32::max)
                    .ceil()
                    .min(91.0)
                    + 5.0
            })
            .collect();
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let columns = |rect: egui::Rect| {
                    let symbol_left = rect.left() + 10.0;
                    let mut right = rect.right() - 8.0;
                    let mut cells = Vec::with_capacity(widths.len() + 1);
                    for width in widths.iter().rev() {
                        let left = (right - width).max(symbol_left);
                        cells.push(egui::Rect::from_min_max(
                            egui::pos2(left, rect.top()),
                            egui::pos2((right - 4.0).max(left), rect.bottom()),
                        ));
                        right = left;
                    }
                    cells.push(egui::Rect::from_min_max(
                        egui::pos2(symbol_left, rect.top()),
                        egui::pos2((right - 4.0).max(symbol_left), rect.bottom()),
                    ));
                    cells.reverse();
                    cells
                };
                let (header, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), ROW_HEIGHT), Sense::hover());
                for (index, (title, column)) in headings.iter().zip(columns(header)).enumerate() {
                    cell(ui, column, title, index != 0, theme.ui.text_secondary);
                }
                for (entry, texts) in self.entries.iter().zip(&rows) {
                    let color = if entry.change_percent > 0.0 {
                        theme.chart.candle_up
                    } else if entry.change_percent < 0.0 {
                        theme.chart.candle_down
                    } else {
                        theme.ui.text
                    };
                    let (rect, response) = row(ui);
                    for (index, (text, column)) in texts.iter().zip(columns(rect)).enumerate() {
                        let color = if index == 2 { color } else { theme.ui.text };
                        cell(ui, column, text, index != 0, color);
                    }
                    let translated_label = l10n::text(ui.ctx(), &entry.symbol);
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::Button, true, &translated_label)
                    });
                    if response.clicked() {
                        action = HotlistsAction::SelectSymbol(entry.symbol.clone());
                    }
                }
            });
        action
    }
}

/// The codes chosen in `choices`, in the order offered.
fn chosen(choices: &[(String, bool)]) -> Vec<String> {
    choices
        .iter()
        .filter(|(_, on)| *on)
        .map(|(code, _)| code.clone())
        .collect()
}

/// A dropdown of checkboxes, read `all` with none chosen, the code with one and `many` with more.
fn choices(ui: &mut Ui, id: &str, all: &str, many: &str, choices: &mut [(String, bool)]) {
    if choices.is_empty() {
        return;
    }
    let chosen = chosen(choices);
    let text = match chosen.as_slice() {
        [] => all.to_owned(),
        [code] => code.clone(),
        _ => l10n::format_text(
            ui.ctx(),
            many,
            &[("count", chosen.len().to_string().as_str())],
        ),
    };
    dropdown(ui, id, &text, |ui| {
        for (code, on) in choices.iter_mut() {
            ui.checkbox(on, l10n::text(ui.ctx(), code.as_str()));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{HistoryPage, HistoryRequest, Reply};
    use egui_kittest::{Harness, kittest::Queryable};
    use std::sync::Mutex;

    /// A feed that keeps each query and its reply, answered when the test chooses.
    #[derive(Default)]
    struct Recorder {
        queries: Mutex<Vec<HotlistQuery>>,
        replies: Mutex<Vec<Reply<Vec<HotlistEntry>>>>,
    }

    impl Datafeed for Recorder {
        fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
            reply.send(Err(FeedError::NotSupported));
        }
        fn hotlist(&self, query: HotlistQuery, reply: Reply<Vec<HotlistEntry>>) {
            self.queries.lock().unwrap().push(query);
            self.replies.lock().unwrap().push(reply);
        }
    }

    /// The panel, polled and shown each frame, and the actions it returned.
    type Panel = Harness<'static, (HotlistsPanel, Vec<HotlistsAction>)>;

    fn harness(panel: HotlistsPanel) -> Panel {
        Harness::builder()
            .allow_missing_glyphs()
            .with_size(vec2(300.0, 500.0))
            .build_ui_state(
                |ui, (panel, actions): &mut (HotlistsPanel, Vec<HotlistsAction>)| {
                    panel.poll(ui.ctx());
                    let action = panel.show(ui);
                    if action != HotlistsAction::None {
                        actions.push(action);
                    }
                },
                (panel, Vec::new()),
            )
    }

    /// Clicks the node labelled `label` and runs a frame past the click, so a menu it opens shows.
    fn press(h: &mut Panel, label: &str) {
        h.get_by_label(label).click();
        h.step();
        h.step();
    }

    /// With an interval of one minute the shown panel asks again a minute after the answer, never
    /// with a request open, keeps the rows and their time until the new answer, and asks on
    /// Refresh.
    #[test]
    fn asks_again_on_its_interval_and_keeps_rows_until_the_answer() {
        let feed = Arc::new(Recorder::default());
        let mut panel = HotlistsPanel::new(vec!["Gainers".into()]);
        panel.set_refresh_interval(Some(Duration::from_secs(60)));
        panel.set_feed(feed.clone());
        let mut h = harness(panel);
        let asked = || feed.queries.lock().unwrap().len();
        let answer = |symbol: &str| {
            let reply = feed.replies.lock().unwrap().pop().unwrap();
            reply.send(Ok(vec![HotlistEntry::new(symbol, 1.0, 1.0)]));
        };
        let at = |h: &mut Panel, time: f64| {
            h.input_mut().time = Some(time);
            h.step();
        };
        // Building the harness ran a few frames of 0.25 s, so the answer comes later.
        answer("AAA");
        at(&mut h, 10.0);
        h.get_by_label_contains("Last update at");
        at(&mut h, 69.9);
        assert_eq!(asked(), 1, "asked before the interval passed");
        at(&mut h, 70.0);
        assert_eq!(asked(), 2, "not asked when the interval passed");
        h.get_by_role_and_label(egui::Role::Button, "AAA");
        h.get_by_label_contains("Last update at");
        at(&mut h, 70.5);
        assert_eq!(asked(), 2, "asked again with a request open");
        answer("BBB");
        at(&mut h, 71.0);
        h.get_by_role_and_label(egui::Role::Button, "BBB");
        h.get_by_label("Refresh").click();
        h.step();
        assert_eq!(asked(), 3, "not asked on Refresh");
        h.get_by_role_and_label(egui::Role::Button, "BBB");
    }

    /// The exchanges and countries the user ticks reach the query, each tick asks again, and a
    /// list change keeps them.
    #[test]
    fn exchange_and_country_choices_reach_the_query_across_lists() {
        let feed = Arc::new(Recorder::default());
        let mut panel = HotlistsPanel::new(vec!["Gainers".into(), "Losers".into()]);
        panel.set_exchanges(["NASDAQ", "NYSE"].map(String::from));
        panel.set_countries(["US", "DE"].map(String::from));
        panel.set_feed(feed.clone());
        let reply = feed.replies.lock().unwrap().pop().unwrap();
        reply.send(Ok(vec![HotlistEntry::new("AAA", 1.0, 1.0)]));
        let mut h = harness(panel);
        press(&mut h, "All exchanges");
        press(&mut h, "NYSE");
        assert!(
            h.query_by_role_and_label(egui::Role::Button, "AAA")
                .is_none()
        );
        if h.query_by_role_and_label(egui::Role::CheckBox, "NASDAQ")
            .is_none()
        {
            press(&mut h, "NYSE");
        }
        press(&mut h, "NASDAQ");
        press(&mut h, "All countries");
        press(&mut h, "DE");
        if h.query_by_role_and_label(egui::Role::CheckBox, "US")
            .is_none()
        {
            press(&mut h, "DE");
        }
        press(&mut h, "US");
        press(&mut h, "Gainers");
        press(&mut h, "Losers");
        let queries = feed.queries.lock().unwrap();
        assert!(queries.iter().all(|query| query.limit == 50));
        let asked: Vec<_> = queries
            .iter()
            .map(|query| {
                let (exchanges, countries) = (query.exchanges.join(","), query.countries.join(","));
                format!("{}|{exchanges}|{countries}", query.list)
            })
            .collect();
        assert_eq!(
            asked,
            [
                "Gainers||",
                "Gainers|NYSE|",
                "Gainers|NASDAQ,NYSE|",
                "Gainers|NASDAQ,NYSE|DE",
                "Gainers|NASDAQ,NYSE|US,DE",
                "Losers|NASDAQ,NYSE|US,DE"
            ]
        );
    }
}
