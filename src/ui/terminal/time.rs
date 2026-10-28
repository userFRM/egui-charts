//! Time: intervals, date ranges, go to date, the time zone and the session.

use super::core::Direction;
use super::layout::{Region, panel_frame};
use super::{Action, Terminal};
use crate::Chart;
use crate::config::{KeyboardAction, TimezoneMode};
use crate::localization as l10n;
use crate::model::{
    Bar, BoxSizeMethod, ChartType, DateRange, Session, Timeframe, TimezoneId, local_instant,
};
use crate::services::UserSettings;
use crate::ui::Control;
use crate::ui::floating_replay_control::ReplayControlAction;
use crate::ui::timeframe_toolbar::{
    DatePicker, DatePickerAction, SessionType, TimeframeToolbar, TimeframeToolbarAction,
};
use crate::ui::top_toolbar::TopToolbarAction;
use crate::ui_kit::dialog::DialogFooter;
use crate::ui_kit::modal::Modal;
use crate::widget::{SeriesId, ShortcutRequest, VisibleTimeRange};
use chrono::{DateTime, Datelike, Duration, Months, NaiveDate, NaiveTime, TimeZone, Utc, Weekday};

/// The zones the time zone menu offers, when the build can resolve them.
const ZONES: [&str; 2] = ["UTC", "America/New_York"];

pub(super) struct State {
    bar: TimeframeToolbar,
    picker: DatePicker,
    /// The text of the interval entry dialog while it is open.
    entry: Option<String>,
    /// Focus the entry field on the next frame the dialog is drawn.
    focus_entry: bool,
}

impl Default for State {
    fn default() -> Self {
        let mut bar = TimeframeToolbar::default();
        bar.config.active_bg = egui::Color32::TRANSPARENT;
        // The dividend adjustment belongs to the feed request, which another group owns.
        bar.set_control_visible(Control::DividendAdjustment, false);
        Self {
            bar,
            picker: DatePicker::new(),
            entry: None,
            focus_entry: false,
        }
    }
}

impl Terminal {
    pub(super) fn time_init(&mut self) {
        self.set_control_visible(Control::Interval, true);
        let shown = self.shown(Control::TimeZone);
        self.time.bar.set_control_visible(Control::TimeZone, shown);
    }

    pub(super) fn time_poll(&mut self, _ctx: &egui::Context) {
        let chart = &self.charts[self.active];
        let has_session = has_session(chart);
        let state = &mut self.time.bar.state;
        state.timeframe = chart.timeframe().unwrap_or_default();
        state.timezone = zone_name(chart);
        state.session_type = match (has_session, chart.extended_hours()) {
            (false, _) => SessionType::Full24x7,
            (true, false) => SessionType::RTH,
            (true, true) => SessionType::ETH,
        };
        state.date_picker_open = self.time.picker.is_open;
        let shown = has_session && self.shown(Control::Session);
        self.time.bar.set_control_visible(Control::Session, shown);
        self.size_range_bars();
    }

    pub(super) fn time_show(&mut self, ui: &mut egui::Ui, region: Region) {
        match region {
            Region::Bottom => {
                let bg = ui.style().visuals.panel_fill;
                egui::Panel::bottom("egui_charts_terminal_bar")
                    .exact_size(39.0)
                    .show_separator_line(false)
                    .frame(panel_frame(bg).outer_margin(egui::Margin {
                        top: 1,
                        ..egui::Margin::ZERO
                    }))
                    .show(ui, |ui| {
                        let action = self.time.bar.show(ui);
                        if action != TimeframeToolbarAction::None {
                            self.emit(Action::BottomBar(action));
                        }
                    });
            }
            Region::Windows => self.show_time_windows(&ui.ctx().clone()),
            _ => {}
        }
    }

    pub(super) fn time_perform(&mut self, action: Action) -> Option<Action> {
        match action {
            Action::TopToolbar(TopToolbarAction::IntervalSelected(timeframe)) => {
                self.user_action(|terminal| terminal.set_interval(timeframe));
            }
            Action::BottomBar(bar) => match bar {
                TimeframeToolbarAction::DateRangeChanged(range) => self.fit_date_range(range),
                TimeframeToolbarAction::OpenDatePicker => self.time.picker.open(),
                TimeframeToolbarAction::OpenTimezoneMenu => {
                    self.time.bar.state.timezone_menu_open = true;
                }
                TimeframeToolbarAction::OpenSessionMenu => {
                    self.time.bar.state.session_menu_open = true;
                }
                bar => return Some(Action::BottomBar(bar)),
            },
            Action::DatePicker(picker) => match picker {
                DatePickerAction::GoToDate(date) => self.go_to_date(date),
                DatePickerAction::SetRange(start, end) => self.set_date_range(start, end),
                _ => {}
            },
            Action::Shortcut(ShortcutRequest::IntervalEntry(text)) => self.open_entry(text),
            Action::Shortcut(ShortcutRequest::Action(KeyboardAction::OpenIntervalSearch)) => {
                self.open_entry(String::new());
            }
            Action::Shortcut(ShortcutRequest::Action(KeyboardAction::GoToDate)) => {
                self.time.picker.open();
            }
            Action::Shortcut(ShortcutRequest::Action(KeyboardAction::ToggleSession)) => {
                if has_session(self.chart()) {
                    self.set_extended_hours(!self.chart().extended_hours());
                }
            }
            action => return Some(action),
        }
        None
    }

    pub(super) fn time_settings(&mut self, settings: &mut UserSettings, direction: Direction) {
        let selector = &mut self.top_toolbar.timeframe_selector;
        match direction {
            Direction::Save => settings.interval_favorites = selector.preferences().favorites,
            Direction::Load => {
                let mut preferences = selector.preferences();
                preferences.favorites = settings.interval_favorites.clone();
                selector.set_preferences(preferences);
            }
        }
    }

    pub(super) fn time_modal(&self) -> bool {
        let state = &self.time.bar.state;
        self.time.entry.is_some()
            || self.time.picker.is_open
            || state.session_menu_open
            || state.timezone_menu_open
    }

    /// Shows `timeframe` on the active chart. Range intervals show range bars.
    fn set_interval(&mut self, timeframe: Timeframe) {
        let chart = self.chart_mut();
        chart.set_timeframe(timeframe);
        if matches!(timeframe, Timeframe::Range(_)) {
            chart.set_chart_type(ChartType::Range);
        }
        self.size_range_bars();
    }

    /// Sizes the range bars of a range interval from the instrument's tick size.
    fn size_range_bars(&mut self) {
        let chart = self.chart_mut();
        let Some(Timeframe::Range(ticks)) = chart.timeframe() else {
            return;
        };
        let tick_size = chart
            .instrument()
            .and_then(|instrument| instrument.tick_size)
            .unwrap_or(0.01);
        let size = ticks.get() as f64 * tick_size;
        let params = chart.chart_type_params();
        if params.range_size != size || params.range_size_method != BoxSizeMethod::Fixed {
            let mut params = params.clone();
            params.range_size = size;
            params.range_size_method = BoxSizeMethod::Fixed;
            chart.set_chart_type_params(params);
        }
    }

    /// Fits the bars of the lookback `range` to the plot, and the synchronised charts to the
    /// resulting time span.
    fn fit_date_range(&mut self, range: DateRange) {
        let chart = self.chart_mut();
        let visible = visible_bars_for_range(range, chart.bars(SeriesId::MAIN));
        let width = chart.get_rendered_price_rect().width();
        // Include the latest slot in the scale's exclusive data range.
        let spacing = width.max(1.0) / (visible.max(1) as f32 + 0.5);
        chart.disable_tracking_mode();
        chart.apply_synced_time_scale(spacing, 0.5);
        if self.layout.sync.sync_date_range
            && let Some(span) = self.chart().visible_time_range()
        {
            self.apply_span(span);
        }
    }

    /// Shows the days `start` to `end`, both included, in the zone of the active chart.
    fn set_date_range(&mut self, start: NaiveDate, end: NaiveDate) {
        let chart = self.chart();
        let Some((from, next)) = local_midnight(chart, start)
            .zip(end.succ_opt().and_then(|next| local_midnight(chart, next)))
        else {
            return;
        };
        self.apply_span(VisibleTimeRange {
            from,
            to: next - Duration::seconds(1),
        });
    }

    fn apply_span(&mut self, span: VisibleTimeRange) {
        let mut slots: Vec<_> = self.charts.iter_mut().collect();
        self.layout.apply_date_range(&mut slots, span);
    }

    /// Scrolls the active chart, and the charts synchronised with it, to the first bar of
    /// `date` in each chart's own time zone.
    fn go_to_date(&mut self, date: NaiveDate) {
        let active = self.active;
        for index in 0..self.charts.len() {
            let synced = self.layout.sync.sync_date_range
                && self.layout.sync_group(index) == self.layout.sync_group(active);
            if index != active && !synced {
                continue;
            }
            let chart = &mut self.charts[index];
            let Some(start) = local_midnight(chart, date) else {
                continue;
            };
            let bars = chart.bars(SeriesId::MAIN);
            // Before the loaded history the chart requests older bars up to the day.
            let target = if bars.first().is_some_and(|bar| bar.time <= start) {
                bars.iter()
                    .find(|bar| bar.time >= start)
                    .or(bars.last())
                    .map_or(start, |bar| bar.time)
            } else {
                start
            };
            chart.scroll_to_time(target);
        }
    }

    fn open_entry(&mut self, text: String) {
        self.time.entry = Some(text);
        self.time.focus_entry = true;
    }

    /// Requests regular hours or, with `extended`, regular and extended hours. A replay shows
    /// history of the old choice, so it stops.
    fn set_extended_hours(&mut self, extended: bool) {
        if self.chart().extended_hours() == extended {
            return;
        }
        if self.replay_active {
            self.perform(Action::Replay(ReplayControlAction::Stop));
        }
        self.chart_mut().set_extended_hours(extended);
    }

    /// Shows the zone on the active chart's time axis.
    fn set_zone(&mut self, name: &str) {
        let timezone = if name == "UTC" {
            TimezoneMode::Utc
        } else if let Ok(zone) = name.parse() {
            TimezoneMode::Timezone(zone)
        } else {
            return;
        };
        let chart = self.chart_mut();
        if chart.chart_options().time_scale.timezone != timezone {
            let mut options = chart.chart_options().clone();
            options.time_scale.timezone = timezone;
            chart.set_chart_options(options);
        }
    }

    fn show_time_windows(&mut self, ctx: &egui::Context) {
        match self.time.picker.show(ctx) {
            DatePickerAction::None => {}
            action => self.emit(Action::DatePicker(action)),
        }
        self.show_interval_entry(ctx);
        self.show_session_menu(ctx);
        self.show_zone_menu(ctx);
    }

    fn show_interval_entry(&mut self, ctx: &egui::Context) {
        let Some(mut text) = self.time.entry.take() else {
            return;
        };
        let mut open = true;
        let mut apply = false;
        Modal::new("interval_entry", "Change interval")
            .width(320.0)
            .show(ctx, &mut open, |ui| {
                ui.label(l10n::text(
                    ctx,
                    "Minutes, or an interval such as 30s, 2h, 1D, 10T, 100V, 10R or 100delta",
                ));
                let input_id = egui::Id::unique("interval_entry_input");
                if std::mem::take(&mut self.time.focus_entry) {
                    ui.memory_mut(|memory| memory.request_focus(input_id));
                }
                ui.add(
                    egui::TextEdit::singleline(&mut text)
                        .id(input_id)
                        .desired_width(ui.available_width()),
                );
                apply = DialogFooter::new("Apply").show(ui).0
                    || ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter));
            });
        if apply {
            // A number alone is minutes.
            let value = if text.chars().all(|c| c.is_ascii_digit()) {
                format!("{text}min")
            } else {
                text.clone()
            };
            match value.parse::<Timeframe>() {
                Ok(timeframe) => {
                    self.emit(Action::TopToolbar(TopToolbarAction::IntervalSelected(
                        timeframe,
                    )));
                    return;
                }
                Err(error) => self.toasts.error(error),
            }
        }
        if open {
            self.time.entry = Some(text);
        }
    }

    fn show_session_menu(&mut self, ctx: &egui::Context) {
        if !self.time.bar.state.session_menu_open {
            return;
        }
        let extended = self.chart().extended_hours();
        let description = self
            .chart()
            .instrument()
            .and_then(|instrument| Some((instrument.session.as_ref()?, &instrument.time_zone)))
            .map(|(session, zone)| session_text(session, zone.as_ref()));
        let mut open = true;
        let mut choice = None;
        Modal::new("session", "Session")
            .width(320.0)
            .show(ctx, &mut open, |ui| {
                for (session, is_extended) in [(SessionType::RTH, false), (SessionType::ETH, true)]
                {
                    let label = l10n::text(ctx, session.tooltip());
                    if ui
                        .selectable_label(extended == is_extended, label)
                        .clicked()
                    {
                        choice = Some(is_extended);
                    }
                }
                if let Some(description) = &description {
                    ui.label(description);
                }
            });
        if let Some(choice) = choice {
            self.set_extended_hours(choice);
            open = false;
        }
        self.time.bar.state.session_menu_open = open;
    }

    fn show_zone_menu(&mut self, ctx: &egui::Context) {
        if !self.time.bar.state.timezone_menu_open {
            return;
        }
        let current = zone_name(self.chart());
        let mut open = true;
        let mut choice = None;
        Modal::new("timezone", "Chart timezone")
            .width(320.0)
            .show(ctx, &mut open, |ui| {
                for name in ZONES
                    .into_iter()
                    .filter(|name| name.parse::<TimezoneId>().is_ok())
                {
                    if ui.selectable_label(current == name, name).clicked() {
                        choice = Some(name);
                    }
                }
            });
        if let Some(name) = choice {
            self.user_action(|terminal| terminal.set_zone(name));
            open = false;
        }
        self.time.bar.state.timezone_menu_open = open;
    }
}

/// Whether the chart's instrument has trading hours to switch between.
fn has_session(chart: &Chart) -> bool {
    chart
        .instrument()
        .and_then(|instrument| instrument.session.as_ref())
        .is_some_and(|session| !session.windows.is_empty() || !session.extended_windows.is_empty())
}

/// The zone of the chart's time axis, `None` for the system's local time.
fn chart_zone(chart: &Chart) -> Option<TimezoneId> {
    match &chart.chart_options().time_scale.timezone {
        TimezoneMode::Utc => Some(TimezoneId::utc()),
        TimezoneMode::Local => None,
        TimezoneMode::Timezone(zone) => Some(zone.clone()),
        // The instrument's own zone replaces the fallback.
        TimezoneMode::Exchange(fallback) => Some(
            chart
                .instrument()
                .and_then(|instrument| instrument.time_zone.clone())
                .unwrap_or_else(|| fallback.clone()),
        ),
    }
}

fn zone_name(chart: &Chart) -> String {
    chart_zone(chart).map_or_else(
        || chrono::Local::now().offset().to_string(),
        |zone| zone.name().to_owned(),
    )
}

/// The first instant of `date` in the chart's time zone.
fn local_midnight(chart: &Chart, date: NaiveDate) -> Option<DateTime<Utc>> {
    let midnight = date.and_time(NaiveTime::MIN);
    match chart_zone(chart) {
        Some(zone) => local_instant(zone, midnight),
        None => local_instant(chrono::Local, midnight),
    }
}

/// How many of `bars` fall in the lookback `range` ending at the latest bar. Session gaps and
/// the interval decide the count; year to date starts on 1 January UTC.
fn visible_bars_for_range(range: DateRange, bars: &[Bar]) -> usize {
    let Some(last) = bars.last() else {
        return 0;
    };
    let end = last.time;
    let start = match range {
        DateRange::Day1 => Some(end - Duration::days(1)),
        DateRange::Day5 => Some(end - Duration::days(5)),
        DateRange::Month1 => end.checked_sub_months(Months::new(1)),
        DateRange::Month3 => end.checked_sub_months(Months::new(3)),
        DateRange::Month6 => end.checked_sub_months(Months::new(6)),
        DateRange::YTD => Utc.with_ymd_and_hms(end.year(), 1, 1, 0, 0, 0).single(),
        DateRange::Year1 => end.checked_sub_months(Months::new(12)),
        DateRange::Year5 => end.checked_sub_months(Months::new(60)),
        DateRange::All => None,
    };
    bars.len() - start.map_or(0, |start| bars.partition_point(|bar| bar.time < start))
}

/// The trading hours as text, in the exchange's zone.
fn session_text(session: &Session, zone: Option<&TimezoneId>) -> String {
    let weekday = |day: &Weekday| day.to_string();
    let windows = |label: &str, windows: &[crate::model::SessionWindow]| {
        let lines: Vec<_> = windows
            .iter()
            .map(|window| {
                let days = &window.days;
                let consecutive = days.windows(2).all(|pair| {
                    pair[1].num_days_from_monday() == (pair[0].num_days_from_monday() + 1) % 7
                });
                let days = match days.as_slice() {
                    days if days.len() == 7 => "Daily".to_owned(),
                    [first, .., last] if days.len() > 2 && consecutive => {
                        format!("{}-{}", weekday(first), weekday(last))
                    }
                    days => days.iter().map(weekday).collect::<Vec<_>>().join(", "),
                };
                format!(
                    "{days} {}-{}",
                    window.open.format("%H:%M"),
                    window.close.format("%H:%M")
                )
            })
            .collect();
        (!lines.is_empty()).then(|| format!("{label}: {}", lines.join("; ")))
    };
    let mut text: Vec<_> = [
        windows("Regular", &session.windows),
        windows("Extended", &session.extended_windows),
    ]
    .into_iter()
    .flatten()
    .collect();
    if let Some(zone) = zone {
        text.push(format!("({})", zone.name()));
    }
    text.join(" ")
}

#[cfg(test)]
mod tests {
    use super::super::test_support::{SIZE, TestFeed, click, frame, key};
    use super::*;
    use crate::data::Datafeed;
    use egui::{Event, Key, Modifiers, vec2};
    use std::sync::Arc;

    fn terminal(symbol: &str, timeframe: Timeframe) -> Terminal {
        let feed: Arc<dyn Datafeed> = Arc::new(TestFeed);
        let mut terminal = Terminal::new(feed)
            .with_symbol(symbol)
            .with_timeframe(timeframe);
        for _ in 0..3 {
            frame(&mut terminal, SIZE, vec![]);
        }
        terminal
    }

    /// Opens the time zone menu from the bottom bar and picks `name`.
    #[cfg(feature = "timezones")]
    fn pick_zone(terminal: &mut Terminal, name: &str) {
        assert!(
            terminal
                .run(Action::BottomBar(TimeframeToolbarAction::OpenTimezoneMenu))
                .is_none()
        );
        // The menu is laid out invisibly on its first frame.
        frame(terminal, SIZE, vec![]);
        let menu = frame(terminal, SIZE, vec![]);
        assert!(click(terminal, SIZE, menu.text_at(name).expect(name)).is_empty());
        frame(terminal, SIZE, vec![]);
    }

    /// A bare number enters minutes, Enter applies it through the normal action path, and undo
    /// and redo bring the interval and the clock of the bottom bar back with the chart.
    #[test]
    #[cfg(feature = "timezones")]
    fn typed_interval_entry_applies_minutes() {
        let mut terminal = terminal("AAPL", Timeframe::Min1);
        frame(&mut terminal, SIZE, vec![Event::Text("15".into())]);
        frame(&mut terminal, SIZE, vec![]);
        let applied = frame(&mut terminal, SIZE, vec![key(Key::Enter, Modifiers::NONE)]);
        assert!(applied.actions.is_empty());
        assert!(terminal.time.entry.is_none());
        assert_eq!(terminal.chart().timeframe().unwrap().duration_ms(), 900_000);
        terminal.run(Action::TopToolbar(TopToolbarAction::IntervalSelected(
            Timeframe::Hour1,
        )));
        pick_zone(&mut terminal, "America/New_York");
        let min15 = Timeframe::Min15;
        for (action, timeframe, zone) in [
            (TopToolbarAction::Undo, Timeframe::Hour1, "UTC"),
            (TopToolbarAction::Undo, min15, "UTC"),
            (TopToolbarAction::Undo, Timeframe::Min1, "UTC"),
            (TopToolbarAction::Redo, min15, "UTC"),
            (TopToolbarAction::Redo, Timeframe::Hour1, "UTC"),
            (TopToolbarAction::Redo, Timeframe::Hour1, "America/New_York"),
        ] {
            terminal.run(Action::TopToolbar(action));
            frame(&mut terminal, SIZE, vec![]);
            assert_eq!(terminal.chart().timeframe(), Some(timeframe));
            assert_eq!(terminal.top_toolbar.timeframe_selector.current, timeframe);
            assert_eq!(terminal.time.bar.state.timeframe, timeframe);
            assert_eq!(terminal.time.bar.state.timezone, zone);
        }
    }

    /// The session shortcut switches regular and extended hours of an instrument with trading
    /// hours, and leaves one that trades around the clock alone.
    #[test]
    fn session_shortcut_switches_instruments_with_hours() {
        let alt_shift = Modifiers::ALT.plus(Modifiers::SHIFT);
        for (symbol, extended, expected) in [
            ("AAPL", false, true),
            ("AAPL", true, false),
            ("EURUSD", false, true),
            ("BTCUSD", false, false),
        ] {
            let mut terminal = terminal(symbol, Timeframe::Hour1);
            terminal.chart_mut().set_extended_hours(extended);
            frame(&mut terminal, SIZE, vec![]);
            frame(&mut terminal, SIZE, vec![key(Key::E, alt_shift)]);
            let shown = frame(&mut terminal, SIZE, vec![]);
            assert_eq!(terminal.chart().extended_hours(), expected, "{symbol}");
            // The session button is labelled with its tooltip. A market that trades around the
            // clock has no such button.
            let tooltip = if expected {
                SessionType::ETH
            } else {
                SessionType::RTH
            }
            .tooltip();
            assert_eq!(
                shown.text_at(tooltip).is_some(),
                symbol != "BTCUSD",
                "{symbol}"
            );
            assert!(shown.actions.is_empty());
        }
    }

    /// A date range fits the bars of its lookback to the plot at either window size.
    #[test]
    fn ranges_fit_displayed_history_in_both_window_sizes() {
        // The last of the 500 hourly bars opens at 23:00 on 1 January 2026. A day back, at 23:00
        // on 31 December, is the first bar of 25; five days back is the first of 121.
        for size in [SIZE, vec2(640.0, 450.0)] {
            let mut terminal = terminal("AAPL", Timeframe::Hour1);
            frame(&mut terminal, size, vec![]);
            for (range, bars) in [
                (DateRange::Day1, Some(25)),
                (DateRange::Day5, Some(121)),
                (DateRange::All, None),
            ] {
                terminal.run(Action::BottomBar(TimeframeToolbarAction::DateRangeChanged(
                    range,
                )));
                frame(&mut terminal, size, vec![]);
                let chart = terminal.chart();
                let end = chart.bars(SeriesId::MAIN).len();
                let (shown_start, shown_end) = chart.state().visible_range();
                assert_eq!(shown_end, end, "{range:?}");
                let start = bars.map_or(0, |bars| end - bars);
                assert!(
                    shown_start.abs_diff(start) <= 1,
                    "{range:?} {shown_start} {shown_end} {start} {end}"
                );
                let expected =
                    chart.get_rendered_price_rect().width() / ((end - start) as f32 + 0.5);
                assert!((chart.get_time_scale_state().0 - expected).abs() < 0.001);
            }
        }
    }

    /// A range interval shows range bars sized in the instrument's ticks. The test feed gives no
    /// tick size, so the default of one hundredth applies.
    #[test]
    fn range_intervals_size_range_bars_in_ticks() {
        let mut terminal = terminal("AAPL", Timeframe::Min1);
        for (interval, size) in [("10r", 0.1), ("25r", 0.25)] {
            let timeframe = interval.parse().unwrap();
            terminal.run(Action::TopToolbar(TopToolbarAction::IntervalSelected(
                timeframe,
            )));
            frame(&mut terminal, SIZE, vec![]);
            let chart = terminal.chart();
            assert_eq!(chart.timeframe(), Some(timeframe));
            assert_eq!(chart.chart_type(), ChartType::Range);
            assert!((chart.chart_type_params().range_size - size).abs() < 1e-10);
        }
    }

    /// Go to date and a date range count days in the chart's time zone, not in UTC.
    #[test]
    #[cfg(feature = "timezones")]
    fn date_picker_actions_follow_the_chart_time_zone() {
        let day = |d| NaiveDate::from_ymd_opt(2025, 12, d).unwrap();
        let utc = |d, h| Utc.with_ymd_and_hms(2025, 12, d, h, 0, 0).unwrap();
        let mut terminal = terminal("AAPL", Timeframe::Hour1);
        pick_zone(&mut terminal, "America/New_York");
        // New York midnight is 05:00 UTC in winter. The first bar of the local day opens then.
        terminal.run(Action::DatePicker(DatePickerAction::GoToDate(day(29))));
        frame(&mut terminal, SIZE, vec![]);
        frame(&mut terminal, SIZE, vec![]);
        let span = terminal.chart().visible_time_range().unwrap();
        let centre = span.from + (span.to - span.from) / 2;
        assert!(
            (centre - utc(29, 5)).abs() < Duration::minutes(30),
            "{span:?}"
        );
        // The range holds both days: from 05:00 UTC on the 28th to 05:00 UTC on the 30th.
        terminal.run(Action::DatePicker(DatePickerAction::SetRange(
            day(28),
            day(29),
        )));
        frame(&mut terminal, SIZE, vec![]);
        let span = terminal.chart().visible_time_range().unwrap();
        assert!(
            (span.from - utc(28, 5)).abs() <= Duration::hours(1),
            "{span:?}"
        );
        assert!(
            (span.to - utc(30, 5)).abs() <= Duration::hours(1),
            "{span:?}"
        );
    }
}
