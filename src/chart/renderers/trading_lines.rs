//! Order, bracket and position lines from a broker's state. Changes are host intents: rendering
//! never changes an order or a position.

use egui::{Color32, CursorIcon, FontId, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::chart::coords::ChartMapping;
use crate::chart::rendering::render_price_line;
use crate::model::{LineStyle, Order, OrderRequest, OrderSide, OrderType, Position, PriceLine};
use crate::theme::ThemeExt;

/// What the user asked for on a trading line. Confirm it with the user, then send it to the
/// broker.
#[derive(Clone, Debug, PartialEq)]
pub enum TradingLineAction {
    /// Cancel the working order.
    Cancel(Order),
    /// Replace the working order with the request: the user dragged its price, its take profit
    /// or its stop loss, or removed a bracket.
    Modify(Order, OrderRequest),
    /// Close the position at the market.
    Close(Position),
    /// Turn the position into one of the same size on the other side, at the market.
    Reverse(Position),
}

/// What [`render_trading_lines`] drew, and what the user asked for.
pub struct TradingLines {
    /// What the user asked for on a line in this frame.
    pub action: Option<TradingLineAction>,
    /// The price and colour of each line drawn, a dragged line at its new price, for tags on the
    /// price axis: for example as [`PriceLine`]s of width 0 passed to
    /// [`Chart::set_price_lines`](crate::Chart::set_price_lines) with the chart's other lines.
    pub tags: Vec<(f64, Color32)>,
}

/// What [`render_trading_lines`] draws and offers.
pub struct TradingLineOptions<'a> {
    /// Order and bracket lines can be dragged, and brackets removed.
    pub modify: bool,
    /// Position lines offer close and reverse buttons.
    pub close: bool,
    /// The last price, for a position's profit or loss when the broker reports none.
    pub last_price: Option<f64>,
    /// The profit or loss of one unit moving by one price unit, in the account's currency.
    pub point_value: f64,
    /// The account's currency, such as "USD".
    pub currency: &'a str,
    /// The positive tick size used to round dragged prices.
    pub tick_size: f64,
}

#[derive(Clone, Copy, PartialEq)]
enum Level {
    Entry,
    TakeProfit,
    StopLoss,
}

#[derive(Clone, Copy, PartialEq)]
enum Button {
    Cancel,
    Reverse,
}

const HEIGHT: f32 = 18.0;
const PADDING: f32 = 6.0;

/// Draws the working orders with their brackets and the open positions of one symbol on the
/// chart, and returns what the user asked for. Pass only the orders and positions of the
/// chart's symbol, and call it after drawing the chart. The price axis is left to the caller; see
/// [`TradingLines::tags`].
pub fn render_trading_lines<'a>(
    ui: &mut Ui,
    mapping: &ChartMapping,
    orders: impl IntoIterator<Item = &'a Order>,
    positions: impl IntoIterator<Item = &'a Position>,
    options: &TradingLineOptions<'_>,
) -> TradingLines {
    let theme = ui.ctx().active_theme();
    let side_color = |side| match side {
        OrderSide::Buy => theme.ui.accent,
        OrderSide::Sell => theme.ui.error,
    };
    let money = |value: f64| {
        let sign = if value < 0.0 { "-" } else { "+" };
        format!("{sign}{:.2} {}", value.abs(), options.currency)
    };
    let mut action = None;
    let mut tags = Vec::new();

    for position in positions {
        let side = if position.quantity > 0.0 {
            OrderSide::Buy
        } else {
            OrderSide::Sell
        };
        let pnl = position.unrealized_pnl.or_else(|| {
            options.last_price.map(|last| {
                (last - position.average_price) * position.quantity * options.point_value
            })
        });
        let pnl_color = match pnl {
            Some(pnl) if pnl < 0.0 => theme.ui.error,
            _ => theme.ui.success,
        };
        let line =
            PriceLine::new(position.average_price, side_color(side)).with_style(LineStyle::Solid);
        let label = Label {
            id: ("trading_position", &position.symbol),
            body: pnl.map_or_else(String::new, money),
            body_color: pnl_color,
            quantity: position.quantity.abs(),
            buttons: if options.close {
                &[Button::Reverse, Button::Cancel]
            } else {
                &[]
            },
            draggable: false,
        };
        let (_, pressed) = label.show(ui, mapping, options, &line, &mut tags);
        action = action.or(match pressed {
            Some(Button::Cancel) => Some(TradingLineAction::Close(position.clone())),
            Some(Button::Reverse) => Some(TradingLineAction::Reverse(position.clone())),
            None => None,
        });
    }

    for order in orders {
        let request = &order.request;
        let open = request.quantity - order.filled;
        let kind = match request.order_type {
            OrderType::Limit(_) => "LMT",
            OrderType::Stop(_) => "STP",
            OrderType::StopLimit { .. } => "STP LMT",
            _ => "MKT",
        };
        let levels = [
            (Level::Entry, entry_price(&request.order_type)),
            (Level::TakeProfit, request.take_profit),
            (Level::StopLoss, request.stop_loss),
        ];
        for (level, price) in levels {
            let Some(price) = price.filter(|price| price.is_finite()) else {
                continue;
            };
            let (body, color) = match level {
                Level::Entry => (
                    format!("{} {kind}", request.side.label()),
                    side_color(request.side),
                ),
                Level::TakeProfit => ("TP".to_owned(), theme.ui.success),
                Level::StopLoss => ("SL".to_owned(), theme.ui.error),
            };
            let line = PriceLine::new(price, color).with_style(if level == Level::Entry {
                LineStyle::Dashed
            } else {
                LineStyle::Dotted
            });
            // Removing a bracket changes the order, so it needs the same support as a drag.
            let cancel = level == Level::Entry || options.modify;
            let label = Label {
                id: ("trading_order", &order.id.0),
                body,
                body_color: color,
                quantity: open,
                buttons: if cancel { &[Button::Cancel] } else { &[] },
                draggable: options.modify,
            };
            let (dragged, pressed) = ui
                .push_id(level as u8, |ui| {
                    label.show(ui, mapping, options, &line, &mut tags)
                })
                .inner;
            let price = match (dragged, pressed) {
                (Some(price), _) => Some(price),
                (None, Some(Button::Cancel)) if level == Level::Entry => {
                    action = action.or(Some(TradingLineAction::Cancel(order.clone())));
                    continue;
                }
                (None, Some(_)) => None,
                (None, None) => continue,
            };
            let mut changed = request.clone();
            match level {
                Level::Entry => {
                    if let Some(price) = price {
                        let delta = price - entry_price(&request.order_type).unwrap_or(price);
                        changed.order_type = match request.order_type {
                            OrderType::Limit(_) => OrderType::Limit(price),
                            OrderType::Stop(_) => OrderType::Stop(price),
                            OrderType::StopLimit { limit, .. } => OrderType::StopLimit {
                                stop: price,
                                limit: limit + delta,
                            },
                            other => other,
                        };
                        changed.take_profit = changed.take_profit.map(|exit| exit + delta);
                        changed.stop_loss = changed.stop_loss.map(|exit| exit + delta);
                    }
                }
                Level::TakeProfit => changed.take_profit = price,
                Level::StopLoss => changed.stop_loss = price,
            }
            action = action.or(Some(TradingLineAction::Modify(order.clone(), changed)));
        }
    }
    TradingLines { action, tags }
}

/// The price an order waits for: its limit or stop price, or the stop of a stop-limit order.
fn entry_price(order_type: &OrderType) -> Option<f64> {
    match *order_type {
        OrderType::Limit(price)
        | OrderType::Stop(price)
        | OrderType::StopLimit { stop: price, .. } => Some(price),
        _ => None,
    }
}

/// The label at the right end of a trading line: a description, the open quantity and buttons.
struct Label<'a> {
    id: (&'a str, &'a str),
    body: String,
    body_color: Color32,
    quantity: f64,
    buttons: &'a [Button],
    draggable: bool,
}

impl Label<'_> {
    /// Draws the line and its label. Returns the price a finished drag ended at, and the button
    /// the user clicked.
    fn show(
        &self,
        ui: &mut Ui,
        mapping: &ChartMapping,
        options: &TradingLineOptions<'_>,
        line: &PriceLine,
        tags: &mut Vec<(f64, Color32)>,
    ) -> (Option<f64>, Option<Button>) {
        let key = ui.scope_id().with(&self.id);
        let staged = ui
            .ctx()
            .data(|data| data.get_temp::<f64>(key))
            .unwrap_or(line.price);
        let y = mapping.price_to_y(staged);
        if !y.is_finite() || !mapping.is_y_visible(y) {
            return (None, None);
        }
        let font = FontId::proportional(11.0);
        let painter = ui.painter().clone();
        let layout = |text: String, color| painter.layout_no_wrap(text, font.clone(), color);
        let body = (!self.body.is_empty()).then(|| layout(self.body.clone(), self.body_color));
        let quantity = layout(format!("{}", self.quantity), Color32::WHITE);
        let width = |galley: &egui::Galley| galley.size().x + 2.0 * PADDING;
        let total = body.as_deref().map_or(0.0, width)
            + width(&quantity)
            + HEIGHT * self.buttons.len() as f32;
        let mut x = mapping.rect.right() - 8.0 - total;
        let rect = Rect::from_min_size(pos2(x, y - HEIGHT / 2.0), vec2(total, HEIGHT));

        let mut dragged = None;
        let mut value = staged;
        let grab = ui.interact(
            Rect::from_min_max(
                pos2(mapping.rect.left(), rect.top()),
                pos2(rect.max.x - HEIGHT * self.buttons.len() as f32, rect.max.y),
            )
            .intersect(mapping.rect),
            key,
            if self.draggable {
                Sense::drag()
            } else {
                Sense::hover()
            },
        );
        if self.draggable {
            if grab.hovered() || grab.dragged() {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
            }
            if grab.dragged()
                && let Some(pointer) = grab.interact_pointer_pos()
            {
                let price =
                    mapping.y_to_price(pointer.y.clamp(mapping.rect.top(), mapping.rect.bottom()));
                value = (price / options.tick_size).round() * options.tick_size;
                if value.is_finite() {
                    ui.ctx().data_mut(|data| data.insert_temp(key, value));
                }
            }
            if grab.drag_stopped() {
                ui.ctx().data_mut(|data| data.remove::<f64>(key));
                if value.is_finite() && value != line.price {
                    dragged = Some(value);
                }
            }
        }
        let line = PriceLine {
            price: value,
            ..line.clone()
        };
        let clipped = painter.with_clip_rect(mapping.rect);
        render_price_line(
            &clipped,
            mapping.rect,
            &mapping.price,
            &line,
            mapping.rect.left(),
            false,
            f32::INFINITY,
        );
        tags.push((value, line.color));

        let y = mapping.price_to_y(value);
        let rect = rect.translate(vec2(0.0, y - rect.center().y));
        let stroke = Stroke::new(1.0, line.color);
        clipped.rect_filled(rect, 2.0, ui.active_theme().ui.panel_bg);
        clipped.rect_stroke(rect, 2.0, stroke, egui::StrokeKind::Inside);
        x = rect.left();
        if let Some(body) = body {
            let w = width(&body);
            clipped.galley(
                pos2(x + PADDING, y - body.size().y / 2.0),
                body,
                self.body_color,
            );
            x += w;
        }
        let w = width(&quantity);
        clipped.rect_filled(
            Rect::from_min_size(pos2(x, rect.top()), vec2(w, HEIGHT)),
            0.0,
            line.color,
        );
        clipped.galley(
            pos2(x + PADDING, y - quantity.size().y / 2.0),
            quantity,
            Color32::WHITE,
        );
        x += w;

        let mut pressed = None;
        for (index, button) in self.buttons.iter().enumerate() {
            let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(HEIGHT, HEIGHT));
            x += HEIGHT;
            clipped.vline(cell.left(), cell.y_range(), stroke);
            let response = ui
                .interact(
                    cell.intersect(mapping.rect),
                    key.with(index),
                    Sense::click(),
                )
                .on_hover_text(match (button, self.buttons.len()) {
                    (Button::Cancel, 1) => "Cancel",
                    (Button::Cancel, _) => "Close",
                    (Button::Reverse, _) => "Reverse",
                });
            if response.hovered() {
                clipped.rect_filled(cell.shrink(1.0), 1.0, line.color.gamma_multiply(0.2));
            }
            if response.clicked() {
                pressed = Some(*button);
            }
            paint_glyph(&clipped, *button, cell.center(), line.color);
        }
        (dragged, pressed)
    }
}

/// A cross for cancel and close, two opposed arrows for reverse.
fn paint_glyph(painter: &egui::Painter, button: Button, center: Pos2, color: Color32) {
    let stroke = Stroke::new(1.5, color);
    let r = 3.5;
    match button {
        Button::Cancel => {
            painter.line_segment([center + vec2(-r, -r), center + vec2(r, r)], stroke);
            painter.line_segment([center + vec2(-r, r), center + vec2(r, -r)], stroke);
        }
        Button::Reverse => {
            for (dx, dir) in [(-2.5, -1.0), (2.5, 1.0)] {
                let top = center + vec2(dx, dir * -r);
                let tip = center + vec2(dx, dir * r);
                painter.line_segment([top, tip], stroke);
                painter.line_segment([tip, tip + vec2(-2.0, dir * -2.0)], stroke);
                painter.line_segment([tip, tip + vec2(2.0, dir * -2.0)], stroke);
            }
        }
    }
}
