//! Frame rendering: the `show` entry points and the per-frame paint and interaction pass.

use super::selection::hit_test_main_chart;
use super::{Chart, PrimitivePass, RightClickTarget, primitives};
use crate::chart::overlays::FootprintConfig;
use crate::chart::renderers::{self, ChartMapping, RenderContext, StyleColors};
use crate::chart::rendering;
use crate::config::{BackgroundStyle, WatermarkPos};
use crate::data::Footprint;
use crate::localization as l10n;
use crate::model::{ChartType, MarkerZOrder, PriceLine};
use crate::scales::TimeFormatterBuilder;
use crate::theme::ThemeExt;
use egui::{Pos2, Rect, Response, Sense, Ui, Vec2};

impl Chart {
    fn show_realtime_button(&mut self, ui: &Ui, rect: Option<Rect>, chart_id: egui::Id) {
        if let Some(rect) = rect
            && rendering::realtime_button(ui, rect, chart_id, &self.config).clicked()
        {
            self.user_action(|chart| chart.scroll_to_real_time(true));
            ui.ctx().request_repaint();
        }
    }

    /// Formats a timestamp with the chart's zone, clock visibility and bar duration, or a key
    /// as the crosshair label of the [horizontal scale](Self::set_horizontal_scale) reads.
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

    fn current_bar_countdown(&self) -> Option<String> {
        if !self.config.show_countdown || !self.config.show_symbol_last_val {
            return None;
        }
        let last = crate::model::last_with_value(&self.state.data().bars)?;
        let timeframe = self.timeframe?;
        if timeframe.is_trade_based() {
            return None;
        }
        let now = self.replay_time().or_else(|| {
            self.server_time_offset
                .and_then(|offset| chrono::Utc::now().checked_add_signed(offset))
                .or_else(|| {
                    self.feed_link
                        .as_ref()
                        .and_then(super::feed::FeedLink::server_time)
                })
                .or_else(|| self.quote.as_ref().map(|quote| quote.ts))
        })?;
        let session = renderers::exchange_session(
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
    fn time_label_seconds(&self) -> i64 {
        let bars = &self.state.data().bars;
        let visible = self.state.visible_data();
        let Some((first, last)) = visible.first().zip(visible.last()) else {
            return 1;
        };
        let end = bars.partition_point(|bar| bar.time <= last.time);
        let at = bars
            .partition_point(|bar| bar.time < first.time)
            .min(end.saturating_sub(2));
        if end >= 2 {
            (bars[at + 1].time - bars[at].time).num_seconds().abs()
        } else {
            1
        }
    }

    pub(super) fn display_timezone(&self) -> crate::scales::TimezoneMode {
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

    pub(crate) fn build_time_formatter(&self) -> Box<dyn crate::scales::TimeFormatter> {
        let mut builder = TimeFormatterBuilder::new()
            .with_24_hour(true)
            .with_seconds(self.chart_options.time_scale.seconds_visible)
            .with_time_visible(self.chart_options.time_scale.time_visible)
            .with_timezone(self.display_timezone());
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

    /// Names months and weekdays in time axis, crosshair and tooltip labels in `locale`'s
    /// language, such as `"de-DE"` (see [`LocaleTimeFormatter`](crate::scales::LocaleTimeFormatter)).
    /// `None` restores English names.
    ///
    /// The names need the `locales` cargo feature, on by default; without it they stay English.
    pub fn set_locale(&mut self, locale: Option<String>) {
        self.locale = locale;
    }

    /// Writes crosshair and tooltip dates with a `strftime` pattern, such as `"%d.%m.%Y"`, followed
    /// by the clock for intraday bars. `None` restores the built-in dates, which an invalid pattern
    /// also falls back to.
    pub fn set_date_format(&mut self, date_format: Option<String>) {
        self.date_format = date_format;
    }

    /// Formats time axis marks with [`TimeFormatter::format`], which receives the
    /// [`TickMarkType`](crate::scales::TickMarkType) of each mark, and crosshair, tooltip and
    /// [`format_time`](Self::format_time) labels with [`TimeFormatter::format_crosshair`].
    ///
    /// The formatter replaces the built-in one, the locale and the date format. It receives
    /// wall-clock times in the chart's time zone, and the time scale's
    /// [`time_visible`](crate::config::TimeScaleOptions::time_visible) option still applies.
    /// `None` restores the built-in formatter.
    ///
    /// [`TimeFormatter::format`]: crate::scales::TimeFormatter::format
    /// [`TimeFormatter::format_crosshair`]: crate::scales::TimeFormatter::format_crosshair
    pub fn set_time_formatter(
        &mut self,
        formatter: Option<std::sync::Arc<dyn crate::scales::TimeFormatter>>,
    ) {
        self.time_formatter = formatter;
    }

    /// Formats price scale labels in the normal and logarithmic modes: the crosshair label and
    /// price tags with [`PriceFormatter::format`], and the axis ticks with
    /// [`PriceFormatter::format_tick`]. The formatter receives prices and replaces the
    /// instrument's decimals. It applies to every price scale; studies keep their own formats.
    /// `None` restores the built-in labels.
    ///
    /// [`PriceFormatter::format`]: crate::scales::PriceFormatter::format
    /// [`PriceFormatter::format_tick`]: crate::scales::PriceFormatter::format_tick
    pub fn set_price_formatter(
        &mut self,
        formatter: Option<std::sync::Arc<dyn crate::scales::PriceFormatter>>,
    ) {
        self.price_formatter = formatter;
    }

    /// Formats price scale labels in the percentage and indexed-to-100 modes, as
    /// [`set_price_formatter`](Self::set_price_formatter) does in the others. The formatter
    /// receives the percentage or index value the axis shows. `None` restores signed
    /// percentages and index values.
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
    /// readouts, price tags and ticks; percentage and indexed scales retain the chart's
    /// percentage formatter. `None` restores chart-wide formatting. Returns false for an
    /// unknown series. A shared axis uses the first visible series' formatter.
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
            _ => self
                .series_formatters
                .get(&id)
                .cloned()
                .or_else(|| self.price_formatter.clone()),
        }
    }

    fn value_formatters(&self) -> renderers::ValueFormatters<'_> {
        renderers::ValueFormatters {
            price: self
                .series_formatters
                .get(&super::SeriesId::MAIN)
                .or(self.price_formatter.as_ref())
                .map(|formatter| formatter.as_ref()),
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

    /// Draw the chart background (solid or gradient)
    fn draw_background(&self, painter: &egui::Painter, rect: Rect) {
        // Skip background when chart is inside a container that handles its own background
        if self.config.skip_background {
            return;
        }

        match self.config.background_style {
            BackgroundStyle::Solid => {
                painter.rect_filled(rect, 0.0, self.config.background_color);
            }
            BackgroundStyle::VerticalGradient {
                top_color,
                bottom_color,
            } => {
                // Draw vertical gradient using a mesh
                let mesh = egui::Mesh {
                    indices: vec![0, 1, 2, 2, 3, 0],
                    vertices: vec![
                        egui::epaint::Vertex {
                            pos: rect.left_top(),
                            uv: egui::epaint::WHITE_UV,
                            color: top_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.right_top(),
                            uv: egui::epaint::WHITE_UV,
                            color: top_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.right_bottom(),
                            uv: egui::epaint::WHITE_UV,
                            color: bottom_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.left_bottom(),
                            uv: egui::epaint::WHITE_UV,
                            color: bottom_color,
                        },
                    ],
                    texture_id: egui::TextureId::default(),
                };
                painter.add(egui::Shape::mesh(mesh));
            }
            BackgroundStyle::HorizontalGradient {
                left_color,
                right_color,
            } => {
                // Draw horizontal gradient using a mesh
                let mesh = egui::Mesh {
                    indices: vec![0, 1, 2, 2, 3, 0],
                    vertices: vec![
                        egui::epaint::Vertex {
                            pos: rect.left_top(),
                            uv: egui::epaint::WHITE_UV,
                            color: left_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.right_top(),
                            uv: egui::epaint::WHITE_UV,
                            color: right_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.right_bottom(),
                            uv: egui::epaint::WHITE_UV,
                            color: right_color,
                        },
                        egui::epaint::Vertex {
                            pos: rect.left_bottom(),
                            uv: egui::epaint::WHITE_UV,
                            color: left_color,
                        },
                    ],
                    texture_id: egui::TextureId::default(),
                };
                painter.add(egui::Shape::mesh(mesh));
            }
        }
    }

    /// Draw watermark overlay (large symbol name)
    fn draw_watermark(&self, painter: &egui::Painter, rect: Rect) {
        if !self.config.show_watermark {
            return;
        }

        let text = self
            .config
            .watermark_text
            .as_deref()
            .unwrap_or(&self.symbol);
        if text.is_empty() {
            return;
        }

        let font_size = crate::styles::typography::font_size(self.config.watermark_font_size);
        let font_id = egui::FontId::proportional(font_size);

        // Calculate position based on watermark_pos
        let pos = match self.config.watermark_pos {
            WatermarkPos::Center => rect.center(),
            WatermarkPos::TopLeft => Pos2::new(rect.min.x + 20.0, rect.min.y + font_size),
            WatermarkPos::TopRight => Pos2::new(rect.max.x - 20.0, rect.min.y + font_size),
            WatermarkPos::BottomLeft => Pos2::new(rect.min.x + 20.0, rect.max.y - 20.0),
            WatermarkPos::BottomRight => Pos2::new(rect.max.x - 20.0, rect.max.y - 20.0),
        };

        let anchor = match self.config.watermark_pos {
            WatermarkPos::Center => egui::Align2::CENTER_CENTER,
            WatermarkPos::TopLeft => egui::Align2::LEFT_TOP,
            WatermarkPos::TopRight => egui::Align2::RIGHT_TOP,
            WatermarkPos::BottomLeft => egui::Align2::LEFT_BOTTOM,
            WatermarkPos::BottomRight => egui::Align2::RIGHT_BOTTOM,
        };

        painter.text(pos, anchor, text, font_id, self.config.watermark_color);
    }

    /// Renders the chart with its studies and drawings, and handles pan, zoom,
    /// crosshair, keyboard shortcuts, the drawing tools and all visual elements
    /// configured in [`ChartConfig`](crate::config::ChartConfig).
    ///
    /// Overlay studies (moving averages, Bollinger Bands) are drawn on the main
    /// pane, any other visible study (RSI, MACD) in a pane below it with its
    /// x-axis aligned to the main pane; [`Chart::get_rendered_indicator_panes`]
    /// returns their layout. A chart built with a feed first asks it for what it needs and
    /// applies what it sent. Studies are then brought up to date: an incremental study at once,
    /// and at most one study per frame in full, the one that has waited longest.
    ///
    /// On price-based types the time scale and reported indices count elements, one OHLC bar
    /// per brick, line, column or range bar. Each carries the last contributing bar's time.
    /// Studies use element OHLC on price-based types; time annotations follow element slots.
    /// Source-bar study results remain available through [`Self::indicators`].
    ///
    /// Reports the series summary and hovered bar values to accessibility clients. Native
    /// applications using `eframe` enable its `accesskit` feature to connect to screen readers.
    ///
    /// Returns an [`egui::Response`] for additional interaction handling.
    pub fn show(&mut self, ui: &mut Ui) -> Response {
        self.show_content(ui, None)
    }

    /// Draws a footprint in place of the price series, below studies, cursor and legend.
    /// Supply executions aggregated with the same clock as the chart's source bars.
    /// Price-based chart types keep their normal series because their x-axis is not time.
    pub fn show_footprint(
        &mut self,
        ui: &mut Ui,
        flow: &Footprint,
        config: &FootprintConfig,
    ) -> Response {
        self.show_content(ui, Some((flow, config)))
    }

    fn show_content(
        &mut self,
        ui: &mut Ui,
        footprint: Option<(&Footprint, &FootprintConfig)>,
    ) -> Response {
        let footprint = footprint.filter(|_| self.replay.is_none());
        let theme = self.theme_override.clone();
        // Axis labels take their size from the theme's label text.
        let scaled = self.config.axis_font_size.map(|size| {
            let mut scaled = theme.clone().unwrap_or_else(|| ui.ctx().active_theme());
            scaled.text.label = crate::styles::typography::font_size(size);
            scaled
        });
        let _theme_scope = scaled
            .as_ref()
            .or(theme.as_ref())
            .map(|theme| crate::theme::ThemeScope::new(ui.ctx(), theme));
        let size = self.size;
        let show = |ui: &mut Ui| {
            if let Some(theme) = &theme {
                crate::theme::apply_to_style(ui.style_mut(), theme);
            }
            self.show_themed_content(ui, footprint)
        };
        // Keep the same UI scope when an override is cleared, preserving focus and widget state.
        match size {
            Some(size) => ui.allocate_ui(size, show).inner,
            None => ui.scope(show).inner,
        }
    }

    fn show_themed_content(
        &mut self,
        ui: &mut Ui,
        footprint: Option<(&Footprint, &FootprintConfig)>,
    ) -> Response {
        self.follow_chart_theme(&ui.active_theme());
        self.host_events.volume_map = None;
        self.host_events.target = None;
        self.apply_config_scale_mode();
        #[cfg(feature = "studies")]
        self.sync_volume_study(ui.ctx());
        #[cfg(feature = "drawings")]
        self.sync_stored_drawings(ui.ctx());
        self.pump(ui.ctx());
        if let Some(wait) = self.refresh_studies(ui.input(|i| i.time)) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(wait));
        }
        self.show_elements();
        if let Some(wait) = self.refresh_element_studies(ui.input(|i| i.time)) {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs_f64(wait));
        }
        self.sync_primitives(ui.ctx());
        let frame = ui.ctx().cumulative_frame_nr();
        let width = ui.available_width();
        let resized = self
            .last_layout
            .is_none_or(|(previous, changed)| previous != width || changed == frame);
        let previous_axis_width = self.measured_axis_width;
        self.last_rendered_indicator_panes.clear();
        self.sync_panes();
        for pane in &mut self.pane_layout.panes {
            pane.rect = Rect::NOTHING;
        }
        // Panes share the available height and the lowest time axis.
        let has_panes = self.has_panes();
        let response = if has_panes {
            self.show_with_panes(ui, footprint)
        } else {
            let response = self.show_internal(ui, None, None, footprint);
            if let Some(main) = self
                .pane_layout
                .panes
                .iter_mut()
                .find(|pane| pane.id == super::PaneId::MAIN)
            {
                main.rect = response.rect;
            }
            response
        };
        if !self.chart_type.is_time_independent() {
            let source_marks = self.mark_source.as_ref().map(|source| &source.page);
            let mut coords = self.get_chart_mapping();
            coords.start_idx = 0;
            let baseline = if has_panes {
                response.rect.bottom() - self.time_axis_height().min(response.rect.height())
            } else {
                coords.rect.bottom()
            };
            coords.rect.max.y = baseline;
            // Source pages cover the view; host lists are windowed before cloning or formatting.
            // Keys keep each badge's interaction stable as the view moves.
            let (from, to) = super::marks::window(self.state.shown(), &coords, coords.rect);
            let mut events: Vec<_> = self
                .events
                .within(from, to)
                .into_iter()
                .map(|index| (index, self.events[index].clone()))
                .collect();
            let mut key = self.events.len();
            if let Some(source) = source_marks {
                events.extend(
                    source
                        .time_scale_marks
                        .iter()
                        .cloned()
                        .enumerate()
                        .map(|(index, event)| (key + index, event)),
                );
                key += source.time_scale_marks.len();
            }
            let typed_count = self.time_scale_events.len();
            let typed = self
                .time_scale_events
                .within(from, to)
                .into_iter()
                .map(|index| (key + index, &self.time_scale_events[index]))
                .chain(source_marks.into_iter().flat_map(|page| {
                    page.events
                        .iter()
                        .enumerate()
                        .map(move |(index, event)| (key + typed_count + index, event))
                }));
            let mut breaks = Vec::new();
            for (key, event) in typed {
                if event.kind == crate::model::EventKind::Earnings
                    && self.event_visibility.show_earnings_breaks
                {
                    breaks.push(event.time);
                }
                if self.event_visibility.shows(event.kind) {
                    events.push((
                        key,
                        crate::model::Marker::event(event.time, event.kind.label())
                            .with_id(&event.id)
                            .with_tooltip(format!("{}\n{}", event.title, event.details)),
                    ));
                }
            }
            renderers::render_event_breaks(ui, &breaks, self.state.shown(), &coords);
            if let Some(id) = renderers::render_event_badges(
                ui,
                events.iter().map(|(key, event)| (*key, event)),
                self.state.shown(),
                &coords,
                baseline,
            ) {
                self.event_click = Some(id);
            }
        }
        if self.last_rendered_price_rect.width() > 1.0
            && self
                .last_layout
                .is_none_or(|(previous, _)| previous != width)
        {
            self.last_layout = Some((width, frame));
        }
        if (self.measured_axis_width - previous_axis_width).abs() > 0.5 {
            let old_width = self.last_rendered_price_rect.width();
            let new_width = self
                .calculate_layout_rects(response.rect)
                .price_rect
                .width();
            // Sizing passes follow the same zoom policy as the viewport resize.
            if (!resized
                || self.desired_visible_bars.is_some_and(|count| count > 0)
                || self
                    .chart_options
                    .time_scale
                    .lock_visible_time_range_on_resize)
                && previous_axis_width > 0.0
                && old_width > 0.0
                && new_width > 0.0
            {
                let scale = self.state.time_scale_mut();
                scale.set_bar_spacing(scale.bar_spacing() * new_width / old_width);
            }
            ui.ctx().request_discard("Chart measured its axis labels");
        }
        self.record_host_events(ui);
        let needs_info = response.clicked()
            || response.double_clicked()
            || response.triple_clicked()
            || response.gained_focus()
            || response.changed()
            || ui
                .ctx()
                .accesskit_node_builder(response.id, |_| ())
                .is_some();
        #[cfg(debug_assertions)]
        let needs_info = needs_info || ui.ctx().global_style().debug.show_interactive_widgets;
        if needs_info {
            // Translation callbacks must run outside the context locks held by widget_info.
            let info = self.accessibility_info(ui.ctx(), response.enabled());
            response.widget_info(|| info.clone());
        }
        // Within the frame of the edit, in case it is the last.
        #[cfg(feature = "drawings")]
        self.save_stored_drawings();
        response
    }

    /// Pulses at the last price of a line-based chart as its `last_price_animation` asks, and
    /// keeps repainting while a visible pulse runs.
    fn paint_last_price_pulse(
        &self,
        ui: &Ui,
        painter: &egui::Painter,
        price_rect: Rect,
        coords: &ChartMapping,
    ) {
        use crate::model::LastPriceAnimation;
        let styles = &self.chart_type_params.styles;
        let animation = styles.last_price_animation;
        let Some(index) = self
            .state
            .shown()
            .iter()
            .rposition(|bar| !bar.is_whitespace())
        else {
            return;
        };
        let last = &self.state.shown()[index];
        let price = self
            .config
            .price_source
            .compute(last.open, last.high, last.low, last.close);
        let Some(color) = rendering::candles::line_color_at(
            self.chart_type,
            styles,
            self.chart_type_params.baseline,
            &coords.price,
            price_rect,
            price,
            last,
        )
        .filter(|_| animation != LastPriceAnimation::Disabled) else {
            return;
        };
        let now = ui.input(|input| input.time);
        let started = match self.last_price_pulse.get() {
            Some((time, seen, started)) if time == last.time && seen.total_cmp(&price).is_eq() => {
                started
            }
            previous => {
                // The first price seen is the loaded history, not an update.
                let started = previous.map_or(f64::NEG_INFINITY, |_| now);
                self.last_price_pulse.set(Some((last.time, price, started)));
                started
            }
        };
        let period = rendering::candles::LAST_PRICE_PULSE_SECONDS;
        let phase = match animation {
            LastPriceAnimation::Continuous => (now / period).fract(),
            _ => (now - started) / period,
        };
        let point = Pos2::new(coords.idx_to_x(index), coords.price.price_to_y(price));
        if (0.0..1.0).contains(&phase) && price_rect.contains(point) {
            rendering::candles::render_last_price_pulse(painter, point, color, phase as f32);
            ui.ctx().request_repaint();
        }
    }

    /// Resolve the timeframe used to pick a session-break granularity.
    ///
    /// The chart's timeframe is the primary source. Without one, the cadence
    /// is inferred from the visible bars by taking the median gap between
    /// consecutive timestamps and snapping it to the nearest preset, so session
    /// breaks work even when the host never set a timeframe. Falls back to the
    /// default (1-minute) when there are too few bars to measure.
    fn resolve_session_timeframe(
        &self,
        visible_data: &[crate::model::Bar],
    ) -> crate::model::Timeframe {
        use crate::model::Timeframe;

        if let Some(tf) = self.timeframe {
            return tf;
        }

        // Infer from the median inter-bar gap (robust to session-edge jumps).
        if visible_data.len() < 2 {
            return Timeframe::default();
        }
        // An even sample of the gaps keeps a zoomed-out frame from sorting all of them.
        let mut gaps_ms: Vec<i64> = visible_data
            .windows(2)
            .step_by((visible_data.len() / 1024).max(1))
            .map(|w| (w[1].time - w[0].time).num_milliseconds())
            .filter(|&g| g > 0)
            .collect();
        if gaps_ms.is_empty() {
            return Timeframe::default();
        }
        gaps_ms.sort_unstable();
        let median = gaps_ms[gaps_ms.len() / 2];

        // Snap to the closest preset by duration.
        Timeframe::all()
            .into_iter()
            .min_by_key(|tf| (tf.duration_ms() - median).abs())
            .unwrap_or_default()
    }

    /// Internal rendering method that orchestrates all modules
    pub(crate) fn show_internal(
        &mut self,
        ui: &mut Ui,
        shared_time_axis: Option<Rect>,
        shared_plot: Option<&Response>,
        footprint: Option<(&Footprint, &FootprintConfig)>,
    ) -> Response {
        // Register the SVG and icon image loaders once per context so the
        // embedded icons used by the toolbars and panels decode without the
        // host app having to wire egui_extras itself.
        #[cfg(feature = "icons")]
        {
            let ctx = ui.ctx().clone();
            let loaders_installed = egui::Id::unique("egui_charts::image_loaders_installed");
            if !ctx.data(|d| d.get_temp::<bool>(loaders_installed).unwrap_or(false)) {
                egui_extras::install_image_loaders(&ctx);
                crate::icons::install_loader(&ctx);
                ctx.data_mut(|d| d.insert_temp(loaders_installed, true));
            }
        }

        // Reset zoom_just_applied flag at the start of each frame
        self.zoom_just_applied = false;

        // A right-click result lives for exactly one frame: clear any stale
        // capture so a target that is never drained does not reappear later.
        self.right_click = None;

        let available_size = ui.available_size();
        let (mut response, mut painter) =
            ui.allocate_painter(available_size, Sense::click_and_drag());
        let rect = response.rect;

        if let Some(axis) = shared_time_axis {
            painter.set_clip_rect(ui.clip_rect().intersect(if rect.height() == 0.0 {
                axis
            } else {
                rect.union(axis)
            }));
            response = response.union(ui.interact(
                axis,
                response.id.with("shared_time_axis"),
                Sense::click_and_drag(),
            ));
        }
        let layout = self.pane_layout_rects(
            rect,
            self.config.show_time_labels && shared_time_axis.is_none(),
        );
        let chart_rect = layout.price_rect;
        let time_axis_top = shared_time_axis.map_or(chart_rect.bottom(), |axis| axis.top());

        self.handle_secondary_axes(ui, rect, chart_rect);
        let chart_width = chart_rect.width();

        // CRITICAL: Apply TimeScale width configuration BEFORE any zoom operations
        // This ensures apply_constraints() inside zoom() uses the correct self.width
        // to calculate constraint bounds. Without this, drawings drift during zoom
        // because constraints are calculated with stale width values.
        self.apply_timescale_config(chart_width);
        self.advance_scroll_animation(ui);

        let navigation_rect = self.navigation_rect(ui, chart_rect);
        let navigation = navigation_rect
            .filter(|_| self.navigation_visible(ui, chart_rect))
            .map(|nav| {
                (
                    nav,
                    self.interact_navigation(ui, nav, chart_rect, response.id),
                )
            });
        let scale = self.state.time_scale();
        let mut live_scale = scale.clone();
        live_scale.scroll_to_realtime();
        let realtime_rect = (self.config.show_realtime_btn
            && !self.state.data().is_empty()
            && (scale.right_offset_precise() - live_scale.right_offset_precise()).abs() > 1e-6)
            .then(|| {
                rendering::realtime_button_rect(
                    chart_rect,
                    self.config.realtime_button_size,
                    self.config.realtime_button_pos,
                    navigation_rect.filter(|_| {
                        self.config.navigation_buttons != crate::config::NavigationButtons::Never
                    }),
                )
            })
            .flatten();
        let realtime_interaction = realtime_rect.is_some_and(|button| {
            ui.input(|input| {
                (input.pointer.any_down() || input.pointer.any_released())
                    && input
                        .pointer
                        .press_origin()
                        .or_else(|| input.pointer.hover_pos())
                        .is_some_and(|pos| button.contains(pos))
            })
        });
        let navigation_interaction = navigation_rect.is_some_and(|nav| {
            self.config.navigation_buttons != crate::config::NavigationButtons::Never
                && !self.state.data().is_empty()
                && ui.input(|i| {
                    (i.pointer.any_down() || i.pointer.any_released())
                        && i.pointer
                            .press_origin()
                            .or_else(|| i.pointer.hover_pos())
                            .is_some_and(|p| nav.contains(p))
                })
        });
        // A button owns the whole gesture, including a drag that leaves the row. The chart's
        // own response goes back to the caller; the gesture handlers see a hover-only one.
        let shown =
            shared_plot.map_or_else(|| response.clone(), |plot| response.union(plot.clone()));
        let axis_corner = Rect::from_min_max(
            Pos2::new(
                if self.config.show_right_axis {
                    chart_rect.right()
                } else {
                    rect.left()
                },
                time_axis_top,
            ),
            Pos2::new(
                if self.config.show_right_axis {
                    rect.right()
                } else {
                    chart_rect.left()
                },
                time_axis_top + 28.0,
            ),
        );
        let axis_interaction = cfg!(feature = "icons")
            && (self.config.show_right_axis || self.config.show_left_axis)
            && ui.input(|input| {
                (input.pointer.any_down() || input.pointer.any_released())
                    && input
                        .pointer
                        .press_origin()
                        .or(input.pointer.hover_pos())
                        .is_some_and(|pos| {
                            axis_corner.contains(pos)
                                || self.axis_hover.is_some_and(|(_, tag)| tag.contains(pos))
                        })
            });
        if navigation_interaction || realtime_interaction || axis_interaction {
            response = ui.interact(
                Rect::NOTHING,
                response.id.with("navigation_input"),
                Sense::hover(),
            );
            response.rect = rect;
        }

        // The visible pane drives time navigation even while the price pane has no height.
        let mut time_response =
            shared_plot.map_or_else(|| response.clone(), |plot| response.union(plot.clone()));
        // Handle tracking mode
        self.handle_tracking_mode(ui, &time_response);

        // Request focus on hover for keyboard shortcuts
        self.request_focus_if_needed(&mut time_response);

        // Handle keyboard shortcuts
        self.handle_keyboard_shortcuts(ui, &time_response, chart_width, chart_rect.min.x);

        // Calculate visible bars
        let logical_range = self.state.time_scale().visible_logical_range();
        let visible_bars = logical_range.length().ceil() as usize;
        self.last_visible_bars = visible_bars;

        // Show grabbing cursor during panning
        self.set_panning_cursor(ui, &time_response);

        // Define axis rects
        let price_axis_rect = Rect::from_min_max(
            Pos2::new(axis_corner.left(), chart_rect.top()),
            Pos2::new(axis_corner.right(), chart_rect.bottom()),
        );
        let time_axis_rect = shared_time_axis.unwrap_or_else(|| {
            Rect::from_min_max(
                Pos2::new(chart_rect.min.x, chart_rect.max.y),
                Pos2::new(chart_rect.max.x, rect.max.y),
            )
        });
        self.time_axis_rect = Rect::from_min_size(
            time_axis_rect.min,
            Vec2::new(
                time_axis_rect.width(),
                self.time_axis_height().min(time_axis_rect.height()),
            ),
        );

        // Handle double-click to reset axes
        self.handle_double_click(&time_response, price_axis_rect, time_axis_rect);

        // Handle mouse wheel for zoom/scroll
        // Block pan/zoom when drawing tool is active OR when manipulating a drawing
        #[cfg(feature = "drawings")]
        let eraser_mode = self.drawings.eraser_mode;
        #[cfg(not(feature = "drawings"))]
        let eraser_mode = false;
        #[cfg(feature = "drawings")]
        let drawing_tool_active = self.drawings.active_tool.is_some();
        #[cfg(not(feature = "drawings"))]
        let drawing_tool_active = false;
        let time_based = !self.chart_type.is_time_independent();
        let show_volume = self.shows_volume();
        #[cfg(not(feature = "drawings"))]
        let is_drawing_interaction = false;
        // The Magic cursor marks while the button is held, so a drag paints rather than pans.
        #[cfg(feature = "drawings")]
        let is_drawing_interaction = self.drawings.active_tool.is_some()
            || self.chart_options.crosshair.style == crate::config::CrosshairStyle::Magic
            || self.drawings.dragging_handle.is_some()
            || self.drawings.text_editor.is_some()
            || self.drawings.curr_drawing.is_some()
            || ui
                .input(|i| i.pointer.press_origin())
                .is_some_and(|origin| {
                    self.drawings
                        .selected_ids()
                        .any(|id| self.drawings.hit_test_handle_by_id(origin, id).is_some())
                        || self
                            .drawings
                            .hit_test(origin)
                            .is_some_and(|id| self.drawings.is_selected(id))
                });
        let pending_price_zoom = self.handle_mouse_wheel(
            ui,
            &time_response,
            chart_width,
            chart_rect.min.x,
            price_axis_rect,
        );

        // Handle pinch-to-zoom for touch/trackpad gestures
        self.handle_pinch_zoom(ui, &time_response, chart_width, chart_rect.min.x);

        // Handle drag to pan (blocked when interacting with drawings)
        self.handle_drag_pan(
            ui,
            &time_response,
            price_axis_rect,
            time_axis_rect,
            chart_rect.min.x,
            is_drawing_interaction,
        );

        // Apply kinetic scrolling
        self.apply_kinetic_scroll(ui);

        // Handle box zoom (only when zoom mode is active from toolbar)
        // Right-click is reserved for context menu, zoom uses left-click when mode is active
        self.zoom_just_applied = self.handle_box_zoom(
            ui,
            &response,
            chart_rect,
            chart_width,
            self.zoom_mode_active,
        );
        if self.zoom_just_applied {
            log::info!("Zoom applied - chart will auto-deactivate zoom mode");
        }

        // Set zoom-in cursor when zoom mode is active
        if self.zoom_mode_active && response.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ZoomIn);
        }
        // Set cursor icon based on crosshair style (cursor type)
        // This is only for default case - drawing manager may override for eraser mode
        else if response.hovered() {
            use crate::config::CrosshairStyle;
            match self.chart_options.crosshair.style {
                CrosshairStyle::Full => {
                    // Cross cursor mode - show crosshair cursor
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
                }
                CrosshairStyle::Dot => {
                    // Dot mode - default pointer (dot is rendered on chart)
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
                }
                CrosshairStyle::Arrow
                | CrosshairStyle::Demonstration
                | CrosshairStyle::Magic
                | CrosshairStyle::Hidden => {
                    // Arrow mode - default pointer
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Default);
                }
            }
        }

        // Draw background (solid or gradient)
        self.draw_background(&painter, rect);
        if let Some(axis) = shared_time_axis {
            self.draw_background(
                &painter,
                Rect::from_min_max(
                    Pos2::new(rect.left(), axis.top()),
                    Pos2::new(rect.right(), axis.bottom()),
                ),
            );
        }

        self.pump_signals(ui.ctx());
        self.pump_targets(ui.ctx());

        // Draw watermark overlay (if enabled)
        self.draw_watermark(&painter, chart_rect);
        if let Some(watermark) = &self.watermark {
            renderers::render_watermark(ui, &painter, chart_rect, watermark);
        }

        if self.state.data().is_empty() {
            self.pump_marks(ui.ctx());
            painter.text(
                rect.center(),
                egui::Align2::CENTER_CENTER,
                l10n::text(ui.ctx(), "No data available"),
                egui::FontId::proportional(ui.active_theme().text.title),
                self.config.text_color,
            );
            return response;
        }

        let price_rect = layout.price_rect;
        let volume_rect = layout.volume_rect;

        // Get visible range
        let (start_idx, end_idx) = self.state.visible_range();
        let (registry, bars) = if self.chart_type.is_time_independent() {
            (
                &mut self.element_studies.studies,
                &self.element_studies.bars[..],
            )
        } else {
            (&mut self.studies, self.state.data().bars.as_slice())
        };
        let mut changed: Vec<_> = registry
            .indicators_mut()
            .iter_mut()
            .enumerate()
            .filter_map(|(at, study)| study.set_visible_range(start_idx..end_idx).then_some(at))
            .collect();
        if !changed.is_empty() {
            let requests = registry
                .indicators()
                .iter()
                .flat_map(|study| study.data_requests())
                .collect();
            let sources = self.study_data.data(
                requests,
                &self.symbol,
                self.timeframe,
                self.replay.as_ref().map(|replay| replay.time),
            );
            for at in super::studies::order(registry.indicators(), &self.study_ids) {
                if let Some(crate::studies::StudySource::Study { id, .. }) =
                    registry.indicators()[at].source()
                    && let Some(parent) = self
                        .study_ids
                        .iter()
                        .position(|candidate| candidate.0 == id)
                    && changed.contains(&parent)
                    && !changed.contains(&at)
                {
                    changed.push(at);
                }
                if changed.contains(&at) {
                    super::studies::update(
                        registry.indicators_mut(),
                        &self.study_ids,
                        at,
                        bars,
                        0,
                        &sources,
                    );
                }
            }
        }
        self.start_idx = start_idx;

        // With several bars to a pixel, read and draw them in groups of a pixel (see `merge`).
        let factor = super::merge::factor(
            self.state.time_scale().bar_spacing(),
            ui.ctx().pixels_per_point(),
        )
        .min(if time_based { usize::MAX } else { 1 });
        self.merge_groups.update(
            self.state.shown(),
            self.state.revision(),
            factor,
            self.config.price_source,
        );

        // Fit the price scale, then apply this frame's price zoom
        let series_values = self.fit_secondary_series();
        let primitive_prices = self.main_primitive_prices();
        let target_prices = self.target_prices();
        let source_marks = self
            .mark_source
            .as_ref()
            .map(|source| &source.page.bar_marks);
        let marker_prices = self
            .marker_prices(
                &self.marks,
                source_marks.map_or(&[], |marks| &marks[..]),
                self.marker_options,
            )
            .filter(|_| self.main_series_visible && time_based)
            .collect::<Vec<_>>();
        let heikin_ashi = (self.chart_type == crate::model::ChartType::Heikin).then(|| {
            self.heikin_ashi
                .bars(&self.state.data().bars, self.state.revision())
        });
        let transform = self.state.fit_price_scale_with_series(
            (if self.chart_type.is_time_independent() {
                &self.element_studies.studies
            } else {
                &self.studies
            })
            .indicators()
            .iter()
            .zip(&self.study_autoscale)
            .filter(|(study, autoscale)| {
                !self.config.scale_price_chart_only && **autoscale && !study.is_volume()
            })
            .map(|(study, _)| study.as_ref()),
            heikin_ashi.as_ref().map(|bars| &bars[start_idx..end_idx]),
            self.main_series_visible,
            self.main_series_style.autoscale.as_ref(),
            (factor > 1
                && heikin_ashi.is_none()
                && self.state.price_scale().options().mode
                    != crate::scales::PriceScaleMode::Logarithmic)
                .then(|| {
                    self.merge_groups
                        .price_range(self.state.shown(), start_idx..end_idx)
                })
                .flatten(),
            marker_prices
                .into_iter()
                .chain(primitive_prices)
                .chain(target_prices),
            series_values,
        );
        drop(heikin_ashi);
        self.apply_price_zoom(pending_price_zoom, &response, price_rect, transform);
        let bar_spacing = self.state.time_scale().bar_spacing();
        if self.config.lock_price_to_bar_ratio != self.last_ratio_lock {
            self.state.price_scale_mut().set_bar_ratio_locked(
                self.config.lock_price_to_bar_ratio,
                bar_spacing,
                price_rect.height(),
            );
        }
        self.config.lock_price_to_bar_ratio = self.state.price_scale().bar_ratio_locked();
        self.last_ratio_lock = self.config.lock_price_to_bar_ratio;
        self.state
            .price_scale_mut()
            .update_bar_ratio(bar_spacing, price_rect.height());
        let price_map = self
            .state
            .price_scale()
            .map(transform, price_rect.min.y, price_rect.max.y);

        self.update_shared_study_maps(price_rect);
        let price_formatter =
            self.series_label_formatter(super::SeriesId::MAIN, price_map.transform().mode);

        // Store the final price mapping and rects for external use (selection dots, hit testing)
        self.last_price_map = price_map;
        self.last_rendered_price_rect = price_rect;
        self.last_rendered_volume_rect = volume_rect;

        self.begin_mark_hits();
        self.pump_marks(ui.ctx());

        // Get visible data
        let visible_data = self.state.visible_data();

        if visible_data.is_empty()
            && !self
                .price_series
                .iter()
                .any(|item| item.visible && !item.bars.is_empty())
        {
            if let Some((nav, buttons)) = navigation {
                self.paint_navigation(ui, nav, buttons);
            }
            self.show_realtime_button(ui, realtime_rect, shown.id);
            return shown;
        }

        let groups = (factor > 1).then(|| {
            let covering = self
                .merge_groups
                .covering(start_idx..start_idx + visible_data.len());
            (covering.start, &self.merge_groups.groups[covering])
        });
        // The first bar of each group, at the group's position, for the time axis and sessions.
        let time_bars: Option<Vec<_>> =
            groups.map(|(_, groups)| groups.iter().map(|group| group.bar.clone()).collect());

        // Largest visible volume: the volume pane and volume candles scale by it
        let max_volume = time_bars
            .as_deref()
            .unwrap_or(visible_data)
            .iter()
            .map(|c| c.volume)
            .filter(|v| v.is_finite())
            .max_by(f64::total_cmp)
            .unwrap_or(1.0);

        // Time and price grid lines sharing a style are painted together.
        let horizontal_grid = self.config.show_grid && self.config.show_horizontal_grid;
        let time_grid = time_based && self.config.show_grid && self.config.show_vertical_grid;
        if horizontal_grid && !time_grid {
            rendering::render_grid(
                &painter,
                price_rect,
                &price_map,
                self.config.grid_color,
                self.config.grid_line_style,
            );
        }

        // Create rendering contexts
        let bar_spacing = self.state.time_scale().bar_spacing();
        let bar_width = bar_spacing * self.config.candle_width;
        let price_ctx = RenderContext::new(&painter, price_rect);
        let coords = ChartMapping::new(
            price_rect,
            bar_spacing,
            start_idx,
            self.state.time_scale().base_idx(),
            self.state.time_scale().right_offset_precise(),
            price_map,
        );
        let (time_data, time_coords) = match (&time_bars, groups) {
            (Some(bars), Some((start, _))) => (
                bars.as_slice(),
                super::merge::group_mapping(&coords, factor, start),
            ),
            _ => (visible_data, coords),
        };
        let colors = StyleColors {
            bullish: self.config.bullish_color,
            bearish: self.config.bearish_color,
            text: self.config.text_color,
            bullish_border: self.config.bullish_border_color,
            bearish_border: self.config.bearish_border_color,
            bullish_wick: self.config.bullish_wick_color,
            bearish_wick: self.config.bearish_wick_color,
            candle_border_width: self.config.candle_border_width,
        };

        let timezone = self.display_timezone();
        let time_scale = self.build_time_formatter();
        let host_scale = self.horizontal_scale.clone();
        let scale: &dyn crate::scales::HorizontalScale =
            host_scale.as_deref().unwrap_or(&time_scale);
        // The grid and the labels share one set of marks and the merged bars' mapping.
        let (time_marks, time_points) = if time_grid || self.config.show_time_labels {
            // Resolved once: a fixed-offset zone allocates on every clone.
            let calendar = self.calendar_timezone();
            renderers::time_axis_marks(
                &RenderContext::new(&painter, chart_rect),
                time_data,
                &time_coords,
                scale,
                &self.chart_options.time_scale,
                |index| {
                    crate::model::time_at_with_timeframe(
                        self.state.shown(),
                        index * factor as f64,
                        self.timeframe(),
                        calendar.as_ref(),
                    )
                },
            )
        } else {
            Default::default()
        };

        if self.config.show_grid && self.config.show_vertical_grid {
            let style = self
                .config
                .vertical_grid_line_style
                .unwrap_or(self.config.grid_line_style);
            if time_based {
                renderers::render_time_grid(
                    &RenderContext::new(&painter, chart_rect),
                    &time_marks,
                    &time_points,
                    &time_coords,
                    self.config
                        .vertical_grid_color
                        .unwrap_or(self.config.grid_color),
                    style,
                    horizontal_grid
                        .then_some((self.config.grid_color, self.config.grid_line_style)),
                );
            } else {
                rendering::render_vertical_grid(
                    &painter,
                    chart_rect,
                    &coords,
                    self.config
                        .vertical_grid_color
                        .unwrap_or(self.config.grid_color),
                    style,
                );
            }
        }

        let session_tf = self.resolve_session_timeframe(visible_data);
        let exchange_session = renderers::exchange_session(
            self.instrument(),
            self.config.session_config.as_ref(),
            &timezone,
            session_tf,
        );
        let session_painter = painter.with_clip_rect(price_rect);
        let session_ctx = RenderContext::new(&session_painter, price_rect);
        // Sessions and extended hours are periods of time, absent from other horizontal scales.
        if self.config.show_session_breaks && host_scale.is_none() {
            let provider = self
                .session_provider
                .as_deref()
                .unwrap_or(&exchange_session);
            let background =
                renderers::SessionBackgroundRenderer::from_background(self.config.background_color);
            background.render(
                &session_ctx,
                time_data,
                provider,
                &time_coords,
                time_coords.start_idx,
            );
            renderers::SessionBreakRenderer::new(renderers::SessionBreakRenderConfig {
                line_color: self.config.session_break_color,
                line_width: 1.0,
                style: self.config.session_break_style,
            })
            .render(
                &session_ctx,
                time_data,
                provider,
                &time_coords,
                time_coords.start_idx,
            );
        }
        if time_based
            && host_scale.is_none()
            && session_tf.duration_ms() < crate::model::Timeframe::Day1.duration_ms()
            && self.config.show_extended_hours
        {
            let color = if self.config.extended_hours_color == egui::Color32::PLACEHOLDER {
                ui.active_theme().ui.accent.linear_multiply(0.08)
            } else {
                self.config.extended_hours_color
            };
            renderers::render_extended_hours(
                &session_ctx,
                time_data,
                &exchange_session,
                &time_coords,
                time_coords.start_idx,
                color,
            );
        }

        // Render chart type with clipping to prevent bars from overlapping axes
        // CRITICAL: Use chart_rect.width() for consistency with drawing coordinate system
        let chart_rect_width = chart_rect.width();
        let idx_to_coord = |idx: usize, min_x: f32| -> f32 {
            self.state
                .time_scale()
                .idx_to_coord(idx, min_x, chart_rect_width)
        };

        // Create clipped painter and contexts to prevent bars from rendering on axes
        let clipped_painter = painter.with_clip_rect(chart_rect);
        let clipped_price_ctx = RenderContext::new(&clipped_painter, price_rect);
        let clipped_volume_ctx = RenderContext::new(&clipped_painter, volume_rect);

        let mut bar_clock = self.bar_clock();
        // TPO days follow the instrument's zone, as the session breaks do, also without its hours.
        if let Some(zone) = self
            .instrument()
            .filter(|instrument| instrument.session.is_none())
            .and_then(|instrument| instrument.time_zone.as_ref())
        {
            bar_clock = bar_clock.with_timezone(zone.clone());
        }
        let render_ctx = rendering::CandleDataContext {
            price_ctx: &clipped_price_ctx,
            volume_ctx: &clipped_volume_ctx,
            price_scale: &price_map,
            colors: &colors,
            visible_data,
            // Full dataset so window-independent transforms (Heikin-Ashi) can be
            // computed over the complete series, not just the visible slice.
            full_data: &self.state.data().bars,
            shows_elements: self.state.shows_elements(),
            revision: self.state.revision(),
            heikin_ashi: &self.heikin_ashi,
            start_idx,
            clock: &bar_clock,
            tick_size: self
                .instrument()
                .and_then(|instrument| instrument.tick_size),
        };

        let render_params = rendering::ChartTypeParams::new(
            rendering::BarDimensions::new(bar_width, self.config.wick_width),
            rendering::VolumeSettings::new(
                self.config.show_volume && !cfg!(feature = "studies"),
                max_volume,
            ),
            &self.chart_type_params,
            rendering::TradingColors::new(self.config.bullish_color, self.config.bearish_color),
            rendering::CoordMapping::new(chart_rect.min.x),
            self.config.price_source,
        );
        // Draw one merged bar, or a column's line points, for each group.
        let merged = super::merge::line_style(self.chart_type)
            .filter(|_| factor > 1)
            .map(|line| {
                let merge = |line| {
                    super::merge::Merged::new(
                        self.state.shown(),
                        &self.merge_groups,
                        start_idx..start_idx + visible_data.len(),
                        line,
                        self.config.price_source,
                    )
                };
                // Volume columns merge as bars beneath a merged line.
                let volume = (line && render_params.show_volume()).then(|| merge(false));
                (merge(line), volume)
            });
        let merged_x = |merged: &super::merge::Merged, index: usize, min_x: f32| {
            // Past the last merged bar, as a renderer measuring its pitch asks for.
            let past = |last: f32| last + (index + 1 - merged.index.len()) as f32 * factor as f32;
            let index = merged
                .index
                .get(index)
                .copied()
                .unwrap_or_else(|| merged.index.last().map_or(0.0, |&last| past(last)));
            self.state
                .time_scale()
                .idx_to_coord_precise(index, min_x, chart_rect_width)
        };
        let merged_params = rendering::ChartTypeParams::new(
            rendering::BarDimensions::new(bar_width * factor as f32, self.config.wick_width),
            rendering::VolumeSettings::new(
                render_params.show_volume() && merged.as_ref().is_some_and(|m| m.1.is_none()),
                max_volume,
            ),
            &self.chart_type_params,
            rendering::TradingColors::new(self.config.bullish_color, self.config.bearish_color),
            rendering::CoordMapping::new(chart_rect.min.x),
            self.config.price_source,
        );

        self.paint_primitives(&painter, &coords, PrimitivePass::Below);
        let studies = if self.chart_type.is_time_independent() {
            &self.element_studies.studies
        } else {
            &self.studies
        };
        let volume_map = volume_rect.is_positive().then(|| {
            crate::scales::PriceMap::new(
                crate::scales::PriceTransform::default(),
                crate::scales::PriceRange::new(
                    0.0,
                    studies
                        .indicators()
                        .iter()
                        .find(|study| {
                            study.is_volume()
                                && study.is_overlay()
                                && study.is_visible()
                                && !matches!(study.scale(), crate::studies::StudyScale::Shared(_))
                        })
                        .map_or(max_volume.max(1.0), |study| {
                            super::volume::range(
                                study.as_ref(),
                                &(start_idx..end_idx),
                                Some(&self.merge_groups),
                            )
                            .1
                        }),
                ),
                volume_rect.top(),
                volume_rect.bottom(),
                false,
            )
            .with_price_precision(Some(
                self.instrument()
                    .and_then(|instrument| instrument.volume_decimals)
                    .unwrap_or(0),
            ))
        });
        self.host_events.volume_map = volume_map;
        // The main pane's visual order: overlays listed before the series are drawn under it.
        let series_at = self
            .pane_layout
            .main
            .iter()
            .position(|id| *id == super::SeriesId::MAIN)
            .unwrap_or(0);
        let (behind, front) = self.pane_layout.main.split_at(series_at);
        let main_visible_range = start_idx..(start_idx + visible_data.len());
        let volume_coords = show_volume.then(|| coords.with_rect(volume_rect));
        // Primitives are hit tested at the pointer, before the chart state is borrowed mutably.
        let pointer = ui
            .input(|input| input.pointer.interact_pos())
            .filter(|pos| chart_rect.contains(*pos));
        let primitive_hit = pointer.and_then(|pos| {
            primitives::primitive_at(
                self.main_primitives(&coords)
                    .filter(|(item, _)| !item.replaces),
                pos,
            )
        });
        let renderer_hit = pointer.and_then(|pos| {
            self.pane_layout.main.iter().rev().find_map(|id| {
                primitives::primitive_at(
                    self.main_primitives(&coords)
                        .filter(|(item, _)| item.replaces && item.series == *id),
                    pos,
                )
            })
        });
        let rendered: Vec<_> = self
            .study_ids
            .iter()
            .copied()
            .filter(|id| self.has_renderer(*id))
            .collect();
        let hit_order: Vec<_> = self
            .pane_layout
            .main
            .iter()
            .copied()
            .filter(|id| {
                !rendered.contains(id) && (*id != super::SeriesId::MAIN || self.main_series_visible)
            })
            .collect();
        let last_bar = self.state.shown().len().saturating_sub(1);
        let secondary_hits = [pointer, response.interact_pointer_pos()].map(|pos| {
            pos.map(|pos| {
                (
                    pos,
                    hit_order.iter().rev().find_map(|id| {
                        let item = self.main_pane_series().find(|item| item.id == *id)?;
                        self.hit_price_series(
                            item,
                            pos,
                            price_rect,
                            self.series_map(item, price_rect),
                        )
                        .map(|bar| (super::ChartElementId::Series(item.id), bar))
                    }),
                )
            })
        });
        let element_at = |pos: Pos2| {
            let renderer = renderer_hit
                .filter(|_| Some(pos) == pointer)
                .and_then(|hit| {
                    if let super::PointerTarget::Element(id) = hit.target {
                        Some((id, coords.x_to_idx(pos.x).min(last_bar)))
                    } else {
                        None
                    }
                });
            let hit = hit_test_main_chart(
                pos,
                studies,
                &self.study_ids,
                &hit_order,
                &coords,
                volume_coords.as_ref(),
                self.state.shown(),
                max_volume,
                main_visible_range.clone(),
                self.main_series_style.hit_tolerance,
                Some(&self.merge_groups),
            )
            .filter(|(id, _)| match id {
                // A renderer hit tests its custom series in place of the study's plots.
                crate::widget::ChartElementId::Indicator(study) => !rendered.contains(study),
                _ => self.main_series_visible,
            });
            let secondary = secondary_hits
                .iter()
                .flatten()
                .find_map(|(at, hit)| (*at == pos).then_some(*hit).flatten());
            hit.into_iter()
                .chain(secondary)
                .chain(renderer)
                .max_by_key(|(element, _)| {
                    let id = match element {
                        super::ChartElementId::Indicator(id)
                        | super::ChartElementId::Series(id) => id,
                    };
                    self.pane_layout.main.iter().position(|member| member == id)
                })
        };
        // The element under the pointer goes last when hovered series are drawn on top.
        let on_top = ui
            .input(|input| input.pointer.hover_pos())
            .filter(|pos| self.chart_options.hovered_series_on_top && chart_rect.contains(*pos))
            .and_then(element_at)
            .map(|(id, _)| id);
        let main_on_top = on_top == Some(super::ChartElementId::Series(super::SeriesId::MAIN));
        let paint_overlay = |id: &super::SeriesId| {
            if let Some(item) = self.main_pane_series().find(|item| item.id == *id) {
                self.paint_series(
                    &clipped_painter,
                    price_rect,
                    item,
                    self.series_map(item, price_rect),
                );
                return;
            }
            let Some(study) = self
                .study_ids
                .iter()
                .position(|study| study == id)
                .map(|idx| &studies.indicators()[idx])
                .filter(|study| study.is_visible() && study.is_overlay())
            else {
                return;
            };
            if study.is_volume()
                && !matches!(study.scale(), crate::studies::StudyScale::Shared(_))
                && self.main_series_visible
            {
                let rect = super::volume::overlay_rect(price_rect, &self.config);
                let (min, max) = super::volume::range(
                    study.as_ref(),
                    &(start_idx..end_idx),
                    Some(&self.merge_groups),
                );
                super::volume::paint(
                    &painter,
                    rect,
                    study.as_ref(),
                    &(start_idx..end_idx),
                    &coords,
                    min,
                    max,
                    Some(&self.merge_groups),
                );
            }
            // TPO studies draw the chart type's profiles beside the bars.
            if let Some(config) = study
                .tpo()
                .filter(|_| !self.chart_type.is_time_independent())
            {
                rendering::candles::tpo::render_live_tpo(
                    &clipped_painter,
                    price_rect,
                    &self.state.data().bars,
                    start_idx..start_idx + visible_data.len(),
                    self.state.revision(),
                    &price_map,
                    config,
                    &bar_clock,
                    idx_to_coord,
                    chart_rect.min.x,
                    self.config.bullish_color,
                    self.config.bearish_color,
                );
            }
            if self.has_renderer(*id) {
                let view = self.primitive_view(
                    *id,
                    crate::chart::renderers::study_mapping(study.as_ref(), &coords),
                );
                for item in self.primitives_of(*id).filter(|item| item.replaces) {
                    item.draw(&clipped_painter, &view, PrimitivePass::Series);
                }
            } else {
                renderers::IndicatorRenderer::render(
                    &price_ctx,
                    std::slice::from_ref(study),
                    visible_data,
                    &price_map,
                    &coords,
                );
            }
        };
        let paint_main_series = || {
            if self.main_series_visible {
                if time_based {
                    self.paint_liquidity(&clipped_painter, &self.get_chart_mapping());
                }
                if let Some((flow, config)) = footprint.filter(|_| time_based) {
                    if show_volume && !cfg!(feature = "studies") {
                        // Volume merges as bars, including beneath a merged line.
                        match merged
                            .as_ref()
                            .map(|(bars, volume)| volume.as_ref().unwrap_or(bars))
                        {
                            Some(volume) => rendering::candles::volume::render_volume_bars(
                                &clipped_volume_ctx,
                                &volume.bars[volume.start..],
                                volume.start,
                                max_volume,
                                &colors,
                                chart_rect.min.x,
                                |index, min_x| merged_x(volume, index, min_x),
                            ),
                            None => rendering::candles::volume::render_volume_bars(
                                &clipped_volume_ctx,
                                visible_data,
                                start_idx,
                                max_volume,
                                &colors,
                                chart_rect.min.x,
                                idx_to_coord,
                            ),
                        }
                    }
                    crate::chart::overlays::render_footprint(
                        &clipped_painter,
                        &self.get_chart_mapping(),
                        self.bars(super::SeriesId::MAIN),
                        flow,
                        config,
                    );
                } else {
                    // Share the renderer instantiation between ordinary and merged bars.
                    let (mut data, params) = match &merged {
                        Some((merged, _)) => (
                            rendering::CandleDataContext {
                                visible_data: &merged.bars[merged.start..],
                                full_data: &merged.bars,
                                start_idx: merged.start,
                                ..render_ctx
                            },
                            &merged_params,
                        ),
                        None => (
                            rendering::CandleDataContext { ..render_ctx },
                            &render_params,
                        ),
                    };
                    // Bars from the first execution on are drawn from executions, those before as
                    // candles.
                    let executions = self
                        .feed_link
                        .as_ref()
                        .and_then(|link| link.footprint.as_ref())
                        .filter(|_| time_based && self.replay.is_none())
                        .map(|flow| {
                            let first = flow.complete_from();
                            let candles = first.map_or(visible_data.len(), |first| {
                                visible_data.partition_point(|bar| bar.time < first)
                            });
                            (flow, candles)
                        });
                    if let Some((_, candles)) = executions {
                        data.visible_data = match &merged {
                            // Merged candles end before the group of the first execution.
                            Some((merged, _)) if candles < visible_data.len() => {
                                let end = merged.count_before(start_idx + candles, factor);
                                &merged.bars[merged.start..end.max(merged.start)]
                            }
                            Some(_) => data.visible_data,
                            None => &visible_data[..candles],
                        };
                    }
                    rendering::render_chart_type(self.chart_type, &data, params, |index, min_x| {
                        match &merged {
                            Some((merged, _)) => merged_x(merged, index, min_x),
                            None => idx_to_coord(index, min_x),
                        }
                    });
                    if let Some((flow, candles)) = executions {
                        if params.show_volume() {
                            rendering::candles::volume::render_volume_bars(
                                &clipped_volume_ctx,
                                &visible_data[candles..],
                                start_idx + candles,
                                max_volume,
                                &colors,
                                chart_rect.min.x,
                                idx_to_coord,
                            );
                        }
                        let config = FootprintConfig {
                            text_color: colors.text,
                            neutral_color: colors.text,
                            ask_color: colors.bullish,
                            bid_color: colors.bearish,
                            ..Default::default()
                        };
                        let mapping = self.get_chart_mapping();
                        let bars = self.bars(super::SeriesId::MAIN);
                        if self.chart_type == ChartType::VolumeFootprint {
                            crate::chart::overlays::render_footprint(
                                &clipped_painter,
                                &mapping,
                                bars,
                                flow,
                                &config,
                            );
                        }
                        #[cfg(feature = "studies")]
                        if self.chart_type == ChartType::SessionVolumeProfile
                            && let Some(ranges) = session_ranges(
                                bars,
                                start_idx + candles..start_idx + visible_data.len(),
                                &bar_clock,
                            )
                        {
                            let profiles = crate::studies::trade_profiles(flow, &ranges);
                            crate::chart::overlays::render_session_volume_profiles(
                                &clipped_painter,
                                &mapping,
                                bars,
                                &profiles,
                                flow.price_step(),
                                flow.tick_size(),
                            );
                        }
                    }
                    if let Some((_, Some(volume))) = &merged {
                        rendering::candles::volume::render_volume_bars(
                            &clipped_volume_ctx,
                            &volume.bars[volume.start..],
                            volume.start,
                            max_volume,
                            &colors,
                            chart_rect.min.x,
                            |index, min_x| merged_x(volume, index, min_x),
                        );
                    }
                }
                self.paint_last_price_pulse(ui, &clipped_painter, price_rect, &coords);
            }
        };
        let paint_unhovered = |id: &super::SeriesId| {
            let element = if self.price_series.iter().any(|item| item.id == *id) {
                super::ChartElementId::Series(*id)
            } else {
                super::ChartElementId::Indicator(*id)
            };
            if on_top != Some(element) {
                paint_overlay(id);
            }
        };
        behind.iter().for_each(paint_unhovered);
        if !main_on_top {
            paint_main_series();
        }

        self.paint_marks(&clipped_price_ctx, &coords, MarkerZOrder::Normal);

        if let Some(bubbles) = self
            .feed_link
            .as_ref()
            .and_then(|link| link.bubbles.as_ref())
            .filter(|_| time_based && self.main_series_visible && self.replay.is_none())
        {
            let x = |i| idx_to_coord(start_idx + i, chart_rect.min.x);
            bubbles.render(&clipped_painter, visible_data, coords.bar_spacing, x, |p| {
                price_map.price_to_y(p)
            });
        }

        front.iter().for_each(paint_unhovered);
        match on_top {
            Some(super::ChartElementId::Indicator(id)) => paint_overlay(&id),
            _ if main_on_top => paint_main_series(),
            Some(super::ChartElementId::Series(id)) => {
                if let Some(item) = self.main_pane_series().find(|item| item.id == id) {
                    self.paint_series(
                        &clipped_painter,
                        price_rect,
                        item,
                        self.series_map(item, price_rect),
                    );
                }
            }
            _ => {}
        }
        self.paint_signals(ui, &coords);
        let target_lines = self
            .show_price_targets
            .then(|| painter.add(egui::Shape::Noop));

        self.paint_primitives(&painter, &coords, PrimitivePass::Series);

        // Resolve a click on the main chart area into a selection. Overlay
        // indicators take priority over the series beneath them; an empty-area
        // click clears the selection. Clicks in a study's pane are resolved
        // where the panes are rendered, outside this rect.
        if response.clicked()
            && let Some(click_pos) = response.interact_pointer_pos()
            && self.mark_at(click_pos).is_none()
        {
            match element_at(click_pos) {
                Some((id, bar_idx)) => self.selection.select(id, Some(bar_idx)),
                None => self.selection.deselect(),
            }
        }

        // Resolve a right-click into a context-menu target. Mirrors the
        // left-click priority (drawing > overlay indicator > series), and
        // right-clicking an object also selects it so the menu and the
        // selection handles agree. The captured target is
        // drained by the host via `take_right_click`, or consumed by
        // `ChartMenus`.
        if response.secondary_clicked()
            && let Some(click_pos) = response.interact_pointer_pos()
            && chart_rect.contains(click_pos)
        {
            let price = coords.y_to_price(click_pos.y);

            // Drawings sit on top of the series, so they take priority.
            #[cfg(feature = "drawings")]
            let drawing_hit = self.drawings.hit_test(click_pos);
            #[cfg(not(feature = "drawings"))]
            let drawing_hit: Option<usize> = None;

            self.right_click = Some(if let Some(drawing_id) = drawing_hit {
                // Right-clicking a drawing selects it and
                // clears any series/indicator selection so handles don't show
                // on two objects at once.
                self.selection.deselect();
                #[cfg(feature = "drawings")]
                {
                    if self.drawings.is_selected(drawing_id) {
                        let ids: Vec<_> = std::iter::once(drawing_id)
                            .chain(self.drawings.selected_ids())
                            .collect();
                        self.drawings.select_drawings(ids);
                    } else {
                        self.drawings.select(drawing_id);
                    }
                }
                RightClickTarget::Drawing {
                    drawing_id,
                    pos: click_pos,
                }
            } else {
                match element_at(click_pos) {
                    Some((id, bar_idx)) => {
                        self.selection.select(id, Some(bar_idx));
                        RightClickTarget::Element {
                            id,
                            bar_idx,
                            pos: click_pos,
                            price,
                        }
                    }
                    None => {
                        self.selection.deselect();
                        RightClickTarget::Background {
                            pos: click_pos,
                            price,
                        }
                    }
                }
            });
        }

        // What the pointer is over, for host pointer events, in the right-click priority.
        if let Some(pos) = pointer {
            #[cfg(feature = "drawings")]
            let drawing = self.drawings.hit_test(pos);
            #[cfg(not(feature = "drawings"))]
            let drawing: Option<usize> = None;
            if let Some(hit) = primitive_hit
                .or(renderer_hit.filter(|hit| {
                    element_at(pos).map(|(id, _)| super::PointerTarget::Element(id))
                        == Some(hit.target)
                }))
                .filter(|_| {
                    drawing.is_none()
                        && response.hovered()
                        && !response.dragged()
                        && !self.zoom_mode_active
                        && !eraser_mode
                        && !drawing_tool_active
                })
            {
                ui.ctx().set_cursor_icon(hit.cursor);
            }
            self.host_events.target = drawing
                .map(super::PointerTarget::Drawing)
                .or(primitive_hit.map(|hit| hit.target))
                .or_else(|| element_at(pos).map(|(id, _)| super::PointerTarget::Element(id)));
        }

        // Draw selection handles for whichever element is currently selected on
        // the main chart (overlay indicator line or series data points).
        self.render_main_chart_selection(&painter, &coords, visible_data, main_visible_range);

        self.paint_marks(&clipped_price_ctx, &coords, MarkerZOrder::AboveSeries);

        if self.config.show_right_axis || self.config.show_left_axis {
            let axis_painter =
                painter.with_clip_rect(painter.clip_rect().intersect(price_axis_rect));
            for (item, view) in self.main_primitives(&coords) {
                item.get()
                    .draw_price_axis(&axis_painter, price_axis_rect, &view);
            }
        }

        // The axis labels go at these depths, painted once the crosshair's tags are placed
        let price_labels = painter.add(egui::Shape::Noop);

        let axis_edge = if self.config.show_right_axis {
            rect.max.x
        } else {
            price_rect.left()
        };
        let show_price_axis = self.config.show_right_axis || self.config.show_left_axis;
        let study_axis = crate::config::PriceAxisOptions {
            show_study_values: self.config.show_indicator_last_val
                && self.config.price_axis.show_study_values,
            ..self.config.price_axis
        };
        let currency = self
            .instrument()
            .and_then(|instrument| instrument.currency.as_deref())
            .filter(|code| self.config.show_currency && !code.is_empty());
        let unit = self
            .instrument()
            .and_then(|instrument| instrument.unit.as_deref())
            .filter(|unit| self.config.show_unit && !unit.is_empty());
        let has_currency = currency.is_some() || unit.is_some();
        let mut tags = Vec::new();
        if has_currency {
            tags.push(Some(Rect::from_min_size(
                price_rect.min,
                egui::vec2(price_rect.width(), 24.0),
            )));
        }
        let mut pending_values = Vec::new();
        for (_, line) in self.price_lines.iter().filter(|_| self.main_series_visible) {
            rendering::render_price_line(
                &painter,
                price_rect,
                &price_map,
                line,
                price_rect.left(),
                false,
                axis_edge,
            );
            if line.axis_label {
                pending_values.push((line.price, rendering::axes::axis_label_colors(line), false));
            }
        }
        #[cfg(feature = "ui")]
        let trading_outlined = std::mem::take(&mut self.trading_outlined);
        #[cfg(feature = "ui")]
        pending_values.extend(
            std::mem::take(&mut self.trading_tags)
                .into_iter()
                .enumerate()
                .map(|(index, (value, fill))| {
                    let outlined = trading_outlined.contains(&index);
                    let colors = if outlined {
                        (fill, ui.active_theme().chart.bg)
                    } else {
                        (rendering::axes::tag_text_color(fill), fill)
                    };
                    (value, colors, outlined)
                }),
        );

        // The previous session's close and the last close, for the previous close line and the
        // legend's day change.
        let session_closes = (self.config.show_symbol_prev_close
            || self.config.show_prev_close_line
            || self.config.show_last_day_change)
            .then(|| {
                let bars = &self.state.data().bars;
                let last = crate::model::last_with_value(bars)?;
                let index = bars
                    .partition_point(|bar| {
                        exchange_session.trading_date(bar.time)
                            < exchange_session.trading_date(last.time)
                    })
                    .checked_sub(1)?;
                crate::model::last_with_value(&bars[..=index])
                    .map(|previous| (previous.close, last.close))
            })
            .flatten();
        if self.main_series_visible
            && (self.config.show_symbol_prev_close || self.config.show_prev_close_line)
            && let Some((previous_close, _)) = session_closes
        {
            let line = PriceLine::new(previous_close, ui.active_theme().ui.text_secondary)
                .with_style(crate::model::LineStyle::Dotted)
                .with_axis_label(self.config.show_symbol_prev_close)
                .with_width(if self.config.show_prev_close_line {
                    1.0
                } else {
                    0.0
                });
            rendering::render_price_line(
                &painter,
                price_rect,
                &price_map,
                &line,
                price_rect.left(),
                false,
                axis_edge,
            );
            if self.config.show_symbol_prev_close {
                pending_values.push((
                    previous_close,
                    rendering::axes::axis_label_colors(&line),
                    false,
                ));
            }
        }

        let style = &self.main_series_style;
        if let Some(line) = super::price_series::base_line(&price_map)
            .filter(|_| self.main_series_visible && style.base_line_visible)
        {
            rendering::render_price_line(
                &painter,
                price_rect,
                &price_map,
                &line,
                price_rect.left(),
                false,
                axis_edge,
            );
        }

        // Last price line & label, from the newest bar even when it is scrolled out of view, or
        // from the newest bar in view
        let last = match style.price_line_source {
            crate::model::PriceLineSource::LastBar => {
                crate::model::last_with_value(&self.state.data().bars)
            }
            crate::model::PriceLineSource::LastVisible => {
                crate::model::last_with_value(visible_data)
            }
        };
        let last_price_tag = if let Some(last) = last.filter(|_| self.main_series_visible) {
            let color = style
                .price_line_color
                .or_else(|| last.colors.as_ref().and_then(|own| own.color))
                .unwrap_or(if last.close >= last.open {
                    self.config.bullish_color
                } else {
                    self.config.bearish_color
                });
            let line = PriceLine::new(last.close, color)
                .with_width(if self.config.show_price_line && style.price_line_visible {
                    style.price_line_width
                } else {
                    0.0
                })
                .with_style(style.price_line_style)
                .with_axis_label(
                    show_price_axis && self.config.show_symbol_last_val && style.last_value_visible,
                );
            // Only a countdown drawn on the axis needs the once-a-second repaint.
            let countdown = self.current_bar_countdown().filter(|_| {
                line.axis_label
                    && price_map
                        .range()
                        .contains(price_map.transform().to_scale(last.close))
            });
            if countdown.is_some()
                && (self.server_time_offset.is_some() || self.feed_link.is_some())
            {
                ui.ctx()
                    .request_repaint_after(std::time::Duration::from_secs(1));
            }
            let quote_tags = rendering::axes::render_quote_tags_with_countdown(
                &painter,
                price_rect,
                &price_map,
                &line,
                self.session_quote
                    .as_ref()
                    .filter(|_| show_price_axis && self.config.show_extended_session_price)
                    .map(|quote| (quote, self.config.extended_session_price_color)),
                (show_volume && show_price_axis && !cfg!(feature = "studies")).then_some((
                    last.volume,
                    // The tag takes the colour of the bar's direction in the applied theme, as
                    // its volume bar does.
                    if last.is_bullish() {
                        ui.active_theme().chart.candle_up
                    } else {
                        ui.active_theme().chart.candle_down
                    },
                )),
                axis_edge,
                countdown.as_deref(),
                self.instrument()
                    .and_then(|instrument| instrument.volume_decimals),
                if has_currency { 24.0 } else { 0.0 },
                price_formatter.as_deref(),
                self.volume_formatter.as_deref(),
                self.config.price_axis.allow_label_overlap,
                if self.config.price_axis.show_symbol_name {
                    &self.symbol
                } else {
                    ""
                },
            );
            tags.extend([quote_tags[1], quote_tags[2]]);
            quote_tags[0]
        } else {
            None
        };

        tags.push(last_price_tag);
        if show_price_axis {
            // Labels that sit closer than a tag is high stack in the order of their prices.
            let mut shown: Vec<_> = pending_values
                .iter()
                .filter(|(value, ..)| {
                    price_map
                        .range()
                        .contains(price_map.transform().to_scale(*value))
                })
                .map(|(value, colors, outlined)| {
                    (price_map.price_to_y(*value), *value, *colors, *outlined)
                })
                .collect();
            shown.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut ys: Vec<_> = shown.iter().map(|shown| shown.0).collect();
            if !self.config.price_axis.allow_label_overlap {
                ys = rendering::axes::spread_axis_ys(&painter, &ys);
            }
            for ((_, value, colors, outlined), y) in shown.into_iter().zip(ys) {
                let label = price_map.transform().label(
                    price_map.transform().to_scale(value),
                    rendering::axes::tag_decimals(&price_map, price_map.price_to_y(value)),
                    price_formatter.as_deref(),
                );
                let tag = rendering::axes::render_axis_value(
                    &painter,
                    price_rect,
                    axis_edge,
                    y,
                    label,
                    colors,
                    &tags,
                    self.config.price_axis.allow_label_overlap,
                );
                if outlined && let Some(rect) = tag {
                    painter.rect_stroke(
                        rect,
                        2.0,
                        egui::Stroke::new(1.0, colors.0),
                        egui::StrokeKind::Inside,
                    );
                }
                tags.push(tag);
            }
        }
        if show_price_axis {
            for (value, colors, map, label) in
                self.secondary_axis_values(self.main_series_scale, price_rect)
            {
                tags.push(rendering::axes::render_axis_value(
                    &painter,
                    price_rect,
                    axis_edge,
                    map.price_to_y(value),
                    label,
                    colors,
                    &tags,
                    self.config.price_axis.allow_label_overlap,
                ));
            }
        }
        if show_price_axis {
            let labels: Vec<_> = self
                .main_primitives(&coords)
                .flat_map(|(item, view)| {
                    let map = view.mapping.price;
                    let labels = item.get().price_axis_labels(&view);
                    labels.into_iter().map(move |label| (label, map))
                })
                .collect();
            for (label, map) in labels {
                if map.range().contains(map.transform().to_scale(label.price)) {
                    tags.push(rendering::axes::render_axis_value(
                        &painter,
                        price_rect,
                        axis_edge,
                        map.price_to_y(label.price),
                        label.text,
                        (rendering::axes::tag_text_color(label.color), label.color),
                        &tags,
                        self.config.price_axis.allow_label_overlap,
                    ));
                }
            }
        }
        let mut independent_axis_width =
            self.paint_series_axes(ui, &painter, rect, price_rect, &colors);
        if show_price_axis {
            let axes = studies.indicators().iter().filter(|study| {
                study.is_visible()
                    && study.is_overlay()
                    && !study.is_volume()
                    && study.scale() == crate::studies::StudyScale::Independent
            });
            let mut edge = price_rect.left()
                - if self.config.show_left_axis {
                    self.config.left_axis_width.max(self.measured_axis_width)
                } else {
                    0.0
                };
            let padding = self
                .config
                .padding
                .max(0.0)
                .min(rect.width().min(rect.height()).max(0.0) / 2.0);
            let series_left = self
                .series_axis_keys()
                .iter()
                .filter(|key| **key != crate::model::SeriesScale::Right)
                .count();
            let slot_width = (edge - rect.left() - padding).max(0.0)
                / (axes.clone().count() + series_left).max(1) as f32;
            edge -= series_left as f32 * slot_width;
            for study in axes {
                let mapping = crate::chart::renderers::study_mapping(study.as_ref(), &coords);
                let axis = Rect::from_min_max(
                    Pos2::new(edge - slot_width, price_rect.top()),
                    Pos2::new(edge, price_rect.bottom()),
                );
                edge -= slot_width;
                independent_axis_width =
                    independent_axis_width.max(crate::chart::renderers::render_study_axis(
                        &painter,
                        &[study.as_ref()],
                        &mapping,
                        axis,
                        &study_axis,
                        None,
                    ));
            }
        }
        if show_price_axis && study_axis.show_study_values {
            for study in studies
                .indicators()
                .iter()
                .filter(|study| study.is_overlay())
            {
                if study.is_volume()
                    && !matches!(study.scale(), crate::studies::StudyScale::Shared(_))
                {
                    if self.main_series_visible {
                        let rect = super::volume::overlay_rect(price_rect, &self.config);
                        let mapping = super::volume::mapping(
                            &coords,
                            rect,
                            study.as_ref(),
                            &(start_idx..end_idx),
                            Some(&self.merge_groups),
                        );
                        super::volume::tags(
                            &painter,
                            price_rect,
                            &mapping.price,
                            study.as_ref(),
                            axis_edge,
                            &mut tags,
                            &study_axis,
                        );
                    }
                    continue;
                }
                if study.scale() != crate::studies::StudyScale::Main
                    && study.scale() != crate::studies::StudyScale::Shared(self.main_series_scale)
                {
                    continue;
                }
                rendering::axes::render_study_tags(
                    &painter,
                    price_rect,
                    &price_map,
                    study.as_ref(),
                    axis_edge,
                    &mut tags,
                    price_formatter.as_deref(),
                    &study_axis,
                );
            }
        }

        if show_price_axis && self.main_series_visible {
            let mut values = Vec::new();
            if self.config.show_high_low {
                let high = visible_data
                    .iter()
                    .map(|bar| bar.high)
                    .fold(f64::NEG_INFINITY, f64::max);
                let low = visible_data
                    .iter()
                    .map(|bar| bar.low)
                    .fold(f64::INFINITY, f64::min);
                values.extend([
                    (high, "High", self.config.bullish_color),
                    (low, "Low", self.config.bearish_color),
                ]);
            }
            let trading_bid_ask = {
                #[cfg(feature = "ui")]
                {
                    std::mem::take(&mut self.trading_bid_ask)
                }
                #[cfg(not(feature = "ui"))]
                {
                    false
                }
            };
            if (self.config.show_bid_ask || trading_bid_ask)
                && let Some(quote) = self.quote.as_ref().filter(|quote| {
                    quote.bid.is_finite()
                        && quote.ask.is_finite()
                        && quote.bid <= quote.ask
                        && quote.spread().is_finite()
                })
            {
                values.extend([
                    (quote.bid, "Bid", ui.active_theme().ui.accent),
                    (quote.ask, "Ask", ui.active_theme().chart.candle_down),
                ]);
            }
            for (value, name, color) in values {
                let line = PriceLine::new(value, color)
                    .with_width(
                        if (name == "Bid" || name == "Ask") && !self.config.show_bid_ask {
                            0.0
                        } else {
                            1.0
                        },
                    )
                    .with_style(crate::model::LineStyle::Dotted)
                    .with_axis_label(false);
                rendering::render_price_line(
                    &painter,
                    price_rect,
                    &price_map,
                    &line,
                    price_rect.left(),
                    false,
                    axis_edge,
                );
                if price_map
                    .range()
                    .contains(price_map.transform().to_scale(value))
                {
                    let price = price_map.transform().label(
                        price_map.transform().to_scale(value),
                        rendering::axes::tag_decimals(&price_map, price_map.price_to_y(value)),
                        price_formatter.as_deref(),
                    );
                    let quote = name == "Bid" || name == "Ask";
                    let label = if quote {
                        price
                    } else {
                        format!("{name} {price}")
                    };
                    let text_color = rendering::axes::tag_text_color(color);
                    let tag = rendering::axes::render_axis_value(
                        &painter,
                        price_rect,
                        axis_edge,
                        price_map.price_to_y(value),
                        label,
                        (text_color, color),
                        &tags,
                        self.config.price_axis.allow_label_overlap,
                    );
                    if quote && let Some(rect) = tag {
                        let badge = painter.layout_no_wrap(
                            l10n::text(ui.ctx(), name),
                            rendering::axes::axis_font(ui.ctx()),
                            text_color,
                        );
                        let box_rect = Rect::from_min_size(
                            egui::pos2(
                                if axis_edge <= price_rect.left() {
                                    rect.right() + 1.0
                                } else {
                                    rect.left() - badge.size().x - 8.0
                                },
                                rect.top(),
                            ),
                            egui::vec2(badge.size().x + 7.0, rect.height()),
                        );
                        painter.rect_filled(box_rect, 2.0, color);
                        painter.galley(box_rect.center() - badge.size() / 2.0, badge, text_color);
                    }
                    tags.push(tag);
                }
            }
        }

        if self.config.show_time_labels {
            let axis = self.time_axis_rect;
            let axis_painter = painter.with_clip_rect(painter.clip_rect().intersect(axis));
            for (item, view) in self.time_axis_primitives(&coords) {
                item.get().draw_time_axis(&axis_painter, axis, &view);
            }
        }
        let time_labels = painter.add(egui::Shape::Noop);
        // Kept apart from the price tags, which size the price axis.
        let mut time_tags = Vec::new();
        if self.config.show_time_labels {
            let labels: Vec<_> = self
                .time_axis_primitives(&coords)
                .flat_map(|(item, view)| {
                    let labels = item.get().time_axis_labels(&view);
                    labels
                        .into_iter()
                        .filter_map(|label| Some((view.time_to_x(label.time)?, label)))
                        .collect::<Vec<_>>()
                })
                .collect();
            for (x, label) in labels {
                if chart_rect.x_range().contains(x) {
                    time_tags.push(Some(rendering::axes::paint_crosshair_tag(
                        &painter,
                        Pos2::new(x, time_axis_top + 1.5),
                        egui::Align2::CENTER_TOP,
                        chart_rect.left()..=chart_rect.right(),
                        label.text,
                        (rendering::axes::tag_text_color(label.color), label.color),
                        25.0,
                        None,
                    )));
                }
            }
        }

        // Handle drawing tools
        #[cfg(feature = "drawings")]
        {
            // Clone/extract values before mutable borrow of self
            let timescale = self.state.time_scale().clone();

            let drawing_history = self.history.start_drawing_action(&mut self.drawings);
            // Temporarily take cursor_modes and the drawings to avoid borrow conflict
            let mut cursor_modes = std::mem::take(&mut self.cursor_modes);
            let mut drawings = std::mem::take(&mut self.drawings);

            self.handle_drawings(
                ui,
                &mut drawings,
                &mut cursor_modes,
                &response,
                price_rect,
                &price_map,
                &painter,
                &timescale,
            );

            // Render eraser highlight if in eraser mode
            self.render_eraser_highlight(&painter, &drawings, &cursor_modes);

            // Put them back
            self.drawings = drawings;
            self.cursor_modes = cursor_modes;
            self.history
                .finish_drawing_action(&mut self.drawings, drawing_history);
        }
        self.paint_marks(&clipped_price_ctx, &coords, MarkerZOrder::Top);
        self.paint_primitives(&painter, &coords, PrimitivePass::Above);

        let bar_seconds = self.time_label_seconds();

        // Draw crosshair with options from chart_options. The pointer still hovers the chart
        // while a context menu is open, so the crosshair, its labels and the tooltip wait until
        // the menu closes.
        #[cfg(feature = "drawings")]
        let eraser_mode = self.drawings.eraser_mode;
        #[cfg(feature = "drawings")]
        let drawing_tool_active = self.drawings.active_tool.is_some();
        let menu_open = std::mem::take(&mut self.menu_open) || response.secondary_clicked();
        let mut crosshair_tags = [None; 2];
        #[cfg(feature = "drawings")]
        {
            self.cursor_modes.update(ui.input(|input| input.stable_dt));
            let trail = Some(self.chart_options.crosshair.style)
                .filter(|style| {
                    matches!(
                        style,
                        crate::config::CrosshairStyle::Demonstration
                            | crate::config::CrosshairStyle::Magic
                    )
                })
                .filter(|_| !eraser_mode);
            self.cursor_modes.follow(
                trail,
                response.hover_pos().filter(|pos| price_rect.contains(*pos)),
                ui.input(|input| input.pointer.primary_down()),
            );
            if trail.is_some() {
                self.cursor_modes.render(ui, price_rect);
            }
        }
        let mut table_rect = price_rect;
        for (id, study) in self
            .study_ids
            .iter()
            .zip(self.rendered_studies().indicators())
            .filter(|(_, study)| study.is_visible() && study.is_overlay())
        {
            if let Some(table) = study.table() {
                table_rect.max.y =
                    crate::studies::table::paint(ui, table_rect, table, ui.scope_id().with(id))
                        .top()
                        - 8.0;
            }
        }
        let hover_rect = shared_plot.map_or_else(
            || Rect::from_min_max(price_rect.min, Pos2::new(price_rect.right(), time_axis_top)),
            |plot| plot.rect,
        );
        let hover_pos = self
            .axis_hover
            .filter(|(_, tag)| {
                ui.input(|input| {
                    input
                        .pointer
                        .hover_pos()
                        .is_some_and(|pos| tag.contains(pos))
                })
            })
            .map(|(pos, _)| pos)
            .or_else(|| {
                ui.input(|input| input.pointer.hover_pos())
                    .filter(|pos| hover_rect.contains(*pos) && ui.rect_contains_pointer(hover_rect))
            });
        let mut study_action = None;
        let mut series_action = None;
        let mut legend_bottom = price_rect.top() + 8.0;
        {
            let visible_data = self.state.visible_data();
            let selected = hover_pos
                .and_then(|pos| coords.local_idx_at_x(pos.x, visible_data.len()))
                .map(|index| start_idx + index)
                .unwrap_or_else(|| {
                    // The last bar with a value, before any whitespace bars at the end.
                    let shown = self.state.shown();
                    shown
                        .iter()
                        .rposition(|bar| !bar.is_whitespace())
                        .unwrap_or(shown.len().saturating_sub(1))
                });
            if let Some(bar) = self.state.shown().get(selected) {
                let timeframe = self.timeframe.map(|tf| tf.as_str()).unwrap_or_default();
                #[cfg(feature = "ui")]
                let order_quantity = self.order_quantity.take();
                let mut legend = super::Legend::new(&self.symbol, &timeframe, bar);
                let statuses: Vec<_> = self
                    .legend_statuses
                    .iter()
                    .map(|(text, color, background)| super::LegendStatus {
                        text,
                        color: *color,
                        background: *background,
                    })
                    .collect();
                legend.formatters = self.value_formatters();
                legend.instrument = self.instrument();
                legend.badge = self
                    .legend_badge
                    .clone()
                    .filter(|_| self.config.show_legend_badge);
                legend.statuses = &statuses;
                legend.quote = self
                    .quote
                    .as_ref()
                    .filter(|_| self.main_series_visible && self.config.show_quote_boxes);
                legend.show_title = self.main_series_present && self.config.show_symbol_labels;
                legend.series_visible = self.main_series_visible;
                legend.title_as_description = self.config.title_as_description;
                // A whitespace bar has no values to show.
                let values = self.main_series_visible && !bar.is_whitespace();
                legend.show_ohlc = values && self.config.show_ohlc_info;
                legend.show_change = values && self.config.show_ohlc_change;
                legend.show_change_percent = values && self.config.show_ohlc_change_percent;
                legend.show_indicator_titles = self.config.show_indicator_labels;
                legend.show_indicator_inputs = self.config.show_indicator_inputs;
                legend.show_indicator_values = self.config.show_indicator_last_val;
                legend.day_change =
                    session_closes.filter(|_| values && self.config.show_last_day_change);
                legend.study_backdrop = self.config.indicator_backdrop_opacity;
                let inputs: Vec<_> = self.study_inputs.iter().map(String::as_str).collect();
                legend.study_inputs = &inputs;
                legend.previous_close =
                    crate::model::last_with_value(&self.state.shown()[..selected])
                        .map(|bar| bar.close);
                legend.bar_index = selected;
                legend.studies = self.rendered_studies().indicators();
                legend.show_volume =
                    values && self.chart_type.supports_volume() && self.config.show_legend_volume;
                #[cfg(feature = "ui")]
                {
                    legend.quantity = order_quantity;
                }
                let (quote_action, study_settings, bottom, quantity, main_action) = ui
                    .push_id(shown.id, |ui| {
                        legend.show_with_colors(
                            ui,
                            Rect::from_min_max(
                                price_rect.min,
                                egui::pos2(
                                    (price_rect.right() - 48.0).min(rect.right() - 100.0),
                                    price_rect.bottom(),
                                ),
                            ),
                            &colors,
                        )
                    })
                    .inner;
                if let Some(action) = quote_action {
                    self.quote_action = Some(action);
                }
                #[cfg(feature = "ui")]
                if quantity.is_some() {
                    self.quantity_clicked = quantity;
                }
                #[cfg(not(feature = "ui"))]
                let _ = quantity;
                study_action = study_settings;
                series_action = main_action;
                legend_bottom = bottom;
            }
        }
        let legend_bottom = self.paint_secondary_legend(
            ui,
            &painter,
            price_rect,
            legend_bottom + 4.0,
            self.main_pane_series(),
        );
        if let Some(lines) = target_lines {
            self.paint_targets(ui, &coords, legend_bottom + 4.0, lines);
        }

        if let Some(hover_pos) = hover_pos {
            // Cache the hover bar index for multi-chart sync
            self.last_hover_bar_idx = coords
                .local_idx_at_x(hover_pos.x, visible_data.len())
                .map(|local| (start_idx + local) as f64);

            if !menu_open {
                let mut tooltip_options = self.chart_options.tooltip.clone();
                tooltip_options.enabled &= self.main_series_visible;
                tooltip_options.show_volume &= self.chart_type.supports_volume();
                let held = ui.input(|input| {
                    input.pointer.primary_down()
                        && (!input.any_touches()
                            || input
                                .pointer
                                .press_start_time()
                                .is_some_and(|start| input.time - start >= 0.5))
                });
                let held_values = tooltip_options.enabled
                    && tooltip_options.on_long_press
                    && held
                    && !eraser_mode
                    && !drawing_tool_active;
                let mut crosshair_options = self.chart_options.crosshair;
                // Arrow keeps the axis labels, which a hidden crosshair never shows.
                if (held_values || eraser_mode)
                    && crosshair_options.style != crate::config::CrosshairStyle::Hidden
                {
                    crosshair_options.style = crate::config::CrosshairStyle::Arrow;
                }
                let index = coords
                    .local_idx_at_x(hover_pos.x, visible_data.len())
                    .map(|local| start_idx + local);
                let snapped = if price_rect.contains(hover_pos) {
                    index.map_or(hover_pos, |index| {
                        self.snap_crosshair(hover_pos, &price_map, index)
                    })
                } else {
                    hover_pos
                };
                self.last_crosshair = index.map(|index| {
                    let main = price_rect.contains(hover_pos) && !volume_rect.contains(hover_pos);
                    (
                        self.state.shown()[index].time,
                        main.then(|| price_map.y_to_price(snapped.y)),
                    )
                });
                crosshair_options.mode = crate::config::CrosshairMode::Normal;
                let mut main_options = crosshair_options;
                if volume_rect.contains(hover_pos) {
                    main_options.horz_line_visible = false;
                    if main_options.style == crate::config::CrosshairStyle::Dot {
                        main_options.style = crate::config::CrosshairStyle::Arrow;
                    }
                }
                crosshair_tags = rendering::render_crosshair_with_options(
                    &price_ctx,
                    snapped,
                    visible_data,
                    &price_map,
                    &coords,
                    &main_options,
                    time_axis_top,
                    axis_edge,
                    last_price_tag,
                    scale,
                    bar_seconds,
                    price_formatter.as_deref(),
                );
                if let Some(index) = index
                    .filter(|_| crosshair_options.style != crate::config::CrosshairStyle::Hidden)
                    .filter(|_| self.main_series_visible)
                {
                    let bar = &self.state.shown()[index];
                    let value = self
                        .config
                        .price_source
                        .compute(bar.open, bar.high, bar.low, bar.close);
                    if let Some(color) = rendering::candles::line_color_at(
                        self.chart_type,
                        &self.chart_type_params.styles,
                        self.chart_type_params.baseline,
                        &price_map,
                        price_rect,
                        value,
                        bar,
                    ) {
                        rendering::candles::render_crosshair_marker(
                            &painter.with_clip_rect(price_rect),
                            Pos2::new(coords.idx_to_x(index), price_map.price_to_y(value)),
                            self.chart_type_params.styles.crosshair_marker_radius,
                            color,
                            self.config.background_color,
                        );
                    }
                }

                if volume_rect.contains(hover_pos)
                    && let Some(map) = volume_map
                {
                    let mut pos = hover_pos;
                    if let Some(index) = index {
                        pos.x = coords.idx_to_x(index);
                        if self.chart_options.crosshair.mode.snaps() {
                            pos.y = map.price_to_y(self.state.shown()[index].volume);
                        }
                    }
                    let mut options = crosshair_options.resolved(ui.ctx());
                    options.vert_line_visible = false;
                    let context = RenderContext::new(&painter, volume_rect);
                    renderers::render_crosshair_lines(
                        &context,
                        pos,
                        &options,
                        volume_rect.bottom(),
                    );
                    crosshair_tags[0] = renderers::render_crosshair_price_tag(
                        &context,
                        pos,
                        &map,
                        &options,
                        axis_edge,
                        None,
                        Some(
                            crate::scales::StudyFormat::Volume
                                .format(map.y_to_price(pos.y), map.price_precision()),
                        ),
                    );
                }

                if tooltip_options.on_long_press
                    && ui.input(|input| input.any_touches() && input.pointer.primary_down())
                {
                    ui.ctx().request_repaint();
                }
                if tooltip_options.enabled
                    && (!tooltip_options.on_long_press || held)
                    && !eraser_mode
                    && !drawing_tool_active
                    && let Some(local_idx) = coords.local_idx_at_x(hover_pos.x, visible_data.len())
                    && !visible_data[local_idx].is_whitespace()
                {
                    let candle = &visible_data[local_idx];
                    if tooltip_options.on_long_press {
                        let x = coords.idx_to_x(start_idx + local_idx);
                        rendering::draw_dashed_line(
                            &clipped_painter,
                            Pos2::new(x, price_rect.top()),
                            Pos2::new(x, price_rect.bottom()),
                            egui::Stroke::new(
                                1.0,
                                self.chart_options
                                    .crosshair
                                    .resolved(ui.ctx())
                                    .vert_line_color,
                            ),
                            5.0,
                            5.0,
                        );
                        for (index, bar) in visible_data
                            .iter()
                            .enumerate()
                            .step_by(25)
                            .filter(|(_, bar)| !bar.is_whitespace())
                        {
                            let pos = Pos2::new(
                                coords.idx_to_x(start_idx + index),
                                price_map.price_to_y(bar.close),
                            );
                            clipped_painter.circle(
                                pos,
                                3.5,
                                ui.active_theme().chart.bg,
                                egui::Stroke::new(1.5, ui.active_theme().ui.accent),
                            );
                        }
                    }
                    // A symbol without metadata still heads the readout.
                    let heading = (self.instrument().is_none() && !self.symbol.is_empty())
                        .then(|| crate::model::Instrument::new(self.symbol.as_str()));
                    rendering::render_tooltip_with_options(
                        &price_ctx,
                        hover_pos,
                        candle,
                        crate::model::last_with_value(&self.state.shown()[..start_idx + local_idx])
                            .map(|bar| bar.close),
                        &tooltip_options,
                        &colors,
                        &price_map,
                        &coords,
                        visible_data,
                        scale,
                        bar_seconds,
                        &self
                            .study_rows_at(start_idx + local_idx)
                            .into_iter()
                            .filter(|(id, _)| {
                                !tooltip_options.on_long_press
                                    || tooltip_options.show_volume_details
                                    || self
                                        .study_ids
                                        .iter()
                                        .zip(self.rendered_studies().indicators())
                                        .any(|(study_id, study)| {
                                            study_id == id && !study.is_volume()
                                        })
                            })
                            .map(|(_, row)| row)
                            .collect::<Vec<_>>(),
                        self.instrument().or(heading.as_ref()),
                        self.value_formatters(),
                    );
                }
            }
        } else {
            // Clear hover bar index when not hovering locally
            self.last_hover_bar_idx = None;

            // Render synced crosshair from other charts (if available)
            {
                if !menu_open && let Some(bar_idx) = self.synced_crosshair_bar_idx {
                    // Convert bar index to screen x coordinate
                    let x = coords.idx_to_x(bar_idx.round() as usize);
                    if coords.is_x_visible(x) {
                        // At the host's price, or a synthetic position at the centre of the
                        // price range for a time alone
                        let mut options = self.chart_options.crosshair;
                        let y = match self.synced_crosshair_price {
                            Some(price) => {
                                options.mode = crate::config::CrosshairMode::Normal;
                                price_map.price_to_y(price)
                            }
                            None => price_rect.center().y,
                        };
                        if let Some(bar) = self.state.shown().get(bar_idx.round() as usize) {
                            self.last_crosshair = Some((bar.time, self.synced_crosshair_price));
                        }
                        crosshair_tags = rendering::render_crosshair_with_options(
                            &price_ctx,
                            Pos2::new(x, y),
                            visible_data,
                            &price_map,
                            &coords,
                            &options,
                            time_axis_top,
                            axis_edge,
                            last_price_tag,
                            scale,
                            bar_seconds,
                            price_formatter.as_deref(),
                        );
                    }
                }
            }
        }

        self.axis_hover = hover_pos.zip(crosshair_tags[0]);
        #[cfg(feature = "icons")]
        if show_price_axis {
            if let Some((pos, tag)) = self.axis_hover.filter(|(pos, _)| {
                self.config.show_plus_button
                    && price_rect.contains(*pos)
                    && !volume_rect.contains(*pos)
            }) {
                let price = if self.chart_options.crosshair.mode.snaps()
                    && let Some(index) = coords.local_idx_at_x(pos.x, visible_data.len())
                {
                    price_map.y_to_price(self.snap_crosshair(pos, &price_map, start_idx + index).y)
                } else {
                    price_map.y_to_price(pos.y)
                };
                let foreground = self
                    .chart_options
                    .crosshair
                    .resolved(ui.ctx())
                    .label_text_color;
                if let Some(price) =
                    rendering::axes::crosshair_price_action(ui, tag, price, foreground)
                {
                    self.right_click = Some(RightClickTarget::Background {
                        pos: tag.left_bottom(),
                        price,
                    });
                }
            }
            if rendering::axes::axis_settings_button(ui, axis_corner).clicked() {
                self.scale_settings_requested = true;
            }
        }

        let current_price = crate::model::last_with_value(&self.state.data().bars)
            .filter(|_| self.main_series_visible && self.config.show_symbol_last_val)
            .map(|bar| {
                price_map.transform().label(
                    price_map.transform().to_scale(bar.close),
                    rendering::axes::tag_decimals(&price_map, price_map.price_to_y(bar.close)),
                    price_formatter.as_deref(),
                )
            });
        self.measured_axis_width = rendering::axes::price_axis_width(
            &painter,
            &price_map,
            &current_price.as_deref().into_iter().collect::<Vec<_>>(),
            price_formatter.as_deref(),
        );
        self.measured_axis_width = self.measured_axis_width.max(independent_axis_width);
        for tag in tags.iter().skip(usize::from(has_currency)).flatten() {
            self.measured_axis_width = self.measured_axis_width.max(tag.width() + 5.0 + 8.0);
        }

        if show_price_axis && has_currency {
            let currency = self
                .instrument()
                .and_then(|instrument| instrument.currency.as_deref())
                .filter(|_| self.config.show_currency)
                .unwrap_or("");
            let unit = self
                .instrument()
                .and_then(|instrument| instrument.unit.as_deref())
                .filter(|_| self.config.show_unit);

            let header = match (currency.is_empty(), unit.filter(|unit| !unit.is_empty())) {
                (false, Some(unit)) => format!("{currency} · {unit}"),
                (true, Some(unit)) => unit.to_owned(),
                _ => currency.to_owned(),
            };
            let header_width = painter
                .layout_no_wrap(header, rendering::axes::axis_font(ui.ctx()), colors.text)
                .size()
                .x
                + if currency.is_empty() { 16.0 } else { 26.0 };
            let axis = Rect::from_min_max(
                Pos2::new(
                    if self.config.show_right_axis {
                        price_rect.right()
                    } else {
                        rect.left()
                    },
                    price_rect.top(),
                ),
                Pos2::new(axis_edge, price_rect.bottom()),
            );
            let (button, choice) =
                renderers::render_axis_currency(ui, axis, currency, unit, &self.currency_choices);
            self.measured_axis_width = self.measured_axis_width.max(header_width);
            if choice.is_some() {
                self.currency_choice = choice;
            }
            tags.push(Some(button.rect));
        }

        // Draw the axis labels, leaving out those the tags overlap
        tags.extend(crosshair_tags);
        let time_tags = if time_tags.is_empty() {
            &tags
        } else {
            time_tags.extend_from_slice(&tags);
            &time_tags
        };
        if self.config.show_right_axis {
            rendering::render_price_labels(
                &price_ctx,
                &price_map,
                &colors,
                &self.config.price_axis,
                &tags,
                price_labels,
                price_formatter.as_deref(),
            );
        } else if self.config.show_left_axis {
            renderers::render_price_labels_at_pos(
                &price_ctx,
                &price_map,
                &colors,
                &self.config.price_axis,
                crate::scales::PriceScaleId::Left,
                &tags,
                price_labels,
                price_formatter.as_deref(),
            );
        }
        if self.config.show_time_labels {
            let axis_context_rect =
                Rect::from_min_max(chart_rect.min, Pos2::new(chart_rect.right(), time_axis_top));
            let chart_ctx = RenderContext::new(&painter, axis_context_rect);
            rendering::render_time_labels(
                &chart_ctx,
                time_marks,
                &time_points,
                &time_coords,
                &colors,
                time_tags,
                &self.chart_options.time_scale,
                time_labels,
            );
        }

        // Draw box zoom rect
        rendering::render_box_zoom(&painter, &self.box_zoom);

        if let Some((nav, buttons)) = navigation {
            self.paint_navigation(ui, nav, buttons);
        }

        self.show_realtime_button(ui, realtime_rect, shown.id);

        // Focus ring for keyboard accessibility
        crate::styles::focus::draw_focus_ring(ui, &shown, 0.0);

        self.show_signal_menu(ui, price_rect);
        if let Some(action) = study_action {
            self.apply_study_action(action);
        }
        if let Some(action) = series_action {
            self.apply_main_series_action(action);
        }
        self.main_pane_actions(ui, rect, chart_rect);
        self.record_host_click(ui, &response);
        shown
    }
}

/// The sessions of `clock` that the bars `shown` of `bars` belong to, each from the open of its
/// first bar to the next session's.
#[cfg(feature = "studies")]
fn session_ranges(
    bars: &[crate::model::Bar],
    shown: std::ops::Range<usize>,
    clock: &crate::model::BarClock,
) -> Option<crate::studies::TradeProfileRanges> {
    use crate::model::SessionProvider;
    let in_view = bars.get(shown.clone())?;
    let (first, last) = (in_view.first()?, in_view.last()?);
    let sessions = clock.sessions();
    let starts: Vec<_> = std::iter::once(bars[..shown.start].partition_point(|bar| {
        sessions
            .session_break_between(&bar.time, &first.time)
            .is_some()
    }))
    .chain(
        sessions
            .session_breaks(in_view)
            .into_iter()
            .map(|(i, _)| shown.start + i),
    )
    .collect();
    let end = shown.end
        + bars[shown.end..].partition_point(|bar| {
            sessions
                .session_break_between(&last.time, &bar.time)
                .is_none()
        });
    let close = bars
        .get(end)
        .map_or(chrono::DateTime::<chrono::Utc>::MAX_UTC, |bar| bar.time);
    let ends = starts.iter().skip(1).map(|&next| bars[next].time);
    let ranges = starts
        .iter()
        .zip(ends.chain([close]))
        .map(|(&start, end)| bars[start].time..end)
        .collect();
    crate::studies::TradeProfileRanges::new(ranges).ok()
}

#[cfg(test)]
mod tests {
    use crate::chart::series::SeriesSettings;
    use crate::scales::PriceScaleMode;
    use crate::widget::test_frames::{chart, frame, shapes};
    use egui::{Event, Pos2, Rect, Shape};

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

    #[test]
    fn footprint_stays_below_legend_at_wide_and_narrow_sizes() {
        use crate::chart::overlays::{FootprintColor, FootprintConfig};
        use crate::data::Footprint;
        use crate::model::{BarClock, OrderSide, Tick, Timeframe};
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for (width, pane) in [(1440.0, false), (600.0, false), (600.0, true)] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut chart = chart(&[100.0], &[110.0]);
                chart.set_symbol("FLOW");
                #[cfg(feature = "studies")]
                if pane {
                    chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
                }
                let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
                flow.push(&Tick::new(
                    chart
                        .bars(crate::widget::SeriesId::MAIN)
                        .last()
                        .unwrap()
                        .time,
                    100.0,
                    2.0,
                    Some(OrderSide::Buy),
                ))
                .unwrap();
                let config = FootprintConfig {
                    color_by: FootprintColor::Volume,
                    neutral_color: egui::Color32::from_rgb(21, 45, 78),
                    ..Default::default()
                };
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            Pos2::ZERO,
                            egui::vec2(width, 500.0),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        chart.show_footprint(ui, &flow, &config);
                    },
                );
                output.textures_delta.clear();
                assert!(
                    output.shapes.iter().any(|shape| match &shape.shape {
                        Shape::Rect(rect) => rect.fill == theme.chart.volume_up(),
                        _ => false,
                    }),
                    "footprint keeps the volume histogram"
                );
                let cells = output
                    .shapes
                    .iter()
                    .position(|shape| match &shape.shape {
                        Shape::Mesh(mesh) => mesh
                            .vertices
                            .iter()
                            .any(|v| v.color == config.neutral_color.gamma_multiply(0.5)),
                        _ => false,
                    })
                    .expect("footprint cells");
                let legend = output
                    .shapes
                    .iter()
                    .position(|shape| match &shape.shape {
                        Shape::Text(text) => text.galley.text().contains("FLOW"),
                        _ => false,
                    })
                    .expect("chart legend");
                assert!(
                    cells < legend,
                    "width {width}, pane {pane}: cells cover legend"
                );
            }
        }
    }

    /// The main series' legend actions hide it, request its settings and remove it through the
    /// chart's typed operations, leaving the studies and other series in place.
    #[cfg(feature = "studies")]
    #[test]
    fn main_series_legend_actions_preserve_studies_and_comparisons() {
        use crate::chart::renderers::SeriesAction;
        use crate::widget::{SeriesId, SeriesOptions};
        let mut chart = chart(&[100.0], &[110.0]);
        let study = chart.add_indicator(Box::new(crate::studies::SMA::new(1)));
        let other = chart.add_series(SeriesOptions::default());
        chart.apply_main_series_action(SeriesAction::ToggleVisibility);
        assert!(!chart.is_main_series_visible());
        chart.apply_main_series_action(SeriesAction::ToggleVisibility);
        assert!(chart.is_main_series_visible());
        chart.apply_main_series_action(SeriesAction::Settings);
        assert_eq!(
            chart.take_indicator_settings_request(),
            Some(SeriesId::MAIN)
        );
        chart.apply_main_series_action(SeriesAction::Remove);
        assert!(!chart.main_series_present);
        assert!(chart.indicators().any(|(id, _)| id == study));
        assert!(chart.series_scale(other).is_some());
    }

    #[cfg(feature = "studies")]
    #[test]
    fn status_visibility_reaches_the_chart_legend() {
        use crate::model::{Instrument, Level1Quote};
        for (description, hidden) in [
            (true, 0),
            (false, 0),
            (true, 1),
            (true, 2),
            (true, 3),
            (true, 4),
            (true, 5),
            (true, 6),
            (true, 7),
            (true, 8),
            (true, 9),
            (true, 10),
            (true, 11),
        ] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[100.0], &[110.0]);
            chart.set_instrument(Some(
                Instrument::new("TICKER").with_description("Instrument description"),
            ));
            let bar = chart.state.data().bars.last().unwrap();
            chart.set_quote(Some(Level1Quote {
                symbol: "TICKER".into(),
                ts: bar.time,
                bid: 109.0,
                ask: 111.0,
                bid_size: 1.0,
                ask_size: 1.0,
            }));
            let study = chart.add_indicator(Box::new(crate::studies::SMA::new(1)));
            chart.set_indicator_inputs(study, "1 close");
            let oscillator = chart.add_indicator(Box::new(crate::studies::RSI::new(2)));
            chart.set_indicator_inputs(oscillator, "2 close");
            let config = chart.config_mut();
            config.title_as_description = description;
            config.show_volume = hidden != 10;
            config.show_symbol_labels = hidden != 1;
            config.show_quote_boxes = hidden != 2;
            config.show_ohlc_info = hidden != 3;
            config.show_ohlc_change = hidden != 4;
            config.show_ohlc_change_percent = hidden != 9;
            config.show_legend_volume = hidden != 5;
            config.show_indicator_labels = hidden != 6;
            config.show_indicator_inputs = hidden != 7;
            config.show_indicator_last_val = hidden != 8;
            chart.set_main_series_visible(hidden != 11);
            frame(&ctx, &mut chart, 0.0, vec![]);
            if let Some(volume) = chart.volume_study {
                chart
                    .indicator_mut(volume)
                    .unwrap()
                    .set_input("show_average", crate::studies::InputValue::Toggle(true))
                    .unwrap();
            }
            let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
            assert_eq!(painted.iter().any(|shape| matches!(shape, Shape::Rect(rect)
                if rect.fill == chart.config.bullish_color && chart.last_rendered_price_rect.contains(rect.rect.center()))), hidden != 11, "main series body: hidden={hidden}");
            assert_eq!(chart.bars(crate::widget::SeriesId::MAIN).len(), 100);
            let texts: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Text(text) if text.pos.x < chart.last_rendered_price_rect.right() => {
                        Some(text.galley.text())
                    }
                    _ => None,
                })
                .collect();
            for (text, flag) in [
                ("SELL", 2),
                ("BUY", 2),
                ("SMA", 6),
                ("1 close", 7),
                ("110.00", 8),
                ("O 100.00", 3),
            ] {
                assert_eq!(
                    texts.contains(&text),
                    hidden != flag && (hidden != 11 || matches!(flag, 6..=8)),
                    "{text}: hidden={hidden} {texts:?}"
                );
            }
            for (text, visible) in [
                ("Vol 1", hidden == 10),
                ("Vol", !matches!(hidden, 5 | 6 | 10 | 11)),
                ("SMA 9", !matches!(hidden, 5 | 7 | 10 | 11)),
                ("1", !matches!(hidden, 5 | 8 | 10 | 11)),
            ] {
                assert_eq!(texts.contains(&text), visible, "{text}: hidden={hidden}");
            }
            assert_eq!(
                texts.iter().any(|text| text.contains("RSI")),
                hidden != 6,
                "{texts:?}"
            );
            assert_eq!(
                texts.iter().any(|text| text.contains("2 close")),
                hidden != 7,
                "{texts:?}"
            );
            let title = if description {
                "Instrument description"
            } else {
                "TICKER"
            };
            assert_eq!(
                texts.iter().any(|text| text.starts_with(title)),
                hidden != 1,
                "{texts:?}"
            );
            assert_eq!(
                chart.last_rendered_volume_rect.is_positive(),
                hidden != 10 && hidden != 11,
                "legend visibility and histogram visibility are independent"
            );
        }
    }

    #[test]
    fn automatic_price_precision_reaches_every_axis_tag() {
        use crate::model::{Bar, BarData, Level1Quote, PriceLine, Timeframe};
        use crate::studies::{CustomIndicator, IndicatorValue};
        let start = chrono::DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .to_utc();
        let bars = (0..100)
            .map(|i| {
                let base = 1.0850 + 0.0002 * (i % 10) as f64;
                Bar::new(
                    start + chrono::Duration::hours(i),
                    base,
                    base + 0.0004,
                    base - 0.0004,
                    base + 0.00012,
                    1.0,
                )
            })
            .collect();
        let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
        chart.set_timeframe(Timeframe::Hour1);
        chart.config.show_volume = false;
        chart.config.show_high_low = true;
        chart.config.show_bid_ask = true;
        chart.config.show_symbol_prev_close = true;
        chart.config.show_extended_session_price = true;
        chart.config.price_axis.allow_label_overlap = true;
        chart.config.price_axis.show_study_names = true;
        chart.config.show_quote_boxes = false;
        chart.price_scale_mut().set_range(1.0845, 1.0873);
        chart.set_quote(Some(Level1Quote {
            symbol: "TEST".into(),
            ts: start + chrono::Duration::hours(99),
            bid: 1.08652,
            ask: 1.08702,
            bid_size: 1.0,
            ask_size: 1.0,
        }));
        chart.set_session_quote(Some(crate::model::quote::SessionQuote::new(1.08512, "EXT")));
        chart.add_price_line(
            crate::widget::SeriesId::MAIN,
            PriceLine::new(1.08532, egui::Color32::GOLD),
        );
        chart.add_indicator(Box::new(
            CustomIndicator::new("Study", |bars| {
                vec![IndicatorValue::Single(1.08572); bars.len()]
            })
            .with_overlay(true),
        ));
        let series = chart.add_series(crate::widget::SeriesOptions::default());
        chart.set_data(
            series,
            (0..100)
                .map(|i| {
                    let close = 1.08632;
                    Bar::new(
                        start + chrono::Duration::hours(i),
                        close,
                        close,
                        close,
                        close,
                        1.0,
                    )
                })
                .collect(),
        );
        let ctx = egui::Context::default();
        shapes(&ctx, &mut chart, 0.0, vec![]);
        let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
        let labels: Vec<_> = painted
            .iter()
            .filter_map(|shape| match shape {
                Shape::Text(text) if text.pos.x >= chart.last_rendered_price_rect.right() => {
                    Some(text.galley.text())
                }
                _ => None,
            })
            .collect();
        // A 0.00025 tick step uses six decimals. These literal prices come from the bars above.
        for expected in [
            "1.086920\n01:00:00",
            "EXT 1.085120",
            "1.085320",
            "Study 1.085720",
            "1.086120",
            "High 1.087200",
            "Low 1.084600",
            "1.086520",
            "1.087020",
            "1.086320",
        ] {
            assert!(labels.contains(&expected), "missing {expected}: {labels:?}");
        }
    }

    #[test]
    fn price_labels_and_lines_are_independent_on_either_axis() {
        use crate::model::{Level1Quote, Timeframe};
        // `series` sets the main series' own label and price line, which the chart-wide
        // switches override.
        for left in [false, true] {
            for (label, line, countdown, series) in [
                (true, true, true, true),
                (true, false, false, true),
                (false, true, false, true),
                (false, false, false, true),
                (true, true, false, false),
            ] {
                let ctx = egui::Context::default();
                let mut chart = chart(&[100.0], &[110.0, 120.0]);
                chart.set_timeframe(Timeframe::Min1);
                chart.config.show_right_axis = !left;
                chart.config.show_left_axis = left;
                chart.config.show_volume = false;
                chart.config.show_symbol_labels = false;
                chart.config.show_ohlc_info = false;
                chart.config.show_ohlc_change = false;
                chart.config.show_ohlc_change_percent = false;
                chart.config.show_symbol_last_val = label;
                chart.config.show_price_line = line;
                chart.config.show_symbol_prev_close = label;
                chart.config.show_prev_close_line = line;
                chart.config.show_countdown = countdown;
                chart.config.show_bid_ask = label;
                chart.main_series_style.last_value_visible = series;
                chart.main_series_style.price_line_visible = series;
                let bar = chart.state.data().bars.last().unwrap();
                chart.set_quote(Some(Level1Quote {
                    symbol: "TEST".into(),
                    ts: bar.time + chrono::Duration::seconds(30),
                    bid: 119.0,
                    ask: 121.0,
                    bid_size: 1.0,
                    ask_size: 1.0,
                }));
                chart.config.show_quote_boxes = false;
                chart.set_session_quote(Some(crate::model::quote::SessionQuote::new(
                    115.0, "EXTENDED",
                )));
                chart.set_price_lines(
                    crate::chart::series::SeriesId::MAIN,
                    vec![crate::model::PriceLine::new(115.0, egui::Color32::GOLD)],
                );
                frame(&ctx, &mut chart, 0.0, vec![]);
                let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
                let plot = chart.last_rendered_price_rect;
                let custom_tag = painted
                    .iter()
                    .find_map(|shape| match shape {
                        Shape::Rect(tag) if tag.fill == egui::Color32::GOLD => Some(tag.rect),
                        _ => None,
                    })
                    .expect("custom price line retains its axis label");
                assert!(if left {
                    custom_tag.right() <= plot.left()
                } else {
                    custom_tag.left() >= plot.right()
                });
                let session_text = painted
                    .iter()
                    .find_map(|shape| match shape {
                        Shape::Text(text) if text.galley.text().starts_with("EXTENDED") => {
                            Some(text)
                        }
                        _ => None,
                    })
                    .expect("session tag");
                assert!(
                    session_text.pos.x
                        >= plot.left()
                            - chart.config.left_axis_width.max(chart.measured_axis_width),
                    "session label is clipped on the left"
                );
                for name in ["Bid", "Ask"] {
                    let badge = painted.iter().find_map(|shape| match shape {
                        Shape::Text(text) if text.galley.text() == name => {
                            Some(text.visual_bounding_rect())
                        }
                        _ => None,
                    });
                    assert_eq!(badge.is_some(), label);
                    if let Some(badge) = badge {
                        assert!(
                            ctx.content_rect().contains_rect(badge)
                                && if left {
                                    badge.center().x >= plot.left()
                                } else {
                                    badge.center().x <= plot.right()
                                },
                            "{name} stays inside the plot on either axis"
                        );
                    }
                }
                let close_y = chart.last_price_map.price_to_y(120.0);
                let prev_y = chart.last_price_map.price_to_y(110.0);
                let price_tags: Vec<_> = painted
                    .iter()
                    .filter_map(|shape| match shape {
                        Shape::Rect(rect)
                            if rect.fill == chart.config.bullish_color
                                && (rect.rect.center().y - close_y).abs() < 1.0
                                && rect.rect.width() > 20.0 =>
                        {
                            Some(rect.rect)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    !price_tags.is_empty(),
                    label && series,
                    "{left} {label} {line}"
                );
                for tag in price_tags {
                    assert!(if left {
                        tag.right() <= plot.left()
                    } else {
                        tag.left() >= plot.right()
                    });
                }
                for (y, color, shown) in [
                    (close_y, chart.config.bullish_color, line && series),
                    (
                        prev_y,
                        crate::theme::ThemeExt::active_theme(&ctx).ui.text_secondary,
                        line,
                    ),
                ] {
                    let has_line = painted.iter().any(|shape| match shape {
                        Shape::LineSegment { points, stroke } => {
                            stroke.color == color
                                && (points[0].y - y).abs() < 0.01
                                && (points[1].y - y).abs() < 0.01
                        }
                        Shape::Mesh(mesh) => {
                            mesh.vertices.len() > 8
                                && mesh.vertices.iter().all(|vertex| {
                                    vertex.color == color && (vertex.pos.y - y).abs() <= 1.5
                                })
                        }
                        _ => false,
                    });
                    assert_eq!(
                        has_line, shown,
                        "line at {y}: left={left} label={label} line={line}"
                    );
                }
                assert_eq!(painted.iter().any(|shape| matches!(shape, Shape::Text(text) if text.galley.text().contains("00:30"))), countdown);
            }
        }
    }

    /// On a logarithmic scale 100 lies halfway between 10 and 1,000 (ln 100 is the mean of ln 10 and
    /// ln 1,000); a linear mapping puts it a tenth of the way up. A range zoomed by hand before the
    /// switch, current or in the zoom history, is in the old mode's units and must not carry over.
    #[test]
    fn log_mode_reaches_the_rendered_mapping() {
        let ctx = egui::Context::default();
        let mut chart = chart(&[10.0], &[1000.0]);
        frame(&ctx, &mut chart, 0.0, Vec::new());
        chart.state.price_scale_mut().zoom(0.5, 500.0);
        chart.state.push_zoom_state();
        chart.apply_series_settings(&SeriesSettings {
            price_scale_mode: PriceScaleMode::Logarithmic,
            ..SeriesSettings::default()
        });
        chart.state.pop_zoom_state();
        frame(&ctx, &mut chart, 0.1, Vec::new());
        let map = chart.get_chart_mapping();
        let (y10, y100, y1000) = (
            map.price_to_y(10.0),
            map.price_to_y(100.0),
            map.price_to_y(1000.0),
        );
        assert!(
            (y100 - (y10 + y1000) / 2.0).abs() < 0.01,
            "{y10} {y100} {y1000}"
        );
        let (top, bottom) = map.price.extent();
        assert!(
            top < y1000 && y10 < bottom,
            "{y10}..{y1000} outside {top}..{bottom}"
        );
    }

    /// An update starts one last price pulse, loading does not, and a continuous pulse always
    /// runs; the crosshair marker sits on the line at the hovered bar's close.
    #[test]
    fn line_pulse_and_crosshair_marker_follow_their_bars() {
        use crate::model::{ChartType, LastPriceAnimation};
        use egui::Color32;
        let ctx = egui::Context::default();
        let mut chart = chart(&[100.0], &[110.0]);
        chart.set_chart_type(ChartType::Line);
        let mut params = chart.chart_type_params().clone();
        params.styles.last_price_animation = LastPriceAnimation::OnDataUpdate;
        params.styles.crosshair_marker_radius = 5.0;
        chart.set_chart_type_params(params.clone());
        let color = params.styles.line.color;
        let circles = |shapes: Vec<Shape>, radius: f32| {
            shapes
                .into_iter()
                .filter_map(|shape| match shape {
                    Shape::Circle(circle) if circle.radius == radius && circle.fill == color => {
                        Some(circle.center)
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        assert!(circles(shapes(&ctx, &mut chart, 0.0, vec![]), 3.0).is_empty());
        let mut last = chart.bars(crate::widget::SeriesId::MAIN)[99].clone();
        last.close = 105.0;
        chart.update(crate::widget::SeriesId::MAIN, last);
        let map = chart.get_chart_mapping();
        assert_eq!(
            circles(shapes(&ctx, &mut chart, 1.0, vec![]), 3.0),
            vec![Pos2::new(map.idx_to_x(99), map.price_to_y(105.0))]
        );
        assert!(circles(shapes(&ctx, &mut chart, 3.7, vec![]), 3.0).is_empty());
        params.styles.last_price_animation = LastPriceAnimation::Continuous;
        chart.set_chart_type_params(params);
        assert_eq!(circles(shapes(&ctx, &mut chart, 5.0, vec![]), 3.0).len(), 1);

        let map = chart.get_chart_mapping();
        let hover = Pos2::new(map.idx_to_x(70), chart.last_rendered_price_rect.center().y);
        assert_eq!(
            circles(
                shapes(&ctx, &mut chart, 5.1, vec![Event::PointerMoved(hover)]),
                5.0
            ),
            vec![Pos2::new(map.idx_to_x(70), map.price_to_y(110.0))]
        );
        // Trailing whitespace leaves the pulse on the last valued bar; both dots use that
        // bar's own colour, and a hovered empty slot has no dot.
        let mut bars = chart.bars(crate::widget::SeriesId::MAIN).to_vec();
        bars[98].colors = Some(Box::new(crate::model::BarColors {
            color: Some(Color32::GOLD),
            ..Default::default()
        }));
        bars[99] = crate::model::Bar::whitespace(bars[99].time);
        chart.set_data(crate::widget::SeriesId::MAIN, bars);
        let map = chart.get_chart_mapping();
        for (time, index, marker) in [(5.2, 98, true), (5.3, 99, false)] {
            let hover = Pos2::new(
                map.idx_to_x(index),
                chart.last_rendered_price_rect.center().y,
            );
            let painted = shapes(&ctx, &mut chart, time, vec![Event::PointerMoved(hover)]);
            for radius in [3.0, 5.0] {
                let dots: Vec<_> = painted
                    .iter()
                    .filter_map(|shape| match shape {
                        Shape::Circle(dot) if dot.radius == radius && dot.fill == Color32::GOLD => {
                            Some(dot.center)
                        }
                        _ => None,
                    })
                    .collect();
                let expected = if radius == 3.0 || marker {
                    vec![Pos2::new(map.idx_to_x(98), map.price_to_y(110.0))]
                } else {
                    Vec::new()
                };
                assert_eq!(dots, expected, "{radius} at {index}");
            }
        }
    }

    /// Axis buttons route their clicks without changing the price range or viewport.
    #[cfg(feature = "icons")]
    #[test]
    fn axis_buttons_route_without_scaling() {
        for (settings, magnet, enabled) in [
            (false, false, true),
            (true, false, true),
            (false, true, true),
            (false, false, false),
        ] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[100.0], &[110.0]);
            chart.config.show_plus_button = enabled;
            if magnet {
                chart.chart_options.crosshair.mode = crate::config::CrosshairMode::Magnet;
            }
            frame(&ctx, &mut chart, 0.0, vec![]);
            let plot = chart.last_rendered_price_rect;
            frame(
                &ctx,
                &mut chart,
                0.1,
                vec![Event::PointerMoved(plot.center() + egui::vec2(0.0, 10.0))],
            );
            let map = chart.get_chart_mapping();
            let range = map.price.range();
            let viewport = chart.get_time_scale_state();
            let pos = if settings {
                Pos2::new(
                    plot.right() + chart.config.right_axis_width / 2.0,
                    plot.bottom() + 14.0,
                )
            } else {
                let (_, tag) = chart.axis_hover.expect("hover tag");
                tag.left_center() + egui::Vec2::new(10.0, if magnet { 5.0 } else { 0.0 })
            };
            for (step, events) in [
                vec![Event::PointerMoved(pos)],
                vec![navigation_press(pos, true)],
                vec![navigation_press(pos, false)],
            ]
            .into_iter()
            .enumerate()
            {
                frame(&ctx, &mut chart, 0.2 + step as f64 * 0.1, events);
            }
            let expected_price = if magnet {
                100.0
            } else {
                map.y_to_price(plot.center().y + 10.0)
            };
            if settings {
                assert!(chart.take_scale_settings_request());
                assert!(!chart.take_scale_settings_request());
            } else if !enabled {
                assert!(chart.take_right_click().is_none());
            } else {
                assert!(
                    matches!(chart.take_right_click(), Some(super::RightClickTarget::Background { price, .. }) if (price - expected_price).abs() < 0.001)
                );
            }
            assert_eq!(chart.get_chart_mapping().price.range().min, range.min);
            assert_eq!(chart.get_chart_mapping().price.range().max, range.max);
            assert_eq!(chart.get_time_scale_state(), viewport);
        }
    }

    /// Held values appear only for a pressed pointer and disappear on release or disable. A
    /// hidden crosshair keeps them and shows no axis label while the pointer is held.
    #[test]
    fn held_values_follow_press_release_and_switch() {
        for (_, theme) in crate::theme::presets() {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut chart = chart(&[100.0], &[110.0, 120.0]);
            chart.config.bullish_color = egui::Color32::BLUE;
            chart.config.show_ohlc_info = true;
            chart.chart_options.crosshair.style = crate::config::CrosshairStyle::Hidden;
            chart.chart_options.tooltip.on_long_press = true;
            chart.chart_options.tooltip.show_time = false;
            #[cfg(feature = "studies")]
            chart.add_indicator(Box::new(crate::studies::SMA::new(3)));
            frame(&ctx, &mut chart, 0.0, vec![]);
            let map = chart.get_chart_mapping();
            let pos = Pos2::new(map.idx_to_x(50), map.rect.center().y);
            for (step, enabled, pressed, expected, volume_details) in [
                (1, true, false, false, false),
                (2, true, true, true, false),
                (3, true, true, true, true),
                (4, true, false, false, false),
                (5, false, true, false, false),
                (6, false, false, false, false),
            ] {
                chart.set_tooltip_enabled(enabled);
                chart.chart_options.tooltip.show_volume_details = volume_details;
                let painted = shapes(
                    &ctx,
                    &mut chart,
                    step as f64 * 0.1,
                    vec![
                        Event::PointerMoved(pos),
                        Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                );
                let labels: Vec<_> = painted
                    .iter()
                    .filter_map(|shape| match shape {
                        Shape::Text(text) => Some(text.galley.text()),
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    labels.contains(&"Open"),
                    expected,
                    "step {step}: {labels:?}"
                );
                assert_eq!(chart.axis_hover, None, "step {step}");
                assert!(painted.iter().any(|shape| matches!(shape, Shape::Text(text)
                    if text.galley.text() == "O 100.00" && text.galley.job.sections[1].format.color == egui::Color32::BLUE)));
                if expected {
                    assert_eq!(
                        labels.iter().any(|label| label.starts_with("Volume:")),
                        volume_details && cfg!(feature = "studies"),
                        "volume study details are opt-in"
                    );
                    assert!(labels.contains(&"Time"));
                    assert!(!labels.contains(&"Change"));
                    assert!(labels.contains(&"Vol"));
                    #[cfg(feature = "studies")]
                    {
                        assert!(labels.contains(&"SMA"), "other study values remain visible");
                        assert!(
                            chart
                                .study_values_at(50)
                                .iter()
                                .any(|row| row.study == "Volume")
                        );
                    }
                    let legend = labels
                        .iter()
                        .position(|label| *label == "O 100.00")
                        .unwrap();
                    let tooltip = labels.iter().position(|label| *label == "Open").unwrap();
                    assert!(
                        tooltip > legend,
                        "the floating readout must paint above the legend"
                    );
                    assert!(painted.iter().any(|shape| matches!(shape, Shape::Text(text)
                        if text.galley.text() == "110.00" && text.galley.job.sections[0].format.color == egui::Color32::BLUE)));
                }
            }
        }
    }

    #[test]
    fn tpo_days_follow_the_instrument_zone_without_trading_hours() {
        use crate::model::{Bar, BarData, ChartType, Instrument, Timeframe, TimezoneId};
        // Half-hourly bars from 02:00 to 08:00 UTC, across midnight in New York at 05:00 UTC
        let start: chrono::DateTime<chrono::Utc> = "2024-03-05T02:00:00Z".parse().unwrap();
        let bars = (0..13)
            .map(|i| {
                let time = start + chrono::Duration::minutes(30 * i);
                Bar::new(time, 100.0, 102.0, 99.0, 101.0, 1.0)
            })
            .collect();
        let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
        chart.set_timeframe(Timeframe::Min30);
        chart.set_chart_type(ChartType::TimePriceOpportunity);
        let new_york = TimezoneId::fixed_offset(-5 * 3600).unwrap();
        chart.set_instrument(Some(Instrument::new("FX").with_time_zone(new_york)));
        let painted = shapes(&egui::Context::default(), &mut chart, 0.0, vec![]);
        // One initial balance bar beside each profile
        let initial_balance = crate::tokens::DESIGN_TOKENS.semantic.tpo.initial_balance;
        let profiles = painted
            .iter()
            .filter(|shape| {
                matches!(shape, Shape::Rect(rect)
                    if rect.fill == initial_balance && rect.rect.width() == 2.0)
            })
            .count();
        assert_eq!(profiles, 2);
    }

    #[cfg(feature = "timezones")]
    #[test]
    fn configured_sessions_reach_breaks_and_extended_shading() {
        use crate::model::{Bar, BarData, Timeframe};
        let times = [
            "2024-03-08T23:00:00Z",
            "2024-03-11T07:59:00Z",
            "2024-03-11T08:00:00Z",
            "2024-03-11T13:00:00Z",
            "2024-03-11T13:30:00Z",
            "2024-03-11T20:00:00Z",
            "2024-03-11T23:59:00Z",
            "2024-03-12T00:00:00Z",
        ];
        for (shade, custom, metadata, expected) in [
            (true, false, false, vec![2]),
            (false, false, false, vec![2]),
            (true, true, false, vec![1, 6]),
            (true, false, true, vec![3]),
        ] {
            let ctx = egui::Context::default();
            let bars = times
                .iter()
                .map(|time| Bar::new(time.parse().unwrap(), 100.0, 102.0, 99.0, 101.0, 1.0))
                .collect();
            let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
            chart.set_timeframe(Timeframe::Hour1);
            if metadata {
                use crate::model::{Instrument, Session, SessionWindow, TimezoneId};
                use chrono::{NaiveTime, Weekday::*};
                let weekdays = [Mon, Tue, Wed, Thu, Fri];
                let clock = |hour, minute| NaiveTime::from_hms_opt(hour, minute, 0).unwrap();
                // The instrument opens pre-market at 05:00, independent of the display zone.
                chart.set_instrument(Some(
                    Instrument::new("SESSION")
                        .with_time_zone(TimezoneId::named("America/New_York").unwrap())
                        .with_session(
                            Session::new([SessionWindow::new(
                                weekdays,
                                clock(9, 30),
                                clock(16, 0),
                            )])
                            .with_extended_windows([
                                SessionWindow::new(weekdays, clock(5, 0), clock(9, 30)),
                                SessionWindow::new(weekdays, clock(16, 0), clock(20, 0)),
                            ]),
                        ),
                ));
                chart.chart_options.time_scale.timezone =
                    crate::scales::TimezoneMode::Timezone("Europe/Zurich".parse().unwrap());
            } else {
                chart.config.session_config = Some(crate::config::SessionConfig {
                    show_premarket: true,
                    show_postmarket: true,
                    ..crate::config::SessionConfig::nyse()
                });
            }
            chart.config.show_session_breaks = true;
            chart.config.session_break_style = crate::model::LineStyle::Solid;
            chart.config.session_break_color = egui::Color32::from_rgb(201, 37, 180);
            chart.config.extended_hours_color =
                egui::Color32::from_rgba_unmultiplied(90, 40, 170, 30);
            chart.config.show_extended_hours = shade;
            if custom {
                chart.set_session_provider(Some(std::sync::Arc::new(
                    crate::model::DailySessionProvider::new(21, 0),
                )));
            }
            let painted = shapes(&ctx, &mut chart, 0.0, vec![]);
            let map = chart.get_chart_mapping();
            let lines: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::LineSegment { points, stroke }
                        if stroke.color == chart.config.session_break_color.gamma_multiply(0.6) =>
                    {
                        Some(points[0].x)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(
                lines,
                expected
                    .iter()
                    .map(|index| map.idx_to_x(*index))
                    .collect::<Vec<_>>()
            );
            let fills: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Rect(rect) if rect.fill == chart.config.extended_hours_color => {
                        Some(rect.rect)
                    }
                    _ => None,
                })
                .collect();
            let expected: Vec<_> = if shade {
                [(0, 1), (if metadata { 3 } else { 2 }, 4), (5, 7)]
                    .into_iter()
                    .map(|(first, end)| {
                        Rect::from_min_max(
                            Pos2::new(map.idx_to_x(first) - map.bar_spacing / 2.0, map.rect.top()),
                            Pos2::new(
                                map.idx_to_x(end - 1) + map.bar_spacing / 2.0,
                                map.rect.bottom(),
                            ),
                        )
                        .intersect(map.rect)
                    })
                    .collect()
            } else {
                vec![]
            };
            assert_eq!(fills, expected);
        }
    }

    #[test]
    fn runtime_options_size_and_crosshair_position_apply_to_a_built_chart() {
        let ctx = egui::Context::default();
        let mut chart = chart(&[100.0], &[200.0]);
        frame(&ctx, &mut chart, 0.0, Vec::new());
        // Unrelated options keep the view; new bounds clamp it; spacing and offset apply.
        chart.apply_synced_time_scale(3.0, -20.0);
        let mut options = chart.chart_options().clone();
        options.crosshair.mode = crate::config::CrosshairMode::Magnet;
        chart.set_chart_options(options.clone());
        assert_eq!(chart.get_time_scale_state(), (3.0, -20.0));
        // Invalid options retain the configured values, even after a pan and repeated applies.
        for invalid in [f32::NAN, f32::INFINITY, f32::NEG_INFINITY] {
            for field in 0..3 {
                let mut invalid_options = options.clone();
                match field {
                    0 => invalid_options.time_scale.right_offset = invalid,
                    1 => invalid_options.time_scale.right_offset_pixels = Some(invalid),
                    _ => invalid_options.time_scale.bar_spacing = invalid,
                }
                chart.apply_synced_time_scale(3.0, -30.0);
                for _ in 0..3 {
                    chart.set_chart_options(invalid_options.clone());
                    assert_eq!(
                        chart.get_time_scale_state(),
                        (3.0, -30.0),
                        "{field}, {invalid}"
                    );
                }
                assert_eq!(chart.chart_options(), &options);
                chart = chart.with_chart_options(invalid_options);
                assert_eq!(chart.chart_options(), &options);
                chart.apply_synced_time_scale(3.0, -30.0);
                chart.set_chart_options(chart.chart_options().clone());
                assert_eq!(chart.get_time_scale_state(), (3.0, -30.0));
            }
        }
        chart.apply_synced_time_scale(3.0, -20.0);
        options.time_scale.min_bar_spacing = 4.0;
        chart.set_chart_options(options.clone());
        assert_eq!(chart.get_time_scale_state(), (4.0, -20.0));
        options.time_scale.bar_spacing = 8.0;
        options.time_scale.right_offset = 5.0;
        chart.set_chart_options(options.clone());
        assert_eq!(chart.get_time_scale_state(), (8.0, 5.0));
        assert_eq!(chart.chart_options(), &options);

        chart.set_size(Some(egui::vec2(300.0, 200.0)));
        frame(&ctx, &mut chart, 0.1, Vec::new());
        let plot = chart.get_rendered_price_rect();
        assert!(plot.max.x < 300.0 && plot.max.y < 200.0 && plot.width() > 200.0);
        chart.set_size(None);

        // The price label follows the host's price, which no axis tick shows.
        let time = chart.state.shown()[50].time;
        let label = |chart: &mut crate::widget::Chart, t| {
            shapes(&ctx, chart, t, Vec::new())
                .iter()
                .any(|shape| matches!(shape, Shape::Text(text) if text.galley.text() == "137.50"))
        };
        chart.set_crosshair_position(Some((time, 137.5)));
        assert!(label(&mut chart, 0.2));
        chart.set_synced_crosshair_time(Some(time));
        assert!(!label(&mut chart, 0.3));
    }

    #[test]
    fn hovered_series_draw_above_the_others_when_enabled() {
        use crate::studies::{CustomIndicator, IndicatorValue};
        use egui::Color32;
        let levels = [
            (150.0, Color32::from_rgb(201, 1, 1)),
            (170.0, Color32::from_rgb(1, 201, 1)),
        ];
        let ctx = egui::Context::default();
        let mut chart = chart(&[100.0], &[200.0]);
        chart.chart_options.tooltip.enabled = false;
        let mut study_ids = Vec::new();
        for (level, color) in levels {
            study_ids.push(
                chart.add_indicator(Box::new(
                    CustomIndicator::new("Level", move |bars| {
                        vec![IndicatorValue::Single(level); bars.len()]
                    })
                    .with_color(color),
                )),
            );
        }
        let added_color = Color32::from_rgb(1, 1, 201);
        let added = chart.add_series(super::super::SeriesOptions {
            color: added_color,
            ..Default::default()
        });
        let bars = chart
            .bars(super::super::SeriesId::MAIN)
            .iter()
            .map(|bar| crate::model::Bar::from_value(bar.time, 130.0))
            .collect();
        chart.set_data(added, bars);
        frame(&ctx, &mut chart, 0.0, Vec::new());
        let map = chart.get_chart_mapping();
        let candle = chart.config().bullish_color;
        let first = |shapes: &[Shape], color| {
            shapes.iter().position(|shape| match shape {
                Shape::Rect(rect) => rect.fill == color,
                Shape::LineSegment { stroke, .. } => stroke.color == color,
                Shape::Path(path) => path.stroke.color == egui::epaint::ColorMode::Solid(color),
                Shape::Mesh(mesh) => mesh.vertices.iter().any(|vertex| vertex.color == color),
                _ => false,
            })
        };
        // (enabled, hovered price, colours from the bottom up)
        for (t, (enabled, price, order)) in [
            (
                false,
                120.0,
                [candle, added_color, levels[0].1, levels[1].1],
            ),
            (true, 120.0, [added_color, levels[0].1, levels[1].1, candle]),
            (true, 150.0, [candle, added_color, levels[1].1, levels[0].1]),
            (true, 130.0, [candle, levels[0].1, levels[1].1, added_color]),
        ]
        .into_iter()
        .enumerate()
        {
            let t = t as f64;
            chart.chart_options.hovered_series_on_top = enabled;
            let pointer = || {
                vec![Event::PointerMoved(Pos2::new(
                    map.idx_to_x(90),
                    map.price_to_y(price),
                ))]
            };
            frame(&ctx, &mut chart, t + 0.1, pointer());
            let painted = shapes(&ctx, &mut chart, t + 0.2, pointer());
            let positions = order.map(|color| first(&painted, color).unwrap());
            assert!(positions.is_sorted(), "{enabled} {price}: {positions:?}");
        }
        let bars_at = |price| {
            chart
                .bars(super::super::SeriesId::MAIN)
                .iter()
                .map(|bar| crate::model::Bar::from_value(bar.time, price))
                .collect::<Vec<_>>()
        };
        let overlapping = bars_at(150.0);
        let original = bars_at(130.0);
        chart.set_data(added, overlapping);
        for (step, order, expected) in [
            (
                0,
                super::super::VisualOrder::SendBackward,
                super::super::ChartElementId::Series(added),
            ),
            (
                1,
                super::super::VisualOrder::BringToFront,
                super::super::ChartElementId::Indicator(study_ids[0]),
            ),
        ] {
            assert!(chart.reorder_indicator(study_ids[0], order));
            chart.drain_events().for_each(drop);
            frame(
                &ctx,
                &mut chart,
                4.1 + step as f64 * 0.1,
                vec![Event::PointerMoved(Pos2::new(
                    map.idx_to_x(90),
                    map.price_to_y(150.0),
                ))],
            );
            assert!(
                chart.drain_events().any(|event| matches!(event,
                    super::super::ChartEvent::CrosshairMoved(Some(pointer))
                        if pointer.target == Some(super::super::PointerTarget::Element(expected))
                )),
                "visual order {order:?}"
            );
        }
        chart.set_data(added, original);
        let pane = chart.add_pane();
        for id in study_ids.iter().copied().chain([added]) {
            assert!(chart.move_series_to_pane(id, pane));
        }
        for (step, (enabled, price, order)) in [
            (false, 150.0, [levels[0].1, levels[1].1, added_color]),
            (true, 150.0, [levels[1].1, added_color, levels[0].1]),
            (true, 130.0, [levels[0].1, levels[1].1, added_color]),
        ]
        .into_iter()
        .enumerate()
        {
            chart.chart_options.hovered_series_on_top = enabled;
            frame(&ctx, &mut chart, 5.0 + step as f64, vec![]);
            let map = chart.series_price_map(added).unwrap();
            let pos = Pos2::new(
                chart.get_chart_mapping().idx_to_x(90),
                map.price_to_y(price),
            );
            let painted = shapes(
                &ctx,
                &mut chart,
                5.1 + step as f64,
                vec![Event::PointerMoved(pos)],
            );
            let positions = order.map(|color| first(&painted, color).unwrap());
            assert!(
                positions.is_sorted(),
                "pane {enabled} {price}: {positions:?}"
            );
        }
    }

    #[test]
    fn zoomed_out_bars_merge_per_pixel_and_keep_their_extremes() {
        use crate::model::{Bar, BarData, ChartType};
        use chrono::{TimeZone, Utc};
        let bars = (0..65_536)
            .map(|i| {
                let t = Utc.timestamp_opt(1_600_000_000 + i * 60, 0).unwrap();
                let (low, high) = match i {
                    34_321 => (100.0, 1000.0),
                    47_777 => (1.0, 101.0),
                    _ => (100.0, 101.0 + (i % 7) as f64),
                };
                Bar::new(t, 100.0, high, low, 101.0, 1.0)
            })
            .collect();
        struct MergeProbe(std::sync::Arc<std::sync::Mutex<Vec<std::ops::Range<usize>>>>);
        impl super::super::Primitive for MergeProbe {
            fn draw_merged(
                &self,
                _: &egui::Painter,
                view: &super::super::PrimitiveView<'_>,
                pass: super::super::PrimitivePass,
                factor: usize,
            ) {
                if pass == super::super::PrimitivePass::Series {
                    *self.0.lock().unwrap() = view.point_groups(factor).collect();
                }
            }
        }
        let groups = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let ctx = egui::Context::default();
        let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
        chart
            .attach_primitive(super::super::SeriesId::MAIN, MergeProbe(groups.clone()))
            .unwrap();
        let mut options = chart.chart_options().clone();
        options.time_scale.min_bar_spacing = 0.0;
        chart.set_chart_options(options);
        frame(&ctx, &mut chart, 0.0, Vec::new());
        chart.apply_synced_time_scale(chart.get_rendered_price_rect().width() / 65_536.0, 0.0);
        frame(&ctx, &mut chart, 0.1, Vec::new());
        {
            let groups = groups.lock().unwrap();
            assert!(
                !groups.is_empty() && groups.len() < 1000,
                "custom renderer receives pixel groups"
            );
            assert_eq!(groups.first().unwrap().start, 0);
            assert_eq!(groups.last().unwrap().end, 65_536);
            assert!(groups.windows(2).all(|pair| pair[0].end == pair[1].start));
            let view =
                chart.primitive_view(super::super::SeriesId::MAIN, chart.get_chart_mapping());
            let mut edge = view.clone();
            edge.visible = 1..7;
            assert_eq!(edge.point_groups(4).collect::<Vec<_>>(), [0..4, 4..8]);
            for visible in [1..1, 65_536..65_536, 65_537..65_538] {
                edge.visible = visible;
                assert_eq!(edge.point_groups(4).next(), None);
            }
        }
        let color = chart.config().bullish_color;
        let painted_ys = |chart: &mut crate::widget::Chart, t| -> Vec<f32> {
            shapes(&ctx, chart, t, Vec::new())
                .iter()
                .flat_map(|shape| match shape {
                    Shape::Rect(rect) if rect.fill == color => {
                        vec![rect.rect.min.y, rect.rect.max.y]
                    }
                    Shape::LineSegment { points, stroke } if stroke.color == color => {
                        points.iter().map(|point| point.y).collect()
                    }
                    Shape::Mesh(mesh) => mesh
                        .vertices
                        .iter()
                        .filter(|vertex| vertex.color == color)
                        .map(|vertex| vertex.pos.y)
                        .collect(),
                    _ => Vec::new(),
                })
                .collect()
        };
        let ys = painted_ys(&mut chart, 0.2);
        // About one bar per pixel column, each still reaching the highest and lowest price.
        assert!(ys.len() < 20_000, "{} points", ys.len());
        let map = chart.get_chart_mapping();
        let top = ys.iter().copied().fold(f32::INFINITY, f32::min);
        let bottom = ys.iter().copied().fold(f32::NEG_INFINITY, f32::max);
        assert!((top - map.price_to_y(1000.0)).abs() < 1.0, "{top}");
        assert!((bottom - map.price_to_y(1.0)).abs() < 1.0, "{bottom}");

        // Without executions the footprint types draw the same merged candles.
        for chart_type in [ChartType::VolumeFootprint, ChartType::SessionVolumeProfile] {
            chart.set_chart_type(chart_type);
            let drawn = painted_ys(&mut chart, 0.2);
            assert!(
                drawn == ys,
                "{chart_type:?}: {} points against {}",
                drawn.len(),
                ys.len()
            );
        }
        chart.set_chart_type(ChartType::Candles);

        // A live bar opening a group, then updating it, is drawn in it, with its volume.
        let time = Utc.timestamp_opt(1_600_000_000 + 65_536 * 60, 0).unwrap();
        for (t, high) in [(0.3, 1500.0), (0.5, 2000.0)] {
            chart.update(
                crate::widget::SeriesId::MAIN,
                Bar::new(time, 100.0, high, 100.0, 101.0, 1.0),
            );
            frame(&ctx, &mut chart, t, Vec::new());
            let ys = painted_ys(&mut chart, t + 0.1);
            let top = ys.iter().copied().fold(f32::INFINITY, f32::min);
            let y = chart.get_chart_mapping().price_to_y(high);
            assert!((top - y).abs() < 1.0, "{high}: {top} against {y}");
        }
    }

    #[test]
    fn time_visibility_flags_reach_axis_and_crosshair() {
        use crate::model::{Bar, BarData, Timeframe};
        use chrono::{Duration, TimeZone, Utc};
        let time = Utc.with_ymd_and_hms(2024, 3, 10, 6, 59, 30).unwrap();
        for (seconds, time_visible, seconds_visible, ticks_visible, expected) in [
            (1, true, true, false, "Sun 10 Mar '24 01:59:30"),
            (1, true, false, false, "Sun 10 Mar '24 01:59"),
            (1, false, true, false, "Mar 10, 2024"),
            (1, false, false, false, "Mar 10, 2024"),
            (1, true, true, true, "Sun 10 Mar '24 01:59:30"),
            (60, true, true, true, "Sun 10 Mar '24 01:59"),
            (86_400, true, true, true, "Mar 10, 2024"),
        ] {
            let ctx = egui::Context::default();
            let bars = (0..100)
                .map(|i| {
                    Bar::new(
                        time + Duration::seconds((i - 50) * seconds),
                        100.0,
                        102.0,
                        99.0,
                        101.0,
                        1.0,
                    )
                })
                .collect();
            let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
            chart.set_timeframe(if seconds == 1 {
                Timeframe::Sec1
            } else if seconds == 60 {
                Timeframe::Min1
            } else {
                Timeframe::Day1
            });
            chart.chart_options.tooltip.enabled = false;
            // Each change after a frame, as the axis menu makes them, so the time labels follow.
            frame(&ctx, &mut chart, 0.0, vec![]);
            chart.chart_options.time_scale.timezone =
                crate::scales::TimezoneMode::Timezone("-05:00".parse().unwrap());
            frame(&ctx, &mut chart, 0.03, vec![]);
            let options = &mut chart.chart_options.time_scale;
            options.time_visible = time_visible;
            options.seconds_visible = seconds_visible;
            options.ticks_visible = ticks_visible;
            frame(&ctx, &mut chart, 0.06, vec![]);
            let map = chart.get_chart_mapping();
            let pos = Pos2::new(map.idx_to_x(50), map.rect.center().y);
            let painted = shapes(&ctx, &mut chart, 0.1, vec![Event::PointerMoved(pos)]);
            assert!(
                painted.iter().any(
                    |shape| matches!(shape, Shape::Text(text) if text.galley.text() == expected)
                ),
                "{seconds} {time_visible} {seconds_visible}"
            );
            let ticks = painted.iter().filter(|shape| matches!(shape, Shape::LineSegment { points, .. } if (points[0].y - map.rect.bottom()).abs() < 0.1 && (points[1].y - points[0].y - 3.0).abs() < 0.1)).count();
            assert_eq!(ticks > 0, ticks_visible, "tick visibility: {seconds}");
        }
    }

    /// Daily bars put bold day and month labels on the axis; 20 characters space them at
    /// least 200 points apart, which the default 8 (80 points) does not.
    #[test]
    fn time_scale_options_style_and_space_the_axis_labels() {
        for (bold, border, characters) in [(true, false, 8), (false, true, 8), (true, false, 20)] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[95.0], &[105.0]);
            chart.config.show_volume = false;
            let options = &mut chart.chart_options.time_scale;
            options.allow_bold_labels = bold;
            options.border_visible = border;
            options.border_color = Some(egui::Color32::RED);
            options.tick_mark_max_character_length = characters;
            frame(&ctx, &mut chart, 0.0, vec![]);
            let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
            let plot = chart.get_rendered_price_rect();
            let labels: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Text(text)
                        if text.pos.y > plot.bottom() && text.pos.x < plot.right() =>
                    {
                        Some(text)
                    }
                    _ => None,
                })
                .collect();
            let bolds = labels.iter().any(|text| {
                text.galley
                    .job
                    .sections
                    .iter()
                    .any(|section| section.format.coords != Default::default())
            });
            assert_eq!(bolds, bold, "{characters} characters");
            let borders = painted.iter().any(|shape| matches!(shape, Shape::LineSegment { points, stroke } if stroke.color == egui::Color32::RED && points[0].y == points[1].y && (points[0].y - plot.bottom()).abs() < 1.0 && points[1].x - points[0].x == plot.width()));
            assert_eq!(borders, border);
            let mut centres: Vec<_> = labels
                .iter()
                .map(|text| text.visual_bounding_rect().center().x)
                .collect();
            centres.sort_by(f32::total_cmp);
            assert!(centres.len() >= 2, "{centres:?}");
            let gap = centres
                .windows(2)
                .map(|pair| pair[1] - pair[0])
                .fold(f32::INFINITY, f32::min);
            assert_eq!(gap >= 200.0, characters == 20, "{centres:?}");
        }
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

    /// The crosshair's time label names the bar nearest the pointer, and a chart that follows
    /// another one's index names the same bar. The bars are daily from 2023-11-14, so bar 50 is
    /// 3 Jan 2024 and bar 51 is 4 Jan 2024; 0.7 of a bar past the centre of bar 50 is nearer to 51.
    #[test]
    fn crosshair_time_label_names_the_nearest_bar() {
        use crate::model::{ChartType, ChartTypeParams};
        let mut highs = vec![120.0; 100];
        highs[99] = 3120.0;
        // (type, highs, element index, pointer over chart, synced index, expected label)
        for case in [
            (
                ChartType::Candles,
                &[200.0][..],
                50,
                true,
                None,
                "Jan 04, 2024",
                false,
            ),
            (
                ChartType::Candles,
                &[200.0][..],
                50,
                false,
                Some(50.7),
                "Jan 04, 2024",
                false,
            ),
            #[cfg(feature = "studies")]
            (
                ChartType::Candles,
                &[200.0][..],
                50,
                true,
                None,
                "Jan 04, 2024",
                true,
            ),
            // Bar 99 alone completes 300 bricks, all dated 21 February 2024.
            (
                ChartType::Renko,
                highs.as_slice(),
                250,
                true,
                None,
                "Feb 21, 2024",
                false,
            ),
        ] {
            let (kind, highs, index, pointer, synced, expected, panes) = case;
            let ctx = egui::Context::default();
            let mut chart = chart(&[100.0], highs);
            chart.chart_options.tooltip.enabled = false;
            #[cfg(feature = "studies")]
            if panes {
                chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
                chart.add_indicator(Box::new(crate::studies::MACD::new(12, 26, 9)));
            }
            chart.set_chart_type(kind);
            chart.set_chart_type_params(ChartTypeParams {
                renko_brick_size: 10.0,
                ..Default::default()
            });
            chart.set_synced_crosshair_bar_idx(synced);
            frame(&ctx, &mut chart, 0.0, Vec::new());
            let map = chart.get_chart_mapping();
            let hover = Pos2::new(
                map.idx_to_x(index) + 0.7 * map.bar_spacing,
                map.rect.center().y,
            );
            let events = || {
                pointer
                    .then_some(Event::PointerMoved(hover))
                    .into_iter()
                    .collect()
            };
            frame(&ctx, &mut chart, 0.1, events());
            let painted = shapes(&ctx, &mut chart, 0.2, events());
            let labels: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Text(text) if text.galley.text().ends_with(", 2024") => {
                        if panes {
                            let bottom = chart
                                .get_rendered_indicator_panes()
                                .last()
                                .unwrap()
                                .panel_rect
                                .bottom();
                            assert!(text.pos.y >= bottom, "time tag above lowest pane");
                        }
                        Some(text.galley.text().to_owned())
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(labels, [expected], "{case:?}");
            let time = chart.state.shown()[index + 1].time;
            assert_eq!(chart.format_time(time), expected, "host readout: {case:?}");
        }
    }

    #[test]
    fn initial_axis_passes_keep_zoom_and_later_measurements_keep_range() {
        use crate::model::Instrument;
        let ctx = egui::Context::default();
        let mut chart = chart(&[100.0], &[110.0]);
        chart.config.show_volume = false;
        chart.set_instrument(Some(Instrument::new("TEST").with_price_decimals(2)));
        let initial_view = chart.get_time_scale_state();
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
            ..Default::default()
        };
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(Rect::ZERO),
                ..Default::default()
            },
            |ui| {
                chart.show(ui);
            },
        )
        .drop_without_applying_deltas();
        let mut passes = 0;
        ctx.run_ui(input(), |ui| {
            chart.show(ui);
            passes += 1;
            if ui.ctx().current_pass_index() == 0 {
                // Metadata arriving during startup can widen labels on the discarded pass.
                chart.set_instrument(Some(Instrument::new("TEST").with_price_decimals(6)));
                ui.ctx().request_discard("instrument metadata arrived");
            }
        })
        .drop_without_applying_deltas();
        assert!(passes > 1);
        assert_eq!(chart.get_time_scale_state(), initial_view);

        ctx.run_ui(input(), |ui| {
            chart.show(ui);
        })
        .drop_without_applying_deltas();
        let width = chart.get_rendered_price_rect().width();
        let spacing = chart.state.time_scale().bar_spacing();
        chart.set_instrument(Some(Instrument::new("TEST").with_price_decimals(10)));
        ctx.run_ui(input(), |ui| {
            chart.show(ui);
        })
        .drop_without_applying_deltas();
        let new_width = chart.get_rendered_price_rect().width();
        assert!(new_width < width, "later labels must widen the axis");
        assert!(
            (new_width / chart.state.time_scale().bar_spacing() - width / spacing).abs() < 1e-4
        );
    }

    #[cfg_attr(target_arch = "wasm32", wasm_bindgen_test::wasm_bindgen_test)]
    #[cfg_attr(not(target_arch = "wasm32"), test)]
    fn first_frame_is_settled_and_an_idle_chart_repaints_only_for_a_drawn_countdown() {
        // A host that repaints only on input or new data shows this frame until the next event.
        use crate::model::{Bar, BarData, ChartType};
        use std::time::Duration;
        let cases = [
            (false, true, Duration::MAX, None, ChartType::Candles),
            (true, true, Duration::from_secs(1), None, ChartType::Candles),
            (true, false, Duration::MAX, None, ChartType::Candles),
        ]
        .into_iter()
        .chain([0, 1, 128].into_iter().flat_map(|count| {
            [
                ChartType::Candles,
                ChartType::Line,
                ChartType::Area,
                ChartType::Heikin,
            ]
            .map(move |kind| (false, true, Duration::MAX, Some(count), kind))
        }));
        for (server_time, axis, delay, flat_count, kind) in cases {
            let ctx = egui::Context::default();
            let mut chart = if let Some(count) = flat_count {
                let bars = (0..count)
                    .map(|i| {
                        let time =
                            chrono::DateTime::from_timestamp(1_700_000_000 + i * 60, 0).unwrap();
                        Bar::new(time, 100.0, 100.0, 100.0, 100.0, 0.0)
                    })
                    .collect();
                crate::widget::Chart::new(BarData::from_bars(bars))
            } else {
                chart(&[100.0, 104.0], &[110.0, 108.0])
            };
            chart.set_chart_type(kind);
            chart.set_visible_bars(50);
            chart.set_timeframe(crate::model::Timeframe::Min1);
            chart.config.show_right_axis = axis;
            chart.set_server_time(server_time.then(|| {
                chart.state.data().bars.last().unwrap().time + chrono::Duration::seconds(30)
            }));
            let mut run = || {
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, egui::vec2(800.0, 600.0))),
                    ..Default::default()
                };
                let mut output = ctx.run_ui(input, |ui| {
                    chart.show(ui);
                });
                let shapes = std::mem::take(&mut output.shapes);
                assert!(
                    !shapes.is_empty() && shapes.len() < 2_000,
                    "{flat_count:?} bars, {kind:?}"
                );
                let meshes = ctx.tessellate(shapes.clone(), output.pixels_per_point);
                let mut vertices = 0;
                for primitive in meshes {
                    assert!(primitive.clip_rect.is_finite());
                    if let egui::epaint::Primitive::Mesh(mesh) = primitive.primitive {
                        assert!(mesh.is_valid());
                        vertices += mesh.vertices.len();
                        assert!(
                            mesh.vertices.iter().all(|v| v.pos.is_finite()
                                && v.pos.x.abs() < 1_600.0
                                && v.pos.y.abs() < 1_200.0),
                            "{flat_count:?} bars, {kind:?}: geometry exceeds two screen dimensions"
                        );
                    }
                }
                assert!(
                    vertices > 0 && vertices < 100_000,
                    "{flat_count:?} bars, {kind:?}: {vertices} vertices"
                );
                let delay = output.viewport_output[&egui::ViewportId::ROOT].repaint_delay;
                output.drop_without_applying_deltas();
                (shapes, delay)
            };
            let (first, _) = run();
            for _ in 0..3 {
                let (shapes, actual) = run();
                if flat_count.is_none() {
                    // A drawn countdown changes its text as the clock runs.
                    if delay == Duration::MAX {
                        assert!(
                            shapes == first,
                            "the first frame must draw the settled chart"
                        );
                    }
                    assert_eq!(actual, delay, "server time {server_time}, axis {axis}");
                }
            }
        }
    }

    #[test]
    fn plot_uses_the_interior_and_configured_axis_widths() {
        for (width, height, padding, left, right, studies, gutter) in [
            (800.0, 600.0, 0.0, 0.0, 70.0, 0, 0.0),
            (640.0, 450.0, 2.0, 50.0, 80.0, 0, 0.0),
            (45.0, 20.0, 0.0, 0.0, 70.0, 0, 0.0),
            (800.0, 600.0, 8.0, 0.0, 0.0, 0, 0.0),
            (240.0, 450.0, 0.0, 0.0, 70.0, 3, 85.0),
            (320.0, 450.0, 0.0, 50.0, 70.0, 3, 100.0),
            (800.0, 600.0, 0.0, 0.0, 70.0, 1, 70.0),
        ] {
            let mut chart = chart(&[100.0], &[110.0]);
            for _ in 0..studies {
                chart.add_indicator(Box::new(
                    crate::studies::CustomIndicator::new("Independent", |_| Vec::new())
                        .with_scale(crate::studies::StudyScale::Independent),
                ));
            }
            chart.config.padding = padding;
            chart.config.show_left_axis = left > 0.0;
            chart.config.left_axis_width = left;
            chart.config.show_right_axis = right > 0.0;
            chart.config.right_axis_width = right;
            let widget = Rect::from_min_size(Pos2::new(7.0, 11.0), egui::vec2(width, height));
            let layout = chart.calculate_layout_rects(widget);
            assert!(widget.contains_rect(layout.price_rect));
            assert_eq!(layout.price_rect.top(), widget.top() + padding);
            assert_eq!(
                layout.price_rect.left(),
                widget.left() + (padding + left + gutter).min(width)
            );
            assert_eq!(
                layout.price_rect.width(),
                (width - 2.0 * padding - left - right - gutter).max(0.0)
            );
            assert_eq!(
                layout.price_rect.height(),
                (height - 2.0 * padding - 28.0).max(0.0)
            );
        }
    }

    #[test]
    fn volume_uses_themes_alpha_and_pitch_above_a_configured_baseline() {
        use crate::chart::selection::{ChartElementId, SeriesId};
        use crate::model::ChartType;
        use crate::theme::{Theme, apply_to_egui};
        use egui::Color32;

        // Volume uses the theme's volume colours with and without the volume study.
        // A second opacity checks that the default study inherits the applied theme.
        for (mut theme, alpha, candle_width, kind, height_fraction, inset, expected) in [
            (
                Theme::dark(),
                153,
                0.6,
                ChartType::Candles,
                0.12,
                12.0,
                [[23, 100, 92, 153], [143, 50, 48, 153]],
            ),
            (
                Theme::dark(),
                51,
                0.2,
                ChartType::Heikin,
                0.12,
                12.0,
                [[8, 33, 31, 51], [48, 17, 16, 51]],
            ),
            (
                Theme::light(),
                128,
                0.6,
                ChartType::Line,
                0.25,
                0.0,
                [[19, 83, 77, 128], [120, 42, 40, 128]],
            ),
            (
                Theme::high_contrast(),
                128,
                0.6,
                ChartType::Area,
                0.4,
                6.0,
                [[19, 83, 77, 128], [120, 42, 40, 128]],
            ),
        ] {
            let ctx = egui::Context::default();
            theme.chart.volume_alpha = alpha;
            apply_to_egui(&ctx, &theme);
            let mut chart = chart(&[100.0], &[110.0]);
            // The last visible bar falls, and has half the other bars' volume.
            let mut bars = chart.state.data().bars.clone();
            let last = bars.last_mut().unwrap();
            last.open = 110.0;
            last.close = 100.0;
            last.volume = 0.5;
            chart.set_data(crate::chart::selection::SeriesId::MAIN, bars);
            chart.config.candle_width = candle_width;
            chart.config.volume_height_fraction = height_fraction;
            chart.config.volume_baseline_inset = inset;
            chart.set_chart_type(kind);
            chart
                .selection
                .select(ChartElementId::Series(SeriesId::VOLUME), None);
            let painted = shapes(&ctx, &mut chart, 0.0, Vec::new());
            let price = chart.last_rendered_price_rect;
            let volume = chart.last_rendered_volume_rect;
            let layout = chart.calculate_layout_rects(egui::Rect::from_min_max(
                Pos2::new(8.0, 8.0),
                Pos2::new(792.0, 592.0),
            ));
            assert_eq!(layout.price_rect, price);
            assert_eq!(layout.volume_rect, volume);
            assert!(
                (volume.height() - (price.height() - inset) * height_fraction).abs() < 0.001,
                "{volume:?} in {price:?}"
            );
            assert_eq!(volume.bottom(), price.bottom() - inset);
            let dots: Vec<_> = painted
                .iter()
                .filter_map(|shape| match shape {
                    Shape::Circle(dot) if dot.radius == 4.5 => Some(dot.center),
                    _ => None,
                })
                .collect();
            assert!(!dots.is_empty(), "missing volume selection handles");
            assert!(dots.iter().all(|dot| (dot.y - volume.top()).abs() < 0.001));
            let pitch = chart.state.time_scale().bar_spacing();
            for (colour, fraction) in expected.into_iter().zip([1.0, 0.5]) {
                let colour =
                    Color32::from_rgba_premultiplied(colour[0], colour[1], colour[2], colour[3]);
                let bars: Vec<_> = painted
                    .iter()
                    .filter_map(|shape| match shape {
                        Shape::Rect(rect) if rect.fill == colour => Some(rect.rect),
                        _ => None,
                    })
                    .collect();
                assert!(!bars.is_empty(), "alpha {alpha}: missing volume {colour:?}");
                for bar in bars {
                    assert!(
                        (bar.height() - volume.height() * fraction).abs() < 0.001,
                        "{bar:?}"
                    );
                    assert_eq!(bar.bottom(), volume.bottom());
                    assert!(
                        (bar.width() - pitch * 0.8).abs() < 0.001,
                        "{bar:?} at pitch {pitch}"
                    );
                }
            }
        }
    }

    fn navigation_pos(chart: &crate::widget::Chart, button: usize) -> Pos2 {
        let rect = chart
            .last_rendered_price_rect
            .union(chart.last_rendered_volume_rect);
        Pos2::new(
            rect.center().x - 112.0 + 32.0 * button as f32,
            rect.bottom() - 26.0,
        )
    }

    fn navigation_click(ctx: &egui::Context, chart: &mut crate::widget::Chart, button: usize) {
        let pos = navigation_pos(chart, button);
        for (i, events) in [
            vec![Event::PointerMoved(pos)],
            vec![navigation_press(pos, true)],
            vec![navigation_press(pos, false)],
            Vec::new(),
        ]
        .into_iter()
        .enumerate()
        {
            frame(ctx, chart, 1.0 + i as f64 * 0.02, events);
        }
    }

    fn navigation_press(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    /// On the 714 px time scale, spacing 20 shows ceil(714/20 - 1) = 35 bars.
    /// A zoom step of 0.2 adds +/-0.2 * 20/10 to spacing and keeps the right edge
    /// (offset -20) fixed; reset restores spacing 6 and the configured 60 px / 6 = 10 bar offset.
    #[test]
    fn navigation_buttons_apply_each_action() {
        use crate::scales::PriceScaleMode::{Logarithmic, Normal, Percentage};
        // (button, initial manual/mode, expected spacing/offset/manual/mode)
        for (button, initial_manual, initial_mode, spacing, offset, manual, mode) in [
            (0, false, Normal, 19.6, -20.0, false, Normal),
            (1, false, Normal, 20.4, -20.0, false, Normal),
            (2, false, Normal, 20.0, -55.0, false, Normal),
            (3, false, Normal, 20.0, 15.0, false, Normal),
            (4, true, Normal, 6.0, 10.0, false, Normal),
            (5, false, Normal, 20.0, -20.0, true, Normal),
            (5, true, Normal, 20.0, -20.0, false, Normal),
            (6, false, Normal, 20.0, -20.0, false, Logarithmic),
            (6, true, Logarithmic, 20.0, -20.0, false, Normal),
            (7, false, Normal, 20.0, -20.0, false, Percentage),
            (7, true, Percentage, 20.0, -20.0, false, Normal),
        ] {
            let ctx = egui::Context::default();
            let mut chart =
                chart(&[100.0], &[200.0]).with_chart_options(crate::config::ChartOptions {
                    time_scale: crate::config::TimeScaleOptions {
                        right_offset_pixels: Some(60.0),
                        ..Default::default()
                    },
                    ..Default::default()
                });
            chart.config.navigation_buttons = crate::config::NavigationButtons::OnHover;
            frame(&ctx, &mut chart, 0.0, Vec::new());
            assert_eq!(chart.state.time_scale().width(), 714.0);
            chart.state.time_scale_mut().set_bar_spacing(20.0);
            chart.state.time_scale_mut().set_right_offset(-20.0);
            chart.apply_series_settings(&SeriesSettings {
                price_scale_mode: initial_mode,
                ..Default::default()
            });
            if initial_manual {
                chart.state.price_scale_mut().set_range(90.0, 210.0);
            }
            frame(&ctx, &mut chart, 0.1, Vec::new());
            navigation_click(&ctx, &mut chart, button);
            let ts = chart.state.time_scale();
            assert!(
                (ts.bar_spacing() - spacing).abs() < 1e-4,
                "button {button}: spacing {}",
                ts.bar_spacing()
            );
            assert!(
                (f64::from(ts.right_offset()) - offset).abs() < 1e-4,
                "button {button}: offset {}",
                ts.right_offset()
            );
            assert_eq!(
                chart.state.price_scale().is_manual(),
                manual,
                "button {button}"
            );
            assert_eq!(
                chart.state.price_scale().options().mode,
                mode,
                "button {button}"
            );
            assert!(chart.undo(), "button {button}: navigation edit is undoable");
            assert_eq!(chart.get_time_scale_state(), (20.0, -20.0));
            assert_eq!(chart.state.price_scale().is_manual(), initial_manual);
            assert_eq!(chart.state.price_scale().options().mode, initial_mode);
            assert!(chart.redo());
            assert!((chart.get_time_scale_state().0 - spacing).abs() < 1e-4);
            assert!((f64::from(chart.get_time_scale_state().1) - offset).abs() < 1e-4);
            assert_eq!(chart.state.price_scale().is_manual(), manual);
            assert_eq!(chart.state.price_scale().options().mode, mode);
        }
    }

    /// The selected fill follows price-axis scaling/reset and series settings on the next frame,
    /// and the row shows while hovering the main pane (volume included) per the visibility setting.
    #[test]
    fn navigation_buttons_show_visibility_and_scale_state() {
        use crate::config::NavigationButtons::{Always, Never, OnHover};
        use crate::scales::PriceScaleMode::{Logarithmic, Normal, Percentage};
        let ctx = egui::Context::default();
        let mut chart = chart(&[100.0], &[200.0]);
        assert_eq!(chart.config.navigation_buttons, Never);
        #[cfg(feature = "ui")]
        assert_eq!(
            crate::widget::settings::ButtonsSettings::default().navigation,
            crate::widget::settings::ButtonVisibility::Hidden,
        );
        frame(&ctx, &mut chart, 0.0, Vec::new());
        for (i, (manual, mode, visibility, hover, selected)) in [
            (false, Normal, OnHover, true, [true, false, false]),
            (true, Normal, OnHover, true, [false, false, false]),
            (false, Logarithmic, OnHover, true, [true, true, false]),
            (false, Percentage, OnHover, true, [true, false, true]),
            (false, Normal, OnHover, false, [false, false, false]),
            (false, Normal, Always, false, [true, false, false]),
            (false, Normal, Never, true, [false, false, false]),
        ]
        .into_iter()
        .enumerate()
        {
            chart.apply_series_settings(&SeriesSettings {
                price_scale_mode: mode,
                ..Default::default()
            });
            if manual {
                chart.state.price_scale_mut().set_range(90.0, 210.0);
            } else {
                chart.state.price_scale_mut().reset();
            }
            chart.config.navigation_buttons = visibility;
            let pointer = if hover {
                chart.last_rendered_volume_rect.center()
            } else {
                chart.last_rendered_price_rect.right_top() + egui::vec2(20.0, 20.0)
            };
            let painted = shapes(
                &ctx,
                &mut chart,
                0.1 + i as f64,
                vec![Event::PointerMoved(pointer)],
            );
            for (toggle, expected) in selected.into_iter().enumerate() {
                let center = navigation_pos(&chart, toggle + 5);
                let active = painted.iter().any(|shape| matches!(shape, Shape::Rect(r)
                    if r.rect.center().distance(center) < 0.1 && r.fill == ctx.global_style().visuals.selection.bg_fill));
                assert_eq!(
                    active, expected,
                    "manual {manual}, mode {mode:?}, toggle {toggle}"
                );
            }
        }
        chart.config.show_scale_mode_buttons = false;
        chart.config.navigation_buttons = Always;
        let painted = shapes(&ctx, &mut chart, 20.0, Vec::new());
        assert!(!painted.iter().any(|shape| matches!(shape, Shape::Text(text) if ["A", "log", "%"].contains(&text.galley.text()))));
        ctx.run_ui(Default::default(), |ui| {
            let plot = chart.last_rendered_price_rect;
            let nav = chart.navigation_rect(ui, plot).unwrap();
            let buttons =
                chart.interact_navigation(ui, nav, plot, egui::Id::unique("hidden_navigation"));
            assert!(
                buttons[5..]
                    .iter()
                    .all(|button| !button.sense.is_focusable()),
                "hidden scale controls must not receive keyboard focus"
            );
        })
        .drop_without_applying_deltas();
    }

    /// A drag starting on a button belongs to that button even after leaving the row.
    /// Clicking it also preserves a series selection while a drawing tool is active.
    #[test]
    #[cfg(feature = "drawings")]
    fn navigation_buttons_do_not_start_chart_gestures() {
        use crate::drawings::DrawingToolType;
        use crate::widget::{ChartElementId, SeriesId};
        for (tool, box_zoom) in [
            (None, false),
            (Some(DrawingToolType::TrendLine), false),
            (Some(DrawingToolType::Rect), false),
            (None, true),
        ] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[100.0], &[200.0]);
            chart.config.navigation_buttons = crate::config::NavigationButtons::OnHover;
            frame(&ctx, &mut chart, 0.0, Vec::new());
            chart.state.time_scale_mut().set_bar_spacing(20.0);
            chart.state.time_scale_mut().set_right_offset(-20.0);
            chart.drawings.active_tool = tool;
            chart.zoom_mode_active = box_zoom;
            chart
                .selection
                .select(ChartElementId::Series(SeriesId::MAIN), Some(50));
            let start = navigation_pos(&chart, 5);
            let end = start + egui::vec2(-100.0, -60.0);
            for (i, events) in [
                vec![Event::PointerMoved(start)],
                vec![navigation_press(start, true)],
                vec![Event::PointerMoved(end)],
                vec![navigation_press(end, false)],
            ]
            .into_iter()
            .enumerate()
            {
                frame(&ctx, &mut chart, 0.1 + i as f64 * 0.02, events);
                assert_eq!(
                    chart.state.time_scale().right_offset(),
                    -20.0,
                    "{tool:?}, box zoom {box_zoom}"
                );
                assert!(chart.scroll_start_pos.is_none());
                assert!(!chart.box_zoom.active);
                assert!(chart.drawings.curr_drawing.is_none());
                assert!(chart.drawings.drawings.is_empty());
            }
            navigation_click(&ctx, &mut chart, 5);
            assert_eq!(chart.state.time_scale().right_offset(), -20.0);
            assert!(chart.scroll_start_pos.is_none());
            assert_eq!(
                chart.selected_element(),
                Some(ChartElementId::Series(SeriesId::MAIN))
            );
            assert!(chart.drawings.curr_drawing.is_none());
        }
    }

    /// Session volume profiles sum whole sessions, from the first bar of a session that is only
    /// partly shown up to the open of the next one, and the last session holds the live bar.
    #[cfg(feature = "studies")]
    #[test]
    fn session_ranges_cover_the_whole_sessions_shown() {
        use crate::model::{Bar, BarClock, Timeframe};
        let open = chrono::DateTime::parse_from_rfc3339("2026-01-05T20:00:00Z")
            .unwrap()
            .to_utc();
        let hour = |h| open + chrono::TimeDelta::hours(h);
        let bars: Vec<_> = (0..9)
            .map(|h| Bar::new(hour(h), 1.0, 1.0, 1.0, 1.0, 1.0))
            .collect();
        let clock = BarClock::new(Timeframe::Hour1);
        let max = chrono::DateTime::<chrono::Utc>::MAX_UTC;
        for (shown, expected) in [
            (2..5, Some(vec![hour(0)..hour(4), hour(4)..max])),
            (0..2, Some(vec![hour(0)..hour(4)])),
            (6..6, None),
        ] {
            let ranges = super::session_ranges(&bars, shown.clone(), &clock);
            let ranges = ranges.map(|ranges| ranges.ranges().to_vec());
            assert_eq!(ranges, expected, "{shown:?}");
        }
    }
}
