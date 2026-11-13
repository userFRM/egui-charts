//! Trade profiles in the price pane, aligned with the bars that contributed their volume.

use crate::chart::coords::ChartMapping;
use crate::chart::overlays::{FootprintConfig, FootprintMode};
use crate::data::Footprint;
use crate::model::Bar;
use crate::studies::TradeProfile;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::{Align2, FontId, Painter, Rect, Stroke, pos2};

pub use crate::model::TradeProfileSettings;

/// Draws bid/ask stacks, total volume or signed delta profiles, their POC, value area,
/// developing POC and volume nodes. `step` is the footprint's grouped price step and
/// `tick` is its instrument tick size before grouping. Bands extend half a tick below
/// their first tick and above their last tick.
/// Histograms of partly visible sessions stay at the left edge of the viewport.
/// Profile ranges and data remain host-owned; edit [`crate::studies::TradeProfileRanges`]
/// in response to a session menu's merge/split actions and recalculate after trades change.
/// A profile that is not [`complete`](TradeProfile::complete) is labelled "Partial executions".
pub fn render_trade_profiles(
    painter: &Painter,
    mapping: &ChartMapping,
    bars: &[Bar],
    profiles: &[TradeProfile],
    step: f64,
    tick: f64,
    config: &FootprintConfig,
) {
    render_profiles(painter, mapping, bars, profiles, step, tick, config, None);
}

/// Draws the session volume profile chart: each session fills its bar span inside a faint box,
/// with up volume stacked before down volume, a POC across the session, and the value area shown
/// by row opacity. Optional annotations follow the supplied settings.
/// A session whose profile is not complete is labelled "Partial executions".
pub(crate) fn render_session_volume_profiles(
    painter: &Painter,
    mapping: &ChartMapping,
    bars: &[Bar],
    profiles: &[TradeProfile],
    flow: &Footprint,
    settings: &TradeProfileSettings,
) {
    let theme = painter.ctx().active_theme();
    let guide = settings
        .value_area_guides
        .then_some(Stroke::new(1.0, theme.chart.axis_text_secondary));
    let config = FootprintConfig {
        modes: vec![settings.mode],
        value_area: settings.value_area,
        node_sensitivity: settings.nodes.unwrap_or_default(),
        ask_color: DESIGN_TOKENS.semantic.brand.accent,
        bid_color: DESIGN_TOKENS.semantic.tpo.period_6,
        neutral_color: theme.chart.axis_text_secondary,
        text_color: DESIGN_TOKENS.semantic.chart.bearish,
        value_area_high: guide,
        value_area_low: guide,
        min_text_height: if settings.nodes.is_some() {
            12.0
        } else {
            f32::INFINITY
        },
        ..Default::default()
    };
    render_profiles(
        painter,
        mapping,
        bars,
        profiles,
        flow.price_step(),
        flow.tick_size(),
        &config,
        Some((settings, flow)),
    );
}

fn render_profiles(
    painter: &Painter,
    mapping: &ChartMapping,
    bars: &[Bar],
    profiles: &[TradeProfile],
    step: f64,
    tick: f64,
    config: &FootprintConfig,
    settings: Option<(&TradeProfileSettings, &Footprint)>,
) {
    let session = settings.is_some();
    if !step.is_finite() || step <= 0.0 || !tick.is_finite() || tick <= 0.0 || bars.is_empty() {
        return;
    }
    let painter = painter.with_clip_rect(mapping.rect.intersect(painter.clip_rect()));
    let theme = painter.ctx().active_theme();
    let pixel = 1.0 / painter.pixels_per_point();
    let mut extensions = Vec::new();
    for profile in profiles {
        let step = step * profile.group as f64;
        let first = bars.partition_point(|b| b.time < profile.range.start);
        let end = bars.partition_point(|b| b.time < profile.range.end);
        if first >= end {
            continue;
        }
        // Extensions survive after their source histogram has left the viewport. A partial
        // profile's levels are not the session's, and away from its label nothing would say so.
        if let Some((settings, _)) = settings
            && profile.complete
            && end < bars.len()
        {
            let start = mapping.idx_to_x(end - 1) + mapping.bar_spacing * 0.5;
            let mut extend = |price: f64, level: i64, stroke: Stroke| {
                // Trading anywhere in the level's row touches it, as for footprint POC revisits.
                // A candle without volume, such as the one a heartbeat opens, traded nowhere.
                let row =
                    level as f64 * step - tick * 0.5..(level as f64 + 1.0) * step - tick * 0.5;
                let last = bars[end..]
                    .iter()
                    .position(|bar| bar.volume > 0.0 && bar.low < row.end && row.start <= bar.high)
                    .map_or(mapping.rect.right(), |i| mapping.idx_to_x(end + i));
                let left = start.max(mapping.rect.left());
                let right = last.min(mapping.rect.right());
                let y = mapping.price_to_y(price);
                if left < right && y.is_finite() {
                    extensions.push(([pos2(left, y), pos2(right, y)], stroke));
                }
            };
            if settings.extend_poc
                && let Some(poc) = profile.volume.poc()
            {
                extend(
                    (poc as f64 + 0.5) * step - tick * 0.5,
                    poc,
                    Stroke::new(1.0, config.text_color),
                );
            }
            if (settings.extend_vah || settings.extend_val)
                && let Some((low, high)) = settings
                    .value_area
                    .and_then(|s| profile.volume.value_area(s))
            {
                for (enabled, edge, level) in [
                    (settings.extend_vah, high as f64 + 1.0, high),
                    (settings.extend_val, low as f64, low),
                ] {
                    if enabled {
                        extend(
                            edge * step - tick * 0.5,
                            level,
                            Stroke::new(1.0, theme.chart.axis_text_secondary),
                        );
                    }
                }
            }
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
                FootprintMode::BidAsk | FootprintMode::Ladder if session => vec![
                    (row.ask, config.ask_color),
                    (row.bid, config.bid_color),
                    (row.unknown, config.neutral_color),
                ],
                FootprintMode::BidAsk | FootprintMode::Ladder => vec![
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
                let y = mapping.price_to_y((row.level as f64 + 0.5) * step - tick * 0.5);
                painter.line_segment(
                    [pos2(x, y), pos2(x + width, y)],
                    Stroke::new(DESIGN_TOKENS.stroke.hairline, config.text_color),
                );
            }
        }
        // A range that starts before the first complete footprint bar leaves those bars out.
        // No narrower profile has room for the note, so its text is never laid out.
        if !profile.complete
            && width >= 60.0
            && let Some(&high) = profile.volume.levels.keys().next_back()
        {
            let y = mapping.price_to_y((high as f64 + 1.0) * step - tick * 0.5);
            let galley = painter.layout_no_wrap(
                crate::localization::text(painter.ctx(), "Partial executions"),
                FontId::proportional(crate::styles::typography::XS),
                theme.chart.axis_text_secondary,
            );
            let size = galley.size();
            if y.is_finite() && size.x <= width {
                painter.galley(pos2(x, y - size.y), galley, theme.chart.axis_text_secondary);
            }
        }
        // A valley can lie on untraded rows, so nodes are placed by level, not by traded row.
        for (label, nodes) in [("HVN", &high), ("LVN", &low)] {
            for &level in nodes {
                let y0 = mapping.price_to_y(level as f64 * step - tick * 0.5);
                let y1 = mapping.price_to_y((level as f64 + 1.0) * step - tick * 0.5);
                if y0.is_finite() && y1.is_finite() && (y0 - y1).abs() >= config.min_text_height {
                    let galley = painter.layout_no_wrap(
                        label.into(),
                        FontId::monospace(crate::styles::typography::TINY),
                        config.text_color,
                    );
                    if session && galley.size().x + 6.0 > width {
                        continue;
                    }
                    let anchor = if session {
                        Align2::RIGHT_CENTER
                    } else {
                        Align2::LEFT_CENTER
                    };
                    let pos = pos2(
                        x + width + if session { -3.0 } else { 3.0 },
                        (y0 + y1) * 0.5,
                    );
                    painter.galley(
                        anchor.anchor_size(pos, galley.size()).min,
                        galley,
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
        if settings.is_none_or(|(s, _)| s.developing_poc) {
            render_developing(
                &painter,
                mapping,
                bars,
                profile
                    .developing_poc
                    .iter()
                    .map(|&(time, poc)| (time, (poc as f64 + 0.5) * step - tick * 0.5)),
                Stroke::new(1.0, config.text_color),
            );
        }
        if let Some((settings, flow)) = settings
            && settings.developing_value_area
            && let Some(share) = settings.value_area
        {
            let areas = profile.developing_value_area(flow, share);
            for high in [false, true] {
                render_developing(
                    &painter,
                    mapping,
                    bars,
                    areas.iter().map(|&(time, (low, upper))| {
                        let edge = if high { upper as f64 + 1.0 } else { low as f64 };
                        (time, edge * step - tick * 0.5)
                    }),
                    Stroke::new(1.0, theme.chart.axis_text_secondary),
                );
            }
        }
    }
    // Later session boxes must not cover the part of a level before its first touch.
    for (points, stroke) in extensions {
        painter.line_segment(points, stroke);
    }
}

fn render_developing(
    painter: &Painter,
    mapping: &ChartMapping,
    bars: &[Bar],
    points: impl Iterator<Item = (chrono::DateTime<chrono::Utc>, f64)>,
    stroke: Stroke,
) {
    let mut previous = None;
    let mut points = points.peekable();
    while let Some((time, price)) = points.next() {
        // Keep the last point before retained candles, so the trace enters at the right level.
        if time < bars[0].time && points.peek().is_some_and(|(next, _)| *next <= bars[0].time) {
            continue;
        }
        let index = bars.partition_point(|b| b.time < time);
        let point = pos2(mapping.idx_to_x(index), mapping.price_to_y(price));
        if !point.is_finite() {
            previous = None;
            continue;
        }
        if let Some(old) = previous {
            painter.line_segment([old, point], stroke);
        }
        previous = Some(point);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BarClock, OrderSide, Tick, Timeframe};
    use crate::scales::{PriceMap, PriceRange, PriceScaleMode, PriceTransform};
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

    /// The first session lost its first bar to the budget, so its POC is not the session's.
    #[test]
    fn partial_sessions_do_not_extend_their_levels() {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        flow.set_budget(2, usize::MAX);
        let mut bars = Vec::new();
        for (minute, price) in [(0, 100.0), (1, 101.0), (2, 103.0)] {
            let at = time + TimeDelta::minutes(minute);
            flow.push(&Tick::new(at, price, 1.0, None)).unwrap();
            bars.push(Bar::new(at, price, price, price, price, 1.0));
        }
        let ranges = TradeProfileRanges::new(vec![
            time..time + TimeDelta::minutes(2),
            time + TimeDelta::minutes(2)..time + TimeDelta::minutes(3),
        ])
        .unwrap();
        let profiles = trade_profiles(&flow, &ranges);
        let transform = PriceTransform::default();
        let mapping = ChartMapping::new(
            Rect::from_min_size(pos2(40.0, 20.0), vec2(400.0, 240.0)),
            40.0,
            0,
            3,
            0.0,
            PriceMap::new(
                transform,
                PriceRange::new(transform.to_scale(98.0), transform.to_scale(105.0)),
                20.0,
                260.0,
                false,
            ),
        );
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            render_session_volume_profiles(
                ui.painter(),
                &mapping,
                &bars,
                &profiles,
                &flow,
                &TradeProfileSettings {
                    extend_poc: true,
                    extend_vah: true,
                    extend_val: true,
                    ..Default::default()
                },
            );
        });
        output.textures_delta.clear();
        // Only the complete second session's own POC reaches past the first session.
        let session_end = mapping.idx_to_x(1) + mapping.bar_spacing * 0.5;
        let own_poc = mapping.price_to_y(103.0);
        assert!(!output.shapes.iter().any(|s| matches!(
            &s.shape,
            Shape::LineSegment { points, .. } if points[1].x > session_end && points[0].y != own_poc
        )));
    }

    /// A heartbeat opens the next session's first slot as an empty candle at the last close.
    /// Nothing trades there, so the prior POC and VAL in that row keep running to the edge.
    #[test]
    fn empty_candle_at_the_previous_close_does_not_end_prior_levels() {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        // Rows 100:1, 101:6, 102:2: POC 101, 70% area 101 to 102; the session closes at 101.
        for (minute, price, size) in [
            (0, 100.0, 1.0),
            (0, 101.0, 4.0),
            (1, 102.0, 2.0),
            (1, 101.0, 2.0),
            (3, 103.0, 1.0),
            (3, 104.0, 1.0),
            (4, 104.0, 1.0),
            (4, 103.0, 1.0),
        ] {
            let at: chrono::DateTime<chrono::Utc> = time + TimeDelta::minutes(minute);
            flow.push(&Tick::new(
                at + TimeDelta::seconds(size as i64),
                price,
                size,
                None,
            ))
            .unwrap();
        }
        let bars: Vec<_> = [
            (100.0, 101.0, 100.0, 101.0, 5.0),
            (102.0, 102.0, 101.0, 101.0, 4.0),
            // The empty candle a heartbeat opens: no execution, the last close carried.
            (101.0, 101.0, 101.0, 101.0, 0.0),
            (103.0, 104.0, 103.0, 104.0, 2.0),
            (104.0, 104.0, 103.0, 103.0, 2.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (open, high, low, close, volume))| {
            Bar::new(
                time + TimeDelta::minutes(i as i64),
                open,
                high,
                low,
                close,
                volume,
            )
        })
        .collect();
        let ranges = TradeProfileRanges::new(vec![
            time..time + TimeDelta::minutes(2),
            time + TimeDelta::minutes(2)..time + TimeDelta::minutes(5),
        ])
        .unwrap();
        let profiles = trade_profiles(&flow, &ranges);
        assert_eq!(profiles[0].volume.poc(), Some(101));
        assert_eq!(profiles[0].volume.value_area(0.7), Some((101, 102)));
        let transform = PriceTransform::default();
        let mapping = ChartMapping::new(
            Rect::from_min_size(pos2(40.0, 20.0), vec2(400.0, 240.0)),
            40.0,
            0,
            4,
            0.0,
            PriceMap::new(
                transform,
                PriceRange::new(transform.to_scale(98.0), transform.to_scale(105.0)),
                20.0,
                260.0,
                false,
            ),
        );
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            render_session_volume_profiles(
                ui.painter(),
                &mapping,
                &bars,
                &profiles,
                &flow,
                &TradeProfileSettings {
                    extend_poc: true,
                    extend_val: true,
                    ..Default::default()
                },
            );
        });
        output.textures_delta.clear();
        // Extensions start at the first session's right edge, x(1) + 20 = 319.
        let start = mapping.idx_to_x(1) + mapping.bar_spacing * 0.5;
        let ends: Vec<_> = [101.0, 100.5]
            .into_iter()
            .map(|price| {
                let y = mapping.price_to_y(price);
                output.shapes.iter().find_map(|s| match &s.shape {
                    Shape::LineSegment { points, .. }
                        if points[0] == pos2(start, y) && points[1].y == y =>
                    {
                        Some(points[1].x)
                    }
                    _ => None,
                })
            })
            .collect();
        // Nothing trades at 101 after the first session: both run to the right edge, 440.
        assert_eq!(
            ends,
            [Some(mapping.rect.right()), Some(mapping.rect.right())],
            "POC 101 and VAL 100.5 end at {ends:?}; the empty candle is at x(2) = {}",
            mapping.idx_to_x(2)
        );
    }

    /// At 70%, 6 + 3 exceeds 7 of 10; after adding 8 at 100, 9 + 6 exceeds 12.6
    /// of 18. Prior levels stop independently at the first later bar trading in their row.
    #[test]
    fn developing_area_and_prior_levels_follow_live_and_rebuilt_history() {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        let ticks: Vec<_> = [
            (0, 100.0, 1.0),
            (1, 101.0, 6.0),
            (2, 102.0, 3.0),
            (60, 100.0, 8.0),
            (120, 103.0, 1.0),
            (121, 104.0, 1.0),
            (180, 100.0, 1.0),
            (240, 99.0, 1.0),
            (241, 101.0, 1.0),
        ]
        .into_iter()
        .map(|(second, price, size)| {
            Tick::new(time + TimeDelta::seconds(second), price, size, None)
        })
        .collect();
        let ranges = TradeProfileRanges::new(vec![
            time..time + TimeDelta::minutes(2),
            time + TimeDelta::minutes(2)..time + TimeDelta::minutes(5),
        ])
        .unwrap();
        let bars: Vec<_> = [
            (100.0, 102.0),
            (100.0, 100.0),
            (103.0, 104.0),
            (100.0, 100.0),
            (99.0, 101.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (low, high))| {
            Bar::new(
                time + TimeDelta::minutes(i as i64),
                low,
                high,
                low,
                high,
                1.0,
            )
        })
        .collect();
        for step in [1.0, 0.003] {
            let ticks: Vec<_> = ticks
                .iter()
                .cloned()
                .map(|mut tick| {
                    tick.price *= step;
                    tick
                })
                .collect();
            let bars: Vec<_> = bars
                .iter()
                .map(|bar| {
                    Bar::new(
                        bar.time,
                        bar.open * step,
                        bar.high * step,
                        bar.low * step,
                        bar.close * step,
                        bar.volume,
                    )
                })
                .collect();
            let ctx = egui::Context::default();
            let theme = ctx.active_theme();
            let poc_color = DESIGN_TOKENS.semantic.chart.bearish;
            let area_color = theme.chart.axis_text_secondary;
            let mut live = Footprint::new(BarClock::new(Timeframe::Min1), step, 1).unwrap();
            let paint = |flow: &Footprint, count, saved, offset, inverted, mode| {
                let transform = PriceTransform {
                    mode,
                    ..Default::default()
                };
                let settings: TradeProfileSettings = serde_json::from_str(saved).unwrap();
                let mapping = ChartMapping::new(
                    Rect::from_min_size(pos2(40.0, 20.0), vec2(400.0, 240.0)),
                    40.0,
                    0,
                    4,
                    offset,
                    PriceMap::new(
                        transform,
                        PriceRange::new(
                            transform.to_scale(98.0 * step),
                            transform.to_scale(105.0 * step),
                        ),
                        20.0,
                        260.0,
                        inverted,
                    ),
                );
                let profiles = trade_profiles(flow, &ranges);
                let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
                    render_session_volume_profiles(
                        ui.painter(),
                        &mapping,
                        &bars[..count],
                        &profiles,
                        flow,
                        &settings,
                    );
                });
                output.textures_delta.clear();
                let lines = output
                    .shapes
                    .into_iter()
                    .filter_map(|s| match s.shape {
                        Shape::LineSegment { points, stroke } => Some((points, stroke.color)),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                (mapping, lines)
            };
            for count in 1..=5 {
                for tick in ticks.iter().filter(|t| {
                    t.ts >= time + TimeDelta::minutes(count as i64 - 1)
                        && t.ts < time + TimeDelta::minutes(count as i64)
                }) {
                    live.push(tick).unwrap();
                }
                let mut rebuilt = Footprint::new(BarClock::new(Timeframe::Min1), step, 1).unwrap();
                for tick in ticks
                    .iter()
                    .filter(|t| t.ts < time + TimeDelta::minutes(count as i64))
                {
                    rebuilt.push(tick).unwrap();
                }
                for (saved, developing, extend_poc, extend_vah, extend_val, share) in [
                    (r#"{}"#, false, false, false, false, 0.7),
                    (
                        r#"{"developing_value_area":true}"#,
                        true,
                        false,
                        false,
                        false,
                        0.7,
                    ),
                    (
                        r#"{"developing_value_area":true,"value_area":0.5}"#,
                        true,
                        false,
                        false,
                        false,
                        0.5,
                    ),
                    (r#"{"extend_poc":true}"#, false, true, false, false, 0.7),
                    (r#"{"extend_vah":true}"#, false, false, true, false, 0.7),
                    (r#"{"extend_val":true}"#, false, false, false, true, 0.7),
                    (
                        r#"{"developing_value_area":true,"extend_poc":true,"extend_vah":true,"extend_val":true}"#,
                        true,
                        true,
                        true,
                        true,
                        0.7,
                    ),
                    (
                        r#"{"developing_value_area":true,"extend_vah":true,"extend_val":true,"value_area":null}"#,
                        false,
                        false,
                        false,
                        false,
                        0.7,
                    ),
                ] {
                    for (offset, inverted, mode) in [
                        (0.0, false, PriceScaleMode::Normal),
                        (9.0, true, PriceScaleMode::Normal),
                        (0.0, false, PriceScaleMode::Logarithmic),
                    ] {
                        let (map, lines) = paint(&live, count, saved, offset, inverted, mode);
                        assert_eq!(
                            lines,
                            paint(&rebuilt, count, saved, offset, inverted, mode).1
                        );
                        let contains = |a, b, color| {
                            lines
                                .iter()
                                .any(|(points, c)| *c == color && *points == [a, b])
                        };
                        let visible_source = map.idx_to_x(1) >= map.rect.left();
                        if count >= 2 && visible_source {
                            if extend_poc {
                                let y = map.price_to_y(100.0 * step);
                                assert!(
                                    contains(
                                        pos2(map.idx_to_x(0) - 18.0, y),
                                        pos2(map.idx_to_x(1) + 20.0, y),
                                        poc_color,
                                    ),
                                    "POC joins its extension at the same price on {mode:?}"
                                );
                            }
                            for (first, last) in [
                                (100.5, 99.5),
                                (
                                    if share == 0.7 { 102.5 } else { 101.5 },
                                    if share == 0.7 { 101.5 } else { 100.5 },
                                ),
                            ] {
                                assert_eq!(
                                    contains(
                                        pos2(map.idx_to_x(0), map.price_to_y(first * step)),
                                        pos2(map.idx_to_x(1), map.price_to_y(last * step)),
                                        area_color
                                    ),
                                    developing,
                                    "developing: {saved}"
                                );
                            }
                        }
                        let start = (map.idx_to_x(1) + 20.0).max(map.rect.left());
                        for (price, stop, enabled, color) in [
                            (100.0, 3, extend_poc, poc_color),
                            (101.5, 4, extend_vah, area_color),
                            (99.5, 3, extend_val, area_color),
                        ] {
                            let end = if stop < count {
                                map.idx_to_x(stop)
                            } else {
                                map.rect.right()
                            };
                            if start < end {
                                assert_eq!(
                                    contains(
                                        pos2(start, map.price_to_y(price * step)),
                                        pos2(end, map.price_to_y(price * step)),
                                        color
                                    ),
                                    enabled && count > 2,
                                    "count {count}, level {price}, {saved}, offset {offset}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
