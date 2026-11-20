//! Forecast tool rendering

use crate::drawings::domain::{CardColors, ChartPoint, Drawing};
use crate::drawings::rendering::measurements::{elapsed_ms, format_duration, framed_label};
use crate::drawings::rendering::{ChartView, bars_between};
use crate::localization as l10n;
use crate::model::{Bar, percent_change};
use crate::scales::{
    DefaultPriceFormatter, PriceFormatter, group_thousands, instrument_price_precision,
    price_decimals,
};
use crate::styles::stroke;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Align2, Color32, Pos2, Stroke};

impl Drawing {
    /// Renders a forecast from the start anchor to the target anchor, with the predicted change
    /// and whether the price has reached the target in time, in the cards' and badges' own
    /// colours.
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
        let built_in = DefaultPriceFormatter::with_precision(precision);
        let price = |value: f64| {
            view.price_formatter.map_or_else(
                || group_thousands(built_in.format(value), view.thousands_sep),
                |formatter| formatter.format(value),
            )
        };
        let anchor_label = |point: ChartPoint| {
            let mut text = price(point.price);
            if view.time_axis {
                let time = elapsed_ms(view.bars, 0.0, point.bar_idx)
                    .and_then(|ms| view.bars.first()?.time.timestamp_millis().checked_add(ms))
                    .and_then(chrono::DateTime::from_timestamp_millis);
                if let Some(time) = time {
                    let date = match view.time_formatter {
                        Some(formatter) => formatter.format_crosshair(time, view.bar_seconds),
                        None => time.format("%Y-%m-%d %H:%M").to_string(),
                    };
                    text.push('\n');
                    text.push_str(&date);
                }
            }
            text
        };
        let change = to.price - from.price;
        let change_text = price(change);
        let sign = if change_text.starts_with(['-', '+'])
            || (view.price_formatter.is_some() && change < 0.0)
        {
            ""
        } else {
            "+"
        };
        let moved = format!(
            "{sign}{change_text} ({:+.2}%)",
            percent_change(from.price, to.price)
        );
        let mut target =
            match elapsed_ms(view.bars, from.bar_idx, to.bar_idx).filter(|_| view.time_axis) {
                Some(ms) => l10n::format_text(
                    painter.ctx(),
                    "{change} in {duration}",
                    &[("change", moved), ("duration", format_duration(ms))],
                ),
                None => moved,
            };
        target.push('\n');
        target.push_str(&anchor_label(to));

        let up = change >= 0.0;
        let gap = DESIGN_TOKENS.spacing.md;
        let style = self.forecast_style();
        let [start_card, target_card, success_card, failure_card] = style.cards(&chart);
        let opacity = u16::from(100 - style.transparency.min(100));
        let label = |align, pos, text: String, colors: CardColors| {
            // Faded and rounded half up as the ghost feed fades its candles; 0% keeps each
            // colour exactly as given.
            let background = if opacity == 100 {
                colors.background
            } else {
                let [r, g, b, a] = colors.background.to_srgba_unmultiplied();
                let a = (u16::from(a) * opacity + 50) / 100;
                Color32::from_rgba_unmultiplied(r, g, b, a as u8)
            };
            let border = colors
                .border
                .map_or(Stroke::NONE, |color| Stroke::new(stroke::HAIRLINE, color));
            framed_label(painter, align, pos, text, colors.text, background, border)
        };
        label(
            if up {
                Align2::CENTER_TOP
            } else {
                Align2::CENTER_BOTTOM
            },
            start + egui::vec2(0.0, if up { gap } else { -gap }),
            anchor_label(from),
            start_card,
        );
        let outcome = forecast_outcome(view.bars, from, to).map(|success| {
            if success {
                ("SUCCESS", success_card)
            } else {
                ("FAILED", failure_card)
            }
        });
        let badge = |align, pos, status: &str, colors| {
            label(align, pos, l10n::text(painter.ctx(), status), colors)
        };
        let mut target_pos = end + egui::vec2(0.0, if up { -gap } else { gap });
        if up && let Some((status, colors)) = outcome {
            let rect = badge(Align2::CENTER_BOTTOM, target_pos, status, colors);
            target_pos.y = rect.top() - DESIGN_TOKENS.spacing.xs;
        }
        let rect = label(
            if up {
                Align2::CENTER_BOTTOM
            } else {
                Align2::CENTER_TOP
            },
            target_pos,
            target,
            target_card,
        );
        if !up && let Some((status, colors)) = outcome {
            badge(
                Align2::CENTER_TOP,
                Pos2::new(end.x, rect.bottom() + DESIGN_TOKENS.spacing.xs),
                status,
                colors,
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
                            thousands_sep: None,
                            time_formatter: None,
                            bar_seconds: 0,
                            instrument: Some(&instrument),
                            bars: &bars,
                            revision: 0,
                            bar_to_x: &|b| b,
                            bar_spacing: 1.0,
                            price_to_y: &|p| p as f32,
                            y_to_price: &f64::from,
                            time_axis,
                            executions: None,
                            timeframe: None,
                            finer: &[],
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

    /// Each card and badge paints its own background, text and border, the transparency fades
    /// the backgrounds alone, a dashed line keeps the borders solid and a forecast's own colours
    /// replace its fill. A new forecast paints blue cards with a blue border and white text and a
    /// green badge, faded by 10%; one saved before these settings reads back and paints as it
    /// did: in the theme's colours, or with its fill as every background.
    #[test]
    fn forecast_cards_and_badges_take_their_own_colours() {
        use crate::drawings::{DrawingToolType, ForecastStyle};
        use crate::model::{Instrument, LineStyle};
        use crate::theme::{Theme, apply_to_egui};
        use egui::{Rect, Shape};

        // A forecast as the release before these settings saved it, in a drawings file, a
        // template or the tool's defaults.
        let saved: Drawing = serde_json::from_str(r#"{"label":null,"pane":null,"selectable":true,"saveable":true,"listed":true,"user_edit_enabled":true,"group":null,"folder":[],"custom_tool":null,"custom_settings":null,"tool_type":"Forecast","screen_anchor":null,"color":[41,98,255,255],"stroke_width":2.0,"text":null,"post":null,"line_style":"Solid","fill_color":[12,34,56,80],"locked":false,"visible":true,"font_size":14.0,"font_weight":"Normal","italic":false,"text_color":null,"background":true,"border":true,"border_color":null,"text_wrap":false,"text_wrap_width":200.0,"text_follows_zoom":false,"text_zoom_spacing":null,"note_text_style":null,"extend_left":false,"extend_right":false,"arrow_start_style":"None","arrow_end_style":"None","timeframe_visibility":{"show_on_all":true,"visible_timeframes":[]},"price_axis_label":true,"time_axis_label":true,"line_measurements":{"persistent":false,"price_change":false,"percent_change":false,"pip_change":false,"bar_count":false,"elapsed_time":false,"length":false,"inclination":false,"midpoint":false,"price_labels":false,"position":"Right","text_horizontal_align":"Center","text_vertical_align":"Bottom","text_orientation":"Vertical","across_panes":false},"range_settings":{"price_change":true,"percent_change":true,"pip_change":true,"bar_count":true,"elapsed_time":true,"volume":true,"text_color":null,"font_size":12.0,"background":true,"background_color":null,"border":true,"border_color":null,"border_width":null,"border_style":null,"extend_top":false,"extend_bottom":false},"position_size":null,"position_sizing":{"point_value":1.0,"lot_size":1.0,"quantity_step":0.0,"leverage":1.0},"position_readout":{"persistent":false,"condensed":false,"account_currency":null,"conversion_rate":null,"quantity_decimals":null},"position_target_color":null,"position_stop_color":null,"position_style":{"line_color":null,"line_width":1.0,"text_color":null,"font_size":12.0,"label_background":true,"label_color":null,"fill":true,"border":false,"border_color":[102,123,139,255]},"marker":{"show_text":true,"glyph":"🙂","show_glyph":false,"glyph_color":null},"fib_config":null,"volume_profile":null,"regression_trend":null,"vwap":null,"gann":null,"ghost_feed":null,"channel_midline":false,"anchor_price_style":null,"middle_line":{"color":null,"width":1.0,"line_style":"Dashed"},"theme_shade":true,"image":null,"glyph_angle":0.0,"closed":false,"pitchfork":null,"elliott_degree":"Minor","elliott_show_wave":true,"bars_pattern":null,"projection":null,"z_order":0}"#).unwrap();
        assert_eq!(saved.forecast, None);
        let (rgb, rgba) = (Color32::from_rgb, Color32::from_rgba_unmultiplied);
        let every = ForecastStyle {
            start_background: Some([10, 20, 30, 255]),
            start_text: Some([70, 80, 90, 255]),
            start_border: Some([40, 50, 60, 255]),
            target_background: Some([100, 110, 120, 255]),
            target_text: Some([160, 170, 180, 255]),
            target_border: Some([130, 140, 150, 255]),
            success_background: Some([190, 200, 210, 255]),
            success_text: Some([220, 230, 240, 255]),
            failure_background: Some([1, 2, 3, 255]),
            failure_text: Some([4, 5, 6, 255]),
            transparency: 0,
        };
        let configured = |edit: fn(&mut ForecastStyle)| {
            let mut style = every;
            edit(&mut style);
            let mut drawing = Drawing::new(1, DrawingToolType::Forecast);
            drawing.forecast = Some(style);
            drawing
        };
        let fill = Some([12, 34, 56, 80]);
        let blue = rgb(41, 98, 255);
        let mut bars = sample_bars();
        for (i, bar) in bars.iter_mut().enumerate() {
            bar.time = chrono::DateTime::from_timestamp(i as i64 * 86_400, 0).unwrap();
            bar.open = 1.1;
            bar.close = 1.1;
            bar.low = 1.1;
            bar.high = 1.1002;
        }
        // Bar 2 reaches the target above; no bar by bar 3 reaches the one below.
        bars[2].high = 1.1006;
        let instrument = Instrument::new("PAIR").with_tick_size(0.00001);
        for theme in [Theme::light(), Theme::dark()] {
            let chart = theme.chart;
            // Background, text and border of the start card, the target card and the badge. Faded
            // by 40%, alpha 255 becomes (255 * 60 + 50) / 100 = 153 and alpha 128 becomes
            // (128 * 60 + 50) / 100 = 77. Fading by 10% gives (255 * 90 + 50) / 100 = 230.
            let cases = [
                (
                    "faded, dashed and filled",
                    true,
                    configured(|style| {
                        style.transparency = 40;
                        style.start_background = Some([10, 20, 30, 128]);
                    }),
                    fill,
                    LineStyle::Dashed,
                    [
                        (rgba(10, 20, 30, 77), rgb(70, 80, 90), Some(rgb(40, 50, 60))),
                        (
                            rgba(100, 110, 120, 153),
                            rgb(160, 170, 180),
                            Some(rgb(130, 140, 150)),
                        ),
                        (rgba(190, 200, 210, 153), rgb(220, 230, 240), None),
                    ],
                ),
                (
                    "failed, target border only",
                    false,
                    configured(|style| style.start_border = None),
                    fill,
                    LineStyle::Solid,
                    [
                        (rgb(10, 20, 30), rgb(70, 80, 90), None),
                        (
                            rgb(100, 110, 120),
                            rgb(160, 170, 180),
                            Some(rgb(130, 140, 150)),
                        ),
                        (rgb(1, 2, 3), rgb(4, 5, 6), None),
                    ],
                ),
                (
                    "background transparency clamps at 100",
                    true,
                    configured(|style| style.transparency = 255),
                    fill,
                    LineStyle::Dotted,
                    [
                        (Color32::TRANSPARENT, rgb(70, 80, 90), Some(rgb(40, 50, 60))),
                        (
                            Color32::TRANSPARENT,
                            rgb(160, 170, 180),
                            Some(rgb(130, 140, 150)),
                        ),
                        (Color32::TRANSPARENT, rgb(220, 230, 240), None),
                    ],
                ),
                (
                    "new",
                    true,
                    Drawing::new(1, DrawingToolType::Forecast),
                    None,
                    LineStyle::Solid,
                    [
                        (rgba(41, 98, 255, 230), Color32::WHITE, Some(blue)),
                        (rgba(41, 98, 255, 230), Color32::WHITE, Some(blue)),
                        (rgba(76, 175, 80, 230), Color32::WHITE, None),
                    ],
                ),
                (
                    "saved before the settings",
                    true,
                    saved.clone(),
                    None,
                    LineStyle::Solid,
                    [
                        (chart.bg_axis, chart.axis_text, None),
                        (chart.bg_axis, chart.axis_text, None),
                        (chart.bg_axis, chart.candle_up, None),
                    ],
                ),
                (
                    "saved before the settings, filled",
                    false,
                    saved.clone(),
                    fill,
                    LineStyle::Solid,
                    [
                        (rgba(12, 34, 56, 80), chart.axis_text, None),
                        (rgba(12, 34, 56, 80), chart.axis_text, None),
                        (rgba(12, 34, 56, 80), chart.candle_down, None),
                    ],
                ),
            ];
            for (case, up, mut drawing, fill, line_style, cards) in cases {
                drawing.fill_color = fill;
                drawing.line_style = line_style;
                let target = if up { 1.10055 } else { 1.09945 };
                drawing.chart_points =
                    vec![ChartPoint::new(0.0, 1.1), ChartPoint::new(3.0, target)];
                drawing.points = vec![
                    Pos2::new(200.0, 300.0),
                    Pos2::new(600.0, if up { 200.0 } else { 400.0 }),
                ];
                let ctx = egui::Context::default();
                apply_to_egui(&ctx, &theme);
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    drawing.render_with_selection(
                        ui.painter(),
                        Rect::from_min_max(Pos2::ZERO, Pos2::new(800.0, 600.0)),
                        &ChartView {
                            price_formatter: None,
                            thousands_sep: None,
                            time_formatter: None,
                            bar_seconds: 0,
                            instrument: Some(&instrument),
                            bars: &bars,
                            revision: 0,
                            bar_to_x: &|b| b,
                            bar_spacing: 1.0,
                            price_to_y: &|p| p as f32,
                            y_to_price: &f64::from,
                            time_axis: true,
                            executions: None,
                            timeframe: None,
                            finer: &[],
                        },
                        false,
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
                assert_eq!((texts.len(), rects.len()), (3, 3), "{case}");
                let labels = [
                    "1.10000\n1970-01-01 00:00",
                    if up {
                        "+0.00055 (+0.05%) in 3d\n1.10055\n1970-01-04 00:00"
                    } else {
                        "-0.00055 (-0.05%) in 3d\n1.09945\n1970-01-04 00:00"
                    },
                    if up { "SUCCESS" } else { "FAILED" },
                ];
                for (label, (background, text, border)) in labels.into_iter().zip(cards) {
                    let shape = texts
                        .iter()
                        .find(|t| t.galley.job.text == label)
                        .unwrap_or_else(|| panic!("{case}: missing {label:?}"));
                    assert_eq!(
                        shape.galley.job.sections[0].format.color, text,
                        "{case}: text of {label:?}"
                    );
                    let card = rects
                        .iter()
                        .find(|r| r.rect.contains(shape.pos))
                        .unwrap_or_else(|| panic!("{case}: no card under {label:?}"));
                    let border = border.map_or(Stroke::NONE, |color| Stroke::new(1.0, color));
                    assert_eq!(
                        (card.fill, card.stroke),
                        (background, border),
                        "{case}: card of {label:?}"
                    );
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
