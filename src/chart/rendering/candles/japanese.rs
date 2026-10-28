//! Japanese chart renderers
//!
//! Includes: Renko, Kagi, Line Break, Point & Figure

use super::helpers::{draw_brick, draw_o_symbol, draw_x_symbol};
use crate::model::KagiStyle;
use crate::model::kagi::{KagiLine, KagiThickness};
use crate::model::line_break::LineBreakLine;
use crate::model::point_figure::PnfColumn;
use crate::model::{Bar, ColumnDirection, RenkoBrick};
use crate::scales::PriceMap;
use egui::{Color32, Painter, Pos2, Rect, Shape, Stroke};

pub(super) fn render_renko(
    painter: &Painter,
    bricks: &[RenkoBrick],
    show_wicks: bool,
    bar_width: f32,
    price_scale: &PriceMap,
    bullish_color: Color32,
    bearish_color: Color32,
    x_of: impl Fn(usize) -> f32,
) {
    let coords = *price_scale;
    for (i, brick) in bricks.iter().enumerate() {
        let x = x_of(i);

        let bar = brick.to_bar();
        let color = if bar.close > bar.open {
            bullish_color
        } else {
            bearish_color
        };
        if show_wicks {
            painter.line_segment(
                [
                    Pos2::new(x, coords.price_to_y(brick.high)),
                    Pos2::new(x, coords.price_to_y(brick.low)),
                ],
                Stroke::new(1.0, color),
            );
        }
        let rect = calc_brick_rect(x, &bar, &coords, bar_width);
        draw_brick(painter, rect, color, 1.0);
    }
}

pub(super) fn render_kagi(
    painter: &Painter,
    lines: &[KagiLine],
    price_scale: &PriceMap,
    style: KagiStyle,
    bullish_color: Color32,
    bearish_color: Color32,
    x_of: impl Fn(usize) -> f32,
) {
    // A non-finite width hides its lines, as a non-positive one does.
    let width = |w: f32| if w.is_finite() { w } else { 0.0 };
    let stroke = |thickness| match thickness {
        KagiThickness::Thick => Stroke::new(width(style.thick_width), bullish_color),
        KagiThickness::Thin => Stroke::new(width(style.thin_width), bearish_color),
    };
    let flip = |thickness| match thickness {
        KagiThickness::Thick => KagiThickness::Thin,
        KagiThickness::Thin => KagiThickness::Thick,
    };
    let mut prev_x = None;
    for (i, line) in lines.iter().enumerate() {
        let x = x_of(i);
        let y = |price| price_scale.price_to_y(price);
        let start = Pos2::new(x, y(line.start_price));
        let start_thickness = line
            .crossing
            .map_or(line.thickness, |_| flip(line.thickness));
        // The shoulder or waist joining the previous line keeps its thickness.
        if let Some(prev_x) = prev_x {
            let joint = Pos2::new(prev_x, start.y);
            painter.add(Shape::line_segment([joint, start], stroke(start_thickness)));
        }
        let end = Pos2::new(x, y(line.end_price));
        if let Some(crossing) = line.crossing {
            let mid = Pos2::new(x, y(crossing));
            painter.add(Shape::line_segment([start, mid], stroke(start_thickness)));
            painter.add(Shape::line_segment([mid, end], stroke(line.thickness)));
        } else {
            painter.add(Shape::line_segment([start, end], stroke(line.thickness)));
        }
        prev_x = Some(x);
    }
}

/// Line Break chart (Three Line Break)
pub(super) fn render_line_break(
    painter: &Painter,
    lines: &[LineBreakLine],
    bar_width: f32,
    price_scale: &PriceMap,
    bullish_color: Color32,
    bearish_color: Color32,
    x_of: impl Fn(usize) -> f32,
) {
    let coords = *price_scale;
    if lines.is_empty() {
        return;
    }

    let actual_width = bar_width.max(3.0);
    for (i, line) in lines.iter().enumerate() {
        let x = x_of(i);

        let color = if line.is_bullish() {
            bullish_color
        } else {
            bearish_color
        };
        let rect = calc_line_break_rect(x, line.open, line.close, &coords, actual_width);
        draw_brick(painter, rect, color, 1.0);
    }
}

/// Point and Figure chart (X and O columns)
pub(super) fn render_point_and_figure(
    painter: &Painter,
    price_rect: Rect,
    columns: &[PnfColumn],
    bar_width: f32,
    price_scale: &PriceMap,
    bullish_color: Color32,
    bearish_color: Color32,
    x_of: impl Fn(usize) -> f32,
) {
    let coords = *price_scale;
    if columns.is_empty() {
        return;
    }

    let symbol_size = (bar_width * 0.8).max(4.0);
    for (i, column) in columns.iter().enumerate() {
        let x = x_of(i);

        draw_pnf_column(
            painter,
            x,
            column,
            &coords,
            price_rect,
            symbol_size,
            bullish_color,
            bearish_color,
        );
    }
}

// === Helper Functions ===

pub(super) fn calc_brick_rect(x: f32, bar: &Bar, coords: &PriceMap, bar_width: f32) -> Rect {
    let y_open = coords.price_to_y(bar.open);
    let y_close = coords.price_to_y(bar.close);
    let top = y_open.min(y_close);
    let bottom = y_open.max(y_close);
    let height = (bottom - top).max(2.0);
    let width = bar_width.max(3.0);

    Rect::from_min_max(
        Pos2::new(x - width / 2.0, top),
        Pos2::new(x + width / 2.0, top + height),
    )
}

fn calc_line_break_rect(x: f32, open: f64, close: f64, coords: &PriceMap, width: f32) -> Rect {
    let y_open = coords.price_to_y(open);
    let y_close = coords.price_to_y(close);
    let top = y_open.min(y_close);
    let height = (y_open.max(y_close) - top).max(2.0);

    Rect::from_min_max(
        Pos2::new(x - width / 2.0, top),
        Pos2::new(x + width / 2.0, top + height),
    )
}

fn draw_pnf_column(
    painter: &Painter,
    x: f32,
    column: &crate::model::point_figure::PnfColumn,
    coords: &PriceMap,
    price_rect: Rect,
    symbol_size: f32,
    bullish_color: Color32,
    bearish_color: Color32,
) {
    let up = column.direction == ColumnDirection::Up;
    let color = if up { bullish_color } else { bearish_color };
    let step = column.box_size;
    let start = column.start_price;

    // A box under 2 px tall is not drawable as a symbol: draw the column as a line.
    if (coords.price_to_y(start) - coords.price_to_y(start + step)).abs() < 2.0 {
        painter.add(Shape::line_segment(
            [
                Pos2::new(x, coords.price_to_y(start)),
                Pos2::new(x, coords.price_to_y(column.end_price)),
            ],
            Stroke::new(1.5, color),
        ));
        return;
    }

    // Box `k` is at `start +- k * step`; only the boxes inside the pane are visited.
    let (p_a, p_b) = (
        coords.y_to_price(price_rect.max.y),
        coords.y_to_price(price_rect.min.y),
    );
    let (low, high) = (p_a.min(p_b), p_a.max(p_b));
    let (k_a, k_b) = if up {
        ((low - start) / step, (high - start) / step)
    } else {
        ((start - high) / step, (start - low) / step)
    };
    let first = k_a.ceil().max(0.0) as usize;
    let last = k_b.floor().min(column.box_cnt as f64 - 1.0);
    if last < 0.0 {
        return;
    }
    let half = (symbol_size / 2.0).max(2.0);
    for k in first..=last as usize {
        let offset = k as f64 * step;
        let y = coords.price_to_y(if up { start + offset } else { start - offset });
        if up {
            draw_x_symbol(painter, Pos2::new(x, y), half, color);
        } else {
            draw_o_symbol(painter, Pos2::new(x, y), half, color);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::RenkoDirection;
    use crate::scales::{PriceRange, PriceTransform};

    #[test]
    fn renko_wick_switch_draws_actual_extremes() {
        // On a 100..110 axis spanning 100 points, 104 maps to y=60 and 101.5 to y=85.
        for show_wicks in [false, true] {
            let ctx = egui::Context::default();
            let map = PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(100.0, 110.0),
                0.0,
                100.0,
                false,
            );
            let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                render_renko(
                    ui.painter(),
                    &[RenkoBrick {
                        ts: chrono::Utc::now(),
                        open: 103.0,
                        close: 104.0,
                        high: 104.0,
                        low: 101.5,
                        direction: RenkoDirection::Up,
                    }],
                    show_wicks,
                    8.0,
                    &map,
                    Color32::GREEN,
                    Color32::RED,
                    |_| 50.0,
                );
            });
            let wicks: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|s| {
                    if let Shape::LineSegment { points, .. } = &s.shape {
                        Some(*points)
                    } else {
                        None
                    }
                })
                .collect();
            output.drop_without_applying_deltas();
            assert_eq!(
                wicks,
                if show_wicks {
                    vec![[Pos2::new(50.0, 60.0), Pos2::new(50.0, 85.0)]]
                } else {
                    vec![]
                }
            );
        }
    }

    #[test]
    fn kagi_paints_each_part_of_a_line_with_its_own_thickness() {
        use KagiThickness::{Thick, Thin};
        // On a 100..110 axis spanning 100 points, price p maps to y = 10 * (110 - p).
        let ctx = egui::Context::default();
        let map = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(100.0, 110.0),
            0.0,
            100.0,
            false,
        );
        let line = |start_price, end_price, thickness, crossing| KagiLine {
            ts: chrono::Utc::now(),
            start_price,
            end_price,
            thickness,
            crossing,
        };
        let style = KagiStyle {
            thick_width: 4.0,
            thin_width: 1.0,
        };
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            render_kagi(
                ui.painter(),
                &[
                    line(102.0, 105.0, Thin, None),
                    line(105.0, 103.0, Thin, None),
                    line(103.0, 107.0, Thick, Some(105.0)),
                ],
                &map,
                style,
                Color32::GREEN,
                Color32::RED,
                |i| 10.0 * i as f32,
            );
        });
        let segments: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|s| match &s.shape {
                Shape::LineSegment { points, stroke } => Some((*points, stroke.width)),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        let p = Pos2::new;
        assert_eq!(
            segments,
            [
                ([p(0.0, 80.0), p(0.0, 50.0)], 1.0),
                ([p(0.0, 50.0), p(10.0, 50.0)], 1.0),
                ([p(10.0, 50.0), p(10.0, 70.0)], 1.0),
                ([p(10.0, 70.0), p(20.0, 70.0)], 1.0),
                ([p(20.0, 70.0), p(20.0, 50.0)], 1.0),
                ([p(20.0, 50.0), p(20.0, 30.0)], 4.0),
            ]
        );
    }
}
