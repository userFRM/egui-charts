//! Forecast tool rendering

use crate::drawings::domain::{ChartPoint, Drawing};
use crate::drawings::rendering::measurements::{boxed_label, elapsed_ms, format_duration};
use crate::drawings::rendering::{ChartView, bars_between};
use crate::model::{Bar, percent_change};
use crate::scales::{
    DefaultPriceFormatter, PriceFormatter, instrument_price_precision, price_decimals,
};
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Align2, Pos2};

impl Drawing {
    /// Renders a forecast from the start anchor to the target anchor, with the
    /// predicted change and whether the price has reached the target in time.
    pub(crate) fn render_forecast(&self, painter: &egui::Painter, view: &ChartView) {
        let (&[start, end, ..], &[from, to, ..]) =
            (self.points.as_slice(), self.chart_points.as_slice())
        else {
            return;
        };

        self.draw_projection_main_line(painter, start, end, self.color32());

        let chart = painter.ctx().active_theme().chart;
        let precision = view
            .instrument
            .and_then(instrument_price_precision)
            .map_or_else(|| price_decimals(from.price), usize::from);
        let formatter = DefaultPriceFormatter::with_precision(precision);
        let anchor_label = |point: ChartPoint| {
            let mut text = formatter.format(point.price);
            if view.time_axis {
                let time = elapsed_ms(view.bars, 0.0, point.bar_idx)
                    .and_then(|ms| view.bars.first()?.time.timestamp_millis().checked_add(ms))
                    .and_then(chrono::DateTime::from_timestamp_millis);
                if let Some(time) = time {
                    text.push_str(&format!("\n{}", time.format("%Y-%m-%d %H:%M")));
                }
            }
            text
        };
        let change = to.price - from.price;
        let change_text = formatter.format(change);
        let sign = if change_text.starts_with('-') {
            ""
        } else {
            "+"
        };
        let mut target = format!(
            "{sign}{change_text} ({:+.2}%)",
            percent_change(from.price, to.price)
        );
        if let Some(ms) = elapsed_ms(view.bars, from.bar_idx, to.bar_idx).filter(|_| view.time_axis)
        {
            target.push_str(&format!(" in {}", format_duration(ms)));
        }
        target.push_str(&format!("\n{}", anchor_label(to)));

        let up = change >= 0.0;
        let gap = DESIGN_TOKENS.spacing.md;
        boxed_label(
            painter,
            if up {
                Align2::CENTER_TOP
            } else {
                Align2::CENTER_BOTTOM
            },
            start + egui::vec2(0.0, if up { gap } else { -gap }),
            anchor_label(from),
            chart.axis_text,
            chart.bg_axis,
        );
        let outcome = forecast_outcome(view.bars, from, to).map(|success| {
            if success {
                ("SUCCESS", chart.candle_up)
            } else {
                ("FAILED", chart.candle_down)
            }
        });
        let badge = |align, pos, status: &str, color| {
            boxed_label(painter, align, pos, status.to_owned(), color, chart.bg_axis)
        };
        let mut target_pos = end + egui::vec2(0.0, if up { -gap } else { gap });
        if up && let Some((status, color)) = outcome {
            let rect = badge(Align2::CENTER_BOTTOM, target_pos, status, color);
            target_pos.y = rect.top() - DESIGN_TOKENS.spacing.xs;
        }
        let rect = boxed_label(
            painter,
            if up {
                Align2::CENTER_BOTTOM
            } else {
                Align2::CENTER_TOP
            },
            target_pos,
            target,
            chart.axis_text,
            chart.bg_axis,
        );
        if !up && let Some((status, color)) = outcome {
            badge(
                Align2::CENTER_TOP,
                Pos2::new(end.x, rect.bottom() + DESIGN_TOKENS.spacing.xs),
                status,
                color,
            );
        }
    }
}

/// Whether the price reached the target after the start bar and by the
/// target bar: `Some(true)` once a bar trades at the target, `Some(false)`
/// once a bar after the target bar exists without that, `None` while the
/// target bar is ahead or still forming (or the target is not after the start).
fn forecast_outcome(bars: &[Bar], from: ChartPoint, to: ChartPoint) -> Option<bool> {
    let (start, end) = (from.bar_idx.round(), to.bar_idx.round());
    if end <= start {
        return None;
    }
    let (_, window) = bars_between(bars, start + 1.0, end)?;
    let up = to.price >= from.price;
    let reached = window.iter().any(|bar| {
        if up {
            bar.high >= to.price
        } else {
            bar.low <= to.price
        }
    });
    if reached {
        Some(true)
    } else {
        ((end as usize).saturating_add(1) < bars.len()).then_some(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::rendering::sample_bars;

    #[test]
    fn forecast_cards_show_prices_dates_direction_and_only_decided_outcomes() {
        use crate::drawings::DrawingToolType;
        use crate::model::Instrument;
        use crate::theme::{Theme, apply_to_egui};
        use egui::{Pos2, Rect, Shape};

        for theme in [Theme::light(), Theme::dark()] {
            for (target_bar, target_price, reached, time_axis, target, badge) in [
                (
                    9.0,
                    1.10055,
                    false,
                    true,
                    "+0.00055 (+0.05%) in 57d\n1.10055\n1970-02-27 00:00",
                    None,
                ),
                (
                    3.0,
                    1.10055,
                    true,
                    true,
                    "+0.00055 (+0.05%) in 3d\n1.10055\n1970-01-04 00:00",
                    Some("SUCCESS"),
                ),
                (
                    3.0,
                    1.09945,
                    false,
                    true,
                    "-0.00055 (-0.05%) in 3d\n1.09945\n1970-01-04 00:00",
                    Some("FAILED"),
                ),
                (
                    9.0,
                    1.10055,
                    false,
                    false,
                    "+0.00055 (+0.05%)\n1.10055",
                    None,
                ),
                (
                    9.0,
                    1000000.12345,
                    false,
                    true,
                    "+999999.02345 (+90909002.13%) in 57d\n1000000.12345\n1970-02-27 00:00",
                    None,
                ),
            ] {
                let mut bars = sample_bars();
                for (i, bar) in bars.iter_mut().enumerate() {
                    bar.time = chrono::DateTime::from_timestamp(i as i64 * 86_400, 0).unwrap();
                    bar.open = 1.1;
                    bar.close = 1.1;
                    bar.low = 1.1;
                    bar.high = 1.1002;
                }
                // A gap at the edge must not turn each future bar into another 49 days.
                bars[6].time = chrono::DateTime::from_timestamp(54 * 86_400, 0).unwrap();
                if reached {
                    bars[2].high = 1.1006;
                }
                let instrument = Instrument::new("PAIR").with_tick_size(0.00001);
                let mut drawing = Drawing::new(1, DrawingToolType::Forecast);
                drawing.chart_points = vec![
                    ChartPoint::new(0.0, 1.1),
                    ChartPoint::new(target_bar, target_price),
                ];
                drawing.points = vec![
                    Pos2::new(200.0, 300.0),
                    Pos2::new(600.0, if target_price >= 1.1 { 200.0 } else { 400.0 }),
                ];
                let ctx = egui::Context::default();
                apply_to_egui(&ctx, &theme);
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    drawing.render_forecast(
                        ui.painter(),
                        &ChartView {
                            price_formatter: None,
                            instrument: Some(&instrument),
                            bars: &bars,
                            revision: 0,
                            bar_to_x: &|b| b,
                            price_to_y: &|p| p as f32,
                            y_to_price: &f64::from,
                            time_axis,
                            executions: None,
                        },
                    );
                });
                output.textures_delta.clear();
                let texts: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        Shape::Text(t) => Some(t),
                        _ => None,
                    })
                    .collect();
                let rects: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        Shape::Rect(r) => Some(r),
                        _ => None,
                    })
                    .collect();
                let label = |text: &str| {
                    texts
                        .iter()
                        .find(|t| t.galley.job.text == text)
                        .unwrap_or_else(|| {
                            panic!(
                                "missing {text:?}: {:?}",
                                texts.iter().map(|t| &t.galley.job.text).collect::<Vec<_>>()
                            )
                        })
                };
                let source = if time_axis {
                    "1.10000\n1970-01-01 00:00"
                } else {
                    "1.10000"
                };
                label(source);
                // 0.00055 / 1.1 * 100 = 0.05%; bar 9 is day 54 + 3 = 57.
                let target_text = label(target);
                assert_eq!(texts.len(), 2 + usize::from(badge.is_some()));
                assert_eq!(rects.len(), texts.len());
                for text in &texts {
                    let bounds = Rect::from_min_size(text.pos, text.galley.size());
                    assert!(
                        rects.iter().any(|r| r.rect.contains_rect(bounds)),
                        "card must contain its text"
                    );
                }
                for text in [label(source), target_text] {
                    assert_eq!(
                        text.galley.job.sections[0].format.color,
                        theme.chart.axis_text
                    );
                    assert!(
                        rects
                            .iter()
                            .any(|r| r.fill == theme.chart.bg_axis && r.rect.contains(text.pos))
                    );
                }
                if let Some(badge) = badge {
                    assert!(label(badge).pos.y > target_text.pos.y + target_text.galley.size().y);
                }
                let lines: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|s| match s.shape {
                        Shape::LineSegment { points, .. } => Some(points),
                        _ => None,
                    })
                    .collect();
                assert_eq!(lines.len(), 3, "line and two arrowhead wings");
                let end = drawing.points[1];
                let direction = (end - drawing.points[0]).normalized();
                for wing in &lines[1..] {
                    assert_eq!(wing[0], end);
                    assert!((wing[1].distance(end) - 12.0).abs() < 0.001);
                    assert!((wing[1] - end).dot(direction) < 0.0);
                }
                output.drop_without_applying_deltas();
            }
        }
    }

    /// Expected verdicts read off the sample bars by hand.
    #[test]
    fn forecast_outcome_checks_bars_up_to_target() {
        let bars = sample_bars();
        let cases = [
            // Up from bar 0: bar 3 (high 14) reaches 13.8 by bar 3.
            ((0.0, 11.0), (3.0, 13.8), Some(true)),
            // Up from bar 0: no high reaches 14.5 by bar 3.
            ((0.0, 11.0), (3.0, 14.5), Some(false)),
            // Target bar 9 is still ahead, but bar 4 (high 15) already reached 14.5.
            ((0.0, 11.0), (9.0, 14.5), Some(true)),
            // Target bar 9 is still ahead and no high reaches 16 yet.
            ((0.0, 11.0), (9.0, 16.0), None),
            // Down from bar 3: bar 5 (low 12) reaches 12.2.
            ((3.0, 13.5), (5.0, 12.2), Some(true)),
            // Down from bar 3: no low reaches 11.5 by bar 5.
            ((3.0, 13.5), (5.0, 11.5), Some(false)),
            // Target bar 6 is the last, still forming bar: not decided yet.
            ((3.0, 13.5), (6.0, 11.5), None),
        ];
        for ((from_bar, from_price), (to_bar, to_price), want) in cases {
            let from = ChartPoint::new(from_bar, from_price);
            let to = ChartPoint::new(to_bar, to_price);
            assert_eq!(
                forecast_outcome(&bars, from, to),
                want,
                "{from:?} -> {to:?}"
            );
        }
    }
}
