//! Study readouts for host data windows and chart tooltips.

use super::Chart;
use crate::chart::renderers::StudyValue;

impl Chart {
    /// Values of visible study plots at a rendered bar index, with legend names and precision.
    /// Gaps remain `None`; price-based charts return no study values because their indices count
    /// rendered elements rather than source bars. Call after [`Self::show`] to use refreshed studies.
    pub fn study_values_at(&self, index: usize) -> Vec<StudyValue> {
        if self.chart_type.is_time_independent() || index >= self.state.shown().len() {
            return Vec::new();
        }
        let mut rows = Vec::new();
        for study in self
            .studies
            .indicators()
            .iter()
            .filter(|study| study.is_visible())
        {
            let names = study.line_names();
            let colors = study.colors();
            for line in (0..study.line_cnt()).filter(|line| study.plot_style(*line).visible) {
                rows.push(StudyValue {
                    study: study.name().to_owned(),
                    format: study.value_format(),
                    plot: names.get(line).cloned().unwrap_or_default(),
                    value: study.plot_value(line, index),
                    precision: study
                        .precision()
                        .unwrap_or_else(|| match study.value_format() {
                            crate::studies::StudyFormat::Price => self
                                .instrument()
                                .and_then(crate::scales::instrument_price_precision)
                                .map_or(2, usize::from),
                            crate::studies::StudyFormat::Fixed(decimals) => decimals.min(12),
                            crate::studies::StudyFormat::Volume
                                if study
                                    .plot_value(line, index)
                                    .is_some_and(|value| value.abs() < 1_000.0) =>
                            {
                                0
                            }
                            _ => 2,
                        }),
                    color: colors.get(line).copied().unwrap_or(self.config.text_color),
                });
            }
        }
        rows
    }
}
