//! The resting size at each price of the live book, as bars at the right edge of the pane.

use crate::chart::coords::ChartMapping;
use crate::data::{
    OrderFilter, OrderFilterUnavailable, OrderQueue, OrderSizeTier, OrderSizeTiers, RestingOrder,
};
use crate::model::{OrderBook, OrderSide};
use crate::theme::{ChartColors, ThemeExt};
use egui::{Color32, FontId, Painter, Rect, Stroke, StrokeKind, pos2, vec2};
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
/// orders count, what a bar measures, and which orders are highlighted. Its tiers also mark the
/// heatmap's cells. The defaults show the aggregate book as the feed sends it. Serializable, so a
/// host can keep it with its chart settings; fields missing from older documents take their
/// defaults.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OrderBookView {
    /// The orders that count. Anything but the default needs a feed that sends order events,
    /// see [`Sink::push_order_event`](crate::data::Sink::push_order_event).
    pub filter: OrderFilter,
    /// What a bar of the profile measures.
    pub profile: BookProfile,
    /// Quantity unit supplied by the host, such as "lots", "shares" or "BTC". Labels use
    /// "feed units" when absent or blank. This describes the supplied sizes, without conversion;
    /// it is independent of the instrument's price unit.
    pub quantity_unit: Option<String>,
    /// Highlights a price whose largest known individual order reaches a tier. A price without
    /// order data, as with an aggregate feed, is not highlighted and not reported as small. The
    /// liquidity heatmap marks its cells by the largest order its history recorded in the row, see
    /// [`DepthColumn::largest_bid_orders`](crate::data::DepthColumn::largest_bid_orders).
    pub tiers: Option<OrderSizeTiers>,
    /// Colours of the notable and the large tier, or `None`, the default, for the
    /// [`ChartColors::order_notable`] and [`order_large`](ChartColors::order_large) of the theme
    /// the view is drawn with, so the marks follow a change of theme.
    pub tier_colors: Option<[Color32; 2]>,
}

impl OrderBookView {
    /// Whether the view needs individual orders: a filter or the largest-order profile.
    fn needs_orders(&self) -> bool {
        self.filter != OrderFilter::default() || self.profile == BookProfile::LargestOrder
    }

    /// The colour of a tier: from [`tier_colors`](Self::tier_colors) when set, otherwise from
    /// `colors`, the chart colours of the theme the view is drawn with.
    pub fn tier_color(&self, tier: OrderSizeTier, colors: &ChartColors) -> Color32 {
        let [notable, large] = self
            .tier_colors
            .unwrap_or([colors.order_notable, colors.order_large]);
        if tier == OrderSizeTier::Large {
            large
        } else {
            notable
        }
    }
}

/// Draws the profile of `queue`'s orders, or of `book` when the feed sends aggregate depth only,
/// as bars growing left from the right edge of `mapping.rect`, longest 60 points, `tick` high at
/// each price, centred on it. Bids use `colors[0]` and asks `colors[1]`, unless the largest
/// order at the price reaches a tier of the view, drawn in [`OrderBookView::tier_color`] with the
/// painter's active theme. Fails, drawing nothing, when the view needs
/// individual orders and the feed has none, so that the host can say the filtering is
/// unavailable. A key identifies the measure, quantity unit and maximum across all supplied
/// levels, including offscreen ones, after filtering. Hovering a row anywhere in the 60-point
/// strip shows its exact price, side and measured resting size, without abbreviating quantities.
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
    let theme = painter.ctx().active_theme();
    let painter = painter.with_clip_rect(mapping.rect.intersect(painter.clip_rect()));
    let pointer = painter
        .ctx()
        .pointer_hover_pos()
        .filter(|p| painter.clip_rect().contains(*p) && p.x >= mapping.rect.right() - 60.0);
    let mut hovered = None;
    for (price, bid, size, tier) in bars {
        let color = tier.map_or(colors[usize::from(!bid)], |tier| {
            view.tier_color(tier, &theme.chart)
        });
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
            if pointer.is_some_and(|p| rect.y_range().contains(p.y)) {
                hovered = Some((price, bid, size));
                painter.rect_stroke(
                    rect,
                    0.0,
                    Stroke::new(1.0, theme.chart.axis_text),
                    StrokeKind::Inside,
                );
            }
        }
    }
    let ctx = painter.ctx();
    let unit = view
        .quantity_unit
        .as_deref()
        .map(str::trim)
        .filter(|unit| !unit.is_empty())
        .map_or_else(
            || crate::localization::text(ctx, "feed units"),
            str::to_owned,
        );
    let measure = crate::localization::text(
        ctx,
        match view.profile {
            BookProfile::Total => "Resting total",
            BookProfile::LargestOrder => "Largest resting order",
        },
    );
    let key = crate::localization::format_text(
        ctx,
        "{measure} ({unit})\nFull width = {max} {unit}\nAll supplied levels, after filtering",
        &[
            ("measure", measure.as_str()),
            ("unit", &unit),
            ("max", &max.to_string()),
        ],
    );
    // Liquidity paints before candles; keep its readout above the series in the same pane.
    let parent = painter.layer_id();
    let layer = egui::LayerId::new(parent.order, parent.id.with("book_profile_readout"));
    ctx.set_sublayer(parent, layer);
    let painter = painter.clone().with_layer_id(layer);
    let label = |text: String, bottom: f32, right: f32| {
        let galley = painter.layout_no_wrap(text, FontId::proportional(11.0), theme.ui.text);
        let size = galley.size() + vec2(12.0, 8.0);
        let at = pos2(
            (right - size.x).max(mapping.rect.left() + 4.0),
            (bottom - size.y).max(mapping.rect.top() + 4.0),
        );
        let rect = Rect::from_min_size(at, size);
        painter.rect_filled(rect, 3.0, theme.ui.panel_bg_floating);
        painter.rect_stroke(
            rect,
            3.0,
            Stroke::new(1.0, theme.ui.border),
            StrokeKind::Inside,
        );
        painter.galley(at + vec2(6.0, 4.0), galley, theme.ui.text);
        rect
    };
    let key_rect = label(
        key,
        mapping.rect.bottom() - 4.0,
        mapping.rect.right() - 64.0,
    );
    if let Some((price, bid, size)) = hovered {
        let text = crate::localization::format_text(
            ctx,
            "{side} · Price {price}\n{measure}: {size} {unit}",
            &[
                (
                    "side",
                    crate::localization::text(ctx, if bid { "Bid" } else { "Ask" }),
                ),
                ("price", price.to_string()),
                ("measure", measure),
                ("size", size.to_string()),
                ("unit", unit),
            ],
        );
        label(
            text,
            (mapping.price_to_y(price) - 4.0).min(key_rect.top() - 4.0),
            mapping.rect.right() - 64.0,
        );
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
        let notable = view(BookProfile::Total)
            .tier_color(OrderSizeTier::Notable, &crate::theme::Theme::dark().chart);
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
            let bars = Vec::from_iter(output.shapes.iter().filter_map(|s| {
                match &s.shape {
                    Shape::Rect(r)
                        if [bid, notable]
                            .map(|c| c.gamma_multiply(0.7))
                            .contains(&r.fill) =>
                    {
                        Some((r.rect.width().round(), r.fill))
                    }
                    _ => None,
                }
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

    /// The maximum includes offscreen levels and uses the same filtered measure as the bars;
    /// hover reads the whole strip, even where a small bar does not reach the pointer.
    #[test]
    fn readout_quantifies_the_full_book_and_the_hovered_row() {
        let time = "2026-01-05T10:00:00Z".parse().unwrap();
        let mut queue = OrderQueue::new(Some(0.25));
        queue
            .snapshot(
                1,
                time,
                vec![
                    RestingOrder::new(1, OrderSide::Buy, 99.75, 25.125),
                    RestingOrder::new(2, OrderSide::Sell, 101.25, 600.0),
                    RestingOrder::new(3, OrderSide::Sell, 101.25, 400.0),
                    RestingOrder::new(4, OrderSide::Sell, 120.0, 2000.0),
                ],
            )
            .unwrap();
        let book = queue.filtered("A", &OrderFilter::default()).unwrap();
        let rect = Rect::from_min_max(pos2(0.0, 0.0), pos2(800.0, 600.0));
        let map = ChartMapping::new(
            rect,
            10.0,
            0,
            0,
            0.0,
            PriceMap::new(
                PriceTransform::default(),
                PriceRange::new(99.0, 102.0),
                0.0,
                600.0,
                false,
            ),
        );
        for theme in [crate::theme::Theme::dark(), crate::theme::Theme::light()] {
            for (profile, unit, orders, minimum, price, x, measure, size, side) in [
                (
                    BookProfile::Total,
                    Some("lots"),
                    false,
                    0.0,
                    101.25,
                    750.0,
                    "Resting total",
                    "1000",
                    "Ask",
                ),
                (
                    BookProfile::LargestOrder,
                    Some("lots"),
                    true,
                    0.0,
                    101.25,
                    750.0,
                    "Largest resting order",
                    "600",
                    "Ask",
                ),
                (
                    BookProfile::Total,
                    Some("lots"),
                    true,
                    500.0,
                    101.25,
                    750.0,
                    "Resting total",
                    "600",
                    "Ask",
                ),
                (
                    BookProfile::Total,
                    None,
                    false,
                    0.0,
                    99.75,
                    750.0,
                    "Resting total",
                    "25.125",
                    "Bid",
                ),
                (
                    BookProfile::Total,
                    Some("  "),
                    false,
                    0.0,
                    101.25,
                    750.0,
                    "Resting total",
                    "1000",
                    "Ask",
                ),
                (
                    BookProfile::Total,
                    Some("lots"),
                    false,
                    0.0,
                    101.25,
                    700.0,
                    "Resting total",
                    "1000",
                    "Ask",
                ),
            ] {
                // Deserialize so the regression also compiles against views predating the unit.
                let mut view: OrderBookView = serde_json::from_value(serde_json::json!({
                    "quantity_unit": unit,
                }))
                .unwrap();
                view.profile = profile;
                view.filter.min_size = minimum;
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(rect),
                        events: vec![egui::Event::PointerMoved(pos2(x, map.price_to_y(price)))],
                        ..Default::default()
                    },
                    |ui| {
                        render_book_profile(
                            ui.painter(),
                            &map,
                            Some(&book),
                            orders.then_some(&queue),
                            0.25,
                            &view,
                            [Color32::GREEN, Color32::RED],
                        )
                        .unwrap();
                    },
                );
                output.textures_delta.clear();
                let texts: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        Shape::Text(t) => Some(t.galley.text()),
                        _ => None,
                    })
                    .collect();
                let unit = unit
                    .filter(|unit| !unit.trim().is_empty())
                    .unwrap_or("feed units");
                assert!(texts.contains(&format!("{measure} ({unit})\nFull width = 2000 {unit}\nAll supplied levels, after filtering").as_str()), "{texts:?}");
                let hover = format!("{side} · Price {price}\n{measure}: {size} {unit}");
                assert_eq!(texts.contains(&hover.as_str()), x >= 740.0, "{texts:?}");
                // Hand calculation: a 1000-lot shelf against an offscreen 2000-lot max is
                // 60 * 1000 / 2000 = 30 points, or 18 for the 600-lot measure.
                if price == 101.25 {
                    let width = 60.0 * size.parse::<f32>().unwrap() / 2000.0;
                    assert!(output.shapes.iter().any(|s| matches!(&s.shape,
                        Shape::Rect(r) if r.rect.contains(pos2(799.0, map.price_to_y(price)))
                            && (r.rect.width() - width).abs() < 0.001
                    )));
                }
            }
        }
    }
}
