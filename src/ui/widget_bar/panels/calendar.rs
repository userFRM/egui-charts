//! Economic, earnings, dividend and IPO calendars the feed supplies.

use crate::localization as l10n;
use std::sync::Arc;

use chrono::{DateTime, Datelike, Days, NaiveDate, NaiveTime, Utc};
use egui::{Label, RichText, Sense, Ui, vec2};

use super::ROW_HEIGHT;
use super::controls::{cell, dropdown};
use crate::data::inbox::Inbox;
use crate::data::{CalendarEvent, CalendarKind, CalendarQuery, Datafeed, FeedError, Importance};
use crate::icons::icons;
use crate::model::{TimezoneId, local_instant, local_time};
use crate::theme::ThemeExt;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::status_message::{MessageKind, status_message, translated_status_message};
use crate::ui_kit::{EmptyState, LoadingIndicator, PanelHeader};

/// The days a calendar lists, in the panel's time zone.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CalendarRange {
    Yesterday,
    Today,
    Tomorrow,
    /// Monday to Sunday of the preceding week.
    LastWeek,
    /// Monday to Sunday of the current week.
    ThisWeek,
    /// Monday to Sunday of the following week.
    NextWeek,
    /// From now through the next four weeks.
    Upcoming,
}

impl CalendarRange {
    const ALL: [CalendarRange; 7] = [
        CalendarRange::Yesterday,
        CalendarRange::Today,
        CalendarRange::Tomorrow,
        CalendarRange::LastWeek,
        CalendarRange::ThisWeek,
        CalendarRange::NextWeek,
        CalendarRange::Upcoming,
    ];

    fn label(self) -> &'static str {
        match self {
            CalendarRange::Yesterday => "Yesterday",
            CalendarRange::Today => "Today",
            CalendarRange::Tomorrow => "Tomorrow",
            CalendarRange::LastWeek => "Last week",
            CalendarRange::ThisWeek => "This week",
            CalendarRange::NextWeek => "Next week",
            CalendarRange::Upcoming => "Upcoming",
        }
    }

    /// The start of the first day and the start of the day after the last, around `now`, with
    /// days starting at midnight in `zone`; upcoming events start at `now`. A midnight the clock
    /// skips starts its day at the jump.
    fn bounds(self, now: DateTime<Utc>, zone: &TimezoneId) -> (DateTime<Utc>, DateTime<Utc>) {
        let today = local_time(zone.clone(), now).date();
        let start = |day: NaiveDate| {
            let midnight = day.and_time(NaiveTime::MIN);
            local_instant(zone.clone(), midnight).unwrap_or_else(|| midnight.and_utc())
        };
        let monday = today - Days::new(u64::from(today.weekday().num_days_from_monday()));
        let (first, days) = match self {
            CalendarRange::Yesterday => (today - Days::new(1), 1),
            CalendarRange::Today => (today, 1),
            CalendarRange::Tomorrow => (today + Days::new(1), 1),
            CalendarRange::LastWeek => (monday - Days::new(7), 7),
            CalendarRange::ThisWeek => (monday, 7),
            CalendarRange::NextWeek => (monday + Days::new(7), 7),
            CalendarRange::Upcoming => return (now, start(today + Days::new(29))),
        };
        (start(first), start(first + Days::new(days)))
    }
}

/// Company calendars keep only columns that the answer supplies, in this order.
#[derive(Clone, Copy)]
enum Figure {
    Actual,
    Forecast,
    Prior,
    Revenue,
    RevenueForecast,
    MarketCap,
    Surprise,
    Yield,
    PaymentDate,
    DealAmount,
    Shares,
    Exchange,
}

impl Figure {
    fn for_kind(kind: CalendarKind) -> &'static [Self] {
        use Figure::*;
        match kind {
            CalendarKind::Economic => &[Actual, Forecast, Prior],
            CalendarKind::Earnings => &[
                Actual,
                Forecast,
                Prior,
                Revenue,
                RevenueForecast,
                MarketCap,
                Surprise,
            ],
            CalendarKind::Dividends => &[Actual, Forecast, Prior, Yield, PaymentDate],
            CalendarKind::Ipos => &[
                Actual, Forecast, Prior, DealAmount, Shares, MarketCap, Exchange,
            ],
        }
    }

    fn label(self, kind: CalendarKind) -> &'static str {
        match (self, kind) {
            (Self::Actual, CalendarKind::Earnings) => "EPS",
            (Self::Forecast, CalendarKind::Earnings) => "EPS estimate",
            (Self::Prior, CalendarKind::Earnings) => "Prior EPS",
            (Self::Actual, CalendarKind::Dividends) => "Amount",
            (Self::Actual, CalendarKind::Ipos) => "Offer price",
            (Self::Actual, _) => "Actual",
            (Self::Forecast, _) => "Forecast",
            (Self::Prior, _) => "Prior",
            (Self::Revenue, _) => "Revenue",
            (Self::RevenueForecast, _) => "Revenue estimate",
            (Self::MarketCap, _) => "Market cap",
            (Self::Surprise, _) => "Surprise",
            (Self::Yield, _) => "Yield",
            (Self::PaymentDate, _) => "Payment date",
            (Self::DealAmount, _) => "Deal amount",
            (Self::Shares, _) => "Shares offered",
            (Self::Exchange, _) => "Exchange",
        }
    }

    fn value(self, event: &CalendarEvent) -> Option<&str> {
        match self {
            Self::Actual if event.kind == CalendarKind::Ipos => {
                event.actual.as_ref().or(event.forecast.as_ref())
            }
            Self::Actual => event.actual.as_ref(),
            Self::Forecast => event.forecast.as_ref(),
            Self::Prior => event.prior.as_ref(),
            Self::Revenue => event.revenue.as_ref(),
            Self::RevenueForecast => event.revenue_forecast.as_ref(),
            Self::MarketCap => event.market_cap.as_ref(),
            Self::Surprise => event.surprise.as_ref(),
            Self::Yield => event.dividend_yield.as_ref(),
            Self::PaymentDate => event.payment_date.as_ref(),
            Self::DealAmount => event.deal_amount.as_ref(),
            Self::Shares => event.shares_offered.as_ref(),
            Self::Exchange => event.exchange.as_ref(),
        }
        .map(String::as_str)
    }
}

/// An action the host performs for the calendar panel.
#[derive(Clone, Debug, PartialEq, Default)]
#[non_exhaustive]
pub enum CalendarAction {
    /// No host action.
    #[default]
    None,
    /// Show this company's symbol, from an earnings, dividend or IPO event.
    SelectSymbol(String),
}

/// Calendars fed by the host's [`Datafeed::calendar`], one at a time.
///
/// The user picks the calendar, the days, and for economic events the least importance; with
/// [`set_countries`](Self::set_countries) also the countries, and with
/// [`set_categories`](Self::set_categories) the categories of economic events. An event with a
/// description opens to it. Days start at midnight and times show in the zone given to
/// [`set_timezone`](Self::set_timezone), UTC until set, and the panel names it. Every change asks
/// the feed again, which applies the filters, and only the answer to the latest request is shown.
/// Call [`poll`](Self::poll) each frame, then [`show`](Self::show) while the panel is open.
pub struct CalendarPanel {
    kind: CalendarKind,
    range: CalendarRange,
    importance: Option<Importance>,
    countries: Vec<(String, bool)>,
    categories: Vec<(String, bool)>,
    timezone: TimezoneId,
    feed: Option<Arc<dyn Datafeed>>,
    events: Vec<CalendarEvent>,
    /// The event whose description shows.
    open: Option<String>,
    error: Option<FeedError>,
    loading: bool,
    inbox: Arc<Inbox>,
    request: u64,
}

impl std::fmt::Debug for CalendarPanel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CalendarPanel")
            .field("kind", &self.kind)
            .field("range", &self.range)
            .field("importance", &self.importance)
            .field("countries", &self.countries)
            .field("categories", &self.categories)
            .field("timezone", &self.timezone)
            .field("events", &self.events)
            .field("open", &self.open)
            .field("error", &self.error)
            .field("loading", &self.loading)
            .field("request", &self.request)
            .finish_non_exhaustive()
    }
}

impl Default for CalendarPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl CalendarPanel {
    /// This week's economic calendar of every country, without a feed.
    pub fn new() -> Self {
        Self {
            kind: CalendarKind::Economic,
            range: CalendarRange::ThisWeek,
            importance: None,
            countries: Vec::new(),
            categories: Vec::new(),
            timezone: TimezoneId::utc(),
            feed: None,
            events: Vec::new(),
            open: None,
            error: None,
            loading: false,
            inbox: Arc::default(),
            request: 0,
        }
    }

    /// Ask `feed` for events from now on, starting at once.
    pub fn set_feed(&mut self, feed: Arc<dyn Datafeed>) {
        self.feed = Some(feed);
        self.refresh();
    }

    /// Offer `countries`, in the codes the feed uses, as a filter. None is selected at first,
    /// which lists every country.
    pub fn set_countries(&mut self, countries: impl IntoIterator<Item = String>) {
        self.countries = countries.into_iter().map(|code| (code, false)).collect();
        self.refresh();
    }

    /// Offer `categories` of economic events, in the values the feed uses, as a filter. None is
    /// selected at first, which lists every category.
    pub fn set_categories(&mut self, categories: impl IntoIterator<Item = String>) {
        self.categories = categories
            .into_iter()
            .map(|category| (category, false))
            .collect();
        self.refresh();
    }

    /// Start days at midnight in `timezone` and show event days and times in it, such as the
    /// chart's. UTC until set. A different zone asks the feed again.
    pub fn set_timezone(&mut self, timezone: TimezoneId) {
        if self.timezone != timezone {
            self.timezone = timezone;
            self.refresh();
        }
    }

    /// Ask the feed again, such as when the host learns of new figures.
    pub fn refresh(&mut self) {
        let Some(feed) = &self.feed else {
            return;
        };
        self.request += 1;
        self.loading = true;
        let (from, to) = self.range.bounds(Utc::now(), &self.timezone);
        let chosen = |choices: &[(String, bool)]| -> Vec<String> {
            choices
                .iter()
                .filter(|(_, on)| *on)
                .map(|(value, _)| value.clone())
                .collect()
        };
        let economic = self.kind == CalendarKind::Economic;
        let query = CalendarQuery {
            kind: self.kind,
            from,
            to,
            countries: chosen(&self.countries),
            categories: if economic {
                chosen(&self.categories)
            } else {
                Vec::new()
            },
            importance: self.importance.filter(|_| economic),
        };
        feed.calendar(query, self.inbox.reply(self.request));
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
                .downcast::<Result<Vec<CalendarEvent>, FeedError>>()
                .expect("calendar")
            {
                Ok(events) => {
                    self.events = events;
                    self.error = None;
                    if self
                        .open
                        .as_ref()
                        .is_some_and(|id| !self.events.iter().any(|event| event.id == *id))
                    {
                        self.open = None;
                    }
                }
                Err(error) => {
                    self.events.clear();
                    self.open = None;
                    self.error = Some(error);
                }
            }
        }
    }

    /// Render the filters and the events by day after [`poll`](Self::poll).
    pub fn show(&mut self, ui: &mut Ui) -> CalendarAction {
        let mut action = CalendarAction::None;
        let theme = ui.active_theme();
        PanelHeader::new("Calendars").show_plain(ui);
        let before = (
            self.kind,
            self.range,
            self.importance,
            self.countries.clone(),
            self.categories.clone(),
        );
        ui.horizontal_wrapped(|ui| {
            for (kind, label) in [
                (CalendarKind::Economic, "Economic"),
                (CalendarKind::Earnings, "Earnings"),
                (CalendarKind::Dividends, "Dividends"),
                (CalendarKind::Ipos, "IPOs"),
            ] {
                if Button::new(label)
                    .small()
                    .selected(self.kind == kind)
                    .show(ui)
                    .clicked()
                {
                    self.kind = kind;
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            dropdown(ui, "calendar_range", self.range.label(), |ui| {
                for range in CalendarRange::ALL {
                    ui.selectable_value(
                        &mut self.range,
                        range,
                        l10n::text(ui.ctx(), range.label()),
                    );
                }
            });
            if self.kind == CalendarKind::Economic {
                dropdown(
                    ui,
                    "calendar_importance",
                    importance_label(self.importance),
                    |ui| {
                        for importance in [None, Some(Importance::Medium), Some(Importance::High)] {
                            ui.selectable_value(
                                &mut self.importance,
                                importance,
                                l10n::text(ui.ctx(), importance_label(importance)),
                            );
                        }
                    },
                );
                if !self.categories.is_empty() {
                    let chosen = self.categories.iter().filter(|(_, on)| *on).count();
                    let text = if chosen == 0 {
                        "All categories".to_owned()
                    } else {
                        l10n::format_text(
                            ui.ctx(),
                            "{count} categories",
                            &[("count", chosen.to_string().as_str())],
                        )
                    };
                    dropdown(ui, "calendar_categories", &text, |ui| {
                        for (category, on) in &mut self.categories {
                            ui.checkbox(on, l10n::text(ui.ctx(), category.as_str()));
                        }
                    });
                }
            }
            ui.label(
                RichText::new(l10n::format_text(
                    ui.ctx(),
                    "Times in {zone}",
                    &[("zone", self.timezone.name())],
                ))
                .size(theme.text.small)
                .color(theme.ui.text_secondary),
            );
        });
        if !self.countries.is_empty() {
            let chosen = self.countries.iter().filter(|(_, on)| *on).count();
            let text = if chosen == 0 {
                "All countries".to_owned()
            } else {
                l10n::format_text(
                    ui.ctx(),
                    "{count} countries",
                    &[("count", chosen.to_string().as_str())],
                )
            };
            dropdown(ui, "calendar_countries", &text, |ui| {
                for (code, on) in &mut self.countries {
                    ui.checkbox(on, l10n::text(ui.ctx(), code.as_str()));
                }
            });
        }
        if before
            != (
                self.kind,
                self.range,
                self.importance,
                self.countries.clone(),
                self.categories.clone(),
            )
        {
            if before.0 != self.kind {
                self.events.clear();
                self.open = None;
            }
            self.refresh();
        }
        match &self.error {
            Some(FeedError::NotSupported) => {
                status_message(ui, MessageKind::Info, "This feed supplies no calendar.");
            }
            Some(error) => {
                translated_status_message(
                    ui,
                    MessageKind::Error,
                    l10n::format_text(
                        ui.ctx(),
                        "Events unavailable: {error}",
                        &[("error", error.to_string().as_str())],
                    ),
                );
            }
            None if self.loading && self.events.is_empty() => {
                LoadingIndicator::new().message("Loading events").show(ui);
            }
            None if self.events.is_empty() => {
                EmptyState::new("No events")
                    .icon(&icons::WIDGET_BAR_CALENDAR)
                    .fill_height()
                    .show(ui);
            }
            None => {}
        }
        let mut toggled = None;
        let columns: Vec<_> = Figure::for_kind(self.kind)
            .iter()
            .copied()
            .filter(|figure| {
                self.kind == CalendarKind::Economic
                    || self
                        .events
                        .iter()
                        .any(|event| figure.value(event).is_some())
            })
            .collect();
        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if columns.len() > 3 {
                    ui.set_min_width(columns.len() as f32 * 112.0);
                }
                let count = columns.len();
                let figures = |rect: egui::Rect| {
                    let rect = rect.shrink2(vec2(8.0, 0.0));
                    (0..count)
                        .map(move |i| {
                            rect.with_min_x(rect.left() + rect.width() * i as f32 / count as f32)
                                .with_max_x(
                                    rect.left() + rect.width() * (i + 1) as f32 / count as f32
                                        - 4.0,
                                )
                        })
                        .collect::<Vec<_>>()
                };
                let mut day = None;
                for event in &self.events {
                    let local = local_time(self.timezone.clone(), event.time);
                    if day != Some(local.date()) {
                        day = Some(local.date());
                        ui.add_space(theme.spacing.sm);
                        ui.label(
                            RichText::new(l10n::text(
                                ui.ctx(),
                                local.format("%A, %b %-d").to_string(),
                            ))
                            .size(theme.text.small)
                            .strong()
                            .color(theme.ui.text_secondary),
                        );
                        ui.separator();
                        if !columns.is_empty() {
                            let (header, _) = ui.allocate_exact_size(
                                vec2(ui.available_width(), ROW_HEIGHT),
                                Sense::hover(),
                            );
                            for (rect, title) in figures(header).into_iter().zip(&columns) {
                                cell(
                                    ui,
                                    rect,
                                    title.label(self.kind),
                                    true,
                                    theme.ui.text_secondary,
                                );
                            }
                        }
                    }
                    let opens = event.description.is_some();
                    let clickable = opens || event.symbol.is_some();
                    let sense = if clickable {
                        Sense::click()
                    } else {
                        Sense::hover()
                    };
                    let (rect, response) = ui.allocate_exact_size(
                        vec2(
                            ui.available_width(),
                            if columns.is_empty() {
                                ROW_HEIGHT
                            } else {
                                2.0 * ROW_HEIGHT
                            },
                        ),
                        sense,
                    );
                    if response.hovered() {
                        ui.painter().rect_filled(rect, 0.0, theme.ui.btn_bg_hover);
                    }
                    let metadata = rect
                        .with_max_y(rect.top() + ROW_HEIGHT)
                        .shrink2(vec2(8.0, 0.0));
                    let time = metadata.with_max_x(metadata.left() + 42.0);
                    let country = metadata
                        .with_min_x(time.right())
                        .with_max_x(time.right() + 28.0);
                    let importance = metadata
                        .with_min_x(country.right())
                        .with_max_x(country.right() + 32.0);
                    let title_rect = metadata.with_min_x(importance.right());
                    cell(
                        ui,
                        time,
                        &local.format("%H:%M").to_string(),
                        false,
                        theme.ui.text_secondary,
                    );
                    cell(ui, country, &event.country, false, theme.ui.text_secondary);
                    if let Some(level) = event.importance {
                        let color = match level {
                            Importance::High => theme.ui.error,
                            Importance::Medium => theme.ui.warning,
                            Importance::Low => theme.ui.text_secondary,
                        };
                        cell(
                            ui,
                            importance,
                            &"●".repeat(level as usize + 1),
                            false,
                            color,
                        )
                        .on_hover_text(l10n::text(
                            ui.ctx(),
                            match level {
                                Importance::Low => "Low importance",
                                Importance::Medium => "Medium importance",
                                Importance::High => "High importance",
                            },
                        ));
                    }
                    let title = match &event.symbol {
                        Some(symbol) => format!("{symbol} · {}", event.title),
                        None => event.title.clone(),
                    };
                    cell(ui, title_rect, &title, false, theme.ui.text);
                    for (rect, value) in figures(rect.with_min_y(rect.top() + ROW_HEIGHT))
                        .into_iter()
                        .zip(&columns)
                    {
                        cell(
                            ui,
                            rect,
                            value.value(event).unwrap_or("-"),
                            true,
                            theme.ui.text,
                        );
                    }
                    let open = opens && self.open.as_ref() == Some(&event.id);
                    if clickable {
                        let translated_label = l10n::text(ui.ctx(), &title);
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Button, true, &translated_label)
                        });
                        if opens {
                            ui.ctx().accesskit_node_builder(response.id, |node| {
                                node.set_expanded(open)
                            });
                        }
                        if response
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            if opens {
                                toggled = Some(event.id.clone());
                            } else if let Some(symbol) = &event.symbol {
                                action = CalendarAction::SelectSymbol(symbol.clone());
                            }
                        }
                    }
                    if open && let Some(description) = &event.description {
                        egui::Frame::new()
                            .inner_margin(egui::Margin::symmetric(8, 4))
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing.y = theme.spacing.xs;
                                if let Some(category) = &event.category {
                                    ui.label(
                                        RichText::new(l10n::text(ui.ctx(), category))
                                            .size(theme.text.small)
                                            .strong()
                                            .color(theme.ui.text_secondary),
                                    );
                                }
                                ui.add(
                                    Label::new(
                                        RichText::new(l10n::text(ui.ctx(), description))
                                            .size(theme.text.small)
                                            .color(theme.ui.text_secondary),
                                    )
                                    .wrap(),
                                );
                                if let Some(symbol) = &event.symbol
                                    && Button::new("Show symbol").small().show(ui).clicked()
                                {
                                    action = CalendarAction::SelectSymbol(symbol.clone());
                                }
                            });
                    }
                }
            });
        if let Some(id) = toggled {
            self.open = (self.open.as_ref() != Some(&id)).then_some(id);
        }
        action
    }
}

fn importance_label(importance: Option<Importance>) -> &'static str {
    match importance {
        None | Some(Importance::Low) => "All importance",
        Some(Importance::Medium) => "Medium and high",
        Some(Importance::High) => "High",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::{HistoryPage, HistoryRequest, Reply};
    use egui_kittest::{Harness, kittest::Queryable};
    use std::sync::Mutex;

    #[derive(Default)]
    struct Feed(Mutex<Vec<(CalendarQuery, Reply<Vec<CalendarEvent>>)>>);

    impl Datafeed for Feed {
        fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
            reply.send(Err(FeedError::NotSupported));
        }
        fn calendar(&self, query: CalendarQuery, reply: Reply<Vec<CalendarEvent>>) {
            self.0.lock().unwrap().push((query, reply));
        }
    }

    /// The texts one frame of `panel` paints, with their visible bounds.
    fn texts(panel: &mut CalendarPanel, ctx: &egui::Context) -> Vec<(String, egui::Rect)> {
        let output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(280.0, 400.0),
                )),
                ..Default::default()
            },
            |ui| {
                panel.show(ui);
            },
        );
        let texts = output
            .shapes
            .iter()
            .filter_map(|shape| {
                let egui::Shape::Text(text) = &shape.shape else {
                    return None;
                };
                Some((
                    text.galley.text().to_owned(),
                    text.visual_bounding_rect().intersect(shape.clip_rect),
                ))
            })
            .collect();
        output.drop_without_applying_deltas();
        texts
    }

    /// Missing figures keep their columns, and event metadata fits on one line.
    #[test]
    fn event_figures_align_under_their_headings() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut panel = CalendarPanel::new();
            let mut event = CalendarEvent::new(
                "release",
                CalendarKind::Economic,
                "2026-10-21T12:30:00Z".parse().unwrap(),
                "Output",
            );
            event.country = "US".into();
            event.importance = Some(Importance::High);
            event.actual = Some("1.2%".into());
            event.prior = Some("10.5%".into());
            panel.events.push(event);
            let texts = texts(&mut panel, &ctx);
            let bounds = |label: &str| {
                texts
                    .iter()
                    .find(|(text, _)| text == label)
                    .unwrap_or_else(|| panic!("missing {label}: {texts:?}"))
                    .1
            };
            for (value, heading) in [("1.2%", "Actual"), ("10.5%", "Prior")] {
                assert!(
                    (bounds(value).right() - bounds(heading).right()).abs() < 2.0,
                    "{texts:?}"
                );
            }
            for label in ["US", "Output"] {
                assert!(
                    (bounds(label).center().y - bounds("12:30").center().y).abs() < 2.0,
                    "{texts:?}"
                );
            }
            assert!(bounds("1.2%").right() < bounds("10.5%").left());
        }
    }

    /// Guards the days each range asks for: whole days from midnight in the panel's zone,
    /// including a midnight the clock skips and a week with a clock change, and weeks from Monday,
    /// so a Sunday belongs to the week that ends with it. Worked by hand from a calendar and the
    /// zones' 2026 rules: 2026-10-21 is a Wednesday and 2026-10-25 a Sunday, New York leaves
    /// daylight saving time on 2026-11-01 and Santiago skips its midnight on 2026-09-06.
    #[test]
    fn ranges_cover_whole_local_days_and_monday_weeks() {
        let at = |text: &str| text.parse::<DateTime<Utc>>().unwrap();
        for (zone, now, range, from, to) in [
            (
                "UTC",
                "2026-10-21T15:30:00Z",
                CalendarRange::LastWeek,
                "2026-10-12T00:00:00Z",
                "2026-10-19T00:00:00Z",
            ),
            (
                "America/New_York",
                "2026-11-04T12:00:00Z",
                CalendarRange::LastWeek,
                "2026-10-26T04:00:00Z",
                "2026-11-02T05:00:00Z",
            ),
            (
                "UTC",
                "2026-10-21T15:30:00Z",
                CalendarRange::Yesterday,
                "2026-10-20T00:00:00Z",
                "2026-10-21T00:00:00Z",
            ),
            (
                "UTC",
                "2026-10-21T15:30:00Z",
                CalendarRange::Today,
                "2026-10-21T00:00:00Z",
                "2026-10-22T00:00:00Z",
            ),
            (
                "UTC",
                "2026-10-21T15:30:00Z",
                CalendarRange::Tomorrow,
                "2026-10-22T00:00:00Z",
                "2026-10-23T00:00:00Z",
            ),
            (
                "UTC",
                "2026-10-21T15:30:00Z",
                CalendarRange::ThisWeek,
                "2026-10-19T00:00:00Z",
                "2026-10-26T00:00:00Z",
            ),
            (
                "UTC",
                "2026-10-21T15:30:00Z",
                CalendarRange::NextWeek,
                "2026-10-26T00:00:00Z",
                "2026-11-02T00:00:00Z",
            ),
            (
                "UTC",
                "2026-10-25T23:59:59Z",
                CalendarRange::ThisWeek,
                "2026-10-19T00:00:00Z",
                "2026-10-26T00:00:00Z",
            ),
            (
                "UTC",
                "2026-10-26T00:00:00Z",
                CalendarRange::ThisWeek,
                "2026-10-26T00:00:00Z",
                "2026-11-02T00:00:00Z",
            ),
            (
                "UTC",
                "2026-12-31T08:00:00Z",
                CalendarRange::NextWeek,
                "2027-01-04T00:00:00Z",
                "2027-01-11T00:00:00Z",
            ),
            // Wednesday 21:00 in New York is already Thursday in UTC.
            (
                "America/New_York",
                "2026-10-22T01:00:00Z",
                CalendarRange::Today,
                "2026-10-21T04:00:00Z",
                "2026-10-22T04:00:00Z",
            ),
            // The week ends an hour later in UTC, after the clocks go back.
            (
                "America/New_York",
                "2026-10-28T12:00:00Z",
                CalendarRange::ThisWeek,
                "2026-10-26T04:00:00Z",
                "2026-11-02T05:00:00Z",
            ),
            // The clock jumps from 00:00 to 01:00, so the day starts at 01:00 and lasts 23 hours.
            (
                "America/Santiago",
                "2026-09-05T16:00:00Z",
                CalendarRange::Tomorrow,
                "2026-09-06T04:00:00Z",
                "2026-09-07T03:00:00Z",
            ),
            // 01:30 on Thursday at +05:30.
            (
                "+05:30",
                "2026-10-21T20:00:00Z",
                CalendarRange::Today,
                "2026-10-21T18:30:00Z",
                "2026-10-22T18:30:00Z",
            ),
            // From now until the end of the 28th day after today: October 21 plus 29 days is
            // November 19, when New York is five hours behind UTC.
            (
                "UTC",
                "2026-10-21T15:30:00Z",
                CalendarRange::Upcoming,
                "2026-10-21T15:30:00Z",
                "2026-11-19T00:00:00Z",
            ),
            (
                "America/New_York",
                "2026-10-22T01:00:00Z",
                CalendarRange::Upcoming,
                "2026-10-22T01:00:00Z",
                "2026-11-19T05:00:00Z",
            ),
        ] {
            let Some(timezone) = TimezoneId::named(zone) else {
                #[cfg(feature = "timezones")]
                panic!("unknown time zone {zone}");
                #[cfg(not(feature = "timezones"))]
                {
                    assert!(zone.contains('/'), "unknown fixed time zone {zone}");
                    continue;
                }
            };
            assert_eq!(
                range.bounds(at(now), &timezone),
                (at(from), at(to)),
                "{range:?} at {now} in {zone}"
            );
        }
    }

    /// Guards the zone the list is written in: event times and day headings follow it, so a
    /// release at 12:30 UTC lists at 08:30 in New York and an evening release stays on its local
    /// day, and the panel names the zone.
    #[cfg(feature = "timezones")]
    #[test]
    fn new_york_lists_its_local_day_and_local_times() {
        let ctx = egui::Context::default();
        let mut panel = CalendarPanel::new();
        panel.set_timezone("America/New_York".parse().unwrap());
        for (id, time) in [
            ("CPI", "2026-10-21T12:30:00Z"),
            ("Late", "2026-10-22T01:00:00Z"),
        ] {
            panel.events.push(CalendarEvent::new(
                id,
                CalendarKind::Economic,
                time.parse().unwrap(),
                id,
            ));
        }
        let texts = texts(&mut panel, &ctx);
        let count = |label: &str| texts.iter().filter(|(text, _)| text == label).count();
        for (label, shown) in [
            ("08:30", 1),
            ("21:00", 1),
            ("Wednesday, Oct 21", 1),
            ("Thursday, Oct 22", 0),
            ("Times in America/New_York", 1),
        ] {
            assert_eq!(count(label), shown, "{label}: {texts:?}");
        }
    }

    /// Guards what the user narrows and opens: a ticked category goes out with economic requests
    /// only, Upcoming asks from now through the next four weeks, and an event with a description
    /// opens to it and its category and closes again.
    #[test]
    fn categories_upcoming_and_descriptions_reach_the_query_and_show() {
        let feed = Arc::new(Feed::default());
        let mut panel = CalendarPanel::new();
        panel.set_categories(["Inflation".to_owned(), "Employment".to_owned()]);
        panel.set_feed(feed.clone());
        let latest = || feed.0.lock().unwrap().last().unwrap().0.clone();
        assert!(latest().categories.is_empty());
        let mut h = Harness::builder()
            .with_size(egui::vec2(320.0, 560.0))
            .with_context_setup(egui_extras::install_image_loaders)
            .build_ui_state(
                |ui, panel: &mut CalendarPanel| {
                    panel.poll(ui.ctx());
                    panel.show(ui);
                },
                panel,
            );
        // The loading spinner asks for a frame after every frame, so step a few at a time.
        h.run_steps(3);
        for label in ["All categories", "Inflation"] {
            h.get_by_label(label).click();
            h.run_steps(3);
        }
        let query = latest();
        assert_eq!(query.kind, CalendarKind::Economic);
        assert_eq!(query.categories, ["Inflation"]);
        h.get_by_label("1 categories");

        for label in ["This week", "Upcoming"] {
            h.get_by_label(label).click();
            h.run_steps(3);
        }
        let query = latest();
        let span = query.to - query.from;
        assert!(query.from <= Utc::now(), "{query:?}");
        assert!(span > chrono::Duration::days(28) && span <= chrono::Duration::days(29));

        let description = "Prices paid by consumers, without food and energy.";
        let mut event = CalendarEvent::new(
            "cpi",
            CalendarKind::Economic,
            query.from + chrono::Duration::hours(1),
            "Core CPI",
        );
        event.category = Some("Inflation".into());
        event.description = Some(description.into());
        let (_, reply) = feed.0.lock().unwrap().pop().unwrap();
        reply.send(Ok(vec![event]));
        h.run_steps(3);
        assert!(
            h.query_by_role_and_label(egui::Role::Label, description)
                .is_none()
        );
        for shown in [true, false] {
            h.get_by_role_and_label(egui::Role::Button, "Core CPI")
                .click();
            h.run_steps(3);
            for value in [description, "Inflation"] {
                assert_eq!(
                    h.query_by_role_and_label(egui::Role::Label, value)
                        .is_some(),
                    shown,
                    "{value}"
                );
            }
        }

        h.get_by_label("Earnings").click();
        h.run_steps(3);
        let query = latest();
        assert_eq!(query.kind, CalendarKind::Earnings);
        assert!(query.categories.is_empty());
        assert!(
            h.state().events.is_empty(),
            "old figures wait for their own calendar kind"
        );
        assert!(h.query_by_label("1 categories").is_none());
    }

    /// Each company calendar names its own figures, keeps a partially missing column aligned
    /// and hides columns missing from the whole answer. Economic columns remain unchanged.
    #[test]
    fn calendar_kinds_show_their_figures_and_omit_empty_columns() {
        for (kind, expected, absent) in [
            (
                CalendarKind::Economic,
                vec![("Actual", "1.2"), ("Forecast", "1.1"), ("Prior", "1.0")],
                vec!["Revenue", "Offer price"],
            ),
            (
                CalendarKind::Earnings,
                vec![
                    ("EPS", "1.2"),
                    ("EPS estimate", "1.1"),
                    ("Prior EPS", "1.0"),
                    ("Revenue", "12B"),
                    ("Revenue estimate", "11B"),
                    ("Market cap", "120B"),
                    ("Surprise", "+9.09%"),
                ],
                vec!["Actual", "Forecast", "Yield"],
            ),
            (
                CalendarKind::Dividends,
                vec![
                    ("Amount", "1.2"),
                    ("Forecast", "1.1"),
                    ("Prior", "1.0"),
                    ("Yield", "3.2%"),
                    ("Payment date", "Nov 30, 2026"),
                ],
                vec!["Revenue"],
            ),
            (
                CalendarKind::Ipos,
                vec![
                    ("Offer price", "1.2"),
                    ("Forecast", "1.1"),
                    ("Prior", "1.0"),
                    ("Deal amount", "500M"),
                    ("Shares offered", "25M"),
                    ("Market cap", "120B"),
                    ("Exchange", "XNAS"),
                ],
                vec!["Actual", "Revenue"],
            ),
        ] {
            let mut panel = CalendarPanel::new();
            panel.kind = kind;
            let mut event =
                CalendarEvent::new("report", kind, DateTime::UNIX_EPOCH, "Company report");
            event.actual = Some("1.2".into());
            event.forecast = Some("1.1".into());
            event.prior = Some("1.0".into());
            event.revenue = Some("12B".into());
            event.revenue_forecast = Some("11B".into());
            event.market_cap = Some("120B".into());
            event.surprise = Some("+9.09%".into());
            event.dividend_yield = Some("3.2%".into());
            event.payment_date = Some("Nov 30, 2026".into());
            event.deal_amount = Some("500M".into());
            event.shares_offered = Some("25M".into());
            event.exchange = Some("XNAS".into());
            panel.events = vec![
                event,
                CalendarEvent::new("unknown", kind, DateTime::UNIX_EPOCH, "Not yet reported"),
            ];
            let mut h = Harness::builder()
                .with_size(vec2(1100.0, 560.0))
                .with_context_setup(egui_extras::install_image_loaders)
                .build_ui_state(
                    |ui, panel: &mut CalendarPanel| {
                        panel.show(ui);
                    },
                    panel,
                );
            h.run();
            for (heading, value) in expected {
                h.get_by_role_and_label(egui::Role::Label, heading);
                h.get_by_role_and_label(egui::Role::Label, value);
            }
            for heading in absent {
                assert!(
                    h.query_by_role_and_label(egui::Role::Label, heading)
                        .is_none(),
                    "{kind:?}: {heading}"
                );
            }
            assert!(h.query_all_by_value("-").next().is_some());
            h.state_mut().events.remove(0);
            h.run();
            if kind != CalendarKind::Economic {
                for figure in Figure::for_kind(kind) {
                    assert!(
                        h.query_by_role_and_label(egui::Role::Label, figure.label(kind))
                            .is_none()
                    );
                }
            }
        }
    }
}
