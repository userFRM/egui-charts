//! Chart data export and renderer-independent image frames.

use super::Chart;
use crate::studies::StudyFormat;
use crate::theme::ThemeExt;
use chrono::{DateTime, Utc};

/// Image options shared by the export dialog and offscreen rendering.
#[derive(Debug, Clone)]
pub struct ImageExportOptions {
    /// Output width and height in pixels.
    pub size: [u32; 2],
    /// Include the symbol above the chart.
    pub include_title: bool,
    /// Include the export time in the chart's time zone above the chart.
    pub include_timestamp: bool,
    /// Omit the chart and header backgrounds, retaining plot and drawing colours.
    pub transparent_background: bool,
}

impl Default for ImageExportOptions {
    fn default() -> Self {
        Self {
            size: [1440, 900],
            include_title: true,
            include_timestamp: true,
            transparent_background: false,
        }
    }
}

/// A complete offscreen frame for an egui renderer, at one pixel per point.
/// Upload `textures_delta.set`, draw `primitives` over transparent black at `size`, then
/// release the textures. The host owns GPU readback, PNG encoding and file/clipboard delivery.
/// This uses its own texture namespace; render with a dedicated egui renderer. Hosts must also
/// bind any user textures referenced by their image sources before drawing.
#[must_use = "Render the frame and apply its texture deltas, or clear them before dropping"]
pub struct ChartImage {
    /// Exact output dimensions in pixels.
    pub size: [u32; 2],
    /// Tessellated chart, header and study panes.
    pub primitives: Vec<egui::ClippedPrimitive>,
    /// Textures referenced by the primitives.
    pub textures_delta: egui::TexturesDelta,
}

impl Chart {
    fn export_time(&self, time: DateTime<Utc>) -> String {
        match self.display_timezone() {
            crate::scales::TimezoneMode::Local => time.with_timezone(&chrono::Local).to_rfc3339(),
            zone => time
                .with_timezone(&zone.to_tz().unwrap_or(chrono_tz::UTC))
                .to_rfc3339(),
        }
    }

    /// Exports all loaded source bars and visible study plots as UTF-8 CSV.
    /// Recalculates copies of the studies so streaming updates and pending refreshes are included
    /// without altering the live chart. Price-based charts export source-bar calculations, aligned
    /// by source index and plot displacement, rather than mixing element values with source bars.
    /// Times include the chart's UTC offset, including DST changes and subsecond timestamps.
    /// Prices inherit instrument precision (two decimals without metadata); study overrides and
    /// non-price units retain their precision. Missing/non-finite values are empty cells.
    pub fn export_csv(&self) -> Result<String, Box<dyn std::error::Error>> {
        let bars = &self.state.data().bars;
        let price_precision = self
            .instrument()
            .and_then(crate::scales::instrument_price_precision)
            .map_or(2, usize::from);
        let mut studies: Vec<_> = self
            .indicators()
            .filter(|(_, study)| study.is_visible())
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
        for (index, study) in studies.iter_mut().enumerate() {
            study.set_instrument(self.instrument());
            study.calculate(bars);
            let names = study.line_names();
            for line in (0..study.line_cnt()).filter(|line| study.plot_style(*line).visible) {
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
        let mut writer = csv::Writer::from_writer(Vec::new());
        writer.write_record(header)?;
        for (index, bar) in bars.iter().enumerate() {
            let mut row = vec![self.export_time(bar.time)];
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
            writer.write_record(row)?;
        }
        Ok(String::from_utf8(writer.into_inner()?)?)
    }

    /// Renders an isolated image frame using the same chart, studies, drawings and scales as the
    /// widget. Does not poll feeds, change live viewport state or capture host panels/dialogs.
    /// `ctx` supplies the host's fonts and style after its first frame. Images use one pixel per point and dimensions
    /// between 100 and 7680 on each axis, with at most 33,177,600 pixels.
    /// Pass the returned frame to an egui renderer; see [`ChartImage`] for delivery responsibilities.
    pub fn export_image(
        &self,
        ctx: &egui::Context,
        options: &ImageExportOptions,
    ) -> Result<ChartImage, String> {
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
        let mut theme = self
            .theme_override
            .clone()
            .unwrap_or_else(|| ctx.active_theme());
        if options.transparent_background {
            theme.chart.bg = egui::Color32::TRANSPARENT;
        }
        chart.theme_override = Some(theme.clone());
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
        let mut primitives = Vec::new();
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
                                ui.label(title);
                            }
                            if let Some(timestamp) = &timestamp {
                                ui.label(timestamp);
                            }
                        });
                    }
                    chart.show(ui);
                },
            );
            textures_delta.append(output.textures_delta);
            primitives = image_ctx.tessellate(output.shapes, 1.0);
        }
        Ok(ChartImage {
            size,
            primitives,
            textures_delta,
        })
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
                feed: series.feed.clone(),
                markers: series.markers.clone(),
                lines: series.lines.clone(),
            })
            .collect();
        chart.series_scales = self.series_scales.clone();
        chart.symbol = self.symbol.clone();
        chart.timeframe = self.timeframe;
        chart.legend_instrument = self.instrument().cloned();
        chart.legend_badge = self.legend_badge.clone();
        chart.legend_statuses = self.legend_statuses.clone();
        chart.quote = self.quote.clone();
        chart.session_quote = self.session_quote.clone();
        chart.session_provider = self.session_provider.clone();
        chart.extended_hours = self.extended_hours;
        chart.theme_override = self.theme_override.clone();
        chart.drawings.drawings = self.drawings.drawings.clone();
        chart.drawings.curr_timeframe = self.drawings.curr_timeframe.clone();
        chart.marks = self.marks.clone();
        chart.price_lines = self.price_lines.clone();
        chart.events = self.events.clone();
        chart.pane_layout = self.pane_layout.clone();
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
        #[cfg(feature = "studies")]
        {
            chart.volume_study = self.volume_study;
            chart.volume_setting = self.volume_setting;
        }
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
        for (zone, expected_times) in [
            (
                crate::scales::TimezoneMode::Utc,
                [
                    "2024-11-03T05:30:00.125+00:00",
                    "2024-11-03T06:30:00.125+00:00",
                ],
            ),
            (
                crate::scales::TimezoneMode::Timezone(chrono_tz::America::New_York),
                [
                    "2024-11-03T01:30:00.125-04:00",
                    "2024-11-03T01:30:00.125-05:00",
                ],
            ),
            (
                crate::scales::TimezoneMode::Exchange(chrono_tz::UTC),
                [
                    "2024-11-03T01:30:00.125-04:00",
                    "2024-11-03T01:30:00.125-05:00",
                ],
            ),
        ] {
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
                .with_line_cnt(3)
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
            let csv = chart.export_csv().unwrap();
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
            transparent_background: false,
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
}
