//! Trade profiles in the price pane, aligned with the bars that contributed their volume.

use crate::chart::coords::ChartMapping;
use crate::chart::overlays::{FootprintConfig, FootprintMode};
use crate::model::Bar;
use crate::studies::TradeProfile;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Align2, FontId, Painter, Rect, Stroke, pos2};

/// Draws bid/ask stacks, total volume or signed delta profiles, their POC, value area,
/// developing POC and volume nodes. `step` is the footprint's grouped price step and
/// `tick` is its instrument tick size before grouping. Bands extend half a tick below
/// their first tick and above their last tick.
/// Histograms of partly visible sessions stay at the left edge of the viewport.
/// Profile ranges and data remain host-owned; edit [`crate::studies::TradeProfileRanges`]
/// in response to a session menu's merge/split actions and recalculate after trades change.
pub fn render_trade_profiles(
    painter: &Painter,
    mapping: &ChartMapping,
    bars: &[Bar],
    profiles: &[TradeProfile],
    step: f64,
    tick: f64,
    config: &FootprintConfig,
) {
    render_profiles(painter, mapping, bars, profiles, step, tick, config, false);
}

/// The standalone chart keeps session framing and its quiet defaults out of overlays.
pub(crate) fn render_session_volume_profiles(
    painter: &Painter,
    mapping: &ChartMapping,
    bars: &[Bar],
    profiles: &[TradeProfile],
    step: f64,
    tick: f64,
) {
    let theme = painter.ctx().active_theme();
    let config = FootprintConfig {
        ask_color: DESIGN_TOKENS.semantic.brand.accent,
        bid_color: DESIGN_TOKENS.semantic.tpo.period_6,
        neutral_color: theme.chart.axis_text_secondary,
        text_color: DESIGN_TOKENS.semantic.chart.bearish,
        value_area_high: None,
        value_area_low: None,
        min_text_height: f32::INFINITY,
        ..Default::default()
    };
    render_profiles(painter, mapping, bars, profiles, step, tick, &config, true);
}

fn render_profiles(
    painter: &Painter,
    mapping: &ChartMapping,
    bars: &[Bar],
    profiles: &[TradeProfile],
    step: f64,
    tick: f64,
    config: &FootprintConfig,
    session: bool,
) {
    if !step.is_finite() || step <= 0.0 || !tick.is_finite() || tick <= 0.0 || bars.is_empty() {
        return;
    }
    let painter = painter.with_clip_rect(mapping.rect.intersect(painter.clip_rect()));
    let theme = painter.ctx().active_theme();
    let pixel = 1.0 / painter.pixels_per_point();
    for profile in profiles {
        let first = bars.partition_point(|b| b.time < profile.range.start);
        let end = bars.partition_point(|b| b.time < profile.range.end);
        if first >= end {
            continue;
        }
        let x = mapping.idx_to_x(first) - mapping.bar_spacing * 0.45;
        if mapping.idx_to_x(end - 1) < mapping.rect.left() || x > mapping.rect.right() {
            continue;
        }
        let x = x.max(mapping.rect.left());
        let session_width = (mapping.idx_to_x(end - 1) + mapping.bar_spacing * 0.5 - x).max(1.0);
        let width = if session {
            session_width
        } else {
            ((end - first) as f32 * mapping.bar_spacing * 0.7)
                .clamp(1.0, 160.0)
                .min(session_width)
        };
        let max = profile
            .volume
            .levels
            .values()
            .map(|r| r.volume())
            .fold(0.0, f64::max);
        if max <= 0.0 {
            continue;
        }
        if session
            && let (Some((&low, _)), Some((&high, _))) = (
                profile.volume.levels.first_key_value(),
                profile.volume.levels.last_key_value(),
            )
        {
            let y0 = mapping.price_to_y(low as f64 * step - tick * 0.5);
            let y1 = mapping.price_to_y((high as f64 + 1.0) * step - tick * 0.5);
            if y0.is_finite() && y1.is_finite() {
                let fill = theme
                    .chart
                    .bg
                    .lerp_to_gamma(theme.chart.axis_text, 0.035)
                    .lerp_to_gamma(config.ask_color, if theme.dark { 0.065 } else { 0.035 });
                painter.rect_filled(
                    Rect::from_min_max(pos2(x, y0.min(y1)), pos2(x + width, y0.max(y1))),
                    0.0,
                    fill,
                );
            }
        }
        let area = config.value_area.and_then(|s| profile.volume.value_area(s));
        let (high, low) = profile.nodes(config.node_sensitivity);
        let poc = profile.volume.poc();
        for row in profile.volume.levels.values() {
            let y0 = mapping.price_to_y(row.level as f64 * step - tick * 0.5);
            let y1 = mapping.price_to_y((row.level as f64 + 1.0) * step - tick * 0.5);
            if !y0.is_finite() || !y1.is_finite() {
                continue;
            }
            let mut top = y0.min(y1);
            let mut bottom = y0.max(y1);
            // Keep a physical-pixel gutter only when the row has room for it.
            if bottom - top >= 2.0 * pixel {
                top = (top / pixel).round() * pixel;
                bottom = (bottom / pixel).round() * pixel - pixel;
            }
            let alpha = match (
                area.is_some_and(|(a, b)| (a..=b).contains(&row.level)),
                theme.dark,
            ) {
                (true, true) => 0.9,
                (true, false) => 0.82,
                (false, true) => 0.32,
                (false, false) => 0.24,
            };
            let mut left = x;
            let mode = config
                .modes
                .first()
                .copied()
                .unwrap_or(FootprintMode::BidAsk);
            let segments = match mode {
                FootprintMode::BidAsk if session => vec![
                    (row.ask, config.ask_color),
                    (row.bid, config.bid_color),
                    (row.unknown, config.neutral_color),
                ],
                FootprintMode::BidAsk => vec![
                    (row.bid, config.bid_color),
                    (row.ask, config.ask_color),
                    (row.unknown, config.neutral_color),
                ],
                FootprintMode::Volume => vec![(row.volume(), config.neutral_color)],
                _ => {
                    left += width * 0.5;
                    vec![(
                        row.delta().abs(),
                        if row.delta() >= 0.0 {
                            config.ask_color
                        } else {
                            config.bid_color
                        },
                    )]
                }
            };
            for (volume, color) in segments {
                let w = width
                    * (volume / max) as f32
                    * if matches!(mode, FootprintMode::Delta | FootprintMode::DeltaVolume) {
                        0.5
                    } else {
                        1.0
                    };
                let right = if row.delta() < 0.0
                    && matches!(mode, FootprintMode::Delta | FootprintMode::DeltaVolume)
                {
                    left - w
                } else {
                    left + w
                };
                painter.rect_filled(
                    Rect::from_min_max(pos2(left.min(right), top), pos2(left.max(right), bottom)),
                    0.0,
                    color.gamma_multiply(alpha),
                );
                left = right;
            }
            if config.show_poc && poc == Some(row.level) {
                painter.line_segment(
                    [pos2(x, (y0 + y1) * 0.5), pos2(x + width, (y0 + y1) * 0.5)],
                    Stroke::new(DESIGN_TOKENS.stroke.hairline, config.text_color),
                );
            }
        }
        // A valley can lie on untraded rows, so nodes are placed by level, not by traded row.
        for (label, nodes) in [("HVN", &high), ("LVN", &low)] {
            for &level in nodes {
                let y0 = mapping.price_to_y(level as f64 * step - tick * 0.5);
                let y1 = mapping.price_to_y((level as f64 + 1.0) * step - tick * 0.5);
                if y0.is_finite() && y1.is_finite() && (y0 - y1).abs() >= config.min_text_height {
                    painter.text(
                        pos2(x + width + 3.0, (y0 + y1) * 0.5),
                        Align2::LEFT_CENTER,
                        label,
                        FontId::monospace(crate::styles::typography::TINY),
                        config.text_color,
                    );
                }
            }
        }
        // The guides sit on the edges of the rows the shading covers.
        if let Some((low_level, high_level)) = area {
            for (stroke, edge) in [
                (config.value_area_high, (high_level as f64 + 1.0) * step),
                (config.value_area_low, low_level as f64 * step),
            ] {
                let y = mapping.price_to_y(edge - tick * 0.5);
                if let Some(stroke) = stroke
                    && y.is_finite()
                {
                    painter.line_segment([pos2(x, y), pos2(x + width, y)], stroke);
                }
            }
        }
        if session {
            continue;
        }
        let mut previous = None;
        for &(time, poc) in &profile.developing_poc {
            let index = bars.partition_point(|b| b.time < time);
            let point = pos2(
                mapping.idx_to_x(index),
                mapping.price_to_y((poc as f64 + 0.5) * step - tick * 0.5),
            );
            if !point.is_finite() {
                previous = None;
                continue;
            }
            if let Some(old) = previous {
                painter.line_segment([old, point], Stroke::new(1.0, config.text_color));
            }
            previous = Some(point);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Footprint;
    use crate::model::{BarClock, OrderSide, Tick, Timeframe};
    use crate::scales::{PriceMap, PriceRange, PriceTransform};
    use crate::studies::{TradeProfileRanges, trade_profiles};
    use chrono::TimeDelta;
    use egui::{Shape, vec2};

    /// A session can start before the viewport while its last bars remain visible. Its full
    /// distribution stays visible, preserving volume proportions within its visible bars.
    #[test]
    fn partly_visible_sessions_keep_their_volume_profile_in_view() {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        flow.push(&Tick::new(time, 100.0, 4.0, Some(OrderSide::Buy)))
            .unwrap();
        flow.push(&Tick::new(
            time + TimeDelta::minutes(1),
            101.0,
            2.0,
            Some(OrderSide::Sell),
        ))
        .unwrap();
        let bars: Vec<_> = (0..4)
            .map(|i| {
                Bar::new(
                    time + TimeDelta::minutes(i),
                    100.0,
                    101.0,
                    100.0,
                    101.0,
                    0.0,
                )
            })
            .collect();
        let ranges = TradeProfileRanges::new(vec![time..time + TimeDelta::minutes(4)]).unwrap();
        let profiles = trade_profiles(&flow, &ranges);
        let rect = Rect::from_min_size(pos2(40.0, 20.0), vec2(240.0, 160.0));
        let config = FootprintConfig {
            modes: vec![FootprintMode::Volume],
            value_area: None,
            show_poc: false,
            ..Default::default()
        };
        for (offset, column) in [
            (0.0, Some((200.0, 56.0))),
            (9.0, Some((40.0, 56.0))),
            (11.0, Some((40.0, 19.0))),
            (-5.0, None),
            (20.0, None),
        ] {
            let mapping = ChartMapping::new(
                rect,
                20.0,
                0,
                3,
                offset,
                PriceMap::new(
                    PriceTransform::default(),
                    PriceRange::new(99.0, 103.0),
                    20.0,
                    180.0,
                    false,
                ),
            );
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(320.0, 200.0))),
                    ..Default::default()
                },
                |ui| {
                    render_trade_profiles(
                        ui.painter(),
                        &mapping,
                        &bars,
                        &profiles,
                        1.0,
                        1.0,
                        &config,
                    )
                },
            );
            output.textures_delta.clear();
            let columns: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| {
                    if let Shape::Rect(rect) = &shape.shape {
                        Some((rect.rect.left(), rect.rect.width()))
                    } else {
                        None
                    }
                })
                .collect();
            let expected = column.map_or_else(Vec::new, |(left, width)| {
                vec![(left, width), (left, width * 0.5)]
            });
            assert_eq!(columns, expected, "scroll offset {offset}");
        }
    }

    /// The VAH and VAL guides lie on the outer edges of the rows the shading covers, each in
    /// its own style, and either can be hidden.
    #[test]
    fn value_area_guides_follow_the_shaded_rows() {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        for (price, size, side) in [
            (100.0, 4.0, OrderSide::Buy),
            (101.0, 1.0, OrderSide::Sell),
            (102.0, 1.0, OrderSide::Sell),
        ] {
            flow.push(&Tick::new(time, price, size, Some(side)))
                .unwrap();
        }
        let bars = [Bar::new(time, 100.0, 102.0, 100.0, 101.0, 0.0)];
        let ranges = TradeProfileRanges::new(vec![time..time + TimeDelta::minutes(1)]).unwrap();
        let profiles = trade_profiles(&flow, &ranges);
        let rect = Rect::from_min_size(pos2(40.0, 20.0), vec2(240.0, 160.0));
        let mapping = ChartMapping::new(
            rect,
            20.0,
            0,
            0,
            0.0,
            PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(99.0, 104.0),
                20.0,
                180.0,
                false,
            ),
        );
        let (red, blue) = (
            Stroke::new(1.0, egui::Color32::RED),
            Stroke::new(2.0, egui::Color32::BLUE),
        );
        let guides = |high, low| {
            let config = FootprintConfig {
                value_area: Some(0.6),
                value_area_high: high,
                value_area_low: low,
                ..Default::default()
            };
            let ctx = egui::Context::default();
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(320.0, 200.0))),
                    ..Default::default()
                },
                |ui| {
                    render_trade_profiles(
                        ui.painter(),
                        &mapping,
                        &bars,
                        &profiles,
                        1.0,
                        1.0,
                        &config,
                    )
                },
            );
            output.textures_delta.clear();
            output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::LineSegment { points, stroke } if *stroke == red || *stroke == blue => {
                        Some((points[0].y, stroke.color))
                    }
                    _ => None,
                })
                .collect::<Vec<_>>()
        };
        // Level 100 holds 4 of 6 contracts, which meets the 60% share on its own.
        let top = mapping.price_to_y(100.5);
        let bottom = mapping.price_to_y(99.5);
        assert_eq!(
            guides(Some(red), Some(blue)),
            [(top, egui::Color32::RED), (bottom, egui::Color32::BLUE)]
        );
        assert_eq!(guides(None, Some(blue)), [(bottom, egui::Color32::BLUE)]);
        assert_eq!(guides(Some(red), None), [(top, egui::Color32::RED)]);
    }
}
