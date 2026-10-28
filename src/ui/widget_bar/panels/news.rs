//! Headlines and highlights the feed supplies, for the chart's symbol, more symbols or the whole
//! market, narrowed by topic and provider.

use crate::localization as l10n;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use egui::{Label, RichText, Sense, Ui};

use crate::data::inbox::Inbox;
use crate::data::{Datafeed, FeedError, NewsQuery, NewsStory};
use crate::icons::icons;
use crate::theme::ThemeExt;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::status_message::{MessageKind, status_message};
use crate::ui_kit::{EmptyState, IconButton, LoadingIndicator, PanelHeader};

use super::controls::dropdown;

/// Stories asked for at most.
const LIMIT: usize = 50;

/// An action the host performs for the news panel.
#[derive(Clone, Debug, PartialEq, Default)]
#[non_exhaustive]
pub enum NewsAction {
    /// No host action.
    #[default]
    None,
    /// Show the chart at the time a story was published, such as with `Chart::scroll_to_time`.
    ShowOnChart(DateTime<Utc>),
    /// Open the full story at this link.
    OpenUrl(String),
}

/// News or highlights fed by the host's [`Datafeed::news`].
///
/// Keep it on the chart's symbol with [`set_symbol`](Self::set_symbol); the user switches between
/// that symbol's stories and the whole market's. With [`set_symbols`](Self::set_symbols) they can
/// add symbols, and with [`set_topics`](Self::set_topics) and [`set_providers`](Self::set_providers)
/// narrow the stories by topic and provider. Each change asks the feed again with the complete
/// selection, and only the answer to the latest request is shown. A story opens to its text, with actions to show the
/// chart at its time and to open its link. Call [`poll`](Self::poll) each frame, then
/// [`show`](Self::show) while the panel is open.
pub struct NewsPanel {
    highlights: bool,
    feed: Option<Arc<dyn Datafeed>>,
    symbol: Option<String>,
    market: bool,
    more_symbols: Vec<(String, bool)>,
    topics: Vec<(String, bool)>,
    providers: Vec<(String, bool)>,
    stories: Vec<NewsStory>,
    open: Option<String>,
    error: Option<FeedError>,
    loading: bool,
    inbox: Arc<Inbox>,
    request: u64,
}

impl Default for NewsPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl NewsPanel {
    /// A headlines panel without a feed.
    pub fn new() -> Self {
        Self {
            highlights: false,
            feed: None,
            symbol: None,
            market: false,
            more_symbols: Vec::new(),
            topics: Vec::new(),
            providers: Vec::new(),
            stories: Vec::new(),
            open: None,
            error: None,
            loading: false,
            inbox: Arc::default(),
            request: 0,
        }
    }

    /// A highlights panel without a feed: summaries shown under their headlines.
    pub fn highlights() -> Self {
        Self {
            highlights: true,
            ..Self::new()
        }
    }

    /// Ask `feed` for stories from now on, starting at once.
    pub fn set_feed(&mut self, feed: Arc<dyn Datafeed>) {
        self.feed = Some(feed);
        self.refresh();
    }

    /// Follow `symbol`, such as the chart's. A new symbol asks again while its stories are shown.
    pub fn set_symbol(&mut self, symbol: &str) {
        if self.symbol.as_deref() != Some(symbol) {
            self.symbol = Some(symbol.to_owned());
            if !self.market {
                self.refresh();
            }
        }
    }

    /// Offer `symbols` as further symbols to show stories about, next to the followed one. None is
    /// selected at first; choices still offered stay selected.
    pub fn set_symbols(&mut self, symbols: impl IntoIterator<Item = String>) {
        offer(&mut self.more_symbols, symbols);
        self.refresh();
    }

    /// Offer `topics`, in the values the feed supports, as a filter. None is selected at first,
    /// which lists every topic; choices still offered stay selected.
    pub fn set_topics(&mut self, topics: impl IntoIterator<Item = String>) {
        offer(&mut self.topics, topics);
        self.refresh();
    }

    /// Offer `providers`, in the identifiers the feed uses, as a filter. None is selected at
    /// first, which lists every provider; choices still offered stay selected.
    pub fn set_providers(&mut self, providers: impl IntoIterator<Item = String>) {
        offer(&mut self.providers, providers);
        self.refresh();
    }

    /// Ask the feed again, such as when the host learns of new stories.
    pub fn refresh(&mut self) {
        let Some(feed) = &self.feed else {
            return;
        };
        self.request += 1;
        self.loading = true;
        let symbols = if self.market {
            Vec::new()
        } else {
            self.symbol
                .iter()
                .chain(chosen(&self.more_symbols))
                .cloned()
                .collect()
        };
        let query = NewsQuery {
            symbols,
            topics: chosen(&self.topics).cloned().collect(),
            providers: chosen(&self.providers).cloned().collect(),
            highlights: self.highlights,
            limit: LIMIT,
        };
        feed.news(query, self.inbox.reply(self.request));
    }

    /// Receive the feed's answer and wake the UI when the next one arrives.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        for (key, answer) in self.inbox.drain().answers {
            if key != self.request {
                continue;
            }
            self.loading = false;
            match *answer
                .downcast::<Result<Vec<NewsStory>, FeedError>>()
                .expect("news")
            {
                Ok(stories) => {
                    self.stories = stories;
                    self.error = None;
                    if self
                        .open
                        .as_ref()
                        .is_some_and(|id| !self.stories.iter().any(|story| story.id == *id))
                    {
                        self.open = None;
                    }
                }
                Err(error) => {
                    self.stories.clear();
                    self.open = None;
                    self.error = Some(error);
                }
            }
        }
    }

    /// Render the stories, or the open story, after [`poll`](Self::poll).
    pub fn show(&mut self, ui: &mut Ui) -> NewsAction {
        let mut action = NewsAction::None;
        let theme = ui.active_theme();
        let title = if self.highlights {
            "Highlights"
        } else {
            "News"
        };
        if let Some(story) = self
            .open
            .as_ref()
            .and_then(|id| self.stories.iter().find(|story| story.id == *id))
        {
            let mut back = false;
            PanelHeader::new(title).show(ui, |ui| {
                back = IconButton::new(&icons::CHEVRON_LEFT)
                    .tooltip("Back to the stories")
                    .show(ui)
                    .clicked();
            });
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(l10n::text(ui.ctx(), &story.headline))
                            .size(theme.text.heading)
                            .strong(),
                    );
                    ui.label(
                        RichText::new(l10n::text(ui.ctx(), meta(story)))
                            .size(theme.text.small)
                            .color(theme.ui.text_secondary),
                    );
                    if !story.symbols.is_empty() {
                        ui.label(
                            RichText::new(l10n::text(ui.ctx(), story.symbols.join(", ")))
                                .size(theme.text.small)
                                .color(theme.ui.text_secondary),
                        );
                    }
                    ui.add_space(theme.spacing.sm);
                    if !story.body.is_empty() {
                        ui.label(l10n::text(ui.ctx(), &story.body));
                        ui.add_space(theme.spacing.sm);
                    }
                    ui.horizontal_wrapped(|ui| {
                        if Button::new("Show on chart").show(ui).clicked() {
                            action = NewsAction::ShowOnChart(story.time);
                        }
                        if let Some(url) = &story.url
                            && Button::new("Open story").show(ui).clicked()
                        {
                            action = NewsAction::OpenUrl(url.clone());
                        }
                    });
                });
            if back {
                self.open = None;
            }
            return action;
        }
        let mut refresh = false;
        PanelHeader::new(title).show(ui, |ui| {
            refresh = IconButton::new(&icons::ROTATE_CCW)
                .tooltip("Refresh")
                .enabled(self.feed.is_some())
                .show(ui)
                .clicked();
        });
        let market = self.market;
        ui.allocate_ui_with_layout(
            egui::vec2(ui.available_width(), 24.0),
            egui::Layout::right_to_left(egui::Align::Center),
            |ui| {
                if Button::new("Market")
                    .small()
                    .selected(self.market)
                    .show(ui)
                    .clicked()
                {
                    self.market = true;
                }
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    if let Some(symbol) = &self.symbol
                        && ui
                            .add(
                                egui::Button::new(l10n::text(ui.ctx(), symbol))
                                    .frame_when_inactive(!self.market)
                                    .stroke(egui::Stroke::NONE)
                                    .selected(!self.market)
                                    .min_size(egui::vec2(0.0, 24.0))
                                    .truncate(),
                            )
                            .on_hover_text(l10n::text(ui.ctx(), symbol))
                            .clicked()
                    {
                        self.market = false;
                    }
                });
            },
        );
        let before = (
            self.more_symbols.clone(),
            self.topics.clone(),
            self.providers.clone(),
        );
        ui.horizontal_wrapped(|ui| {
            for (id, noun, choices) in [
                ("news_symbols", "symbols", &mut self.more_symbols),
                ("news_topics", "topics", &mut self.topics),
                ("news_providers", "providers", &mut self.providers),
            ] {
                if choices.is_empty() || (id == "news_symbols" && self.market) {
                    continue;
                }
                let count = choices.iter().filter(|(_, on)| *on).count();
                let text = if count == 0 {
                    format!("All {noun}")
                } else {
                    format!("{count} {noun}")
                };
                dropdown(ui, id, &text, |ui| {
                    for (value, on) in choices.iter_mut() {
                        ui.checkbox(on, l10n::text(ui.ctx(), value.as_str()));
                    }
                });
            }
        });
        if refresh
            || market != self.market
            || before
                != (
                    self.more_symbols.clone(),
                    self.topics.clone(),
                    self.providers.clone(),
                )
        {
            self.refresh();
        }
        match &self.error {
            Some(FeedError::NotSupported) => {
                status_message(ui, MessageKind::Info, "This feed supplies no stories.");
            }
            Some(error) => {
                status_message(
                    ui,
                    MessageKind::Error,
                    &format!("Stories unavailable: {error}"),
                );
            }
            None if self.loading && self.stories.is_empty() => {
                LoadingIndicator::new().message("Loading stories").show(ui);
            }
            None if self.stories.is_empty() => {
                EmptyState::new("No stories")
                    .icon(&icons::WIDGET_BAR_NEWS)
                    .fill_height()
                    .show(ui);
            }
            None => {}
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for story in &self.stories {
                    let response = ui
                        .scope_builder(egui::UiBuilder::new().sense(Sense::click()), |ui| {
                            let hovered = ui.response().hovered();
                            egui::Frame::new()
                                .fill(if hovered {
                                    theme.ui.btn_bg_hover
                                } else {
                                    egui::Color32::TRANSPARENT
                                })
                                .inner_margin(egui::Margin::symmetric(8, 0))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width());
                                    ui.set_min_height(26.0);
                                    ui.style_mut().interaction.selectable_labels = false;
                                    ui.label(
                                        RichText::new(l10n::text(ui.ctx(), meta(story)))
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
                                    if self.highlights && !story.body.is_empty() {
                                        ui.label(l10n::text(ui.ctx(), &story.body));
                                    }
                                });
                        })
                        .response;
                    let translated_label = l10n::text(ui.ctx(), &story.headline);
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::Button, true, &translated_label)
                    });
                    if response
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                    {
                        self.open = Some(story.id.clone());
                    }
                    ui.separator();
                }
            });
        action
    }
}

/// Replace the offered values with `values`, keeping the choice of those still offered.
fn offer(choices: &mut Vec<(String, bool)>, values: impl IntoIterator<Item = String>) {
    let kept = std::mem::take(choices);
    *choices = values
        .into_iter()
        .map(|value| {
            let on = kept.iter().any(|(old, on)| *on && *old == value);
            (value, on)
        })
        .collect();
}

/// The values the user ticked.
fn chosen(choices: &[(String, bool)]) -> impl Iterator<Item = &String> {
    choices.iter().filter(|(_, on)| *on).map(|(value, _)| value)
}

/// The publication time in UTC and the source.
fn meta(story: &NewsStory) -> String {
    let time = story.time.format("%b %-d, %H:%M UTC").to_string();
    if story.source.is_empty() {
        time
    } else {
        format!("{time} · {}", story.source)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{HistoryPage, HistoryRequest, Reply};
    use std::sync::Mutex;

    #[derive(Default)]
    struct Feed(Mutex<Vec<(NewsQuery, Reply<Vec<NewsStory>>)>>);

    impl Datafeed for Feed {
        fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
            reply.send(Err(FeedError::NotSupported));
        }
        fn news(&self, query: NewsQuery, reply: Reply<Vec<NewsStory>>) {
            self.0.lock().unwrap().push((query, reply));
        }
    }

    /// Guards which stories show: a new symbol asks for its stories, the market scope asks
    /// without a symbol and ignores later symbol changes, and an answer to an earlier request
    /// never replaces the latest one, which an answer arriving late would otherwise do.
    #[test]
    fn only_the_latest_request_answers_and_the_scope_follows_the_symbol() {
        let feed = Arc::new(Feed::default());
        let mut panel = NewsPanel::new();
        panel.set_symbol("AAA");
        panel.set_feed(feed.clone());
        panel.set_symbol("BBB");
        let ctx = egui::Context::default();
        let story = |id: &str| NewsStory::new(id, DateTime::UNIX_EPOCH, id);
        let mut requests = std::mem::take(&mut *feed.0.lock().unwrap());
        let queried: Vec<_> = requests.iter().map(|(q, _)| q.symbols.clone()).collect();
        assert_eq!(queried, [["AAA"], ["BBB"]]);
        let (_, latest) = requests.pop().unwrap();
        let (_, earlier) = requests.pop().unwrap();
        latest.send(Ok(vec![story("b")]));
        earlier.send(Ok(vec![story("a")]));
        panel.poll(&ctx);
        assert_eq!(panel.stories, [story("b")]);

        panel.set_topics(["tech".into(), "fx".into()]);
        panel.set_providers(["wire".into()]);
        panel.set_symbols(["DDD".into()]);
        panel.topics[1].1 = true;
        panel.more_symbols[0].1 = true;
        panel.providers[0].1 = true;
        panel.refresh();
        panel.set_topics(["fx".into(), "oil".into()]);
        let queries: Vec<_> = std::mem::take(&mut *feed.0.lock().unwrap())
            .into_iter()
            .map(|(query, _)| (query.symbols, query.topics, query.providers))
            .collect();
        // The complete selection is sent, and re-offering values keeps the ones still offered.
        let selected = (
            vec!["BBB".to_owned(), "DDD".to_owned()],
            vec!["fx".to_owned()],
            vec!["wire".to_owned()],
        );
        assert_eq!(queries[3], selected);
        assert_eq!(queries[4], selected);

        panel.market = true;
        panel.refresh();
        panel.set_symbol("CCC");
        let mut requests = std::mem::take(&mut *feed.0.lock().unwrap());
        assert_eq!(requests.len(), 1);
        let (query, reply) = requests.pop().unwrap();
        assert_eq!((query.symbols, query.highlights), (vec![], false));
        reply.send(Err(FeedError::NotSupported));
        panel.poll(&ctx);
        assert!(panel.stories.is_empty());
        assert_eq!(panel.error, Some(FeedError::NotSupported));
    }
}
