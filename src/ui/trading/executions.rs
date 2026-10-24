//! The account's fills on the chart: an arrow on the bar of each fill, its tip on the fill's
//! price.

use egui::{Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui, pos2, vec2};

use crate::model::{Execution, OrderSide};
use crate::theme::ThemeExt;
use crate::widget::Chart;

/// Half the width of an arrow's head.
const HALF_WIDTH: f32 = 5.0;
/// The height of an arrow's head.
const HEAD: f32 = 6.0;
/// The height of an arrow's stem.
const STEM: f32 = 6.0;
/// Half the width of an arrow's stem.
const HALF_STEM: f32 = 1.5;
/// The space between an arrow and its label, and between stacked arrows.
const GAP: f32 = 3.0;

/// "Buy" or "Sell".
pub(super) fn side_name(side: OrderSide) -> &'static str {
    match side {
        OrderSide::Buy => "Buy",
        OrderSide::Sell => "Sell",
    }
}

/// Draws `fills` on the price pane of `chart`: an arrow pointing up in the buy colour or down in
/// the sell colour, on the bar the fill's time falls in, with its tip on the fill's price and,
/// with `labels`, the side and quantity beside it. An arrow that would cover an earlier one moves
/// away from its price, below it for a buy and above it for a sell. Hovering an arrow shows the
/// fill's side, quantity, price and time.
pub(super) fn show(
    ui: &mut Ui,
    chart: &Chart,
    fills: &[&Execution],
    labels: bool,
    decimals: usize,
) {
    let mapping = chart.get_chart_mapping();
    let theme = ui.ctx().active_theme();
    let painter = ui.painter().with_clip_rect(mapping.rect);
    let height = HEAD + STEM;
    let mut placed = Vec::<Rect>::new();
    for (index, fill) in fills.iter().enumerate() {
        // A bar spans from its time to the next bar's, so a fill belongs to the bar at or before it.
        let Some(x) = chart
            .time_to_index(fill.time)
            .and_then(|at| chart.index_to_x(at.floor()))
        else {
            continue;
        };
        let y = mapping.price_to_y(fill.price);
        if !y.is_finite() || x < mapping.rect.left() - HALF_WIDTH || x > mapping.rect.right() {
            continue;
        }
        let (color, down) = match fill.side {
            OrderSide::Buy => (theme.ui.accent, 1.0),
            OrderSide::Sell => (theme.ui.error, -1.0),
        };
        let label = labels.then(|| {
            painter.layout_no_wrap(
                format!("{} {}", side_name(fill.side), fill.quantity),
                FontId::proportional(11.0),
                color,
            )
        });
        let width = label
            .as_ref()
            .map_or(0.0, |label| GAP + label.size().x + 2.0);
        let mut rect = Rect::from_two_pos(
            pos2(x - HALF_WIDTH, y),
            pos2(x + HALF_WIDTH + width, y + down * height),
        );
        // Touching is not covering: a buy and a sell at one price both keep their tips on it.
        while placed
            .iter()
            .any(|other| other.intersect(rect).is_positive())
        {
            rect = rect.translate(vec2(0.0, down * (height + GAP)));
        }
        placed.push(rect);
        let tip = pos2(
            x,
            if down > 0.0 {
                rect.top()
            } else {
                rect.bottom()
            },
        );
        paint_arrow(&painter, tip, down, color, theme.chart.bg);
        if let Some(label) = label {
            let at = pos2(
                x + HALF_WIDTH + GAP,
                tip.y + down * height / 2.0 - label.size().y / 2.0,
            );
            // The chart's background keeps the label readable over the bars.
            painter.rect_filled(
                Rect::from_min_size(at, label.size()).expand(1.0),
                2.0,
                theme.chart.bg,
            );
            painter.galley(at, label, color);
        }
        let hover = rect.intersect(mapping.rect);
        if hover.is_positive() {
            ui.interact(
                hover,
                ui.scope_id().with(("execution", index)),
                Sense::hover(),
            )
            .on_hover_text(format!(
                "{} {} at {:.*}\n{}",
                side_name(fill.side),
                fill.quantity,
                decimals,
                fill.price,
                // With seconds, also on charts of daily bars.
                chart.build_time_formatter().format_crosshair(fill.time, 1)
            ));
        }
    }
}

/// An arrow with its tip at `tip`, hanging below it when `down` is 1 and above it when -1,
/// outlined in `halo` so that it stands out from the bars behind it.
fn paint_arrow(painter: &egui::Painter, tip: Pos2, down: f32, color: Color32, halo: Color32) {
    let at = |x: f32, y: f32| tip + vec2(x, down * y);
    painter.add(Shape::closed_line(
        vec![
            at(0.0, 0.0),
            at(HALF_WIDTH, HEAD),
            at(HALF_STEM, HEAD),
            at(HALF_STEM, HEAD + STEM),
            at(-HALF_STEM, HEAD + STEM),
            at(-HALF_STEM, HEAD),
            at(-HALF_WIDTH, HEAD),
        ],
        Stroke::new(2.0, halo),
    ));
    painter.add(Shape::convex_polygon(
        vec![at(0.0, 0.0), at(HALF_WIDTH, HEAD), at(-HALF_WIDTH, HEAD)],
        color,
        Stroke::NONE,
    ));
    painter.rect_filled(
        Rect::from_two_pos(at(-HALF_STEM, HEAD - 0.5), at(HALF_STEM, HEAD + STEM)),
        0.0,
        color,
    );
}
