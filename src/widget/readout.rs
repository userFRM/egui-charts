//! Study readouts for host data windows and chart tooltips.

use super::{Chart, SeriesId};
use crate::chart::renderers::StudyValue;

impl Chart {
    /// Takes the source preselection requested by a study menu's Add indicator action.
    /// Open the host indicator chooser with this source, then call `set_indicator_source`
    /// on the added study. The chart owns no chooser or account service.
    pub fn take_indicator_source_request(&mut self) -> Option<crate::studies::StudySource> {
        self.indicator_source_request.take()
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
    /// Gaps remain `None`; price-based charts use studies calculated from element OHLC.
    /// Call after [`Self::show`] to use refreshed studies.
    pub fn study_values_at(&self, index: usize) -> Vec<StudyValue> {
        self.study_rows_at(index)
            .into_iter()
            .map(|(_, row)| row)
            .collect()
    }

    /// [`Self::study_values_at`] with the id of each row's study.
    pub(super) fn study_rows_at(&self, index: usize) -> Vec<(SeriesId, StudyValue)> {
        if index >= self.state.shown().len() {
            return Vec::new();
        }
        let mut rows = Vec::new();
        for (&id, study) in self
            .study_ids
            .iter()
            .zip(self.rendered_studies().indicators())
            .filter(|(_, study)| study.is_visible())
        {
            let names = study.line_names();
            let colors = study.colors();
            for line in (0..study.line_count()).filter(|line| study.plot_style(*line).visible) {
                let row = StudyValue {
                    formatted: study
                        .plot_value(line, index)
                        .map(|value| study.format_plot_value(line, value)),
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
