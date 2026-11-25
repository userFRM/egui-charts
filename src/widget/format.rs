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
    /// date, under a display zone behind UTC unless they sit at its midnight; see the time zones
    /// guide.
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

    /// The name the readouts give the horizontal value that [`Self::format_time`] writes: the
    /// [horizontal scale](Self::set_horizontal_scale)'s
    /// [`value_name`](crate::scales::HorizontalScale::value_name), else "Time", as the time
    /// scale names it.
    #[cfg(feature = "ui")]
    pub(crate) fn horizontal_value_name(&self) -> Option<&str> {
        match &self.horizontal_scale {
            Some(scale) => scale.value_name(),
            None => Some("Time"),
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
        let session_close = if let Some(link) = self
            .feed_link
            .as_ref()
            .filter(|link| link.clock.has_session())
        {
            link.clock
                .sessions()
                .bar_session_close(link.clock.bar_open(last.time), timeframe, true)
        } else {
            let session = rendering::exchange_session(
                self.instrument(),
                self.config.session_config.as_ref(),
                &self.display_timezone(),
                timeframe,
            );
            // Calendar bars encode their trading date in the feed's selected zone.
            let session_open = if self.feed_link.is_some() && timeframe.duration_ms() >= 86_400_000
            {
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
            session.bar_session_close(session_open, timeframe, self.extended_hours)
        };
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
                    || !session.overnight_windows.is_empty()
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

    /// The zone the time axis and the crosshair show: the chosen
    /// [`TimezoneMode`](crate::scales::TimezoneMode), with exchange time resolved to the zone of
    /// the [instrument](Self::instrument), or to the fallback zone it carries when the instrument
    /// names none. Local time stays [`TimezoneMode::Local`](crate::scales::TimezoneMode::Local),
    /// the system's zone. Supplied bars of a day or longer can still date in UTC, so that they
    /// keep their trading date. The bottom toolbar's clock follows the chart through
    /// `TimeframeToolbarState::set_display_timezone`.
    pub fn display_timezone(&self) -> crate::scales::TimezoneMode {
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

    /// The zone that dates the bars. Supplied bars of a day or longer carry their trading date at
    /// 00:00 UTC. A display zone behind UTC shows that stamp as the evening before, so there they
    /// read in UTC unless they sit at its midnight; UTC gives a bar at that midnight the same
    /// date. Other zones keep the date of the stamp and of their own midnight, so they read in
    /// the display zone, as intraday bars and the bars of the chart's feed always do. Bar replay
    /// decides on the history it retains, since its window is cut on this zone's calendar.
    /// `bar_seconds` gives [`time_label_seconds`](Self::time_label_seconds), read only for
    /// supplied bars on a time interval.
    pub(crate) fn label_timezone(
        &self,
        bar_seconds: impl FnOnce() -> i64,
    ) -> crate::scales::TimezoneMode {
        let zone = self.display_timezone();
        let wall = |bar: &crate::model::Bar| match &zone {
            crate::scales::TimezoneMode::Local => crate::model::local_time(chrono::Local, bar.time),
            zone => crate::model::local_time(zone.to_tz().unwrap_or_default(), bar.time),
        };
        let bars = (self.replay.as_ref())
            .and_then(|replay| replay.bars.get(&super::SeriesId::MAIN))
            .unwrap_or(&self.state.data().bars);
        let edges = || bars.first().into_iter().chain(bars.last());
        let utc_dates = self.feed_link.is_none()
            && !self
                .timeframe
                .is_some_and(crate::model::Timeframe::is_trade_based)
            && bar_seconds() >= 86_400
            && edges().any(|bar| wall(bar) < bar.time.naive_utc())
            && !edges().all(|bar| wall(bar).time() == chrono::NaiveTime::MIN);
        if utc_dates {
            crate::scales::TimezoneMode::Utc
        } else {
            self.display_timezone()
        }
    }

    /// The zone of the calendar that dates the bars, the one [`Self::go_to_local_time`] reads.
    pub(crate) fn bar_date_timezone(&self) -> crate::scales::TimezoneMode {
        self.label_timezone(|| self.time_label_seconds())
    }

    /// Resolves local fields on the calendar that dates the bars. A skipped wall time opens at
    /// the clock jump, a repeated one at its first occurrence; unrepresentable dates give `None`.
    pub(super) fn local_date_instant(
        &self,
        date: chrono::NaiveDate,
        time: Option<chrono::NaiveTime>,
    ) -> Option<chrono::DateTime<chrono::Utc>> {
        let local = date.and_time(time.unwrap_or(chrono::NaiveTime::MIN));
        match Self::calendar_zone(self.bar_date_timezone()) {
            Some(zone) => crate::model::local_instant(zone, local),
            None => crate::model::local_instant(chrono::Local, local),
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
        let time_scale = &self.chart_options.time_scale;
        let mut builder = TimeFormatterBuilder::new()
            .with_24_hour(time_scale.use_24_hour)
            .with_seconds(time_scale.seconds_visible)
            .with_time_visible(time_scale.time_visible)
            .with_weekday_names(time_scale.weekday_names)
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
    /// `locale`'s language: a language code such as `"de"`, alone or with a region such as
    /// `"de-AT"`, in any letter case. A language alone uses its most common region, `"de"` as
    /// `"de-DE"`; [`LocaleTimeFormatter::locale`](crate::scales::LocaleTimeFormatter::locale)
    /// lists the accepted forms. `None` restores English names, which other forms and languages
    /// without names also get.
    ///
    /// The names need the `locales` cargo feature, on by default; without it they stay English.
    pub fn set_locale(&mut self, locale: Option<String>) {
        self.locale = locale;
    }

    /// Writes crosshair, tooltip and forecast dates with a `strftime` pattern, such as
    /// `"%d.%m.%Y"`, followed by the clock for intraday bars. `None` or an empty pattern restores
    /// the built-in dates, which an invalid pattern also falls back to.
    /// [`TimeScaleOptions::weekday_names`](crate::config::TimeScaleOptions::weekday_names) can
    /// add the weekday to a pattern without one.
    pub fn set_date_format(&mut self, date_format: Option<String>) {
        self.date_format = date_format.filter(|pattern| !pattern.is_empty());
    }

    /// Formats time axis marks with [`TimeFormatter::format_tick`], whose default calls
    /// [`TimeFormatter::format`], and crosshair, tooltip, forecast date and
    /// [`format_time`](Self::format_time) labels with [`TimeFormatter::format_crosshair`].
    ///
    /// `format_tick` receives the chart's locale in a
    /// [`TickMarkContext`](crate::scales::TickMarkContext) and can return `None` to use the
    /// built-in locale-aware text for that tick. Otherwise the
    /// formatter replaces the built-in one, the locale, date format, clock and weekday names.
    /// It receives wall-clock times in the zone [`format_time`](Self::format_time) reads them
    /// in, and the time scale's
    /// [`time_visible`](crate::config::TimeScaleOptions::time_visible) option still applies.
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
    /// the instrument's decimals. It applies to every price scale, to the prices drawings print
    /// and to studies in price units, unless [`set_series_formatter`](Self::set_series_formatter)
    /// for `MAIN` or [`set_indicator_formatter`](Self::set_indicator_formatter) overrides it.
    /// `None` restores the built-in labels.
    ///
    /// [`PriceFormatter::format`]: crate::scales::PriceFormatter::format
    /// [`PriceFormatter::format_ticks`]: crate::scales::PriceFormatter::format_ticks
    pub fn set_price_formatter(
        &mut self,
        formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
    ) {
        self.price_formatter = formatter;
        self.sync_study_formatters();
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
        self.volume_formatter = formatter;
        self.sync_study_formatters();
    }

    /// Overrides raw-price formatting for one bar series, including `MAIN`. The `MAIN` formatter
    /// also formats studies in price units. This affects
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
        self.sync_study_formatters();
        true
    }

    /// Formats study `id`'s values with `formatter`: its legend values, data window and tooltip
    /// readouts, axis tags and, in a pane of its own, its axis labels and crosshair label, for
    /// every output of the study. It takes precedence over the chart's formatters. A study in
    /// price units otherwise takes the main series' formatter, then
    /// [`set_price_formatter`](Self::set_price_formatter)'s, and a volume study
    /// [`set_volume_formatter`](Self::set_volume_formatter)'s. `None` restores that formatting,
    /// then the study's declared units and precision. Returns false for an id that is not a
    /// study of the chart, or a custom presentation that keeps its own formatting.
    pub fn set_indicator_formatter(
        &mut self,
        id: super::SeriesId,
        formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
    ) -> bool {
        let Some(at) = self.study_ids.iter().position(|study| *study == id) else {
            return false;
        };
        if let Some(formatter) = formatter {
            self.study_formatters.insert(id, formatter);
        } else {
            self.study_formatters.remove(&id);
        }
        let resolved = self.study_formatter(id, self.studies.indicators()[at].value_format());
        if !self.studies.indicators_mut()[at].set_value_formatter(resolved) {
            self.study_formatters.remove(&id);
            return false;
        }
        self.sync_study_formatters();
        true
    }

    /// A series' saved display declaration, else the fraction chosen on its scale, else the
    /// instrument's on the scale that takes the instrument's decimals, while none are chosen.
    pub(super) fn display_format(&self, id: super::SeriesId) -> Option<SeriesDisplayFormat> {
        self.series_style(id)?.display_format.or_else(|| {
            let scale = self.series_price_scale(id)?;
            let fraction = scale.chosen_fraction().or_else(|| {
                let follows = std::ptr::eq(scale, self.state.price_scale())
                    && scale.chosen_precision().is_none();
                self.instrument()?.price_fraction.filter(|_| follows)
            })?;
            Some(SeriesDisplayFormat {
                kind: SeriesFormat::Fraction(fraction),
                ..Default::default()
            })
        })
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
                        let format = self.display_format(id)?;
                        // An apostrophe separates the fraction, so it groups no thousands there.
                        let fraction = matches!(format.kind, SeriesFormat::Fraction(_));
                        Some(std::sync::Arc::new(DisplayFormatLabels {
                            format,
                            thousands_sep: self.config.thousands_sep.filter(|separator| {
                                *separator != '.'
                                    && !separator.is_ascii_digit()
                                    && !(fraction && *separator == '\'')
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

    /// The formatter of study `id` with value format `format`: its own, else the one it
    /// inherits.
    pub(super) fn study_formatter(
        &self,
        id: super::SeriesId,
        format: crate::scales::StudyFormat,
    ) -> Option<std::sync::Arc<dyn crate::scales::PriceFormatter>> {
        self.study_formatters
            .get(&id)
            .cloned()
            .or_else(|| self.inherited_study_formatter(format))
    }

    /// A study's formatter from the chart: the main series' host formatter, then the chart's,
    /// for prices, and the volume formatter for volumes. Saved display formats stay with series.
    pub(super) fn inherited_study_formatter(
        &self,
        format: crate::scales::StudyFormat,
    ) -> Option<std::sync::Arc<dyn crate::scales::PriceFormatter>> {
        match format {
            crate::scales::StudyFormat::Price => self
                .series_formatters
                .get(&super::SeriesId::MAIN)
                .or(self.price_formatter.as_ref())
                .cloned(),
            crate::scales::StudyFormat::Volume => self.volume_formatter.clone(),
            _ => None,
        }
    }

    /// Passes every study, and its element copy, the formatter [`Self::study_formatter`] gives.
    pub(super) fn sync_study_formatters(&mut self) {
        let formatters: Vec<_> = self
            .study_ids
            .iter()
            .zip(self.studies.indicators())
            .map(|(id, study)| self.study_formatter(*id, study.value_format()))
            .collect();
        for registry in [&mut self.studies, &mut self.element_studies.studies] {
            for (study, formatter) in registry.indicators_mut().iter_mut().zip(&formatters) {
                study.set_value_formatter(formatter.clone());
            }
        }
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
        self.display_format(self.series_axis_owner(id))
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
            ..Default::default()
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
            Weekday::{Fri, Mon, Sat, Thu, Tue, Wed},
        };
        struct Feed;
        impl Datafeed for Feed {
            fn history(&self, _: HistoryRequest, _: Reply<HistoryPage>) {
                unreachable!("this test only reads the attached calendar");
            }
        }
        for (interval, open, now, session, session_config, expected) in [
            (
                "7min",
                "2024-03-31T00:59:00Z",
                "2024-03-31T00:59:30Z",
                false,
                false,
                Some("00:30"),
            ),
            (
                "1D",
                "2024-03-10T23:00:00Z",
                "2024-03-11T14:59:00Z",
                true,
                false,
                Some("01:00"),
            ),
            (
                "1W",
                "2024-03-10T23:00:00Z",
                "2024-03-15T14:59:00Z",
                true,
                false,
                Some("01:00"),
            ),
            (
                "1M",
                "2024-02-29T23:00:00Z",
                "2024-03-29T14:59:00Z",
                true,
                false,
                Some("01:00"),
            ),
            (
                "3D",
                "2026-11-11T23:00:00Z",
                "2026-11-16T14:59:00Z",
                true,
                false,
                Some("01:00"),
            ),
            // The chart's saved hours supply the close when the feed has no session.
            (
                "5min",
                "2026-11-16T20:55:00Z",
                "2026-11-16T20:58:00Z",
                false,
                true,
                Some("02:00"),
            ),
            (
                "1D",
                "2026-11-15T23:00:00Z",
                "2026-11-16T20:58:00Z",
                false,
                true,
                Some("02:00"),
            ),
            (
                "1W",
                "2026-11-15T23:00:00Z",
                "2026-11-20T20:58:00Z",
                false,
                true,
                Some("02:00"),
            ),
            (
                "1M",
                "2026-10-31T23:00:00Z",
                "2026-11-30T20:58:00Z",
                false,
                true,
                Some("02:00"),
            ),
            (
                "10t",
                "2024-03-11T14:59:00Z",
                "2024-03-11T14:59:30Z",
                false,
                false,
                None,
            ),
        ] {
            let timeframe: Timeframe = interval.parse().unwrap();
            let mut chart = chart(&[100.0], &[110.0]);
            chart.timeframe = Some(timeframe);
            chart.config.session_config = session_config.then(crate::config::SessionConfig::nyse);
            chart.chart_options.time_scale.timezone =
                crate::scales::TimezoneMode::Exchange("Europe/Zurich".parse().unwrap());
            chart.attach_feed(std::sync::Arc::new(Feed), 1);
            let mut instrument =
                Instrument::new("TEST").with_time_zone(TimezoneId::named("Europe/Zurich").unwrap());
            if session {
                let mut hours = Session::new([SessionWindow::new(
                    [Mon, Tue, Wed, Thu, Fri],
                    NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                    NaiveTime::from_hms_opt(16, 0, 0).unwrap(),
                )]);
                // Saturday trades only in extended hours, outside the regular feed's runs.
                hours.extended_windows.push(SessionWindow::new(
                    [Sat],
                    NaiveTime::from_hms_opt(9, 0, 0).unwrap(),
                    NaiveTime::from_hms_opt(16, 0, 0).unwrap(),
                ));
                instrument.session = Some(hours);
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
                "{interval}, session={session}, session_config={session_config}"
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
        let cases: [(Setup, &[&str]); 7] = [
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
            (
                |chart| {
                    chart.add_indicator(Box::new(crate::studies::CustomIndicator::new(
                        "Price units",
                        |bars| vec![crate::studies::IndicatorValue::Single(151.234); bars.len()],
                    )));
                    chart.add_indicator(Box::new(
                        crate::studies::CustomIndicator::new("Pane units", |bars| {
                            vec![crate::studies::IndicatorValue::Single(161.234); bars.len()]
                        })
                        .with_overlay(false)
                        .with_value_format(crate::scales::StudyFormat::Price),
                    ));
                    chart.set_price_formatter(Some(Arc::new(
                        crate::scales::CustomPriceFormatter::new(|value| format!("{value:.3} u")),
                    )));
                    chart.set_quote(Some(crate::model::Level1Quote {
                        ts: Utc.with_ymd_and_hms(2024, 3, 13, 14, 30, 0).unwrap(),
                        symbol: String::new(),
                        bid: 120.125,
                        bid_size: 1.0,
                        ask: 120.5,
                        ask_size: 1.0,
                    }));
                },
                &["120.125 u", "120.500 u", "0.375 u", "161.234 u"],
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
            if chart.quote.is_some() {
                // Sections, so that a row joining a name and a value in one shape counts too.
                let sections = |label: &str| {
                    painted
                        .iter()
                        .filter_map(|shape| match shape {
                            Shape::Text(text)
                                if text.galley.job.sections.iter().any(|section| {
                                    &text.galley.job.text
                                        [section.byte_range.start.0..section.byte_range.end.0]
                                        == label
                                }) =>
                            {
                                Some(text)
                            }
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                };
                let plot = chart.last_rendered_price_rect;
                let overlay = sections("151.234 u");
                assert_eq!(
                    overlay
                        .iter()
                        .filter(|text| text.pos.x < plot.center().x)
                        .count(),
                    1,
                    "the overlay's legend value uses the host formatter"
                );
                assert!(
                    overlay.iter().any(|text| text.pos.x > plot.right()),
                    "the overlay's price axis tag shows the same text"
                );
                assert!(
                    sections("161.234 u")
                        .iter()
                        .any(|text| text.galley.text().starts_with("Pane units")),
                    "the pane legend uses the host formatter"
                );
                let readouts = |chart: &crate::widget::Chart| {
                    chart
                        .study_values_at(50)
                        .into_iter()
                        .filter(|row| row.study.ends_with(" units"))
                        .map(|row| row.formatted_value())
                        .collect::<Vec<_>>()
                };
                assert_eq!(readouts(&chart), ["151.234 u", "161.234 u"], "data window");
                let pane = chart
                    .indicators()
                    .find(|(_, study)| study.name() == "Pane units")
                    .unwrap()
                    .0;
                assert!(chart.set_indicator_formatter(
                    pane,
                    Some(Arc::new(crate::scales::CustomPriceFormatter::new(
                        |value| { format!("{value:.3} p") }
                    ))),
                ));
                chart.set_price_formatter(None);
                assert_eq!(
                    readouts(&chart),
                    ["151.23", "161.234 p"],
                    "a study's own formatter outlasts the chart's"
                );
                assert!(chart.set_indicator_formatter(pane, None));
                assert_eq!(
                    readouts(&chart),
                    ["151.23", "161.23"],
                    "None restores the declared format"
                );
                assert!(!chart.set_indicator_formatter(crate::widget::SeriesId::MAIN, None));
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
    /// in the readout, the data table and the day, month and year marks of the axis; intraday
    /// bars keep the clock of the display zone.
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
        // (interval, first bar, bar read, its label and date, marks that must be painted)
        for (timeframe, first, at, expected, date, required) in [
            (
                Timeframe::Day1,
                day(2024, 1, 1),
                95,
                "Apr 05, 2024",
                "2024-04-05",
                &["Feb", "Mar", "Apr"][..],
            ),
            (
                Timeframe::Week1,
                day(2023, 1, 2),
                95,
                "Oct 28, 2024",
                "2024-10-28",
                &["2024"][..],
            ),
            (
                Timeframe::Month1,
                day(2016, 1, 1),
                95,
                "Dec 01, 2023",
                "2023-12-01",
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
                // The data table and the CSV export write the same date and the bar's instant.
                let cell = chart.data_table().rows.swap_remove(at).swap_remove(0);
                assert_eq!(cell.get(..10), Some(date), "{timeframe:?} {zone:?}: {cell}");
                assert_eq!(
                    chrono::DateTime::parse_from_rfc3339(&cell).map(|time| time.to_utc()),
                    Ok(times[at]),
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

    /// Bar replay completes supplied daily bars, forms the bar of its cursor's date and lets
    /// studies group weeks on the dates the labels give the bars: in the display zone east of
    /// UTC, and in UTC behind it, where the history keeps its 00:00 UTC stamps.
    #[cfg(feature = "timezones")]
    #[test]
    fn replay_follows_the_dates_of_supplied_daily_bars_in_every_zone() {
        use crate::model::{Bar, BarClock, BarData, Timeframe};
        use crate::scales::TimezoneMode;
        use crate::widget::SeriesId;
        use chrono::{Duration, TimeZone, Utc};
        let bar = |time| Bar::new(time, 100.0, 110.0, 100.0, 110.0, 1.0);
        let at = |month, day, hour| Utc.with_ymd_and_hms(2024, month, day, hour, 0, 0).unwrap();
        // The zone, a cursor on 5 February and the zone's midnight starting that date.
        for (zone, cursor, midnight) in [
            ("Asia/Tokyo", at(2, 5, 12), at(2, 4, 15)),
            ("America/New_York", at(2, 5, 17), at(2, 5, 5)),
        ] {
            let mut chart = crate::widget::Chart::new(BarData::from_bars(
                (0..40)
                    .map(|i| bar(at(1, 1, 0) + Duration::days(i)))
                    .collect(),
            ));
            chart.set_timeframe(Timeframe::Day1);
            chart.chart_options.time_scale.timezone = TimezoneMode::Timezone(zone.parse().unwrap());
            #[cfg(feature = "studies")]
            chart.add_indicator(Box::<crate::studies::HigherIntervalCandles>::default());
            // 1 January completes when its date ends, also after a cursor that showed no bar.
            chart.set_replay_time(Some(at(1, 1, 12)));
            chart.set_replay_time(Some(at(1, 2, 6)));
            let early = chart.bars(SeriesId::MAIN).len();
            chart.set_replay_time(Some(cursor));
            let hours = (0..24)
                .map(|i| bar(midnight + Duration::hours(i)))
                .collect();
            assert!(
                chart.set_replay_source(hours, BarClock::new(Timeframe::Hour1)),
                "{zone}"
            );
            let bars = chart.bars(SeriesId::MAIN);
            let labels: Vec<_> = (bars[bars.len() - 2..].iter())
                .map(|bar| chart.format_time(bar.time))
                .collect();
            assert_eq!(
                (early, bars.len(), labels),
                (
                    1,
                    36,
                    vec!["Feb 04, 2024".to_owned(), "Feb 05, 2024".to_owned()]
                ),
                "{zone}"
            );
            #[cfg(feature = "studies")]
            {
                chart.refresh_studies(1.0);
                let study = chart.indicators().next().unwrap().1;
                // The Mondays from 1 January; the last is the bar replay forms.
                let starts: Vec<_> = (study.candles().iter()).map(|c| c.bars.start).collect();
                assert_eq!(starts, [0, 7, 14, 21, 28, 35], "{zone}");
            }
        }
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
