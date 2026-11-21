//! Trade profiles in the price pane, aligned with the bars that contributed their volume.

use crate::chart::coords::ChartMapping;
use crate::chart::overlays::{FootprintConfig, FootprintMode};
use crate::data::FootprintView;
use crate::model::Bar;
use crate::studies::TradeProfile;
use crate::theme::{Theme, ThemeExt};
use crate::tokens::DESIGN_TOKENS;
use egui::{Align2, Color32, FontId, Painter, Rect, Stroke, pos2};

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

/// Draws the session volume profile chart: each session's histogram grows from the left or right
/// edge of its session over the chosen share of its width, inside a faint box the settings can
/// hide or recolour, with up volume stacked before down volume, a POC across the session, and the
/// value area shown by row colour, by default the same colours at a higher opacity. Rows tall
/// enough for the text can show their volume. The levels, their colours and optional annotations
/// follow the supplied settings.
/// A session whose profile is not complete is labelled "Partial executions".
pub(crate) fn render_session_volume_profiles(
    painter: &Painter,
    mapping: &ChartMapping,
    bars: &[Bar],
    profiles: &[TradeProfile],
    flow: FootprintView<'_>,
    settings: &TradeProfileSettings,
    volume_decimals: Option<u8>,
) {
    let theme = painter.ctx().active_theme();
    let colors = SessionColors::new(settings, &theme);
    let guide = |shown: bool, color: Color32| {
        (settings.value_area_guides && shown).then_some(Stroke::new(1.0, color))
    };
    let config = FootprintConfig {
        modes: vec![settings.mode],
        volume_decimals,
        value_area: settings.value_area,
        node_sensitivity: settings.nodes.unwrap_or_default(),
        neutral_color: theme.chart.axis_text_secondary,
        text_color: DESIGN_TOKENS.semantic.chart.bearish,
        show_poc: settings.poc,
        value_area_high: guide(settings.value_area_high, colors.value_area_high),
        value_area_low: guide(settings.value_area_low, colors.value_area_low),
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

/// Colours resolved from the session settings and the active theme. Built-in row colours
/// carry their row opacity; configured colours retain their own opacity.
pub(crate) struct SessionColors {
    /// The POC line and its extension.
    pub(crate) poc: Color32,
    /// The value area high guide and its extension.
    pub(crate) value_area_high: Color32,
    /// The value area low guide and its extension.
    pub(crate) value_area_low: Color32,
    /// Up and down volume outside the value area.
    pub(crate) volume: [Color32; 2],
    /// Up and down volume inside the value area.
    pub(crate) value_area_volume: [Color32; 2],
    /// The box behind each session.
    pub(crate) session_box: Color32,
    /// The row values.
    pub(crate) values: Color32,
}

impl SessionColors {
    pub(crate) fn new(settings: &TradeProfileSettings, theme: &Theme) -> Self {
        let guide = theme.chart.axis_text_secondary;
        let volume = |inside, up: Option<Color32>, down: Option<Color32>| {
            let alpha = row_opacity(inside, theme.dark);
            [
                up.unwrap_or(DESIGN_TOKENS.semantic.brand.accent.gamma_multiply(alpha)),
                down.unwrap_or(DESIGN_TOKENS.semantic.tpo.period_6.gamma_multiply(alpha)),
            ]
        };
        Self {
            poc: settings
                .poc_color
                .unwrap_or(DESIGN_TOKENS.semantic.chart.bearish),
            value_area_high: settings.value_area_high_color.unwrap_or(guide),
            value_area_low: settings.value_area_low_color.unwrap_or(guide),
            volume: volume(false, settings.up_volume_color, settings.down_volume_color),
            value_area_volume: volume(
                true,
                settings.value_area_up_color,
                settings.value_area_down_color,
            ),
            session_box: settings.session_box_color.unwrap_or_else(|| {
                theme
                    .chart
                    .bg
                    .lerp_to_gamma(theme.chart.axis_text, 0.035)
                    .lerp_to_gamma(
                        DESIGN_TOKENS.semantic.brand.accent,
                        if theme.dark { 0.065 } else { 0.035 },
                    )
            }),
            values: settings.values_color.unwrap_or(theme.chart.axis_text),
        }
    }
}

/// Opacity of built-in row colours; rows inside the value area stand out.
fn row_opacity(inside: bool, dark: bool) -> f32 {
    match (inside, dark) {
        (true, true) => 0.9,
        (true, false) => 0.82,
        (false, true) => 0.32,
        (false, false) => 0.24,
    }
}

/// The share of its session's width a histogram's longest row spans: 0.01 to 1, NaN as 1.
fn histogram_share(width: f64) -> f32 {
    if width.is_nan() {
        1.0
    } else {
        width.clamp(0.01, 1.0) as f32
    }
}

/// The footprint text matching the bar a session row draws in `mode`.
fn value_mode(mode: FootprintMode) -> FootprintMode {
    match mode {
        FootprintMode::BidAsk | FootprintMode::Ladder => FootprintMode::BidAsk,
        FootprintMode::Volume | FootprintMode::Delta | FootprintMode::DeltaVolume => mode,
        _ => FootprintMode::Delta,
    }
}

fn render_profiles(
    painter: &Painter,
    mapping: &ChartMapping,
    bars: &[Bar],
    profiles: &[TradeProfile],
    step: f64,
    tick: f64,
    config: &FootprintConfig,
    settings: Option<(&TradeProfileSettings, FootprintView<'_>)>,
) {
    let session = settings.is_some();
    if !step.is_finite() || step <= 0.0 || !tick.is_finite() || tick <= 0.0 || bars.is_empty() {
        return;
    }
    let painter = painter.with_clip_rect(mapping.rect.intersect(painter.clip_rect()));
    let theme = painter.ctx().active_theme();
    let colors = settings.map(|(settings, _)| SessionColors::new(settings, &theme));
    let poc_color = colors
        .as_ref()
        .map_or(config.text_color, |colors| colors.poc);
    let pixel = 1.0 / painter.pixels_per_point();
    let values = settings.filter(|(s, _)| s.values).map(|_| {
        let font = FontId::monospace(crate::styles::typography::TINY);
        let height = painter.ctx().fonts_mut(|fonts| fonts.row_height(&font));
        (font, height)
    });
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
            && let Some(colors) = &colors
            && profile.complete
            && end < bars.len()
        {
            let start = mapping.idx_to_x(end - 1) + mapping.bar_spacing * 0.5;
            let mut extend = |price: f64, stroke: Stroke| {
                // Decimal tick arithmetic can put the drawn level a few ulps off its price.
                let epsilon = (price.abs() * f64::EPSILON * 4.0).max(f64::from_bits(1));
                let last = bars[end..]
                    .iter()
                    .position(|bar| {
                        bar.volume > 0.0
                            && bar.low - price <= epsilon
                            && price - bar.high <= epsilon
                    })
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
                    Stroke::new(1.0, colors.poc),
                );
            }
            if (settings.extend_vah || settings.extend_val)
                && let Some((low, high)) = settings
                    .value_area
                    .and_then(|s| profile.volume.value_area(s))
            {
                for (enabled, edge, color) in [
                    (
                        settings.extend_vah,
                        high as f64 + 1.0,
                        colors.value_area_high,
                    ),
                    (settings.extend_val, low as f64, colors.value_area_low),
                ] {
                    if enabled {
                        extend(edge * step - tick * 0.5, Stroke::new(1.0, color));
                    }
                }
            }
        }
        let start = mapping.idx_to_x(first) - mapping.bar_spacing * 0.45;
        let stop = mapping.idx_to_x(end - 1) + mapping.bar_spacing * 0.5;
        if mapping.idx_to_x(end - 1) < mapping.rect.left() || start > mapping.rect.right() {
            continue;
        }
        let x = start.max(mapping.rect.left());
        let session_width = (stop - x).max(1.0);
        // Fit each histogram to the visible part of its session, so both of its edges stay in view.
        let (anchor, toward, width) = match settings {
            Some((s, _)) if s.right_aligned => {
                let anchor = stop.min(mapping.rect.right());
                (
                    anchor,
                    -1.0,
                    (anchor - x).max(1.0) * histogram_share(s.width),
                )
            }
            Some((s, _)) => (x, 1.0, session_width * histogram_share(s.width)),
            None => (
                x,
                1.0,
                ((end - first) as f32 * mapping.bar_spacing * 0.7)
                    .clamp(1.0, 160.0)
                    .min(session_width),
            ),
        };
        let hist_left = if toward < 0.0 { anchor - width } else { anchor };
        // Levels, labels and the box span the session; a trade profile has only its column.
        let span = if session { session_width } else { width };
        let max = profile
            .volume
            .levels
            .values()
            .map(|r| r.volume())
            .fold(0.0, f64::max);
        if max <= 0.0 {
            continue;
        }
        if let Some((s, _)) = settings
            && s.session_box
            && let Some(colors) = &colors
            && let (Some((&low, _)), Some((&high, _))) = (
                profile.volume.levels.first_key_value(),
                profile.volume.levels.last_key_value(),
            )
        {
            let y0 = mapping.price_to_y(low as f64 * step - tick * 0.5);
            let y1 = mapping.price_to_y((high as f64 + 1.0) * step - tick * 0.5);
            if y0.is_finite() && y1.is_finite() {
                painter.rect_filled(
                    Rect::from_min_max(pos2(x, y0.min(y1)), pos2(x + session_width, y0.max(y1))),
                    0.0,
                    colors.session_box,
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
            let inside = area.is_some_and(|(a, b)| (a..=b).contains(&row.level));
            let alpha = row_opacity(inside, theme.dark);
            let [ask, bid] = match &colors {
                Some(colors) if inside => colors.value_area_volume,
                Some(colors) => colors.volume,
                None => [config.ask_color, config.bid_color].map(|c| c.gamma_multiply(alpha)),
            };
            let neutral = config.neutral_color.gamma_multiply(alpha);
            let mut left = anchor;
            let mode = config
                .modes
                .first()
                .copied()
                .unwrap_or(FootprintMode::BidAsk);
            let segments = match mode {
                FootprintMode::BidAsk | FootprintMode::Ladder if session => {
                    vec![(row.ask, ask), (row.bid, bid), (row.unknown, neutral)]
                }
                FootprintMode::BidAsk | FootprintMode::Ladder => {
                    vec![(row.bid, bid), (row.ask, ask), (row.unknown, neutral)]
                }
                FootprintMode::Volume => vec![(row.volume(), neutral)],
                _ => {
                    left = hist_left + width * 0.5;
                    vec![(
                        row.delta().abs(),
                        if row.delta() >= 0.0 { ask } else { bid },
                    )]
                }
            };
            let delta = matches!(mode, FootprintMode::Delta | FootprintMode::DeltaVolume);
            let centred = !matches!(
                mode,
                FootprintMode::BidAsk | FootprintMode::Ladder | FootprintMode::Volume
            );
            // Stacks grow away from their anchor; delta keeps its sign about the middle.
            let dir = if !centred {
                toward
            } else if delta && row.delta() < 0.0 {
                -1.0
            } else {
                1.0
            };
            for (volume, color) in segments {
                let w = width * (volume / max) as f32 * if delta { 0.5 } else { 1.0 };
                let right = left + dir * w;
                painter.rect_filled(
                    Rect::from_min_max(pos2(left.min(right), top), pos2(left.max(right), bottom)),
                    0.0,
                    color,
                );
                left = right;
            }
            if config.show_poc && poc == Some(row.level) {
                let y = mapping.price_to_y((row.level as f64 + 0.5) * step - tick * 0.5);
                painter.line_segment(
                    [pos2(x, y), pos2(x + span, y)],
                    Stroke::new(DESIGN_TOKENS.stroke.hairline, poc_color),
                );
            }
            if let Some((font, height)) = &values
                && let Some(colors) = &colors
                && (y0 - y1).abs() >= *height
            {
                let text = super::footprint::cell_text(
                    value_mode(mode),
                    row,
                    true,
                    true,
                    config.volume_decimals,
                );
                let (lo, hi) = (x + 2.0, (x + session_width).min(mapping.rect.right()) - 2.0);
                let galley = painter.layout_no_wrap(text, font.clone(), colors.values);
                let size = galley.size();
                if size.x > 0.0 && size.x <= hi - lo {
                    // Beside the outer end of the bar, pulled inside the visible session box.
                    let tx = if dir < 0.0 {
                        left - 3.0 - size.x
                    } else {
                        left + 3.0
                    };
                    let pos = pos2(tx.clamp(lo, hi - size.x), (y0 + y1) * 0.5 - size.y * 0.5);
                    painter.galley(pos, galley, colors.values);
                }
            }
        }
        // A range that starts before the first complete footprint bar leaves those bars out.
        // No narrower profile has room for the note, so its text is never laid out.
        if !profile.complete
            && span >= 60.0
            && let Some(&high) = profile.volume.levels.keys().next_back()
        {
            let y = mapping.price_to_y((high as f64 + 1.0) * step - tick * 0.5);
            let galley = painter.layout_no_wrap(
                crate::localization::text(painter.ctx(), "Partial executions"),
                FontId::proportional(crate::styles::typography::XS),
                theme.chart.axis_text_secondary,
            );
            let size = galley.size();
            if y.is_finite() && size.x <= span {
                painter.galley(pos2(x, y - size.y), galley, theme.chart.axis_text_secondary);
            }
        }
        // A valley can lie on untraded rows, so nodes are placed by level, not by traded row.
        for (label, nodes) in [("HVN", &high), ("LVN", &low)] {
            let label = crate::localization::text(painter.ctx(), label);
            for &level in nodes {
                let y0 = mapping.price_to_y(level as f64 * step - tick * 0.5);
                let y1 = mapping.price_to_y((level as f64 + 1.0) * step - tick * 0.5);
                if y0.is_finite() && y1.is_finite() && (y0 - y1).abs() >= config.min_text_height {
                    let galley = painter.layout_no_wrap(
                        label.clone(),
                        FontId::monospace(crate::styles::typography::TINY),
                        config.text_color,
                    );
                    if session && galley.size().x + 6.0 > span {
                        continue;
                    }
                    // Session node labels sit opposite the edge the histogram grows from.
                    let right_placed = session && toward < 0.0;
                    let align = if session && !right_placed {
                        Align2::RIGHT_CENTER
                    } else {
                        Align2::LEFT_CENTER
                    };
                    let label_x = if right_placed {
                        x + 3.0
                    } else {
                        x + span + if session { -3.0 } else { 3.0 }
                    };
                    let pos = pos2(label_x, (y0 + y1) * 0.5);
                    painter.galley(
                        align.anchor_size(pos, galley.size()).min,
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
                    painter.line_segment([pos2(x, y), pos2(x + span, y)], stroke);
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
            let areas = profile.developing_value_area_read(flow, share);
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
    let mut previous: Option<egui::Pos2> = None;
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
            let corner = pos2(point.x, old.y);
            painter.line_segment([old, corner], stroke);
            painter.line_segment([corner, point], stroke);
        }
        previous = Some(point);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Footprint;
    use crate::model::{BarClock, OrderSide, Tick, Timeframe};
    use crate::scales::{PriceMap, PriceRange, PriceScaleMode, PriceTransform};
    use crate::studies::{TradeProfileRanges, trade_profiles};
    use chrono::TimeDelta;
    use egui::{Shape, vec2};

    /// The four-bar session spans x 200 to 279. At half width a histogram spans 39.5 points from
    /// the edge it grows from and a row with half the top volume 19.75, while the box keeps the
    /// whole session. Past the viewport, a right-placed histogram grows from the viewport's edge.
    #[test]
    fn session_histograms_follow_their_placement_width_and_box() {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        for (price, size, side) in [(100.0, 4.0, OrderSide::Buy), (101.0, 2.0, OrderSide::Sell)] {
            flow.push(&Tick::new(time, price, size, Some(side)))
                .unwrap();
        }
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
        let cases = [
            (
                serde_json::json!({}),
                0.0,
                true,
                [(200.0, 279.0), (200.0, 239.5)],
            ),
            (
                serde_json::json!({"right_aligned": true, "width": 0.5}),
                0.0,
                true,
                [(239.5, 279.0), (259.25, 279.0)],
            ),
            (
                serde_json::json!({"width": 0.5}),
                0.0,
                true,
                [(200.0, 239.5), (200.0, 219.75)],
            ),
            // Delta grows about the histogram's centre, with its sign in either placement.
            (
                serde_json::json!({"right_aligned": true, "width": 0.5, "mode": "Delta"}),
                0.0,
                true,
                [(259.25, 279.0), (249.375, 259.25)],
            ),
            (
                serde_json::json!({"session_box": false}),
                0.0,
                false,
                [(200.0, 279.0), (200.0, 239.5)],
            ),
            (
                serde_json::json!({"session_box_color": [40, 50, 60, 128]}),
                0.0,
                true,
                [(200.0, 279.0), (200.0, 239.5)],
            ),
            // Stop=319 is beyond the viewport; the anchor stays at 280, giving width=20.
            (
                serde_json::json!({"right_aligned": true, "width": 0.5}),
                -2.0,
                true,
                [(260.0, 280.0), (270.0, 280.0)],
            ),
            // Start=20 is before the viewport; the longest row stops at its edge, x=40.
            (
                serde_json::json!({"right_aligned": true}),
                9.0,
                true,
                [(40.0, 99.0), (69.5, 99.0)],
            ),
            (
                serde_json::json!({"width": 2.0}),
                0.0,
                true,
                [(200.0, 279.0), (200.0, 239.5)],
            ),
        ];
        for (saved, offset, boxed, expected) in cases {
            let settings = serde_json::from_value(saved.clone()).unwrap();
            let mapping = ChartMapping::new(
                Rect::from_min_size(pos2(40.0, 20.0), vec2(240.0, 160.0)),
                20.0,
                0,
                3,
                offset,
                PriceMap::new(
                    PriceTransform::default(),
                    PriceRange::new(99.0, 104.0),
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
                    render_session_volume_profiles(
                        ui.painter(),
                        &mapping,
                        &bars,
                        &profiles,
                        (&flow).into(),
                        &settings,
                        None,
                    )
                },
            );
            output.textures_delta.clear();
            let rects: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    Shape::Rect(r) if r.rect.width() > 0.0 => Some((r.rect, r.fill)),
                    _ => None,
                })
                .collect();
            // The box and the POC span the session, whatever the histogram's share.
            let (left, right) = if offset == 0.0 {
                (200.0, 279.0)
            } else if offset > 0.0 {
                (40.0, 99.0)
            } else {
                (240.0, 319.0)
            };
            let boxes: Vec<_> = rects.iter().filter(|(r, _)| r.height() == 64.0).collect();
            assert_eq!(boxes.len(), usize::from(boxed), "{saved}, offset {offset}");
            if let Some((rect, fill)) = boxes.first() {
                assert_eq!(
                    *rect,
                    Rect::from_min_max(pos2(left, 100.0), pos2(right, 164.0)),
                    "{saved}, offset {offset}"
                );
                if saved.get("session_box_color").is_some() {
                    assert_eq!(*fill, Color32::from_rgba_premultiplied(40, 50, 60, 128));
                }
            }
            let rows: Vec<_> = rects
                .iter()
                .filter(|(r, _)| r.height() == 31.0)
                .map(|(r, _)| (r.left(), r.top(), r.right(), r.bottom()))
                .collect();
            assert_eq!(
                rows,
                [
                    (expected[0].0, 132.0, expected[0].1, 163.0),
                    (expected[1].0, 100.0, expected[1].1, 131.0)
                ],
                "{saved}, offset {offset}"
            );
            let lines: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    Shape::LineSegment { points, .. } => Some((points[0].x, points[1].x)),
                    _ => None,
                })
                .collect();
            assert_eq!(lines, [(left, right)], "{saved}, offset {offset}");
        }
    }

    /// Four hundred ticks in rows of 17 give keys 100 through 123, the 24 rows asked for;
    /// rows of 16 would give 26.
    #[test]
    fn default_session_profile_groups_four_hundred_ticks_into_twenty_four_rows() {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        for price in 1700..2100 {
            flow.push(&Tick::new(time, price as f64, 1.0, Some(OrderSide::Buy)))
                .unwrap();
        }
        let bars = [Bar::new(time, 1700.0, 2099.0, 1700.0, 2099.0, 400.0)];
        let ranges = TradeProfileRanges::new(vec![time..time + TimeDelta::minutes(1)]).unwrap();
        for (rows, expected) in [
            (TradeProfileSettings::default().rows, 24),
            (crate::model::ProfileRows::Feed, 400),
        ] {
            let profiles = crate::studies::trade_profiles_with_rows(&flow, &ranges, rows);
            let settings = TradeProfileSettings {
                rows,
                session_box: false,
                poc: false,
                ..Default::default()
            };
            let mapping = ChartMapping::new(
                Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, 900.0)),
                100.0,
                0,
                0,
                0.0,
                PriceMap::new(
                    PriceTransform::default(),
                    PriceRange::new(1690.0, 2110.0),
                    0.0,
                    900.0,
                    false,
                ),
            );
            let mut output = egui::Context::default().run_ui(egui::RawInput::default(), |ui| {
                render_session_volume_profiles(
                    ui.painter(),
                    &mapping,
                    &bars,
                    &profiles,
                    (&flow).into(),
                    &settings,
                    None,
                );
            });
            output.textures_delta.clear();
            let rectangles = output
                .shapes
                .iter()
                .filter(|s| matches!(&s.shape, Shape::Rect(r) if r.rect.width() > 0.0))
                .count();
            assert_eq!(rectangles, expected, "{rows:?}");
        }
    }

    /// The rows hold bid 500 with ask 1,500 and bid 1,000 alone: totals 2K and 1K, deltas +1K and
    /// -1K. Their text fits rows 32 points tall but not 1.6, beside each bar inside the box.
    #[test]
    fn session_profile_values_follow_mode_rows_and_colour() {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        for (price, size, side) in [
            (100.0, 1500.0, OrderSide::Buy),
            (100.0, 500.0, OrderSide::Sell),
            (101.0, 1000.0, OrderSide::Sell),
        ] {
            flow.push(&Tick::new(time, price, size, Some(side)))
                .unwrap();
        }
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
        let cases: [(serde_json::Value, bool, Option<u8>, &[&str]); 11] = [
            (serde_json::json!({}), false, None, &[]),
            (
                serde_json::json!({"values": true}),
                false,
                None,
                &["500 × 1.5K", "1K × 0"],
            ),
            (
                serde_json::json!({"values": true, "values_color": [0, 0, 200, 200], "mode": "Volume"}),
                false,
                None,
                &["2K", "1K"],
            ),
            (
                serde_json::json!({"values": true, "values_color": [0, 0, 200, 200], "mode": "Delta"}),
                false,
                None,
                &["+1K", "-1K"],
            ),
            (serde_json::json!({"values": true}), true, None, &[]),
            (
                serde_json::json!({"values": true, "right_aligned": true}),
                false,
                None,
                &["500 × 1.5K", "1K × 0"],
            ),
            (
                serde_json::json!({"values": true, "mode": "DeltaVolume"}),
                false,
                None,
                &["+1K / 2K", "-1K / 1K"],
            ),
            (
                serde_json::json!({"values": true, "mode": "Ladder"}),
                false,
                None,
                &["500 × 1.5K", "1K × 0"],
            ),
            (
                serde_json::json!({"values": true, "mode": "Trades"}),
                false,
                None,
                &["+1K", "-1K"],
            ),
            (
                serde_json::json!({"values": true, "mode": "Volume"}),
                false,
                Some(0),
                &["2K", "1K"],
            ),
            (
                serde_json::json!({"values": true, "mode": "Volume"}),
                false,
                Some(3),
                &["2.000K", "1.000K"],
            ),
        ];
        for (saved, short, decimals, expected) in cases {
            let settings = serde_json::from_value(saved.clone()).unwrap();
            for theme in [Theme::dark(), Theme::light()] {
                let mapping = ChartMapping::new(
                    Rect::from_min_size(pos2(40.0, 20.0), vec2(240.0, 160.0)),
                    40.0,
                    0,
                    3,
                    0.0,
                    PriceMap::new(
                        PriceTransform::default(),
                        if short {
                            PriceRange::new(50.0, 150.0)
                        } else {
                            PriceRange::new(99.0, 104.0)
                        },
                        20.0,
                        180.0,
                        false,
                    ),
                );
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(320.0, 200.0))),
                        ..Default::default()
                    },
                    |ui| {
                        render_session_volume_profiles(
                            ui.painter(),
                            &mapping,
                            &bars,
                            &profiles,
                            (&flow).into(),
                            &settings,
                            decimals,
                        )
                    },
                );
                output.textures_delta.clear();
                let texts: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        Shape::Text(t)
                            if !matches!(t.galley.text(), "Partial executions" | "HVN" | "LVN") =>
                        {
                            Some(t)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(
                    texts.iter().map(|t| t.galley.text()).collect::<Vec<_>>(),
                    expected,
                    "{saved}, short {short}, dark {}",
                    theme.dark
                );
                let color = if saved.get("values_color").is_some() {
                    Color32::from_rgba_premultiplied(0, 0, 200, 200)
                } else {
                    theme.chart.axis_text
                };
                for t in &texts {
                    assert_eq!(t.galley.job.sections[0].format.color, color, "{saved}");
                    assert!(
                        t.pos.x >= 123.0 && t.pos.x + t.galley.size().x <= 277.001,
                        "{saved}: {t:?}"
                    );
                }
                if !short
                    && saved.get("values") == Some(&serde_json::json!(true))
                    && saved.get("mode").is_none()
                {
                    let right = saved.get("right_aligned").is_some();
                    let (first, second) = if right {
                        (texts[0].pos.x, texts[1].pos.x + texts[1].galley.size().x)
                    } else {
                        (texts[0].pos.x + texts[0].galley.size().x, texts[1].pos.x)
                    };
                    assert!(
                        (first - if right { 123.0 } else { 277.0 }).abs() < 1e-3,
                        "{saved}"
                    );
                    assert!(
                        (second - if right { 197.0 } else { 203.0 }).abs() < 1e-3,
                        "{saved}"
                    );
                }
            }
        }
    }

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

    /// Each switch hides only its line, and each colour paints its own line or rows in both
    /// themes. Older settings retain the token colours at their original row opacities.
    #[test]
    fn session_profile_lines_and_rows_follow_their_own_settings() {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        // Total 20, POC 101 (12); 70% needs 14, so add row 102 (6) rather than 100 (2).
        for (price, buy, sell) in [(100.0, 1.0, 1.0), (101.0, 8.0, 4.0), (102.0, 3.0, 3.0)] {
            for (size, side) in [(buy, OrderSide::Buy), (sell, OrderSide::Sell)] {
                flow.push(&Tick::new(time, price, size, Some(side)))
                    .unwrap();
            }
        }
        let bars = [Bar::new(time, 100.0, 102.0, 100.0, 101.0, 0.0)];
        let ranges = TradeProfileRanges::new(vec![time..time + TimeDelta::minutes(1)]).unwrap();
        let profiles = trade_profiles(&flow, &ranges);
        let mapping = ChartMapping::new(
            Rect::from_min_size(pos2(40.0, 20.0), vec2(240.0, 160.0)),
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
        let rgba = egui::Color32::from_rgba_premultiplied;
        let keys = [
            "poc_color",
            "value_area_high_color",
            "value_area_low_color",
            "up_volume_color",
            "down_volume_color",
            "value_area_up_color",
            "value_area_down_color",
        ];
        // Hand-calculated premultiplied channels: (channel * opacity + 0.5) as u8.
        let themes = [
            (
                crate::theme::Theme::dark(),
                [
                    rgba(242, 54, 69, 255),
                    rgba(184, 184, 184, 255),
                    rgba(184, 184, 184, 255),
                    rgba(13, 31, 82, 82),
                    rgba(46, 46, 42, 82),
                    rgba(37, 88, 230, 230),
                    rgba(130, 130, 119, 230),
                ],
            ),
            (
                crate::theme::Theme::light(),
                [
                    rgba(242, 54, 69, 255),
                    rgba(0, 0, 0, 255),
                    rgba(0, 0, 0, 255),
                    rgba(10, 24, 61, 61),
                    rgba(35, 35, 32, 61),
                    rgba(34, 80, 209, 209),
                    rgba(118, 118, 108, 209),
                ],
            ),
        ];
        let cases: [(serde_json::Value, &[usize]); 7] = [
            (serde_json::json!({}), &[0]),
            (serde_json::json!({"value_area_guides": true}), &[0, 1, 2]),
            (
                serde_json::json!({"value_area_guides": true, "poc": false}),
                &[1, 2],
            ),
            (
                serde_json::json!({"value_area_guides": true, "value_area_high": false}),
                &[0, 2],
            ),
            (
                serde_json::json!({"value_area_guides": true, "value_area_low": false, "poc": false}),
                &[1],
            ),
            (
                serde_json::json!({"value_area_guides": true, "poc_color": [200, 0, 0, 255],
                "value_area_high_color": [0, 200, 0, 255], "value_area_low_color": [0, 0, 100, 128],
                "up_volume_color": [60, 0, 0, 60], "down_volume_color": [0, 60, 0, 60],
                "value_area_up_color": [0, 0, 120, 120],
                "value_area_down_color": [120, 120, 0, 120]}),
                &[0, 1, 2],
            ),
            (
                serde_json::json!({"down_volume_color": [0, 60, 0, 60], "value_area_up_color": [0, 0, 120, 120]}),
                &[0],
            ),
        ];
        for (saved, lines) in cases {
            let settings: TradeProfileSettings = serde_json::from_value(saved.clone()).unwrap();
            for (theme, defaults) in &themes {
                let color = |i: usize| {
                    saved
                        .get(keys[i])
                        .map_or(defaults[i], |c| serde_json::from_value(c.clone()).unwrap())
                };
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, theme);
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(320.0, 200.0))),
                        ..Default::default()
                    },
                    |ui| {
                        render_session_volume_profiles(
                            ui.painter(),
                            &mapping,
                            &bars,
                            &profiles,
                            (&flow).into(),
                            &settings,
                            None,
                        )
                    },
                );
                output.textures_delta.clear();
                let painted = output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        Shape::LineSegment { points, stroke } => Some((points[0].y, stroke.color)),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let rows = output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        Shape::Rect(r) if r.rect.width() > 0.0 && r.rect.height() < 40.0 => {
                            Some((mapping.y_to_price(r.rect.center().y).round(), r.fill))
                        }
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                let y = [101.0, 102.5, 100.5].map(|p| mapping.price_to_y(p));
                assert_eq!(
                    painted,
                    lines.iter().map(|&i| (y[i], color(i))).collect::<Vec<_>>(),
                    "{saved}, dark {}",
                    theme.dark
                );
                assert_eq!(
                    rows,
                    [
                        (100.0, color(3)),
                        (100.0, color(4)),
                        (101.0, color(5)),
                        (101.0, color(6)),
                        (102.0, color(5)),
                        (102.0, color(6))
                    ],
                    "{saved}, dark {}",
                    theme.dark
                );
            }
        }
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
                (&flow).into(),
                &TradeProfileSettings {
                    extend_poc: true,
                    extend_vah: true,
                    extend_val: true,
                    ..Default::default()
                },
                None,
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
                (&flow).into(),
                &TradeProfileSettings {
                    extend_poc: true,
                    extend_val: true,
                    ..Default::default()
                },
                None,
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

    /// A traded tick inside a grouped row does not reach its centre or outside edges.
    #[test]
    fn prior_session_levels_end_only_when_a_traded_range_reaches_their_price() {
        use crate::model::ProfileRows;
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        let at = |minute| time + TimeDelta::minutes(minute);
        let ticks: Vec<_> = [
            (0, 100.0),
            (1, 100.0),
            (2, 100.0),
            (2, 102.0),
            (3, 103.0),
            (3, 104.0),
            (4, 99.0),
            (4, 100.0),
        ]
        .into_iter()
        .map(|(minute, price)| Tick::new(at(minute), price, 1.0, None))
        .collect();
        let bars: Vec<_> = [
            (100.0, 100.0),
            (100.0, 100.0),
            (100.0, 102.0),
            (103.0, 104.0),
            (99.0, 100.0),
        ]
        .into_iter()
        .enumerate()
        .map(|(i, (low, high))| Bar::new(at(i as i64), low, high, low, high, 1.0))
        .collect();
        let ranges = TradeProfileRanges::new(vec![time..at(1)]).unwrap();
        for (rows, expected) in [
            (ProfileRows::Ticks(4), [(101.5, 2), (103.5, 3), (99.5, 4)]),
            (ProfileRows::Count(2), [(101.5, 2), (103.5, 3), (99.5, 4)]),
            (ProfileRows::Feed, [(100.0, 1), (100.5, 2), (99.5, 4)]),
            // One traded row cannot be split, so it keeps the feed row.
            (ProfileRows::Count(24), [(100.0, 1), (100.5, 2), (99.5, 4)]),
        ] {
            let (mut ticks, mut bars) = (ticks.clone(), bars.clone());
            if rows == ProfileRows::Count(2) {
                // Eight feed rows / two requested rows = four ticks per row. The lower row's
                // 1.3 of 1.7 volume holds the 70% area alone, as the other grouped-row case does.
                ticks.extend((101..=107).map(|price| Tick::new(time, price as f64, 0.1, None)));
                ticks.sort_by_key(|tick| tick.ts);
                bars[0].high = 107.0;
                bars[0].volume = 1.7;
            }
            for tick in [1.0, 0.003, 0.1, 0.05, 0.01] {
                let ticks: Vec<_> = ticks
                    .iter()
                    .cloned()
                    .map(|mut trade| {
                        trade.price *= tick;
                        trade
                    })
                    .collect();
                let bars: Vec<_> = bars
                    .iter()
                    .cloned()
                    .map(|mut bar| {
                        bar.open *= tick;
                        bar.high *= tick;
                        bar.low *= tick;
                        bar.close *= tick;
                        bar
                    })
                    .collect();
                for inverted in [false, true] {
                    for mode in [
                        crate::scales::PriceScaleMode::Normal,
                        crate::scales::PriceScaleMode::Logarithmic,
                    ] {
                        let mut live =
                            Footprint::new(BarClock::new(Timeframe::Min1), tick, 1).unwrap();
                        for count in 1..=bars.len() {
                            for tick in ticks.iter().filter(|t| t.ts == at(count as i64 - 1)) {
                                live.push(tick).unwrap();
                            }
                            let mut rebuilt =
                                Footprint::new(BarClock::new(Timeframe::Min1), tick, 1).unwrap();
                            for tick in ticks.iter().filter(|t| t.ts < at(count as i64)) {
                                rebuilt.push(tick).unwrap();
                            }
                            for flow in [&live, &rebuilt] {
                                let transform = PriceTransform {
                                    mode,
                                    ..Default::default()
                                };
                                let mapping = ChartMapping::new(
                                    Rect::from_min_size(pos2(40.0, 20.0), vec2(500.0, 300.0)),
                                    60.0,
                                    0,
                                    4,
                                    0.0,
                                    PriceMap::new(
                                        transform,
                                        PriceRange::new(
                                            transform.to_scale(98.0 * tick),
                                            transform.to_scale(106.0 * tick),
                                        ),
                                        20.0,
                                        320.0,
                                        inverted,
                                    ),
                                );
                                let profiles =
                                    crate::studies::trade_profiles_with_rows(flow, &ranges, rows);
                                let settings = TradeProfileSettings {
                                    rows,
                                    extend_poc: true,
                                    extend_vah: true,
                                    extend_val: true,
                                    ..Default::default()
                                };
                                let mut output = egui::Context::default().run_ui(
                                    egui::RawInput::default(),
                                    |ui| {
                                        render_session_volume_profiles(
                                            ui.painter(),
                                            &mapping,
                                            &bars[..count],
                                            &profiles,
                                            flow.into(),
                                            &settings,
                                            None,
                                        );
                                    },
                                );
                                output.textures_delta.clear();
                                let start = mapping.idx_to_x(0) + mapping.bar_spacing * 0.5;
                                for (price, touch) in expected {
                                    let end = if touch < count {
                                        mapping.idx_to_x(touch)
                                    } else {
                                        mapping.rect.right()
                                    };
                                    let y = mapping.price_to_y(price * tick);
                                    let line = output.shapes.iter().any(|s| matches!(&s.shape, Shape::LineSegment {points, ..} if *points == [pos2(start,y),pos2(end,y)]));
                                    assert_eq!(
                                        line,
                                        count > 1,
                                        "{rows:?} {mode:?} inverted {inverted}, count {count}, price {price}, tick {tick}"
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// At 70%, 6 + 3 exceeds 7 of 10; after adding 8 at 100, 9 + 6 exceeds 12.6
    /// of 18. Prior levels stop independently when a later traded range reaches their price.
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
                        (flow).into(),
                        &settings,
                        None,
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
                        r#"{"developing_poc":true}"#,
                        false,
                        false,
                        false,
                        false,
                        0.7,
                    ),
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
                        r#"{"developing_poc":true,"developing_value_area":true,"extend_poc":true,"extend_vah":true,"extend_val":true,"poc_color":[200,0,0,255],"value_area_high_color":[0,200,0,255],"value_area_low_color":[0,0,100,128]}"#,
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
                    let colors: TradeProfileSettings = serde_json::from_str(saved).unwrap();
                    let poc = colors.poc_color.unwrap_or(poc_color);
                    let [high, low] = [colors.value_area_high_color, colors.value_area_low_color]
                        .map(|color| color.unwrap_or(area_color));
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
                        assert!(
                            lines.iter().all(|(points, _)| points[0].x == points[1].x
                                || points[0].y == points[1].y),
                            "levels hold until the next bar: {saved}, {mode:?}"
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
                                        poc,
                                    ),
                                    "POC joins its extension at the same price on {mode:?}"
                                );
                            }
                            for (first, last, color, enabled) in [
                                (101.0, 100.0, poc_color, colors.developing_poc),
                                (100.5, 99.5, area_color, developing),
                                (
                                    if share == 0.7 { 102.5 } else { 101.5 },
                                    if share == 0.7 { 101.5 } else { 100.5 },
                                    area_color,
                                    developing,
                                ),
                            ] {
                                let old = pos2(map.idx_to_x(0), map.price_to_y(first * step));
                                let next = pos2(map.idx_to_x(1), map.price_to_y(last * step));
                                let corner = pos2(next.x, old.y);
                                assert_eq!(
                                    contains(old, corner, color) && contains(corner, next, color),
                                    enabled,
                                    "developing step: {saved}, {mode:?}"
                                );
                            }
                        }
                        let start = (map.idx_to_x(1) + 20.0).max(map.rect.left());
                        for (price, stop, enabled, color) in [
                            (100.0, 3, extend_poc, poc),
                            (101.5, 5, extend_vah, high),
                            (99.5, 4, extend_val, low),
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
