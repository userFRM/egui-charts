//! The resting size at each price of the live book, as bars at the right edge of the pane.

use crate::chart::coords::ChartMapping;
use crate::data::{
    OrderFilter, OrderFilterUnavailable, OrderQueue, OrderSizeTier, OrderSizeTiers, RestingOrder,
};
use crate::model::{OrderBook, OrderSide};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Painter, Rect, pos2};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// What the length of a bar of the book profile shows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BookProfile {
    /// The total resting size of the orders at the price.
    #[default]
    Total,
    /// The size of the largest known individual order at the price. Needs order events.
    LargestOrder,
}

/// How the live book is shown by the heatmap's profile and by a depth ladder: which individual
/// orders count, what a bar measures, and which orders are highlighted. The defaults show the
/// aggregate book as the feed sends it. Serializable, so a host can keep it with its chart
/// settings; fields missing from older documents take their defaults.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OrderBookView {
    /// The orders that count. Anything but the default needs a feed that sends order events,
    /// see [`Sink::push_order_event`](crate::data::Sink::push_order_event).
    pub filter: OrderFilter,
    /// What a bar of the profile measures.
    pub profile: BookProfile,
    /// Highlights a price whose largest known individual order reaches a tier. A price without
    /// order data, as with an aggregate feed, is not highlighted and not reported as small.
    pub tiers: Option<OrderSizeTiers>,
    /// Colours of the notable and the large tier.
    pub tier_colors: [Color32; 2],
}

impl Default for OrderBookView {
    fn default() -> Self {
        Self {
            filter: OrderFilter::default(),
            profile: BookProfile::Total,
            tiers: None,
            tier_colors: [
                DESIGN_TOKENS.semantic.status.caution,
                DESIGN_TOKENS.semantic.status.error,
            ],
        }
    }
}

impl OrderBookView {
    /// Whether the view needs individual orders: a filter or the largest-order profile.
    fn needs_orders(&self) -> bool {
        self.filter != OrderFilter::default() || self.profile == BookProfile::LargestOrder
    }

    /// The colour of a tier.
    pub fn tier_color(&self, tier: OrderSizeTier) -> Color32 {
        self.tier_colors[usize::from(tier == OrderSizeTier::Large)]
    }
}

/// Draws the profile of `queue`'s orders, or of `book` when the feed sends aggregate depth only,
/// as bars growing left from the right edge of `mapping.rect`, longest 60 points, `tick` high at
/// each price, centred on it. Bids use `colors[0]` and asks `colors[1]`, unless the largest
/// order at the price reaches a tier of the view. Fails, drawing nothing, when the view needs
/// individual orders and
/// the feed has none, so that the host can say the filtering is unavailable.
pub fn render_book_profile(
    painter: &Painter,
    mapping: &ChartMapping,
    book: Option<&OrderBook>,
    queue: Option<&OrderQueue>,
    tick: f64,
    view: &OrderBookView,
    colors: [Color32; 2],
) -> Result<(), OrderFilterUnavailable> {
    if !tick.is_finite() || tick <= 0.0 {
        return Ok(());
    }
    let level = |price: f64| (price / tick).round() as i64;
    // Bars as (price, is bid, length measure, tier).
    let mut bars: Vec<(f64, bool, f64, Option<OrderSizeTier>)> = Vec::new();
    match (queue, book) {
        (Some(queue), _) => {
            let summed = queue.filtered("", &view.filter)?;
            let largest = queue.largest_orders(&view.filter);
            let tier = |order: &RestingOrder| view.tiers.and_then(|tiers| tiers.tier(order.size));
            let tiers: BTreeMap<_, _> = largest
                .iter()
                .map(|o| ((o.side == OrderSide::Buy, level(o.price)), tier(o)))
                .collect();
            match view.profile {
                BookProfile::Total => {
                    for (bid, levels) in [(true, &summed.bids), (false, &summed.asks)] {
                        for l in levels {
                            let at = tiers.get(&(bid, level(l.price))).copied().flatten();
                            bars.push((l.price, bid, l.size, at));
                        }
                    }
                }
                BookProfile::LargestOrder => bars.extend(
                    largest
                        .iter()
                        .map(|o| (o.price, o.side == OrderSide::Buy, o.size, tier(o))),
                ),
            }
        }
        (None, _) if view.needs_orders() => return Err(OrderFilterUnavailable::AggregateDepth),
        (None, Some(book)) => {
            for (bid, levels) in [(true, &book.bids), (false, &book.asks)] {
                bars.extend(levels.iter().map(|l| (l.price, bid, l.size, None)));
            }
        }
        (None, None) => {}
    }
    let max = bars.iter().map(|b| b.2).fold(0.0, f64::max);
    if max <= 0.0 {
        return Ok(());
    }
    let painter = painter.with_clip_rect(mapping.rect.intersect(painter.clip_rect()));
    for (price, bid, size, tier) in bars {
        let color = tier.map_or(colors[usize::from(!bid)], |tier| view.tier_color(tier));
        let (y0, y1) = (
            mapping.price_to_y(price - tick * 0.5),
            mapping.price_to_y(price + tick * 0.5),
        );
        let rect = Rect::from_min_max(
            pos2(
                mapping.rect.right() - 60.0 * (size / max) as f32,
                y0.min(y1),
            ),
            pos2(mapping.rect.right(), y0.max(y1)),
        );
        if rect.is_finite() && rect.intersects(mapping.rect) {
            painter.rect_filled(rect, 0.0, color.gamma_multiply(0.7));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::OrderBookLevel;
    use crate::scales::{PriceMap, PriceRange, PriceTransform};
    use egui::epaint::Shape;

    /// Guards the profile rules: a bar measures the total at the price or its largest order, a
    /// price is highlighted by the tier of its largest known order whichever the measure, an
    /// aggregate feed has no known order, so nothing is highlighted and a view that needs orders
    /// fails instead of drawing aggregate sizes, and the length of a bar is its share of the
    /// longest, 60 points.
    #[test]
    fn bars_follow_the_measure_and_highlight_the_tier_of_the_largest_order() {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut queue = OrderQueue::new(Some(0.25));
        let orders = [(1, 99.0, 3.0), (2, 99.0, 9.0), (3, 98.75, 2.0)]
            .map(|(id, price, size)| RestingOrder::new(id, OrderSide::Buy, price, size));
        queue.snapshot(1, time, orders.to_vec()).unwrap();
        let aggregate = OrderBook {
            ts: time,
            symbol: "A".into(),
            bids: vec![OrderBookLevel {
                price: 99.0,
                size: 12.0,
                order_cnt: None,
            }],
            asks: vec![],
        };
        let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(200.0, 200.0));
        let map = ChartMapping::new(
            rect,
            10.0,
            0,
            0,
            0.0,
            PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(98.0, 100.0),
                0.0,
                200.0,
                false,
            ),
        );
        let (bid, ask) = (Color32::GREEN, Color32::RED);
        let view = |profile| OrderBookView {
            profile,
            tiers: Some(OrderSizeTiers {
                notable: 5.0,
                large: 10.0,
            }),
            ..Default::default()
        };
        let notable = view(BookProfile::Total).tier_color(OrderSizeTier::Notable);
        let draw = |view: &OrderBookView, queue, book| {
            let ctx = egui::Context::default();
            let mut result = Ok(());
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(rect),
                    ..Default::default()
                },
                |ui| {
                    result = render_book_profile(
                        ui.painter(),
                        &map,
                        book,
                        queue,
                        0.25,
                        view,
                        [bid, ask],
                    );
                },
            );
            output.textures_delta.clear();
            let bars = Vec::from_iter(output.shapes.iter().filter_map(|s| match &s.shape {
                Shape::Rect(r) => Some((r.rect.width().round(), r.fill)),
                _ => None,
            }));
            (result, bars)
        };
        let [bid_fill, tier_fill] = [bid, notable].map(|c| c.gamma_multiply(0.7));
        // Total: 12 and 2 against 12; the largest order at 99.0 is 9, a notable one.
        let (result, bars) = draw(&view(BookProfile::Total), Some(&queue), None);
        assert_eq!(result, Ok(()));
        assert_eq!(bars, [(60.0, tier_fill), (10.0, bid_fill)]);
        // Largest order: 9 and 2 against 9.
        let (_, bars) = draw(&view(BookProfile::LargestOrder), Some(&queue), None);
        assert_eq!(bars, [(60.0, tier_fill), (13.0, bid_fill)]);
        // Aggregate depth: the total, with no known order to highlight; the largest order has
        // no answer.
        let (result, bars) = draw(&view(BookProfile::Total), None, Some(&aggregate));
        assert_eq!((result, bars), (Ok(()), vec![(60.0, bid_fill)]));
        let (result, bars) = draw(&view(BookProfile::LargestOrder), None, Some(&aggregate));
        assert_eq!(
            (result, bars),
            (Err(OrderFilterUnavailable::AggregateDepth), vec![])
        );
    }
}
