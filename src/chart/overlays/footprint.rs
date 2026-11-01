//! Footprint cells painted through the chart's existing price and time mapping.

use crate::chart::coords::ChartMapping;
use crate::data::{Footprint, FootprintBar, FootprintLevel, NodeSensitivity};
use crate::model::Bar;
use egui::epaint::{Galley, Mesh, Shape};
use egui::{Align2, Color32, FontId, Painter, Rect, Stroke, StrokeKind, pos2, vec2};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::Arc;

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
    /// Hide numbers when a row is shorter than this height. A column uses compact values and
    /// fonts down to 8 points before switching to a profile overview without numbers.
    pub min_text_height: f32,
    /// Dominance multiple, normally 3 (300%).
    pub imbalance_ratio: f64,
    /// Compare to the opposite side one price away instead of at the same price.
    pub diagonal: bool,
    /// Outline the highest-volume row when its cells are large enough for the stroke.
    pub show_poc: bool,
    /// Shade the contiguous value area containing this share of the bar volume.
    pub value_area: Option<f64>,
    /// Trade profiles: the guide along the top edge of the value area (VAH); `None` hides it.
    pub value_area_high: Option<Stroke>,
    /// Trade profiles: the guide along the bottom edge of the value area (VAL); `None` hides it.
    pub value_area_low: Option<Stroke>,
    /// Trade profiles: how volume nodes are chosen.
    pub node_sensitivity: NodeSensitivity,
    /// Prefer 1.2K for 1,200 and 3.4M for 3,400,000, even when every digit would fit.
    /// Otherwise compact units are only a width-fitting fallback.
    pub abbreviate: bool,
    /// Write zero volumes as 0. Otherwise they are left blank, so a one-sided row shows only
    /// the side that traded.
    pub show_zero: bool,
    /// Preferred smallest and largest font size of the numbers, in points. A row's size follows
    /// its volume relative to the bar's largest row; equal values request one size. Both are held
    /// within 8 and 24 points. Fitting to the cell can reduce the size to 8 points; columns that
    /// cannot fit readable numbers use an overview without them.
    pub font_size: [f32; 2],
    /// Write the ask/bid ratio of the highest and lowest row of each bar above and below it,
    /// or `n/a` when the bid is zero and the ratio is undefined.
    pub extreme_ratio: bool,
    /// Outline the rows of the bar's pullback range, see [`pullback_range`].
    pub pullback: bool,
    /// Reserve a per-bar statistics table below the chart's price pane.
    /// Used by [`crate::widget::Chart::show_footprint`].
    pub statistics: bool,
    /// Annotations calculated from the displayed rows after executions or grouping change.
    /// `None` hides them. Used by [`crate::widget::Chart::show_footprint`].
    #[cfg(feature = "studies")]
    pub studies: Option<crate::studies::FootprintStudyConfig>,
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
            statistics: false,
            #[cfg(feature = "studies")]
            studies: None,
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
/// before looking up executions. A price spine occupies a separate gutter to the left of each
/// bar: the opening tick points left, the closing tick right, and the colour follows price direction.
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
    // Use the existing half-gap so price markers do not shrink execution columns.
    let gutter = (mapping.bar_spacing * 0.04).min(2.5);
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
        let left = x - mapping.bar_spacing * 0.46;
        let price_left = (left - gutter).max(mapping.rect.left());
        let price_right = left.max(price_left);
        let spine = (price_left + price_right) * 0.5;
        let color = if bar.close > bar.open {
            config.ask_color
        } else if bar.close < bar.open {
            config.bid_color
        } else {
            config.text_color
        };
        let stroke = Stroke::new((gutter * 0.2).clamp(0.25, 1.0), color);
        let [high, low, open, close] =
            [bar.high, bar.low, bar.open, bar.close].map(|price| mapping.price_to_y(price));
        if price_right > price_left && [high, low, open, close].iter().all(|y| y.is_finite()) {
            painter.line_segment([pos2(spine, high), pos2(spine, low)], stroke);
            painter.line_segment(
                [pos2(spine, open), pos2(spine, close)],
                Stroke::new(stroke.width * 2.0, color),
            );
            painter.line_segment([pos2(price_left, open), pos2(spine, open)], stroke);
            painter.line_segment([pos2(spine, close), pos2(price_right, close)], stroke);
        }
        // Automatic grouping uses the local price mapping, including log and inverted scales.
        let group = row_group(mapping, bar, flow, config);
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
        let text_rows: Vec<_> = shown
            .levels
            .values()
            .filter(|row| {
                let price =
                    row.level as f64 * flow.price_step() * group as f64 - flow.tick_size() * 0.5;
                let y1 = mapping.price_to_y(price);
                let y2 = mapping.price_to_y(price + flow.price_step() * group as f64);
                let center = (y1 + y2) * 0.5;
                y1.is_finite()
                    && y2.is_finite()
                    && center - FONT_LIMITS[0] * 0.5 >= mapping.rect.top()
                    && center + FONT_LIMITS[0] * 0.5 <= mapping.rect.bottom()
                    && (y2 - y1).abs() >= config.min_text_height.max(10.0)
                    && Rect::from_min_max(
                        pos2(left, y1.min(y2)),
                        pos2(x + mapping.bar_spacing * 0.46, y1.max(y2)),
                    )
                    .intersects(mapping.rect)
            })
            .collect();
        let text: Vec<_> = config
            .modes
            .iter()
            .take(columns)
            .map(|mode| {
                column_galleys(&painter, &text_rows, max_volume, *mode, config, width - 2.5)
            })
            .collect();
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
                if poc == Some(row.level) && rect.width() >= 12.0 && rect.height() >= 4.0 {
                    painter.rect_stroke(
                        rect,
                        0,
                        Stroke::new(1.0, config.text_color),
                        StrokeKind::Inside,
                    );
                }
                if rect.height() >= config.min_text_height.max(10.0)
                    && let Some(galley) = text[column]
                        .as_ref()
                        .and_then(|galleys| galleys.get(&row.level))
                {
                    let mut galley = galley.clone();
                    if !galley.is_empty() && galley.size().y > rect.height() {
                        let size = galley.job.sections[0].format.font_id.size;
                        for size in [
                            (size * rect.height() / galley.size().y).max(FONT_LIMITS[0]),
                            FONT_LIMITS[0],
                        ] {
                            galley = painter.layout_no_wrap(
                                galley.text().to_owned(),
                                FontId::monospace(size),
                                config.text_color,
                            );
                            if galley.size().y <= rect.height() {
                                break;
                            }
                        }
                    }
                    if !galley.is_empty() && galley.size().y <= rect.height() {
                        let center = rect.center().x;
                        let x = match mode {
                            FootprintMode::DominantSide if row.bid > row.ask => rect.left() + 1.0,
                            FootprintMode::DominantSide if row.ask > row.bid => {
                                rect.right() - 1.0 - galley.size().x
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

/// Fit the whole column before painting, so its largest executions never disappear alone.
fn column_galleys(
    painter: &Painter,
    rows: &[&FootprintLevel],
    max_volume: f64,
    mode: FootprintMode,
    config: &FootprintConfig,
    width: f32,
) -> Option<BTreeMap<i64, Arc<Galley>>> {
    if rows.is_empty() || width < 18.0 {
        return None;
    }
    let [smallest, largest] = font_limits(config.font_size);
    for compact in [false, true] {
        let mut fits = true;
        let galleys = rows
            .iter()
            .map(|row| {
                let mut text = cell_text(mode, row, config.abbreviate || compact, config.show_zero);
                if compact {
                    text = text.replace(" × ", "×").replace(" / ", "/");
                }
                let share = if max_volume > 0.0 {
                    (row.volume() / max_volume) as f32
                } else {
                    0.0
                };
                let size = smallest + (largest - smallest) * share;
                let mut galley = painter.layout_no_wrap(
                    text.clone(),
                    FontId::monospace(size),
                    config.text_color,
                );
                if galley.size().x > width {
                    let size = (size * width / galley.size().x).max(FONT_LIMITS[0]);
                    galley =
                        painter.layout_no_wrap(text, FontId::monospace(size), config.text_color);
                }
                if galley.size().x > width && size > FONT_LIMITS[0] {
                    galley = painter.layout_no_wrap(
                        galley.text().to_owned(),
                        FontId::monospace(FONT_LIMITS[0]),
                        config.text_color,
                    );
                }
                fits &= galley.size().x <= width;
                (row.level, galley)
            })
            .collect();
        if fits {
            return Some(galleys);
        }
    }
    None
}

/// The text of one cell in `mode`; empty when every number in it is a hidden zero.
fn cell_text(
    mode: FootprintMode,
    row: &FootprintLevel,
    abbreviate: bool,
    show_zero: bool,
) -> String {
    let hidden = |value: f64| value == 0.0 && !show_zero;
    let number = |value: f64| {
        if hidden(value) {
            String::new()
        } else {
            volume_text(value, abbreviate)
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

fn row_group(mapping: &ChartMapping, bar: &Bar, flow: &Footprint, config: &FootprintConfig) -> i64 {
    let raw_height =
        (mapping.price_to_y(bar.close + flow.price_step()) - mapping.price_to_y(bar.close)).abs();
    config
        .auto_row_height
        .filter(|h| h.is_finite() && *h > 0.0)
        .map_or(1, |height| {
            (height / raw_height.max(0.01))
                .ceil()
                .clamp(1.0, 1_000_000.0) as i64
        })
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
/// delta percentage, and ask/bid ratio (undefined for zero volume or bid). Bars before
/// [`Footprint::complete_from`] get no column and no share of the cumulative delta.
/// Shared labels occupy the left margin. Numeric columns stay aligned with their bars and
/// use unabbreviated volume values when they fit, then compact units. Percentages have one
/// decimal and ask/bid ratios two decimals.
/// A column cut by the margin or plot edge is omitted as a whole. When the complete table
/// cannot fit at a readable size, a zoom-in notice replaces it; values remain in [`FootprintBar`].
/// A view without complete executions reports that statistics are unavailable.
pub fn render_footprint_statistics(
    painter: &Painter,
    mapping: &ChartMapping,
    bars: &[Bar],
    flow: &Footprint,
    text_color: Color32,
) {
    let painter = painter.with_clip_rect(mapping.rect.intersect(painter.clip_rect()));
    let labels = [
        "Vol", "Bid", "Ask", "Delta", "Min", "Max", "CVD", "Delta %", "Ask/Bid",
    ];
    let row_height = (mapping.rect.height() / labels.len() as f32).min(12.0);
    let font = FontId::monospace(10.0);
    let labels: Vec<_> = labels
        .into_iter()
        .map(|label| painter.layout_no_wrap(label.into(), font.clone(), text_color))
        .collect();
    let label_width = labels
        .iter()
        .map(|label| label.size().x)
        .fold(0.0, f32::max)
        + 8.0;
    let mut cumulative = 0.0;
    let complete = flow.complete_from();
    let mut has_data = false;
    let mut columns = Vec::new();
    for (i, bar) in bars.iter().enumerate() {
        let Some(row) = flow
            .bar_at(bar.time)
            .filter(|_| complete.is_some_and(|from| bar.time >= from))
        else {
            continue;
        };
        cumulative += row.delta();
        let x = mapping.idx_to_x(i);
        if !mapping.is_x_visible(x) {
            continue;
        }
        has_data = true;
        if x - mapping.bar_spacing * 0.5 < mapping.rect.left() + label_width
            || x + mapping.bar_spacing * 0.5 > mapping.rect.right()
        {
            continue;
        }
        let values = [
            Some(row.volume()),
            Some(row.bid),
            Some(row.ask),
            Some(row.delta()),
            Some(row.min_delta),
            Some(row.max_delta),
            Some(cumulative),
            row.delta_percent(),
            (row.bid > 0.0).then(|| row.ask / row.bid),
        ];
        columns.push((x, values));
    }
    let mut size = 10.0;
    let mut fitted = Vec::new();
    if mapping.bar_spacing >= 22.0 && row_height >= 10.0 {
        'fitting: for abbreviate in [false, true] {
            for font_size in [10.0, FONT_LIMITS[0]] {
                size = font_size;
                fitted = columns
                    .iter()
                    .map(|(x, values)| {
                        (
                            *x,
                            values
                                .iter()
                                .copied()
                                .enumerate()
                                .map(|(line, value)| {
                                    let text = value.map_or_else(
                                        || "n/a".to_owned(),
                                        |value| match line {
                                            7 => format!("{value:.1}%"),
                                            8 => format!("{value:.2}:1"),
                                            _ => volume_text(value, abbreviate),
                                        },
                                    );
                                    painter.layout_no_wrap(
                                        text,
                                        FontId::monospace(size),
                                        text_color,
                                    )
                                })
                                .collect::<Vec<_>>(),
                        )
                    })
                    .collect::<Vec<_>>();
                if fitted.iter().all(|(_, values)| {
                    values
                        .iter()
                        .all(|value| value.size().x <= mapping.bar_spacing - 4.0)
                }) {
                    break 'fitting;
                }
            }
        }
    }
    if fitted.is_empty()
        || fitted.iter().any(|(_, values)| {
            values
                .iter()
                .any(|value| value.size().x > mapping.bar_spacing - 4.0)
        })
    {
        painter.text(
            mapping.rect.center(),
            Align2::CENTER_CENTER,
            if has_data {
                "Zoom in for per-bar statistics"
            } else {
                "Execution statistics unavailable"
            },
            font,
            text_color,
        );
        return;
    }
    for (line, label) in labels.into_iter().enumerate() {
        painter.galley(
            pos2(
                mapping.rect.left() + 2.0,
                mapping.rect.top() + row_height * line as f32,
            ),
            label,
            text_color,
        );
    }
    for (x, values) in fitted {
        for (line, value) in values.into_iter().enumerate() {
            let at = pos2(
                x - value.size().x * 0.5,
                mapping.rect.top() + row_height * line as f32 + (10.0 - size) * 0.5,
            );
            painter.galley(at, value, text_color);
        }
    }
}

#[cfg(feature = "studies")]
#[derive(Clone, Default)]
struct FootprintStudyCache {
    source: Vec<(FootprintBar, i64)>,
    config: Option<crate::studies::FootprintStudyConfig>,
    studies: std::sync::Arc<[crate::studies::FootprintStudy]>,
}

/// Cache calculations per chart, including display grouping and execution corrections.
#[cfg(feature = "studies")]
pub(crate) fn render_footprint_annotations(
    painter: &Painter,
    mapping: &ChartMapping,
    bars: &[Bar],
    flow: &Footprint,
    config: &FootprintConfig,
    chart_id: egui::Id,
) {
    let Some(settings) = &config.studies else {
        return;
    };
    let complete = flow.complete_from();
    let source: Vec<_> = bars
        .iter()
        .filter(|bar| complete.is_some_and(|from| bar.time >= from))
        .filter_map(|bar| {
            flow.bar_at(bar.time)
                .map(|row| (row, row_group(mapping, bar, flow, config)))
        })
        .collect();
    let studies = painter.ctx().data_mut(|data| {
        let cache = data
            .get_temp_mut_or_default::<FootprintStudyCache>(chart_id.with("footprint_annotations"));
        if cache.config.as_ref() != Some(settings)
            || cache.source.len() != source.len()
            || !cache
                .source
                .iter()
                .zip(&source)
                .all(|((old, group), (new, next_group))| old == *new && group == next_group)
        {
            cache.source = source
                .iter()
                .map(|(bar, group)| ((*bar).clone(), *group))
                .collect();
            let grouped: Vec<_> = source
                .iter()
                .map(|(bar, group)| (group_bar(bar, *group), *group))
                .collect();
            let rows: Vec<_> = grouped.iter().map(|(bar, group)| (bar, *group)).collect();
            cache.studies = crate::studies::footprint_studies_on_rows(&rows, settings).into();
            cache.config = Some(settings.clone());
        }
        cache.studies.clone()
    });
    render_footprint_studies(painter, mapping, bars, flow, &studies, config);
}

/// Paints footprint study levels and zones. Compute `studies` only after executions change.
/// Stacks, naked POCs and unfinished levels extend to their first revisit, their expiry or the
/// last displayed bar, in full colour while unvisited and dimmed once revisited or expired;
/// local annotations stay anchored to their originating bar. Delta percentage and ratio markers
/// are triangles outside the bar: buy markers above it, sell markers below, the delta marker
/// nearest and the ratio marker beyond it. Captions avoid every footprint and its marker rows; details are
/// available on hover when a caption has no room. Overview marks shrink with bar spacing and
/// zones become faint centre lines. Prices use the aggregation's row step.
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
    let occupied: BTreeMap<_, _> = bars
        .iter()
        .enumerate()
        .filter_map(|(index, bar)| {
            let source = flow.bar_at(bar.time)?;
            let group = row_group(mapping, bar, flow, config);
            let low = source.levels.first_key_value()?.0.div_euclid(group);
            let high = source.levels.last_key_value()?.0.div_euclid(group);
            let step = flow.price_step() * group as f64;
            let y1 = mapping.price_to_y((low as f64 * step - flow.tick_size() * 0.5).min(bar.low));
            let y2 = mapping
                .price_to_y(((high as f64 + 1.0) * step - flow.tick_size() * 0.5).max(bar.high));
            let x = mapping.idx_to_x(index);
            let rect = Rect::from_min_max(
                pos2(x - mapping.bar_spacing * 0.46, y1.min(y2)),
                pos2(x + mapping.bar_spacing * 0.46, y1.max(y2)),
            );
            // The two threshold-marker rows extend up to 18 points beyond either extreme.
            rect.intersects(mapping.rect)
                .then_some((index, rect.expand2(vec2(0.0, 18.0))))
        })
        .collect();
    let mut labels: Vec<Rect> = Vec::new();
    let mut hovered = Vec::new();
    let pointer = painter
        .ctx()
        .pointer_hover_pos()
        .filter(|p| mapping.rect.contains(*p) && painter.clip_rect().contains(*p));
    let detail = mapping.bar_spacing >= 48.0;
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
        let label = match study.kind {
            FootprintStudyKind::StackedBid => "Bid stack",
            FootprintStudyKind::StackedAsk => "Ask stack",
            FootprintStudyKind::UnfinishedHigh => "UH",
            FootprintStudyKind::UnfinishedLow => "UL",
            FootprintStudyKind::NakedPoc => "POC",
            FootprintStudyKind::DeltaPoc => "Delta POC",
            FootprintStudyKind::VolumeCluster => "Cluster",
            FootprintStudyKind::SinglePrint => "Single",
            FootprintStudyKind::DeltaAsk => "Buy delta %",
            FootprintStudyKind::DeltaBid => "Sell delta %",
            FootprintStudyKind::RatioAsk => "Buy ask/bid ratio",
            FootprintStudyKind::RatioBid => "Sell bid/ask ratio",
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
            let radius = (mapping.bar_spacing * 0.2).min(4.0);
            let tip = edge + away * (radius * 0.75 + radius * 2.25 * order);
            let base = tip + away * radius * 1.5;
            let x = (rect.left() + rect.right()) / 2.0;
            painter.add(Shape::convex_polygon(
                vec![pos2(x, tip), pos2(x - radius, base), pos2(x + radius, base)],
                if detail {
                    color
                } else {
                    color.gamma_multiply(0.5)
                },
                Stroke::NONE,
            ));
            let bounds = Rect::from_min_max(
                pos2(x - radius, tip.min(base)),
                pos2(x + radius, tip.max(base)),
            );
            if pointer.is_some_and(|p| bounds.expand(2.0).contains(p)) {
                let source = flow.bar_at(bars[start].time);
                let value = match study.kind {
                    FootprintStudyKind::DeltaAsk | FootprintStudyKind::DeltaBid => source
                        .and_then(FootprintBar::delta_percent)
                        .map(|value| format!("{value:.1}%")),
                    _ => source.and_then(|bar| {
                        let (side, opposite) = if above {
                            (bar.ask, bar.bid)
                        } else {
                            (bar.bid, bar.ask)
                        };
                        (opposite > 0.0).then(|| format!("{:.2}:1", side / opposite))
                    }),
                };
                hovered.push(format!("{label}: {}", value.as_deref().unwrap_or("n/a")));
            }
            continue;
        }
        if pointer.is_some_and(|p| rect.expand(2.0).contains(p)) {
            let state = match study.state {
                FootprintStudyState::Unvisited => "unvisited",
                FootprintStudyState::Revisited => "revisited",
                FootprintStudyState::Expired => "expired",
                FootprintStudyState::Local => "this bar",
            };
            hovered.push(format!(
                "{label}: {} to {} ({state})",
                study.low as f64 * flow.price_step(),
                study.high as f64 * flow.price_step(),
            ));
        }
        if !detail {
            painter.line_segment(
                [rect.left_center(), rect.right_center()],
                Stroke::new(0.5, color.gamma_multiply(0.3)),
            );
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
        {
            let (at, anchor) = if matches!(
                study.kind,
                FootprintStudyKind::StackedAsk
                    | FootprintStudyKind::StackedBid
                    | FootprintStudyKind::NakedPoc
                    | FootprintStudyKind::UnfinishedHigh
                    | FootprintStudyKind::UnfinishedLow
            ) {
                (rect.right_top() - vec2(0.0, 3.0), Align2::RIGHT_BOTTOM)
            } else {
                (rect.left_center() - vec2(3.0, 0.0), Align2::RIGHT_CENTER)
            };
            let galley = painter.layout_no_wrap(label.into(), FontId::monospace(10.0), color);
            let bounds = anchor.anchor_size(at, galley.size());
            let bar = occupied.get(&start).copied().unwrap_or(rect);
            let alternatives = [
                bounds,
                Align2::CENTER_BOTTOM
                    .anchor_size(pos2(bar.center().x, bar.top() - 3.0), galley.size()),
                Align2::CENTER_TOP
                    .anchor_size(pos2(bar.center().x, bar.bottom() + 3.0), galley.size()),
            ];
            if let Some(bounds) = alternatives.into_iter().find(|bounds| {
                mapping.rect.contains_rect(*bounds)
                    && !labels
                        .iter()
                        .any(|label| label.expand(1.0).intersects(*bounds))
                    && !occupied
                        .values()
                        .any(|cell| cell.expand(1.0).intersects(*bounds))
            }) {
                painter.galley(bounds.min, galley, color);
                labels.push(bounds);
            } else {
                let x = mapping.idx_to_x(start) + mapping.bar_spacing * 0.46 + 0.5;
                painter.circle_filled(pos2(x, rect.center().y), 0.75, color);
            }
        }
        painter.rect_stroke(rect, 0, Stroke::new(1.0, color), StrokeKind::Inside);
    }
    if let Some(pointer) = pointer.filter(|_| !hovered.is_empty()) {
        hovered.sort();
        hovered.dedup();
        egui::Tooltip::always_open(
            painter.ctx().clone(),
            painter.layer_id(),
            egui::Id::unique((
                "footprint_study_details",
                mapping.rect.min.x.to_bits(),
                mapping.rect.min.y.to_bits(),
            )),
            pointer,
        )
        .show(|ui| {
            for text in &hovered {
                ui.label(text);
            }
        });
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

    /// Shared row labels and complete numeric columns survive narrow detail widths. The first
    /// bar has 13 buys and 3 sells: delta 10, 62.5%, ask/bid 4.33:1; the second adds 2500 sells.
    #[test]
    fn statistics_keep_shared_rows_and_aligned_values_or_explain_the_overview() {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        let next = time + chrono::TimeDelta::minutes(1);
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 0.25, 1).unwrap();
        for (time, bid, ask) in [(time, 3.0, 13.0), (next, 2500.0, 0.0)] {
            for (size, side) in [(bid, OrderSide::Sell), (ask, OrderSide::Buy)] {
                if size > 0.0 {
                    flow.push(&Tick::new(time, 100.0, size, Some(side)))
                        .unwrap();
                }
            }
        }
        let bars = [
            Bar::new(time, 100.0, 100.0, 100.0, 100.0, 16.0),
            Bar::new(next, 100.0, 100.0, 100.0, 100.0, 2500.0),
        ];
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(240.0, 112.0));
        let empty = Footprint::new(BarClock::new(Timeframe::Min1), 0.25, 1).unwrap();
        for (spacing, available, large) in [
            (60.0, true, false),
            (40.0, true, false),
            (3.0, true, false),
            (60.0, false, false),
            (40.0, true, true),
        ] {
            let mut shown = flow.clone();
            let mut bars = bars.clone();
            if large {
                shown
                    .push(&Tick::new(
                        next,
                        100.0,
                        123_454_289.0,
                        Some(OrderSide::Sell),
                    ))
                    .unwrap();
                bars[1].volume = 123_456_789.0;
            }
            let map = ChartMapping::new(
                rect,
                spacing,
                0,
                1,
                0.0,
                PriceMap::new(
                    PriceTransform::default(),
                    PriceRange::new(99.0, 102.0),
                    0.0,
                    112.0,
                    false,
                ),
            );
            let mut output = egui::Context::default().run_ui(
                egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |ui| {
                    render_footprint_statistics(
                        ui.painter(),
                        &map,
                        &bars,
                        if available { &shown } else { &empty },
                        Color32::WHITE,
                    )
                },
            );
            output.textures_delta.clear();
            let texts: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    Shape::Text(t) => Some(t),
                    _ => None,
                })
                .collect();
            if spacing == 3.0 || !available {
                assert_eq!(texts.len(), 1);
                assert_eq!(
                    texts[0].galley.text(),
                    if available {
                        "Zoom in for per-bar statistics"
                    } else {
                        "Execution statistics unavailable"
                    }
                );
                continue;
            }
            assert_eq!(
                texts.len(),
                27,
                "nine labels and two complete columns at {spacing}"
            );
            assert_eq!(
                texts
                    .iter()
                    .filter(|t| t.galley.text() == "Delta %")
                    .count(),
                1
            );
            for (column, expected) in [
                ["16", "3", "13", "10", "-3", "10", "10", "62.5%", "4.33:1"],
                if large {
                    [
                        "123.5M", "123.5M", "0", "-123.5M", "-123.5M", "0", "-123.5M", "-100.0%",
                        "0.00:1",
                    ]
                } else {
                    [
                        "2500", "2500", "0", "-2500", "-2500", "0", "-2490", "-100.0%", "0.00:1",
                    ]
                },
            ]
            .into_iter()
            .enumerate()
            {
                for (line, expected) in expected.into_iter().enumerate() {
                    let text = texts[9 + column * 9 + line];
                    assert_eq!(text.galley.text(), expected);
                    assert!(
                        (text.pos.x + text.galley.size().x * 0.5 - map.idx_to_x(column)).abs()
                            < 0.01
                    );
                }
            }
        }
    }

    /// OHLC ticks use exact prices even inside grouped rows and on inverted mappings.
    /// Identical execution delta must not hide opposite price directions.
    #[test]
    fn price_spines_identify_open_close_and_direction_without_covering_values() {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 0.25, 1).unwrap();
        for price in [100.0, 100.25, 100.5, 100.75, 101.0] {
            flow.push(&Tick::new(time, price, 3.0, Some(OrderSide::Buy)))
                .unwrap();
        }
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(240.0, 160.0));
        for spacing in [60.0, 3.0] {
            for inverted in [false, true] {
                for (open, close) in [(100.25, 100.75), (100.75, 100.25), (100.5, 100.5)] {
                    let map = ChartMapping::new(
                        rect,
                        spacing,
                        0,
                        0,
                        0.0,
                        PriceMap::new(
                            PriceTransform::default(),
                            PriceRange::new(99.0, 103.0),
                            0.0,
                            160.0,
                            inverted,
                        ),
                    );
                    let bars = [Bar::new(time, open, 101.0, 100.0, close, 15.0)];
                    let config = FootprintConfig::default();
                    let mut output = egui::Context::default().run_ui(
                        egui::RawInput {
                            screen_rect: Some(rect),
                            ..Default::default()
                        },
                        |ui| render_footprint(ui.painter(), &map, &bars, &flow, &config),
                    );
                    output.textures_delta.clear();
                    let lines: Vec<_> = output
                        .shapes
                        .iter()
                        .filter_map(|s| match &s.shape {
                            Shape::LineSegment { points, stroke } => Some((points, stroke)),
                            _ => None,
                        })
                        .collect();
                    assert_eq!(
                        lines.len(),
                        4,
                        "price spine and body with opening and closing ticks"
                    );
                    // y(P) = (103 - P) * 40; inversion gives 160 - y(P).
                    let y = |price: f64| {
                        let y = (103.0 - price) as f32 * 40.0;
                        if inverted { 160.0 - y } else { y }
                    };
                    assert_eq!(lines[0].0.map(|p| p.y), [y(101.0), y(100.0)]);
                    assert_eq!(lines[2].0.map(|p| p.y), [y(open); 2]);
                    assert_eq!(lines[3].0.map(|p| p.y), [y(close); 2]);
                    assert!(lines[2].0[0].x < lines[2].0[1].x);
                    assert!(lines[3].0[0].x < lines[3].0[1].x);
                    let color = if close > open {
                        config.ask_color
                    } else if close < open {
                        config.bid_color
                    } else {
                        config.text_color
                    };
                    assert!(lines.iter().all(|(_, stroke)| stroke.color == color));
                    for shape in &output.shapes {
                        if let Shape::Text(text) = &shape.shape {
                            assert!(text.pos.x > lines[3].0[1].x);
                        }
                    }
                }
            }
        }
    }

    /// A column keeps its largest values at detail zoom and switches all its numbers off
    /// together when even compact text is too wide. POC strokes must not dominate the overview.
    #[test]
    fn detail_columns_keep_large_values_and_overview_preserves_fills() {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 0.25, 1).unwrap();
        for (price, bid, ask) in [
            (100.0, 1234.0, 3456.0),
            (100.25, 1.0, 2.0),
            (100.5, 3456.0, 1234.0),
        ] {
            for (size, side) in [(bid, OrderSide::Sell), (ask, OrderSide::Buy)] {
                flow.push(&Tick::new(time, price, size, Some(side)))
                    .unwrap();
            }
        }
        let bars = [Bar::new(time, 100.0, 100.5, 100.0, 100.5, 9383.0)];
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(240.0, 160.0));
        for mode in [FootprintMode::BidAsk, FootprintMode::DeltaVolume] {
            for (spacing, numbers, outlines, clipped) in [
                (60.0, 3, 1, false),
                (57.0, 3, 1, false),
                (22.0, 0, 1, false),
                (3.0, 0, 0, false),
                (60.0, 3, 1, true),
            ] {
                let mut visible = flow.clone();
                let mut bars = bars.clone();
                if clipped {
                    // A print cut by the top edge must not suppress the fully visible numbers.
                    visible
                        .push(&Tick::new(time, 102.0, 1e20, Some(OrderSide::Buy)))
                        .unwrap();
                    bars[0].high = 102.0;
                    bars[0].close = 102.0;
                    bars[0].volume += 1e20;
                }
                let map = ChartMapping::new(
                    rect,
                    spacing,
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
                let config = FootprintConfig {
                    modes: vec![mode],
                    auto_row_height: None,
                    ..Default::default()
                };
                let mut output = egui::Context::default().run_ui(
                    egui::RawInput {
                        screen_rect: Some(rect),
                        ..Default::default()
                    },
                    |ui| render_footprint(ui.painter(), &map, &bars, &visible, &config),
                );
                output.textures_delta.clear();
                let texts: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        Shape::Text(t) => Some(t),
                        _ => None,
                    })
                    .collect();
                assert_eq!(texts.len(), numbers, "{mode:?}, spacing {spacing}");
                for text in texts {
                    assert!(
                        text.pos.x >= map.idx_to_x(0) - spacing * 0.46
                            && text.pos.x + text.galley.size().x
                                <= map.idx_to_x(0) + spacing * 0.46,
                        "execution values stay inside their column"
                    );
                    assert!(
                        text.galley
                            .job
                            .sections
                            .iter()
                            .all(|section| section.format.font_id.size >= 8.0)
                    );
                }
                assert_eq!(output.shapes.iter().filter(|s| matches!(&s.shape, Shape::Rect(r) if r.stroke.color == config.text_color)).count(), outlines, "POC at {spacing}");
                assert!(
                    output
                        .shapes
                        .iter()
                        .any(|s| matches!(&s.shape, Shape::Mesh(m) if !m.is_empty()))
                );
            }
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
            assert_eq!(
                cell_text(mode, &row, config.abbreviate, config.show_zero),
                expected,
                "{mode:?} {row:?}"
            );
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
        // A local label anchored beside the second bar used to cover the first bar's values.
        let next = time + chrono::TimeDelta::minutes(1);
        flow.push(&Tick::new(next, 100.0, 2.0, Some(OrderSide::Buy)))
            .unwrap();
        let bars = [
            bars[0].clone(),
            Bar::new(next, 100.0, 100.0, 100.0, 100.0, 2.0),
        ];
        let local = FootprintStudy {
            start: next,
            end: Some(next),
            low: 400,
            high: 400,
            kind: FootprintStudyKind::SinglePrint,
            state: FootprintStudyState::Local,
        };
        for spacing in [60.0, 3.0] {
            let map = ChartMapping {
                bar_spacing: spacing,
                base_idx: 1,
                ..map
            };
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |ui| {
                    render_footprint(ui.painter(), &map, &bars, &flow, &config);
                    render_footprint_studies(
                        ui.painter(),
                        &map,
                        &bars,
                        &flow,
                        &[
                            local.clone(),
                            FootprintStudy {
                                kind: FootprintStudyKind::VolumeCluster,
                                ..local.clone()
                            },
                            FootprintStudy {
                                kind: FootprintStudyKind::DeltaPoc,
                                ..local.clone()
                            },
                            FootprintStudy {
                                kind: FootprintStudyKind::DeltaAsk,
                                high: 401,
                                ..local.clone()
                            },
                            FootprintStudy {
                                kind: FootprintStudyKind::RatioAsk,
                                high: 401,
                                ..local.clone()
                            },
                            studies[3].clone(),
                        ],
                        &config,
                    );
                },
            );
            output.textures_delta.clear();
            let texts: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    Shape::Text(t) => Some(t),
                    _ => None,
                })
                .collect();
            for (index, caption) in texts
                .iter()
                .enumerate()
                .filter(|(_, t)| ["Single", "Cluster", "Delta POC"].contains(&t.galley.text()))
            {
                let bounds = Rect::from_min_size(caption.pos, caption.galley.size());
                assert!(
                    texts.iter().enumerate().all(|(other, t)| other == index
                        || !bounds.intersects(Rect::from_min_size(t.pos, t.galley.size()))),
                    "caption over execution values"
                );
                for shape in &output.shapes {
                    if let Shape::Path(path) = &shape.shape {
                        assert!(
                            !bounds.intersects(Rect::from_points(&path.points)),
                            "caption over threshold marker"
                        );
                    }
                }
            }
            if spacing == 60.0 {
                let marker = output
                    .shapes
                    .iter()
                    .find_map(|s| match &s.shape {
                        Shape::Circle(circle) => Some(circle),
                        _ => None,
                    })
                    .expect("a caption without room leaves a marker");
                assert!(
                    marker.center.x > map.idx_to_x(1) + spacing * 0.46,
                    "annotation marker uses the gap after the cells, away from the price spine"
                );
            }
            if spacing == 3.0 {
                assert!(
                    output
                        .shapes
                        .iter()
                        .all(|s| !matches!(&s.shape, Shape::Rect(r) if r.stroke.width > 0.0)),
                    "no historical boxes at overview zoom"
                );
                for shape in &output.shapes {
                    if let Shape::Path(path) = &shape.shape {
                        let (lo, hi) = path
                            .points
                            .iter()
                            .fold((f32::INFINITY, f32::NEG_INFINITY), |(lo, hi), p| {
                                (lo.min(p.x), hi.max(p.x))
                            });
                        assert!(
                            hi - lo <= spacing * 0.8,
                            "overview triangle stays within its bar"
                        );
                    }
                }
            }
        }
        let map = ChartMapping { base_idx: 1, ..map };
        let at = pos2(map.idx_to_x(1), map.price_to_y(100.0));
        let mut details = false;
        // Tooltips need a sizing frame before their text is painted.
        for _ in 0..3 {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(rect),
                    events: vec![egui::Event::PointerMoved(at)],
                    ..Default::default()
                },
                |ui| {
                    render_footprint_studies(
                        ui.painter(),
                        &map,
                        &bars,
                        &flow,
                        std::slice::from_ref(&local),
                        &config,
                    )
                },
            );
            output.textures_delta.clear();
            details |= output.shapes.iter().any(|s| matches!(&s.shape, Shape::Text(t) if t.galley.text().starts_with("Single: 100 to 100")));
        }
        assert!(details, "compact annotations retain hover details");
    }
}
