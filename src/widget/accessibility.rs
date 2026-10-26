//! Text equivalent of the chart's series and cursor readout.

use super::Chart;
use crate::localization as l10n;
use egui::{Role, WidgetInfo};
use std::fmt::Write;

impl Chart {
    pub(super) fn accessibility_info(&self, ctx: &egui::Context, enabled: bool) -> WidgetInfo {
        let mut info = WidgetInfo::labeled(
            Role::GraphicsDocument,
            enabled,
            l10n::format_text(
                ctx,
                "{symbol} chart",
                &[("symbol", self.symbol().to_owned())],
            )
            .trim(),
        );
        let bars = self.state.shown();
        let mut text = l10n::text(ctx, self.chart_type.as_str());
        if let Some(timeframe) = self.timeframe() {
            let _ = write!(text, ", {timeframe}");
        }
        text.push_str(&l10n::format_text(
            ctx,
            ". {count} bars",
            &[("count", bars.len().to_string())],
        ));
        if let Some((first, last)) = bars.first().zip(bars.last()) {
            text.push_str(&l10n::format_text(
                ctx,
                ", {first} to {last}",
                &[
                    ("first", first.time.to_rfc3339()),
                    ("last", last.time.to_rfc3339()),
                ],
            ));
        } else {
            text.push_str(&l10n::text(ctx, ". No data"));
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
                l10n::text(
                    ctx,
                    if hovered.is_some() {
                        "Hovered bar"
                    } else {
                        "Latest bar"
                    }
                ),
                bar.time.to_rfc3339()
            );
            if self.main_series_visible && bar.is_whitespace() {
                text.push_str(&l10n::text(ctx, ". No value"));
            } else if self.main_series_visible {
                let decimals = self
                    .instrument()
                    .and_then(crate::scales::instrument_price_precision)
                    .map_or(2, usize::from);
                text.push_str(&l10n::format_text(
                    ctx,
                    ". Open {open}, high {high}, low {low}, close {close}, volume {volume}",
                    &[
                        ("open", format!("{:.decimals$}", bar.open)),
                        ("high", format!("{:.decimals$}", bar.high)),
                        ("low", format!("{:.decimals$}", bar.low)),
                        ("close", format!("{:.decimals$}", bar.close)),
                        ("volume", bar.volume.to_string()),
                    ],
                ));
            } else {
                text.push_str(&l10n::text(ctx, ". Main series hidden"));
            }
            for study in self.study_values_at(index) {
                let value = if study.value.is_some_and(f64::is_finite) {
                    study.formatted_value()
                } else {
                    l10n::text(ctx, "No value")
                };
                let _ = write!(text, ". {}: {value}", study.label());
            }
        } else if !bars.is_empty() {
            text.push_str(&l10n::text(ctx, ". No values"));
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
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    #[test]
    fn summary_translation_runs_only_when_consumed() {
        let ctx = egui::Context::default();
        let calls = Arc::new(AtomicUsize::new(0));
        let lookup_calls = calls.clone();
        let lookup_ctx = ctx.clone();
        l10n::set_translator(&ctx, move |english| {
            let _ = lookup_ctx.global_style();
            if english == "{symbol} chart" {
                lookup_calls.fetch_add(1, Ordering::Relaxed);
                Some("Graphique {symbol}".into())
            } else {
                None
            }
        });
        let mut chart = super::super::test_frames::chart(&[10.0], &[12.0]);
        chart.set_symbol("TEST");
        let mut draw = |events| {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    #[cfg(debug_assertions)]
                    {
                        ui.style_mut().debug.show_interactive_widgets = false;
                    }
                    chart.show(ui);
                },
            )
        };
        draw(Vec::new()).drop_without_applying_deltas();
        draw(Vec::new()).drop_without_applying_deltas();
        assert_eq!(calls.load(Ordering::Relaxed), 0);

        let pos = egui::pos2(300.0, 200.0);
        let button = |pressed| egui::Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        draw(vec![egui::Event::PointerMoved(pos)]).drop_without_applying_deltas();
        draw(vec![button(true)]).drop_without_applying_deltas();
        let output = draw(vec![button(false)]);
        assert!(output.platform_output.events.iter().any(|event| {
            matches!(event, egui::output::OutputEvent::Clicked(info)
                if info.label.as_deref() == Some("Graphique TEST")
                    && info.current_text_value.as_deref().is_some_and(|text| text.contains("100 bars")))
        }));
        output.drop_without_applying_deltas();
        assert!(calls.swap(0, Ordering::Relaxed) > 0);

        draw(Vec::new()).drop_without_applying_deltas();
        assert_eq!(calls.load(Ordering::Relaxed), 0);
        ctx.enable_accesskit();
        draw(Vec::new()).drop_without_applying_deltas();
        assert!(calls.swap(0, Ordering::Relaxed) > 0);
        ctx.disable_accesskit();
        draw(Vec::new()).drop_without_applying_deltas();
        assert_eq!(calls.load(Ordering::Relaxed), 0);

        #[cfg(debug_assertions)]
        {
            ctx.global_style_mut(|style| style.debug.show_interactive_widgets = true);
            draw(Vec::new()).drop_without_applying_deltas();
            assert!(calls.load(Ordering::Relaxed) > 0);
        }
        l10n::clear_translator(&ctx);
    }

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
                l10n::set_translator(&harness.ctx, |english| match english {
                    "{symbol} chart" => Some("Graphique {symbol}".into()),
                    ". {count} bars" => Some(". {count} barres".into()),
                    ". No data" => Some(". Aucune donnee".into()),
                    _ => None,
                });
                harness.run();
                let chart =
                    harness.get_by_role_and_label(egui::Role::GraphicsDocument, "Graphique TEST");
                assert!(
                    chart
                        .accesskit_node()
                        .value()
                        .unwrap()
                        .contains("0 barres. Aucune donnee")
                );
                l10n::clear_translator(&harness.ctx);
            }
        }
    }
}
