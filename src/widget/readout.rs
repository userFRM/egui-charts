//! Study readouts for host data windows and chart tooltips.

use super::{Chart, CursorDataPolicy, SeriesId};
use crate::chart::rendering::StudyValue;

/// What the data table reads of a visible added series: id, title, whether it lists its open,
/// high and low, bar revision and last bar (time in nanoseconds, open, high, low and close bits).
#[cfg(feature = "ui")]
pub(crate) type SeriesFingerprint = (usize, String, bool, u64, Option<(i64, [u64; 4])>);

impl Chart {
    /// The table is current while this is unchanged: the history revision, a revision moved
    /// by edits of the last bar, study edits, host-fed study data, the instrument and the
    /// horizontal scale, and the decimals of its prices.
    #[cfg(feature = "ui")]
    pub(crate) fn data_table_revision(&self) -> (u64, u64, Option<u8>) {
        (
            self.state.revision(),
            self.readout_revision,
            self.readout_price_decimals(),
        )
    }

    /// What the table lists of each visible added series: its id, title, whether it lists its
    /// open, high and low, bar revision and last bar, which the revision leaves out. The table is
    /// current while this is unchanged too.
    #[cfg(feature = "ui")]
    pub(crate) fn data_table_series(&self) -> Vec<SeriesFingerprint> {
        self.price_series
            .iter()
            .filter(|item| item.visible)
            .map(|item| {
                let last = item.bars.last().map(|bar| {
                    (
                        bar.time.timestamp_nanos_opt().unwrap_or_default(),
                        [bar.open, bar.high, bar.low, bar.close].map(f64::to_bits),
                    )
                });
                (
                    item.id.0,
                    item.options.name.clone(),
                    item.options.chart_type.uses_ohlc(),
                    item.revision,
                    last,
                )
            })
            .collect()
    }

    /// What the table writes its price cells with besides their values: the thousands
    /// separator, the display format of the main series and of each visible added series, and
    /// the host's price formatters by address. The table is current while this is unchanged too.
    #[cfg(feature = "ui")]
    pub(crate) fn data_table_format(
        &self,
    ) -> (
        Option<char>,
        Vec<Option<crate::model::SeriesDisplayFormat>>,
        Vec<usize>,
    ) {
        (
            self.config.thousands_sep,
            std::iter::once(SeriesId::MAIN)
                .chain(
                    self.price_series
                        .iter()
                        .filter(|item| item.visible)
                        .map(|item| item.id),
                )
                .map(|id| self.display_format(id))
                .collect(),
            self.price_formatter
                .iter()
                .chain(self.series_formatters.values())
                .chain(self.study_formatters.values())
                .map(|formatter| std::sync::Arc::as_ptr(formatter).cast::<()>().addr())
                .collect(),
        )
    }

    /// Tells this chart from the others a host shows one data table over, through a restore of
    /// its saved state, which adds its studies again under new ids.
    #[cfg(feature = "ui")]
    pub(crate) fn data_table_identity(&self) -> u64 {
        self.data_table_identity
    }

    /// Takes the source preselection requested by a study menu's Add indicator action.
    /// Open the host indicator chooser with this source, then call `set_indicator_source`
    /// on the added study. The chart owns no chooser or account service.
    pub fn take_indicator_source_request(&mut self) -> Option<crate::studies::StudySource> {
        self.indicator_source_request.take()
    }

    /// The bar the status line, tooltip and data window describe at a rendered index, with the
    /// close of the bar with a value before it: the drawn candle on a Heikin Ashi chart, whose
    /// change is measured between averaged closes, else the rendered bar.
    pub(crate) fn readout_bar_at(&self, index: usize) -> Option<(crate::model::Bar, Option<f64>)> {
        let readout = |bars: &[crate::model::Bar]| {
            let bar = bars.get(index)?.clone();
            Some((
                bar,
                crate::model::last_with_value(&bars[..index]).map(|bar| bar.close),
            ))
        };
        if self.chart_type == crate::model::ChartType::Heikin && !self.state.data().is_empty() {
            readout(
                &self
                    .heikin_ashi
                    .bars(&self.state.data().bars, self.state.revision()),
            )
        } else {
            readout(self.state.shown())
        }
    }

    /// Whether `row` of the study `id` is a column of a volume study, whose value the data
    /// window's own volume row already shows. Its moving average is not.
    #[cfg(feature = "ui")]
    pub(crate) fn is_volume_column(&self, id: SeriesId, row: &StudyValue) -> bool {
        self.study_ids
            .iter()
            .zip(self.rendered_studies().indicators())
            .find(|(study, _)| **study == id)
            .is_some_and(|(_, study)| {
                let names = study.line_names();
                study.is_volume()
                    && (0..study.line_count()).any(|line| {
                        study.plot_style(line).plot_type == crate::studies::PlotType::Columns
                            && names.get(line) == Some(&row.plot)
                    })
            })
    }

    /// The first visible output with a value at a displayed bar index. Price-based charts
    /// read element study values, as the legend does. The output number is stable within
    /// the study and can be used by host alert services.
    pub fn indicator_value_at(&self, id: SeriesId, index: usize) -> Option<(usize, f64)> {
        let at = self.study_ids.iter().position(|study| *study == id)?;
        super::menu_value(
            self.rendered_studies().indicators().get(at)?.as_ref(),
            index,
        )
    }

    /// Formats a displayed study value with the presentation used by the chart.
    pub fn indicator_value_text(&self, id: SeriesId, value: f64) -> Option<String> {
        let at = self.study_ids.iter().position(|study| *study == id)?;
        Some(
            self.rendered_studies()
                .indicators()
                .get(at)?
                .format_value(value),
        )
    }

    /// Values of visible study plots at a rendered bar index, with legend names and precision.
    /// Gaps remain `None`; price-based charts use studies calculated from element OHLC, and
    /// those that hide volume list no volume study.
    /// Call after [`Self::show`] to use refreshed studies.
    pub fn study_values_at(&self, index: usize) -> Vec<StudyValue> {
        self.study_rows_at(index)
            .into_iter()
            .map(|(_, row)| row)
            .collect()
    }

    /// [`Self::study_values_at`] with the id of each row's study.
    pub(crate) fn study_rows_at(&self, index: usize) -> Vec<(SeriesId, StudyValue)> {
        self.study_rows_at_with_policy(index, CursorDataPolicy::Visible)
    }

    /// Study readouts with stable ids and the host's cursor visibility policy.
    pub(super) fn study_rows_at_with_policy(
        &self,
        index: usize,
        policy: CursorDataPolicy,
    ) -> Vec<(SeriesId, StudyValue)> {
        if index >= self.state.shown().len() {
            return Vec::new();
        }
        let mut rows = Vec::new();
        for (&id, study) in self
            .study_ids
            .iter()
            .zip(self.rendered_studies().indicators())
            .filter(|(_, study)| {
                policy == CursorDataPolicy::IncludeHidden || self.shows_study(study.as_ref())
            })
            // A chart type that hides volume lists no volume rows.
            .filter(|(_, study)| self.chart_type.supports_volume() || !study.is_volume())
        {
            let names = study.line_names();
            let colors = study.colors();
            for line in (0..study.line_count()).filter(|line| {
                policy == CursorDataPolicy::IncludeHidden || study.plot_style(*line).visible
            }) {
                let row = StudyValue {
                    formatted: study.plot_value(line, index).map(|value| {
                        let text = study.format_plot_value(line, value);
                        // Built-in prices group as in the legend; formatter text stays as written.
                        if study.value_format() == crate::scales::StudyFormat::Price
                            && self.study_formatter(id, study.value_format()).is_none()
                        {
                            crate::scales::group_thousands(
                                text,
                                self.config.thousands_sep.filter(|separator| {
                                    *separator != '.' && !separator.is_ascii_digit()
                                }),
                            )
                        } else {
                            text
                        }
                    }),
                    study: study.name().to_owned(),
                    format: study.value_format(),
                    plot: names.get(line).cloned().unwrap_or_default(),
                    value: study.plot_value(line, index),
                    precision: study
                        .precision()
                        .unwrap_or_else(|| match study.value_format() {
                            crate::scales::StudyFormat::Price => self
                                .instrument()
                                .and_then(crate::scales::instrument_price_precision)
                                .map_or(2, usize::from),
                            crate::scales::StudyFormat::Fixed(decimals) => decimals.min(12),
                            crate::scales::StudyFormat::Volume
                                if study
                                    .plot_value(line, index)
                                    .is_some_and(|value| value.abs() < 1_000.0) =>
                            {
                                0
                            }
                            _ => 2,
                        }),
                    color: colors.get(line).copied().unwrap_or(self.config.text_color),
                };
                rows.push((id, row));
            }
        }
        rows
    }
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use super::*;
    use crate::model::{Bar, BarData, ChartType, ChartTypeParams};
    use crate::widget::test_frames::frame;

    #[test]
    fn menu_values_follow_displayed_elements() {
        let bars = [100.0, 130.0, 160.0]
            .into_iter()
            .enumerate()
            .map(|(index, close)| {
                Bar::from_value(
                    chrono::DateTime::UNIX_EPOCH + chrono::Duration::minutes(index as i64),
                    close,
                )
            })
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        let id = chart.add_indicator(Box::new(crate::studies::SMA::new(3)));
        chart.set_chart_type(ChartType::Renko);
        chart.set_chart_type_params(ChartTypeParams {
            renko_brick_size: 10.0,
            ..Default::default()
        });
        frame(&egui::Context::default(), &mut chart, 0.0, vec![]);
        // Six rising bricks close at 110, 120, 130, 140, 150, 160. The third brick's
        // SMA(3) is (110+120+130)/3 = 120; the third source bar's SMA would be 130.
        assert_eq!(chart.indicator_value_at(id, 2), Some((0, 120.0)));
        assert_eq!(
            chart.indicator_value_text(id, 120.0).as_deref(),
            Some("120.00")
        );
        assert_eq!(chart.indicator_value_at(id, 0), None);
        // Bricks hide volume; a time-based chart lists its study.
        chart.set_chart_type(ChartType::Candles);
        frame(&egui::Context::default(), &mut chart, 0.1, vec![]);
        chart.set_volume_formatter(Some(std::sync::Arc::new(
            crate::scales::CustomPriceFormatter::new(|value| format!("{value:.0} lots")),
        )));
        let volume = chart
            .indicators()
            .find(|(_, study)| study.is_volume())
            .unwrap()
            .0;
        let values = chart.study_values_at(2);
        assert!(
            values
                .iter()
                .any(|row| row.study == "Volume" && row.formatted_value().ends_with(" lots"))
        );
        assert!(
            chart
                .indicator_value_text(volume, 25.0)
                .unwrap()
                .ends_with(" lots")
        );
    }
}
