//! Range-based chart renderers
//!
//! Includes: High-Low, Range Bars

use crate::scales::{PriceFormatter, PriceMap};
use egui::{Color32, FontId, Galley, Painter, Pos2, Rect, Shape};

use super::volume::render_volume_bars;
use crate::chart::rendering::axes::tag_decimals;
use crate::chart::rendering::{RenderContext, StyleColors};
use crate::model::{Bar, HighLowStyle};

use super::japanese::calc_brick_rect;

/// Clear space between a bar's end and its price label, and between neighbouring labels.
const LABEL_GAP: f32 = 2.0;

/// High-Low chart: one colour for every bar's range, with optional price labels.
pub(super) fn render_high_low(
    painter: &Painter,
    volume_ctx: &RenderContext,
    visible_data: &[Bar],
    start_idx: usize,
    bar_width: f32,
    show_volume: bool,
    max_volume: f64,
    colors: &StyleColors,
    price_scale: &PriceMap,
    price_formatter: Option<&dyn PriceFormatter>,
    chart_rect_min_x: f32,
    idx_to_coord: impl Fn(usize, f32) -> f32,
    style: &HighLowStyle,
) {
    // A flat bar or a dense view still leaves a visible mark.
    let half_width = (bar_width / 4.0).max(0.5);
    let mut ends = Vec::with_capacity(visible_data.len());
    for (i, bar) in visible_data.iter().enumerate() {
        if bar.is_whitespace() {
            continue;
        }
        let x = idx_to_coord(start_idx + i, chart_rect_min_x);

        let y_high = price_scale.price_to_y(bar.high);
        let y_low = price_scale.price_to_y(bar.low);
        let top = y_high.min(y_low);

        let hl_rect = Rect::from_min_max(
            Pos2::new(x - half_width, top),
            Pos2::new(x + half_width, y_high.max(y_low).max(top + 1.0)),
        );
        painter.add(Shape::rect_filled(hl_rect, 0.0, style.bar_color));
        if bar.high.is_finite() && bar.low.is_finite() {
            ends.push((bar, x, hl_rect));
        }
    }

    if style.labels_visible && style.label_font_size > 0.0 && style.label_font_size.is_finite() {
        let style = HighLowStyle {
            label_color: if style.label_color == Color32::PLACEHOLDER {
                colors.text
            } else {
                style.label_color
            },
            ..*style
        };
        render_high_low_labels(painter, &ends, price_scale, price_formatter, &style);
    }

    if show_volume {
        render_volume_bars(
            volume_ctx,
            visible_data,
            start_idx,
            max_volume,
            colors,
            chart_rect_min_x,
            &idx_to_coord,
        );
    }
}

/// Prints every bar's high and low, or nothing while any label would not fit the bar spacing.
fn render_high_low_labels(
    painter: &Painter,
    ends: &[(&Bar, f32, Rect)],
    price_scale: &PriceMap,
    price_formatter: Option<&dyn PriceFormatter>,
    style: &HighLowStyle,
) {
    let spacing = ends
        .windows(2)
        .map(|pair| (pair[1].1 - pair[0].1).abs())
        .fold(f32::INFINITY, f32::min);
    let layout = |price: f64| {
        let label = price_scale.label(
            price_scale.transform().to_scale(price),
            tag_decimals(price_scale, price_scale.price_to_y(price)),
            price_formatter,
        );
        painter.layout_no_wrap(
            label,
            FontId::proportional(crate::styles::typography::font_size(style.label_font_size)),
            style.label_color,
        )
    };
    let fits = |galley: &Galley| galley.size().x + LABEL_GAP <= spacing;
    // One label rules out a dense view before laying out the thousands of bars it can hold.
    let Some((first, ..)) = ends.first() else {
        return;
    };
    if !fits(&layout(first.high)) {
        return;
    }
    let mut labels = Vec::with_capacity(ends.len());
    for (bar, x, rect) in ends {
        let (high, low) = (layout(bar.high), layout(bar.low));
        if !fits(&high) || !fits(&low) {
            return;
        }
        // An inverted scale puts the high at the bottom of the bar.
        let high_on_top = price_scale.price_to_y(bar.high) <= price_scale.price_to_y(bar.low);
        let (above, below) = if high_on_top {
            (high, low)
        } else {
            (low, high)
        };
        labels.push((*x, *rect, above, below));
    }
    for (x, rect, above, below) in labels {
        let height = above.size().y;
        let (above_x, below_x) = (x - above.size().x / 2.0, x - below.size().x / 2.0);
        painter.galley(
            Pos2::new(above_x, rect.top() - LABEL_GAP - height),
            above,
            style.label_color,
        );
        painter.galley(
            Pos2::new(below_x, rect.bottom() + LABEL_GAP),
            below,
            style.label_color,
        );
    }
}

/// Range bars at their element slots, each over its whole range from its low to its high.
pub(super) fn render_range_bars(
    painter: &Painter,
    range_bars: &[Bar],
    bar_width: f32,
    price_scale: &PriceMap,
    bullish_color: Color32,
    bearish_color: Color32,
    x_of: impl Fn(usize) -> f32,
) {
    for (i, range_bar) in range_bars.iter().enumerate() {
        let x = x_of(i);
        let color = if range_bar.close > range_bar.open {
            bullish_color
        } else {
            bearish_color
        };
        let rect = calc_brick_rect(x, range_bar.low, range_bar.high, price_scale, bar_width);
        painter.add(Shape::rect_filled(rect, 0.0, color));
    }
}
