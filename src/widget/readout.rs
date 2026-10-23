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
                    plot: names.get(line).cloned().unwrap_or_default(),
                    value: study.values().value(line, index),
                    precision: study.precision().unwrap_or_else(|| {
                        if study.is_overlay() {
                            self.instrument()
                                .and_then(|instrument| instrument.price_decimals)
                                .map_or_else(
                                    || {
                                        crate::chart::renderers::price_decimals(
                                            self.state.shown()[index].close,
                                        )
                                    },
                                    usize::from,
                                )
                                .min(8)
                        } else {
                            2
                        }
                    }),
                    color: colors.get(line).copied().unwrap_or(self.config.text_color),
                });
            }
        }
        rows
    }
}
