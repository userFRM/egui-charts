//! Headlines and highlights the feed supplies, for the chart's symbol, more symbols or the whole
//! market, narrowed by topic, provider and country, with named views of the selection.

use crate::localization as l10n;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use chrono::{DateTime, Offset, Utc};
use egui::{Label, RichText, Sense, Ui};
use serde::{Deserialize, Serialize};

use crate::data::inbox::Inbox;
use crate::data::{Datafeed, FeedError, NewsQuery, NewsStory};
use crate::icons::icons;
use crate::model::TimezoneId;
use crate::services::{ConflictChoice, Notification, NotificationTarget, Storage, StoreKind};
use crate::theme::ThemeExt;
use crate::ui::dialogs::templates::TemplateStore;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::filter::FilterWidget;
use crate::ui_kit::status_message::{MessageKind, status_message, translated_status_message};
use crate::ui_kit::{EmptyState, IconButton, LoadingIndicator, PanelHeader};

use super::controls::dropdown;

/// Stories asked for at most.
const LIMIT: usize = 50;

/// Stories kept at most of those one alert reported; past it the alert loses its oldest.
const REPORTED: usize = 100;

// Escape can clear focus before rendering, so remember it after the host draws its widgets.
#[derive(Default)]
struct NewsFocus(egui::ViewportIdMap<bool>);

impl egui::plugin::Plugin for NewsFocus {
    fn debug_name(&self) -> &'static str {
        "News focus"
    }

    fn on_end_pass(&mut self, ui: &mut Ui) {
        self.0.insert(
            ui.ctx().viewport_id(),
            ui.memory(|memory| memory.focused().is_some()),
        );
    }
}

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

/// A selection the user named.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct NewsView {
    /// Stories about the whole market instead of the followed symbol.
    market: bool,
    /// The symbols added to the followed one.
    symbols: Vec<String>,
    topics: Vec<String>,
    providers: Vec<String>,
    countries: Vec<String>,
}

impl NewsView {
    /// The view with each list sorted and without repeats, to compare what two views ask for.
    fn sorted(mut self) -> Self {
        for list in [
            &mut self.symbols,
            &mut self.topics,
            &mut self.providers,
            &mut self.countries,
        ] {
            list.sort_unstable();
            list.dedup();
        }
        self
    }
}

/// The views of one panel and the one shown last, as kept in [`StoreKind::NewsViews`].
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
struct NewsViews {
    current: Option<String>,
    views: BTreeMap<String, NewsView>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    alerts: BTreeMap<String, NewsAlert>,
}

/// An alert fixes the followed symbol at creation, so later chart changes cannot retarget it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct NewsAlert {
    view: NewsView,
    active: bool,
}

/// Recent publications seen since an alert started or resumed, including late arrivals.
#[derive(Debug)]
struct SeenNews {
    since: DateTime<Utc>,
    publications: BTreeSet<(DateTime<Utc>, String)>,
}

/// News or highlights fed by the host's [`Datafeed::news`].
///
/// Keep it on the chart's symbol with [`set_symbol`](Self::set_symbol); the user switches between
/// that symbol's stories and the whole market's. With [`set_symbols`](Self::set_symbols) they can
/// add symbols, and with [`set_topics`](Self::set_topics), [`set_providers`](Self::set_providers)
/// and [`set_countries`](Self::set_countries) narrow the stories by topic, provider and country;
/// offering the values again keeps the choices still offered. With
/// [`set_storage`](Self::set_storage) they save the selection as a named view, switch, rename and
/// delete views, and get the last view back when the panel opens. Each change asks the feed again
/// with the complete selection, and only the answer to the latest request is shown. A story opens
/// to its text, with actions to show the chart at its time and to open its link. Publication times
/// show in the zone given to [`set_timezone`](Self::set_timezone), UTC until set. Call
/// [`poll`](Self::poll) each frame, then [`show`](Self::show) while the panel is open.
/// With the keyboard, Up and Down move between stories and Enter opens one. Escape returns to
/// the list when a story control or nothing has focus. Home and End reach the first and last
/// headline; Page Up, Page Down and Space page through them, with Shift+Space paging back.
/// Named alerts keep the current filter, including its symbols, through storage. Call
/// [`refresh`](Self::refresh) when the host learns of new stories, even while hidden, and drain
/// [`take_notifications`](Self::take_notifications) after polling for host delivery. Historical
/// stories from before an alert starts or resumes do not notify. Nor do the stories of an alert
/// that asks for the stories the panel listed when it was last shown: they show in the list and
/// are marked as seen. Each notification names its story, and [`open_story`](Self::open_story)
/// reads it among the stories its alert reported.
pub struct NewsPanel {
    highlights: bool,
    feed: Option<Arc<dyn Datafeed>>,
    symbol: Option<String>,
    market: bool,
    more_symbols: Vec<(String, bool)>,
    topics: Vec<(String, bool)>,
    providers: Vec<(String, bool)>,
    countries: Vec<(String, bool)>,
    /// The named views, in this panel's document.
    saved: TemplateStore<NewsViews>,
    stored: bool,
    /// Whether the view shown last is still to be shown once the views are read.
    restore: bool,
    /// The name being written.
    name: FilterWidget,
    timezone: TimezoneId,
    stories: Vec<NewsStory>,
    open: Option<String>,
    focus_back: bool,
    refocus: Option<String>,
    error: Option<FeedError>,
    loading: bool,
    inbox: Arc<Inbox>,
    request: u64,
    alert_inbox: Arc<Inbox>,
    alert_request: u64,
    alert_requests: BTreeMap<u64, (String, NewsAlert)>,
    /// At most 10,000 recent publication identities per alert, to suppress repeat answers.
    alert_seen: BTreeMap<String, SeenNews>,
    alerts_refresh: bool,
    alert_errors: BTreeMap<String, FeedError>,
    notifications: Vec<Notification>,
    /// The stories each alert reported, oldest first and at most [`REPORTED`].
    alert_stories: BTreeMap<String, Vec<NewsStory>>,
    /// The opened publication and its alert, if any, held apart from the refreshed list.
    reading: Option<(Option<String>, NewsStory)>,
    /// Whether `show` ran since the last poll.
    shown: bool,
}

impl std::fmt::Debug for NewsPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("NewsPanel")
            .field("highlights", &self.highlights)
            .field("symbol", &self.symbol)
            .field("market", &self.market)
            .field("more_symbols", &self.more_symbols)
            .field("topics", &self.topics)
            .field("providers", &self.providers)
            .field("countries", &self.countries)
            .field("saved", &self.saved)
            .field("stored", &self.stored)
            .field("timezone", &self.timezone)
            .field("stories", &self.stories)
            .field("open", &self.open)
            .field("error", &self.error)
            .field("loading", &self.loading)
            .field("request", &self.request)
            .finish_non_exhaustive()
    }
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
            countries: Vec::new(),
            saved: TemplateStore::new(StoreKind::NewsViews),
            stored: false,
            restore: false,
            name: FilterWidget::new()
                .placeholder("View name")
                .show_icon(false),
            timezone: TimezoneId::utc(),
            stories: Vec::new(),
            open: None,
            focus_back: false,
            refocus: None,
            error: None,
            loading: false,
            inbox: Arc::default(),
            request: 0,
            alert_inbox: Arc::default(),
            alert_request: 0,
            alert_requests: BTreeMap::new(),
            alert_seen: BTreeMap::new(),
            alerts_refresh: false,
            alert_errors: BTreeMap::new(),
            notifications: Vec::new(),
            alert_stories: BTreeMap::new(),
            reading: None,
            shown: false,
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
        self.alert_inbox = Arc::default();
        self.alert_requests.clear();
        self.alert_seen.clear();
        self.alert_errors.clear();
        self.notifications.clear();
        self.alert_stories.clear();
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
        if offer(&mut self.more_symbols, symbols) {
            self.refresh();
        }
    }

    /// Offer `topics`, in the values the feed supports, as a filter. None is selected at first,
    /// which lists every topic; choices still offered stay selected.
    pub fn set_topics(&mut self, topics: impl IntoIterator<Item = String>) {
        if offer(&mut self.topics, topics) {
            self.refresh();
        }
    }

    /// Offer `providers`, in the identifiers the feed uses, as a filter. None is selected at
    /// first, which lists every provider; choices still offered stay selected.
    pub fn set_providers(&mut self, providers: impl IntoIterator<Item = String>) {
        if offer(&mut self.providers, providers) {
            self.refresh();
        }
    }

    /// Offer `countries`, in the codes the feed uses, as a filter. None is selected at first,
    /// which lists every country; choices still offered stay selected.
    pub fn set_countries(&mut self, countries: impl IntoIterator<Item = String>) {
        if offer(&mut self.countries, countries) {
            self.refresh();
        }
    }

    /// Keep named views in `storage`, in [`StoreKind::NewsViews`], and show the view shown last
    /// once they are read. Poll every frame, as the panel is.
    pub fn set_storage(&mut self, storage: Arc<dyn Storage>) {
        self.stored = true;
        self.restore = true;
        self.saved.set_storage(storage);
        self.alert_inbox = Arc::default();
        self.alert_requests.clear();
        self.alert_seen.clear();
        self.alert_errors.clear();
        self.notifications.clear();
        self.alert_stories.clear();
        self.alerts_refresh = true;
    }

    /// The views document whose write the host refused because another client changed or created
    /// it; see [`TemplateStore::storage_conflict`].
    pub fn storage_conflict(&self) -> Option<&str> {
        self.saved.storage_conflict()
    }

    /// Settles the refused write of the views document `name`; see
    /// [`TemplateStore::resolve_storage_conflict`].
    pub fn resolve_storage_conflict(&mut self, name: &str, choice: ConflictChoice) {
        self.saved.resolve_storage_conflict(name, choice);
        if choice == ConflictChoice::ReloadRemote {
            self.restore = true;
            self.alert_requests.clear();
            self.alert_seen.clear();
            self.alert_errors.clear();
            self.alerts_refresh = true;
        }
    }

    /// Show publication times in `timezone`, such as the chart's. UTC until set.
    pub fn set_timezone(&mut self, timezone: TimezoneId) {
        self.timezone = timezone;
    }

    /// Open the story the host resolved from an event's id or from the
    /// [`NotificationTarget::Story`] of a notification, even when the current filter did not
    /// return it. The filter and saved view stay selected.
    ///
    /// A story that an alert of this panel reported opens apart from the list, among the newest
    /// 100 stories of that alert: Previous and Next, or the Left and Right keys, move to the
    /// story it reported before or after. It is matched by identifier and publication time.
    /// If several alerts reported it, the first by name is read. The opened publication stays
    /// visible across refreshes until the reader goes back to the list.
    pub fn open_story(&mut self, story: NewsStory) {
        self.focus_back = false;
        self.open = None;
        let alert = self.alert_stories.iter_mut().find_map(|(alert, stories)| {
            let held = stories
                .iter_mut()
                .find(|held| held.id == story.id && held.time == story.time)?;
            *held = story.clone();
            Some(alert.clone())
        });
        self.reading = Some((alert, story));
    }

    /// Ask the feed again, such as when the host learns of new stories.
    pub fn refresh(&mut self) {
        self.alerts_refresh = true;
        self.alert_requests.clear();
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
            countries: chosen(&self.countries).cloned().collect(),
            highlights: self.highlights,
            limit: LIMIT,
        };
        feed.news(query, self.inbox.reply(self.request));
    }

    /// New stories matching active named alerts, each naming its alert in the title, its
    /// headline in the body and its story in the target, which the notifications panel offers
    /// to open. The host delivers or logs them. Drain after each `poll`; at most
    /// 10,000 notifications are retained, dropping the oldest if the host stops draining.
    pub fn take_notifications(&mut self) -> Vec<Notification> {
        std::mem::take(&mut self.notifications)
    }

    /// Query each active filter independently; the feed applies topics, providers and countries.
    fn refresh_alerts(&mut self) {
        let Some(feed) = &self.feed else {
            return;
        };
        let alerts = self.views().alerts;
        self.alert_errors
            .retain(|name, _| alerts.get(name).is_some_and(|alert| alert.active));
        self.alert_seen
            .retain(|name, _| alerts.get(name).is_some_and(|alert| alert.active));
        self.alert_stories
            .retain(|name, _| alerts.contains_key(name));
        for (name, alert) in alerts.into_iter().filter(|(_, alert)| alert.active) {
            self.alert_seen
                .entry(name.clone())
                .or_insert_with(|| SeenNews {
                    since: Utc::now(),
                    publications: BTreeSet::new(),
                });
            if self
                .alert_requests
                .values()
                .any(|(pending, _)| pending == &name)
            {
                continue;
            }
            let mut query = NewsQuery::new(LIMIT);
            query.symbols.clone_from(&alert.view.symbols);
            query.topics.clone_from(&alert.view.topics);
            query.providers.clone_from(&alert.view.providers);
            query.countries.clone_from(&alert.view.countries);
            query.highlights = self.highlights;
            self.alert_request += 1;
            self.alert_requests
                .insert(self.alert_request, (name, alert));
            feed.news(query, self.alert_inbox.reply(self.alert_request));
        }
    }

    /// The name of this panel's views document.
    fn document(&self) -> &'static str {
        if self.highlights {
            "highlights"
        } else {
            "news"
        }
    }

    fn views(&self) -> NewsViews {
        self.saved
            .entries()
            .get(self.document())
            .cloned()
            .unwrap_or_default()
    }

    /// The selection as an alert keeps it: the followed symbol joins the chosen ones, and the
    /// whole market has none.
    fn alert_view(&self) -> NewsView {
        let mut view = self.view();
        if !view.market {
            view.symbols = self
                .symbol
                .iter()
                .chain(chosen(&self.more_symbols))
                .cloned()
                .collect();
        } else {
            view.symbols.clear();
        }
        view
    }

    /// The selection shown, as a view.
    fn view(&self) -> NewsView {
        NewsView {
            market: self.market,
            symbols: chosen(&self.more_symbols).cloned().collect(),
            topics: chosen(&self.topics).cloned().collect(),
            providers: chosen(&self.providers).cloned().collect(),
            countries: chosen(&self.countries).cloned().collect(),
        }
    }

    /// Select what `view` names. Values not offered are added as chosen, so that the request
    /// reproduces the view; the host's next offer of that list keeps them only if offered.
    fn apply(&mut self, view: &NewsView) {
        self.market = view.market;
        for (choices, values) in [
            (&mut self.more_symbols, &view.symbols),
            (&mut self.topics, &view.topics),
            (&mut self.providers, &view.providers),
            (&mut self.countries, &view.countries),
        ] {
            for (value, on) in choices.iter_mut() {
                *on = values.contains(value);
            }
            for value in values {
                if !choices.iter().any(|(offered, _)| offered == value) {
                    choices.push((value.clone(), true));
                }
            }
        }
    }

    fn write(&mut self, views: NewsViews) {
        let document = self.document();
        if let Err(error) = self.saved.save(document, views) {
            self.saved.error = Some(error);
        }
    }

    /// Receive the feed's answer and wake the UI when the next one arrives.
    pub fn poll(&mut self, ctx: &egui::Context) {
        let shown = std::mem::take(&mut self.shown);
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        let repaint = ctx.clone();
        self.alert_inbox
            .set_waker(move || repaint.request_repaint());
        self.saved.poll(ctx);
        if self.restore && self.saved.ready() {
            self.restore = false;
            let views = self.views();
            if let Some(name) = views.current
                && let Some(view) = views.views.get(&name)
            {
                self.apply(view);
                *self.name.query_mut() = name;
                self.refresh();
            }
        }
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
        if self.alerts_refresh && self.saved.ready() {
            self.alerts_refresh = false;
            self.refresh_alerts();
        }
        let alerts = self.views().alerts;
        let listed = shown.then(|| self.alert_view().sorted());
        for (key, answer) in self.alert_inbox.drain().answers {
            let Some((name, requested)) = self.alert_requests.remove(&key) else {
                continue;
            };
            if alerts.get(&name) != Some(&requested) || !requested.active {
                continue;
            }
            match *answer
                .downcast::<Result<Vec<NewsStory>, FeedError>>()
                .expect("news alert")
            {
                Ok(mut stories) => {
                    self.alert_errors.remove(&name);
                    stories.sort_by_key(|story| story.time);
                    let Some(seen) = self.alert_seen.get_mut(&name) else {
                        continue;
                    };
                    let quiet = listed.as_ref() == Some(&requested.view.clone().sorted());
                    for story in stories {
                        let publication = (story.time, story.id.clone());
                        if story.time < seen.since
                            || seen.publications.contains(&publication)
                            || (!requested.view.symbols.is_empty()
                                && !story.symbols.is_empty()
                                && !story
                                    .symbols
                                    .iter()
                                    .any(|symbol| requested.view.symbols.contains(symbol)))
                        {
                            continue;
                        }
                        seen.publications.insert(publication);
                        if seen.publications.len() > 10_000 {
                            seen.publications.pop_first();
                        }
                        // The list shows it: `refresh` asks for the list and the alerts together.
                        if quiet {
                            continue;
                        }
                        let mut notification = Notification::new(story.time, name.clone());
                        notification.body.clone_from(&story.headline);
                        notification.symbol = story
                            .symbols
                            .iter()
                            .find(|symbol| {
                                requested.view.symbols.is_empty()
                                    || requested.view.symbols.contains(symbol)
                            })
                            .cloned();
                        notification.target = Some(NotificationTarget::Story(story.id.clone()));
                        self.notifications.push(notification);
                        let reported = self.alert_stories.entry(name.clone()).or_default();
                        reported.push(story);
                        let extra = reported.len().saturating_sub(REPORTED);
                        reported.drain(..extra);
                    }
                }
                Err(error) => {
                    self.alert_errors.insert(name, error);
                }
            }
        }
        let extra = self.notifications.len().saturating_sub(10_000);
        self.notifications.drain(..extra);
    }

    /// Render the stories, or the open story, after [`poll`](Self::poll).
    pub fn show(&mut self, ui: &mut Ui) -> NewsAction {
        self.shown = true;
        let focus = ui.ctx().plugin_or_default::<NewsFocus>();
        let had_focus = *focus
            .lock()
            .0
            .entry(ui.ctx().viewport_id())
            .or_insert_with(|| ui.memory(|memory| memory.focused().is_some()));
        let mut action = NewsAction::None;
        let theme = ui.active_theme();
        let title = if self.highlights {
            "Highlights"
        } else {
            "News"
        };
        // A host-opened publication is held apart from the list, with alert navigation if available.
        let reading = self.reading.take();
        let walk = reading.as_ref().and_then(|(alert, story)| {
            let stories = self.alert_stories.get(alert.as_ref()?)?;
            Some((stories, stories.iter().position(|held| held == story)?))
        });
        let story = match &reading {
            Some((_, story)) => Some(story),
            None => self
                .open
                .as_ref()
                .and_then(|id| self.stories.iter().find(|story| story.id == *id)),
        };
        if let Some(story) = story {
            let mut controls = [None; 5];
            let mut back = false;
            let mut step = None;
            PanelHeader::new(title).show(ui, |ui| {
                let response = IconButton::new(&icons::CHEVRON_LEFT)
                    .tooltip("Back to the stories")
                    .show(ui);
                if std::mem::take(&mut self.focus_back) {
                    response.request_focus();
                }
                controls[0] = Some(response.id);
                back = response.clicked();
            });
            if let Some((stories, at)) = walk {
                ui.horizontal(|ui| {
                    let response = Button::new("Previous").small().enabled(at > 0).show(ui);
                    controls[3] = Some(response.id);
                    if response.clicked() {
                        step = Some(at - 1);
                    }
                    let response = Button::new("Next")
                        .small()
                        .enabled(at + 1 < stories.len())
                        .show(ui);
                    controls[4] = Some(response.id);
                    if response.clicked() {
                        step = Some(at + 1);
                    }
                });
            }
            // Each story keeps its own place, so the next one starts at its top.
            egui::ScrollArea::vertical()
                .id_salt(("news_story", story.id.as_str()))
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.label(
                        RichText::new(l10n::text(ui.ctx(), &story.headline))
                            .size(theme.text.heading)
                            .strong(),
                    );
                    ui.label(
                        RichText::new(l10n::text(ui.ctx(), meta(story, &self.timezone)))
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
                        let response = Button::new("Show on chart").show(ui);
                        controls[1] = Some(response.id);
                        if response.clicked() {
                            action = NewsAction::ShowOnChart(story.time);
                        }
                        if let Some(url) = &story.url {
                            let response = Button::new("Open story").show(ui);
                            controls[2] = Some(response.id);
                            if response.clicked() {
                                action = NewsAction::OpenUrl(url.clone());
                            }
                        }
                    });
                });
            let story_focused = ui.memory(|memory| match memory.focused() {
                Some(id) => controls.contains(&Some(id)),
                None => {
                    !had_focus
                        || controls
                            .into_iter()
                            .flatten()
                            .any(|id| memory.had_focus_last_frame(id))
                }
            });
            if story_focused && let Some((stories, at)) = walk {
                let (left, right) = ui.input_mut(|i| {
                    (
                        i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowLeft),
                        i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowRight),
                    )
                });
                if left || right {
                    // The arrows move between stories here, not the focus between controls.
                    ui.memory_mut(|memory| memory.move_focus(egui::FocusDirection::None));
                }
                if right && at + 1 < stories.len() {
                    step = Some(at + 1);
                } else if left && at > 0 {
                    step = Some(at - 1);
                }
            }
            back |= story_focused
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
            let next = step.and_then(|at| walk.map(|(stories, _)| stories[at].clone()));
            if back {
                // Back on the list, the story keeps the focus only where the list holds it.
                self.refocus = self
                    .stories
                    .iter()
                    .any(|listed| listed.id == story.id)
                    .then(|| story.id.clone());
                self.open = None;
            } else if let Some((alert, story)) = reading {
                self.reading = Some((alert, next.unwrap_or(story)));
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
            self.countries.clone(),
        );
        let offered =
            !(self.topics.is_empty() && self.providers.is_empty() && self.countries.is_empty())
                || (!self.more_symbols.is_empty() && !self.market);
        if offered {
            ui.horizontal_wrapped(|ui| {
                for (id, all, some, choices) in [
                    (
                        "news_symbols",
                        "All symbols",
                        "{count} symbols",
                        &mut self.more_symbols,
                    ),
                    (
                        "news_topics",
                        "All topics",
                        "{count} topics",
                        &mut self.topics,
                    ),
                    (
                        "news_providers",
                        "All providers",
                        "{count} providers",
                        &mut self.providers,
                    ),
                    (
                        "news_countries",
                        "All countries",
                        "{count} countries",
                        &mut self.countries,
                    ),
                ] {
                    if choices.is_empty() || (id == "news_symbols" && self.market) {
                        continue;
                    }
                    let count = choices.iter().filter(|(_, on)| *on).count();
                    let text = if count == 0 {
                        l10n::text(ui.ctx(), all)
                    } else {
                        l10n::format_text(ui.ctx(), some, &[("count", count.to_string().as_str())])
                    };
                    dropdown(ui, id, &text, |ui| {
                        for (value, on) in choices.iter_mut() {
                            ui.checkbox(on, l10n::text(ui.ctx(), value.as_str()));
                        }
                    });
                }
            });
        }
        if self.stored {
            refresh |= self.show_views(ui);
        }
        if refresh
            || market != self.market
            || before
                != (
                    self.more_symbols.clone(),
                    self.topics.clone(),
                    self.providers.clone(),
                    self.countries.clone(),
                )
        {
            self.refresh();
        }
        match &self.error {
            Some(FeedError::NotSupported) => {
                status_message(ui, MessageKind::Info, "This feed supplies no stories.");
            }
            Some(error) => {
                translated_status_message(
                    ui,
                    MessageKind::Error,
                    l10n::format_text(
                        ui.ctx(),
                        "Stories unavailable: {error}",
                        &[("error", error.to_string().as_str())],
                    ),
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
        let rows = ui.scope_id().with("news_stories");
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let mut responses = Vec::new();
                for story in &self.stories {
                    let response = ui
                        .scope_builder(
                            egui::UiBuilder::new()
                                .scope_id(rows.with(&story.id))
                                .sense(Sense::click()),
                            |ui| {
                                let response = ui.response();
                                let hovered =
                                    response.hovered() || ui.memory(|m| m.has_focus(response.id));
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
                                            RichText::new(l10n::text(
                                                ui.ctx(),
                                                meta(story, &self.timezone),
                                            ))
                                            .size(theme.text.small)
                                            .color(theme.ui.text_secondary),
                                        );
                                        ui.add(
                                            Label::new(
                                                RichText::new(l10n::text(
                                                    ui.ctx(),
                                                    &story.headline,
                                                ))
                                                .strong(),
                                            )
                                            .wrap(),
                                        );
                                        if self.highlights && !story.body.is_empty() {
                                            ui.label(l10n::text(ui.ctx(), &story.body));
                                        }
                                    });
                            },
                        )
                        .response;
                    let translated_label = l10n::text(ui.ctx(), &story.headline);
                    response.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::Button, true, &translated_label)
                    });
                    responses.push((response.id, response.rect));
                    if self.refocus.as_ref() == Some(&story.id) {
                        response.request_focus();
                        response.scroll_to_me(None);
                        self.refocus = None;
                    }
                    let focused = response.has_focus();
                    if response
                        .on_hover_cursor(egui::CursorIcon::PointingHand)
                        .clicked()
                        && !(focused && ui.input(|i| i.key_pressed(egui::Key::Space)))
                    {
                        self.open = Some(story.id.clone());
                        self.focus_back = focused;
                    }
                    ui.separator();
                }
                if let Some(at) =
                    ui.memory(|m| responses.iter().position(|(id, _)| m.has_focus(*id)))
                {
                    let (step, page, home, end) = ui.input_mut(|i| {
                        let step = i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)
                            as isize
                            - i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp) as isize;
                        let back = i.consume_key(egui::Modifiers::SHIFT, egui::Key::Space) as isize;
                        let page = i.consume_key(egui::Modifiers::NONE, egui::Key::PageDown)
                            as isize
                            + i.consume_key(egui::Modifiers::NONE, egui::Key::Space) as isize
                            - i.consume_key(egui::Modifiers::NONE, egui::Key::PageUp) as isize
                            - back;
                        (
                            step,
                            page,
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Home),
                            i.consume_key(egui::Modifiers::NONE, egui::Key::End),
                        )
                    });
                    if step != 0 || page != 0 || home || end {
                        let last = responses.len() - 1;
                        let next = if home {
                            0
                        } else if end {
                            last
                        } else if page != 0 {
                            let y = responses[at].1.top() + ui.clip_rect().height() * page as f32;
                            if page > 0 {
                                responses
                                    .iter()
                                    .position(|(_, rect)| rect.top() >= y)
                                    .unwrap_or(last)
                            } else {
                                responses
                                    .iter()
                                    .rposition(|(_, rect)| rect.top() <= y)
                                    .unwrap_or(0)
                            }
                        } else {
                            at.saturating_add_signed(step).min(last)
                        };
                        let (id, rect) = responses[next];
                        ui.memory_mut(|m| {
                            m.request_focus(id);
                            m.move_focus(egui::FocusDirection::None);
                        });
                        ui.scroll_to_rect(rect, None);
                    }
                }
            });
        action
    }

    /// The views row: switch to a view, and save, rename or delete one under the written name.
    fn show_views(&mut self, ui: &mut Ui) -> bool {
        let mut views = self.views();
        let mut old_alerts = views.alerts.clone();
        let ready = self.saved.ready();
        let mut switch = None;
        let (mut save, mut rename, mut delete, mut alert) = (false, false, false, false);
        let mut changed = false;
        ui.add_enabled_ui(ready, |ui| {
            ui.horizontal_wrapped(|ui| {
                if !views.views.is_empty() {
                    dropdown(
                        ui,
                        "news_views",
                        views.current.as_deref().unwrap_or("Views"),
                        |ui| {
                            for name in views.views.keys() {
                                let on = views.current.as_ref() == Some(name);
                                if ui
                                    .selectable_label(on, l10n::text(ui.ctx(), name))
                                    .clicked()
                                    && ready
                                {
                                    switch = Some(name.clone());
                                }
                            }
                        },
                    );
                }
                ui.allocate_ui(egui::vec2(128.0, 28.0), |ui| {
                    self.name.show(ui);
                });
                let named = ready && !self.name.query().trim().is_empty();
                save = Button::new("Save").enabled(named).show(ui).clicked();
                rename = Button::new("Rename")
                    .enabled(
                        named
                            && views.current.is_some()
                            && !views.views.contains_key(self.name.query().trim()),
                    )
                    .show(ui)
                    .clicked();
                delete = Button::new("Delete")
                    .enabled(ready && views.current.is_some())
                    .show(ui)
                    .clicked();
                alert = Button::new("Create alert")
                    .enabled(named)
                    .show(ui)
                    .clicked();
            });
        });
        let name = self.name.query().trim().to_owned();
        let switched = switch.is_some();
        if let Some(target) = switch
            && let Some(view) = views.views.get(&target).cloned()
        {
            self.apply(&view);
            *self.name.query_mut() = target.clone();
            views.current = Some(target);
            self.write(views);
        } else if save || alert {
            views.views.insert(name.clone(), self.view());
            if alert || views.alerts.contains_key(&name) {
                let view = self.alert_view();
                let active = alert || views.alerts.get(&name).is_some_and(|alert| alert.active);
                views
                    .alerts
                    .insert(name.clone(), NewsAlert { view, active });
                changed = true;
            }
            views.current = Some(name);
            self.write(views);
        } else if rename
            && let Some(old) = views.current.clone()
            && old != name
        {
            if let Some(view) = views.views.remove(&old) {
                views.views.insert(name.clone(), view);
            }
            let moved = views.alerts.remove(&old);
            let replaced = views.alerts.remove(&name);
            changed = moved.is_some() || replaced.is_some();
            old_alerts.remove(&old);
            old_alerts.remove(&name);
            if let Some(alert) = moved {
                old_alerts.insert(name.clone(), alert.clone());
                views.alerts.insert(name.clone(), alert);
                let seen = self.alert_seen.remove(&old);
                self.alert_seen.remove(&name);
                if let Some(seen) = seen {
                    self.alert_seen.insert(name.clone(), seen);
                }
                let reported = self.alert_stories.remove(&old);
                self.alert_stories.remove(&name);
                if let Some(reported) = reported {
                    self.alert_stories.insert(name.clone(), reported);
                }
                if let Some((reading, _)) = &mut self.reading
                    && reading.as_ref() == Some(&old)
                {
                    *reading = Some(name.clone());
                }
                let error = self.alert_errors.remove(&old);
                self.alert_errors.remove(&name);
                if let Some(error) = error {
                    self.alert_errors.insert(name.clone(), error);
                }
                self.alert_requests
                    .retain(|_, (pending, _)| pending != &name);
                for (pending, _) in self.alert_requests.values_mut() {
                    if *pending == old {
                        pending.clone_from(&name);
                    }
                }
            }
            views.current = Some(name);
            self.write(views);
        } else if delete && let Some(old) = views.current.take() {
            views.views.remove(&old);
            changed = views.alerts.remove(&old).is_some();
            self.write(views);
        }
        let mut views = self.views();
        let ready = self.saved.ready();
        let mut remove = None;
        let mut toggled = false;
        for (name, alert) in &mut views.alerts {
            ui.push_id(("news_alert", name), |ui| {
                ui.horizontal(|ui| {
                    let label = l10n::format_text(ui.ctx(), "Alert: {name}", &[("name", name)]);
                    toggled |= ui
                        .add_enabled(ready, egui::Checkbox::new(&mut alert.active, label))
                        .changed();
                    let delete =
                        l10n::format_text(ui.ctx(), "Delete alert {name}", &[("name", name)]);
                    if Button::new(&delete)
                        .small()
                        .enabled(ready)
                        .show(ui)
                        .clicked()
                    {
                        remove = Some(name.clone());
                    }
                });
            });
        }
        if let Some(name) = &remove {
            views.alerts.remove(name);
        }
        if toggled || remove.is_some() {
            self.write(views);
            changed = true;
        }
        if changed {
            let alerts = self.views().alerts;
            self.alert_requests
                .retain(|_, (name, alert)| alerts.get(name) == Some(alert));
            self.alert_errors
                .retain(|name, _| alerts.get(name).is_some_and(|alert| alert.active));
            self.alert_seen.retain(|name, _| {
                alerts
                    .get(name)
                    .is_some_and(|alert| alert.active && old_alerts.get(name) == Some(alert))
            });
            self.alerts_refresh = true;
        }
        if let Some(error) = &self.saved.error {
            translated_status_message(
                ui,
                MessageKind::Error,
                l10n::format_text(
                    ui.ctx(),
                    "Saved views: {error}",
                    &[("error", error.as_str())],
                ),
            );
        }
        for (name, error) in &self.alert_errors {
            translated_status_message(
                ui,
                MessageKind::Error,
                l10n::format_text(
                    ui.ctx(),
                    "News alert {name} unavailable: {error}",
                    &[("name", name), ("error", &error.to_string())],
                ),
            );
        }
        switched
    }
}

/// Replace the offered values with `values`, keeping the choice of those still offered, and
/// tell whether the choice changed.
fn offer(choices: &mut Vec<(String, bool)>, values: impl IntoIterator<Item = String>) -> bool {
    let kept = std::mem::take(choices);
    *choices = values
        .into_iter()
        .map(|value| {
            let on = kept.iter().any(|(old, on)| *on && *old == value);
            (value, on)
        })
        .collect();
    !chosen(&kept).eq(chosen(choices))
}

/// The values the user ticked.
fn chosen(choices: &[(String, bool)]) -> impl Iterator<Item = &String> {
    choices.iter().filter(|(_, on)| *on).map(|(value, _)| value)
}

/// The publication time in `zone`, with its offset on that date, and the source.
fn meta(story: &NewsStory, zone: &TimezoneId) -> String {
    let local = story.time.with_timezone(zone);
    let time = format!(
        "{} {}",
        local.format("%b %-d, %H:%M"),
        super::symbol_details::zone_label(local.offset().fix().local_minus_utc()),
    );
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
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };
    use serde_json::json;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Feed(Mutex<Vec<(NewsQuery, Reply<Vec<NewsStory>>)>>);

    #[derive(Default)]
    struct Documents(Mutex<BTreeMap<String, String>>);

    impl Storage for Documents {
        fn list(&self, _: StoreKind, reply: Reply<Vec<String>>) {
            reply.send(Ok(self.0.lock().unwrap().keys().cloned().collect()));
        }
        fn load(&self, _: StoreKind, name: &str, reply: Reply<Option<String>>) {
            reply.send(Ok(self.0.lock().unwrap().get(name).cloned()));
        }
        fn save(&self, _: StoreKind, name: &str, document: String, reply: Reply<()>) {
            self.0.lock().unwrap().insert(name.into(), document);
            reply.send(Ok(()));
        }
        fn delete(&self, _: StoreKind, name: &str, reply: Reply<()>) {
            self.0.lock().unwrap().remove(name);
            reply.send(Ok(()));
        }
    }

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
    /// never replaces the latest one, which an answer arriving late would otherwise do. Story
    /// times show in the panel's zone, with the offset on their date.
    #[test]
    fn latest_request_scope_and_time_zone_decide_what_shows() {
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
        let shown = |panel: &mut NewsPanel| {
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                panel.show(ui);
            });
            let texts: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            texts
        };
        assert!(shown(&mut panel).contains(&"Jan 1, 00:00 UTC".to_owned()));
        // 1970-01-01T00:00Z is 20:30 the evening before at three and a half hours west of UTC.
        panel.set_timezone(TimezoneId::fixed_offset(-(3 * 3600 + 30 * 60)).unwrap());
        assert!(feed.0.lock().unwrap().is_empty());
        let texts = shown(&mut panel);
        assert!(
            texts.contains(&"Dec 31, 20:30 GMT-3:30".to_owned()),
            "{texts:?}"
        );

        panel.set_topics(["tech".into(), "fx".into()]);
        panel.set_providers(["wire".into()]);
        panel.set_symbols(["DDD".into()]);
        panel.topics[1].1 = true;
        panel.more_symbols[0].1 = true;
        panel.providers[0].1 = true;
        panel.refresh();
        panel.set_topics(["fx".into(), "oil".into()]);
        panel.set_providers(["other".into()]);
        let queries: Vec<_> = std::mem::take(&mut *feed.0.lock().unwrap())
            .into_iter()
            .map(|(query, _)| (query.symbols, query.topics, query.providers))
            .collect();
        // The complete selection is sent. Offering values again keeps the ones still offered and
        // asks again only when that changes the selection.
        let symbols = vec!["BBB".to_owned(), "DDD".to_owned()];
        let fx = vec!["fx".to_owned()];
        assert_eq!(
            queries,
            [
                (symbols.clone(), fx.clone(), vec!["wire".to_owned()]),
                (symbols, fx, vec![]),
            ]
        );

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

    /// Guards named views through storage: saving, switching, renaming and deleting each write
    /// the panel's document, a switch asks for the view's selection, and a panel opened on the
    /// same storage asks for the view shown last.
    #[test]
    fn views_save_switch_rename_delete_and_restore_through_storage() {
        fn panel(feed: &Arc<Feed>, documents: &Arc<Documents>) -> NewsPanel {
            let mut panel = NewsPanel::new();
            panel.set_topics(["fx".into(), "oil".into()]);
            panel.set_countries(["US".into(), "DE".into()]);
            panel.set_feed(feed.clone());
            panel.set_storage(documents.clone());
            panel
        }
        /// Write `name` and press `button`. The loading spinner asks for a frame after every
        /// frame, so step a few at a time.
        fn press(h: &mut Harness<'_, NewsPanel>, name: &str, button: &str) {
            *h.state_mut().name.query_mut() = name.into();
            h.run_steps(3);
            h.get_by_label(button).click();
            h.run_steps(3);
        }
        let feed = Arc::new(Feed::default());
        let documents = Arc::new(Documents::default());
        let stored = || {
            let document = documents.0.lock().unwrap()["news"].clone();
            let mut document: serde_json::Value = serde_json::from_str(&document).unwrap();
            document["template"].take()
        };
        let latest = || {
            let (query, _) = feed.0.lock().unwrap().pop().unwrap();
            (query.topics, query.countries)
        };
        let mut h = Harness::builder()
            .with_context_setup(egui_extras::install_image_loaders)
            .build_ui_state(
                |ui, panel: &mut NewsPanel| {
                    panel.poll(ui.ctx());
                    panel.show(ui);
                },
                panel(&feed, &documents),
            );
        h.run_steps(3);
        h.state_mut().topics[0].1 = true;
        h.state_mut().countries[0].1 = true;
        press(&mut h, "Rates", "Save");
        let rates = json!({
            "market": false,
            "symbols": [],
            "topics": ["fx"],
            "providers": [],
            "countries": ["US"],
        });
        assert_eq!(
            stored(),
            json!({ "current": "Rates", "views": { "Rates": rates } })
        );

        h.state_mut().topics = vec![("fx".into(), false), ("oil".into(), true)];
        h.state_mut().countries[0].1 = false;
        press(&mut h, "Energy", "Save");
        assert_eq!(stored()["current"], "Energy");
        assert_eq!(stored()["views"]["Energy"]["topics"], json!(["oil"]));
        h.get_by_label("Energy").click();
        h.run_steps(3);
        h.get_by_label("Rates").click();
        h.run_steps(3);
        assert_eq!(latest(), (vec!["fx".to_owned()], vec!["US".to_owned()]));
        assert_eq!(stored()["current"], "Rates");

        let before = stored();
        *h.state_mut().name.query_mut() = "Energy".into();
        h.run_steps(3);
        assert!(h.get_by_label("Rename").accesskit_node().is_disabled());
        assert_eq!(stored(), before);

        press(&mut h, "Rates EU", "Rename");
        let views = stored();
        assert_eq!(views["current"], "Rates EU");
        assert_eq!(views["views"]["Rates EU"], rates);
        assert!(views["views"].get("Rates").is_none() && views["views"].get("Energy").is_some());

        let mut restored = panel(&feed, &documents);
        for _ in 0..3 {
            restored.poll(&egui::Context::default());
        }
        assert_eq!(latest(), (vec!["fx".to_owned()], vec!["US".to_owned()]));

        press(&mut h, "Rates EU", "Delete");
        let views = stored();
        assert_eq!(views["current"], serde_json::Value::Null);
        assert_eq!(views["views"].as_object().unwrap().len(), 1);
        assert!(views["views"].get("Energy").is_some());
    }

    /// Reading with the keyboard keeps focus on one story, including at either end of the list
    /// and after returning from its text, while another field keeps its own Escape handling.
    #[test]
    fn arrow_keys_move_between_stories_enter_opens_escape_returns() {
        let mut panel = NewsPanel::new();
        for title in ["First", "Second", "Third"] {
            let mut story = NewsStory::new(title, DateTime::UNIX_EPOCH, title);
            story.body = format!("Text of {title}");
            panel.stories.push(story);
        }
        let mut outside = String::new();
        let mut h = Harness::builder()
            .with_context_setup(|ctx| {
                egui_extras::install_image_loaders(ctx);
                ctx.global_style_mut(|style| {
                    style.scroll_animation = egui::style::ScrollAnimation::none()
                });
            })
            .build_ui_state(
                move |ui, panel: &mut NewsPanel| {
                    egui::Panel::left("news").show(ui, |ui| {
                        panel.show(ui);
                    });
                    ui.add(egui::TextEdit::singleline(&mut outside).hint_text("Outside news"));
                },
                panel,
            );
        h.run();
        h.get_by_role_and_label(egui::Role::Button, "First").focus();
        h.run();
        for (key, shift, title) in [
            (egui::Key::ArrowDown, false, "Second"),
            (egui::Key::ArrowUp, false, "First"),
            (egui::Key::ArrowUp, false, "First"),
            (egui::Key::ArrowDown, false, "Second"),
            (egui::Key::ArrowDown, false, "Third"),
            (egui::Key::ArrowDown, false, "Third"),
            (egui::Key::ArrowUp, false, "Second"),
            (egui::Key::Home, false, "First"),
            (egui::Key::PageDown, false, "Third"),
            (egui::Key::PageUp, false, "First"),
            (egui::Key::End, false, "Third"),
            (egui::Key::Home, false, "First"),
            (egui::Key::Space, false, "Third"),
            (egui::Key::ArrowUp, false, "Second"),
            (egui::Key::Space, true, "First"),
            (egui::Key::ArrowDown, false, "Second"),
        ] {
            h.key_press_modifiers(
                if shift {
                    egui::Modifiers::SHIFT
                } else {
                    egui::Modifiers::NONE
                },
                key,
            );
            h.run();
            assert!(
                h.get_by_role_and_label(egui::Role::Button, title)
                    .is_focused(),
                "{key:?}: {title}"
            );
        }
        h.key_press(egui::Key::Enter);
        h.run();
        assert_eq!(h.state().open.as_deref(), Some("Second"));
        assert!(h.query_all_by_value("Text of Second").next().is_some());
        h.key_press(egui::Key::Escape);
        h.run();
        assert!(h.state().open.is_none());
        assert!(
            h.get_by_role_and_label(egui::Role::Button, "Second")
                .is_focused()
        );

        h.get_by_role_and_label(egui::Role::Button, "First").click();
        h.run();
        assert_eq!(h.state().open.as_deref(), Some("First"));
        h.key_press(egui::Key::Escape);
        h.run();
        assert!(h.state().open.is_none());
        h.get_by_role_and_label(egui::Role::Button, "First").click();
        h.run();
        h.get_by_label("Show on chart").focus();
        h.run();
        h.key_press(egui::Key::Escape);
        h.run();
        assert!(h.state().open.is_none());

        h.get_by_role_and_label(egui::Role::Button, "First").click();
        h.run();
        h.get_by_role(egui::Role::TextInput).focus();
        h.run();
        assert!(h.get_by_role(egui::Role::TextInput).is_focused());
        h.key_press(egui::Key::Escape);
        h.run();
        assert_eq!(h.state().open.as_deref(), Some("First"));
        assert!(!h.get_by_role(egui::Role::TextInput).is_focused());
        h.key_press(egui::Key::Escape);
        h.run();
        assert!(h.state().open.is_none());

        // With more than a page of rows, paging reaches an interior story, rather than one row
        // down or the end, and paging back returns to the first row.
        for number in 4..=9 {
            let title = format!("Story {number}");
            h.state_mut()
                .stories
                .push(NewsStory::new(&title, DateTime::UNIX_EPOCH, &title));
        }
        h.set_size(egui::vec2(320.0, 300.0));
        h.key_press(egui::Key::Home);
        h.run();
        h.key_press(egui::Key::PageDown);
        h.run();
        let mut middle = std::iter::once("Third".to_owned())
            .chain((4..9).map(|number| format!("Story {number}")));
        assert!(middle.any(|title| {
            h.query_by_role_and_label(egui::Role::Button, &title)
                .is_some_and(|node| node.is_focused())
        }));
        h.key_press(egui::Key::PageUp);
        h.run();
        assert!(
            h.get_by_role_and_label(egui::Role::Button, "First")
                .is_focused()
        );
    }

    /// Independent saved alerts retain their symbols and filters, deliver simultaneous stories
    /// once, and remain silent while paused or after deletion, including replies already pending.
    #[test]
    fn named_alerts_keep_their_filters_and_pause_delete_and_restore() {
        let feed = Arc::new(Feed::default());
        let documents = Arc::new(Documents::default());
        let mut panel = NewsPanel::new();
        panel.set_symbol("AAA");
        panel.set_topics(["fx".into(), "oil".into()]);
        panel.set_providers(["wire".into(), "other".into()]);
        panel.set_countries(["US".into(), "DE".into()]);
        panel.set_storage(documents.clone());
        panel.set_feed(feed.clone());
        let mut h = Harness::builder()
            .with_context_setup(egui_extras::install_image_loaders)
            .build_ui_state(
                |ui, panel: &mut NewsPanel| {
                    panel.poll(ui.ctx());
                    panel.show(ui);
                },
                panel,
            );
        h.run_steps(3);
        for (name, symbol, choice) in [("Alpha", "AAA", 0), ("Beta", "BBB", 1)] {
            let panel = h.state_mut();
            panel.set_symbol(symbol);
            for choices in [
                &mut panel.topics,
                &mut panel.providers,
                &mut panel.countries,
            ] {
                for (at, (_, selected)) in choices.iter_mut().enumerate() {
                    *selected = at == choice;
                }
            }
            *panel.name.query_mut() = name.into();
            h.run_steps(3);
            h.get_by_label("Create alert").click();
            h.run_steps(3);
        }
        // Leave both alerts' filters, whose stories the list would show.
        h.state_mut().set_symbol("CCC");
        h.run_steps(3);
        let requests = std::mem::take(&mut *feed.0.lock().unwrap());
        for (symbol, topic, provider, country) in
            [("AAA", "fx", "wire", "US"), ("BBB", "oil", "other", "DE")]
        {
            assert!(requests.iter().any(|(query, _)| query.symbols == [symbol]
                && query.topics == [topic]
                && query.providers == [provider]
                && query.countries == [country]));
        }
        let time = Utc::now();
        let mut stories: Vec<_> = [
            ("a1", &["AAA"][..]),
            ("a2", &["AAA"][..]),
            ("b1", &["BBB"][..]),
            ("both", &["AAA", "BBB"][..]),
        ]
        .into_iter()
        .map(|(id, symbols)| {
            let mut story = NewsStory::new(id, time, id.to_uppercase());
            story.symbols = symbols.iter().map(|symbol| (*symbol).to_owned()).collect();
            story
        })
        .collect();
        let mut historical =
            NewsStory::new("history", time - chrono::Duration::hours(1), "History");
        historical.symbols = vec!["AAA".into()];
        stories.push(historical);
        for (_, reply) in requests {
            reply.send(Ok(stories.clone()));
        }
        h.run_steps(3);
        let notifications = h.state_mut().take_notifications();
        let mut titles: Vec<_> = notifications
            .iter()
            .map(|notification| {
                let Some(NotificationTarget::Story(story)) = &notification.target else {
                    panic!("no story in {notification:?}");
                };
                (
                    notification.title.as_str(),
                    notification.body.as_str(),
                    notification.symbol.as_deref().unwrap(),
                    story.as_str(),
                )
            })
            .collect();
        titles.sort_unstable();
        assert_eq!(
            titles,
            [
                ("Alpha", "A1", "AAA", "a1"),
                ("Alpha", "A2", "AAA", "a2"),
                ("Alpha", "BOTH", "AAA", "both"),
                ("Beta", "B1", "BBB", "b1"),
                ("Beta", "BOTH", "BBB", "both")
            ]
        );

        // Re-reading the same answer does not deliver it again.
        h.state_mut().refresh();
        h.run_steps(3);
        for (_, reply) in std::mem::take(&mut *feed.0.lock().unwrap()) {
            reply.send(Ok(stories.clone()));
        }
        h.run_steps(3);
        assert!(h.state_mut().take_notifications().is_empty());

        // A fresh publication can arrive after a more recent one, without being history or a
        // duplicate. Both answers are still ordered newest first by the feed.
        let published = Utc::now();
        let mut newest = NewsStory::new(
            "newest",
            published + chrono::Duration::seconds(1),
            "Newest story",
        );
        newest.symbols = vec!["AAA".into()];
        let mut late = NewsStory::new("late", published, "Late story");
        late.symbols = vec!["AAA".into()];
        for (answer, expected) in [
            (vec![newest.clone()], "Newest story"),
            (vec![newest, late], "Late story"),
        ] {
            h.state_mut().refresh();
            h.run_steps(3);
            for (_, reply) in std::mem::take(&mut *feed.0.lock().unwrap()) {
                reply.send(Ok(answer.clone()));
            }
            h.run_steps(3);
            let notifications = h.state_mut().take_notifications();
            assert_eq!(notifications.len(), 1, "{expected}");
            assert_eq!(notifications[0].title, "Alpha");
            assert_eq!(notifications[0].body, expected);
        }

        // One filter's successful reply must not hide another filter's failure.
        h.state_mut().refresh();
        h.run_steps(3);
        for (query, reply) in std::mem::take(&mut *feed.0.lock().unwrap()) {
            if query.symbols == ["AAA"] {
                reply.send(Err(FeedError::NotSupported));
            } else {
                reply.send(Ok(vec![]));
            }
        }
        h.run_steps(3);
        assert_eq!(
            h.state().alert_errors.get("Alpha"),
            Some(&FeedError::NotSupported)
        );
        assert!(!h.state().alert_errors.contains_key("Beta"));

        for label in ["Beta", "Alpha"] {
            h.get_by_role_and_label(egui::Role::Button, label).click();
            h.run_steps(3);
        }
        let mut pending = None;
        for (query, reply) in std::mem::take(&mut *feed.0.lock().unwrap()) {
            if query.symbols == ["AAA"] {
                pending = Some(reply);
            } else {
                reply.send(Ok(vec![]));
            }
        }
        let mut during_rename = NewsStory::new("a3", Utc::now(), "During rename");
        during_rename.symbols = vec!["AAA".into()];
        *h.state_mut().name.query_mut() = "Alpha EU".into();
        h.run_steps(3);
        h.get_by_label("Rename").click();
        h.run_steps(3);
        pending.unwrap().send(Ok(vec![during_rename]));
        h.run_steps(3);
        let notifications = h.state_mut().take_notifications();
        assert_eq!(notifications.len(), 1, "a rename keeps an in-flight story");
        assert_eq!(notifications[0].title, "Alpha EU");
        assert_eq!(notifications[0].body, "During rename");
        *h.state_mut().name.query_mut() = "Alpha".into();
        h.run_steps(3);
        h.get_by_label("Rename").click();
        h.run_steps(3);

        h.get_by_label("Alert: Beta").click();
        h.run_steps(3);
        assert!(!h.state().views().alerts["Beta"].active);
        h.state_mut().refresh();
        h.run_steps(3);
        let mut story = NewsStory::new("b2", Utc::now(), "Paused story");
        story.symbols = vec!["BBB".into()];
        for (_, reply) in std::mem::take(&mut *feed.0.lock().unwrap()) {
            reply.send(Ok(vec![story.clone()]));
        }
        h.run_steps(3);
        assert!(h.state_mut().take_notifications().is_empty());

        let mut restored = NewsPanel::new();
        restored.set_storage(documents.clone());
        restored.poll(&egui::Context::default());
        assert_eq!(restored.views().alerts, h.state().views().alerts);
        assert_eq!(restored.views().alerts["Alpha"].view.symbols, ["AAA"]);
        h.state_mut().refresh();
        h.run_steps(3);
        let pending = std::mem::take(&mut *feed.0.lock().unwrap());
        h.get_by_label("Delete alert Alpha").click();
        h.run_steps(3);
        for (_, reply) in pending {
            reply.send(Ok(stories.clone()));
        }
        h.run_steps(3);
        assert!(h.state_mut().take_notifications().is_empty());
        assert!(!h.state().views().alerts.contains_key("Alpha"));
        let mut restored = NewsPanel::new();
        restored.set_storage(documents);
        restored.poll(&egui::Context::default());
        assert_eq!(restored.views().alerts.len(), 1);
        assert!(!restored.views().alerts["Beta"].active);
    }

    /// Guards the quiet rule and reading an alert's stories: while the panel shows the
    /// selection an alert asks for, the alert's new stories join the list without a
    /// notification, and once the selection differs they notify; a notified story opens among
    /// the stories its alert reported, with Previous, Next and the arrow keys moving between
    /// them and Escape back to the list.
    #[test]
    fn a_shown_alert_stays_quiet_and_its_stories_read_in_turn() {
        let feed = Arc::new(Feed::default());
        let mut panel = NewsPanel::new();
        panel.set_symbol("AAA");
        panel.set_storage(Arc::new(Documents::default()));
        panel.set_feed(feed.clone());
        let mut h = Harness::builder()
            .with_context_setup(egui_extras::install_image_loaders)
            .build_ui_state(
                |ui, panel: &mut NewsPanel| {
                    panel.poll(ui.ctx());
                    panel.show(ui);
                },
                panel,
            );
        h.run_steps(3);
        *h.state_mut().name.query_mut() = "Alpha".into();
        h.run_steps(3);
        h.get_by_label("Create alert").click();
        h.run_steps(3);
        let time = Utc::now();
        let [first, second, third] =
            [(1, "First"), (2, "Second"), (3, "Third")].map(|(seconds, headline)| {
                let mut story = NewsStory::new(
                    headline.to_lowercase(),
                    time + chrono::Duration::seconds(seconds),
                    headline,
                );
                story.symbols = vec!["AAA".into()];
                story
            });
        let answer = |h: &mut Harness<'_, NewsPanel>, stories: &[NewsStory]| {
            h.run_steps(3);
            for (_, reply) in std::mem::take(&mut *feed.0.lock().unwrap()) {
                reply.send(Ok(stories.to_vec()));
            }
            h.run_steps(3);
        };
        h.state_mut().refresh();
        answer(&mut h, std::slice::from_ref(&first));
        assert!(h.state_mut().take_notifications().is_empty());
        h.get_by_role_and_label(egui::Role::Button, "First");

        h.get_by_label("Market").click();
        answer(&mut h, &[third, second.clone(), first]);
        let notifications = h.state_mut().take_notifications();
        let delivered: Vec<_> = notifications
            .iter()
            .map(|notification| (notification.body.as_str(), notification.target.clone()))
            .collect();
        assert_eq!(
            delivered,
            [
                ("Second", Some(NotificationTarget::Story("second".into()))),
                ("Third", Some(NotificationTarget::Story("third".into())))
            ]
        );

        h.state_mut().open_story(second);
        h.run_steps(3);
        let disabled = |h: &Harness<'_, NewsPanel>, label| {
            h.get_by_role_and_label(egui::Role::Button, label)
                .accesskit_node()
                .is_disabled()
        };
        h.get_by_label("Second");
        assert!(disabled(&h, "Previous"));
        assert!(!disabled(&h, "Next"));
        h.get_by_label("Next").click();
        h.run_steps(3);
        h.get_by_label("Third");
        assert!(disabled(&h, "Next"));
        h.key_press(egui::Key::ArrowLeft);
        h.run_steps(3);
        h.get_by_label("Second");
        h.key_press(egui::Key::Escape);
        h.run_steps(3);
        h.get_by_role_and_label(egui::Role::Button, "First");
        assert!(h.query_by_label("Previous").is_none());
    }
}
