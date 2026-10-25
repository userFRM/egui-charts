//! Text equivalent of the chart's series and cursor readout.

use super::Chart;
use egui::{Role, WidgetInfo};
use std::fmt::Write;

impl Chart {
    pub(super) fn accessibility_info(&self, enabled: bool) -> WidgetInfo {
        let mut info = WidgetInfo::labeled(
            Role::GraphicsDocument,
            enabled,
            format!("{} chart", self.symbol()).trim(),
        );
        let bars = self.state.shown();
        let mut text = self.chart_type.as_str().to_owned();
        if let Some(timeframe) = self.timeframe() {
            let _ = write!(text, ", {timeframe}");
        }
        let _ = write!(text, ". {} bars", bars.len());
        if let Some((first, last)) = bars.first().zip(bars.last()) {
            let _ = write!(
                text,
                ", {} to {}",
                first.time.to_rfc3339(),
                last.time.to_rfc3339()
            );
        } else {
            text.push_str(". No data");
        }
        let hovered = self
            .last_hover_bar_idx
            .filter(|index| index.is_finite() && *index >= 0.0)
            .map(|index| index.round() as usize)
            .filter(|index| *index < bars.len());
        if let Some(index) = hovered.or_else(|| bars.iter().rposition(|bar| !bar.is_whitespace())) {
            let bar = &bars[index];
            let _ = write!(
                text,
                ". {}: {}",
                if hovered.is_some() {
                    "Hovered bar"
                } else {
                    "Latest bar"
                },
                bar.time.to_rfc3339()
            );
            if self.main_series_visible && bar.is_whitespace() {
                text.push_str(". No value");
            } else if self.main_series_visible {
                let decimals = self
                    .instrument()
                    .and_then(crate::scales::instrument_price_precision)
                    .map_or(2, usize::from);
                let _ = write!(
                    text,
                    ". Open {:.decimals$}, high {:.decimals$}, low {:.decimals$}, close {:.decimals$}, volume {}",
                    bar.open, bar.high, bar.low, bar.close, bar.volume
                );
            } else {
                text.push_str(". Main series hidden");
            }
            for study in self.study_values_at(index) {
                let value = if study.value.is_some_and(f64::is_finite) {
                    study.formatted_value()
                } else {
                    "No value".to_owned()
                };
                let _ = write!(text, ". {}: {value}", study.label());
            }
        } else if !bars.is_empty() {
            text.push_str(". No values");
        }
        info.current_text_value = Some(text);
        info
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Bar, BarData, Timeframe};
    use chrono::{TimeZone, Utc};
    use egui_kittest::{
        Harness,
        kittest::{NodeT, Queryable},
    };

    #[test]
    fn chart_summary_tracks_hover_updates_and_empty_history() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for width in [320.0, 800.0] {
                let mut chart = Chart::new(BarData::from_bars(vec![
                    Bar::new(
                        Utc.timestamp_opt(0, 0).unwrap(),
                        10.0,
                        15.0,
                        8.0,
                        12.0,
                        100.0,
                    ),
                    Bar::new(
                        Utc.timestamp_opt(60, 0).unwrap(),
                        12.0,
                        18.0,
                        11.0,
                        16.0,
                        200.0,
                    ),
                ]));
                #[cfg(feature = "studies")]
                chart.add_indicator(Box::new(crate::studies::SMA::new(2)));
                chart.set_symbol("TEST");
                chart.set_timeframe(Timeframe::Min1);
                let theme = theme.clone();
                let mut harness = Harness::builder()
                    .allow_missing_glyphs()
                    .with_context_setup(move |ctx| crate::theme::apply_to_egui(ctx, &theme))
                    .with_size(egui::vec2(width, 600.0))
                    .build_ui_state(
                        |ui, chart| {
                            chart.show(ui);
                        },
                        chart,
                    );
                harness.run();
                let summary = |harness: &Harness<'_, Chart>| {
                    harness
                        .get_by_role_and_label(Role::GraphicsDocument, "TEST chart")
                        .accesskit_node()
                        .value()
                        .unwrap()
                };
                assert!(
                    summary(&harness)
                        .contains("2 bars, 1970-01-01T00:00:00+00:00 to 1970-01-01T00:01:00+00:00")
                );
                assert!(summary(&harness).contains("Latest bar: 1970-01-01T00:01:00+00:00. Open 12.00, high 18.00, low 11.00, close 16.00, volume 200"));
                #[cfg(feature = "studies")]
                assert!(summary(&harness).contains("SMA: SMA(2): 14.00"));
                let map = harness.state().get_chart_mapping();
                harness.hover_at(egui::pos2(map.idx_to_x(0), map.rect.center().y));
                harness.run();
                assert!(summary(&harness).contains("Hovered bar: 1970-01-01T00:00:00+00:00. Open 10.00, high 15.00, low 8.00, close 12.00, volume 100"));
                #[cfg(feature = "studies")]
                assert!(summary(&harness).contains("SMA: SMA(2): No value"));
                harness.remove_cursor();
                harness.run();
                assert!(summary(&harness).contains("Latest bar:"));
                harness.state_mut().update(
                    super::super::SeriesId::MAIN,
                    Bar::new(
                        Utc.timestamp_opt(60, 0).unwrap(),
                        12.0,
                        19.0,
                        11.0,
                        17.0,
                        250.0,
                    ),
                );
                harness.run();
                assert!(
                    summary(&harness)
                        .contains("Open 12.00, high 19.00, low 11.00, close 17.00, volume 250")
                );
                #[cfg(feature = "studies")]
                assert!(summary(&harness).contains("SMA: SMA(2): 14.50"));
                let gap = Bar::whitespace(Utc.timestamp_opt(120, 0).unwrap());
                let mut bars = harness.state().bars(super::super::SeriesId::MAIN).to_vec();
                bars.push(gap.clone());
                harness
                    .state_mut()
                    .set_data(super::super::SeriesId::MAIN, bars);
                harness.run();
                assert!(summary(&harness).contains("3 bars"));
                assert!(summary(&harness).contains("Latest bar: 1970-01-01T00:01:00+00:00"));
                let map = harness.state().get_chart_mapping();
                harness.hover_at(egui::pos2(map.idx_to_x(2), map.rect.center().y));
                harness.run();
                assert!(
                    summary(&harness).contains("Hovered bar: 1970-01-01T00:02:00+00:00. No value")
                );
                assert!(!summary(&harness).contains("NaN"));
                harness.remove_cursor();
                harness
                    .state_mut()
                    .set_data(super::super::SeriesId::MAIN, vec![gap]);
                harness.run();
                assert!(summary(&harness).contains("No values"));
                assert!(!summary(&harness).contains("NaN"));
                harness
                    .state_mut()
                    .set_data(super::super::SeriesId::MAIN, Vec::new());
                harness.run();
                assert!(summary(&harness).contains("0 bars. No data"));
                assert!(!summary(&harness).contains("Open"));
            }
        }
    }
}
