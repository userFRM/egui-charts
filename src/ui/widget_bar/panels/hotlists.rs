//! Hotlists the feed supplies: the market's top movers by the measures the host names.

use crate::localization as l10n;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Offset, Utc};
use egui::{RichText, Sense, Ui, vec2};

use super::ROW_HEIGHT;
use super::controls::{self, cell, row};
use super::symbol_details::zone_label;
use super::{format_price, price_decimals};
use crate::data::inbox::Inbox;
use crate::data::{Datafeed, FeedError, HotlistEntry, HotlistQuery};
use crate::icons::icons;
use crate::model::TimezoneId;
use crate::theme::ThemeExt;
use crate::ui::trading::grouped;
use crate::ui_kit::buttons::ButtonSize;
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
    /// Add the symbol to the watchlist at this index of the names given to
    /// [`HotlistsPanel::set_watchlists`], such as with
    /// [`WatchlistPanel::add_symbol_to`](crate::ui::widget_bar::WatchlistPanel::add_symbol_to).
    AddToWatchlist {
        /// Index into the names given to [`HotlistsPanel::set_watchlists`].
        list: usize,
        /// Instrument identifier used by the host.
        symbol: String,
    },
    /// The user starred or unstarred a list. These are the names of the starred lists in the order
    /// given to [`HotlistsPanel::new`], for the host to keep and give back with
    /// [`HotlistsPanel::set_starred`].
    Starred(Vec<String>),
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
    /// The lists' names, each with whether it is starred.
    lists: Vec<(String, bool)>,
    list: usize,
    watchlists: Vec<String>,
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
            .field("watchlists", &self.watchlists)
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
            lists: lists.into_iter().map(|name| (name, false)).collect(),
            list: 0,
            watchlists: Vec::new(),
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

    /// Offers adding a row's symbol to these watchlists, such as the names of
    /// [`WatchlistPanel::lists`](crate::ui::widget_bar::WatchlistPanel::lists), in the row's menu.
    /// The choice returns as [`HotlistsAction::AddToWatchlist`]. Without names rows have no menu.
    pub fn set_watchlists(&mut self, names: impl IntoIterator<Item = impl Into<String>>) {
        self.watchlists = names.into_iter().map(Into::into).collect();
    }

    /// Stars the lists named in `lists`, such as the names kept from [`HotlistsAction::Starred`],
    /// and unstars the others; names that are not lists are ignored. Starred lists come first in
    /// the list menu, each group in the order given to [`new`](Self::new). The shown list stays.
    pub fn set_starred(&mut self, lists: impl IntoIterator<Item = String>) {
        let starred: HashSet<String> = lists.into_iter().collect();
        for (name, star) in &mut self.lists {
            *star = starred.contains(name);
        }
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
        let (Some(feed), Some((list, _))) = (&self.feed, self.lists.get(self.list)) else {
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
        // Starred lists first, each group in the given order.
        let mut order: Vec<usize> = (0..self.lists.len()).collect();
        order.sort_by_key(|&index| !self.lists[index].1);
        let mut starred = None;
        ui.horizontal_wrapped(|ui| {
            if let Some((name, _)) = self.lists.get(self.list) {
                controls::dropdown(ui, "hotlist", name, |ui| {
                    for index in order {
                        let (name, star) = &self.lists[index];
                        ui.horizontal(|ui| {
                            if IconButton::new(if *star {
                                &icons::STAR_FILLED
                            } else {
                                &icons::STAR_EMPTY
                            })
                            .size(ButtonSize::SM)
                            .tooltip(if *star {
                                "Remove from favorites"
                            } else {
                                "Add to favorites"
                            })
                            .show(ui)
                            .clicked()
                            {
                                starred = Some(index);
                            }
                            ui.selectable_value(&mut self.list, index, l10n::text(ui.ctx(), name));
                        });
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
        if let Some(index) = starred {
            self.lists[index].1 = !self.lists[index].1;
            let names = self.lists.iter().filter(|(_, star)| *star);
            action = HotlistsAction::Starred(names.map(|(name, _)| name.clone()).collect());
        }
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
                    format!("{}%", grouped(entry.change_percent, 2, true)),
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
        // Value columns size to their text, with the figure yielding room for the symbol heading.
        let font = egui::TextStyle::Body.resolve(ui.style());
        let symbol_width = ui
            .painter()
            .layout_no_wrap(l10n::text(ui.ctx(), "Symbol"), font.clone(), theme.ui.text)
            .size()
            .x
            .ceil()
            + 5.0;
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
                    let values_left = (symbol_left + symbol_width).min(right);
                    let figure_width = (right - values_left - widths[0] - widths[1]).max(0.0);
                    let mut cells = Vec::with_capacity(widths.len() + 1);
                    for (index, &width) in widths.iter().enumerate().rev() {
                        let width = if index == 2 {
                            width.min(figure_width)
                        } else {
                            width
                        };
                        let left = (right - width).max(values_left);
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
                    if !self.watchlists.is_empty() {
                        response.context_menu(|ui| {
                            let label = l10n::format_text(
                                ui.ctx(),
                                "Add {symbol} to watchlist",
                                &[("symbol", entry.symbol.as_str())],
                            );
                            ui.menu_button(label, |ui| {
                                for (list, name) in self.watchlists.iter().enumerate() {
                                    if ui.button(l10n::text(ui.ctx(), name)).clicked() {
                                        action = HotlistsAction::AddToWatchlist {
                                            list,
                                            symbol: entry.symbol.clone(),
                                        };
                                        ui.close();
                                    }
                                }
                            });
                        });
                    }
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

/// Size the whole title before the filter row decides whether to wrap the button.
fn dropdown(ui: &mut Ui, id: &str, text: &str, contents: impl FnOnce(&mut Ui)) {
    let button = ui.add(
        egui::Button::new((
            l10n::text(ui.ctx(), text),
            icons::CHEVRON_DOWN.as_image_square(16.0),
        ))
        .frame_when_inactive(false)
        .image_tint_follows_text_color(true)
        .min_size(vec2(0.0, 28.0))
        .wrap_mode(egui::TextWrapMode::Extend),
    );
    egui::Popup::menu(&button)
        .id(ui.make_persistent_id(id))
        .show(|ui| {
            egui::ScrollArea::vertical()
                .max_height(ui.spacing().combo_height)
                .show(ui, contents);
        });
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

    /// A row's menu returns the chosen watchlist with the row's symbol. A star returns the starred
    /// lists in the given order and moves them first in the list menu; `set_starred` replaces the
    /// stars without an action and ignores names that are not lists.
    #[test]
    fn row_menu_adds_to_a_watchlist_and_stars_lead_the_list_menu() {
        let feed = Arc::new(Recorder::default());
        let mut panel = HotlistsPanel::new(vec!["Gainers".into(), "Losers".into()]);
        panel.set_watchlists(["Main", "Tech"]);
        panel.set_feed(feed.clone());
        let reply = feed.replies.lock().unwrap().pop().unwrap();
        reply.send(Ok(vec![HotlistEntry::new("AAA", 1.0, 1.0)]));
        let mut h = harness(panel);
        h.get_by_role_and_label(egui::Role::Button, "AAA")
            .click_secondary();
        h.run();
        press(&mut h, "Add AAA to watchlist");
        press(&mut h, "Tech");
        let added = HotlistsAction::AddToWatchlist {
            list: 1,
            symbol: "AAA".into(),
        };
        assert_eq!(h.state().1, [added]);
        let starred = |names: &[&str]| {
            HotlistsAction::Starred(names.iter().map(|name| name.to_string()).collect())
        };
        let top = |h: &Panel, label: &str| h.get_by_label(label).rect().top();
        press(&mut h, "Gainers");
        h.get_all_by_label("Add to favorites")
            .nth(1)
            .expect("a star for Losers")
            .click();
        h.run();
        assert_eq!(h.state().1.last(), Some(&starred(&["Losers"])));
        press(&mut h, "Gainers");
        assert!(top(&h, "Remove from favorites") < top(&h, "Add to favorites"));
        assert!(
            top(&h, "Losers") < top(&h, "Add to favorites"),
            "the starred list leads"
        );
        press(&mut h, "Add to favorites");
        assert_eq!(h.state().1.last(), Some(&starred(&["Gainers", "Losers"])));
        h.state_mut()
            .0
            .set_starred(["Losers", "Missing"].map(String::from));
        press(&mut h, "Gainers");
        assert!(top(&h, "Losers") < top(&h, "Add to favorites"));
        press(&mut h, "Add to favorites");
        assert_eq!(h.state().1.len(), 4, "set_starred returns no action");
        assert_eq!(h.state().1.last(), Some(&starred(&["Gainers", "Losers"])));
    }
}
