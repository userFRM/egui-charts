//! Footprint cells painted through the chart's existing price and time mapping.

use crate::chart::coords::ChartMapping;
use crate::data::{Footprint, FootprintBar, FootprintLevel, NodeSensitivity};
use crate::model::Bar;
use egui::epaint::{Mesh, Shape};
use egui::{Align2, Color32, FontId, Painter, Rect, Stroke, StrokeKind, pos2, vec2};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The smallest and largest font size a footprint number is drawn at, in points.
const FONT_LIMITS: [f32; 2] = [8.0, 24.0];

/// Numbers shown in a footprint column.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FootprintMode {
    /// Sell volume on the left, buy volume on the right.
    BidAsk,
    /// All executed volume, including unknown sides.
    Volume,
    /// Buy minus sell volume.
    Delta,
    /// Delta followed by total volume.
    DeltaVolume,
    /// Only the larger of the sell and buy volume of the row, at the left of the cell when sells
    /// dominate and at the right when buys do, centred on a tie. The other side stays out.
    DominantSide,
}

/// What determines a footprint cell's colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FootprintColor {
    /// Volume relative to the largest row of this bar.
    Volume,
    /// Delta as a fraction of the row's classified volume.
    Delta,
    /// Dominant bid or ask side, using the configured comparison.
    Imbalance,
    /// The neutral colour for every cell, whatever was traded.
    Fixed,
    /// The buy colour for a positive delta and the sell colour for a negative one, with the
    /// opacity growing with the delta's absolute size relative to the bar's largest. A zero delta
    /// takes the neutral colour at full opacity.
    DeltaOpacity,
}

/// Footprint appearance. Supply colours from the host's theme. Serializable, so a host can keep
/// it with its chart settings; fields missing from older documents take their defaults.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct FootprintConfig {
    /// One to four columns per bar; only the first four entries are drawn.
    pub modes: Vec<FootprintMode>,
    /// Draw proportional volume profiles behind the numbers.
    pub profile: bool,
    /// Cell colouring rule.
    pub color_by: FootprintColor,
    /// Buy colour.
    pub ask_color: Color32,
    /// Sell colour.
    pub bid_color: Color32,
    /// Text colour and POC outline.
    pub text_color: Color32,
    /// Cell fill for balanced or unclassified executions.
    pub neutral_color: Color32,
    /// Group adjacent rows automatically to at least this screen height. `None` keeps the
    /// aggregation's manual grouping. Grouping is an integer multiple of the input step.
    pub auto_row_height: Option<f32>,
    /// Hide numbers when a row is shorter than this height or the column cannot fit them.
    pub min_text_height: f32,
    /// Dominance multiple, normally 3 (300%).
    pub imbalance_ratio: f64,
    /// Compare to the opposite side one price away instead of at the same price.
    pub diagonal: bool,
    /// Outline the highest-volume row.
    pub show_poc: bool,
    /// Shade the contiguous value area containing this share of the bar volume.
    pub value_area: Option<f64>,
    /// Trade profiles: the guide along the top edge of the value area (VAH); `None` hides it.
    pub value_area_high: Option<Stroke>,
    /// Trade profiles: the guide along the bottom edge of the value area (VAL); `None` hides it.
    pub value_area_low: Option<Stroke>,
    /// Trade profiles: how volume nodes are chosen.
    pub node_sensitivity: NodeSensitivity,
    /// Write 1,200 as 1.2K and 3,400,000 as 3.4M, instead of every digit.
    pub abbreviate: bool,
    /// Write zero volumes as 0. Otherwise they are left blank, so a one-sided row shows only
    /// the side that traded.
    pub show_zero: bool,
    /// Smallest and largest font size of the numbers, in points. A row's size follows its volume
    /// relative to the bar's largest row, and equal values give a fixed size. Both are held
    /// within 8 and 24 points so that numbers stay readable, and a number whose size exceeds
    /// its row's height is left out.
    pub font_size: [f32; 2],
    /// Write the ask/bid ratio of the highest and lowest row of each bar above and below it,
    /// or `n/a` when the bid is zero and the ratio is undefined.
    pub extreme_ratio: bool,
    /// Outline the rows of the bar's pullback range, see [`pullback_range`].
    pub pullback: bool,
}

impl Default for FootprintConfig {
    fn default() -> Self {
        Self {
            modes: vec![FootprintMode::BidAsk],
            profile: true,
            color_by: FootprintColor::Delta,
            ask_color: crate::tokens::DESIGN_TOKENS.semantic.chart.bullish,
            bid_color: crate::tokens::DESIGN_TOKENS.semantic.chart.bearish,
            text_color: Color32::WHITE,
            neutral_color: Color32::GRAY,
            auto_row_height: Some(16.0),
            min_text_height: 12.0,
            imbalance_ratio: 3.0,
            diagonal: true,
            show_poc: true,
            value_area: Some(0.7),
            value_area_high: Some(Stroke::new(1.0, Color32::GRAY)),
            value_area_low: Some(Stroke::new(1.0, Color32::GRAY)),
            node_sensitivity: NodeSensitivity::default(),
            abbreviate: false,
            show_zero: true,
            font_size: [10.0, 10.0],
            extreme_ratio: false,
            pullback: false,
        }
    }
}

/// The rows of a bar between its close and the extreme it reached beyond the close: from the
/// close up to the highest traded row when the bar closed at or above its open, and from the
/// lowest traded row up to the close when it closed below. Returns inclusive level keys of
/// `flow`, or `None` when the bar has no executions or closed on its extreme row. The range
/// follows the bar as it develops, and it only selects rows: their executed volume is never
/// changed.
pub fn pullback_range(bar: &Bar, flow: &Footprint) -> Option<(i64, i64)> {
    let source = flow.bar_at(bar.time)?;
    let low = *source.levels.first_key_value()?.0;
    let high = *source.levels.last_key_value()?.0;
    let close = flow.level_of(bar.close)?.clamp(low, high);
    let range = if bar.close >= bar.open {
        (close, high)
    } else {
        (low, close)
    };
    (range.0 < range.1).then_some(range)
}

/// The width of a column of cells at `bar_spacing` points per bar, its half-point gap included,
/// or `None` when cells would be under a point wide. Those are unreadable and, painted without
/// antialiasing, can fall between pixel centres and draw nothing at all.
pub(crate) fn footprint_column_width(bar_spacing: f32, config: &FootprintConfig) -> Option<f32> {
    let columns = config.modes.len().min(4);
    let width = bar_spacing * 0.92 / columns as f32;
    (columns > 0 && bar_spacing.is_finite() && width - 0.5 >= 1.0).then_some(width)
}

/// Draws visible footprint bars, bounded by the visible bar range and clipped to `mapping.rect`.
///
/// For the chart widget use [`crate::widget::Chart::show_footprint`] to keep cells below its
/// legend and cursor. Standalone hosts supply their pane mapping and source bars. Use only
/// time-based bars whose timestamps lie inside their aggregation slots. Text layout uses
/// egui's galley cache; rows below the minimum height
/// never request a layout. Row bands extend half an instrument tick below their first
/// tick and above their last tick, including grouped rows. Fractional volumes below 100 retain
/// up to three decimals; nonzero magnitudes below 0.001 use scientific notation. Only display
/// grouping runs during painting. Cells under a point wide are too narrow to read and are skipped
/// before looking up executions.
pub fn render_footprint(
    painter: &Painter,
    mapping: &ChartMapping,
    bars: &[Bar],
    flow: &Footprint,
    config: &FootprintConfig,
) {
    let Some(width) = footprint_column_width(mapping.bar_spacing, config) else {
        return;
    };
    if bars.is_empty() {
        return;
    }
    let painter = painter.with_clip_rect(mapping.rect.intersect(painter.clip_rect()));
    let first = mapping
        .x_to_idx(mapping.rect.left())
        .saturating_sub(1)
        .min(bars.len());
    let end = mapping
        .x_to_idx(mapping.rect.right())
        .saturating_add(2)
        .min(bars.len());
    // One mesh avoids one tessellation job and allocation per cell at wide zooms.
    let background = painter.add(Shape::Noop);
    let mut mesh = Mesh::default();
    let columns = config.modes.len().min(4);
    // Bars cut by dropped executions are drawn as candles, not as complete footprint bars.
    let complete_from = flow.complete_from();
    for (index, bar) in bars.iter().enumerate().take(end).skip(first) {
        let Some(source) = flow
            .bar_at(bar.time)
            .filter(|_| complete_from.is_some_and(|from| bar.time >= from))
        else {
            continue;
        };
        let x = mapping.idx_to_x(index);
        // Automatic grouping uses the local price mapping, including log and inverted scales.
        let raw_height = (mapping.price_to_y(bar.close + flow.price_step())
            - mapping.price_to_y(bar.close))
        .abs();
        let group = config
            .auto_row_height
            .filter(|h| h.is_finite() && *h > 0.0)
            .map_or(1, |height| {
                (height / raw_height.max(0.01))
                    .ceil()
                    .clamp(1.0, 1_000_000.0) as i64
            });
        let grouped;
        let shown = if group > 1 {
            grouped = group_bar(source, group);
            &grouped
        } else {
            source
        };
        let max_volume = shown
            .levels
            .values()
            .map(|row| row.volume())
            .fold(0.0, f64::max);
        let max_delta = shown
            .levels
            .values()
            .map(|row| row.delta().abs())
            .fold(0.0, f64::max);
        let area = config.value_area.and_then(|share| shown.value_area(share));
        let poc = config.show_poc.then(|| shown.poc()).flatten();
        let pullback = config
            .pullback
            .then(|| pullback_range(bar, flow))
            .flatten()
            .map(|(low, high)| (low.div_euclid(group), high.div_euclid(group)));
        let [smallest, largest] = font_limits(config.font_size);
        for row in shown.levels.values() {
            let price =
                row.level as f64 * flow.price_step() * group as f64 - flow.tick_size() * 0.5;
            let y1 = mapping.price_to_y(price);
            let y2 = mapping.price_to_y(price + flow.price_step() * group as f64);
            if !y1.is_finite() || !y2.is_finite() {
                continue;
            }
            for (column, mode) in config.modes.iter().take(columns).enumerate() {
                let left = x - mapping.bar_spacing * 0.46 + column as f32 * width;
                let rect = Rect::from_min_max(
                    pos2(left, y1.min(y2)),
                    pos2(left + width - 0.5, y1.max(y2)),
                );
                if !rect.intersects(mapping.rect) {
                    continue;
                }
                let color = cell_color(shown, row, max_volume, max_delta, config);
                if area.is_some_and(|(low, high)| (low..=high).contains(&row.level)) {
                    mesh.add_colored_rect(
                        rect.intersect(mapping.rect),
                        config.neutral_color.gamma_multiply(0.12),
                    );
                }
                let share = if config.profile && max_volume > 0.0 {
                    (row.volume() / max_volume) as f32
                } else {
                    1.0
                };
                let fill = Rect::from_min_size(
                    rect.min,
                    vec2(rect.width() * share, rect.height().max(1.0)),
                );
                if config.color_by == FootprintColor::Imbalance
                    && shown.imbalance(row.level, config.imbalance_ratio, config.diagonal)
                        == (true, true)
                {
                    let (bid, ask) = fill.split_left_right_at_x(fill.center().x);
                    mesh.add_colored_rect(
                        bid.intersect(mapping.rect),
                        config.bid_color.gamma_multiply(0.5),
                    );
                    mesh.add_colored_rect(
                        ask.intersect(mapping.rect),
                        config.ask_color.gamma_multiply(0.5),
                    );
                } else {
                    mesh.add_colored_rect(fill.intersect(mapping.rect), color.gamma_multiply(0.5));
                }
                if pullback.is_some_and(|(low, high)| (low..=high).contains(&row.level)) {
                    painter.rect_stroke(
                        rect,
                        0,
                        Stroke::new(1.0, config.neutral_color),
                        StrokeKind::Inside,
                    );
                }
                if poc == Some(row.level) {
                    painter.rect_stroke(
                        rect,
                        0,
                        Stroke::new(1.0, config.text_color),
                        StrokeKind::Inside,
                    );
                }
                if rect.height() >= config.min_text_height.max(10.0) && width >= 22.0 {
                    let text = cell_text(*mode, row, config);
                    if text.is_empty() {
                        continue;
                    }
                    let share = if max_volume > 0.0 {
                        (row.volume() / max_volume) as f32
                    } else {
                        0.0
                    };
                    let size = smallest + (largest - smallest) * share;
                    let galley =
                        painter.layout_no_wrap(text, FontId::monospace(size), config.text_color);
                    if galley.size().x + 4.0 <= rect.width() && size <= rect.height() {
                        let center = rect.center().x;
                        let x = match mode {
                            FootprintMode::DominantSide if row.bid > row.ask => rect.left() + 2.0,
                            FootprintMode::DominantSide if row.ask > row.bid => {
                                rect.right() - 2.0 - galley.size().x
                            }
                            _ => center - galley.size().x / 2.0,
                        };
                        let at = pos2(x, rect.center().y - galley.size().y / 2.0);
                        // Text is whole or absent: a cell cut by the plot edge shows no number.
                        if mapping
                            .rect
                            .contains_rect(Rect::from_min_size(at, galley.size()))
                        {
                            painter.galley(at, galley, config.text_color);
                        }
                    }
                }
            }
        }
        if config.extreme_ratio {
            let step = flow.price_step() * group as f64;
            let first = shown.levels.first_key_value();
            let last = shown.levels.last_key_value();
            // A bar of one row has a single extreme.
            for (extreme, high) in [
                (last, true),
                (first.filter(|_| shown.levels.len() > 1), false),
            ] {
                let Some((&level, row)) = extreme else {
                    continue;
                };
                let edge = mapping
                    .price_to_y((level + i64::from(high)) as f64 * step - flow.tick_size() * 0.5);
                let text = row.ratio().map_or_else(
                    || "A/B n/a".to_owned(),
                    |ratio| {
                        format!(
                            "A/B {}",
                            egui::emath::format_with_decimals_in_range(ratio, 0..=2)
                        )
                    },
                );
                let galley =
                    painter.layout_no_wrap(text, FontId::monospace(10.0), config.text_color);
                let anchor = if high {
                    Align2::CENTER_BOTTOM
                } else {
                    Align2::CENTER_TOP
                };
                let bounds = anchor.anchor_size(pos2(x, edge), galley.size());
                if edge.is_finite()
                    && galley.size().x <= mapping.bar_spacing
                    && mapping.rect.contains_rect(bounds)
                {
                    painter.galley(bounds.min, galley, config.text_color);
                }
            }
        }
    }
    painter.set(background, Shape::mesh(mesh));
}

/// The font sizes of the smallest and the largest row: the configured pair within the readable
/// limits, in ascending order; the default size for one that is not a number.
fn font_limits(sizes: [f32; 2]) -> [f32; 2] {
    let [a, b] = sizes.map(|size| {
        if size.is_nan() {
            10.0
        } else {
            size.clamp(FONT_LIMITS[0], FONT_LIMITS[1])
        }
    });
    [a.min(b), a.max(b)]
}

/// The text of one cell in `mode`; empty when every number in it is a hidden zero.
fn cell_text(mode: FootprintMode, row: &FootprintLevel, config: &FootprintConfig) -> String {
    let hidden = |value: f64| value == 0.0 && !config.show_zero;
    let number = |value: f64| {
        if hidden(value) {
            String::new()
        } else {
            volume_text(value, config.abbreviate)
        }
    };
    let signed = |value: f64| match number(value) {
        text if text.is_empty() || value < 0.0 => text,
        text => format!("+{text}"),
    };
    match mode {
        FootprintMode::BidAsk if hidden(row.bid) && hidden(row.ask) => String::new(),
        FootprintMode::BidAsk => format!("{} × {}", number(row.bid), number(row.ask)),
        FootprintMode::Volume => number(row.volume()),
        FootprintMode::Delta => signed(row.delta()),
        FootprintMode::DeltaVolume if hidden(row.delta()) && hidden(row.volume()) => String::new(),
        FootprintMode::DeltaVolume => {
            format!("{} / {}", signed(row.delta()), number(row.volume()))
        }
        FootprintMode::DominantSide => number(row.bid.max(row.ask)),
    }
}

fn volume_text(value: f64, abbreviate: bool) -> String {
    if abbreviate && value.abs() >= 1000.0 {
        // 999,950 and above round to 1000.0K at one decimal, so they take the next unit.
        let (scaled, unit) = if value.abs() >= 999_950.0 {
            (value / 1e6, 'M')
        } else {
            (value / 1e3, 'K')
        };
        let text = format!("{scaled:.1}");
        return format!("{}{unit}", text.strip_suffix(".0").unwrap_or(&text));
    }
    if value != 0.0 && value.abs() < 0.001 {
        format!("{value:.2e}")
    } else {
        egui::emath::format_with_decimals_in_range(
            value,
            0..=if value.abs() < 100.0 { 3 } else { 0 },
        )
    }
}

fn group_bar(source: &FootprintBar, group: i64) -> FootprintBar {
    let mut shown = FootprintBar {
        time: source.time,
        levels: BTreeMap::new(),
        bid: source.bid,
        ask: source.ask,
        unknown: source.unknown,
        min_delta: source.min_delta,
        max_delta: source.max_delta,
    };
    for row in source.levels.values() {
        let key = row.level.div_euclid(group);
        let target = shown.levels.entry(key).or_insert_with(|| FootprintLevel {
            level: key,
            ..Default::default()
        });
        target.bid += row.bid;
        target.ask += row.ask;
        target.unknown += row.unknown;
    }
    shown
}

fn cell_color(
    bar: &FootprintBar,
    row: &FootprintLevel,
    max_volume: f64,
    max_delta: f64,
    config: &FootprintConfig,
) -> Color32 {
    match config.color_by {
        FootprintColor::Fixed => config.neutral_color,
        FootprintColor::DeltaOpacity if row.delta() == 0.0 => config.neutral_color,
        FootprintColor::DeltaOpacity => {
            let side = if row.delta() > 0.0 {
                config.ask_color
            } else {
                config.bid_color
            };
            side.gamma_multiply((row.delta().abs() / max_delta) as f32)
        }
        FootprintColor::Volume => config.neutral_color.gamma_multiply(if max_volume > 0.0 {
            (row.volume() / max_volume) as f32
        } else {
            0.0
        }),
        FootprintColor::Delta if row.bid + row.ask == 0.0 => config.neutral_color,
        FootprintColor::Delta => {
            let share = if row.bid + row.ask > 0.0 {
                row.ask / (row.bid + row.ask)
            } else {
                0.5
            };
            config
                .bid_color
                .lerp_to_gamma(config.ask_color, share as f32)
        }
        FootprintColor::Imbalance => {
            match bar.imbalance(row.level, config.imbalance_ratio, config.diagonal) {
                (true, false) => config.bid_color,
                (false, true) => config.ask_color,
                _ => config.neutral_color,
            }
        }
    }
}

/// Draws a compact per-bar statistics table under the footprint at the same x coordinates.
/// Rows show total, bid, ask, delta, minimum and maximum intrabar delta, cumulative delta,
/// delta percentage, and ask/bid ratio (undefined for zero volume or bid).
/// A column too narrow for text, or cut by the plot edge, is left empty; values are still
/// available in [`FootprintBar`].
pub fn render_footprint_statistics(
    painter: &Painter,
    mapping: &ChartMapping,
    bars: &[Bar],
    flow: &Footprint,
    text_color: Color32,
) {
    let painter = painter.with_clip_rect(mapping.rect.intersect(painter.clip_rect()));
    let mut cumulative = 0.0;
    for (i, bar) in bars.iter().enumerate() {
        let Some(row) = flow.bar_at(bar.time) else {
            continue;
        };
        cumulative += row.delta();
        let x = mapping.idx_to_x(i);
        if !mapping.is_x_visible(x) || mapping.bar_spacing < 48.0 {
            continue;
        }
        for (line, (label, value)) in [
            ("Vol", Some(row.volume())),
            ("Bid", Some(row.bid)),
            ("Ask", Some(row.ask)),
            ("Delta", Some(row.delta())),
            ("Min", Some(row.min_delta)),
            ("Max", Some(row.max_delta)),
            ("CVD", Some(cumulative)),
            ("Delta %", row.delta_percent()),
            ("Ask/Bid", (row.bid > 0.0).then(|| row.ask / row.bid)),
        ]
        .into_iter()
        .enumerate()
        {
            let text = value.map_or_else(
                || format!("{label} n/a"),
                |value| format!("{label} {}", volume_text(value, false)),
            );
            let galley = painter.layout_no_wrap(text, FontId::monospace(10.0), text_color);
            let at = pos2(
                x - galley.size().x / 2.0,
                mapping.rect.top() + 12.0 * line as f32,
            );
            // Whole or absent, as in the cells: a column cut by the plot edge shows no number.
            if galley.size().x <= mapping.bar_spacing - 2.0
                && mapping
                    .rect
                    .contains_rect(Rect::from_min_size(at, galley.size()))
            {
                painter.galley(at, galley, text_color);
            }
        }
    }
}

/// Paints footprint study levels and zones. Compute `studies` only after executions change.
/// Stacks, naked POCs and unfinished levels extend to their first revisit, their expiry or the
/// last displayed bar, in full colour while unvisited and dimmed once revisited or expired;
/// local annotations remain inside their originating bar. Delta percentage and ratio markers
/// are triangles outside the bar: buy markers above it, sell markers below, the delta marker
/// nearest and the ratio marker beyond it. Prices use the aggregation's row step.
#[cfg(feature = "studies")]
#[cfg_attr(docsrs, doc(cfg(feature = "studies")))]
pub fn render_footprint_studies(
    painter: &Painter,
    mapping: &ChartMapping,
    bars: &[Bar],
    flow: &Footprint,
    studies: &[crate::studies::FootprintStudy],
    config: &FootprintConfig,
) {
    use crate::studies::{FootprintStudyKind, FootprintStudyState};
    let painter = painter.with_clip_rect(mapping.rect.intersect(painter.clip_rect()));
    let indices: BTreeMap<_, _> = bars
        .iter()
        .enumerate()
        .filter_map(|(i, bar)| flow.bar_at(bar.time).map(|flow_bar| (flow_bar.time, i)))
        .collect();
    let mut labels: Vec<Rect> = Vec::new();
    for study in studies {
        let Some(&start) = indices.get(&study.start) else {
            continue;
        };
        let end = study
            .end
            .and_then(|time| indices.get(&time).copied())
            .unwrap_or(bars.len().saturating_sub(1));
        let left = mapping.idx_to_x(start) - mapping.bar_spacing * 0.46;
        let right = mapping.idx_to_x(end) + mapping.bar_spacing * 0.46;
        let y1 = mapping.price_to_y(study.low as f64 * flow.price_step() - flow.tick_size() * 0.5);
        let y2 = mapping
            .price_to_y((study.high as f64 + 1.0) * flow.price_step() - flow.tick_size() * 0.5);
        if !y1.is_finite() || !y2.is_finite() {
            continue;
        }
        let rect = Rect::from_min_max(pos2(left, y1.min(y2)), pos2(right, y1.max(y2)));
        if !rect.intersects(mapping.rect) {
            continue;
        }
        let color = match study.kind {
            FootprintStudyKind::StackedBid
            | FootprintStudyKind::DeltaBid
            | FootprintStudyKind::RatioBid => config.bid_color,
            FootprintStudyKind::StackedAsk
            | FootprintStudyKind::DeltaAsk
            | FootprintStudyKind::RatioAsk => config.ask_color,
            _ => config.text_color,
        };
        let color = match study.state {
            FootprintStudyState::Revisited | FootprintStudyState::Expired => {
                color.gamma_multiply(0.45)
            }
            _ => color,
        };
        let marker = match study.kind {
            FootprintStudyKind::DeltaAsk | FootprintStudyKind::DeltaBid => Some(0.0),
            FootprintStudyKind::RatioAsk | FootprintStudyKind::RatioBid => Some(1.0),
            _ => None,
        };
        if let Some(order) = marker {
            let above = matches!(
                study.kind,
                FootprintStudyKind::DeltaAsk | FootprintStudyKind::RatioAsk
            );
            let (edge, away) = if above {
                (rect.top(), -1.0)
            } else {
                (rect.bottom(), 1.0)
            };
            let tip = edge + away * (3.0 + 9.0 * order);
            let base = tip + away * 6.0;
            let x = (rect.left() + rect.right()) / 2.0;
            painter.add(Shape::convex_polygon(
                vec![pos2(x, tip), pos2(x - 4.0, base), pos2(x + 4.0, base)],
                color,
                Stroke::NONE,
            ));
            continue;
        }
        if matches!(
            study.kind,
            FootprintStudyKind::StackedBid
                | FootprintStudyKind::StackedAsk
                | FootprintStudyKind::VolumeCluster
        ) {
            painter.rect_filled(rect, 0, color.gamma_multiply(0.12));
        }
        if mapping.bar_spacing >= 48.0 {
            let label = match study.kind {
                FootprintStudyKind::StackedBid => "Bid stack",
                FootprintStudyKind::StackedAsk => "Ask stack",
                FootprintStudyKind::UnfinishedHigh => "UH",
                FootprintStudyKind::UnfinishedLow => "UL",
                FootprintStudyKind::NakedPoc => "POC",
                FootprintStudyKind::DeltaPoc => "Delta POC",
                FootprintStudyKind::VolumeCluster => "Cluster",
                FootprintStudyKind::SinglePrint => "Single",
                _ => continue,
            };
            let (at, anchor) = if matches!(
                study.kind,
                FootprintStudyKind::StackedAsk
                    | FootprintStudyKind::StackedBid
                    | FootprintStudyKind::NakedPoc
                    | FootprintStudyKind::UnfinishedHigh
                    | FootprintStudyKind::UnfinishedLow
            ) {
                (rect.right_top(), Align2::RIGHT_BOTTOM)
            } else {
                (rect.left_center(), Align2::RIGHT_CENTER)
            };
            let galley = painter.layout_no_wrap(label.into(), FontId::monospace(10.0), color);
            let bounds = anchor.anchor_size(at, galley.size());
            if mapping.rect.contains_rect(bounds)
                && !labels.iter().any(|label| label.intersects(bounds))
            {
                painter.galley(bounds.min, galley, color);
                labels.push(bounds);
            }
        }
        painter.rect_stroke(rect, 0, Stroke::new(1.0, color), StrokeKind::Inside);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{BarClock, OrderSide, Tick, Timeframe};
    use crate::scales::{PriceMap, PriceRange, PriceTransform};

    /// Rows cover the nearest-tick band; spacing that leaves cells under a point wide produces no
    /// geometry.
    #[test]
    fn rendered_rows_surround_their_instrument_ticks() {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        // y(P) = (high - P) / (high - 99) * 160, calculated independently.
        #[rustfmt::skip]
        let cases = [
            (1.63, 1.0, 1, None, 100.0, 102.0, None),
            (1.64, 1.0, 1, None, 100.0, 102.0, Some((80.0, 133.33333))),
            (100.0, 1.0, 1, None, 100.0, 102.0, Some((80.0, 133.33333))),
            (100.0, 0.25, 1, None, 100.25, 102.0, Some((86.66667, 100.0))),
            (100.0, 1.0, 4, None, 100.0, 106.0, Some((57.142857, 148.57143))),
            (100.0, 1.0, 1, Some(75.0), 100.0, 106.0, Some((57.142857, 148.57143))),
        ];
        for (spacing, tick, ticks, auto, price, high, extent) in cases {
            let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), tick, ticks).unwrap();
            flow.push(&Tick::new(time, price, 3.0, Some(OrderSide::Buy)))
                .unwrap();
            let bars = [Bar::new(time, price, price, price, price, 3.0)];
            let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(240.0, 160.0));
            let map = ChartMapping::new(
                rect,
                spacing,
                0,
                0,
                0.0,
                PriceMap::new(
                    PriceTransform::default(),
                    PriceRange::new(99.0, high),
                    0.0,
                    160.0,
                    false,
                ),
            );
            let config = FootprintConfig {
                auto_row_height: auto,
                show_poc: false,
                value_area: None,
                ..Default::default()
            };
            let mut output = egui::Context::default().run_ui(
                egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |ui| render_footprint(ui.painter(), &map, &bars, &flow, &config),
            );
            output.textures_delta.clear();
            let Some((top, bottom)) = extent else {
                assert!(
                    output.shapes.is_empty(),
                    "spacing {spacing}: unreadable cells"
                );
                continue;
            };
            let extent = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    Shape::Mesh(mesh) => Some(mesh),
                    _ => None,
                })
                .flat_map(|mesh| mesh.vertices.iter())
                .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), v| {
                    (lo.min(v.pos.y), hi.max(v.pos.y))
                });
            assert!(
                (extent.0 - top).abs() < 0.01 && (extent.1 - bottom).abs() < 0.01,
                "spacing {spacing}, tick {tick}, ticks {ticks}, auto {auto:?}: {extent:?}, expected {top}..{bottom}"
            );
        }
    }

    /// A bar cut by dropped executions is left to its candle, not drawn as a complete bar.
    #[test]
    fn bar_cut_by_dropped_executions_is_not_drawn() {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 0.25, 1).unwrap();
        let trades = [100.0, 100.25].map(|price| Tick::new(time, price, 3.0, Some(OrderSide::Buy)));
        trades.iter().for_each(|trade| _ = flow.push(trade));
        flow.drop_through(time, &trades[1..]);
        assert!(flow.bar_at(time).is_some());
        let bars = [Bar::new(time, 100.0, 100.25, 100.0, 100.25, 3.0)];
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(240.0, 160.0));
        let map = ChartMapping::new(
            rect,
            240.0,
            0,
            0,
            0.0,
            PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(99.0, 102.0),
                0.0,
                160.0,
                false,
            ),
        );
        let config = FootprintConfig::default();
        let mut output = egui::Context::default().run_ui(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ui| render_footprint(ui.painter(), &map, &bars, &flow, &config),
        );
        output.textures_delta.clear();
        assert!(output.shapes.iter().all(|shape| match &shape.shape {
            Shape::Text(_) => false,
            Shape::Mesh(mesh) => mesh.is_empty(),
            _ => true,
        }));
    }

    #[test]
    fn four_modes_hide_small_text_and_group_adjacent_rows() {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 0.25, 1).unwrap();
        for (price, size, side) in [
            (100.0, 2.0, OrderSide::Sell),
            (100.0, 3.0, OrderSide::Buy),
            (100.25, 4.0, OrderSide::Buy),
        ] {
            flow.push(&Tick::new(time, price, size, Some(side)))
                .unwrap();
        }
        let bars = [Bar::new(time, 100.0, 100.25, 100.0, 100.25, 9.0)];
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(240.0, 160.0));
        let mapping = |high| {
            ChartMapping::new(
                rect,
                240.0,
                0,
                0,
                0.0,
                PriceMap::new(
                    PriceTransform::default(),
                    PriceRange::new(99.0, high),
                    0.0,
                    160.0,
                    false,
                ),
            )
        };
        let map = mapping(102.0);
        // Rows are 13.3 points high, or 10.5 with a top of 102.8: still tall enough for the
        // default 10-point numbers. The last case puts the plot edge through the first column:
        // its numbers are left out.
        for (minimum, auto, high, left, expected) in [
            (14.0, None, 102.0, 0.0, 0),
            (10.0, None, 102.0, 0.0, 8),
            (10.0, Some(30.0), 102.0, 0.0, 4),
            (10.0, None, 102.8, 0.0, 8),
            (10.0, None, 102.0, 30.0, 6),
        ] {
            let map = mapping(high);
            let plot = ChartMapping::new(
                Rect::from_min_max(pos2(left, 0.0), rect.max),
                240.0,
                0,
                0,
                0.0,
                map.price,
            );
            let ctx = egui::Context::default();
            let config = FootprintConfig {
                modes: vec![
                    FootprintMode::BidAsk,
                    FootprintMode::Volume,
                    FootprintMode::Delta,
                    FootprintMode::DeltaVolume,
                ],
                min_text_height: minimum,
                auto_row_height: auto,
                ..Default::default()
            };
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |ui| render_footprint(ui.painter(), &plot, &bars, &flow, &config),
            );
            output.textures_delta.clear();
            let texts: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Text(text) => Some(text.galley.text()),
                    _ => None,
                })
                .collect();
            assert_eq!(
                texts.len(),
                expected,
                "minimum {minimum}, auto {auto:?}, top {high}, left {left}: {texts:?}"
            );
            if auto.is_some() {
                assert_eq!(texts, ["2 × 7", "9", "+5", "+5 / 9"]);
            }
        }
        // Both diagonals can dominate at the same price; keep both side colours.
        let mut dual = Footprint::new(BarClock::new(Timeframe::Min1), 0.25, 1).unwrap();
        for (price, size, side) in [
            (100.0, 1.0, OrderSide::Sell),
            (100.25, 9.0, OrderSide::Sell),
            (100.25, 12.0, OrderSide::Buy),
            (100.5, 2.0, OrderSide::Buy),
        ] {
            dual.push(&Tick::new(time, price, size, Some(side)))
                .unwrap();
        }
        let config = FootprintConfig {
            color_by: FootprintColor::Imbalance,
            auto_row_height: None,
            value_area: None,
            show_poc: false,
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ui| render_footprint(ui.painter(), &map, &bars, &dual, &config),
        );
        output.textures_delta.clear();
        for color in [config.bid_color, config.ask_color] {
            assert!(
                output.shapes.iter().any(|shape| match &shape.shape {
                    Shape::Mesh(mesh) => mesh
                        .vertices
                        .iter()
                        .any(|v| v.color == color.gamma_multiply(0.5)),
                    _ => false,
                }),
                "both diagonal sides must be visible"
            );
        }
        // Fractional executions must not be labelled as zero in any display mode.
        let mut small = Footprint::new(BarClock::new(Timeframe::Min1), 0.25, 1).unwrap();
        small
            .push(&Tick::new(time, 100.0, 0.125, Some(OrderSide::Sell)))
            .unwrap();
        small
            .push(&Tick::new(time, 100.0, 0.375, Some(OrderSide::Buy)))
            .unwrap();
        for (mode, expected) in [
            (FootprintMode::BidAsk, "0.125 × 0.375"),
            (FootprintMode::Volume, "0.5"),
            (FootprintMode::Delta, "+0.25"),
            (FootprintMode::DeltaVolume, "+0.25 / 0.5"),
        ] {
            let config = FootprintConfig {
                modes: vec![mode],
                min_text_height: 10.0,
                auto_row_height: None,
                ..Default::default()
            };
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |ui| render_footprint(ui.painter(), &map, &bars, &small, &config),
            );
            output.textures_delta.clear();
            assert!(
                output.shapes.iter().any(|shape| match &shape.shape {
                    Shape::Text(text) => text.galley.text() == expected,
                    _ => false,
                }),
                "missing {expected}"
            );
        }
        // The highest and lowest rows carry their ask/bid ratio; a zero bid is undefined, not
        // infinite. Here 100.25 holds buys only and 100.0 holds 2 sells against 3 buys.
        let config = FootprintConfig {
            modes: vec![FootprintMode::Volume],
            extreme_ratio: true,
            auto_row_height: None,
            ..Default::default()
        };
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 0.25, 1).unwrap();
        for (price, size, side) in [
            (100.0, 2.0, OrderSide::Sell),
            (100.0, 3.0, OrderSide::Buy),
            (100.25, 4.0, OrderSide::Buy),
        ] {
            flow.push(&Tick::new(time, price, size, Some(side)))
                .unwrap();
        }
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ui| render_footprint(ui.painter(), &map, &bars, &flow, &config),
        );
        output.textures_delta.clear();
        for expected in ["A/B n/a", "A/B 1.5"] {
            assert!(
                output.shapes.iter().any(|shape| match &shape.shape {
                    Shape::Text(text) => text.galley.text() == expected,
                    _ => false,
                }),
                "missing {expected}"
            );
        }
    }

    /// Guards the number and colour choices: abbreviation switches units before one decimal
    /// would round up to 1000, a hidden zero leaves only the side that traded, the dominant
    /// side is the only number of its cell, delta opacity follows the share of the bar's largest
    /// delta, and font limits keep numbers readable whatever the host asks for.
    #[test]
    fn number_colour_and_font_choices_follow_their_documented_rules() {
        let row = |bid, ask, unknown| FootprintLevel {
            level: 0,
            bid,
            ask,
            unknown,
        };
        let config = |abbreviate, show_zero| FootprintConfig {
            abbreviate,
            show_zero,
            ..Default::default()
        };
        #[rustfmt::skip]
        let texts = [
            (FootprintMode::Volume, row(0.0, 1234.0, 0.0), config(true, true), "1.2K"),
            (FootprintMode::Volume, row(0.0, 1234.0, 0.0), config(false, true), "1234"),
            (FootprintMode::Volume, row(0.0, 999.0, 0.0), config(true, true), "999"),
            (FootprintMode::Volume, row(0.0, 1000.0, 0.0), config(true, true), "1K"),
            (FootprintMode::Volume, row(0.0, 999_949.0, 0.0), config(true, true), "999.9K"),
            (FootprintMode::Volume, row(0.0, 999_950.0, 0.0), config(true, true), "1M"),
            (FootprintMode::Volume, row(0.0, 3_400_000.0, 0.0), config(true, true), "3.4M"),
            (FootprintMode::Delta, row(2500.0, 0.0, 0.0), config(true, true), "-2.5K"),
            (FootprintMode::BidAsk, row(0.0, 7.0, 0.0), config(false, false), " × 7"),
            (FootprintMode::BidAsk, row(0.0, 0.0, 4.0), config(false, false), ""),
            (FootprintMode::BidAsk, row(0.0, 0.0, 4.0), config(false, true), "0 × 0"),
            (FootprintMode::Delta, row(3.0, 3.0, 0.0), config(false, false), ""),
            (FootprintMode::DominantSide, row(5.0, 3.0, 0.0), config(false, true), "5"),
            (FootprintMode::DominantSide, row(2.0, 3.0, 9.0), config(false, true), "3"),
        ];
        for (mode, row, config, expected) in texts {
            assert_eq!(cell_text(mode, &row, &config), expected, "{mode:?} {row:?}");
        }
        let bar = FootprintBar {
            time: "2026-01-05T10:00:00Z".parse().unwrap(),
            levels: BTreeMap::new(),
            bid: 0.0,
            ask: 0.0,
            unknown: 0.0,
            min_delta: 0.0,
            max_delta: 0.0,
        };
        let colors = |color_by, row: FootprintLevel| {
            let config = FootprintConfig {
                color_by,
                ..Default::default()
            };
            (cell_color(&bar, &row, 10.0, 6.0, &config), config)
        };
        let (color, config) = colors(FootprintColor::DeltaOpacity, row(2.0, 8.0, 0.0));
        assert_eq!(color, config.ask_color.gamma_multiply(1.0));
        let (color, config) = colors(FootprintColor::DeltaOpacity, row(5.0, 2.0, 0.0));
        assert_eq!(color, config.bid_color.gamma_multiply(0.5));
        let (color, config) = colors(FootprintColor::DeltaOpacity, row(4.0, 4.0, 1.0));
        assert_eq!(color, config.neutral_color);
        let (color, config) = colors(FootprintColor::Fixed, row(2.0, 8.0, 0.0));
        assert_eq!(color, config.neutral_color);
        assert_eq!(font_limits([4.0, 100.0]), [8.0, 24.0]);
        assert_eq!(font_limits([18.0, 12.0]), [12.0, 18.0]);
        assert_eq!(font_limits([f32::NAN, 12.0]), [10.0, 12.0]);
    }

    /// Guards the pullback rule against hand-worked bars: levels 100, 100.25 and 100.5 are keys
    /// 400 to 402. A bar that closed up selects the close row to the high, one that closed down
    /// the low to the close row, one that closed on its extreme row none, and a close outside the
    /// executed rows is held to them.
    #[test]
    fn the_pullback_range_runs_from_the_close_to_the_extreme_it_left() {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 0.25, 1).unwrap();
        for price in [100.0, 100.25, 100.5] {
            flow.push(&Tick::new(time, price, 1.0, Some(OrderSide::Buy)))
                .unwrap();
        }
        for (open, close, expected) in [
            (100.0, 100.25, Some((401, 402))),
            (100.5, 100.25, Some((400, 401))),
            (100.0, 100.5, None),
            (100.5, 100.0, None),
            (100.0, 99.0, None),
            (100.0, 101.0, None),
        ] {
            let bar = Bar::new(time, open, 100.5, 100.0, close, 3.0);
            assert_eq!(pullback_range(&bar, &flow), expected, "{open} to {close}");
        }
    }

    /// Guards that the painted study annotations follow their public state: an unvisited zone
    /// keeps its colour, a revisited or expired one is dimmed, and a buy marker is a triangle
    /// above the bar's top row.
    #[cfg(feature = "studies")]
    #[test]
    fn studies_are_painted_as_their_state_and_kind_say() {
        use crate::studies::{FootprintStudy, FootprintStudyKind, FootprintStudyState};
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 0.25, 1).unwrap();
        flow.push(&Tick::new(time, 100.0, 1.0, Some(OrderSide::Buy)))
            .unwrap();
        let bars = [Bar::new(time, 100.0, 100.25, 100.0, 100.25, 1.0)];
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(240.0, 160.0));
        let map = ChartMapping::new(
            rect,
            60.0,
            0,
            0,
            0.0,
            PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(99.0, 102.0),
                0.0,
                160.0,
                false,
            ),
        );
        let study = |kind, state, low| FootprintStudy {
            start: time,
            end: Some(time),
            low,
            high: low,
            kind,
            state,
        };
        let studies = [
            study(
                FootprintStudyKind::StackedAsk,
                FootprintStudyState::Unvisited,
                400,
            ),
            study(
                FootprintStudyKind::StackedAsk,
                FootprintStudyState::Revisited,
                401,
            ),
            study(
                FootprintStudyKind::StackedAsk,
                FootprintStudyState::Expired,
                402,
            ),
            study(
                FootprintStudyKind::DeltaAsk,
                FootprintStudyState::Local,
                400,
            ),
        ];
        let config = FootprintConfig::default();
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ui| render_footprint_studies(ui.painter(), &map, &bars, &flow, &studies, &config),
        );
        output.textures_delta.clear();
        let strokes: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                Shape::Rect(rect) if rect.stroke.width > 0.0 => Some(rect.stroke.color),
                _ => None,
            })
            .collect();
        let dimmed = config.ask_color.gamma_multiply(0.45);
        assert_eq!(strokes, [config.ask_color, dimmed, dimmed]);
        let top = map.price_to_y(100.125);
        let triangle = output.shapes.iter().find_map(|shape| match &shape.shape {
            Shape::Path(path) if path.fill == config.ask_color => Some(path.points.clone()),
            _ => None,
        });
        assert!(
            triangle.is_some_and(|points| points.len() == 3 && points.iter().all(|p| p.y < top)),
            "buy marker above the bar"
        );
    }
}
