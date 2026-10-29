//! Chart data export and renderer-independent image frames.

use super::Chart;
use crate::localization as l10n;
use crate::scales::StudyFormat;
use crate::theme::ThemeExt;
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;

/// Image options shared by the export dialog and offscreen rendering.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub struct ImageExportOptions {
    /// Output width and height in pixels.
    pub size: [u32; 2],
    /// Include the symbol above the chart.
    pub include_title: bool,
    /// Include the export time in the chart's time zone above the chart.
    pub include_timestamp: bool,
    /// Draw the symbol watermark and the one set with [`Chart::set_watermark`], as they show on
    /// screen, independently of the title and timestamp.
    pub include_watermark: bool,
    /// Omit the chart and header backgrounds, retaining plot and drawing colours.
    pub transparent_background: bool,
    /// Draw the crosshair where the pointer last placed it over the plots, or where
    /// [`Chart::set_crosshair_position`] or a synchronized chart placed it since.
    pub include_crosshair: bool,
    /// Pixels of the textures the host binds itself, by the id its images use, so
    /// [`Chart::export_png`] and [`Chart::export_svg`] can draw them. Without an entry, an image
    /// of such a texture fails the export. Supplied textures use linear filtering and clamp to
    /// their edges. Prefer [`egui::TextureId::User`]: ids that collide with the offscreen frame's
    /// own textures return an error. [`ChartImage`] frames include the supplied pixels too.
    pub textures: HashMap<egui::TextureId, Arc<egui::ColorImage>>,
}

impl Default for ImageExportOptions {
    fn default() -> Self {
        Self {
            size: [1440, 900],
            include_title: true,
            include_timestamp: true,
            include_watermark: true,
            transparent_background: false,
            include_crosshair: false,
            textures: HashMap::new(),
        }
    }
}

/// A complete offscreen frame for an egui renderer, at one pixel per point.
/// Upload `textures_delta.set`, draw `primitives` over transparent black at `size`, then
/// release the textures. The host owns GPU readback, PNG encoding and file/clipboard delivery.
/// This uses its own texture namespace; render with a dedicated egui renderer. Hosts must also
/// bind any user textures referenced by their image sources whose pixels were not supplied in
/// [`ImageExportOptions::textures`] before drawing.
#[must_use = "Render the frame and apply its texture deltas, or clear them before dropping"]
#[non_exhaustive]
pub struct ChartImage {
    /// Exact output dimensions in pixels.
    pub size: [u32; 2],
    /// Tessellated chart, header, study panes and requested layout widgets.
    pub primitives: Vec<egui::ClippedPrimitive>,
    /// Textures referenced by the primitives.
    pub textures_delta: egui::TexturesDelta,
}

/// The loaded bars and visible study plots as text, aligned by chart time: what
/// [`Chart::export_csv`] writes and the chart data table lists.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct DataTable {
    /// Column titles: time, open, high, low, close and volume, then one per visible study plot.
    pub headers: Vec<String>,
    /// One row per loaded bar, oldest first. A missing or non-finite value is an empty cell.
    pub rows: Vec<Vec<String>>,
}

impl DataTable {
    /// CSV text of the `columns` (indices into [`Self::headers`]) in the given order.
    pub fn csv(
        &self,
        columns: &[usize],
    ) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let mut writer = csv::Writer::from_writer(Vec::new());
        writer.write_record(columns.iter().map(|&column| &self.headers[column]))?;
        for row in &self.rows {
            writer.write_record(columns.iter().map(|&column| &row[column]))?;
        }
        Ok(String::from_utf8(writer.into_inner()?)?)
    }

    /// Tab-separated text of the `columns` with a header line, for pasting into a spreadsheet.
    pub fn tsv(&self, columns: &[usize]) -> String {
        let line = |cells: &mut dyn Iterator<Item = &String>| {
            cells.map(String::as_str).collect::<Vec<_>>().join("\t")
        };
        let mut lines = vec![line(
            &mut columns.iter().map(|&column| &self.headers[column]),
        )];
        lines.extend(
            self.rows
                .iter()
                .map(|row| line(&mut columns.iter().map(|&column| &row[column]))),
        );
        lines.join("\n")
    }
}

/// Sets the host's textures in `delta` beside the frame's own, for the encoders that draw them.
fn add_host_textures(
    delta: &mut egui::TexturesDelta,
    options: &ImageExportOptions,
) -> Result<(), String> {
    for (&id, image) in &options.textures {
        if delta.set.contains_key(&id) {
            return Err(format!(
                "Host texture {id:?} conflicts with an offscreen texture; use TextureId::User"
            ));
        }
        let [width, height] = image.size;
        if width == 0
            || height == 0
            || width > u32::MAX as usize
            || height > u32::MAX as usize
            || width.checked_mul(height) != Some(image.pixels.len())
        {
            return Err(format!(
                "Host texture {id:?} has invalid dimensions or pixels"
            ));
        }
        delta.push(
            id,
            egui::epaint::ImageDelta::full(
                egui::ImageData::Color(Arc::clone(image)),
                egui::TextureOptions::LINEAR,
            ),
        );
    }
    Ok(())
}

impl Chart {
    fn export_time(&self, time: DateTime<Utc>) -> String {
        match self.display_timezone() {
            crate::scales::TimezoneMode::Local => time.with_timezone(&chrono::Local).to_rfc3339(),
            zone => time
                .with_timezone(&zone.to_tz().unwrap_or(crate::model::TimezoneId::utc()))
                .to_rfc3339(),
        }
    }

    /// Exports all loaded source bars and visible study plots as UTF-8 CSV.
    /// Recalculates copies of the studies so streaming updates and pending refreshes are included
    /// without altering the live chart. Price-based charts export source-bar calculations, aligned
    /// by source index and plot displacement, rather than mixing element values with source bars.
    /// Times include the chart's UTC offset, including DST changes and subsecond timestamps; on a
    /// [horizontal scale](Self::set_horizontal_scale) other than time, keys read as the crosshair
    /// label does.
    /// Prices inherit instrument precision (two decimals without metadata); study overrides and
    /// non-price units retain their precision. Missing/non-finite values are empty cells.
    pub fn export_csv(&self) -> Result<String, Box<dyn std::error::Error + Send + Sync>> {
        let table = self.data_table();
        table.csv(&(0..table.headers.len()).collect::<Vec<_>>())
    }

    /// The cells of [`Self::export_csv`] as a table, with the same bars, study plots, times and
    /// precision. During replay it holds the bars the replay cutoff has completed, and on
    /// price-based chart types still the source bars. Loads nothing: only loaded bars are listed.
    pub fn data_table(&self) -> DataTable {
        let bars = &self.state.data().bars;
        let price_precision = self
            .instrument()
            .and_then(crate::scales::instrument_price_precision)
            .map_or(2, usize::from);
        let mut studies: Vec<_> = self
            .indicators()
            .map(|(_, study)| study.clone_box())
            .collect();
        let mut columns = Vec::new();
        let mut header = vec![
            "time".into(),
            "open".into(),
            "high".into(),
            "low".into(),
            "close".into(),
            "volume".into(),
        ];
        let clock = self.bar_clock();
        for study in &mut studies {
            study.set_instrument(self.instrument());
            if let Some(timeframe) = self.timeframe {
                study.set_timeframe(timeframe);
            }
            study.set_bar_clock(&clock);
        }
        let requests = studies
            .iter()
            .flat_map(|study| study.data_requests())
            .collect();
        let sources = self.study_data.data(
            requests,
            &self.symbol,
            self.timeframe,
            self.replay.as_ref().map(|replay| replay.time),
        );
        super::studies::calculate(&mut studies, &self.study_ids, bars, &sources);
        for (index, study) in studies
            .iter_mut()
            .enumerate()
            .filter(|(_, study)| study.is_visible())
        {
            let names = study.line_names();
            for line in (0..study.line_count()).filter(|line| study.plot_style(*line).visible) {
                header.push(format!(
                    "{}: {}",
                    study.name(),
                    names.get(line).map_or("", String::as_str)
                ));
                let precision = study.precision().unwrap_or(match study.value_format() {
                    StudyFormat::Price => price_precision,
                    StudyFormat::Fixed(decimals) => decimals.min(12),
                    _ => 2,
                });
                columns.push((index, line, precision));
            }
        }
        let number = |value: f64, precision: usize| {
            if value.is_finite() {
                format!("{value:.precision$}")
            } else {
                String::new()
            }
        };
        let mut rows = Vec::with_capacity(bars.len());
        for (index, bar) in bars.iter().enumerate() {
            let mut row = vec![if self.horizontal_scale.is_some() {
                self.format_time(bar.time)
            } else {
                self.export_time(bar.time)
            }];
            row.extend(
                [bar.open, bar.high, bar.low, bar.close]
                    .map(|value| number(value, price_precision)),
            );
            row.push(if bar.volume.is_finite() {
                bar.volume.to_string()
            } else {
                String::new()
            });
            row.extend(columns.iter().map(|&(study, line, precision)| {
                studies[study]
                    .plot_value(line, index)
                    .map_or_else(String::new, |value| number(value, precision))
            }));
            rows.push(row);
        }
        DataTable {
            headers: header,
            rows,
        }
    }

    /// Renders an isolated image frame using the same chart, studies, primitives, drawings and
    /// scales as the widget. Does not poll feeds, change live viewport state, call the lifecycle
    /// hooks of primitives or capture host panels/dialogs.
    /// `ctx` supplies the host's fonts and style after its first frame. Images use one pixel per point and dimensions
    /// between 100 and 7680 on each axis, with at most 33,177,600 pixels.
    /// Pass the returned frame to an egui renderer; see [`ChartImage`] for delivery responsibilities.
    pub fn export_image(
        &self,
        ctx: &egui::Context,
        options: &ImageExportOptions,
    ) -> Result<ChartImage, String> {
        self.export_layout_image(ctx, options, |ui, chart| {
            chart.show(ui);
        })
    }

    /// Like [`Self::export_image`] for a layout of library widgets: `layout` draws everything
    /// inside the image, the toolbars, panels and trading overlays around the chart, and shows the
    /// chart it is given, a frozen copy of this one, with [`Chart::show`] or
    /// [`Chart::show_footprint`]. It runs once for each of several layout passes, on a context
    /// of its own with no pointer, so its widgets show as at rest. The title and timestamp, if
    /// requested, sit above it.
    ///
    /// ```no_run
    /// # fn capture(chart: &egui_charts::Chart, ctx: &egui::Context) -> Result<(), String> {
    /// let options = egui_charts::widget::ImageExportOptions::default();
    /// let png = chart.export_layout_png(ctx, &options, |ui, snapshot| {
    ///     ui.horizontal(|ui| {
    ///         ui.label(snapshot.symbol());
    ///         ui.label("1m");
    ///     });
    ///     snapshot.show(ui);
    /// })?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn export_layout_image(
        &self,
        ctx: &egui::Context,
        options: &ImageExportOptions,
        layout: impl FnMut(&mut egui::Ui, &mut Chart),
    ) -> Result<ChartImage, String> {
        let (shapes, textures_delta, image_ctx) = self.export_shapes(ctx, options, layout)?;
        Ok(ChartImage {
            size: options.size,
            primitives: image_ctx.tessellate(shapes, 1.0),
            textures_delta,
        })
    }

    /// The shapes of the last pass of the offscreen frame, the textures it sets and the context
    /// whose fonts laid it out.
    fn export_shapes(
        &self,
        ctx: &egui::Context,
        options: &ImageExportOptions,
        mut layout: impl FnMut(&mut egui::Ui, &mut Chart),
    ) -> Result<
        (
            Vec<egui::epaint::ClippedShape>,
            egui::TexturesDelta,
            egui::Context,
        ),
        String,
    > {
        let size = options.size;
        if size.iter().any(|&side| !(100..=7680).contains(&side))
            || u64::from(size[0]) * u64::from(size[1]) > 33_177_600
        {
            return Err(
                "Image size must be 100-7680 pixels per side and at most 33,177,600 pixels".into(),
            );
        }
        if ctx.cumulative_pass_nr() == 0 {
            return Err("Run the egui context's first frame before exporting an image".into());
        }
        let mut chart = self.export_snapshot();
        chart.config.skip_background = options.transparent_background;
        chart.config.show_symbol_labels &= options.include_title;
        if !options.include_watermark {
            chart.config.show_watermark = false;
            chart.watermark = None;
        }
        let mut theme = self
            .theme_override
            .clone()
            .unwrap_or_else(|| ctx.active_theme());
        if options.transparent_background {
            theme.chart.bg = egui::Color32::TRANSPARENT;
        }
        chart.theme_override = Some(theme.clone());
        if let Some((time, price)) = self.last_crosshair.filter(|_| options.include_crosshair) {
            match price {
                Some(price) => chart.set_crosshair_position(Some((time, price))),
                None => {
                    chart.set_synced_crosshair_time(Some(time));
                    chart.chart_options.crosshair.horz_line_visible = false;
                }
            }
        }
        let image_ctx = egui::Context::default();
        image_ctx.set_global_style(ctx.global_style());
        crate::theme::apply_to_egui(&image_ctx, &theme);
        image_ctx.set_fonts(ctx.fonts(|fonts| fonts.definitions().clone()));
        let timestamp = options
            .include_timestamp
            .then(|| self.export_time(Utc::now()));
        let title = options.include_title.then(|| self.symbol().to_owned());
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(size[0] as f32, size[1] as f32),
            )),
            ..Default::default()
        };
        let mut textures_delta = egui::TexturesDelta::default();
        // Resolve pane layout, axis measurement and each pending element study before the final frame.
        let passes = chart.studies.indicators().len() + 3;
        let mut shapes = Vec::new();
        for pass in 0..passes {
            let output = image_ctx.run_ui(
                egui::RawInput {
                    time: Some(pass as f64 * 3600.0),
                    ..input.clone()
                },
                |ui| {
                    if !options.transparent_background {
                        ui.painter()
                            .rect_filled(ui.max_rect(), 0.0, chart.config.background_color);
                    }
                    ui.set_min_size(ui.available_size());
                    if title.is_some() || timestamp.is_some() {
                        ui.horizontal(|ui| {
                            if let Some(title) = &title {
                                ui.label(l10n::text(ui.ctx(), title));
                            }
                            if let Some(timestamp) = &timestamp {
                                ui.label(timestamp);
                            }
                        });
                    }
                    layout(ui, &mut chart);
                },
            );
            textures_delta.append(output.textures_delta);
            shapes = output.shapes;
        }
        if let Err(error) = add_host_textures(&mut textures_delta, options) {
            textures_delta.clear();
            return Err(error);
        }
        Ok((shapes, textures_delta, image_ctx))
    }

    /// Renders the image of [`Self::export_image`] on the CPU and encodes it as PNG, so the host
    /// needs no renderer or GPU readback. Fails with the reason for content it cannot draw, host
    /// paint callbacks and images of textures the host binds itself, such as a legend badge from a
    /// user texture, unless [`ImageExportOptions::textures`] holds their pixels, instead of
    /// leaving it out.
    pub fn export_png(
        &self,
        ctx: &egui::Context,
        options: &ImageExportOptions,
    ) -> Result<Vec<u8>, String> {
        self.export_layout_png(ctx, options, |ui, chart| {
            chart.show(ui);
        })
    }

    /// Like [`Self::export_png`] for a layout of library widgets, drawn by `layout` as
    /// [`Self::export_layout_image`] describes.
    pub fn export_layout_png(
        &self,
        ctx: &egui::Context,
        options: &ImageExportOptions,
        layout: impl FnMut(&mut egui::Ui, &mut Chart),
    ) -> Result<Vec<u8>, String> {
        let mut image = self.export_layout_image(ctx, options, layout)?;
        let pixels = super::raster::rasterize(image.size, &image.primitives, &image.textures_delta);
        image.textures_delta.clear();
        super::raster::png(image.size, &pixels?)
    }

    /// Renders the frame of [`Self::export_image`] as an SVG document of the options' size, with
    /// the same title, timestamp, watermark and background. Text stays text, in the font family
    /// class of the on-screen font, and the frame's textures, such as legend icons, are embedded
    /// as PNG, with the host's [`ImageExportOptions::textures`]. Fails with the reason for content
    /// SVG cannot take over unchanged, such as host paint callbacks and textures the host binds
    /// itself without supplying their pixels, instead of leaving it out.
    pub fn export_svg(
        &self,
        ctx: &egui::Context,
        options: &ImageExportOptions,
    ) -> Result<String, String> {
        self.export_layout_svg(ctx, options, |ui, chart| {
            chart.show(ui);
        })
    }

    /// Like [`Self::export_svg`] for a layout of library widgets, drawn by `layout` as
    /// [`Self::export_layout_image`] describes.
    pub fn export_layout_svg(
        &self,
        ctx: &egui::Context,
        options: &ImageExportOptions,
        layout: impl FnMut(&mut egui::Ui, &mut Chart),
    ) -> Result<String, String> {
        let (shapes, mut textures_delta, _) = self.export_shapes(ctx, options, layout)?;
        let svg = super::svg::svg(options.size, &shapes, &textures_delta);
        textures_delta.clear();
        svg
    }

    fn export_snapshot(&self) -> Self {
        let mut chart = Chart::with_config(Default::default(), self.config.clone());
        chart.state = self.state.clone();
        chart.chart_options = self.chart_options.clone();
        chart
            .chart_options
            .time_scale
            .lock_visible_time_range_on_resize = true;
        chart.prev_width = self.prev_width;
        chart.chart_options.time_scale.timezone = self.display_timezone();
        chart.chart_type = self.chart_type;
        chart.chart_type_params = self.chart_type_params.clone();
        chart.pending_box_sizes = self.pending_box_sizes;
        chart.main_series_visible = self.main_series_visible;
        chart.main_series_present = self.main_series_present;
        chart.main_series_scale = self.main_series_scale;
        chart.price_series = self
            .price_series
            .iter()
            .map(|series| super::price_series::PriceSeries {
                id: series.id,
                options: series.options.clone(),
                visible: series.visible,
                bars: series.bars.clone(),
                revision: series.revision,
                heikin_ashi: Default::default(),
                feed: series.feed.clone(),
                markers: series.markers.clone(),
                marker_options: series.marker_options,
                change_marks: None,
                lines: series.lines.clone(),
            })
            .collect();
        chart.series_scales = self.series_scales.clone();
        chart.symbol = self.symbol.clone();
        chart.timeframe = self.timeframe;
        chart.feed_link = self.feed_link.as_ref().map(super::feed::FeedLink::frozen);
        chart.legend_instrument = self.instrument().cloned();
        chart.legend_badge = self.legend_badge.clone();
        chart.watermark = self.watermark.clone();
        chart.legend_statuses = self.legend_statuses.clone();
        chart.quote = self.quote.clone();
        chart.session_quote = self.session_quote.clone();
        chart.session_provider = self.session_provider.clone();
        chart.horizontal_scale = self.horizontal_scale.clone();
        chart.locale = self.locale.clone();
        chart.date_format = self.date_format.clone();
        chart.time_formatter = self.time_formatter.clone();
        chart.price_formatter = self.price_formatter.clone();
        chart.percentage_formatter = self.percentage_formatter.clone();
        chart.volume_formatter = self.volume_formatter.clone();
        chart.series_formatters = self.series_formatters.clone();
        chart.extended_hours = self.extended_hours;
        chart.theme_override = self.theme_override.clone();
        chart.inherited_theme = self.inherited_theme.clone();
        #[cfg(feature = "drawings")]
        {
            chart.drawings.registry = self.drawings.registry.clone();
            chart.drawings.drawings = self.drawings.drawings.clone();
            chart.drawings.curr_timeframe = self.drawings.curr_timeframe.clone();
        }
        chart.marks = self.marks.clone();
        chart.marker_options = self.marker_options;
        chart.price_lines = self.price_lines.clone();
        chart.events = self.events.clone();
        chart.time_scale_events = self.time_scale_events.clone();
        chart.event_visibility = self.event_visibility.clone();
        // Freeze loaded source marks without connecting the offscreen chart to the host.
        if let Some(source) = &self.mark_source {
            chart.marks.extend_from_slice(&source.page.bar_marks);
            chart
                .events
                .extend_from_slice(&source.page.time_scale_marks);
            chart
                .time_scale_events
                .extend_from_slice(&source.page.events);
        }
        chart.desired_visible_bars = self.desired_visible_bars;
        chart.apply_visible_bars_once = self.apply_visible_bars_once;
        chart.start_idx = self.start_idx;
        for (_, study) in self.indicators() {
            chart.add_indicator(study.clone_box());
        }
        chart.study_ids = self.study_ids.clone();
        chart.study_inputs = self.study_inputs.clone();
        chart.study_autoscale = self.study_autoscale.clone();
        chart.next_series = self.next_series;
        // Copied after the studies, replacing the panes that adding them created.
        chart.pane_layout = self.pane_layout.clone();
        chart.pane_options = self.pane_options;
        #[cfg(feature = "studies")]
        {
            chart.volume_study = self.volume_study;
            chart.volume_setting = self.volume_setting;
        }
        chart.primitives = self.primitives.clone();
        chart.next_primitive = self.next_primitive;
        chart.settle_primitives();
        chart.study_data = self.study_data.snapshot();
        chart.recalculate_studies();
        chart
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Bar, BarData, Instrument, Timeframe};
    use crate::studies::{CustomIndicator, IndicatorValue};
    use crate::widget::SeriesId;

    #[test]
    fn csv_aligns_fresh_visible_plots_and_formats_chart_times_and_prices() {
        let times: Vec<_> = [
            "2024-11-03T05:30:00.125Z",
            "2024-11-03T06:30:00.125Z",
            "2024-11-03T07:30:00.125Z",
        ]
        .map(|time| DateTime::parse_from_rfc3339(time).unwrap().to_utc())
        .into();
        let zones = [
            (
                crate::scales::TimezoneMode::Utc,
                [
                    "2024-11-03T05:30:00.125+00:00",
                    "2024-11-03T06:30:00.125+00:00",
                ],
            ),
            #[cfg(feature = "timezones")]
            (
                crate::scales::TimezoneMode::Timezone("America/New_York".parse().unwrap()),
                [
                    "2024-11-03T01:30:00.125-04:00",
                    "2024-11-03T01:30:00.125-05:00",
                ],
            ),
            #[cfg(feature = "timezones")]
            (
                crate::scales::TimezoneMode::Exchange(crate::model::TimezoneId::utc()),
                [
                    "2024-11-03T01:30:00.125-04:00",
                    "2024-11-03T01:30:00.125-05:00",
                ],
            ),
        ];
        for (zone, expected_times) in zones {
            let bars: Vec<_> = times
                .iter()
                .zip([10.0, 14.0, 18.0])
                .map(|(&time, close)| {
                    Bar::new(time, close, close + 1.0, close - 1.0, close, 1234.5)
                })
                .collect();
            let mut chart = Chart::new(BarData::from_bars(bars.clone()));
            chart.set_timeframe(Timeframe::Hour1);
            chart.chart_options.time_scale.timezone = zone;
            let mut instrument = Instrument::new("TEST");
            instrument.price_decimals = Some(2);
            instrument.time_zone = crate::model::TimezoneId::named("America/New_York");
            chart.set_instrument(Some(instrument.clone()));
            let study = chart.add_indicator(Box::new(
                CustomIndicator::new("Average, pair", |bars| {
                    bars.iter()
                        .enumerate()
                        .map(|(index, bar)| {
                            let mean = if index == 0 {
                                f64::NAN
                            } else {
                                (bars[index - 1].close + bar.close) / 2.0
                            };
                            IndicatorValue::Multiple(vec![mean, mean, bar.close * 2.0])
                        })
                        .collect()
                })
                .with_line_count(3)
                .with_line_names(vec!["mean".into(), "shifted".into(), "hidden".into()])
                .with_offsets(vec![0, -1, 0]),
            ));
            let settings_study = chart.indicator_mut(study).unwrap();
            let mut settings = settings_study.settings();
            settings.plots[2].visible = false;
            settings_study.apply_settings(&[], settings).unwrap();
            let hidden = chart
                .add_indicator(Box::new(CustomIndicator::new("Hidden study", |bars| {
                    vec![IndicatorValue::Single(999.0); bars.len()]
                })));
            chart.set_indicator_visible(hidden, false);
            chart.add_indicator(Box::new(
                CustomIndicator::new("Fixed", |bars| {
                    vec![IndicatorValue::Single(1.23456); bars.len()]
                })
                .with_value_format(StudyFormat::Fixed(4)),
            ));
            let override_id = chart
                .add_indicator(Box::new(CustomIndicator::new("Override", |bars| {
                    vec![IndicatorValue::Single(9.876); bars.len()]
                })));
            let indicator = chart.indicator_mut(override_id).unwrap();
            let mut settings = indicator.settings();
            settings.precision = Some(1);
            indicator.apply_settings(&[], settings).unwrap();
            instrument.price_decimals = Some(3);
            chart.set_instrument(Some(instrument));
            let mut last = bars[2].clone();
            last.close = 22.0;
            last.high = 23.0;
            chart.update(SeriesId::MAIN, last);
            let csv = chart.export_csv();
            fn assert_send_sync<T: Send + Sync>(_: &T) {}
            assert_send_sync(&csv);
            let csv = csv.unwrap();
            let mut reader = csv::Reader::from_reader(csv.as_bytes());
            assert_eq!(
                reader.headers().unwrap().iter().collect::<Vec<_>>(),
                [
                    "time",
                    "open",
                    "high",
                    "low",
                    "close",
                    "volume",
                    "Average, pair: mean",
                    "Average, pair: shifted",
                    "Fixed: Fixed",
                    "Override: Override"
                ]
            );
            let rows = reader.records().collect::<Result<Vec<_>, _>>().unwrap();
            assert_eq!(rows.len(), 3);
            assert_eq!(&rows[0][0], expected_times[0]);
            assert_eq!(&rows[1][0], expected_times[1]);
            assert_eq!(&rows[0][1], "10.000");
            assert_eq!(&rows[2][4], "22.000");
            assert_eq!(&rows[0][5], "1234.5");
            // Two-bar means are 12 and (14 + 22) / 2 = 18; displacement -1 moves them left.
            assert_eq!(
                rows.iter().map(|row| &row[6]).collect::<Vec<_>>(),
                ["", "12.000", "18.000"]
            );
            assert_eq!(
                rows.iter().map(|row| &row[7]).collect::<Vec<_>>(),
                ["12.000", "18.000", ""]
            );
            assert_eq!(&rows[0][8], "1.2346");
            assert_eq!(&rows[0][9], "9.9");
            // Export must not refresh or mutate the chart's scheduled study results.
            assert!(chart.pending_from[0].is_some());
        }
        let empty = Chart::new(BarData::new()).export_csv().unwrap();
        assert_eq!(empty, "time,open,high,low,close,volume\n");
    }

    #[test]
    fn images_retain_loaded_marks_and_event_visibility_without_fetching() {
        use crate::data::Reply;
        use crate::model::{EventKind, EventVisibility, Marker, TimeScaleEvent};
        use crate::services::{Marks, MarksPage, MarksRequest};
        use crate::widget::test_frames;
        use std::sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        };

        struct Source {
            page: MarksPage,
            calls: AtomicUsize,
        }
        impl Marks for Source {
            fn marks(&self, _: MarksRequest, reply: Reply<MarksPage>) {
                self.calls.fetch_add(1, Ordering::Relaxed);
                reply.send(Ok(self.page.clone()));
            }
        }
        let ctx = egui::Context::default();
        let mut chart = test_frames::chart(&[95.0], &[105.0]);
        test_frames::frame(&ctx, &mut chart, 0.0, vec![]);
        let time = chart.state.shown()[70].time;
        let event = TimeScaleEvent {
            id: "earnings".into(),
            time,
            kind: EventKind::Earnings,
            title: "Host earnings".into(),
            details: String::new(),
        };
        chart.set_markers(SeriesId::MAIN, vec![Marker::new(time).with_text("Manual")]);
        chart.set_events(vec![Marker::event(time, "Manual axis")]);
        chart.set_time_scale_events(vec![event.clone()]);
        chart.set_event_visibility(EventVisibility {
            show_earnings: true,
            show_news: false,
            ..Default::default()
        });
        let source = Arc::new(Source {
            page: MarksPage {
                bar_marks: vec![Marker::new(time).with_text("Source bar")],
                time_scale_marks: vec![Marker::event(time, "Source axis")],
                events: vec![TimeScaleEvent {
                    kind: EventKind::News,
                    ..event
                }],
            },
            calls: AtomicUsize::new(0),
        });
        chart.set_mark_source(Some(source.clone()));
        chart.pump_marks(&ctx);
        assert_eq!(source.calls.load(Ordering::Relaxed), 1);

        let mut snapshot = chart.export_snapshot();
        assert!(snapshot.mark_source.is_none());
        assert_eq!(snapshot.event_visibility(), chart.event_visibility());
        assert_eq!(snapshot.time_scale_events.len(), 2);
        let shapes = test_frames::shapes(&ctx, &mut snapshot, 0.1, vec![]);
        for label in ["Manual", "Manual axis", "Source bar", "Source axis", "E"] {
            assert!(
                shapes.iter().any(|shape| matches!(shape,
                egui::Shape::Text(text) if text.galley.text() == label)),
                "missing {label}"
            );
        }
        assert!(!shapes.iter().any(|shape| matches!(shape,
            egui::Shape::Text(text) if text.galley.text() == "N")));
        let mut image = chart
            .export_image(&ctx, &ImageExportOptions::default())
            .unwrap();
        image.textures_delta.clear();
        assert_eq!(source.calls.load(Ordering::Relaxed), 1);
        assert_eq!(chart.marks.len(), 1);
        assert_eq!(chart.events.len(), 1);
        assert_eq!(chart.time_scale_events.len(), 1);
    }

    #[test]
    #[cfg(feature = "drawings")]
    fn images_apply_options_and_drawing_visibility_without_changing_the_live_view() {
        let ctx = egui::Context::default();
        let mut chart = Chart::new(BarData::new());
        chart.symbol = "EXPORT TEST".into();
        chart.add_indicator(Box::new(
            CustomIndicator::new("Pane", |_| vec![]).with_overlay(false),
        ));
        ctx.run_ui(Default::default(), |_| {})
            .drop_without_applying_deltas();
        let before = chart.get_time_scale_state();
        let vertices = |image: &ChartImage| {
            image
                .primitives
                .iter()
                .map(|primitive| match &primitive.primitive {
                    egui::epaint::Primitive::Mesh(mesh) => mesh.vertices.len(),
                    _ => 0,
                })
                .sum::<usize>()
        };
        let base = ImageExportOptions {
            size: [480, 320],
            include_title: false,
            include_timestamp: false,
            include_watermark: true,
            transparent_background: false,
            include_crosshair: false,
            ..Default::default()
        };
        let mut opaque = chart.export_image(&ctx, &base).unwrap();
        let has_textures = !opaque.textures_delta.set.is_empty();
        opaque.textures_delta.clear();
        assert_eq!(opaque.size, [480, 320]);
        assert!(has_textures);
        for (title, timestamp) in [(true, false), (false, true)] {
            let mut image = chart
                .export_image(
                    &ctx,
                    &ImageExportOptions {
                        include_title: title,
                        include_timestamp: timestamp,
                        ..base.clone()
                    },
                )
                .unwrap();
            image.textures_delta.clear();
            assert!(
                vertices(&image) > vertices(&opaque),
                "missing requested header"
            );
        }
        let mut transparent = chart
            .export_image(
                &ctx,
                &ImageExportOptions {
                    transparent_background: true,
                    ..base.clone()
                },
            )
            .unwrap();
        transparent.textures_delta.clear();
        assert!(
            vertices(&transparent) < vertices(&opaque),
            "backgrounds were retained"
        );
        assert_eq!(chart.get_time_scale_state(), before);
        assert!(
            chart.last_rendered_price_rect.is_negative(),
            "live geometry changed"
        );
        use crate::drawings::{ChartPoint, Drawing, DrawingToolType, TimeframeVisibility};
        let mut drawing = Drawing::new(1, DrawingToolType::HorizontalLine);
        drawing.completed = true;
        drawing.chart_points = vec![ChartPoint::new(0.0, 10.0)];
        drawing.color = [71, 89, 123, 255];
        drawing.timeframe_visibility = TimeframeVisibility::specific(&["1h"]);
        chart.drawings.drawings.push(drawing);
        chart.set_data(
            SeriesId::MAIN,
            vec![Bar::new(
                DateTime::from_timestamp(0, 0).unwrap(),
                10.0,
                11.0,
                9.0,
                10.0,
                1.0,
            )],
        );
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            chart.theme_override = Some(theme);
            for (timeframe, visible) in [(Timeframe::Min1, false), (Timeframe::Hour1, true)] {
                chart.set_timeframe(timeframe);
                let mut image = chart.export_image(&ctx, &base).unwrap();
                image.textures_delta.clear();
                assert_eq!(image.primitives.iter().any(|primitive| {
                    matches!(&primitive.primitive, egui::epaint::Primitive::Mesh(mesh)
                        if mesh.vertices.iter().any(|vertex| vertex.color == egui::Color32::from_rgb(71, 89, 123)))
                }), visible, "drawing visibility at {timeframe:?}");
            }
        }
        for size in [[0, 320], [480, u32::MAX], [7680, 7680]] {
            assert!(
                chart
                    .export_image(
                        &ctx,
                        &ImageExportOptions {
                            size,
                            ..base.clone()
                        }
                    )
                    .is_err()
            );
        }
    }

    #[test]
    fn table_text_holds_the_chosen_columns_with_their_missing_cells() {
        let table = DataTable {
            headers: vec!["time".into(), "close".into(), "SMA: 5".into()],
            rows: vec![
                vec!["t0".into(), "1.50".into(), String::new()],
                vec!["t1".into(), "2.50".into(), "2.00".into()],
            ],
        };
        assert_eq!(table.csv(&[0, 2]).unwrap(), "time,SMA: 5\nt0,\nt1,2.00\n");
        assert_eq!(table.tsv(&[2, 1]), "SMA: 5\tclose\n\t1.50\n2.00\t2.50");
    }

    #[test]
    fn svg_images_apply_the_options_and_report_what_they_cannot_hold() {
        use crate::widget::test_frames::{chart, frame};
        let ctx = egui::Context::default();
        let mut chart = chart(&[10.0, 12.0], &[14.0, 13.0]);
        chart.symbol = "EXPORT TEST".into();
        chart.config.show_watermark = true;
        chart.config.watermark_text = Some("WATERMARK".into());
        frame(&ctx, &mut chart, 0.0, vec![]);
        let base = ImageExportOptions {
            size: [480, 320],
            include_title: false,
            include_timestamp: false,
            include_watermark: true,
            transparent_background: false,
            include_crosshair: false,
            ..Default::default()
        };
        let svg = |options: &ImageExportOptions| chart.export_svg(&ctx, options).unwrap();
        let opaque = svg(&base);
        assert!(opaque.starts_with("<svg") && opaque.contains(r#"width="480" height="320""#));
        assert!(opaque.contains("WATERMARK"), "watermark was dropped");
        assert!(
            opaque.contains("data:image/png;base64,"),
            "frame textures were dropped"
        );
        assert!(!opaque.contains("EXPORT TEST"));
        let no_watermark = svg(&ImageExportOptions {
            include_watermark: false,
            include_title: true,
            ..base.clone()
        });
        assert!(!no_watermark.contains("WATERMARK") && no_watermark.contains("EXPORT TEST"));
        let [r, g, b, _] = chart.config.background_color.to_srgba_unmultiplied();
        let bg = format!(r#"fill="rgb({r},{g},{b})""#);
        assert!(opaque.contains(&bg));
        let transparent = svg(&ImageExportOptions {
            transparent_background: true,
            ..base.clone()
        });
        assert!(!transparent.contains(&bg), "background was retained");
        let callback = |ui: &mut egui::Ui, _: &mut Chart| {
            ui.painter().add(egui::PaintCallback {
                rect: ui.max_rect(),
                callback: std::sync::Arc::new(()),
            });
        };
        let host_texture = |ui: &mut egui::Ui, _: &mut Chart| {
            ui.painter().image(
                egui::TextureId::User(7),
                ui.max_rect(),
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
        };
        for (error, reason) in [
            (chart.export_layout_svg(&ctx, &base, callback), "callbacks"),
            (
                chart
                    .export_layout_png(&ctx, &base, callback)
                    .map(|_| String::new()),
                "callbacks",
            ),
            (
                chart.export_layout_svg(&ctx, &base, host_texture),
                "host textures",
            ),
            (
                chart
                    .export_layout_png(&ctx, &base, host_texture)
                    .map(|_| String::new()),
                "host textures",
            ),
        ] {
            let error = error.unwrap_err();
            assert!(error.contains(reason), "{error}");
        }
        let red = egui::Color32::from_rgb(200, 30, 30);
        let green = egui::Color32::from_rgb(30, 200, 30);
        let high_id = egui::TextureId::User(7 | 1 << 63);
        let supplied = ImageExportOptions {
            textures: HashMap::from([
                (
                    egui::TextureId::User(7),
                    Arc::new(egui::ColorImage::new([2, 1], vec![red, green])),
                ),
                (high_id, Arc::new(egui::ColorImage::filled([1, 1], green))),
            ]),
            ..base.clone()
        };
        let additive = ImageExportOptions {
            textures: HashMap::from([(
                egui::TextureId::User(7),
                Arc::new(egui::ColorImage::filled(
                    [1, 1],
                    egui::Color32::from_rgb_additive(200, 0, 0),
                )),
            )]),
            ..base.clone()
        };
        let error = chart
            .export_layout_svg(&ctx, &additive, host_texture)
            .unwrap_err();
        assert!(error.contains("Additive texture User(7)"), "{error}");
        let layout = |ui: &mut egui::Ui, chart: &mut Chart| {
            let (rect, _) = ui.allocate_exact_size(egui::vec2(128.0, 32.0), egui::Sense::hover());
            ui.painter().image(
                egui::TextureId::User(7),
                rect,
                egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                egui::Color32::WHITE,
            );
            ui.label("LAYOUT TOOLBAR");
            for (id, x) in [(high_id, 160.0), (egui::TextureId::User(7), 208.0)] {
                ui.painter().image(
                    id,
                    egui::Rect::from_min_size(egui::pos2(x, 8.0), egui::vec2(32.0, 32.0)),
                    egui::Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1.0, 1.0)),
                    egui::Color32::from_rgba_premultiplied(128, 64, 32, 128),
                );
            }
            chart.show(ui);
        };
        let svg = chart.export_layout_svg(&ctx, &supplied, layout).unwrap();
        assert!(svg.contains("LAYOUT TOOLBAR") && svg.contains("WATERMARK"));
        assert!(svg.contains(r#"width="2" height="1""#));
        for id in [7, 7 | 1u64 << 63] {
            assert!(svg.contains(&format!(r#"id="tu{id}_"#)));
        }
        #[cfg(feature = "icons")]
        {
            let tree = resvg::usvg::Tree::from_str(&svg, &Default::default()).unwrap();
            let mut groups = vec![tree.root()];
            let mut colors = Vec::new();
            while let Some(group) = groups.pop() {
                for node in group.children() {
                    match node {
                        resvg::usvg::Node::Group(group) => groups.push(group),
                        resvg::usvg::Node::Image(image) => {
                            let resvg::usvg::ImageKind::PNG(data) = image.kind() else {
                                panic!("expected an embedded PNG");
                            };
                            let mut reader =
                                png::Decoder::new(data.as_slice()).read_info().unwrap();
                            let mut pixels = vec![0; reader.output_buffer_size()];
                            reader.next_frame(&mut pixels).unwrap();
                            let [r, g, b, a] = pixels[..4].try_into().unwrap();
                            colors
                                .push(egui::Color32::from_rgba_unmultiplied(r, g, b, a).to_array());
                        }
                        _ => {}
                    }
                }
            }
            // Rounded products of source and tint channels / 255, with alpha 255 * 128 / 255.
            for expected in [[15, 50, 4, 128], [100, 8, 4, 128], red.to_array()] {
                assert!(
                    colors.contains(&expected),
                    "missing {expected:?} in {colors:?}"
                );
            }
        }
        let png = chart.export_layout_png(&ctx, &supplied, layout).unwrap();
        let mut reader = png::Decoder::new(png.as_slice()).read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size()];
        reader.next_frame(&mut pixels).unwrap();
        // Both samples lie in the edge-clamped halves of the two-texel image.
        assert_eq!(pixels[(20 * 480 + 20) * 4..][..4], red.to_array());
        assert_eq!(pixels[(20 * 480 + 120) * 4..][..4], green.to_array());
        for (id, size, pixels, reason) in [
            (
                egui::TextureId::User(7),
                [0, 1],
                vec![],
                "invalid dimensions",
            ),
            (
                egui::TextureId::User(7),
                [2, 1],
                vec![red],
                "invalid dimensions",
            ),
            (egui::TextureId::Managed(0), [1, 1], vec![red], "conflicts"),
        ] {
            let options = ImageExportOptions {
                textures: HashMap::from([(
                    id,
                    Arc::new(egui::ColorImage {
                        size,
                        source_size: egui::vec2(size[0] as f32, size[1] as f32),
                        pixels,
                    }),
                )]),
                ..base.clone()
            };
            for error in [
                chart.export_svg(&ctx, &options).unwrap_err(),
                chart.export_png(&ctx, &options).unwrap_err(),
            ] {
                assert!(
                    error.contains(reason) && error.contains(&format!("{id:?}")),
                    "{error}"
                );
            }
        }
    }

    #[test]
    fn png_images_rasterise_the_chart_with_the_last_crosshair_on_request() {
        use crate::widget::test_frames::{chart, frame};
        use egui::{Event, Pos2};
        let ctx = egui::Context::default();
        let mut chart = chart(&[100.0], &[110.0]);
        frame(
            &ctx,
            &mut chart,
            0.0,
            vec![Event::PointerMoved(Pos2::new(400.0, 200.0))],
        );
        frame(&ctx, &mut chart, 0.1, vec![Event::PointerGone]);
        let decode = |include_crosshair| {
            let png = chart
                .export_png(
                    &ctx,
                    &ImageExportOptions {
                        size: [480, 320],
                        include_title: false,
                        include_timestamp: false,
                        include_crosshair,
                        ..Default::default()
                    },
                )
                .unwrap();
            let mut reader = png::Decoder::new(png.as_slice()).read_info().unwrap();
            let mut pixels = vec![0; reader.output_buffer_size()];
            let info = reader.next_frame(&mut pixels).unwrap();
            assert_eq!(
                (info.width, info.height, info.color_type),
                (480, 320, png::ColorType::Rgba)
            );
            pixels.as_chunks::<4>().0.to_vec()
        };
        let (plain, crosshair) = (decode(false), decode(true));
        // Candle bodies keep their exact colour through rasterisation and encoding.
        let bullish = chart.config().bullish_color.to_srgba_unmultiplied();
        assert!(plain.iter().filter(|pixel| **pixel == bullish).count() > 1000);
        // The crosshair where the pointer left adds a vertical and a horizontal line.
        let changed = |x: usize, y: usize| plain[y * 480 + x] != crosshair[y * 480 + x];
        let column = (0..480).map(|x| (0..320).filter(|&y| changed(x, y)).count());
        let row = (0..320).map(|y| (0..480).filter(|&x| changed(x, y)).count());
        assert!(column.max() > Some(60) && row.max() > Some(100));
    }
}
