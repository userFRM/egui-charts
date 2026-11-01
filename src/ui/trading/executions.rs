//! The account's fills on the chart: one mark per side and bar, up to three filled triangles
//! stacked outward from the bar, under its low for buys and over its high for sells.

use std::sync::Arc;

use crate::localization as l10n;
use egui::{FontId, Galley, Rect, Sense, Shape, Stroke, Ui, pos2};

use crate::model::{Execution, OrderSide};
use crate::theme::ThemeExt;
use crate::widget::Chart;

/// The most triangles one mark draws; a "+N" line counts the rest.
const MAX_STACK: usize = 3;
/// The most rows crowded labels stack in; a label with no room left is left out.
const MAX_LABEL_ROWS: usize = 3;
/// The most fills a mark's hover lists before "and N more".
const MAX_HOVER_FILLS: usize = 20;
/// Half the width of a triangle's base.
const HALF_WIDTH: f32 = 7.0;
/// The height of a triangle, from its tip to its base.
const HEIGHT: f32 = 17.0;
/// The space between stacked triangles.
const STEP: f32 = 2.0;
/// The space between a bar and its marks, and between a mark and its text.
const GAP: f32 = 4.0;

use super::account_panel::quantity_decimals;
use super::format::Numbers;
use super::widgets::{side_color, side_name};

/// Draws `fills` on the price pane of `chart`. The fills of one side in one bar form one mark: up
/// to [`MAX_STACK`] triangles stacked outward, pointing up in the buy colour just under the bar's
/// low, or down in the sell colour just over its high, and beyond them a "+N" line for the fills
/// not drawn. With `labels`, a line beyond the stack gives the side and total quantity. Text that
/// would overlap newer text moves outward, up to [`MAX_LABEL_ROWS`] rows, or is left out. Hovering a
/// mark lists its fills with their side, quantity, price and time, up to [`MAX_HOVER_FILLS`].
pub(super) fn show(
    ui: &mut Ui,
    chart: &Chart,
    fills: &[&Execution],
    labels: bool,
    numbers: Numbers<'_>,
) {
    // A bar spans from its time to the next bar's, so a fill belongs to the bar at or before it.
    let bars = chart.state.shown();
    let mut placed: Vec<(f64, &Execution)> = fills
        .iter()
        .filter_map(|fill| {
            let index = if let Some(cutoff) = chart.replay_time() {
                if fill.time >= cutoff {
                    return None;
                }
                bars.partition_point(|bar| bar.time <= fill.time)
                    .checked_sub(1)
                    .map(|index| index as f64)
            } else {
                chart.time_to_index(fill.time).map(f64::floor)
            };
            Some((index?, *fill))
        })
        .collect();
    // Stable, so that a mark keeps its fills in the list's order.
    let is_sell = |fill: &Execution| matches!(fill.side, OrderSide::Sell);
    placed.sort_by(|a, b| {
        a.0.total_cmp(&b.0)
            .then_with(|| is_sell(a.1).cmp(&is_sell(b.1)))
    });

    let mapping = chart.get_chart_mapping();
    let theme = ui.ctx().active_theme();
    let painter = ui.painter().with_clip_rect(mapping.rect);
    let (mut text_rects, mut texts): (Vec<Rect>, Vec<(Rect, Arc<Galley>)>) = Default::default();
    // Newest first, so that where labels crowd each other the newer ones keep their text.
    for run in placed
        .chunk_by(|a, b| a.0 == b.0 && a.1.side == b.1.side)
        .rev()
    {
        let (index, side) = (run[0].0, run[0].1.side);
        let Some(x) = chart.index_to_x(index) else {
            continue;
        };
        if x < mapping.rect.left() - HALF_WIDTH || x > mapping.rect.right() + HALF_WIDTH {
            continue;
        }
        let quantity: f64 = run.iter().map(|(_, fill)| fill.quantity).sum();
        let price = run
            .iter()
            .map(|(_, fill)| fill.price * fill.quantity)
            .sum::<f64>()
            / quantity;
        // A fill timed after the last bar waits at its price for its bar.
        let (low, high) = usize::try_from(index as i64)
            .ok()
            .and_then(|index| bars.get(index))
            .map_or((price, price), |bar| (bar.low, bar.high));
        let (low_y, high_y) = (mapping.price_to_y(low), mapping.price_to_y(high));
        let color = side_color(&theme, side);
        let (down, edge) = match side {
            OrderSide::Buy => (1.0, low_y.max(high_y) + GAP),
            OrderSide::Sell => (-1.0, low_y.min(high_y) - GAP),
        };
        if !edge.is_finite() {
            continue;
        }
        let at = |dx: f32, dy: f32| pos2(x + dx, edge + down * dy);
        let drawn = run.len().min(MAX_STACK);
        for triangle in 0..drawn {
            let dy = triangle as f32 * (HEIGHT + STEP);
            painter.add(Shape::convex_polygon(
                vec![
                    at(0.0, dy),
                    at(HALF_WIDTH, dy + HEIGHT),
                    at(-HALF_WIDTH, dy + HEIGHT),
                ],
                color,
                Stroke::NONE,
            ));
        }
        let length = drawn as f32 * HEIGHT + (drawn - 1) as f32 * STEP;
        let mut rect = Rect::from_two_pos(at(-HALF_WIDTH, 0.0), at(HALF_WIDTH, length));
        let more = (run.len() > MAX_STACK).then(|| format!("+{}", run.len() - MAX_STACK));
        let label = labels.then(|| {
            l10n::format_text(
                ui.ctx(),
                match side {
                    OrderSide::Buy => "Buy {quantity}",
                    OrderSide::Sell => "Sell {quantity}",
                },
                &[(
                    "quantity",
                    numbers
                        .quantity(quantity, quantity_decimals(quantity))
                        .as_str(),
                )],
            )
        });
        let lines: Vec<_> = more
            .into_iter()
            .chain(label)
            .map(|line| {
                painter.layout_no_wrap(
                    line,
                    FontId::proportional(ui.active_theme().text.label),
                    chart.config().text_color,
                )
            })
            .collect();
        if !lines.is_empty() {
            let height: f32 = lines.iter().map(|line| line.size().y).sum::<f32>()
                + (lines.len() - 1) as f32 * GAP / 2.0;
            let width = lines.iter().map(|line| line.size().x).fold(0.0, f32::max);
            let size = egui::vec2(width, height);
            let top = if down > 0.0 {
                rect.bottom() + GAP
            } else {
                rect.top() - GAP - size.y
            };
            let block = (0..MAX_LABEL_ROWS)
                .map(|row| {
                    Rect::from_min_size(pos2(x - size.x / 2.0, top), size)
                        .translate(egui::vec2(0.0, down * row as f32 * (size.y + GAP)))
                })
                .find(|block| {
                    // The nearest labels, the likeliest to overlap, were placed last.
                    !text_rects
                        .iter()
                        .rev()
                        .any(|other| other.expand(GAP / 2.0).intersects(*block))
                });
            // Without room among the newer labels the mark keeps its triangles and hover only.
            if let Some(block) = block {
                text_rects.push(block);
                let mut y = if down > 0.0 {
                    block.top()
                } else {
                    block.bottom()
                };
                for line in lines {
                    let size = line.size();
                    let top = if down > 0.0 { y } else { y - size.y };
                    texts.push((Rect::from_min_size(pos2(x - size.x / 2.0, top), size), line));
                    y += down * (size.y + GAP / 2.0);
                }
                rect = rect.union(block);
            }
        }
        let hover = rect.expand(2.0).intersect(mapping.rect);
        if hover.is_positive() {
            ui.interact(
                hover,
                ui.scope_id()
                    .with(("execution", side_name(side), index.to_bits())),
                Sense::hover(),
            )
            // Written only while it shows: a mark can hold every fill of the list.
            .on_hover_ui(|ui| {
                let time = chart.build_time_formatter();
                let decimals = numbers.decimals;
                let mut lines: Vec<_> = run
                    .iter()
                    .take(MAX_HOVER_FILLS)
                    .map(|(_, fill)| {
                        l10n::format_text(
                            ui.ctx(),
                            "{side} {quantity} at {price}, {time}",
                            &[
                                ("side", l10n::text(ui.ctx(), side_name(fill.side)).as_str()),
                                ("quantity", fill.quantity.to_string().as_str()),
                                ("price", format!("{:.decimals$}", fill.price).as_str()),
                                // With seconds, also on charts of daily bars.
                                ("time", time.format_crosshair(fill.time, 1).as_str()),
                            ],
                        )
                    })
                    .collect();
                let more = run.len().saturating_sub(MAX_HOVER_FILLS);
                if more > 0 {
                    lines.push(l10n::format_text(
                        ui.ctx(),
                        "and {0} more",
                        &[("0", more.to_string())],
                    ));
                }
                ui.set_max_width(ui.spacing().tooltip_width);
                ui.label(l10n::text(ui.ctx(), lines.join("\n")));
            });
        }
    }
    // Over every triangle, so that a neighbouring stack never hides a label.
    for (rect, line) in texts {
        // The chart's background keeps the text readable over the bars beside its own.
        painter.rect_filled(rect.expand(1.0), 2.0, theme.chart.bg);
        painter.galley(rect.min, line, chart.config().text_color);
    }
}
