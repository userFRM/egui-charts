//! Footprint cells painted through the chart's existing price and time mapping.

use crate::chart::coords::ChartMapping;
use crate::chart::rendering::draw_dashed_line;
use crate::data::{Footprint, FootprintBar, FootprintLevel, NodeSensitivity};
use crate::model::{Bar, ChartTypeParams, FootprintStatistic};
use crate::theme::ThemeExt;
use egui::epaint::{Galley, Mesh, Shape};
use egui::{Align2, Color32, FontId, Painter, Rect, Stroke, StrokeKind, pos2, vec2};
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

/// The smallest and largest font size a footprint number is drawn at, in points.
const FONT_LIMITS: [f32; 2] = [8.0, 24.0];
/// The share of a ladder column each of its side cells takes.
const LADDER_CELL_SHARE: f32 = 0.39;
/// The height of a ladder bar's totals and box, three rows of the 10 point font and their margins.
const INFO_BOX_HEIGHT: f32 = 46.0;

pub use crate::model::{FootprintColor, FootprintMode};

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
    /// Buy colour of imbalance strips, imbalance colouring and stacked imbalance zones, or
    /// `None`, the default, for `ask_color`.
    pub imbalance_ask_color: Option<Color32>,
    /// Sell colour of imbalance strips, imbalance colouring and stacked imbalance zones, or
    /// `None`, the default, for `bid_color`.
    pub imbalance_bid_color: Option<Color32>,
    /// Text colour and POC outline, or POC fill under [`FootprintColor::VolumeSteps`], which
    /// picks black or white numbers for each cell, or each half of a split cell, instead. The
    /// default suits a dark chart; set the theme's text colour on a light one, or the fill, lines
    /// and labels vanish.
    pub text_color: Color32,
    /// Cell fill for balanced or unclassified executions; under [`FootprintColor::VolumeSteps`]
    /// its weakest shade fills a row without classified volume, apart from a POC of the same
    /// colour.
    pub neutral_color: Color32,
    /// The colour behind the cells, which the volume shades and the patches behind numbers are
    /// mixed with, or `None`, the default, for the plot background of the painter's active theme.
    pub background: Option<Color32>,
    /// Group adjacent rows automatically to at least this screen height. `None` keeps the
    /// aggregation's manual grouping. Grouping is an integer multiple of the input step.
    pub auto_row_height: Option<f32>,
    /// Minimum instrument ticks per displayed row, rounded up to a multiple of the input step.
    /// Zero keeps the input grouping. Automatic grouping can raise this minimum.
    pub row_ticks: u32,
    /// Hide numbers when a row is shorter than this height. A column uses compact values and
    /// fonts down to 8 points before switching to a profile overview without numbers.
    pub min_text_height: f32,
    /// Dominance multiple, normally 3 (300%).
    pub imbalance_ratio: f64,
    /// Inclusive minimum dominant-side volume for cell imbalances and stacked zones, in
    /// instrument volume units after row grouping. Overrides the study config's minimum when
    /// drawn by the chart. Zero preserves all ratio signals; negative or non-finite, none.
    pub imbalance_min_volume: f64,
    /// Compare to the opposite side one price away instead of at the same price.
    pub diagonal: bool,
    /// Put a strip along the outer edge of a side that dominates under the imbalance settings,
    /// under [`FootprintColor::VolumeSteps`]: in the side's imbalance colour, or its strongest
    /// shade next to a cell of that colour.
    pub show_imbalance: bool,
    /// Outline the highest-volume row when its cells are large enough for the stroke, or fill it
    /// under [`FootprintColor::VolumeSteps`].
    pub show_poc: bool,
    /// Mark the contiguous value area containing this share of the bar volume: dotted lines
    /// along its edges under [`FootprintColor::VolumeSteps`], otherwise a faint fill.
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
    /// Write the sell and buy totals and a delta and total volume box below each bar of a
    /// [`FootprintMode::Ladder`] column, while its numbers fit.
    pub info_box: bool,
    /// Reserve a per-bar statistics table below the chart's price pane.
    /// Used by the footprint chart type and [`crate::widget::Chart::show_footprint`].
    pub statistics: bool,
    /// Statistics rows in display order. An empty list hides the table.
    pub statistics_rows: Vec<FootprintStatistic>,
    /// Annotations calculated from the displayed rows after executions or grouping change.
    /// `None` hides them. Used by the footprint chart type and [`crate::widget::Chart::show_footprint`].
    #[cfg(feature = "studies")]
    pub studies: Option<crate::studies::FootprintStudyConfig>,
}

impl Default for FootprintConfig {
    fn default() -> Self {
        Self {
            modes: vec![FootprintMode::Ladder],
            profile: false,
            color_by: FootprintColor::VolumeSteps,
            ask_color: crate::tokens::DESIGN_TOKENS.semantic.chart.bullish,
            bid_color: crate::tokens::DESIGN_TOKENS.semantic.chart.bearish,
            imbalance_ask_color: None,
            imbalance_bid_color: None,
            text_color: Color32::WHITE,
            neutral_color: Color32::GRAY,
            background: None,
            auto_row_height: Some(16.0),
            row_ticks: 0,
            min_text_height: 12.0,
            imbalance_ratio: 3.0,
            imbalance_min_volume: 0.0,
            diagonal: true,
            show_imbalance: true,
            show_poc: true,
            value_area: Some(0.7),
            value_area_high: Some(Stroke::new(1.0, Color32::GRAY)),
            value_area_low: Some(Stroke::new(1.0, Color32::GRAY)),
            node_sensitivity: NodeSensitivity::default(),
            abbreviate: true,
            show_zero: true,
            font_size: [10.0, 10.0],
            extreme_ratio: false,
            pullback: false,
            info_box: true,
            statistics: false,
            // Keep the existing volume statistics selected; execution metrics are opt-in.
            statistics_rows: FootprintStatistic::ALL[..9].to_vec(),
            #[cfg(feature = "studies")]
            studies: None,
        }
    }
}

impl FootprintConfig {
    pub(crate) fn from_chart_params(params: &ChartTypeParams, colors: [Color32; 4]) -> Self {
        let [ask_color, bid_color, text_color, neutral_color] =
            std::array::from_fn(|i| params.footprint_colors[i].unwrap_or(colors[i]));
        Self {
            modes: params.footprint_modes.clone(),
            profile: params.footprint_profile,
            color_by: params.footprint_color_by,
            ask_color,
            bid_color,
            imbalance_ask_color: params.footprint_imbalance_colors[0],
            imbalance_bid_color: params.footprint_imbalance_colors[1],
            text_color,
            neutral_color,
            auto_row_height: (params.footprint_row_ticks == 0).then_some(16.0),
            row_ticks: params.footprint_row_ticks,
            imbalance_ratio: params.footprint_imbalance_ratio,
            imbalance_min_volume: params.footprint_imbalance_min_volume,
            diagonal: params.footprint_diagonal,
            show_imbalance: params.footprint_imbalance,
            show_poc: params.footprint_poc,
            value_area: params.footprint_value_area,
            abbreviate: params.footprint_abbreviate,
            show_zero: params.footprint_show_zero,
            font_size: params.footprint_font_size,
            extreme_ratio: params.footprint_extreme_ratio,
            pullback: params.footprint_pullback,
            info_box: params.footprint_info_box,
            statistics: params.footprint_statistics,
            statistics_rows: params.footprint_statistics_rows.clone(),
            #[cfg(feature = "studies")]
            studies: (params.footprint_stacked_imbalances
                || params.footprint_naked_poc
                || params.footprint_delta_poc
                || params.footprint_unfinished_auctions
                || params.footprint_cluster_share.is_some()
                || params.footprint_single_print_max.is_some()
                || params.footprint_delta_percent.is_some()
                || params.footprint_ratio.is_some())
            .then_some(crate::studies::FootprintStudyConfig {
                stacked_imbalances: params.footprint_stacked_imbalances,
                stack: params.footprint_stack,
                naked_poc: params.footprint_naked_poc,
                delta_poc: params.footprint_delta_poc,
                unfinished_auctions: params.footprint_unfinished_auctions,
                cluster_share: params.footprint_cluster_share,
                single_print_max: params.footprint_single_print_max,
                delta_percent: params.footprint_delta_percent,
                ratio: params.footprint_ratio,
                imbalance_ratio: params.footprint_imbalance_ratio,
                imbalance_min_volume: params.footprint_imbalance_min_volume,
                diagonal: params.footprint_diagonal,
                ..Default::default()
            }),
            ..Default::default()
        }
    }

    /// The sell and buy colours of imbalance marks: their own, or else the side's.
    fn imbalance_colors(&self) -> [Color32; 2] {
        [
            self.imbalance_bid_color.unwrap_or(self.bid_color),
            self.imbalance_ask_color.unwrap_or(self.ask_color),
        ]
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
    let cell = if has_ladder(config) {
        (width - 0.5) * LADDER_CELL_SHARE
    } else {
        width - 0.5
    };
    (columns > 0 && bar_spacing.is_finite() && cell >= 1.0).then_some(width)
}

/// Draws visible footprint bars, bounded by the visible bar range and clipped to `mapping.rect`.
///
/// For the chart widget use [`crate::widget::Chart::show_footprint`] to keep cells below its
/// legend and cursor. Standalone hosts supply their pane mapping and source bars from the
/// footprint's clock and grouping anchor; trade-based bars keep their exact allocated opens.
/// Text layout uses egui's galley cache; rows below the minimum height never request a layout.
/// Row bands extend half an instrument tick below their first tick and above their last tick,
/// including grouped rows. Fractional volumes below 100 retain
/// up to three decimals; nonzero magnitudes below 0.001 use scientific notation. Only display
/// grouping runs during painting. Cells under a point wide are too narrow to read and are skipped
/// before looking up executions. Unless a [`FootprintMode::Ladder`] column draws its candle, a
/// price spine occupies a separate gutter to the left of each bar: the opening tick points left,
/// the closing tick right, and the colour follows price direction.
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
    let text_background = config
        .background
        .unwrap_or_else(|| painter.ctx().active_theme().chart.bg)
        .to_opaque();
    let steps = config.color_by == FootprintColor::VolumeSteps;
    let imbalance_colors = config.imbalance_colors();
    let spine_drawn = !has_ladder(config);
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
        if spine_drawn
            && price_right > price_left
            && [high, low, open, close].iter().all(|y| y.is_finite())
        {
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
        let shown = group_bar(source, group);
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
        let side_range = |side: fn(&FootprintLevel) -> f64| {
            shown
                .levels
                .values()
                .map(side)
                .fold([f64::INFINITY, 0.0], |[min, max], value| {
                    [min.min(value), max.max(value)]
                })
        };
        let side_ranges = [side_range(|row| row.bid), side_range(|row| row.ask)];
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
                let fit = |width, text: &dyn Fn(&FootprintLevel, bool) -> String| {
                    column_galleys(&painter, &text_rows, max_volume, text, config, width)
                };
                if *mode == FootprintMode::Ladder {
                    let width = (width - 0.5) * LADDER_CELL_SHARE - 2.5;
                    let sides = [
                        fit(width, &|row, compact| ladder_text(row.bid, config, compact)),
                        fit(width, &|row, compact| ladder_text(row.ask, config, compact)),
                    ];
                    // One side must not lose its numbers alone, see `column_galleys`.
                    if sides.iter().all(Option::is_some) {
                        sides
                    } else {
                        [None, None]
                    }
                } else {
                    let text = |row: &FootprintLevel, compact| {
                        cell_text(*mode, row, config.abbreviate || compact, config.show_zero)
                    };
                    [fit(width - 2.5, &text), None]
                }
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
                let ladder = *mode == FootprintMode::Ladder;
                let poc_row = poc == Some(row.level);
                let imbalances = if steps && config.show_imbalance {
                    shown.imbalance(
                        row.level,
                        config.imbalance_ratio,
                        config.diagonal,
                        config.imbalance_min_volume,
                    )
                } else {
                    (false, false)
                };
                let shades = [
                    (row.bid, config.bid_color, 0),
                    (row.ask, config.ask_color, 1),
                ]
                .map(|(volume, color, side)| {
                    if poc_row {
                        config.text_color
                    } else if row.bid + row.ask == 0.0 && row.unknown > 0.0 {
                        step_color(config.neutral_color, 0, text_background)
                    } else {
                        step_color(
                            color,
                            volume_step(volume, side_ranges[side]),
                            text_background,
                        )
                    }
                });
                if steps {
                    // Only on the bar's outer edges, within its half of the gap to the next bar:
                    // three points wide and one clear of the cell while that gap has room.
                    let half_gap = mapping.bar_spacing * 0.04;
                    let clear = (half_gap - 3.0).clamp(0.0, 1.0);
                    let strip_width = half_gap.clamp(1.0, 3.0);
                    for (imbalanced, edge, side, color, shade) in [
                        (
                            imbalances.0 && column == 0,
                            rect.left(),
                            -1.0,
                            imbalance_colors[0],
                            shades[0],
                        ),
                        (
                            imbalances.1 && column + 1 == columns,
                            rect.right(),
                            1.0,
                            imbalance_colors[1],
                            shades[1],
                        ),
                    ] {
                        if imbalanced {
                            let strip = Rect::from_two_pos(
                                pos2(edge + side * clear, rect.top()),
                                pos2(edge + side * (clear + strip_width), rect.bottom()),
                            );
                            // A strip of its cell's colour would only make the cell look wider.
                            let color = if color == shade {
                                step_color(color, 3, text_background)
                            } else {
                                color
                            };
                            mesh.add_colored_rect(strip.intersect(mapping.rect), color);
                        }
                    }
                }
                let sides = if ladder {
                    ladder_sides(rect)
                } else {
                    [rect, rect]
                };
                for (side, rect) in sides.into_iter().enumerate().take(1 + usize::from(ladder)) {
                    let color = cell_color(&shown, row, max_volume, max_delta, config);
                    let (value, largest) = if ladder {
                        (
                            if side == 0 { row.bid } else { row.ask },
                            side_ranges[0][1].max(side_ranges[1][1]),
                        )
                    } else {
                        (row.volume(), max_volume)
                    };
                    // The POC row stays a full band, so both of its numbers lie on its fill.
                    let share = if config.profile && !(steps && poc_row) && largest > 0.0 {
                        (value / largest) as f32
                    } else {
                        1.0
                    };
                    let cell = if steps && rect.height() >= 3.0 {
                        rect.shrink2(vec2(0.0, 0.5))
                    } else {
                        rect
                    };
                    let size = vec2(cell.width() * share, cell.height().max(1.0));
                    // The sell side of a ladder grows away from the candle, mirroring the buy side.
                    let fill = if ladder && side == 0 {
                        Rect::from_min_size(pos2(cell.right() - size.x, cell.top()), size)
                    } else {
                        Rect::from_min_size(cell.min, size)
                    };
                    let mut fills = [
                        area.filter(|_| !steps)
                            .is_some_and(|(low, high)| (low..=high).contains(&row.level))
                            .then_some((rect, config.neutral_color.gamma_multiply(0.12))),
                        Some((fill, color.gamma_multiply(0.5))),
                        None,
                    ];
                    if steps && ladder {
                        fills[1] = Some((fill, shades[side]));
                    } else if steps {
                        let (bid, ask) = fill.split_left_right_at_x(fill.center().x);
                        fills[1] = Some((bid, shades[0]));
                        fills[2] = Some((ask, shades[1]));
                    } else if config.color_by == FootprintColor::Imbalance
                        && shown.imbalance(
                            row.level,
                            config.imbalance_ratio,
                            config.diagonal,
                            config.imbalance_min_volume,
                        ) == (true, true)
                    {
                        if ladder {
                            fills[1] = Some((fill, imbalance_colors[side].gamma_multiply(0.5)));
                        } else {
                            let (bid, ask) = fill.split_left_right_at_x(fill.center().x);
                            fills[1] = Some((bid, imbalance_colors[0].gamma_multiply(0.5)));
                            fills[2] = Some((ask, imbalance_colors[1].gamma_multiply(0.5)));
                        }
                    }
                    let inks = if !steps {
                        [config.text_color; 2]
                    } else if ladder {
                        [contrasting_text(shades[side]); 2]
                    } else {
                        shades.map(contrasting_text)
                    };
                    for &(fill, color) in fills.iter().flatten() {
                        mesh.add_colored_rect(fill.intersect(mapping.rect), color);
                    }
                    if pullback.is_some_and(|(low, high)| (low..=high).contains(&row.level)) {
                        painter.rect_stroke(
                            rect,
                            0,
                            Stroke::new(1.0, config.neutral_color),
                            StrokeKind::Inside,
                        );
                    }
                    if !steps && poc_row && rect.width() >= 12.0 && rect.height() >= 4.0 {
                        painter.rect_stroke(
                            rect,
                            0,
                            Stroke::new(1.0, config.text_color),
                            StrokeKind::Inside,
                        );
                    }
                    if rect.height() >= config.min_text_height.max(10.0)
                        && let Some(galley) = text[column][side]
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
                                FootprintMode::DominantSide if row.bid > row.ask => {
                                    rect.left() + 1.0
                                }
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
                                // Replay the cell fills over an opaque patch so underlays cannot
                                // show through the numbers, keeping their existing cell colours.
                                let backing = Rect::from_min_size(at, galley.size())
                                    .expand2(vec2(2.0, 1.0))
                                    .intersect(rect)
                                    .intersect(mapping.rect);
                                mesh.add_colored_rect(backing, text_background);
                                for &(fill, color) in fills.iter().flatten() {
                                    if fill.intersects(backing) {
                                        mesh.add_colored_rect(fill.intersect(backing), color);
                                    }
                                }
                                // A number overhanging its profile bar lies partly on the
                                // background, where a colour picked for the fill can vanish.
                                let inks = if fill.left() <= at.x
                                    && at.x + galley.size().x <= fill.right()
                                {
                                    inks
                                } else {
                                    [config.text_color; 2]
                                };
                                if inks[0] == inks[1] {
                                    painter.galley_with_override_text_color(at, galley, inks[0]);
                                } else {
                                    // Halves that need different inks each show their part.
                                    let (sell, buy) = fill.split_left_right_at_x(fill.center().x);
                                    for (half, ink) in [sell, buy].into_iter().zip(inks) {
                                        let half =
                                            Rect::from_x_y_ranges(half.x_range(), rect.y_range());
                                        painter
                                            .with_clip_rect(painter.clip_rect().intersect(half))
                                            .galley_with_override_text_color(
                                                at,
                                                galley.clone(),
                                                ink,
                                            );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        for (column, _) in config
            .modes
            .iter()
            .take(columns)
            .enumerate()
            .filter(|(_, mode)| **mode == FootprintMode::Ladder)
        {
            let block = Rect::from_x_y_ranges(
                left + column as f32 * width..=left + (column + 1) as f32 * width - 0.5,
                0.0..=0.0,
            );
            if [high, low, open, close].iter().all(|y| y.is_finite()) {
                let center = block.center().x;
                let half_body = (width * 0.087).max(0.5);
                let mid = (open + close) * 0.5;
                let half_height = ((open - close).abs() * 0.5).max(1.0);
                mesh.add_colored_rect(
                    Rect::from_two_pos(pos2(center - 0.5, high), pos2(center + 0.5, low))
                        .intersect(mapping.rect),
                    color,
                );
                mesh.add_colored_rect(
                    Rect::from_two_pos(
                        pos2(center - half_body, mid - half_height),
                        pos2(center + half_body, mid + half_height),
                    )
                    .intersect(mapping.rect),
                    color,
                );
            }
            if config.info_box && text[column][0].is_some() {
                render_info_box(&painter, mapping, flow, &shown, group, block, config);
            }
        }
        if steps && let Some((low, high)) = area {
            let block_right = left + columns as f32 * width - 0.5;
            let step = flow.price_step() * group as f64;
            for (level, label) in [(high + 1, "VAH"), (low, "VAL")] {
                let y = mapping.price_to_y(level as f64 * step - flow.tick_size() * 0.5);
                if !y.is_finite() {
                    continue;
                }
                let y = painter.round_to_pixel_center(y);
                draw_dashed_line(
                    &painter,
                    pos2(left, y),
                    pos2(block_right, y),
                    Stroke::new(1.0, config.text_color),
                    2.0,
                    2.0,
                );
                // The marker needs the gap to the previous bar, or it covers that bar's cell.
                if mapping.bar_spacing * 0.08 >= 6.0 {
                    painter.add(Shape::convex_polygon(
                        vec![
                            pos2(left - 6.0, y - 3.0),
                            pos2(left - 3.0, y),
                            pos2(left - 6.0, y + 3.0),
                        ],
                        config.text_color,
                        Stroke::NONE,
                    ));
                    let galley = painter.layout_no_wrap(
                        label.to_owned(),
                        FontId::monospace(FONT_LIMITS[0]),
                        config.text_color,
                    );
                    // Beside the marker only where the gap to the previous bar has room.
                    if galley.size().x + 7.0 <= mapping.bar_spacing * 0.08 {
                        let at =
                            Align2::RIGHT_CENTER.anchor_size(pos2(left - 7.0, y), galley.size());
                        painter.galley(at.min, galley, config.text_color);
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
    cell: &dyn Fn(&FootprintLevel, bool) -> String,
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
                let mut text = cell(row, compact);
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
        FootprintMode::BidAsk | FootprintMode::Ladder if hidden(row.bid) && hidden(row.ask) => {
            String::new()
        }
        FootprintMode::BidAsk | FootprintMode::Ladder => {
            format!("{} × {}", number(row.bid), number(row.ask))
        }
        FootprintMode::Volume => number(row.volume()),
        FootprintMode::Delta => signed(row.delta()),
        FootprintMode::DeltaVolume if hidden(row.delta()) && hidden(row.volume()) => String::new(),
        FootprintMode::DeltaVolume => {
            format!("{} / {}", signed(row.delta()), number(row.volume()))
        }
        FootprintMode::DominantSide => number(row.bid.max(row.ask)),
        FootprintMode::BidAskTrades
            if hidden(row.bid_trades as f64) && hidden(row.ask_trades as f64) =>
        {
            String::new()
        }
        FootprintMode::BidAskTrades => format!(
            "{} × {}",
            number(row.bid_trades as f64),
            number(row.ask_trades as f64)
        ),
        FootprintMode::Trades => number(row.trades() as f64),
        FootprintMode::AverageSize => row.average_size().map_or_else(String::new, number),
    }
}

/// The number of a ladder cell: two decimals when abbreviated, one when the column is too narrow.
fn ladder_text(value: f64, config: &FootprintConfig, compact: bool) -> String {
    if value == 0.0 && !config.show_zero {
        return String::new();
    }
    if config.abbreviate && !compact && value.abs() >= 1000.0 {
        // 999,995 and above round to 1000.00K, so they take the next unit.
        let (scaled, unit) = if value.abs() >= 999_995.0 {
            (value / 1e6, 'M')
        } else {
            (value / 1e3, 'K')
        };
        return format!(
            "{}{unit}",
            egui::emath::format_with_decimals_in_range(scaled, 0..=2)
        );
    }
    volume_text(value, config.abbreviate || compact)
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

/// Input rows per displayed row holding at least `row_ticks` instrument ticks; zero keeps the
/// input rows.
pub(crate) fn fixed_row_group(flow: &Footprint, row_ticks: u32) -> i64 {
    let input_ticks = (flow.price_step() / flow.tick_size()).round() as u32;
    i64::from(row_ticks.div_ceil(input_ticks).clamp(1, 1_000_000))
}

/// The automatic row height, when it is a usable one: the renderer ignores a height that is not
/// finite and positive.
pub(crate) fn automatic_row_height(config: &FootprintConfig) -> Option<f32> {
    config.auto_row_height.filter(|h| h.is_finite() && *h > 0.0)
}

/// Input rows per displayed row of `bar` under `mapping`: the minimum group, raised until a row
/// is as tall as a usable automatic height.
pub(crate) fn row_group(
    mapping: &ChartMapping,
    bar: &Bar,
    flow: &Footprint,
    config: &FootprintConfig,
) -> i64 {
    let raw_height =
        (mapping.price_to_y(bar.close + flow.price_step()) - mapping.price_to_y(bar.close)).abs();
    let manual = fixed_row_group(flow, config.row_ticks);
    automatic_row_height(config).map_or(manual, |height| {
        (height / raw_height.max(0.01))
            .ceil()
            .clamp(manual as f32, 1_000_000.0) as i64
    })
}

/// The prices of the bottom edge of the lowest row and the top edge of the highest row of each
/// complete bar of `bars`, its rows holding `group(bar)` input rows.
pub(crate) fn row_edges<'a>(
    bars: &'a [Bar],
    flow: &'a Footprint,
    group: impl Fn(&Bar) -> i64 + 'a,
) -> impl Iterator<Item = f64> + 'a {
    bars.iter()
        .filter(move |bar| flow.complete_from().is_some_and(|from| bar.time >= from))
        .filter_map(move |bar| {
            let rows = &flow.bar_at(bar.time)?.levels;
            let (&low, _) = rows.first_key_value()?;
            let (&high, _) = rows.last_key_value()?;
            let group = group(bar);
            let step = flow.price_step() * group as f64;
            Some(
                [
                    low.div_euclid(group) as f64,
                    high.div_euclid(group) as f64 + 1.0,
                ]
                .map(|row| row * step - flow.tick_size() * 0.5),
            )
        })
        .flatten()
}

fn group_bar(source: &FootprintBar, group: i64) -> Cow<'_, FootprintBar> {
    if group == 1 {
        return Cow::Borrowed(source);
    }
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
        target.absorb(row);
    }
    Cow::Owned(shown)
}

fn has_ladder(config: &FootprintConfig) -> bool {
    config
        .modes
        .iter()
        .take(4)
        .any(|mode| *mode == FootprintMode::Ladder)
}

/// How far below a bar's lowest row its summary starts: past the extreme ratio and threshold
/// marker rows, which sit directly below the bar, when they are on.
fn info_box_offset(config: &FootprintConfig) -> f32 {
    #[cfg(feature = "studies")]
    let studies = config.studies.is_some();
    #[cfg(not(feature = "studies"))]
    let studies = false;
    if config.extreme_ratio || studies {
        20.0
    } else {
        2.0
    }
}

/// The room a bar's summary takes below its lowest row, in points; zero when none is drawn.
pub(crate) fn summary_room(config: &FootprintConfig) -> f32 {
    if config.info_box && has_ladder(config) {
        info_box_offset(config) + INFO_BOX_HEIGHT
    } else {
        0.0
    }
}

/// The sell and buy cells of a ladder column; the candle body takes the gap between them.
fn ladder_sides(column: Rect) -> [Rect; 2] {
    let width = column.width() * LADDER_CELL_SHARE;
    [
        Rect::from_x_y_ranges(column.left()..=column.left() + width, column.y_range()),
        Rect::from_x_y_ranges(column.right() - width..=column.right(), column.y_range()),
    ]
}

/// The quarter of the `[smallest, largest]` range of its side that `volume` falls in, 0 to 3. A
/// side that traded one volume throughout takes the strongest shade, or the weakest at zero.
fn volume_step(volume: f64, [smallest, largest]: [f64; 2]) -> usize {
    if largest > smallest {
        (((volume - smallest) / (largest - smallest) * 4.0) as usize).min(3)
    } else if volume > 0.0 {
        3
    } else {
        0
    }
}

/// One of the four opaque shades of a side's colour, from the one that recedes into the chart
/// background to the one that stands out most: on a light background the colour itself is
/// second and the strongest two darken it, on a dark one it is never reached so numbers stay
/// readable.
fn step_color(color: Color32, step: usize, background: Color32) -> Color32 {
    if background.intensity() > 0.5 {
        match step {
            0 => background.lerp_to_gamma(color, 0.5),
            1 => color,
            2 => Color32::BLACK.lerp_to_gamma(color, 0.84),
            _ => Color32::BLACK.lerp_to_gamma(color, 0.53),
        }
    } else {
        background.lerp_to_gamma(color, (step + 1) as f32 * 0.2)
    }
}

/// Black or white, whichever reads on `fill`.
fn contrasting_text(fill: Color32) -> Color32 {
    if fill.intensity() > 0.6 {
        Color32::BLACK
    } else {
        Color32::WHITE
    }
}

/// Writes a bar's sell and buy totals below their ladder cells, and its delta and total volume in
/// a box below them: beside their labels, or as values alone, compact if need be, where the labels
/// do not fit. Each part is drawn whole or not at all.
fn render_info_box(
    painter: &Painter,
    mapping: &ChartMapping,
    flow: &Footprint,
    shown: &FootprintBar,
    group: i64,
    block: Rect,
    config: &FootprintConfig,
) {
    let (Some((&lowest, _)), Some((&highest, _))) = (
        shown.levels.first_key_value(),
        shown.levels.last_key_value(),
    ) else {
        return;
    };
    let step = flow.price_step() * group as f64;
    let edge = |level: i64| mapping.price_to_y(level as f64 * step - flow.tick_size() * 0.5);
    // An inverted scale puts the highest row at the bottom.
    let (low_edge, high_edge) = (edge(lowest), edge(highest + 1));
    if !low_edge.is_finite() || !high_edge.is_finite() {
        return;
    }
    let bottom = low_edge.max(high_edge);
    let font = FontId::monospace(10.0);
    let text = |value, color| {
        painter.layout_no_wrap(ladder_text(value, config, false), font.clone(), color)
    };
    let top = bottom + info_box_offset(config);
    let sides = ladder_sides(block);
    let totals = [(shown.bid, config.bid_color), (shown.ask, config.ask_color)];
    for (side, (value, color)) in sides.iter().zip(totals) {
        let mut galley = text(value, color);
        if galley.size().x > side.width() {
            galley = painter.layout_no_wrap(ladder_text(value, config, true), font.clone(), color);
        }
        let bounds = Align2::CENTER_TOP.anchor_size(pos2(side.center().x, top), galley.size());
        if galley.size().x <= side.width() && mapping.rect.contains_rect(bounds) {
            painter.galley(bounds.min, galley, color);
        }
    }
    let delta = shown.delta();
    let delta_color = if delta > 0.0 {
        config.ask_color
    } else if delta < 0.0 {
        config.bid_color
    } else {
        config.text_color
    };
    let labels = ["Delta", "Total"]
        .map(|label| painter.layout_no_wrap(label.to_owned(), font.clone(), config.text_color));
    let line = labels[0].size().y;
    let labels_width = labels[0].size().x.max(labels[1].size().x) + 6.0;
    let bounds = Rect::from_x_y_ranges(block.x_range(), top + line + 3.0..=top + line * 3.0 + 9.0);
    if !mapping.rect.contains_rect(bounds) {
        return;
    }
    let values_width = |values: &[Arc<Galley>; 2]| values[0].size().x.max(values[1].size().x);
    let room = bounds.width() - 8.0;
    let mut values = [
        text(delta, delta_color),
        text(shown.volume(), config.text_color),
    ];
    let labelled = labels_width + values_width(&values) <= room;
    if values_width(&values) > room {
        values =
            [(delta, delta_color), (shown.volume(), config.text_color)].map(|(value, color)| {
                painter.layout_no_wrap(
                    ladder_text(value, config, true),
                    FontId::monospace(FONT_LIMITS[0]),
                    color,
                )
            });
        if values_width(&values) > room {
            return;
        }
    }
    let background = config
        .background
        .unwrap_or_else(|| painter.ctx().active_theme().chart.bg)
        .to_opaque();
    painter.rect_filled(bounds, 4, background);
    painter.rect_stroke(
        bounds,
        4,
        Stroke::new(1.0, config.neutral_color.gamma_multiply(0.25)),
        StrokeKind::Inside,
    );
    let x = bounds.center().x - (labels_width + values_width(&values)) * 0.5;
    for (i, (label, value)) in labels.into_iter().zip(values).enumerate() {
        let y = bounds.top() + 3.0 + i as f32 * line;
        if labelled {
            painter.galley(pos2(x, y), label, config.text_color);
            painter.galley(pos2(x + labels_width, y), value, config.text_color);
        } else {
            // Delta above total, in the order of the labels they stand without.
            let center = pos2(bounds.center().x, y + line * 0.5);
            let at = Align2::CENTER_CENTER.anchor_size(center, value.size());
            painter.galley(at.min, value, config.text_color);
        }
    }
}

fn cell_color(
    bar: &FootprintBar,
    row: &FootprintLevel,
    max_volume: f64,
    max_delta: f64,
    config: &FootprintConfig,
) -> Color32 {
    match config.color_by {
        FootprintColor::Fixed | FootprintColor::VolumeSteps => config.neutral_color,
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
            match bar.imbalance(
                row.level,
                config.imbalance_ratio,
                config.diagonal,
                config.imbalance_min_volume,
            ) {
                (true, false) => config.imbalance_colors()[0],
                (false, true) => config.imbalance_colors()[1],
                _ => config.neutral_color,
            }
        }
    }
}

/// Draws a compact per-bar statistics table under the footprint at the same x coordinates.
/// Rows show total, bid, ask, delta, minimum and maximum intrabar delta, cumulative delta,
/// delta percentage and ask/bid ratio (undefined without a denominator). Select other rows with
/// [`render_footprint_statistics_rows`] or [`FootprintConfig::statistics_rows`] in
/// [`crate::widget::Chart::show_footprint`]. Bars before
/// [`Footprint::complete_from`] get no column and no share of the cumulative delta.
/// Shared labels occupy the left margin. Numeric columns stay aligned with their bars and
/// use unabbreviated volume values when they fit, then compact units. Percentages have one
/// decimal and ask/bid ratios two decimals.
/// A column cut by the margin or plot edge is omitted as a whole. When the complete table
/// cannot fit at a readable size, a zoom-in or resize notice replaces it; values remain in
/// [`FootprintBar`].
/// A view without complete executions reports that statistics are unavailable.
pub fn render_footprint_statistics(
    painter: &Painter,
    mapping: &ChartMapping,
    bars: &[Bar],
    flow: &Footprint,
    text_color: Color32,
) {
    let config = FootprintConfig {
        text_color,
        ..Default::default()
    };
    render_footprint_statistics_rows(painter, mapping, bars, flow, &config);
}

/// Draws the rows of [`FootprintConfig::statistics_rows`] in their order and in
/// [`FootprintConfig::text_color`]. An empty selection draws nothing. Uses the same fitting and
/// clipping rules as [`render_footprint_statistics`]; an undefined value is shown as `n/a`.
///
/// Buy and sell percentages are shares of the bar's total volume, and the volume change is
/// relative to the previous bar with executions and blank for the first; all percentages have
/// one decimal. The range is the bar's high minus its low. The point of control is the lowest
/// price of the highest-volume row among the rows [`render_footprint`] groups for `config` under
/// the price mapping of `mapping`, so it names the row marked in the bar. Range and point of
/// control are written at the decimals of the instrument's tick.
pub fn render_footprint_statistics_rows(
    painter: &Painter,
    mapping: &ChartMapping,
    bars: &[Bar],
    flow: &Footprint,
    config: &FootprintConfig,
) {
    let (rows, text_color) = (config.statistics_rows.as_slice(), config.text_color);
    if rows.is_empty() {
        return;
    }
    let painter = painter.with_clip_rect(mapping.rect.intersect(painter.clip_rect()));
    let labels: Vec<_> = rows.iter().map(|row| row.label()).collect();
    let row_height = (mapping.rect.height() / labels.len() as f32).min(12.0);
    let font = FontId::monospace(10.0);
    let labels: Vec<_> = labels
        .into_iter()
        .map(|label| painter.layout_no_wrap(label.into(), font.clone(), text_color))
        .collect();
    let rows_fit = labels.iter().all(|label| label.size().y <= row_height);
    let label_width = labels
        .iter()
        .map(|label| label.size().x)
        .fold(0.0, f32::max)
        + 8.0;
    let mut cumulative = 0.0;
    let mut last_volume = None;
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
        let volume = row.volume();
        let previous = last_volume.replace(volume);
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
        let share = |side: f64| (volume > 0.0).then(|| side / volume * 100.0);
        let poc = rows
            .contains(&FootprintStatistic::PointOfControl)
            .then(|| {
                let group = row_group(mapping, bar, flow, config);
                let step = flow.price_step() * group as f64;
                group_bar(row, group).poc().map(|level| level as f64 * step)
            })
            .flatten();
        let values = [
            Some(volume),
            Some(row.bid),
            Some(row.ask),
            Some(row.delta()),
            Some(row.min_delta),
            Some(row.max_delta),
            Some(cumulative),
            row.delta_percent(),
            (row.bid > 0.0).then(|| row.ask / row.bid),
            Some(row.trades() as f64),
            row.average_size(),
            share(row.bid),
            share(row.ask),
            Some(bar.high - bar.low),
            poc,
            previous
                .filter(|previous| *previous > 0.0)
                .map(|previous| (volume - previous) / previous * 100.0),
        ];
        let values: Vec<_> = rows.iter().map(|row| values[*row as usize]).collect();
        columns.push((x, previous.is_none(), values));
    }
    let decimals = crate::scales::decimals(flow.tick_size());
    let mut size = 10.0;
    let mut fitted = Vec::new();
    if mapping.bar_spacing >= 22.0 && rows_fit {
        'fitting: for abbreviate in [false, true] {
            for font_size in [10.0, FONT_LIMITS[0]] {
                size = font_size;
                fitted = columns
                    .iter()
                    .map(|(x, first, values)| {
                        (
                            *x,
                            values
                                .iter()
                                .copied()
                                .enumerate()
                                .map(|(line, value)| {
                                    let text = match (rows[line], value) {
                                        // The first bar has no bar to change from.
                                        (FootprintStatistic::VolumeChange, None) if *first => {
                                            String::new()
                                        }
                                        (_, None) => "n/a".to_owned(),
                                        (
                                            FootprintStatistic::DeltaPercent
                                            | FootprintStatistic::BidPercent
                                            | FootprintStatistic::AskPercent
                                            | FootprintStatistic::VolumeChange,
                                            Some(value),
                                        ) => format!("{value:.1}%"),
                                        (FootprintStatistic::AskBidRatio, Some(value)) => {
                                            format!("{value:.2}:1")
                                        }
                                        (
                                            FootprintStatistic::Range
                                            | FootprintStatistic::PointOfControl,
                                            Some(value),
                                        ) => format!("{value:.decimals$}"),
                                        (_, Some(value)) => volume_text(value, abbreviate),
                                    };
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
            if !has_data {
                "Execution statistics unavailable"
            } else if !rows_fit {
                "Enlarge chart for per-bar statistics"
            } else {
                "Zoom in for per-bar statistics"
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
    revision: Option<u64>,
    source: Vec<(chrono::DateTime<chrono::Utc>, i64)>,
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
    let settings = crate::studies::FootprintStudyConfig {
        imbalance_min_volume: config.imbalance_min_volume,
        ..settings.clone()
    };
    let complete = flow.complete_from();
    let source: Vec<_> = bars
        .iter()
        .filter(|bar| complete.is_some_and(|from| bar.time >= from))
        .filter_map(|bar| {
            flow.bar_at(bar.time)
                .map(|row| (bar.time, row, row_group(mapping, bar, flow, config)))
        })
        .collect();
    let studies = painter.ctx().data_mut(|data| {
        let cache = data
            .get_temp_mut_or_default::<FootprintStudyCache>(chart_id.with("footprint_annotations"));
        if cache.revision != Some(flow.revision())
            || cache.config.as_ref() != Some(&settings)
            || cache.source.len() != source.len()
            || !cache.source.iter().zip(&source).all(
                |((time, group), (next_time, _, next_group))| {
                    time == next_time && group == next_group
                },
            )
        {
            cache.source = source
                .iter()
                .map(|(time, _, group)| (*time, *group))
                .collect();
            let grouped: Vec<_> = source
                .iter()
                .map(|(_, bar, group)| (group_bar(bar, *group), *group))
                .collect();
            let rows: Vec<_> = grouped
                .iter()
                .map(|(bar, group)| (bar.as_ref(), *group))
                .collect();
            cache.studies = crate::studies::footprint_studies_on_rows(&rows, &settings).into();
            cache.config = Some(settings);
            cache.revision = Some(flow.revision());
        }
        cache.studies.clone()
    });
    render_footprint_studies(painter, mapping, bars, flow, &studies, config);
}

/// Paints footprint study levels and zones. Compute `studies` only after executions change.
/// Stacks, naked POCs and unfinished levels extend to their first revisit, their expiry or the
/// last displayed bar, in full colour while unvisited and dimmed once revisited or expired;
/// stacks take the imbalance colour of their side;
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
            // The two threshold-marker rows extend up to 18 points beyond either extreme, and a
            // ladder's summary below the bar takes the same space or more.
            let mut reserved = rect.expand2(vec2(0.0, 18.0));
            let summary = summary_room(config);
            if summary > 0.0 {
                reserved.max.y = rect.max.y + summary;
            }
            rect.intersects(mapping.rect).then_some((index, reserved))
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
            FootprintStudyKind::StackedBid => config.imbalance_colors()[0],
            FootprintStudyKind::StackedAsk => config.imbalance_colors()[1],
            FootprintStudyKind::DeltaBid | FootprintStudyKind::RatioBid => config.bid_color,
            FootprintStudyKind::DeltaAsk | FootprintStudyKind::RatioAsk => config.ask_color,
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

    #[cfg(feature = "studies")]
    fn cached_annotations(
        ctx: &egui::Context,
        mapping: &ChartMapping,
        bars: &[Bar],
        flow: &Footprint,
        config: &FootprintConfig,
    ) -> Arc<[crate::studies::FootprintStudy]> {
        let id = egui::Id::unique("cache regression");
        let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
            render_footprint_annotations(ui.painter(), mapping, bars, flow, config, id);
        });
        output.textures_delta.clear();
        ctx.data_mut(|data| {
            data.get_temp::<FootprintStudyCache>(id.with("footprint_annotations"))
                .unwrap()
                .studies
        })
    }

    #[cfg(feature = "studies")]
    fn annotation_mapping() -> ChartMapping {
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(240.0, 160.0));
        ChartMapping::new(
            rect,
            120.0,
            0,
            0,
            0.0,
            PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(98.0, 110.0),
                0.0,
                160.0,
                false,
            ),
        )
    }

    #[cfg(feature = "studies")]
    #[test]
    fn annotation_cache_matches_eager_studies_across_selection_group_and_config_changes() {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        let bars: Vec<_> = (0..3)
            .map(|minute| {
                let at = time + chrono::TimeDelta::minutes(minute);
                for (price, size, side) in [
                    (100.0, 2.0, OrderSide::Sell),
                    (101.0, 7.0, OrderSide::Buy),
                    (102.0, 9.0, OrderSide::Buy),
                    (103.0, 11.0, OrderSide::Buy),
                    (104.0, 1.0, OrderSide::Sell),
                ] {
                    flow.push(&Tick::new(at, price, size, Some(side))).unwrap();
                }
                Bar::new(at, 100.0, 104.0, 100.0, 104.0, 30.0)
            })
            .collect();
        let ctx = egui::Context::default();
        let map = annotation_mapping();
        let mut config = FootprintConfig {
            auto_row_height: None,
            studies: Some(crate::studies::FootprintStudyConfig {
                cluster_share: Some(0.25),
                single_print_max: Some(2.0),
                delta_percent: Some(50.0),
                ratio: Some(3.0),
                ..Default::default()
            }),
            ..Default::default()
        };
        let eager = |selected: &[Bar], flow: &Footprint, config: &FootprintConfig| {
            let grouped: Vec<_> = selected
                .iter()
                .filter(|bar| flow.complete_from().is_some_and(|from| bar.time >= from))
                .filter_map(|bar| {
                    flow.bar_at(bar.time).map(|source| {
                        let group = row_group(&map, bar, flow, config);
                        let mut owned = FootprintBar::empty(source.time);
                        owned.absorb_grouped(source, group);
                        (owned, group)
                    })
                })
                .collect();
            let rows: Vec<_> = grouped.iter().map(|(bar, group)| (bar, *group)).collect();
            let settings = crate::studies::FootprintStudyConfig {
                imbalance_min_volume: config.imbalance_min_volume,
                ..config.studies.clone().unwrap()
            };
            crate::studies::footprint_studies_on_rows(&rows, &settings)
        };
        for selected in [
            bars.clone(),
            bars[1..].to_vec(),
            vec![bars[0].clone(), bars[2].clone()],
            bars.iter().rev().cloned().collect(),
        ] {
            for group in [1, 2, 3] {
                config.row_ticks = group;
                let cached = cached_annotations(&ctx, &map, &selected, &flow, &config);
                assert_eq!(&*cached, eager(&selected, &flow, &config));
                let repeated = cached_annotations(&ctx, &map, &selected, &flow, &config);
                assert!(Arc::ptr_eq(&cached, &repeated));
            }
        }
        config.row_ticks = 1;
        config.auto_row_height = Some(20.0);
        for minimum in [0.0, 100.0, 0.0] {
            config.imbalance_min_volume = minimum;
            let cached = cached_annotations(&ctx, &map, &bars, &flow, &config);
            assert_eq!(&*cached, eager(&bars, &flow, &config));
        }
        config.studies.as_mut().unwrap().naked_poc = false;
        let cached = cached_annotations(&ctx, &map, &bars, &flow, &config);
        assert_eq!(&*cached, eager(&bars, &flow, &config));
        flow.received_from(time);
        let covered = cached_annotations(&ctx, &map, &bars, &flow, &config);
        assert!(!Arc::ptr_eq(&cached, &covered));
        assert_eq!(&*covered, eager(&bars, &flow, &config));
        flow.received_from(time);
        flow.set_budget(100, 100);
        let repeated = cached_annotations(&ctx, &map, &bars, &flow, &config);
        assert!(Arc::ptr_eq(&covered, &repeated));
    }

    /// The settings of a chart saved before the ladder became the default.
    fn bid_ask_config() -> FootprintConfig {
        FootprintConfig {
            modes: vec![FootprintMode::BidAsk],
            profile: true,
            color_by: FootprintColor::Delta,
            abbreviate: false,
            ..Default::default()
        }
    }

    /// Volumes fall into equal quarters of their side's range, whatever the other side traded, and
    /// each shade is the hand-worked mix of the side's colour with the background or black.
    #[test]
    fn volume_steps_follow_their_side_range_and_theme() {
        #[rustfmt::skip]
        let steps = [
            (10.0, [10.0, 50.0], 0),
            (19.9, [10.0, 50.0], 0),
            (20.0, [10.0, 50.0], 1),
            (30.0, [10.0, 50.0], 2),
            (40.0, [10.0, 50.0], 3),
            (50.0, [10.0, 50.0], 3),
            (50.0, [50.0, 50.0], 3),
            (0.0, [0.0, 0.0], 0),
        ];
        for (volume, range, step) in steps {
            assert_eq!(volume_step(volume, range), step, "{volume} in {range:?}");
        }
        let sell = Color32::from_rgb(242, 54, 69);
        #[rustfmt::skip]
        let shades = [
            (Color32::WHITE, 0, [248, 154, 162]),
            (Color32::WHITE, 1, [242, 54, 69]),
            (Color32::WHITE, 2, [203, 45, 58]),
            (Color32::WHITE, 3, [128, 29, 37]),
            (Color32::BLACK, 0, [48, 11, 14]),
            (Color32::BLACK, 1, [97, 22, 28]),
            (Color32::BLACK, 2, [145, 32, 41]),
            (Color32::BLACK, 3, [194, 43, 55]),
        ];
        for (background, step, [r, g, b]) in shades {
            let shade = step_color(sell, step, background);
            for (channel, expected) in [(shade.r(), r), (shade.g(), g), (shade.b(), b)] {
                assert!(
                    channel.abs_diff(expected) <= 1,
                    "{background:?} {step}: {shade:?}"
                );
            }
        }
        assert_eq!(
            contrasting_text(step_color(sell, 0, Color32::WHITE)),
            Color32::BLACK
        );
        assert_eq!(
            contrasting_text(step_color(sell, 1, Color32::WHITE)),
            Color32::WHITE
        );
        assert_eq!(contrasting_text(Color32::WHITE), Color32::BLACK);
    }

    /// With the default settings each row has a sell cell left and a buy cell right of a candle
    /// body. The bar holds (sell, buy) rows 104: 10, 50; 103: 10, 20; 102: 50, 50; 101: 20, 10;
    /// 100: 30, 10. 50 bought at 104 is five times the 10 sold one row below; 30 sold at 100 is
    /// exactly three times the 10 bought one row above; no other diagonal pair reaches three.
    /// Value area of 182 out of 260: POC 102 (100), a tie adds 103 and 101 (160), then 104 (220),
    /// so its edges are at 104.5 and 100.5.
    #[test]
    fn the_default_ladder_shades_sides_and_marks_imbalances_and_the_value_area() {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        let bars = [Bar::new(time, 101.0, 104.0, 100.0, 103.0, 260.0)];
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(240.0, 160.0));
        let map = ChartMapping::new(
            rect,
            120.0,
            0,
            0,
            0.0,
            PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(98.0, 110.0),
                0.0,
                160.0,
                false,
            ),
        );
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        for (price, sell, buy) in [
            (104.0, 10.0, 50.0),
            (103.0, 10.0, 20.0),
            (102.0, 50.0, 50.0),
            (101.0, 20.0, 10.0),
            (100.0, 30.0, 10.0),
        ] {
            for (size, side) in [(sell, OrderSide::Sell), (buy, OrderSide::Buy)] {
                flow.push(&Tick::new(time, price, size, Some(side)))
                    .unwrap();
            }
        }
        let config = FootprintConfig {
            auto_row_height: None,
            min_text_height: 100.0,
            text_color: Color32::from_rgb(1, 2, 3),
            neutral_color: Color32::from_rgb(4, 5, 6),
            ..Default::default()
        };
        let ctx = egui::Context::default();
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ui| render_footprint(ui.painter(), &map, &bars, &flow, &config),
        );
        output.textures_delta.clear();
        let quads = quads(&output.shapes);
        let at = |x: f32, price: f64| color_at(&quads, pos2(x, map.price_to_y(price)));
        let background = ctx.active_theme().chart.bg.to_opaque();
        let center = map.idx_to_x(0);
        let edge = 120.0 * 0.46;
        let [sell_x, buy_x] = [center - edge + 20.0, center + edge - 20.0];
        let sell = |step| Some(step_color(config.bid_color, step, background));
        let buy = |step| Some(step_color(config.ask_color, step, background));
        let poc = Some(config.text_color);
        for (price, sell_shade, buy_shade) in [
            (104.0, sell(0), buy(3)),
            (103.0, sell(0), buy(1)),
            (102.0, poc, poc),
            (101.0, sell(1), buy(0)),
            (100.0, sell(2), buy(0)),
        ] {
            assert_eq!(at(sell_x, price), sell_shade, "sell at {price}");
            assert_eq!(at(buy_x, price), buy_shade, "buy at {price}");
        }
        let [sell_strip, buy_strip] = [center - edge - 2.5, center + edge - 0.5 + 2.5];
        for price in [104.0, 103.0, 102.0, 101.0, 100.0] {
            assert_eq!(
                at(buy_strip, price),
                (price == 104.0).then_some(config.ask_color),
                "buy strip at {price}"
            );
            assert_eq!(
                at(sell_strip, price),
                (price == 100.0).then_some(config.bid_color),
                "sell strip at {price}"
            );
        }
        // The body spans the open and close, the wick the high; neither reaches the side cells.
        let middle = center - 0.25;
        assert_eq!(at(middle + 5.0, 102.0), Some(config.ask_color));
        assert_eq!(at(middle + 5.0, 103.75), None);
        assert_eq!(at(middle, 103.75), Some(config.ask_color));
        assert_eq!(at(middle + 10.5, 102.0), None);
        // Two dotted lines and no fill mark the value area.
        assert!(
            quads
                .iter()
                .all(|(_, c)| *c != config.neutral_color.gamma_multiply(0.12))
        );
        let mut dotted: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::LineSegment { points, stroke } if stroke.color == config.text_color => {
                    Some(points[0].y)
                }
                _ => None,
            })
            .collect();
        dotted.sort_by(f32::total_cmp);
        dotted.dedup();
        assert_eq!(dotted.len(), 2);
        for (y, price) in dotted.iter().zip([104.5, 100.5]) {
            assert!((y - map.price_to_y(price)).abs() <= 1.0, "{y} at {price}");
        }
        // In the profile display a number wider than its bar takes the text colour, because a
        // colour picked for the fill can vanish on the background; 50 fills its cell.
        let profile = FootprintConfig {
            profile: true,
            min_text_height: 10.0,
            ..config.clone()
        };
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ui| render_footprint(ui.painter(), &map, &bars, &flow, &profile),
        );
        output.textures_delta.clear();
        let inks = |number: &str| -> Vec<_> {
            output
                .shapes
                .iter()
                .filter_map(|s| match &s.shape {
                    Shape::Text(t) if t.galley.text() == number => t.override_text_color,
                    _ => None,
                })
                .collect()
        };
        assert_eq!(inks("10"), [profile.text_color; 4]);
        assert_eq!(inks("50"), [Color32::WHITE; 3]);
    }

    /// One bar of (price, sell, buy) rows at a spacing, painted with `config` over the 98 to 110
    /// range, inverted or not. Returns the shapes and the centre of the bar.
    fn paint_ladder(
        rows: &[(f64, f64, f64)],
        spacing: f32,
        inverted: bool,
        config: &FootprintConfig,
    ) -> (Vec<Shape>, f32) {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        // Open and close match so the candle takes the text colour, not a side's.
        let bars = [Bar::new(time, 102.0, 104.0, 100.0, 102.0, 1.0)];
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(480.0, 160.0));
        let map = ChartMapping::new(
            rect,
            spacing,
            0,
            0,
            0.0,
            PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(98.0, 110.0),
                0.0,
                160.0,
                inverted,
            ),
        );
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        for &(price, sell, buy) in rows {
            for (size, side) in [(sell, OrderSide::Sell), (buy, OrderSide::Buy)] {
                flow.push(&Tick::new(time, price, size, Some(side)))
                    .unwrap();
            }
        }
        let mut output = egui::Context::default().run_ui(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ui| render_footprint(ui.painter(), &map, &bars, &flow, config),
        );
        output.textures_delta.clear();
        let shapes = output.shapes.into_iter().map(|s| s.shape).collect();
        (shapes, map.idx_to_x(0))
    }

    fn painted_text(shapes: &[Shape]) -> Vec<(String, Rect)> {
        shapes
            .iter()
            .filter_map(|shape| match shape {
                Shape::Text(t) => Some((
                    t.galley.text().to_owned(),
                    Rect::from_min_size(t.pos, t.galley.size()),
                )),
                _ => None,
            })
            .collect()
    }

    /// Imbalance strips belong to the outer edges of their bar and to its half of the gap, and
    /// value-area markers to the gap before it, so neither covers a neighbouring cell or column.
    #[test]
    fn ladder_marks_stay_on_the_outer_edges_of_their_bar() {
        // 50 bought at 104 and 30 sold at 100 are imbalanced; see the default ladder test.
        let rows = [
            (104.0, 10.0, 50.0),
            (103.0, 10.0, 20.0),
            (102.0, 50.0, 50.0),
            (101.0, 20.0, 10.0),
            (100.0, 30.0, 10.0),
        ];
        for (spacing, modes, show_imbalance) in [
            (120.0, vec![FootprintMode::Ladder], true),
            (120.0, vec![FootprintMode::Ladder], false),
            (40.0, vec![FootprintMode::Ladder], true),
            (
                240.0,
                vec![FootprintMode::Ladder, FootprintMode::Volume],
                true,
            ),
        ] {
            let config = FootprintConfig {
                modes,
                auto_row_height: None,
                show_imbalance,
                ..Default::default()
            };
            let (shapes, center) = paint_ladder(&rows, spacing, false, &config);
            let [left, right] = [center - spacing * 0.46, center + spacing * 0.46 - 0.5];
            let slot = (center - spacing * 0.5)..=(center + spacing * 0.5);
            let mut strips = [0, 0];
            for shape in &shapes {
                let Shape::Mesh(mesh) = shape else { continue };
                for quad in mesh.vertices.chunks(4) {
                    let rect = Rect::from_two_pos(quad[0].pos, quad[3].pos);
                    let side = [config.bid_color, config.ask_color]
                        .iter()
                        .position(|color| *color == quad[0].color);
                    if let Some(side) = side.filter(|_| rect.width() <= 3.01) {
                        strips[side] += 1;
                        assert!(
                            if side == 0 {
                                rect.right() <= left && slot.contains(&rect.left())
                            } else {
                                rect.left() >= right && slot.contains(&rect.right())
                            },
                            "strip {rect:?} at spacing {spacing}"
                        );
                    }
                }
            }
            assert_eq!(
                strips,
                [usize::from(show_imbalance); 2],
                "strips at spacing {spacing}"
            );
            for shape in &shapes {
                if let Shape::Path(path) = shape {
                    assert!(
                        path.points.iter().all(|p| p.x >= left - spacing * 0.08),
                        "marker {:?} at spacing {spacing}",
                        path.points
                    );
                }
            }
        }
    }

    /// Both sides of a ladder keep their numbers or lose them together, and the summary below the
    /// bar leaves room for the extreme ratio written in the same place. Where the numbers fit but
    /// the labels of the summary do not, its delta and total stay as values alone.
    #[test]
    fn ladder_numbers_and_summary_do_not_vanish_alone_or_collide() {
        let config = FootprintConfig {
            auto_row_height: None,
            ..Default::default()
        };
        // A buy cell of 22 points cannot hold 123.5K even at 8 points; the sell number 1 fits.
        let one_sided = [(103.0, 1.0, 123_456.0), (102.0, 1.0, 123_456.0)];
        let (shapes, _) = paint_ladder(&one_sided, 70.0, false, &config);
        let numbers = painted_text(&shapes);
        assert!(numbers.is_empty(), "one side kept its numbers: {numbers:?}");
        // Totals of 543.5K and 370.1K are wide enough to reach the centred ratio below the bar.
        let rows = [(106.0, 223_400.0, 250_100.0), (105.0, 320_100.0, 120_000.0)];
        // On an inverted scale the lowest price is on top, and the rows end at 106.5, y 113.33.
        let (shapes, _) = paint_ladder(&rows, 120.0, true, &config);
        let text = painted_text(&shapes);
        for total in ["543.5K", "370.1K"] {
            let (_, rect) = text
                .iter()
                .find(|(label, _)| label == total)
                .unwrap_or_else(|| panic!("{total} missing from {text:?}"));
            assert!(rect.top() >= 113.0, "{total} over the rows at {rect:?}");
        }
        let ratio = FootprintConfig {
            extreme_ratio: true,
            ..config.clone()
        };
        let (shapes, _) = paint_ladder(&rows, 120.0, false, &ratio);
        let text = painted_text(&shapes);
        let below = text
            .iter()
            .filter(|(label, _)| label.starts_with("A/B"))
            .max_by(|a, b| a.1.top().total_cmp(&b.1.top()))
            .expect("the lower extreme ratio is written")
            .1;
        for total in ["543.5K", "370.1K", "Delta", "Total"] {
            let (_, rect) = text
                .iter()
                .find(|(label, _)| label == total)
                .unwrap_or_else(|| panic!("{total} missing from {text:?}"));
            assert!(!rect.intersects(below), "{total} over the extreme ratio");
        }
        // At 80 points per bar the labelled summary needs 80 points in a block of 73.1.
        let rows = [(106.0, 2_340.0, 2_510.0), (105.0, 3_201.0, 1_200.0)];
        let (shapes, _) = paint_ladder(&rows, 80.0, false, &config);
        let text = painted_text(&shapes);
        let at = |wanted: &str| {
            text.iter()
                .find(|(label, _)| label == wanted)
                .unwrap_or_else(|| panic!("{wanted} missing from {text:?}"))
                .1
        };
        // Totals of 5,541 and 3,710, written compactly in their 28.5 point cells.
        for total in ["5.5K", "3.7K"] {
            for value in ["-1.83K", "9.25K"] {
                assert!(
                    at(value).top() >= at(total).bottom(),
                    "{value} over {total}"
                );
            }
        }
        // A side cell needs a point: 3.3 points per bar leave it 0.99, 3.4 leave it 1.02.
        assert!(paint_ladder(&rows, 3.3, false, &config).0.is_empty());
        assert!(!paint_ladder(&rows, 3.4, false, &config).0.is_empty());
    }

    /// Rows cover the nearest-tick band; spacing that leaves cells under a point wide produces no
    /// geometry.
    #[test]
    fn rendered_execution_rows_surround_their_instrument_ticks() {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        // y(P) = (high - P) / (high - 99) * 160, calculated independently.
        #[rustfmt::skip]
        let cases = [
            (1.63, 1.0, 1, 0, None, 100.0, 102.0, None),
            (1.64, 1.0, 1, 0, None, 100.0, 102.0, Some((80.0, 133.33333))),
            (100.0, 1.0, 1, 0, None, 100.0, 102.0, Some((80.0, 133.33333))),
            (100.0, 0.25, 1, 0, None, 100.25, 102.0, Some((86.66667, 100.0))),
            (100.0, 1.0, 4, 0, None, 100.0, 106.0, Some((57.142857, 148.57143))),
            (100.0, 1.0, 1, 0, Some(75.0), 100.0, 106.0, Some((57.142857, 148.57143))),
            (100.0, 1.0, 1, 2, None, 100.0, 106.0, Some((102.85714, 148.57143))),
            (100.0, 0.1, 1, 3, None, 100.0, 102.0, Some((98.66667, 114.66667))),
            (100.0, 0.1, 3, 3, None, 100.0, 102.0, Some((98.66667, 114.66667))),
        ];
        for (spacing, tick, ticks, manual, auto, price, high, extent) in cases {
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
                row_ticks: manual,
                show_poc: false,
                value_area: None,
                ..bid_ask_config()
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
        for modes in [
            vec![
                FootprintMode::BidAsk,
                FootprintMode::Volume,
                FootprintMode::Delta,
                FootprintMode::DeltaVolume,
            ],
            vec![
                FootprintMode::BidAskTrades,
                FootprintMode::Trades,
                FootprintMode::AverageSize,
                FootprintMode::Volume,
            ],
        ] {
            for (minimum, auto, ticks, high, left, expected) in [
                (14.0, None, 0, 102.0, 0.0, 0),
                (10.0, None, 0, 102.0, 0.0, 8),
                (10.0, Some(30.0), 0, 102.0, 0.0, 4),
                (10.0, None, 2, 102.0, 0.0, 4),
                (10.0, None, 0, 102.8, 0.0, 8),
                (10.0, None, 0, 102.0, 30.0, 6),
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
                    modes: modes.clone(),
                    row_ticks: ticks,
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
                if auto.is_some() || ticks > 0 {
                    let expected = if modes[0] == FootprintMode::BidAsk {
                        ["2 × 7", "9", "+5", "+5 / 9"]
                    } else {
                        ["1 × 2", "3", "3", "9"]
                    };
                    assert_eq!(texts, expected);
                }
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
        for (minimum, bid, ask) in [(0.0, true, true), (10.0, false, true), (13.0, false, false)] {
            let config = FootprintConfig {
                imbalance_min_volume: minimum,
                ..config.clone()
            };
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |ui| render_footprint(ui.painter(), &map, &bars, &dual, &config),
            );
            output.textures_delta.clear();
            for (color, expected) in [(config.bid_color, bid), (config.ask_color, ask)] {
                assert_eq!(
                    output.shapes.iter().any(|shape| match &shape.shape {
                        Shape::Mesh(mesh) => mesh
                            .vertices
                            .iter()
                            .any(|v| v.color == color.gamma_multiply(0.5)),
                        _ => false,
                    }),
                    expected,
                    "each diagonal side must reach minimum {minimum}"
                );
            }
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

    /// The same minimum gates grouped cells and zones, and changing it refreshes cached zones.
    #[cfg(feature = "studies")]
    #[test]
    fn imbalance_minimum_applies_after_grouping_to_cells_and_zones() {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        let bars = [Bar::new(time, 100.0, 107.0, 100.0, 107.0, 39.0)];
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(240.0, 160.0));
        let map = ChartMapping::new(
            rect,
            120.0,
            0,
            0,
            0.0,
            PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(98.0, 110.0),
                0.0,
                160.0,
                false,
            ),
        );
        for ticks in [1, 2] {
            let ctx = egui::Context::default();
            let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, ticks).unwrap();
            // Each raw row has less than 10 on either side; pairs sum to asks 12, 9, 15
            // against bids of 1 one grouped row below.
            for (price, bid, ask) in [
                (100.0, 1.0, 0.0),
                (102.0, 1.0, 12.0),
                (104.0, 1.0, 9.0),
                (106.0, 0.0, 15.0),
            ] {
                for offset in [0.0, 1.0] {
                    for (size, side) in [(bid / 2.0, OrderSide::Sell), (ask / 2.0, OrderSide::Buy)]
                    {
                        flow.push(&Tick::new(time, price + offset, size, Some(side)))
                            .unwrap();
                    }
                }
            }
            for (minimum, middle, zone, own) in [
                (0.0, true, true, None),
                (9.0, true, true, None),
                (10.0, false, false, None),
                (0.0, true, true, None),
                // An imbalance colour of its own replaces the buy colour in cells and zones.
                (0.0, true, true, Some(Color32::from_rgb(255, 0, 255))),
            ] {
                let config = FootprintConfig {
                    color_by: FootprintColor::Imbalance,
                    profile: false,
                    value_area: None,
                    show_poc: false,
                    auto_row_height: (ticks == 1).then_some(20.0),
                    imbalance_min_volume: minimum,
                    imbalance_ask_color: own,
                    studies: Some(crate::studies::FootprintStudyConfig {
                        naked_poc: false,
                        delta_poc: false,
                        unfinished_auctions: false,
                        ..Default::default()
                    }),
                    ..Default::default()
                };
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(rect),
                        ..Default::default()
                    },
                    |ui| {
                        render_footprint(ui.painter(), &map, &bars, &flow, &config);
                        render_footprint_annotations(
                            ui.painter(),
                            &map,
                            &bars,
                            &flow,
                            &config,
                            egui::Id::unique("minimum"),
                        );
                    },
                );
                output.textures_delta.clear();
                let buy = own.unwrap_or(config.ask_color);
                for (price, active) in [(102.0, true), (104.0, middle), (106.0, true)] {
                    let top = map.price_to_y(price + 1.5);
                    let color = if active { buy } else { config.neutral_color }.gamma_multiply(0.5);
                    let filled = output.shapes.iter().any(|s| match &s.shape {
                        Shape::Mesh(mesh) => mesh
                            .vertices
                            .iter()
                            .any(|v| (v.pos.y - top).abs() < 0.01 && v.color == color),
                        _ => false,
                    });
                    assert!(filled, "ticks {ticks}, minimum {minimum}, price {price}");
                }
                let has_zone = output
                    .shapes
                    .iter()
                    .any(|s| matches!(&s.shape, Shape::Rect(r) if r.stroke.color == buy));
                assert_eq!(has_zone, zone, "ticks {ticks}, minimum {minimum}");
            }
        }
    }

    /// Shared row labels and complete numeric columns survive narrow detail widths; a short pane
    /// reports unavailable detail instead of clipping a row. The first bar has 13 buys and 3 sells:
    /// delta 10, 62.5%, ask/bid 4.33:1, three executions averaging 16/3; the second adds 2500 sells.
    ///
    /// The tallest pane holds every row. Of the first bar's volume of 16, 3 sold are 18.75% and
    /// 13 bought 81.25%, written to the even digit as 18.8% and 81.2%; it ranges from 100.00 to
    /// 100.25, and has no bar to change from. Rows of 0.25 are 11.3 points tall here, so the
    /// footprint draws them in pairs and both bars have their point of control in the row at
    /// 100.00, where the busiest price of the first is 100.25. The second bar trades 2500 after
    /// 16, a change of 2484 / 16 = 15525%, all of it sold, at one price.
    #[test]
    fn statistics_keep_shared_rows_and_aligned_values_or_explain_the_overview() {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        let next = time + chrono::TimeDelta::minutes(1);
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 0.25, 1).unwrap();
        for (at, price, size, side) in [
            (time, 100.0, 3.0, OrderSide::Sell),
            (time, 100.25, 5.0, OrderSide::Buy),
            (time, 100.25, 8.0, OrderSide::Buy),
            (next, 100.0, 2500.0, OrderSide::Sell),
        ] {
            flow.push(&Tick::new(at, price, size, Some(side))).unwrap();
        }
        let bars = [
            Bar::new(time, 100.0, 100.25, 100.0, 100.25, 16.0),
            Bar::new(next, 100.0, 100.0, 100.0, 100.0, 2500.0),
        ];
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(240.0, 136.0));
        let empty = Footprint::new(BarClock::new(Timeframe::Min1), 0.25, 1).unwrap();
        for (spacing, available, large, height) in [
            (60.0, true, false, 136.0),
            (40.0, true, false, 136.0),
            (3.0, true, false, 136.0),
            (60.0, false, false, 136.0),
            (40.0, true, true, 136.0),
            (60.0, true, false, 90.0),
            (60.0, true, false, 112.0),
            (60.0, true, false, 196.0),
        ] {
            // The default renderer keeps its nine rows, which fit the panes hosts already reserve.
            let count = if height == 112.0 {
                9
            } else if height == 196.0 {
                16
            } else {
                11
            };
            let rect = Rect::from_min_size(rect.min, vec2(rect.width(), height));
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
                    136.0,
                    false,
                ),
            );
            let mut output = egui::Context::default().run_ui(
                egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |ui| {
                    let flow = if available { &shown } else { &empty };
                    if count == 9 {
                        render_footprint_statistics(ui.painter(), &map, &bars, flow, Color32::WHITE)
                    } else {
                        let config = FootprintConfig {
                            text_color: Color32::WHITE,
                            statistics_rows: FootprintStatistic::ALL[..count].to_vec(),
                            ..Default::default()
                        };
                        render_footprint_statistics_rows(ui.painter(), &map, &bars, flow, &config)
                    }
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
            for text in &texts {
                assert!(
                    rect.contains_rect(Rect::from_min_size(text.pos, text.galley.size())),
                    "statistics text is clipped at height {height}: {} {:?}",
                    text.galley.text(),
                    Rect::from_min_size(text.pos, text.galley.size()),
                );
            }
            if spacing == 3.0 || !available || height == 90.0 {
                assert_eq!(texts.len(), 1);
                assert_eq!(
                    texts[0].galley.text(),
                    if !available {
                        "Execution statistics unavailable"
                    } else if height == 90.0 {
                        "Enlarge chart for per-bar statistics"
                    } else {
                        "Zoom in for per-bar statistics"
                    }
                );
                continue;
            }
            assert_eq!(
                texts.len(),
                count * 3,
                "{count} labels and two complete columns at {spacing}"
            );
            assert_eq!(
                texts
                    .iter()
                    .filter(|t| t.galley.text() == "Delta %")
                    .count(),
                1
            );
            for (column, expected) in [
                [
                    "16", "3", "13", "10", "-3", "10", "10", "62.5%", "4.33:1", "3", "5.333",
                    "18.8%", "81.2%", "0.25", "100.00", "",
                ],
                if large {
                    [
                        "123.5M", "123.5M", "0", "-123.5M", "-123.5M", "0", "-123.5M", "-100.0%",
                        "0.00:1", "2", "61.7M", "", "", "", "", "",
                    ]
                } else {
                    [
                        "2500", "2500", "0", "-2500", "-2500", "0", "-2490", "-100.0%", "0.00:1",
                        "1", "2500", "100.0%", "0.0%", "0.00", "100.00", "15525.0%",
                    ]
                },
            ]
            .into_iter()
            .enumerate()
            {
                for (line, expected) in expected.into_iter().take(count).enumerate() {
                    let text = texts[count + column * count + line];
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
                    let config = bid_ask_config();
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
                    ..bid_ask_config()
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
            ..Default::default()
        };
        let config = |abbreviate, show_zero| FootprintConfig {
            abbreviate,
            show_zero,
            ..Default::default()
        };
        let counted = FootprintLevel {
            bid: 4.0,
            ask: 8.0,
            unknown: 8.0,
            bid_trades: 1,
            ask_trades: 2,
            unknown_trades: 1,
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
            (FootprintMode::BidAskTrades, counted.clone(), config(false, true), "1 × 2"),
            (FootprintMode::Trades, counted.clone(), config(false, true), "4"),
            (FootprintMode::AverageSize, counted, config(false, true), "5"),
            (FootprintMode::BidAskTrades, row(0.0, 0.0, 0.0), config(false, false), ""),
            (FootprintMode::BidAskTrades, row(0.0, 0.0, 0.0), config(false, true), "0 × 0"),
            (FootprintMode::Trades, row(0.0, 0.0, 0.0), config(false, true), "0"),
            (FootprintMode::AverageSize, row(0.0, 0.0, 0.0), config(false, true), ""),
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
        let config = bid_ask_config();
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
        // At 120 a default ladder bar has room for its summary below, and captions avoid that too.
        let ladder = FootprintConfig::default();
        for (spacing, height, config) in [
            (120.0, 260.0, &ladder),
            (60.0, 160.0, &config),
            (3.0, 160.0, &config),
        ] {
            let map = ChartMapping {
                bar_spacing: spacing,
                base_idx: 1,
                rect: rect.with_max_y(height),
                ..map.clone()
            };
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |ui| {
                    render_footprint(ui.painter(), &map, &bars, &flow, config);
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
                        config,
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

    /// One bar of (price, sell, buy) rows at `spacing` points per bar over 98 to 110 on a plot
    /// 480 by 320 points, painted with `config` under `theme`.
    fn paint_themed(
        rows: &[(f64, f64, f64)],
        spacing: f32,
        theme: &crate::theme::Theme,
        config: &FootprintConfig,
    ) -> (Vec<egui::epaint::ClippedShape>, ChartMapping) {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        // Open and close match so the candle takes the text colour, not a side's.
        let bars = [Bar::new(time, 102.0, 104.0, 100.0, 102.0, 1.0)];
        let rect = Rect::from_min_size(pos2(0.0, 0.0), vec2(480.0, 320.0));
        let map = ChartMapping::new(
            rect,
            spacing,
            0,
            0,
            0.0,
            PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(98.0, 110.0),
                0.0,
                320.0,
                false,
            ),
        );
        let mut flow = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        for &(price, sell, buy) in rows {
            for (size, side) in [(sell, OrderSide::Sell), (buy, OrderSide::Buy)] {
                flow.push(&Tick::new(time, price, size, Some(side)))
                    .unwrap();
            }
        }
        let ctx = egui::Context::default();
        crate::theme::apply_to_egui(&ctx, theme);
        let mut output = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(rect),
                ..Default::default()
            },
            |ui| render_footprint(ui.painter(), &map, &bars, &flow, config),
        );
        output.textures_delta.clear();
        (output.shapes, map)
    }

    /// Every quad of the meshes in `shapes`, in paint order.
    fn quads(shapes: &[egui::epaint::ClippedShape]) -> Vec<(Rect, Color32)> {
        shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::Mesh(mesh) => Some(
                    mesh.vertices
                        .chunks(4)
                        .map(|q| (Rect::from_two_pos(q[0].pos, q[3].pos), q[0].color))
                        .collect::<Vec<_>>(),
                ),
                _ => None,
            })
            .flatten()
            .collect()
    }

    /// The colour painted last at `point`.
    fn color_at(quads: &[(Rect, Color32)], point: egui::Pos2) -> Option<Color32> {
        quads
            .iter()
            .rev()
            .find(|(rect, _)| rect.contains(point))
            .map(|(_, color)| *color)
    }

    /// An imbalance strip has to stand apart from the cell it marks. On a light chart the second
    /// shade is the side colour itself, and below 100 points per bar the strip is less than a
    /// point clear of its cell. Rows (sell, buy) 104: 10, 10; 103: 20, 40; 102: 10, 90 (POC);
    /// 101: 60, 20; 100: 30, 10. The buy range is 10 to 90, so 40 is in its second quarter, and
    /// 40 bought at 103 is four times the 10 sold one row below, the bar's only imbalance.
    #[test]
    fn light_imbalance_strip_stands_apart_from_its_cell() {
        let rows = [
            (104.0, 10.0, 10.0),
            (103.0, 20.0, 40.0),
            (102.0, 10.0, 90.0),
            (101.0, 60.0, 20.0),
            (100.0, 30.0, 10.0),
        ];
        let config = FootprintConfig {
            auto_row_height: None,
            text_color: Color32::BLACK,
            ..Default::default()
        };
        let mut marks = Vec::new();
        for spacing in [60.0, 90.0, 120.0] {
            let (shapes, map) =
                paint_themed(&rows, spacing, &crate::theme::Theme::light(), &config);
            let quads = quads(&shapes);
            let y = map.price_to_y(103.0);
            // The buy cell ends at the outer edge of the bar's only column.
            let edge = map.idx_to_x(0) - spacing * 0.46 + spacing * 0.92 - 0.5;
            let cell = color_at(&quads, pos2(edge - 1.0, y));
            let strip = quads.iter().find(|(rect, _)| {
                rect.width() <= 3.01 && rect.left() >= edge - 0.01 && rect.y_range().contains(y)
            });
            marks.push((
                spacing,
                strip.map(|(rect, _)| rect.left() - edge),
                strip.map(|s| s.1),
                cell,
            ));
        }
        assert!(
            marks.iter().all(|&(_, gap, strip, cell)| {
                gap.is_some_and(|gap| gap >= 0.99) || (strip.is_some() && strip != cell)
            }),
            "(spacing, strip gap from its cell, strip colour, cell colour): {marks:?}"
        );
    }

    /// A buy imbalance colour of its own paints the strip and leaves the cell it marks in the
    /// buy colour's shade. Rows as above: 40 bought at 103 is the bar's only imbalance.
    #[test]
    fn imbalance_strip_takes_its_own_colour() {
        let rows = [
            (104.0, 10.0, 10.0),
            (103.0, 20.0, 40.0),
            (102.0, 10.0, 90.0),
            (101.0, 60.0, 20.0),
            (100.0, 30.0, 10.0),
        ];
        let own = Color32::from_rgb(255, 0, 255);
        let config = FootprintConfig {
            auto_row_height: None,
            text_color: Color32::BLACK,
            imbalance_ask_color: Some(own),
            ..Default::default()
        };
        let (shapes, map) = paint_themed(&rows, 120.0, &crate::theme::Theme::light(), &config);
        let quads = quads(&shapes);
        let y = map.price_to_y(103.0);
        // The buy cell ends at the outer edge of the bar's only column.
        let edge = map.idx_to_x(0) - 120.0 * 0.46 + 120.0 * 0.92 - 0.5;
        let strip = quads.iter().find(|(rect, _)| {
            rect.width() <= 3.01 && rect.left() >= edge - 0.01 && rect.y_range().contains(y)
        });
        assert_eq!(strip.map(|strip| strip.1), Some(own));
        assert_ne!(color_at(&quads, pos2(edge - 1.0, y)), Some(own));
    }

    /// Under the volume step colouring a sell × buy cell is split into a sell half and a buy half.
    /// On a light chart a half in the weakest quarter needs black text and a half above it white.
    /// Rows (sell, buy) 104: 10, 50; 103: 10, 20; 102: 50, 50 (POC); 101: 20, 10; 100: 30, 10. Both
    /// ranges are 10 to 50, so every row but the POC has one half in the weakest quarter and one
    /// above it. White on the weakest sell shade (249, 155, 162) is 2.05:1, on the weakest buy
    /// shade (132, 204, 192) 1.85:1; black on them is 10.2:1 and 11.4:1.
    #[test]
    fn light_sell_buy_numbers_read_on_both_halves() {
        let rows = [
            (104.0, 10.0, 50.0),
            (103.0, 10.0, 20.0),
            (102.0, 50.0, 50.0),
            (101.0, 20.0, 10.0),
            (100.0, 30.0, 10.0),
        ];
        let config = FootprintConfig {
            modes: vec![FootprintMode::BidAsk],
            auto_row_height: None,
            text_color: Color32::BLACK,
            ..Default::default()
        };
        let (shapes, _) = paint_themed(&rows, 120.0, &crate::theme::Theme::light(), &config);
        let quads = quads(&shapes);
        let mut numbers = std::collections::BTreeSet::new();
        let mut ends = 0;
        let mut unreadable = Vec::new();
        for clipped in &shapes {
            let Shape::Text(text) = &clipped.shape else {
                continue;
            };
            if !text.galley.text().contains('×') {
                continue;
            }
            // A number painted in two halves is still one number, at one position.
            numbers.insert((text.pos.x.to_bits(), text.pos.y.to_bits()));
            let ink = text.override_text_color.unwrap_or(config.text_color);
            let y = text.pos.y + text.galley.size().y * 0.5;
            for x in [text.pos.x + 2.0, text.pos.x + text.galley.size().x - 2.0] {
                if !clipped.clip_rect.contains(pos2(x, y)) {
                    continue;
                }
                ends += 1;
                let fill = color_at(&quads, pos2(x, y)).expect("a cell under the number");
                let ratio = crate::theme::contrast(ink, fill);
                if ratio < 3.0 {
                    unreadable.push((text.galley.text().to_owned(), ink, fill, ratio));
                }
            }
        }
        assert_eq!(numbers.len(), 5, "every row writes its numbers");
        assert_eq!(ends, 10, "both ends of every number are painted");
        assert!(
            unreadable.is_empty(),
            "numbers under 3:1 on their half: {unreadable:?}"
        );
    }
}
