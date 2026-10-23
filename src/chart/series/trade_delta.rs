//! Histogram and cumulative candles for execution-based delta values.

use crate::chart::coords::ChartMapping;
use crate::studies::TradeDeltaValue;
use egui::{Color32, Painter, Rect, Stroke, pos2};

/// Trade delta representation in a host-supplied pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TradeDeltaStyle {
    /// Each bar's buy volume minus sell volume, relative to zero.
    Histogram,
    /// Open, high, low and close of cumulative delta.
    Candles,
}

/// Draws exact delta values using the supplied pane's mapping and colours.
/// `values` has the same global bar indices as the price chart. The host fits the pane's
/// scale to delta (histogram) or cumulative low/high (candles), including zero.
pub fn render_trade_delta(
    painter: &Painter,
    mapping: &ChartMapping,
    values: &[TradeDeltaValue],
    style: TradeDeltaStyle,
    buy: Color32,
    sell: Color32,
) {
    if mapping.bar_spacing <= 0.0 || !mapping.bar_spacing.is_finite() {
        return;
    }
    let painter = painter.with_clip_rect(mapping.rect.intersect(painter.clip_rect()));
    let first = mapping
        .x_to_idx(mapping.rect.left())
        .saturating_sub(1)
        .min(values.len());
    let end = mapping
        .x_to_idx(mapping.rect.right())
        .saturating_add(2)
        .min(values.len());
    let half_width = (mapping.bar_spacing * 0.35).max(0.5);
    for (i, value) in values.iter().enumerate().take(end).skip(first) {
        let x = mapping.idx_to_x(i);
        let color = if value.delta >= 0.0 { buy } else { sell };
        let (open, close) = match style {
            TradeDeltaStyle::Histogram => (0.0, value.delta),
            TradeDeltaStyle::Candles => (value.open, value.close),
        };
        let y1 = mapping.price_to_y(open);
        let y2 = mapping.price_to_y(close);
        if !y1.is_finite() || !y2.is_finite() {
            continue;
        }
        if style == TradeDeltaStyle::Candles {
            let high = mapping.price_to_y(value.high);
            let low = mapping.price_to_y(value.low);
            if high.is_finite() && low.is_finite() {
                painter.line_segment([pos2(x, high), pos2(x, low)], Stroke::new(1.0, color));
            }
        }
        painter.rect_filled(
            Rect::from_min_max(
                pos2(x - half_width, y1.min(y2)),
                pos2(x + half_width, y1.max(y2).max(y1.min(y2) + 1.0)),
            ),
            0,
            color,
        );
    }
}
