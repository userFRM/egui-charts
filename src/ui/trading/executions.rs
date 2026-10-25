//! The account's fills on the chart: one mark per side and bar, an arrow under the bar's low for
//! buys and over its high for sells, with a chevron for each fill.

use egui::{FontId, Rect, Sense, Shape, Stroke, Ui, pos2};

use crate::model::{Execution, OrderSide};
use crate::styles::typography;
use crate::theme::ThemeExt;
use crate::widget::Chart;

/// The most chevrons one mark draws; a mark of more fills still lists them all on hover.
const MAX_CHEVRONS: usize = 10;
/// Half the width of a chevron.
const HALF_WIDTH: f32 = 5.0;
/// The height of a chevron.
const CHEVRON: f32 = 5.0;
/// The distance between stacked chevrons.
const STEP: f32 = 4.0;
/// The length of a mark of one fill, from its tip to the end of its shaft.
const LENGTH: f32 = 16.0;
/// The space between a bar and its marks, and between a mark and its label.
const GAP: f32 = 4.0;

/// "Buy" or "Sell".
pub(super) fn side_name(side: OrderSide) -> &'static str {
    match side {
        OrderSide::Buy => "Buy",
        OrderSide::Sell => "Sell",
    }
}

/// Draws `fills` on the price pane of `chart`. The fills of one side in one bar form one mark: an
/// arrow pointing up in the buy colour just under the bar's low, or down in the sell colour just
/// over its high, with one chevron per fill stacked along its shaft, up to [`MAX_CHEVRONS`]. With
/// `labels`, "<quantity> @ <price>" sits under a buy mark and over a sell mark: the total quantity
/// at the quantity-weighted average price, written with `decimals`. Hovering a mark lists each of
/// its fills with its side, quantity, price and time.
pub(super) fn show(
    ui: &mut Ui,
    chart: &Chart,
    fills: &[&Execution],
    labels: bool,
    decimals: usize,
) {
    // A bar spans from its time to the next bar's, so a fill belongs to the bar at or before it.
    let mut marks: Vec<(f64, OrderSide, Vec<&Execution>)> = Vec::new();
    for fill in fills {
        let Some(index) = chart.time_to_index(fill.time).map(f64::floor) else {
            continue;
        };
        match marks
            .iter_mut()
            .find(|(at, side, _)| *at == index && *side == fill.side)
        {
            Some((_, _, group)) => group.push(fill),
            None => marks.push((index, fill.side, vec![fill])),
        }
    }

    let mapping = chart.get_chart_mapping();
    let theme = ui.ctx().active_theme();
    let painter = ui.painter().with_clip_rect(mapping.rect);
    let bars = chart.state.shown();
    // Coalesce neighbouring runs whose labels would overlap horizontally. Use the newest
    // bar as the anchor and retain every fill for the weighted label and hover details.
    if labels {
        marks.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut i = 0;
        while i < marks.len() {
            let mut j = i + 1;
            while j < marks.len() {
                let width = |group: &[&Execution]| {
                    let quantity: f64 = group.iter().map(|f| f.quantity).sum();
                    let price = group.iter().map(|f| f.price * f.quantity).sum::<f64>() / quantity;
                    painter
                        .layout_no_wrap(
                            format!("{quantity} @ {price:.decimals$}"),
                            FontId::proportional(typography::SM_MD),
                            chart.config().text_color,
                        )
                        .size()
                        .x
                };
                if marks[i].1 == marks[j].1
                    && chart
                        .index_to_x(marks[i].0)
                        .zip(chart.index_to_x(marks[j].0))
                        .is_some_and(|(a, b)| {
                            (b - a).abs() < (width(&marks[i].2) + width(&marks[j].2)) / 2.0 + GAP
                        })
                {
                    let other = marks.remove(j);
                    marks[i].0 = other.0;
                    marks[i].2.extend(other.2);
                    j = i + 1;
                } else {
                    j += 1;
                }
            }
            i += 1;
        }
    }
    let mut label_rects: Vec<Rect> = Vec::new();
    for (index, side, group) in &marks {
        let Some(x) = chart.index_to_x(*index) else {
            continue;
        };
        if x < mapping.rect.left() - HALF_WIDTH || x > mapping.rect.right() + HALF_WIDTH {
            continue;
        }
        let quantity: f64 = group.iter().map(|fill| fill.quantity).sum();
        let price = group
            .iter()
            .map(|fill| fill.price * fill.quantity)
            .sum::<f64>()
            / quantity;
        // A fill timed after the last bar waits at its price for its bar.
        let (low, high) = usize::try_from(*index as i64)
            .ok()
            .and_then(|index| bars.get(index))
            .map_or((price, price), |bar| (bar.low, bar.high));
        let (color, down, edge) = match side {
            OrderSide::Buy => (theme.ui.accent, 1.0, mapping.price_to_y(low) + GAP),
            OrderSide::Sell => (theme.ui.error, -1.0, mapping.price_to_y(high) - GAP),
        };
        if !edge.is_finite() {
            continue;
        }
        let chevrons = group.len().min(MAX_CHEVRONS);
        let length = LENGTH + STEP * (chevrons - 1) as f32;
        let at = |dx: f32, dy: f32| pos2(x + dx, edge + down * dy);
        let stroke = Stroke::new(1.5, color);
        painter.line_segment([at(0.0, 0.0), at(0.0, length)], stroke);
        for chevron in 0..chevrons {
            let dy = STEP * chevron as f32;
            painter.add(Shape::line(
                vec![
                    at(-HALF_WIDTH, dy + CHEVRON),
                    at(0.0, dy),
                    at(HALF_WIDTH, dy + CHEVRON),
                ],
                stroke,
            ));
        }
        let mut rect = Rect::from_two_pos(at(-HALF_WIDTH, 0.0), at(HALF_WIDTH, length));
        if labels {
            let label = painter.layout_no_wrap(
                format!("{quantity} @ {price:.decimals$}"),
                FontId::proportional(typography::SM_MD),
                chart.config().text_color,
            );
            let size = label.size();
            let top = if down > 0.0 {
                rect.bottom() + GAP
            } else {
                rect.top() - GAP - size.y
            };
            let mut label_rect = Rect::from_min_size(pos2(x - size.x / 2.0, top), size);
            while label_rects
                .iter()
                .any(|other| other.expand(GAP / 2.0).intersects(label_rect))
            {
                label_rect = label_rect.translate(egui::vec2(0.0, down * (size.y + GAP)));
            }
            label_rects.push(label_rect);
            // The chart's background keeps the label readable over the bars beside its own.
            painter.rect_filled(label_rect.expand(1.0), 2.0, theme.chart.bg);
            painter.galley(label_rect.min, label, chart.config().text_color);
            rect = rect.union(label_rect);
        }
        let hover = rect.expand(2.0).intersect(mapping.rect);
        if hover.is_positive() {
            let time = chart.build_time_formatter();
            let text = group
                .iter()
                .map(|fill| {
                    format!(
                        "{} {} at {:.decimals$}, {}",
                        side_name(fill.side),
                        fill.quantity,
                        fill.price,
                        // With seconds, also on charts of daily bars.
                        time.format_crosshair(fill.time, 1)
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            ui.interact(
                hover,
                ui.scope_id()
                    .with(("execution", side_name(*side), index.to_bits())),
                Sense::hover(),
            )
            .on_hover_text(text);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::OrderId;

    #[test]
    fn neighbouring_labels_coalesce_into_a_weighted_run() {
        let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
        let bars = chart.bars(crate::chart::selection::SeriesId::MAIN);
        let fills = [
            Execution::new(OrderId::new("a"), bars[94].time, OrderSide::Buy, 90.0, 1.0),
            Execution::new(OrderId::new("b"), bars[95].time, OrderSide::Buy, 100.0, 2.0),
            Execution::new(
                OrderId::new("c"),
                bars[95].time,
                OrderSide::Sell,
                105.0,
                3.0,
            ),
        ];
        let ctx = egui::Context::default();
        for _ in 0..2 {
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                chart.show(ui);
                show(ui, &chart, &fills.iter().collect::<Vec<_>>(), true, 2);
            });
            let labels: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    Shape::Text(text) if text.galley.text().contains(" @ ") => Some((
                        text.galley.text(),
                        text.galley.rect.translate(text.pos.to_vec2()),
                    )),
                    _ => None,
                })
                .collect();
            assert_eq!(
                labels.iter().map(|(text, _)| *text).collect::<Vec<_>>(),
                ["3 @ 96.67", "3 @ 105.00"]
            );
            assert!(!labels[0].1.intersects(labels[1].1));
            output.drop_without_applying_deltas();
        }
    }
}
