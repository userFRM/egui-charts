//! Footprint cells painted through the chart's existing price and time mapping.

use crate::chart::coords::ChartMapping;
use crate::data::{Footprint, FootprintBar, FootprintLevel};
use crate::model::Bar;
#[cfg(feature = "studies")]
use egui::Align2;
use egui::epaint::{Mesh, Shape};
use egui::{Color32, FontId, Painter, Rect, Stroke, StrokeKind, pos2, vec2};
use std::collections::BTreeMap;

/// Numbers shown in a footprint column.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FootprintMode {
    /// Sell volume on the left, buy volume on the right.
    BidAsk,
    /// All executed volume, including unknown sides.
    Volume,
    /// Buy minus sell volume.
    Delta,
    /// Delta followed by total volume.
    DeltaVolume,
}

/// What determines a footprint cell's colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FootprintColor {
    /// Volume relative to the largest row of this bar.
    Volume,
    /// Delta as a fraction of the row's classified volume.
    Delta,
    /// Dominant bid or ask side, using the configured comparison.
    Imbalance,
}

/// Footprint appearance. Supply colours from the host's theme.
#[derive(Clone, Debug)]
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
        }
    }
}

/// Draws visible footprint bars, bounded by the visible bar range and clipped to `mapping.rect`.
///
/// For the chart widget use [`crate::widget::Chart::show_footprint`] to keep cells below its
/// legend and cursor. Standalone hosts supply their pane mapping and source bars. Use only
/// time-based bars whose timestamps lie inside their aggregation slots. Text layout uses
/// egui's galley cache; rows below the minimum height
/// never request a layout. Fractional volumes below 100 retain up to three decimals;
/// nonzero magnitudes below 0.001 use scientific notation. Only display grouping runs during painting.
pub fn render_footprint(
    painter: &Painter,
    mapping: &ChartMapping,
    bars: &[Bar],
    flow: &Footprint,
    config: &FootprintConfig,
) {
    if bars.is_empty()
        || config.modes.is_empty()
        || !mapping.bar_spacing.is_finite()
        || mapping.bar_spacing <= 0.0
    {
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
    let width = (mapping.bar_spacing * 0.92 / columns as f32).max(0.5);
    for (index, bar) in bars.iter().enumerate().take(end).skip(first) {
        let Some(source) = flow.bar_at(bar.time) else {
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
        let area = config.value_area.and_then(|share| shown.value_area(share));
        let poc = config.show_poc.then(|| shown.poc()).flatten();
        for row in shown.levels.values() {
            let price = row.level as f64 * flow.price_step() * group as f64;
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
                let color = cell_color(shown, row, max_volume, config);
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
                if poc == Some(row.level) {
                    painter.rect_stroke(
                        rect,
                        0,
                        Stroke::new(1.0, config.text_color),
                        StrokeKind::Inside,
                    );
                }
                if rect.height() >= config.min_text_height.max(10.0) && width >= 22.0 {
                    let text = match mode {
                        FootprintMode::BidAsk => {
                            format!("{} × {}", volume_text(row.bid), volume_text(row.ask))
                        }
                        FootprintMode::Volume => volume_text(row.volume()),
                        FootprintMode::Delta => format!(
                            "{}{}",
                            if row.delta() >= 0.0 { "+" } else { "" },
                            volume_text(row.delta())
                        ),
                        FootprintMode::DeltaVolume => {
                            format!(
                                "{}{} / {}",
                                if row.delta() >= 0.0 { "+" } else { "" },
                                volume_text(row.delta()),
                                volume_text(row.volume())
                            )
                        }
                    };
                    let galley =
                        painter.layout_no_wrap(text, FontId::monospace(10.0), config.text_color);
                    let at = rect.center() - galley.size() / 2.0;
                    // Text is whole or absent: a cell cut by the plot edge shows no number.
                    if galley.size().x + 4.0 <= rect.width()
                        && mapping
                            .rect
                            .contains_rect(Rect::from_min_size(at, galley.size()))
                    {
                        painter.galley(at, galley, config.text_color);
                    }
                }
            }
        }
    }
    painter.set(background, Shape::mesh(mesh));
}

fn volume_text(value: f64) -> String {
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
    config: &FootprintConfig,
) -> Color32 {
    match config.color_by {
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
/// A column too narrow for text is left empty; values are still available in [`FootprintBar`].
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
                |value| format!("{label} {}", volume_text(value)),
            );
            let galley = painter.layout_no_wrap(text, FontId::monospace(10.0), text_color);
            if galley.size().x <= mapping.bar_spacing - 2.0 {
                painter.galley(
                    pos2(
                        x - galley.size().x / 2.0,
                        mapping.rect.top() + 12.0 * line as f32,
                    ),
                    galley,
                    text_color,
                );
            }
        }
    }
}

/// Paints footprint study levels and zones. Compute `studies` only after executions change.
/// Stacks and naked POCs extend to their first revisit or the last displayed bar; local
/// annotations remain inside their originating bar. Prices use the aggregation's row step.
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
    use crate::studies::FootprintStudyKind;
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
        let y1 = mapping.price_to_y(study.low as f64 * flow.price_step());
        let y2 = mapping.price_to_y((study.high as f64 + 1.0) * flow.price_step());
        if !y1.is_finite() || !y2.is_finite() {
            continue;
        }
        let rect = Rect::from_min_max(pos2(left, y1.min(y2)), pos2(right, y1.max(y2)));
        if !rect.intersects(mapping.rect) {
            continue;
        }
        let color = match study.kind {
            FootprintStudyKind::StackedBid => config.bid_color,
            FootprintStudyKind::StackedAsk => config.ask_color,
            _ => config.text_color,
        };
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
                FootprintStudyKind::UnfinishedAuction => "UA",
                FootprintStudyKind::NakedPoc => "POC",
                FootprintStudyKind::DeltaPoc => "Delta POC",
                FootprintStudyKind::VolumeCluster => "Cluster",
                FootprintStudyKind::SinglePrint => "Single",
            };
            let (at, anchor) = if matches!(
                study.kind,
                FootprintStudyKind::StackedAsk
                    | FootprintStudyKind::StackedBid
                    | FootprintStudyKind::NakedPoc
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
        for (minimum, auto, expected) in [(14.0, None, 0), (10.0, None, 8), (10.0, Some(30.0), 4)] {
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
                |ui| render_footprint(ui.painter(), &map, &bars, &flow, &config),
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
                "minimum {minimum}, auto {auto:?}: {texts:?}"
            );
            if auto.is_some() {
                assert_eq!(texts, ["2 × 7", "9", "+5", "+5 / 9"]);
            }
        }
        // A plot edge through the first column leaves its numbers out rather than cutting them.
        let cut = ChartMapping::new(
            Rect::from_min_max(pos2(30.0, 0.0), rect.max),
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
            min_text_height: 10.0,
            auto_row_height: None,
            ..Default::default()
        };
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ui| render_footprint(ui.painter(), &cut, &bars, &flow, &config),
        );
        output.textures_delta.clear();
        let texts = output
            .shapes
            .iter()
            .filter(|shape| matches!(shape.shape, Shape::Text(_)))
            .count();
        assert_eq!(texts, 6);
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
    }
}
