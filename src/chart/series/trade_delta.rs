//! Histogram, per-bar candles and cumulative candles for execution-based delta values.

use crate::chart::coords::ChartMapping;
use crate::studies::TradeDeltaValue;
use egui::{Color32, Painter, Rect, Stroke, pos2};

/// Trade delta representation in a host-supplied pane.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum TradeDeltaStyle {
    /// Each bar's buy volume minus sell volume, relative to zero.
    Histogram,
    /// Open, high, low and close of cumulative delta.
    CumulativeCandles,
    /// Each bar's delta as a candle that opens at zero, closes at the bar's delta and has a
    /// wick from the lowest to the highest running delta of its executions.
    BarCandles,
}

/// Draws exact delta values using the supplied pane's mapping and colours.
/// `values` has the same global bar indices as the price chart. The host fits the pane's
/// scale, including zero, to `delta` (histogram), `delta_low` to `delta_high` (bar candles)
/// or `low` to `high` (cumulative candles). A bar without executions, whose
/// [`traded`](TradeDeltaValue::traded) is `false`, is not drawn. Histograms and bar candles draw
/// a bar whose delta is above zero in `buy` and any other in `sell`, so a balanced bar sells;
/// cumulative candles draw a bar whose delta is zero or more in `buy`, so an unchanged total
/// keeps buying.
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
        if !value.traded {
            continue;
        }
        let x = mapping.idx_to_x(i);
        // A balanced bar sells; an unchanged cumulative total keeps buying, as the studies paint.
        let buying = value.delta > 0.0
            || (value.delta == 0.0 && style == TradeDeltaStyle::CumulativeCandles);
        let color = if buying { buy } else { sell };
        let (open, close, wick) = match style {
            TradeDeltaStyle::Histogram => (0.0, value.delta, None),
            TradeDeltaStyle::BarCandles => {
                (0.0, value.delta, Some((value.delta_high, value.delta_low)))
            }
            TradeDeltaStyle::CumulativeCandles => {
                (value.open, value.close, Some((value.high, value.low)))
            }
        };
        let y1 = mapping.price_to_y(open);
        let y2 = mapping.price_to_y(close);
        if !y1.is_finite() || !y2.is_finite() {
            continue;
        }
        if let Some((high, low)) = wick {
            let (high, low) = (mapping.price_to_y(high), mapping.price_to_y(low));
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Footprint;
    use crate::model::{Bar, BarClock, OrderSide, Tick, Timeframe};
    use crate::scales::{PriceMap, PriceRange, PriceTransform};
    use crate::studies::trade_delta;
    use chrono::{DateTime, TimeDelta, Utc};
    use egui::{Context, LayerId, Pos2, Shape, vec2};

    /// Bar 0 buys 4. Bar 1 buys 5, sells 8 and buys 5: running delta 5, -3, 2 from a cumulative
    /// 4. Bar 2 buys 6 and sells 6, a balanced bar from a cumulative 6, and bar 3 has no
    /// executions. On a -16..16 pane 128 points tall a delta `p` is drawn at `y = 64 - 4p`. The
    /// per-bar styles paint the balanced bar in the selling colour and the cumulative candles its
    /// unchanged total in the buying colour; no style draws bar 3 (#4065).
    #[test]
    fn bar_candles_span_the_bar_range_from_zero_beside_histogram_and_cumulative_candles() {
        let start: DateTime<Utc> = "2026-01-05T10:00:00Z".parse().unwrap();
        let bars: Vec<_> = (0..4)
            .map(|i| {
                Bar::new(
                    start + TimeDelta::minutes(i),
                    100.0,
                    100.0,
                    100.0,
                    100.0,
                    0.0,
                )
            })
            .collect();
        let mut footprint = Footprint::new(BarClock::new(Timeframe::Min1), 1.0, 1).unwrap();
        for (second, size, side) in [
            (0, 4.0, OrderSide::Buy),
            (60, 5.0, OrderSide::Buy),
            (61, 8.0, OrderSide::Sell),
            (62, 5.0, OrderSide::Buy),
            (120, 6.0, OrderSide::Buy),
            (121, 6.0, OrderSide::Sell),
        ] {
            footprint
                .push(&Tick::new(
                    start + TimeDelta::seconds(second),
                    100.0,
                    size,
                    Some(side),
                ))
                .unwrap();
        }
        let values = trade_delta(&footprint, &bars, 1, |_, _| false);
        let rect = Rect::from_min_size(Pos2::ZERO, vec2(100.0, 128.0));
        let price = PriceMap::new(
            PriceTransform::default(),
            PriceRange::new(-16.0, 16.0),
            0.0,
            128.0,
            false,
        );
        let mapping = ChartMapping::new(rect, 20.0, 0, 3, 0.0, price);
        let (g, r) = (Color32::GREEN, Color32::RED);
        for (style, wicks, bodies) in [
            (
                TradeDeltaStyle::Histogram,
                vec![],
                vec![(48.0, 64.0, g), (56.0, 64.0, g), (64.0, 65.0, r)],
            ),
            // Bar 1 from 0 to 2, wick from 5 to -3, although earlier bars left 4. Bar 2 from 0
            // to 0, wick from 6 to 0.
            (
                TradeDeltaStyle::BarCandles,
                vec![(48.0, 64.0, g), (44.0, 76.0, g), (40.0, 64.0, r)],
                vec![(48.0, 64.0, g), (56.0, 64.0, g), (64.0, 65.0, r)],
            ),
            // Bar 1 from 4 to 6, wick from 9 to 1. Bar 2 from 6 to 6, wick from 12 to 6.
            (
                TradeDeltaStyle::CumulativeCandles,
                vec![(48.0, 64.0, g), (28.0, 60.0, g), (16.0, 40.0, g)],
                vec![(48.0, 64.0, g), (40.0, 48.0, g), (40.0, 41.0, g)],
            ),
        ] {
            let ctx = Context::default();
            let output = ctx.run_ui(Default::default(), |_| {
                let painter = Painter::new(ctx.clone(), LayerId::background(), rect);
                render_trade_delta(&painter, &mapping, &values, style, g, r);
            });
            let (mut drawn_wicks, mut drawn_bodies) = (Vec::new(), Vec::new());
            for shape in &output.shapes {
                match &shape.shape {
                    Shape::LineSegment { points, stroke } => {
                        drawn_wicks.push((points[0].y, points[1].y, stroke.color))
                    }
                    Shape::Rect(body) => {
                        drawn_bodies.push((body.rect.top(), body.rect.bottom(), body.fill))
                    }
                    _ => {}
                }
            }
            output.drop_without_applying_deltas();
            assert_eq!((drawn_wicks, drawn_bodies), (wicks, bodies), "{style:?}");
        }
    }
}
