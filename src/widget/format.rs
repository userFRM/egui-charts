use super::Chart;
use crate::chart::rendering;
use crate::model::{SeriesDisplayFormat, SeriesFormat};
use crate::scales::{PriceFormatter, TimeFormatterBuilder};

#[derive(Clone)]
struct DisplayFormatLabels {
    format: SeriesDisplayFormat,
    thousands_sep: Option<char>,
}

impl PriceFormatter for DisplayFormatLabels {
    fn format(&self, value: f64) -> String {
        let text = self.format.format(value);
        if self.format.kind == SeriesFormat::Volume {
            text
        } else {
            crate::scales::group_thousands(text, self.thousands_sep)
        }
    }

    fn format_change(&self, change: f64) -> String {
        let text = self.format(change);
        if text.starts_with(['-', '+']) || change < 0.0 {
            text
        } else {
            format!("+{text}")
        }
    }

    fn clone_box(&self) -> Box<dyn PriceFormatter> {
        Box::new(self.clone())
    }
}

impl Chart {
    /// Formats a timestamp with the chart's zone, clock visibility and interval, or the median
    /// spacing of the source bars in or nearest the view for inferred and trade-based intervals.
    /// Bars of a day or longer given without a feed read in UTC, where they carry their trading
    /// date, unless they sit at the display zone's midnight; see the time zones guide.
    /// On a [horizontal scale](Self::set_horizontal_scale), formats its key as the crosshair does.
    /// Hosts can use this for data windows and other readouts beside the chart.
    pub fn format_time(&self, time: chrono::DateTime<chrono::Utc>) -> String {
        let bar_seconds = self.time_label_seconds();
        match &self.horizontal_scale {
            Some(scale) => scale.format_crosshair(time, bar_seconds),
            None => self
                .build_time_formatter()
                .format_crosshair(time, bar_seconds),
        }
    }

    /// The chart's clock: the replay time while replaying, else the server time from
    /// [`set_server_time`](Self::set_server_time) or the feed, when known.
    pub(super) fn chart_time(&self) -> Option<chrono::DateTime<chrono::Utc>> {
        self.replay_time().or_else(|| {
            self.server_time_offset
                .and_then(|offset| chrono::Utc::now().checked_add_signed(offset))
                .or_else(|| {
                    self.feed_link
                        .as_ref()
                        .and_then(super::feed::FeedLink::server_time)
                })
        })
    }

    pub(super) fn current_bar_countdown(&self) -> Option<String> {
        if !self.config.show_countdown || !self.config.show_symbol_last_val {
            return None;
        }
        let last = crate::model::last_with_value(&self.state.data().bars)?;
        let timeframe = self.timeframe?;
        if timeframe.is_trade_based() {
            return None;
        }
        let now = self
            .chart_time()
            .or_else(|| self.quote.as_ref().map(|quote| quote.ts))?;
        let session = rendering::exchange_session(
            self.instrument(),
            self.config.session_config.as_ref(),
            &self.display_timezone(),
            timeframe,
        );
        // Calendar bars encode their trading date in the feed's selected zone.
        let session_open = if self.feed_link.is_some() && timeframe.duration_ms() >= 86_400_000 {
            let date = match self.display_timezone() {
                crate::scales::TimezoneMode::Local => {
                    last.time.with_timezone(&chrono::Local).date_naive()
                }
                zone => last
                    .time
                    .with_timezone(&zone.to_tz().unwrap_or(crate::model::TimezoneId::utc()))
                    .date_naive(),
            };
            date.and_hms_opt(0, 0, 0)?.and_utc()
        } else {
            last.time
        };
        let session_close = session.bar_session_close(session_open, timeframe, self.extended_hours);
        let boundary = self
            .feed_link
            .as_ref()
            .and_then(|link| link.clock.next_open(link.clock.bar_open(last.time)));
        let close = match (session_close, boundary) {
            (Some(session), Some(boundary)) if timeframe.duration_ms() < 86_400_000 => {
                Some(session.min(boundary))
            }
            (session, boundary) => session.or(boundary),
        };
        let has_session = self
            .instrument()
            .and_then(|instrument| instrument.session.as_ref())
            .is_some_and(|session| {
                !session.windows.is_empty()
                    || !session.extended_windows.is_empty()
                    || !session.overrides.is_empty()
            })
            || self.config.session_config.is_some();
        if self.bar_close_time.is_none() && session_close.is_none() && has_session {
            return None;
        }
        crate::scales::bar_countdown(last.time, timeframe, now, self.bar_close_time.or(close))
    }

    // Source-bar cadence sets precision even when many elements share one time.
    pub(super) fn time_label_seconds(&self) -> i64 {
        if let Some(timeframe) = self
            .timeframe
            .filter(|timeframe| !timeframe.is_trade_based())
        {
            return timeframe.duration().num_seconds();
        }
        let bars = &self.state.data().bars;
        let shown = self.state.shown();
        // An empty range starts at 0 before the first bar and at the bar count after the last.
        let (start, end) = self.state.visible_range();
        let nearest = |index: usize| shown.get(index.min(shown.len().saturating_sub(1)));
        let Some((first, last)) = nearest(start).zip(nearest(end.saturating_sub(1).max(start)))
        else {
            return 1;
        };
        // At least 16 nearby source bars keep a lone session gap from setting the precision.
        let end = bars
            .partition_point(|bar| bar.time <= last.time)
            .max(16)
            .min(bars.len());
        let at = bars
            .partition_point(|bar| bar.time < first.time)
            .min(end.saturating_sub(16));
        // Bounded, so a zoomed out view does not sort every visible gap on each frame.
        let end = end.min(at + 256);
        if end - at < 2 {
            return 1;
        }
        crate::scales::median_bar_duration(bars[at..end].iter().map(|bar| bar.time)).num_seconds()
    }

    pub(crate) fn display_timezone(&self) -> crate::scales::TimezoneMode {
        match &self.chart_options.time_scale.timezone {
            crate::scales::TimezoneMode::Exchange(fallback) => {
                crate::scales::TimezoneMode::Exchange(
                    self.instrument()
                        .and_then(|instrument| instrument.time_zone.as_ref())
                        .cloned()
                        .unwrap_or_else(|| fallback.clone()),
                )
            }
            timezone => timezone.clone(),
        }
    }

    /// The zone that dates the bars: the clock they were stamped on. Supplied bars of a day or
    /// longer carry their trading date at 00:00 UTC, so they keep it in every display zone,
    /// unless they sit at midnight in the display zone; intraday bars and the bars of the
    /// chart's feed, stamped on the bar clock of the display zone, read in the display zone.
    /// `bar_seconds` gives [`time_label_seconds`](Self::time_label_seconds), read only for
    /// supplied bars on a time interval.
    pub(crate) fn label_timezone(
        &self,
        bar_seconds: impl FnOnce() -> i64,
    ) -> crate::scales::TimezoneMode {
        let zone = self.display_timezone();
        let local_midnight = |bar: &crate::model::Bar| {
            let wall = match &zone {
                crate::scales::TimezoneMode::Local => {
                    crate::model::local_time(chrono::Local, bar.time)
                }
                zone => crate::model::local_time(zone.to_tz().unwrap_or_default(), bar.time),
            };
            wall.time() == chrono::NaiveTime::MIN
        };
        let bars = &self.state.data().bars;
        let utc_dates = self.feed_link.is_none()
            && !self
                .timeframe
                .is_some_and(crate::model::Timeframe::is_trade_based)
            && bar_seconds() >= 86_400
            && !bars
                .first()
                .into_iter()
                .chain(bars.last())
                .all(local_midnight);
        if utc_dates {
            crate::scales::TimezoneMode::Utc
        } else {
            self.display_timezone()
        }
    }

    /// Dates and the axis marks read in [`label_timezone`](Self::label_timezone).
    pub(crate) fn build_time_formatter(&self) -> Box<dyn crate::scales::TimeFormatter> {
        self.time_formatter_in(self.label_timezone(|| self.time_label_seconds()))
    }

    /// Formats in `timezone`, for instants with a clock such as fills, on any interval.
    pub(crate) fn time_formatter_in(
        &self,
        timezone: crate::scales::TimezoneMode,
    ) -> Box<dyn crate::scales::TimeFormatter> {
        let mut builder = TimeFormatterBuilder::new()
            .with_24_hour(true)
            .with_seconds(self.chart_options.time_scale.seconds_visible)
            .with_time_visible(self.chart_options.time_scale.time_visible)
            .with_timezone(timezone);
        if let Some(locale) = &self.locale {
            builder = builder.with_locale(locale.clone());
        }
        if let Some(date_format) = &self.date_format {
            builder = builder.with_date_format(date_format.clone());
        }
        if let Some(formatter) = &self.time_formatter {
            builder = builder.with_formatter(formatter.clone_box());
        }
        builder.build()
    }

    /// Names months and weekdays in time axis, crosshair, tooltip and forecast date labels in
    /// `locale`'s language, such as `"de-DE"` (see
    /// [`LocaleTimeFormatter`](crate::scales::LocaleTimeFormatter)).
    /// `None` restores English names.
    ///
    /// The names need the `locales` cargo feature, on by default; without it they stay English.
    pub fn set_locale(&mut self, locale: Option<String>) {
        self.locale = locale;
    }

    /// Writes crosshair, tooltip and forecast dates with a `strftime` pattern, such as
    /// `"%d.%m.%Y"`, followed by the clock for intraday bars. `None` restores the built-in dates,
    /// which an invalid pattern also falls back to.
    pub fn set_date_format(&mut self, date_format: Option<String>) {
        self.date_format = date_format;
    }

    /// Formats time axis marks with [`TimeFormatter::format_tick`], whose default calls
    /// [`TimeFormatter::format`], and crosshair, tooltip, forecast date and
    /// [`format_time`](Self::format_time) labels with [`TimeFormatter::format_crosshair`].
    ///
    /// `format_tick` receives the chart's locale in a
    /// [`TickMarkContext`](crate::scales::TickMarkContext) and can return `None` to use the
    /// built-in locale-aware text for that tick. Otherwise the
    /// formatter replaces the built-in one, the locale and the date format. It receives
    /// wall-clock times in the zone [`format_time`](Self::format_time) reads them in, and the
    /// time scale's [`time_visible`](crate::config::TimeScaleOptions::time_visible) option
    /// still applies.
    /// `None` restores the built-in formatter.
    ///
    /// [`TimeFormatter::format`]: crate::scales::TimeFormatter::format
    /// [`TimeFormatter::format_tick`]: crate::scales::TimeFormatter::format_tick
    /// [`TimeFormatter::format_crosshair`]: crate::scales::TimeFormatter::format_crosshair
    pub fn set_time_formatter(
        &mut self,
        formatter: Option<std::sync::Arc<dyn crate::scales::TimeFormatter>>,
    ) {
        self.time_formatter = formatter;
    }

    /// Formats price scale labels in the normal and logarithmic modes: the crosshair label and
    /// price tags with [`PriceFormatter::format`], and the axis ticks with
    /// [`PriceFormatter::format_ticks`], which receives the complete ordered list for each
    /// axis, separately for layout and painting. The formatter receives prices and replaces
    /// the instrument's decimals. It applies to every price scale and to the prices drawings
    /// print; studies keep their own formats.
    /// `None` restores the built-in labels.
    ///
    /// [`PriceFormatter::format`]: crate::scales::PriceFormatter::format
    /// [`PriceFormatter::format_ticks`]: crate::scales::PriceFormatter::format_ticks
    pub fn set_price_formatter(
        &mut self,
        formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
    ) {
        self.price_formatter = formatter;
    }

    /// Formats price scale labels in the percentage and indexed-to-100 modes, as
    /// [`set_price_formatter`](Self::set_price_formatter) does in the others. The formatter
    /// receives the percentage or index values the axis shows, its ticks as one list through
    /// [`PriceFormatter::format_ticks`]. `None` restores signed percentages and index values.
    ///
    /// [`PriceFormatter::format_ticks`]: crate::scales::PriceFormatter::format_ticks
    pub fn set_percentage_formatter(
        &mut self,
        formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
    ) {
        self.percentage_formatter = formatter;
    }

    /// Formats volume readouts, including volume-formatted studies. `None` restores the
    /// built-in suffixes and instrument volume precision. Values stay in volume units.
    pub fn set_volume_formatter(
        &mut self,
        formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
    ) {
        self.volume_formatter = formatter.clone();
        for study in self
            .studies
            .indicators_mut()
            .iter_mut()
            .chain(self.element_studies.studies.indicators_mut().iter_mut())
        {
            if study.value_format() == crate::scales::StudyFormat::Volume {
                study.set_value_formatter(formatter.clone());
            }
        }
    }

    /// Overrides raw-price formatting for one bar series, including `MAIN`. This affects
    /// readouts, price tags, ticks and the prices drawings in its pane print; percentage and
    /// indexed scales retain the chart's percentage formatter. `None` restores chart-wide
    /// formatting, then the saved [`SeriesStyle::display_format`](crate::model::SeriesStyle::display_format).
    /// Returns false for an unknown series. A shared axis uses the first visible series'
    /// formatter. Custom text does not replace the declaration's tick spacing.
    pub fn set_series_formatter(
        &mut self,
        id: super::SeriesId,
        formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
    ) -> bool {
        if !(id == super::SeriesId::MAIN && self.main_series_present
            || self.price_series.iter().any(|item| item.id == id))
        {
            return false;
        }
        if let Some(formatter) = formatter {
            self.series_formatters.insert(id, formatter);
        } else {
            self.series_formatters.remove(&id);
        }
        true
    }

    pub(super) fn series_label_formatter(
        &self,
        id: super::SeriesId,
        mode: crate::scales::PriceScaleMode,
    ) -> Option<std::sync::Arc<dyn crate::scales::PriceFormatter>> {
        match mode {
            crate::scales::PriceScaleMode::Percentage
            | crate::scales::PriceScaleMode::IndexedTo100 => self.percentage_formatter.clone(),
            _ => {
                self.series_formatters
                    .get(&id)
                    .cloned()
                    .or_else(|| self.price_formatter.clone())
                    .or_else(|| {
                        Some(std::sync::Arc::new(DisplayFormatLabels {
                            format: self.series_style(id)?.display_format?,
                            thousands_sep: self.config.thousands_sep.filter(|separator| {
                                *separator != '.' && !separator.is_ascii_digit()
                            }),
                        }))
                    })
            }
        }
    }

    /// Raw-value formatting: the series' host hook, the chart's, then the saved declaration.
    pub(crate) fn series_price_formatter(
        &self,
        id: super::SeriesId,
    ) -> Option<std::sync::Arc<dyn crate::scales::PriceFormatter>> {
        self.series_label_formatter(id, crate::scales::PriceScaleMode::Normal)
    }

    /// First visible bar series owning an axis. The main series precedes added series.
    pub(super) fn series_axis_owner(&self, id: super::SeriesId) -> super::SeriesId {
        let (pane, scale) = if id == super::SeriesId::MAIN {
            // Removing the main series leaves its axis in the price pane.
            (Some(self.main_series_pane()), Some(self.main_series_scale))
        } else {
            (self.series_pane(id), self.series_scale(id))
        };
        if pane == Some(self.main_series_pane())
            && scale == Some(self.main_series_scale)
            && self.main_series_present
            && self.main_series_visible
        {
            return super::SeriesId::MAIN;
        }
        pane.and_then(|pane| {
            self.pane_series(pane)
                .into_iter()
                .find(|item| self.is_series_visible(*item) && self.series_scale(*item) == scale)
        })
        .unwrap_or(id)
    }

    pub(super) fn series_display_map(
        &self,
        id: super::SeriesId,
        map: crate::scales::PriceMap,
    ) -> crate::scales::PriceMap {
        self.series_style(self.series_axis_owner(id))
            .and_then(|style| style.display_format)
            .map_or(map, |format| map.with_display_format(format))
    }

    pub(crate) fn value_formatters<'a>(
        &'a self,
        price: Option<&'a dyn crate::scales::PriceFormatter>,
    ) -> rendering::ValueFormatters<'a> {
        rendering::ValueFormatters {
            price,
            volume: self.volume_formatter.as_deref(),
            percentage: self.percentage_formatter.as_deref(),
        }
    }

    /// Overrides automatic session breaks with host logic. `None` restores metadata-based breaks.
    pub fn set_session_provider(
        &mut self,
        provider: Option<std::sync::Arc<dyn crate::model::SessionProvider>>,
    ) {
        self.session_provider = provider;
    }
}

#[cfg(test)]
mod tests {
    use crate::chart::series::SeriesSettings;
    use crate::scales::PriceScaleMode;
    #[cfg(feature = "timezones")]
    use crate::widget::test_frames::chart;
    use crate::widget::test_frames::{frame, shapes};
    use egui::{Event, Pos2, Shape};

    #[cfg(feature = "timezones")]
    #[test]
    fn countdown_uses_feed_calendar_and_session_trading_dates() {
        use crate::data::{Datafeed, HistoryPage, HistoryRequest, Reply};
        use crate::model::{
            Bar, Instrument, Level1Quote, Session, SessionWindow, Timeframe, TimezoneId,
        };
        use chrono::{
            NaiveTime,
            Weekday::{Fri, Mon, Thu, Tue, Wed},
        };
        struct Feed;
        impl Datafeed for Feed {
            fn history(&self, _: HistoryRequest, _: Reply<HistoryPage>) {
                unreachable!("this test only reads the attached calendar");
            }
        }
        for (interval, open, now, session, expected) in [
            (
                "7min",
                "2024-03-31T00:59:00Z",
                "2024-03-31T00:59:30Z",
                false,
                Some("00:30"),
            ),
            (
                "1D",
                "2024-03-10T23:00:00Z",
                "2024-03-11T14:59:00Z",
                true,
                Some("01:00"),
            ),
            (
                "1W",
                "2024-03-10T23:00:00Z",
                "2024-03-15T14:59:00Z",
                true,
                Some("01:00"),
            ),
            (
                "1M",
                "2024-02-29T23:00:00Z",
                "2024-03-29T14:59:00Z",
                true,
                Some("01:00"),
            ),
            (
                "10t",
                "2024-03-11T14:59:00Z",
                "2024-03-11T14:59:30Z",
                false,
                None,
            ),
        ] {
            let timeframe: Timeframe = interval.parse().unwrap();
            let mut chart = chart(&[100.0], &[110.0]);
            chart.timeframe = Some(timeframe);
            chart.chart_options.time_scale.timezone =
                crate::scales::TimezoneMode::Exchange("Europe/Zurich".parse().unwrap());
            chart.attach_feed(std::sync::Arc::new(Feed), 1);
            let mut instrument =
                Instrument::new("TEST").with_time_zone(TimezoneId::named("Europe/Zurich").unwrap());
            if session {
                instrument.session = Some(Session::new([SessionWindow::new(
                    [Mon, Tue, Wed, Thu, Fri],
                    NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                    NaiveTime::from_hms_opt(16, 0, 0).unwrap(),
                )]));
            }
            chart.set_instrument(Some(instrument));
            // Metadata resolves Exchange timezone before the bar is assigned to its slot.
            chart.feed_link.as_mut().unwrap().clock = chart.bar_clock();
            chart.set_data(
                crate::widget::SeriesId::MAIN,
                vec![Bar::new(
                    open.parse().unwrap(),
                    100.0,
                    110.0,
                    100.0,
                    110.0,
                    1.0,
                )],
            );
            chart.set_quote(Some(Level1Quote {
                symbol: "TEST".into(),
                ts: now.parse().unwrap(),
                bid: 109.0,
                ask: 111.0,
                bid_size: 1.0,
                ask_size: 1.0,
            }));
            assert_eq!(
                chart.current_bar_countdown().as_deref(),
                expected,
                "{interval}"
            );
        }
    }

    /// A session gap at the view's edge never changes minute precision. A known daily
    /// interval keeps date precision even when the clock change makes one bar 23 hours long.
    #[test]
    fn time_precision_follows_the_interval_across_gaps_and_clock_changes() {
        use crate::model::{Bar, BarData, LogicalRange, Timeframe};
        use chrono::{Duration, TimeZone, Utc};
        let session = |day, minutes| {
            let open = Utc.with_ymd_and_hms(2024, 1, day, 14, 30, 0).unwrap();
            (0..minutes).map(move |minute| {
                Bar::new(
                    open + Duration::minutes(minute),
                    100.0,
                    110.0,
                    100.0,
                    110.0,
                    1.0,
                )
            })
        };
        // (Monday bars, left, right, bar formatted, expected)
        for (monday, left, right, at, expected) in [
            (390, 390.5, 450.5, 405, "Mon 08 Jan '24 14:45"),
            (390, 389.5, 450.5, 405, "Mon 08 Jan '24 14:45"),
            (1, 400.5, 460.5, 390, "Mon 08 Jan '24 14:30"),
        ] {
            for timeframe in [
                Some(Timeframe::Min1),
                None,
                Some(Timeframe::Ticks(std::num::NonZeroU64::new(10).unwrap())),
            ] {
                let ctx = egui::Context::default();
                let bars = session(5, 390).chain(session(8, monday)).collect();
                let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
                if let Some(timeframe) = timeframe {
                    chart.set_timeframe(timeframe);
                }
                chart.chart_options.tooltip.enabled = false;
                frame(&ctx, &mut chart, 0.0, vec![]);
                assert!(chart.set_visible_logical_range(LogicalRange::new(left, right)));
                let time = chart.state.data().bars[at].time;
                assert_eq!(
                    chart.format_time(time),
                    expected,
                    "{left}..{right}, {timeframe:?}"
                );
                if at as f32 >= left {
                    let map = chart.get_chart_mapping();
                    let painted = shapes(
                        &ctx,
                        &mut chart,
                        0.1,
                        vec![Event::PointerMoved(Pos2::new(
                            map.idx_to_x(at),
                            map.rect.center().y,
                        ))],
                    );
                    assert!(
                        painted.iter().any(|shape| matches!(shape,
                            Shape::Text(text) if text.galley.text() == expected
                        )),
                        "crosshair: {left}..{right}, {timeframe:?}"
                    );
                }
            }
        }
        #[cfg(feature = "timezones")]
        for (left, right) in [(39.5, 69.5), (38.5, 68.5)] {
            let ctx = egui::Context::default();
            let first = Utc.with_ymd_and_hms(2024, 2, 1, 5, 0, 0).unwrap();
            // New York midnight is 05:00Z through March 10, then 04:00Z from March 11.
            let bars = (0..90)
                .map(|day| {
                    let time = first + Duration::days(day)
                        - if day >= 39 {
                            Duration::hours(1)
                        } else {
                            Duration::zero()
                        };
                    Bar::new(time, 100.0, 110.0, 100.0, 110.0, 1.0)
                })
                .collect();
            let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
            chart.set_timeframe(Timeframe::Day1);
            chart.chart_options.time_scale.timezone =
                crate::scales::TimezoneMode::Timezone("America/New_York".parse().unwrap());
            frame(&ctx, &mut chart, 0.0, vec![]);
            assert!(chart.set_visible_logical_range(LogicalRange::new(left, right)));
            assert_eq!(
                chart.format_time(chart.state.data().bars[39].time),
                "Mar 11, 2024",
                "{left}..{right}"
            );
        }
    }

    #[test]
    fn tick_context_tracks_locale_and_falls_back_without_changing_crosshair() {
        use crate::model::{BarData, Timeframe};
        use crate::scales::{TickMarkContext, TickMarkType, TimeFormatter, TimezoneMode};
        use chrono::{DateTime, Utc};
        use std::sync::Arc;

        #[derive(Clone)]
        struct Host;
        impl TimeFormatter for Host {
            fn format(&self, _: DateTime<Utc>, _: TickMarkType) -> String {
                "legacy tick".into()
            }
            fn format_tick(&self, context: TickMarkContext<'_>) -> Option<String> {
                match context.mark_type {
                    TickMarkType::Year => Some(format!(
                        "{} {}",
                        context.locale.unwrap_or("default"),
                        context.time.format("%Y-%m-%d %H:%M:%S%.3f")
                    )),
                    TickMarkType::TimeWithMilliseconds => Some(String::new()),
                    _ => None,
                }
            }
            fn format_crosshair(&self, time: DateTime<Utc>, _: i64) -> String {
                format!("hover {}", time.format("%H:%M"))
            }
            fn clone_box(&self) -> Box<dyn TimeFormatter> {
                Box::new(self.clone())
            }
        }

        let time = "2024-02-29T19:00:01.123Z".parse().unwrap();
        let mut chart = crate::widget::Chart::new(BarData::from_bars(vec![]));
        chart.set_timeframe(Timeframe::Min1);
        chart.chart_options.time_scale.timezone = TimezoneMode::Timezone("+05:45".parse().unwrap());
        chart.set_time_formatter(Some(Arc::new(Host)));
        chart.set_date_format(Some("%d.%m.%Y".into()));
        // Add 5h45m by hand: leap day 19:00 UTC becomes March 1, 00:45 display time.
        for (locale, prefix, month) in [
            (Some("en-US"), "en-US", "Mar"),
            (
                Some("de-DE"),
                "de-DE",
                if cfg!(feature = "locales") {
                    "Mär"
                } else {
                    "Mar"
                },
            ),
            (Some("unknown"), "unknown", "Mar"),
            (None, "default", "Mar"),
        ] {
            chart.set_locale(locale.map(str::to_owned));
            let formatter = chart.build_time_formatter();
            for formatter in [formatter.clone_box(), formatter] {
                for (kind, expected) in [
                    (
                        TickMarkType::Year,
                        format!("{prefix} 2024-03-01 00:45:01.123"),
                    ),
                    (TickMarkType::Month, month.into()),
                    (TickMarkType::DayOfMonth, "01".into()),
                    (TickMarkType::Time, "00:45".into()),
                    (TickMarkType::TimeWithSeconds, "00:45:01".into()),
                    (TickMarkType::TimeWithMilliseconds, String::new()),
                ] {
                    assert_eq!(
                        formatter.format(time, kind),
                        expected,
                        "{locale:?}, {kind:?}"
                    );
                }
                assert_eq!(formatter.format_crosshair(time, 60), "hover 00:45");
            }
            assert_eq!(chart.format_time(time), "hover 00:45");
        }
        chart.chart_options.time_scale.seconds_visible = false;
        assert_eq!(
            chart
                .build_time_formatter()
                .format(time, TickMarkType::TimeWithSeconds),
            "00:45"
        );
        chart.chart_options.time_scale.time_visible = false;
        assert_eq!(
            chart
                .build_time_formatter()
                .format(time, TickMarkType::Time),
            "hover 00:45"
        );
    }

    #[test]
    fn host_formatters_reach_axes_crosshair_and_tags() {
        use crate::model::{Bar, BarData, Timeframe};
        use crate::scales::{PriceFormatter, TickMarkType, TimeFormatter};
        use chrono::{DateTime, Duration, TimeZone, Utc};
        use std::sync::Arc;
        #[derive(Clone)]
        struct Host;
        impl TimeFormatter for Host {
            fn format(&self, _: DateTime<Utc>, _: TickMarkType) -> String {
                "tick time".into()
            }
            fn format_crosshair(&self, _: DateTime<Utc>, _: i64) -> String {
                "hover time".into()
            }
            fn clone_box(&self) -> Box<dyn TimeFormatter> {
                Box::new(Host)
            }
        }
        impl PriceFormatter for Host {
            fn format(&self, _: f64) -> String {
                "label price".into()
            }
            fn format_tick(&self, _: f64) -> String {
                "tick price".into()
            }
            fn clone_box(&self) -> Box<dyn PriceFormatter> {
                Box::new(Host)
            }
        }
        // Wednesday 13 March 2024, 14:30 UTC, at the hovered bar.
        let time = Utc.with_ymd_and_hms(2024, 3, 13, 14, 30, 0).unwrap();
        type Setup = fn(&mut crate::widget::Chart);
        let cases: [(Setup, &[&str]); 6] = [
            (
                |chart| chart.set_locale(Some("de-DE".into())),
                &[if cfg!(feature = "locales") {
                    "Mi 13 Mär '24 14:30"
                } else {
                    "Wed 13 Mar '24 14:30"
                }],
            ),
            (
                |chart| chart.set_date_format(Some("%d.%m.%Y".into())),
                &["13.03.2024 14:30"],
            ),
            (
                |chart| chart.set_time_formatter(Some(Arc::new(Host))),
                &["hover time", "tick time"],
            ),
            (
                |chart| chart.set_price_formatter(Some(Arc::new(Host))),
                &["label price", "tick price", "O label price"],
            ),
            (
                |chart| {
                    chart.add_indicator(Box::new(
                        crate::studies::CustomIndicator::new("Volume units", |bars| {
                            vec![crate::studies::IndicatorValue::Single(99.0); bars.len()]
                        })
                        .with_value_format(crate::scales::StudyFormat::Volume),
                    ));
                    chart.set_volume_formatter(Some(Arc::new(Host)));
                },
                &["label price"],
            ),
            (
                |chart| {
                    chart.set_percentage_formatter(Some(Arc::new(Host)));
                    chart.apply_series_settings(&SeriesSettings {
                        price_scale_mode: PriceScaleMode::Percentage,
                        ..SeriesSettings::default()
                    });
                },
                &["label price", "tick price"],
            ),
        ];
        for (setup, expected) in cases {
            let ctx = egui::Context::default();
            let bars = (0..100)
                .map(|i| {
                    let open = 100.0 + i as f64;
                    Bar::new(
                        time + Duration::minutes(i - 50),
                        open,
                        open + 2.0,
                        open - 1.0,
                        open + 1.0,
                        1.0,
                    )
                })
                .collect();
            let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
            chart.set_timeframe(Timeframe::Min1);
            chart.chart_options.tooltip.enabled = false;
            frame(&ctx, &mut chart, 0.0, vec![]);
            // After a first frame, so the formatters reach a chart that has already drawn.
            setup(&mut chart);
            frame(&ctx, &mut chart, 0.05, vec![]);
            let map = chart.get_chart_mapping();
            let pos = Pos2::new(map.idx_to_x(50), map.rect.center().y);
            let painted = shapes(&ctx, &mut chart, 0.1, vec![Event::PointerMoved(pos)]);
            for text in expected {
                assert!(
                    painted.iter().any(
                        |shape| matches!(shape, Shape::Text(shape) if shape.galley.text() == *text)
                    ),
                    "{text}"
                );
            }
            if chart.volume_formatter.is_some() {
                // The fallback volume row combines its prefix and value in one text shape.
                let labels = painted
                    .iter()
                    .filter(|shape| {
                        matches!(shape,
                            Shape::Text(text) if text.pos.x < chart.last_rendered_price_rect.center().x
                                && text.galley.job.sections.iter().any(|section| {
                                    &text.galley.job.text[section.byte_range.start.0..section.byte_range.end.0]
                                        == "label price"
                                })
                        )
                    })
                    .count();
                assert_eq!(
                    labels, 2,
                    "the volume row and overlay legend use the host formatter"
                );
            }
            if expected.contains(&"O label price") {
                chart.chart_options.tooltip.enabled = true;
                chart.chart_options.tooltip.on_long_press = false;
                let painted = shapes(&ctx, &mut chart, 0.2, vec![Event::PointerMoved(pos)]);
                let labels = painted.iter().filter(|shape| matches!(shape, Shape::Text(text) if text.galley.text() == "label price")).count();
                assert!(
                    labels >= 5,
                    "OHLC tooltip values must use the host formatter: {labels}"
                );
            }
        }
    }

    /// Bars supplied at 00:00 UTC on their trading date keep that date in every display zone,
    /// in the readout and in the day, month and year marks of the axis; intraday bars keep the
    /// clock of the display zone.
    #[cfg(feature = "timezones")]
    #[test]
    fn supplied_calendar_bars_keep_their_date_in_every_display_zone() {
        use crate::model::{Bar, BarData, LogicalRange, Timeframe};
        use crate::scales::TimezoneMode;
        use chrono::{Datelike, Duration, Months, NaiveDate, TimeZone, Utc};
        let day = |y, m, d| Utc.with_ymd_and_hms(y, m, d, 0, 0, 0).unwrap();
        let months = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        // (interval, first bar, bar read, its label, marks that must be painted)
        for (timeframe, first, at, expected, required) in [
            (
                Timeframe::Day1,
                day(2024, 1, 1),
                95,
                "Apr 05, 2024",
                &["Feb", "Mar", "Apr"][..],
            ),
            (
                Timeframe::Week1,
                day(2023, 1, 2),
                95,
                "Oct 28, 2024",
                &["2024"][..],
            ),
            (
                Timeframe::Month1,
                day(2016, 1, 1),
                95,
                "Dec 01, 2023",
                &["2020"][..],
            ),
        ] {
            let times: Vec<_> = (0..100)
                .map(|i| match timeframe {
                    Timeframe::Month1 => first.checked_add_months(Months::new(i)).unwrap(),
                    Timeframe::Week1 => first + Duration::weeks(i.into()),
                    _ => first + Duration::days(i.into()),
                })
                .collect();
            for zone in [
                TimezoneMode::Utc,
                TimezoneMode::Timezone("America/New_York".parse().unwrap()),
                TimezoneMode::Timezone("Asia/Tokyo".parse().unwrap()),
            ] {
                let ctx = egui::Context::default();
                let bars = times
                    .iter()
                    .map(|&time| Bar::new(time, 100.0, 110.0, 100.0, 110.0, 1.0))
                    .collect();
                let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
                chart.set_timeframe(timeframe);
                chart.chart_options.time_scale.timezone = zone.clone();
                frame(&ctx, &mut chart, 0.0, vec![]);
                assert!(chart.set_visible_logical_range(LogicalRange::new(-0.5, 99.5)));
                assert_eq!(
                    chart.format_time(times[at]),
                    expected,
                    "{timeframe:?} {zone:?}"
                );
                let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
                let map = chart.get_chart_mapping();
                let mut seen = Vec::new();
                for text in painted.iter().filter_map(|shape| match shape {
                    Shape::Text(text) if text.pos.y > map.rect.bottom() => Some(text),
                    _ => None,
                }) {
                    let label = text.galley.text();
                    let index = map.x_to_idx(text.pos.x + text.galley.size().x / 2.0);
                    let date = times[index].date_naive();
                    let starts_month =
                        index == 0 || times[index - 1].date_naive().month() != date.month();
                    let fits = match label.parse::<i32>() {
                        Ok(year) if year > 31 => {
                            starts_month && date.month() == 1 && date.year() == year
                        }
                        Ok(day) => date.day() as i32 == day,
                        Err(_) => {
                            starts_month
                                && months.iter().position(|name| *name == label)
                                    == Some(date.month0() as usize)
                        }
                    };
                    assert!(fits, "{timeframe:?} {zone:?}: {label} on {date}");
                    seen.push(label.to_owned());
                }
                for label in required {
                    assert!(
                        seen.iter().any(|seen| seen == label),
                        "{timeframe:?} {zone:?}: {label} missing from {seen:?}"
                    );
                }
            }
        }
        // Hourly bars keep the New York clock and date.
        let first = NaiveDate::from_ymd_opt(2024, 1, 5)
            .unwrap()
            .and_hms_opt(3, 0, 0)
            .unwrap()
            .and_utc();
        let bars = (0..100)
            .map(|i| Bar::new(first + Duration::hours(i), 100.0, 110.0, 100.0, 110.0, 1.0))
            .collect();
        let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
        chart.set_timeframe(Timeframe::Hour1);
        chart.chart_options.time_scale.timezone = TimezoneMode::nyse();
        assert_eq!(chart.format_time(first), "Thu 04 Jan '24 22:00");
    }

    #[cfg(feature = "timezones")]
    #[test]
    fn hover_times_share_the_chart_zone_through_dst() {
        use crate::model::{Bar, BarData, Timeframe};
        use crate::scales::TimezoneMode;
        use chrono::{Duration, TimeZone, Utc};
        // US: UTC-5 to -4 at 07:00Z in March, -4 to -5 at 06:00Z in November.
        // Zurich: UTC+1 to +2 at 01:00Z in March, +2 to +1 at 01:00Z in October.
        for (zone, month, day, hour, before, after) in [
            (
                "America/New_York",
                3,
                10,
                7,
                "Sun 10 Mar '24 01:59:59",
                "Sun 10 Mar '24 03:00:00",
            ),
            (
                "America/New_York",
                11,
                3,
                6,
                "Sun 03 Nov '24 01:59:59",
                "Sun 03 Nov '24 01:00:00",
            ),
            (
                "Europe/Zurich",
                3,
                31,
                1,
                "Sun 31 Mar '24 01:59:59",
                "Sun 31 Mar '24 03:00:00",
            ),
            (
                "Europe/Zurich",
                10,
                27,
                1,
                "Sun 27 Oct '24 02:59:59",
                "Sun 27 Oct '24 02:00:00",
            ),
        ] {
            let transition = Utc.with_ymd_and_hms(2024, month, day, hour, 0, 0).unwrap();
            for (time, expected) in [
                (transition - Duration::seconds(1), before),
                (transition, after),
            ] {
                for mode in [
                    crate::config::TooltipMode::Floating,
                    crate::config::TooltipMode::Tracking,
                ] {
                    let ctx = egui::Context::default();
                    let bars = (0..100)
                        .map(|i| {
                            Bar::new(
                                time + Duration::seconds(i - 50),
                                100.0,
                                102.0,
                                99.0,
                                101.0,
                                1.0,
                            )
                        })
                        .collect();
                    let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
                    let study = chart.add_indicator(Box::new(
                        crate::studies::CustomIndicator::new("Fixture", |bars| {
                            vec![crate::studies::IndicatorValue::Single(0.12345); bars.len()]
                        }),
                    ));
                    let study = chart.indicator_mut(study).unwrap();
                    let mut settings = study.settings();
                    settings.precision = Some(3);
                    study.apply_settings(&[], settings).unwrap();
                    chart.set_timeframe(Timeframe::Sec1);
                    chart.chart_options.time_scale.timezone = TimezoneMode::nyse();
                    chart.set_instrument(Some(
                        crate::model::Instrument::new("DST")
                            .with_time_zone(crate::model::TimezoneId::named(zone).unwrap()),
                    ));
                    assert_eq!(chart.format_time(time), expected);
                    chart.chart_options.tooltip.mode = mode;
                    chart.chart_options.tooltip.show_time = true;
                    chart.chart_options.tooltip.on_long_press = false;
                    frame(&ctx, &mut chart, 0.0, vec![]);
                    let map = chart.get_chart_mapping();
                    let pos = Pos2::new(map.idx_to_x(50), map.rect.center().y);
                    let painted = shapes(&ctx, &mut chart, 0.1, vec![Event::PointerMoved(pos)]);
                    let times: Vec<_> = painted
                        .iter()
                        .filter_map(|shape| match shape {
                            Shape::Text(text) if text.galley.text().contains(expected) => {
                                Some(text.galley.text())
                            }
                            _ => None,
                        })
                        .collect();
                    let values: Vec<_> = painted
                        .iter()
                        .filter_map(|shape| match shape {
                            Shape::Text(text) => Some(text.galley.text()),
                            _ => None,
                        })
                        .collect();
                    match mode {
                        crate::config::TooltipMode::Floating => {
                            assert_eq!(
                                values.iter().filter(|text| **text == "Fixture").count(),
                                2,
                                "{values:?}"
                            );
                            assert_eq!(
                                values
                                    .windows(2)
                                    .filter(|pair| *pair == ["Fixture", "0.123"])
                                    .count(),
                                2,
                                "{values:?}"
                            );
                        }
                        _ => assert!(
                            values.iter().any(|text| text.contains("Fixture: 0.123")),
                            "{values:?}"
                        ),
                    }
                    assert_eq!(
                        times.len(),
                        2,
                        "{zone} {time} {mode:?}: expected matching tooltip and crosshair, {times:?}"
                    );
                }
            }
        }
    }
}
