//! Economic, earnings, dividend and IPO calendars the feed supplies.

use crate::localization as l10n;
use std::sync::Arc;

use chrono::{DateTime, Datelike, Days, NaiveTime, Utc};
use egui::{RichText, Sense, Ui, vec2};

use super::ROW_HEIGHT;
use super::controls::{cell, dropdown};
use crate::data::inbox::Inbox;
use crate::data::{CalendarEvent, CalendarKind, CalendarQuery, Datafeed, FeedError, Importance};
use crate::icons::icons;
use crate::theme::ThemeExt;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::status_message::{MessageKind, status_message};
use crate::ui_kit::{EmptyState, LoadingIndicator, PanelHeader};

/// The days a calendar lists, in UTC.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CalendarRange {
    Yesterday,
    Today,
    Tomorrow,
    /// Monday to Sunday of the current week.
    ThisWeek,
    /// Monday to Sunday of the following week.
    NextWeek,
}

impl CalendarRange {
    const ALL: [CalendarRange; 5] = [
        CalendarRange::Yesterday,
        CalendarRange::Today,
        CalendarRange::Tomorrow,
        CalendarRange::ThisWeek,
        CalendarRange::NextWeek,
    ];

    fn label(self) -> &'static str {
        match self {
            CalendarRange::Yesterday => "Yesterday",
            CalendarRange::Today => "Today",
            CalendarRange::Tomorrow => "Tomorrow",
            CalendarRange::ThisWeek => "This week",
            CalendarRange::NextWeek => "Next week",
        }
    }

    /// The start of the first day and the start of the day after the last, around `now`.
    fn bounds(self, now: DateTime<Utc>) -> (DateTime<Utc>, DateTime<Utc>) {
        let today = now.date_naive();
        let monday = today - Days::new(u64::from(today.weekday().num_days_from_monday()));
        let (first, days) = match self {
            CalendarRange::Yesterday => (today - Days::new(1), 1),
            CalendarRange::Today => (today, 1),
            CalendarRange::Tomorrow => (today + Days::new(1), 1),
            CalendarRange::ThisWeek => (monday, 7),
            CalendarRange::NextWeek => (monday + Days::new(7), 7),
        };
        let start = |day: chrono::NaiveDate| day.and_time(NaiveTime::MIN).and_utc();
        (start(first), start(first + Days::new(days)))
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
/// [`set_countries`](Self::set_countries) also the countries. Every change asks the feed again,
/// which applies the filters, and only the answer to the latest request is shown. Call
/// [`poll`](Self::poll) each frame, then [`show`](Self::show) while the panel is open.
pub struct CalendarPanel {
    kind: CalendarKind,
    range: CalendarRange,
    importance: Option<Importance>,
    countries: Vec<(String, bool)>,
    feed: Option<Arc<dyn Datafeed>>,
    events: Vec<CalendarEvent>,
    error: Option<FeedError>,
    loading: bool,
    inbox: Arc<Inbox>,
    request: u64,
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
            feed: None,
            events: Vec::new(),
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

    /// Ask the feed again, such as when the host learns of new figures.
    pub fn refresh(&mut self) {
        let Some(feed) = &self.feed else {
            return;
        };
        self.request += 1;
        self.loading = true;
        let (from, to) = self.range.bounds(Utc::now());
        let query = CalendarQuery {
            kind: self.kind,
            from,
            to,
            countries: self
                .countries
                .iter()
                .filter(|(_, on)| *on)
                .map(|(code, _)| code.clone())
                .collect(),
            importance: self
                .importance
                .filter(|_| self.kind == CalendarKind::Economic),
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
                }
                Err(error) => {
                    self.events.clear();
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
            }
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
            )
        {
            self.refresh();
        }
        match &self.error {
            Some(FeedError::NotSupported) => {
                status_message(ui, MessageKind::Info, "This feed supplies no calendar.");
            }
            Some(error) => {
                status_message(
                    ui,
                    MessageKind::Error,
                    &l10n::format_text(
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
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let figures = |rect: egui::Rect| {
                    let rect = rect.shrink2(vec2(8.0, 0.0));
                    std::array::from_fn::<_, 3, _>(|i| {
                        rect.with_min_x(rect.left() + rect.width() * i as f32 / 3.0)
                            .with_max_x(rect.left() + rect.width() * (i + 1) as f32 / 3.0 - 4.0)
                    })
                };
                let mut day = None;
                for event in &self.events {
                    if day != Some(event.time.date_naive()) {
                        day = Some(event.time.date_naive());
                        ui.add_space(theme.spacing.sm);
                        ui.label(
                            RichText::new(l10n::text(
                                ui.ctx(),
                                event.time.format("%A, %b %-d").to_string(),
                            ))
                            .size(theme.text.small)
                            .strong()
                            .color(theme.ui.text_secondary),
                        );
                        ui.separator();
                        let (header, _) = ui.allocate_exact_size(
                            vec2(ui.available_width(), ROW_HEIGHT),
                            Sense::hover(),
                        );
                        for (rect, title) in figures(header)
                            .into_iter()
                            .zip(["Actual", "Forecast", "Prior"])
                        {
                            cell(ui, rect, title, true, theme.ui.text_secondary);
                        }
                    }
                    let sense = if event.symbol.is_some() {
                        Sense::click()
                    } else {
                        Sense::hover()
                    };
                    let (rect, response) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 2.0 * ROW_HEIGHT), sense);
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
                        &event.time.format("%H:%M").to_string(),
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
                        .zip([&event.actual, &event.forecast, &event.prior])
                    {
                        cell(
                            ui,
                            rect,
                            value.as_deref().unwrap_or("-"),
                            true,
                            theme.ui.text,
                        );
                    }
                    if let Some(symbol) = &event.symbol {
                        let translated_label = l10n::text(ui.ctx(), &title);
                        response.widget_info(|| {
                            egui::WidgetInfo::labeled(egui::Role::Button, true, &translated_label)
                        });
                        if response
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
                            action = CalendarAction::SelectSymbol(symbol.clone());
                        }
                    }
                }
            });
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
            let texts: Vec<_> = output
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

    /// Guards the days each range asks for: whole UTC days from midnight, weeks from Monday, so a
    /// Sunday belongs to the week that ends with it. Worked by hand from a calendar: 2026-10-21 is
    /// a Wednesday and 2026-10-25 a Sunday.
    #[test]
    fn ranges_cover_whole_utc_days_and_monday_weeks() {
        let at = |text: &str| text.parse::<DateTime<Utc>>().unwrap();
        for (now, range, from, to) in [
            (
                "2026-10-21T15:30:00Z",
                CalendarRange::Yesterday,
                "2026-10-20T00:00:00Z",
                "2026-10-21T00:00:00Z",
            ),
            (
                "2026-10-21T15:30:00Z",
                CalendarRange::Today,
                "2026-10-21T00:00:00Z",
                "2026-10-22T00:00:00Z",
            ),
            (
                "2026-10-21T15:30:00Z",
                CalendarRange::Tomorrow,
                "2026-10-22T00:00:00Z",
                "2026-10-23T00:00:00Z",
            ),
            (
                "2026-10-21T15:30:00Z",
                CalendarRange::ThisWeek,
                "2026-10-19T00:00:00Z",
                "2026-10-26T00:00:00Z",
            ),
            (
                "2026-10-21T15:30:00Z",
                CalendarRange::NextWeek,
                "2026-10-26T00:00:00Z",
                "2026-11-02T00:00:00Z",
            ),
            (
                "2026-10-25T23:59:59Z",
                CalendarRange::ThisWeek,
                "2026-10-19T00:00:00Z",
                "2026-10-26T00:00:00Z",
            ),
            (
                "2026-10-26T00:00:00Z",
                CalendarRange::ThisWeek,
                "2026-10-26T00:00:00Z",
                "2026-11-02T00:00:00Z",
            ),
            (
                "2026-12-31T08:00:00Z",
                CalendarRange::NextWeek,
                "2027-01-04T00:00:00Z",
                "2027-01-11T00:00:00Z",
            ),
        ] {
            assert_eq!(
                range.bounds(at(now)),
                (at(from), at(to)),
                "{range:?} at {now}"
            );
        }
    }
}
