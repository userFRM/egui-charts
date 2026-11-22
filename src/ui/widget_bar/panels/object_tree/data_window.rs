//! Data Window component
//!
//! Displays real-time OHLCV data at the cursor position.
//! Collapsible section at the top of the Object Tree panel.

use crate::localization as l10n;
use egui::{RichText, Ui};

use crate::icons::icons;
use crate::scales::VolumeFormatter;
use crate::theme::ThemeExt;
use crate::ui_kit::IconButton;

use super::super::format_price;
use super::types::{DataWindowInfo, DataWindowSeries};
use crate::ui_kit::ext::UiExt;

impl DataWindowInfo {
    /// Populates the readout from the bar the chart's status line shows: the one under its
    /// crosshair, whether from the pointer, a linked chart or the host, falling back to the
    /// latest bar with a value.
    /// Uses the chart's time formatter, price precision, visible added series and visible study
    /// plots, leaving out the volume columns that the volume row shows.
    /// Call after [`crate::widget::Chart::show`] so hover and study values describe the current frame.
    pub fn from_chart(chart: &crate::widget::Chart) -> Self {
        let index = chart
            .readout_index
            .filter(|index| *index < chart.state().shown().len());
        Self::at(chart, index, None)
    }

    /// Populates the readout from the main-series point of a pointer event, with the event's
    /// study rows, as [`Self::from_chart`] does at the chart's crosshair. A pointer without a
    /// main-series point, or one the chart no longer shows, reads as [`Self::from_chart`].
    pub fn from_pointer(
        chart: &crate::widget::Chart,
        pointer: &crate::widget::ChartPointer,
    ) -> Self {
        let index = usize::try_from(pointer.index).ok().filter(|index| {
            pointer.bar.as_ref().is_some_and(|bar| {
                chart
                    .state()
                    .shown()
                    .get(*index)
                    .is_some_and(|shown| shown.time == bar.time)
            })
        });
        match index {
            Some(_) => Self::at(chart, index, Some(&pointer.studies)),
            None => Self::from_chart(chart),
        }
    }

    /// The readout at the shown bar `hovered`, or at the latest bar with a value without one,
    /// with `studies`, or the chart's own study rows at that bar.
    fn at(
        chart: &crate::widget::Chart,
        hovered: Option<usize>,
        studies: Option<&[(crate::widget::SeriesId, crate::chart::rendering::StudyValue)]>,
    ) -> Self {
        let bars = chart.state().shown();
        let index = hovered.unwrap_or_else(|| {
            bars.iter()
                .rposition(|bar| !bar.is_whitespace())
                .unwrap_or(bars.len().saturating_sub(1))
        });
        let mut info = Self::new(chart.symbol())
            .with_timeframe(chart.timeframe().map(|tf| tf.as_str()).unwrap_or_default());
        info.value_name = chart.horizontal_value_name().map(str::to_owned);
        if let Some(instrument) = chart.instrument() {
            info = info.with_instrument(instrument);
        }
        info.volume_hidden = !chart.chart_type().supports_volume();
        if let Some((bar, previous_close)) = chart.readout_bar_at(index) {
            info = info
                .with_ohlcv(bar.open, bar.high, bar.low, bar.close, bar.volume)
                .with_timestamp(chart.format_time(bar.time))
                .with_prev_close(previous_close.unwrap_or(bar.open));
            // The volume row above shows the volume columns' value.
            info.studies = studies
                .map_or_else(|| chart.study_rows_at(index), <[_]>::to_vec)
                .into_iter()
                .filter(|(id, row)| !chart.is_volume_column(*id, row))
                .map(|(_, row)| row)
                .collect();
        }
        // Added series read the hovered time, or their own latest bar, as their status rows do.
        let time = hovered
            .and_then(|index| bars.get(index))
            .map(|bar| bar.time);
        info.series = chart
            .series_readouts(time)
            .map(|(name, color, value)| DataWindowSeries::new(name, color, value))
            .collect();
        info.with_chart_formatters(chart)
    }

    /// Formats this readout's raw values as the chart's status line writes them: with the
    /// chart's host formatters, else with its price decimals and thousands separator.
    /// [`Self::price_decimals`] becomes the precision chosen for the chart's price scale, else
    /// the instrument's; without either it keeps its own, and `None` writes the status line's
    /// decimals. Call after populating OHLCV and previous close.
    pub fn with_chart_formatters(mut self, chart: &crate::widget::Chart) -> Self {
        let price_formatter = chart.series_price_formatter(crate::widget::SeriesId::MAIN);
        let formats = chart.value_formatters(price_formatter.as_deref());
        self.price_decimals = chart.readout_price_decimals().or(self.price_decimals);
        let decimals = self
            .price_decimals
            .map_or_else(|| crate::scales::price_decimals(self.close), usize::from);
        // Built-in labels write '.' as the decimal point, so '.' and digits group nothing.
        let separator = chart
            .config
            .thousands_sep
            .filter(|separator| *separator != '.' && !separator.is_ascii_digit());
        let price = |value: f64| match formats.price {
            Some(formatter) => formatter.format(value),
            None if value.is_finite() => {
                crate::scales::group_thousands(format!("{value:.decimals$}"), separator)
            }
            None => "\u{2014}".to_owned(),
        };
        self.formatted_prices = Some([self.open, self.high, self.low, self.close].map(price));
        self.formatted_volume = formats
            .volume
            .map(|formatter| formatter.format(self.volume));
        let absolute = formats.price.map_or_else(
            || crate::scales::group_thousands(format!("{:+.decimals$}", self.change), separator),
            |formatter| formatter.format_change(self.change),
        );
        let percentage = formats.percentage.map_or_else(
            || format!("{:+.2}%", self.change_percent),
            |formatter| formatter.format(self.change_percent),
        );
        self.formatted_change = Some(format!("{absolute} ({percentage})"));
        self
    }
}

/// Render the Data Window section
///
/// Shows OHLCV data at cursor position in a compact grid layout.
pub fn show_data_window(ui: &mut Ui, data: &DataWindowInfo, expanded: bool) -> bool {
    let mut is_expanded = expanded;

    // Header with collapse button
    ui.horizontal(|ui| {
        // Collapse/expand button
        let icon = if is_expanded {
            &icons::CHEVRON_DOWN
        } else {
            &icons::CHEVRON_RIGHT
        };
        if IconButton::new(icon)
            .tooltip("Toggle Data Window")
            .show(ui)
            .clicked()
        {
            is_expanded = !is_expanded;
        }

        // Title
        ui.strong_label("Data Window");

        // Symbol badge
        ui.right_aligned(|ui| {
            if !data.symbol.is_empty() {
                ui.label(
                    RichText::new(l10n::text(ui.ctx(), &data.symbol))
                        .small()
                        .color(ui.visuals().weak_text_color()),
                );
            }
        });
    });

    // Content (if expanded)
    if is_expanded {
        ui.indent("data_window_content", |ui| {
            ui.space_xs();

            egui::Grid::new("bar_time").num_columns(2).show(ui, |ui| {
                if !data.timestamp.is_empty() {
                    // An unnamed value keeps its column empty and skips the lookup, as the
                    // chart's readout does.
                    if let Some(name) = data.value_name.as_deref().filter(|name| !name.is_empty()) {
                        ui.hint_label(name);
                    } else {
                        ui.label("");
                    }
                    ui.label(l10n::text(ui.ctx(), &data.timestamp));
                    ui.end_row();
                }
                if !data.timeframe.is_empty() {
                    ui.hint_label("Interval");
                    ui.label(l10n::text(ui.ctx(), &data.timeframe));
                    ui.end_row();
                }
            });
            ui.space_xs();

            // OHLCV grid
            show_ohlcv_grid(ui, data);
            if !data.series.is_empty() {
                ui.space_sm();
                egui::Grid::new("series_values")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for series in &data.series {
                            ui.hint_label(&series.name);
                            ui.label(
                                RichText::new(series.value.as_deref().unwrap_or("∅"))
                                    .color(series.color),
                            );
                            ui.end_row();
                        }
                    });
            }
            if !data.studies.is_empty() {
                ui.space_sm();
                egui::Grid::new("study_values")
                    .num_columns(2)
                    .show(ui, |ui| {
                        for study in &data.studies {
                            ui.label(
                                RichText::new(study.translated_label(ui.ctx()))
                                    .small()
                                    .weak(),
                            );
                            ui.label(
                                RichText::new(l10n::text(ui.ctx(), study.formatted_value()))
                                    .color(study.color),
                            );
                            ui.end_row();
                        }
                    });
            }

            ui.space_sm();
        });
    }

    is_expanded
}

/// Render the OHLCV grid
fn show_ohlcv_grid(ui: &mut Ui, data: &DataWindowInfo) {
    // Without an instrument, prices take the status line's decimals for the close.
    let decimals = Some(
        data.price_decimals
            .unwrap_or_else(|| crate::scales::price_decimals(data.close) as u8),
    );
    let change_color = if data.is_bullish() {
        ui.active_theme().chart.candle_up
    } else {
        ui.active_theme().chart.candle_down
    };

    egui::Grid::new("ohlcv_grid")
        .num_columns(2)
        .spacing([ui.active_theme().spacing.md, ui.active_theme().spacing.xs])
        .min_col_width(50.0)
        .show(ui, |ui| {
            ui.hint_label("O");
            ui.label(l10n::text(
                ui.ctx(),
                data.formatted_prices.as_ref().map_or_else(
                    || format_price(data.open, decimals),
                    |values| values[0].clone(),
                ),
            ));
            ui.end_row();
            ui.hint_label("H");
            ui.label(l10n::text(
                ui.ctx(),
                data.formatted_prices.as_ref().map_or_else(
                    || format_price(data.high, decimals),
                    |values| values[1].clone(),
                ),
            ));
            ui.end_row();

            ui.hint_label("L");
            ui.label(l10n::text(
                ui.ctx(),
                data.formatted_prices.as_ref().map_or_else(
                    || format_price(data.low, decimals),
                    |values| values[2].clone(),
                ),
            ));
            ui.end_row();
            ui.hint_label("C");
            ui.label(
                RichText::new(l10n::text(
                    ui.ctx(),
                    data.formatted_prices.as_ref().map_or_else(
                        || format_price(data.close, decimals),
                        |values| values[3].clone(),
                    ),
                ))
                .color(change_color),
            );
            ui.end_row();

            if !data.volume_hidden {
                ui.hint_label("Vol");
                ui.label(l10n::text(
                    ui.ctx(),
                    data.formatted_volume.clone().unwrap_or_else(|| {
                        data.volume_precision.map_or_else(
                            || format_volume(data.volume),
                            |decimals| {
                                VolumeFormatter {
                                    precision: usize::from(decimals),
                                }
                                .format_with_min_unit_decimals(data.volume, 2)
                            },
                        )
                    }),
                ));
                ui.end_row();
            }
            ui.hint_label("Chg");
            ui.label(
                RichText::new(data.formatted_change.clone().unwrap_or_else(|| {
                    format!(
                        "{:+.*} ({:+.2}%)",
                        usize::from(data.price_decimals.unwrap_or(2)),
                        data.change,
                        data.change_percent
                    )
                }))
                .color(change_color)
                .small(),
            );
            ui.end_row();
        });
}

/// Format volume with K/M/B suffixes
fn format_volume(volume: f64) -> String {
    if volume == 0.0 {
        return "\u{2014}".to_string();
    }

    crate::scales::StudyFormat::Volume.format(volume, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instrument_precision_formats_prices_change_and_volume() {
        use crate::model::{Bar, BarData};
        use crate::scales::CustomPriceFormatter;
        use std::sync::Arc;
        let time = chrono::DateTime::from_timestamp(1_704_067_200, 0).unwrap();
        let next = time + chrono::TimeDelta::minutes(1);
        // 0.00001 / 1.23455 * 100 = 0.000810... percentage points; without an instrument,
        // 41,639.77 / 1,000 * 100 = 4,163.977 percentage points, written as the status line does.
        // A precision chosen for the price scale replaces the instrument's.
        for (instrument, custom, separator, chosen, expected) in [
            (
                true,
                false,
                Some(','),
                None,
                &["0.00000", "1.23456", "+0.00001 (+0.00%)", "1.234K"][..],
            ),
            (
                true,
                true,
                Some(','),
                None,
                &[
                    "p 0.00000",
                    "p 1.23456",
                    "p 0.00001 (pct 0.001)",
                    "1234 units",
                ],
            ),
            (
                false,
                false,
                Some(','),
                None,
                &["1,332.57", "42,639.77", "+41,639.77 (+4163.98%)"],
            ),
            (
                false,
                false,
                None,
                None,
                &["1332.57", "42639.77", "+41639.77 (+4163.98%)"],
            ),
            (
                false,
                false,
                Some('.'),
                None,
                &["1332.57", "42639.77", "+41639.77 (+4163.98%)"],
            ),
            (
                true,
                false,
                Some(','),
                Some(7),
                &["0.0000000", "1.2345600", "+0.0000100 (+0.00%)", "1.234K"],
            ),
        ] {
            let mut chart = crate::widget::Chart::new(BarData::from_bars(if instrument {
                vec![
                    Bar::new(time, 1.23455, 1.23455, 1.23455, 1.23455, 0.0),
                    Bar::new(next, 0.0, 1.23456, 0.0, 1.23456, 1234.0),
                ]
            } else {
                vec![
                    Bar::from_value(time, 1000.0),
                    Bar::new(next, 1332.57, 42639.77, 1332.57, 42639.77, 1234.0),
                ]
            }));
            chart.config.thousands_sep = separator;
            if instrument {
                let mut instrument = crate::model::Instrument::new("FX").with_volume_decimals(3);
                instrument.tick_size = Some(0.00001);
                chart.set_instrument(Some(instrument));
            }
            if custom {
                chart.set_price_formatter(Some(Arc::new(CustomPriceFormatter::new(|value| {
                    format!("p {value:.5}")
                }))));
                chart.set_percentage_formatter(Some(Arc::new(CustomPriceFormatter::new(
                    |value| format!("pct {value:.3}"),
                ))));
                chart.set_volume_formatter(Some(Arc::new(CustomPriceFormatter::new(|value| {
                    format!("{value:.0} units")
                }))));
            }
            chart.price_scale_mut().set_price_precision(chosen);
            let data = DataWindowInfo::from_chart(&chart);
            assert_eq!(data.price_decimals, chosen.or(instrument.then_some(5)));
            // A host that reads the bar from its own pointer events, as the terminal does on
            // hover, writes the same prices.
            let mut host = DataWindowInfo::new("FX")
                .with_ohlcv(data.open, data.high, data.low, data.close, data.volume)
                .with_prev_close(data.prev_close);
            if let Some(instrument) = chart.instrument() {
                host = host.with_instrument(instrument);
            }
            let host = host.with_chart_formatters(&chart);
            assert_eq!(host.formatted_prices, data.formatted_prices, "{chosen:?}");
            assert_eq!(host.formatted_change, data.formatted_change, "{chosen:?}");
            let ctx = egui::Context::default();
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                show_ohlcv_grid(ui, &data);
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
            for expected in expected {
                assert!(
                    texts.iter().any(|text| text == expected),
                    "{expected}: {texts:?}"
                );
            }
        }
    }

    /// The chart types that hide volume list no volume row and no volume study rows in the
    /// data window, as their tooltip and legend do; a candle chart keeps both.
    #[test]
    fn data_window_lists_no_volume_on_charts_that_hide_volume() {
        use crate::model::ChartType;
        use crate::widget::test_frames::{chart, frame};
        let mut failures = Vec::new();
        for (kind, shown) in [
            (ChartType::Candles, true),
            (ChartType::Renko, false),
            (ChartType::Kagi, false),
            (ChartType::LineBreak, false),
            (ChartType::PointAndFigure, false),
            (ChartType::Range, false),
        ] {
            let ctx = egui::Context::default();
            // Closes of 100, 103, 106 and 109 in turn build elements at the default sizes.
            let mut chart = chart(&[100.0], &[100.0, 103.0, 106.0, 109.0]);
            chart.set_chart_type(kind);
            frame(&ctx, &mut chart, 0.0, vec![]);
            // The average keeps a row beside the volume row, which shows the columns' value.
            let volume = chart
                .indicators()
                .find(|(_, study)| study.is_volume())
                .unwrap()
                .0;
            chart
                .indicator_mut(volume)
                .unwrap()
                .set_input("show_average", crate::studies::InputValue::Toggle(true))
                .unwrap();
            frame(&ctx, &mut chart, 0.1, vec![]);
            let data = DataWindowInfo::from_chart(&chart);
            let rows: Vec<_> = data
                .studies
                .iter()
                .filter(|row| row.study == "Volume")
                .map(|row| row.label())
                .collect();
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                show_ohlcv_grid(ui, &data);
            });
            let volume = output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Vol")
            });
            output.drop_without_applying_deltas();
            if rows.is_empty() == shown || volume != shown {
                failures.push(format!(
                    "{kind:?}: volume row {volume}, study rows {rows:?}"
                ));
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// The data window names the horizontal value as the chart's readout does: "Time" on a time
    /// chart and in a readout the host builds, else the horizontal scale's name through the
    /// translator, and no name for a numeric scale that names nothing, even when the host
    /// translates the empty text.
    #[test]
    fn data_window_names_the_horizontal_value_as_the_readout_does() {
        use crate::model::{Bar, BarData};
        use crate::scales::{
            CustomHorizontalScale, HorizontalScale, MaturityScale, PriceHorizontalScale, key_time,
        };
        use std::sync::Arc;
        let from_chart = |bars: Vec<Bar>, scale: Option<Arc<dyn HorizontalScale>>| {
            let mut chart = crate::widget::Chart::new(BarData::from_bars(bars));
            chart.set_horizontal_scale(scale);
            DataWindowInfo::from_chart(&chart)
        };
        let at = |keys: &[f64]| -> Vec<Bar> {
            keys.iter()
                .map(|key| Bar::from_value(key_time(*key).unwrap(), 20.0))
                .collect()
        };
        let mut strikes = PriceHorizontalScale::new(0);
        strikes.value_name = Some("Strike");
        let sides = CustomHorizontalScale::new(["Call", "Put"], |key: &&str| (*key).to_owned())
            .with_value_name("Side");
        let side_bars = vec![
            sides.value(&"Call", 20.0).unwrap(),
            sides.value(&"Put", 20.0).unwrap(),
        ];
        let time = chrono::DateTime::from_timestamp(1_704_067_200, 0).unwrap();
        let clock = vec![
            Bar::from_value(time, 20.0),
            Bar::from_value(time + chrono::TimeDelta::minutes(1), 20.0),
        ];
        // Each readout shows its latest bar: strike 4925, 30 months = 2 * 12 + 6, and "Put".
        // A time chart's value text depends on today's date, so only its name is checked.
        let cases: [(DataWindowInfo, &[&str]); 6] = [
            (
                from_chart(at(&[4900.0, 4925.0]), Some(Arc::new(strikes))),
                &["Strike", "4925"],
            ),
            (
                from_chart(
                    at(&[1.0, 6.0, 30.0]),
                    Some(Arc::new(MaturityScale::default())),
                ),
                &["Maturity", "2Y6M"],
            ),
            (
                from_chart(
                    at(&[4900.0, 4925.0]),
                    Some(Arc::new(PriceHorizontalScale::new(0))),
                ),
                &["4925"],
            ),
            (
                from_chart(side_bars, Some(Arc::new(sides))),
                &["Seite", "Put"],
            ),
            (from_chart(clock, None), &["Time"]),
            (
                DataWindowInfo::new("").with_timestamp("12:00"),
                &["Time", "12:00"],
            ),
        ];
        let mut failures = Vec::new();
        for (info, expected) in cases {
            let ctx = egui::Context::default();
            crate::localization::set_translator(&ctx, |label| match label {
                "" => Some("Unexpected name".to_owned()),
                "Side" => Some("Seite".to_owned()),
                _ => None,
            });
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                show_data_window(ui, &info, true);
            });
            let texts: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if !text.galley.text().is_empty() => {
                        Some(text.galley.text().to_owned())
                    }
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            // The value's row follows the title: these charts have no symbol and no interval.
            let title = texts.iter().position(|text| text == "Data Window").unwrap();
            let shown = &texts[title + 1..title + 1 + expected.len()];
            if shown != expected {
                failures.push(format!("{expected:?}: {texts:?}"));
            }
        }
        assert!(failures.is_empty(), "{failures:#?}");
    }

    #[test]
    fn test_format_volume() {
        use crate::scales::StudyFormat::*;
        for (format, value, precision, expected) in [
            (Volume, 1_500_000_000.0, None, "1.50B"),
            (Volume, 2_500_000.0, None, "2.50M"),
            (Volume, 3_500.0, None, "3.50K"),
            (Volume, 500.0, None, "500"),
            (Volume, -12_345_678.0, None, "-12.35M"),
            (Volume, 0.0, Some(2), "0.00"),
            (Volume, 1_234.0, Some(3), "1.234K"),
            (Price, 1.23456, Some(5), "1.23456"),
            (Price, -37.625, Some(3), "-37.625"),
            (Percent, 5.125, None, "5.12%"),
            (Percent, -2.5, Some(1), "-2.5%"),
            (Fixed(4), 0.0004, None, "0.0004"),
            (Fixed(4), 1.25, Some(1), "1.2"),
            (Fixed(usize::MAX), 1.0, None, "1.000000000000"),
            (Volume, f64::NAN, None, "-"),
            (Percent, f64::INFINITY, None, "-"),
        ] {
            assert_eq!(format.format(value, precision), expected, "{format:?}");
            if format == Volume && value > 0.0 && precision.is_none() {
                assert_eq!(format_volume(value), expected);
            }
        }
    }
}
