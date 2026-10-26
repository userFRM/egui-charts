//! Study readouts for host data windows and chart tooltips.

use super::{Chart, SeriesId};
use crate::chart::renderers::StudyValue;

impl Chart {
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
