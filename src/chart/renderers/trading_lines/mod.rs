//! Order, bracket and position lines from a broker's state. Changes are host intents: rendering
//! never changes an order or a position.

use egui::{Color32, CursorIcon, FontId, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::chart::coords::ChartMapping;
use crate::chart::rendering::{draw_dashed_polyline, render_price_line};
use crate::localization as l10n;
use crate::model::{
    LineStyle, Order, OrderRequest, OrderSide, OrderType, Position, PositionBracket, PriceLine,
};
use crate::theme::{Theme, ThemeExt};

/// What the user asked for on a trading line. Confirm it with the user, then send it to the
/// broker.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum TradingLineAction {
    /// Cancel the working order.
    Cancel(Order),
    /// Replace the working order with the request: the user dragged its price, a take profit or
    /// its stop loss, removed one of them, or dragged a take profit or stop loss out of its label.
    Modify(Order, OrderRequest),
    /// Close the position at the market.
    Close(Position),
    /// Turn the position into one of the same size on the other side, at the market.
    Reverse(Position),
    /// Place the order the user dragged out of the position's TP or SL button: a limit order
    /// for a take profit or a stop order for a stop loss, on the other side for the position's
    /// size.
    Exit(Position, OrderRequest),
}

/// What a profit or loss on the trading lines is written in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PnlUnit {
    /// In the account's currency, such as "+12.50 USD".
    #[default]
    Money,
    /// In ticks of the symbol's price, such as "+25 ticks".
    Ticks,
    /// As a percentage of the entry price, such as "+0.42%".
    Percent,
}

impl PnlUnit {
    /// Every unit, in the order a menu lists them.
    pub const ALL: [Self; 3] = [Self::Money, Self::Ticks, Self::Percent];

    /// The unit's name in menus.
    pub fn label(self) -> &'static str {
        match self {
            Self::Money => "Money",
            Self::Ticks => "Ticks",
            Self::Percent => "Percent",
        }
    }
}

/// Horizontal placement of trading line labels inside the price pane.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum TradingLabelAlignment {
    /// Place labels beside the left edge.
    Left,
    /// Centre labels in the pane.
    Center,
    /// Place labels near the right edge.
    #[default]
    Right,
}

impl TradingLabelAlignment {
    fn left(self, rect: Rect, width: f32, inset: f32) -> f32 {
        let left = rect.left() + 8.0;
        let right = rect.right() - inset - width;
        if right < left {
            return right;
        }
        match self {
            Self::Left => left,
            Self::Center => (rect.center().x - width / 2.0).clamp(left, right),
            Self::Right => right,
        }
    }
}

/// What [`render_trading_lines`] drew, and what the user asked for.
pub struct TradingLines {
    /// What the user asked for on a line in this frame.
    pub action: Option<TradingLineAction>,
    /// The price and colour of each line drawn, a dragged line at its new price, for tags on the
    /// price axis: for example as [`PriceLine`]s of width 0 passed to
    /// [`Chart::set_price_lines`](crate::Chart::set_price_lines) with the chart's other lines.
    pub tags: Vec<(f64, Color32)>,
    /// Indices in `tags` that use chart background and a coloured outline.
    pub outlined: Vec<usize>,
    /// The projected order whose submit button the user pressed.
    pub submitted: Option<OrderRequest>,
}

/// What [`render_trading_lines`] draws and offers.
#[derive(Clone)]
pub struct TradingLineOptions<'a> {
    /// Draw orders and their brackets. Hidden orders still identify existing position exits.
    pub show_orders: bool,
    /// Draw positions. Hidden positions still supply their brackets' entry price.
    pub show_positions: bool,
    /// Extend lines to the left edge instead of starting at their labels.
    pub extend_lines: bool,
    /// Horizontal placement of order and position labels.
    pub label_alignment: TradingLabelAlignment,
    /// Treat the orders as projected, not yet placed: their entry lines are dashed and they offer
    /// a Buy or Sell submit button.
    pub submit: bool,
    /// Order and bracket lines can be dragged, and brackets removed.
    pub modify: bool,
    /// Position labels offer a close button, and a reverse button with
    /// [`reverse`](Self::reverse).
    pub close: bool,
    /// Hovering a position's label shows its reverse button. Needs [`close`](Self::close).
    pub reverse: bool,
    /// Position labels offer a TP button, dragged out to place a take profit as a limit order.
    pub take_profit: bool,
    /// Position labels offer an SL button, dragged out to place a stop loss as a stop order.
    pub stop_loss: bool,
    /// Labels of orders with an entry price and no position bracket offer a TP button while the
    /// order has no take profit, and an SL button while it has no stop loss. Dragging one out
    /// places that exit at the pointer's price and asks for it as a
    /// [`TradingLineAction::Modify`].
    pub order_exits: bool,
    /// A position's label gives its profit or loss in this unit, or none.
    pub pnl: Option<PnlUnit>,
    /// A take profit or stop loss gives, in this unit, the profit or loss its order makes if
    /// the price reaches it, or none.
    pub bracket_pnl: Option<PnlUnit>,
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
    Submit,
}

const HEIGHT: f32 = 20.0;
const LABEL_INSET: f32 = 84.0;
const NODE_INSET: f32 = 42.0;
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
    let side_color = |side| crate::theme::trading_side_color(&theme, side);
    let positions: Vec<_> = positions.into_iter().collect();
    let orders: Vec<_> = orders.into_iter().collect();
    let mut action = None;
    let mut tags = Vec::new();
    let mut outlined = Vec::new();
    let mut submitted = None;

    for position in positions.iter().filter(|_| options.show_positions) {
        let mut position_options = options.clone();
        for order in &orders {
            if order.request.symbol == position.symbol
                && (order.request.side == OrderSide::Buy) != (position.quantity > 0.0)
            {
                match order.request.position_bracket {
                    Some(PositionBracket::TakeProfit) => position_options.take_profit = false,
                    Some(PositionBracket::StopLoss) => position_options.stop_loss = false,
                    None => {}
                }
            }
        }
        let pressed = show_position(ui, mapping, &position_options, position, &mut tags);
        action = action.or(pressed);
    }

    for order in orders.into_iter().filter(|_| options.show_orders) {
        let request = &order.request;
        let open = request.quantity - order.filled;
        let kind = match request.order_type {
            OrderType::Limit(_) => "Limit",
            OrderType::Stop(_) => "Stop",
            OrderType::StopLimit { .. } => "Stop limit",
            _ => "Market",
        };
        let parent = request.position_bracket.and_then(|_| {
            positions.iter().find(|p| {
                p.symbol == request.symbol && (p.quantity > 0.0) != (request.side == OrderSide::Buy)
            })
        });
        let entry_level = match request.position_bracket {
            Some(PositionBracket::TakeProfit) => Level::TakeProfit,
            Some(PositionBracket::StopLoss) => Level::StopLoss,
            None => Level::Entry,
        };
        let initial_entry = entry_price(&request.order_type);
        // Each level: its kind, its index among the take-profit levels, its price and quantity.
        let mut levels = vec![(Level::Entry, 0, initial_entry, open)];
        if request.take_profit_levels.is_empty() {
            levels.push((Level::TakeProfit, 0, request.take_profit, open));
        }
        levels.extend(
            request
                .take_profit_levels
                .iter()
                .enumerate()
                .map(|(index, take_profit)| {
                    (
                        Level::TakeProfit,
                        index,
                        Some(take_profit.price),
                        take_profit.quantity,
                    )
                }),
        );
        levels.push((Level::StopLoss, 0, request.stop_loss, open));
        let mut parent_price = initial_entry;
        for (level, index, price, quantity) in levels {
            let Some(price) = price.filter(|price| price.is_finite()) else {
                continue;
            };
            let price = if level != Level::Entry {
                price
                    + parent_price
                        .zip(initial_entry)
                        .map_or(0.0, |(moved, original)| moved - original)
            } else {
                price
            };
            let appearance = if level == Level::Entry {
                entry_level
            } else {
                level
            };
            let (name, color) = match appearance {
                Level::Entry => (l10n::text(ui.ctx(), kind), side_color(request.side)),
                Level::TakeProfit => (l10n::text(ui.ctx(), "TP"), theme.ui.take_profit),
                Level::StopLoss => (l10n::text(ui.ctx(), "SL"), theme.ui.stop_loss),
            };
            // A bracket's result if it is reached, from the entry or, for a market order, the
            // last price.
            let entry = parent
                .map(|p| p.average_price)
                .or(parent_price)
                .or(options.last_price);
            let ctx = ui.ctx().clone();
            let body = |price: f64| match (appearance, options.bracket_pnl, entry) {
                (Level::TakeProfit | Level::StopLoss, Some(unit), Some(entry)) => {
                    let gain = (price - entry)
                        * parent.map_or(direction(request.side), |p| p.quantity.signum());
                    let money = gain * quantity * options.point_value;
                    pnl_text(&ctx, unit, money, gain, entry, options).0
                }
                _ => name.clone(),
            };
            let line = PriceLine::new(price, color).with_style(
                if appearance == Level::Entry && options.submit {
                    LineStyle::Dashed
                } else {
                    LineStyle::Solid
                },
            );
            // Removing a bracket changes the order, so it needs the same support as a drag.
            let cancel = level == Level::Entry || options.modify;
            // What an exit dragged out of the entry's label makes if the price reaches it.
            let exit_pnl = |unit, at: f64| {
                let gain = (at - price) * direction(request.side);
                let money = gain * open * options.point_value;
                pnl_text(&ctx, unit, money, gain, price, options).0
            };
            let exits = if level == Level::Entry
                && options.order_exits
                && request.position_bracket.is_none()
                && initial_entry.is_some()
            {
                offered_exits(
                    request.take_profit.is_none() && request.take_profit_levels.is_empty(),
                    request.stop_loss.is_none(),
                )
            } else {
                &[]
            };
            let label = Label {
                id: ("trading_order", &order.id.0),
                body: &body,
                body_color: color,
                quantity,
                buttons: if cancel { &[Button::Cancel] } else { &[] },
                draggable: options.modify,
                submit: (options.submit && level == Level::Entry).then_some(request.side),
                exits,
                exit_pnl: &exit_pnl,
            };
            let first_tag = tags.len();
            let (dragged, pressed, released) = ui
                .push_id((level as u8, index), |ui| {
                    label.show(ui, mapping, options, &line, &mut tags)
                })
                .inner;
            let drawn = tags.get(first_tag).map_or(price, |tag| tag.0);
            if appearance == Level::Entry && tags.len() > first_tag {
                outlined.push(first_tag);
                parent_price = Some(drawn);
            }
            if let Some(parent) = parent {
                connector(
                    ui,
                    mapping,
                    parent.average_price,
                    drawn,
                    side_color(if parent.quantity > 0.0 {
                        OrderSide::Buy
                    } else {
                        OrderSide::Sell
                    }),
                );
            } else if level != Level::Entry
                && let Some(entry) = entry
            {
                connector(ui, mapping, entry, drawn, side_color(request.side));
            }
            if tags.len() == first_tag {
                continue;
            }
            if let Some((exit, price)) = released {
                let mut changed = request.clone();
                if exit == Level::TakeProfit {
                    changed.take_profit = Some(price);
                } else {
                    changed.stop_loss = Some(price);
                }
                action = action.or(Some(TradingLineAction::Modify(order.clone(), changed)));
                continue;
            }
            let price = match (dragged, pressed) {
                (_, Some(Button::Submit)) => {
                    submitted = Some(request.clone());
                    continue;
                }
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
                        // The other levels keep their distance, written at the tick's decimals.
                        let scale = 10f64.powi(decimals(options.tick_size) as i32);
                        let shift = |other: f64| ((other + delta) * scale).round() / scale;
                        changed.order_type = match request.order_type {
                            OrderType::Limit(_) => OrderType::Limit(price),
                            OrderType::Stop(_) => OrderType::Stop(price),
                            OrderType::StopLimit { limit, .. } => OrderType::StopLimit {
                                stop: price,
                                limit: shift(limit),
                            },
                            other => other,
                        };
                        changed.take_profit = changed.take_profit.map(shift);
                        for take_profit in &mut changed.take_profit_levels {
                            take_profit.price = shift(take_profit.price);
                        }
                        changed.stop_loss = changed.stop_loss.map(shift);
                    }
                }
                Level::TakeProfit if changed.take_profit_levels.is_empty() => {
                    changed.take_profit = price;
                }
                Level::TakeProfit => match price {
                    Some(price) => changed.take_profit_levels[index].price = price,
                    None => remove_take_profit_level(&mut changed, index),
                },
                Level::StopLoss => changed.stop_loss = price,
            }
            action = action.or(Some(TradingLineAction::Modify(order.clone(), changed)));
        }
    }
    TradingLines {
        action,
        tags,
        outlined,
        submitted,
    }
}

/// Removes a take-profit level, adding its quantity to the level before it, or after it for the
/// first. The last level left becomes the order's single take profit.
fn remove_take_profit_level(request: &mut OrderRequest, index: usize) {
    let levels = &mut request.take_profit_levels;
    let removed = levels.remove(index);
    if let Some(neighbour) = levels.get_mut(index.saturating_sub(1)) {
        let places = decimals(removed.quantity).max(decimals(neighbour.quantity));
        let scale = 10f64.powi(places as i32);
        neighbour.quantity = ((neighbour.quantity + removed.quantity) * scale).round() / scale;
    }
    if let [only] = levels.as_slice() {
        request.take_profit = Some(only.price);
        levels.clear();
    }
}

/// 1 for a buy and -1 for a sell: the sign of a price move in the side's favour.
fn direction(side: OrderSide) -> f64 {
    match side {
        OrderSide::Buy => 1.0,
        OrderSide::Sell => -1.0,
    }
}

/// A profit or loss of `money` in the account's currency, or of `gain` in price from `entry`,
/// written in `unit`, and the value written, whose sign colours it.
fn pnl_text(
    ctx: &egui::Context,
    unit: PnlUnit,
    money: f64,
    gain: f64,
    entry: f64,
    options: &TradingLineOptions<'_>,
) -> (String, f64) {
    let round = |value: f64, decimals: i32| {
        let scale = 10f64.powi(decimals);
        (value * scale).round() / scale
    };
    let (value, text) = match unit {
        PnlUnit::Money => {
            let value = round(money, 2);
            (value, format!("{:.2} {}", value.abs(), options.currency))
        }
        PnlUnit::Ticks => {
            let value = (gain / options.tick_size).round();
            (
                value,
                l10n::format_text(ctx, "{0} ticks", &[("0", value.abs().to_string())]),
            )
        }
        PnlUnit::Percent => {
            let value = round(gain / entry * 100.0, 2);
            (value, format!("{:.2}%", value.abs()))
        }
    };
    let sign = if value > 0.0 {
        "+"
    } else if value < 0.0 {
        "-"
    } else {
        ""
    };
    (format!("{sign}{}", text.trim_end()), value)
}

/// Draws a position's line at its average price and its label at the right of the plot: from
/// left to right a reverse button while the label is hovered, TP and SL buttons, the quantity,
/// the profit or loss and a close button. Dragging TP or SL out of the label shows the exit at
/// the pointer's price; releasing it asks for that exit order. Returns what the user asked for.
fn show_position(
    ui: &mut Ui,
    mapping: &ChartMapping,
    options: &TradingLineOptions<'_>,
    position: &Position,
    tags: &mut Vec<(f64, Color32)>,
) -> Option<TradingLineAction> {
    let theme = ui.ctx().active_theme();
    let side = if position.quantity > 0.0 {
        OrderSide::Buy
    } else {
        OrderSide::Sell
    };
    let color = crate::theme::trading_side_color(&theme, side);
    let size = position.quantity.abs();
    let entry = position.average_price;
    let y = mapping.price_to_y(entry);
    if !y.is_finite() || !mapping.is_y_visible(y) {
        return None;
    }
    let painter = ui.painter().with_clip_rect(mapping.rect);
    let line = PriceLine::new(entry, color).with_style(LineStyle::Solid);
    tags.push((entry, color));

    let key = ui.scope_id().with(("trading_position", &position.symbol));
    let font = FontId::proportional(ui.ctx().active_theme().text.ohlc);
    let layout = |text: String, color| painter.layout_no_wrap(text, font.clone(), color);
    let width = |galley: &egui::Galley| galley.size().x + 2.0 * PADDING;
    // The result the position makes when the price moves by `gain` in its favour.
    let result = |unit, gain: f64| {
        pnl_text(
            ui.ctx(),
            unit,
            gain * size * options.point_value,
            gain,
            entry,
            options,
        )
    };
    let pnl = options.pnl.and_then(|unit| {
        let gain = options
            .last_price
            .map(|last| (last - entry) * direction(side))
            .or_else(|| {
                position
                    .unrealized_pnl
                    .map(|pnl| pnl / (size * options.point_value))
            })?;
        let (text, value) = match (unit, position.unrealized_pnl) {
            // The broker's own figure, when it reports one.
            (PnlUnit::Money, Some(pnl)) => pnl_text(ui.ctx(), unit, pnl, gain, entry, options),
            _ => result(unit, gain),
        };
        let text_color = if value > 0.0 {
            theme.ui.take_profit
        } else if value < 0.0 {
            theme.ui.error
        } else {
            theme.ui.text_secondary
        };
        Some((layout(text, text_color), text_color))
    });
    let exits = offered_exits(options.take_profit, options.stop_loss);
    let quantity = layout(format!("{size}"), Color32::WHITE);
    let close = if options.close { HEIGHT } else { 0.0 };
    let total = exit_buttons_width(ui, exits)
        + width(&quantity)
        + pnl.as_ref().map_or(0.0, |(galley, _)| width(galley))
        + close;
    let reverse = options.close && options.reverse;
    let label_width = total + if reverse { HEIGHT * 1.6 } else { 0.0 };
    let inset = LABEL_INSET.min((mapping.rect.width() - total - HEIGHT * 1.6).max(8.0));
    let (left, right) = if options.label_alignment == TradingLabelAlignment::Right
        || label_width > mapping.rect.width() - 16.0
    {
        let right = mapping.rect.right() - inset;
        // Preserve the rounding at the edge where the reverse button moves to another row.
        let left = right - total - if reverse { HEIGHT * 1.6 } else { 0.0 };
        (left, right)
    } else {
        let left = options
            .label_alignment
            .left(mapping.rect, label_width, inset);
        (left, left + label_width)
    };
    let label = Rect::from_min_max(pos2(left, y - HEIGHT / 2.0), pos2(right, y + HEIGHT / 2.0));
    render_price_line(
        &painter,
        mapping.rect,
        &mapping.price,
        &line,
        if options.extend_lines {
            mapping.rect.left()
        } else {
            label.left()
        },
        false,
        f32::INFINITY,
    );
    let cell = |x: f32, w: f32| Rect::from_min_size(pos2(x, label.top()), vec2(w, HEIGHT));
    let reverse_rect = if left < mapping.rect.left() {
        let top = if label.bottom() + 4.0 + HEIGHT <= mapping.rect.bottom() {
            label.bottom() + 4.0
        } else {
            label.top() - 4.0 - HEIGHT
        };
        Rect::from_min_size(pos2(right - total, top), vec2(HEIGHT, HEIGHT))
    } else {
        cell(left, HEIGHT)
    };
    let hover_rect = if reverse {
        label.union(reverse_rect)
    } else {
        label
    };
    let hovered = ui.rect_contains_pointer(hover_rect.intersect(mapping.rect));
    let stroke = Stroke::new(1.0, color);
    let mut action = None;

    if reverse && hovered {
        let rect = reverse_rect;
        let response = ui
            .interact(
                rect.intersect(mapping.rect),
                key.with("reverse"),
                Sense::click(),
            )
            .on_hover_text(l10n::text(ui.ctx(), "Reverse position"));
        painter.rect(
            rect,
            4.0,
            label_fill(&theme, color),
            stroke,
            egui::StrokeKind::Inside,
        );
        if response.hovered() {
            painter.rect_filled(rect.shrink(1.0), 1.0, color.gamma_multiply(0.2));
        }
        paint_glyph(&painter, Button::Reverse, rect.center(), color);
        if response.clicked() {
            action = Some(TradingLineAction::Reverse(position.clone()));
        }
    }

    let (mut x, released) = ExitButtons {
        key,
        exits,
        x: right - total,
        y,
        entry,
        line_left: if options.extend_lines {
            mapping.rect.left()
        } else {
            label.left()
        },
        preview_right: right,
        pnl: &|unit, price| result(unit, (price - entry) * direction(side)).0,
    }
    .show(ui, mapping, options, tags);
    if let Some((exit, price)) = released {
        let mut request = OrderRequest::new(
            position.symbol.clone(),
            match side {
                OrderSide::Buy => OrderSide::Sell,
                OrderSide::Sell => OrderSide::Buy,
            },
            size,
            if exit == Level::TakeProfit {
                OrderType::Limit(price)
            } else {
                OrderType::Stop(price)
            },
        );
        request.position_bracket = Some(if exit == Level::TakeProfit {
            PositionBracket::TakeProfit
        } else {
            PositionBracket::StopLoss
        });
        action = action.or(Some(TradingLineAction::Exit(position.clone(), request)));
    }

    if !exits.is_empty() {
        x += HEIGHT * 0.65;
    }
    let group_rect = Rect::from_min_max(pos2(x, label.top()), pos2(right, label.bottom()));
    painter.rect(
        group_rect,
        4.0,
        label_fill(&theme, color),
        stroke,
        egui::StrokeKind::Inside,
    );
    painter.circle_filled(pos2(mapping.rect.right() - NODE_INSET, y), 3.0, color);
    let rect = cell(x, width(&quantity));
    x = rect.right();
    painter.rect_filled(
        rect,
        egui::CornerRadius {
            nw: 4,
            sw: 4,
            ne: 0,
            se: 0,
        },
        color,
    );
    painter.galley(
        pos2(rect.left() + PADDING, y - quantity.size().y / 2.0),
        quantity,
        Color32::WHITE,
    );
    if let Some((pnl, text_color)) = pnl {
        let rect = cell(x, width(&pnl));
        x = rect.right();
        painter.vline(rect.left(), rect.y_range(), stroke);
        painter.galley(
            pos2(rect.left() + PADDING, y - pnl.size().y / 2.0),
            pnl,
            text_color,
        );
    }
    if options.close {
        let rect = cell(x, HEIGHT);
        let response = ui
            .interact(
                rect.intersect(mapping.rect),
                key.with("close"),
                Sense::click(),
            )
            .on_hover_text(l10n::text(ui.ctx(), "Close position"));
        painter.vline(rect.left(), rect.y_range(), stroke);
        if response.hovered() {
            painter.rect_filled(rect.shrink(1.0), 1.0, color.gamma_multiply(0.2));
        }
        paint_glyph(&painter, Button::Cancel, rect.center(), color);
        if response.clicked() {
            action = action.or(Some(TradingLineAction::Close(position.clone())));
        }
    }
    action
}

/// The TP and SL buttons a label offers, from left to right.
fn offered_exits(take_profit: bool, stop_loss: bool) -> &'static [Level] {
    match (take_profit, stop_loss) {
        (true, true) => &[Level::TakeProfit, Level::StopLoss],
        (true, false) => &[Level::TakeProfit],
        (false, true) => &[Level::StopLoss],
        (false, false) => &[],
    }
}

/// An exit's name and colour.
fn exit_style(theme: &Theme, exit: Level) -> (&'static str, Color32) {
    if exit == Level::TakeProfit {
        ("TP", theme.ui.take_profit)
    } else {
        ("SL", theme.ui.stop_loss)
    }
}

/// The width of the TP and SL buttons in `exits` with the gap after them, or 0 without buttons.
fn exit_buttons_width(ui: &Ui, exits: &[Level]) -> f32 {
    let theme = ui.ctx().active_theme();
    let font = FontId::proportional(theme.text.ohlc);
    let buttons = exits
        .iter()
        .map(|exit| {
            let (name, color) = exit_style(&theme, *exit);
            let name = l10n::text(ui.ctx(), name);
            let galley = ui.painter().layout_no_wrap(name, font.clone(), color);
            galley.size().x + 2.0 * PADDING
        })
        .sum::<f32>();
    match exits.len() {
        0 => 0.0,
        // The two buttons share an edge.
        2 => buttons - 1.0 + HEIGHT * 0.65,
        _ => buttons + HEIGHT * 0.65,
    }
}

/// The TP and SL buttons on a label's row.
struct ExitButtons<'a> {
    key: egui::Id,
    /// From left to right.
    exits: &'a [Level],
    /// The left edge of the first button.
    x: f32,
    /// The middle of the row.
    y: f32,
    /// A dragged exit asks for nothing at this price.
    entry: f64,
    /// Where a dragged exit's line starts.
    line_left: f32,
    /// The right edge of a dragged exit's label.
    preview_right: f32,
    /// The text after the name on a dragged exit's label, from its price.
    pnl: &'a dyn Fn(PnlUnit, f64) -> String,
}

impl ExitButtons<'_> {
    /// Draws the buttons and handles their drags: while one is dragged, the exit's line and a
    /// label at the pointer's tick price. Returns the right edge of the last button, and the exit
    /// released with its price.
    fn show(
        &self,
        ui: &Ui,
        mapping: &ChartMapping,
        options: &TradingLineOptions<'_>,
        tags: &mut Vec<(f64, Color32)>,
    ) -> (f32, Option<(Level, f64)>) {
        let theme = ui.ctx().active_theme();
        let painter = ui.painter().with_clip_rect(mapping.rect);
        let font = FontId::proportional(theme.text.ohlc);
        let layout = |text: String, color| painter.layout_no_wrap(text, font.clone(), color);
        let width = |galley: &egui::Galley| galley.size().x + 2.0 * PADDING;
        let y = self.y;
        let joined = self.exits.len() == 2;
        let mut x = self.x;
        let mut released = None;
        for &exit in self.exits {
            let (name, exit_color) = exit_style(&theme, exit);
            let galley = layout(l10n::text(ui.ctx(), name), exit_color);
            let rect = Rect::from_min_size(pos2(x, y - HEIGHT / 2.0), vec2(width(&galley), HEIGHT));
            let take_profit = exit == Level::TakeProfit;
            x = rect.right() - if take_profit && joined { 1.0 } else { 0.0 };
            let id = self.key.with(name);
            let response = ui
                .interact(rect.intersect(mapping.rect), id, Sense::drag())
                .on_hover_text(l10n::text(
                    ui.ctx(),
                    if take_profit {
                        "Drag to place a take profit"
                    } else {
                        "Drag to place a stop loss"
                    },
                ));
            if response.hovered() || response.dragged() {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
            }
            let r = rect.shrink(0.5);
            let left_radius = if !take_profit && joined { 0.0 } else { 4.0 };
            let right_radius = if take_profit && joined { 0.0 } else { 4.0 };
            painter.rect_filled(
                r,
                egui::CornerRadius {
                    nw: left_radius as u8,
                    sw: left_radius as u8,
                    ne: right_radius as u8,
                    se: right_radius as u8,
                },
                label_fill(&theme, exit_color),
            );
            draw_dashed_polyline(
                &painter,
                &[
                    r.left_top() + vec2(left_radius, 0.0),
                    r.right_top() - vec2(right_radius, 0.0),
                    r.right_top() + vec2(-right_radius / 4.0, right_radius / 4.0),
                    r.right_top() + vec2(0.0, right_radius),
                    r.right_bottom() - vec2(0.0, right_radius),
                    r.right_bottom() - vec2(right_radius / 4.0, right_radius / 4.0),
                    r.right_bottom() - vec2(right_radius, 0.0),
                    r.left_bottom() + vec2(left_radius, 0.0),
                    r.left_bottom() + vec2(left_radius / 4.0, -left_radius / 4.0),
                    r.left_bottom() - vec2(0.0, left_radius),
                    r.left_top() + vec2(0.0, left_radius),
                    r.left_top() + vec2(left_radius / 4.0, left_radius / 4.0),
                    r.left_top() + vec2(left_radius, 0.0),
                ],
                Stroke::new(1.0, exit_color),
                1.0,
                2.0,
            );
            painter.galley(
                pos2(rect.left() + PADDING, y - galley.size().y / 2.0),
                galley,
                exit_color,
            );
            let price = response
                .interact_pointer_pos()
                .filter(|_| response.dragged() || response.drag_stopped())
                .map(|pointer| {
                    let y = pointer.y.clamp(mapping.rect.top(), mapping.rect.bottom());
                    to_step(mapping.y_to_price(y), options.tick_size, f64::round)
                })
                .filter(|price| price.is_finite() && *price != self.entry);
            let Some(price) = price else {
                continue;
            };
            if response.drag_stopped() {
                released = Some((exit, price));
                continue;
            }
            // The exit being dragged: its line and a label like a bracket's.
            let line = PriceLine::new(price, exit_color).with_style(LineStyle::Solid);
            render_price_line(
                &painter,
                mapping.rect,
                &mapping.price,
                &line,
                self.line_left,
                false,
                f32::INFINITY,
            );
            tags.push((price, exit_color));
            let text = match options.bracket_pnl {
                Some(unit) => format!("{} {}", l10n::text(ui.ctx(), name), (self.pnl)(unit, price)),
                None => l10n::text(ui.ctx(), name),
            };
            let text = layout(text, exit_color);
            let y = mapping.price_to_y(price);
            let preview = Rect::from_min_size(
                pos2(self.preview_right - width(&text), y - HEIGHT / 2.0),
                vec2(width(&text), HEIGHT),
            );
            painter.rect(
                preview,
                2.0,
                label_fill(&theme, exit_color),
                Stroke::new(1.0, exit_color),
                egui::StrokeKind::Inside,
            );
            painter.galley(
                pos2(preview.left() + PADDING, y - text.size().y / 2.0),
                text,
                exit_color,
            );
        }
        (x, released)
    }
}

/// `value` as the whole multiple of `step` that `round` picks from `value / step`, written at the
/// step's decimals.
pub(crate) fn to_step(value: f64, step: f64, round: impl Fn(f64) -> f64) -> f64 {
    let scale = 10f64.powi(decimals(step) as i32);
    (round(value / step) * step * scale).round() / scale
}

/// Decimals needed to write `step`, such as 2 for 0.25.
pub(crate) fn decimals(step: f64) -> usize {
    let text = format!("{step}");
    text.split_once('.')
        .map_or(0, |(_, fraction)| fraction.len())
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

/// The label at the right end of a trading line: TP and SL buttons, the open quantity, a
/// description and buttons.
struct Label<'a> {
    id: (&'a str, &'a str),
    /// The description at the line's price.
    body: &'a dyn Fn(f64) -> String,
    body_color: Color32,
    quantity: f64,
    buttons: &'a [Button],
    draggable: bool,
    submit: Option<OrderSide>,
    /// The TP and SL buttons before the label.
    exits: &'a [Level],
    /// The text after the name on a label of an exit dragged out of `exits`, from its price.
    exit_pnl: &'a dyn Fn(PnlUnit, f64) -> String,
}

impl Label<'_> {
    /// Draws the line and its label. Returns the price a finished drag ended at, the button the
    /// user clicked, and the exit dragged out of the label with its price.
    fn show(
        &self,
        ui: &mut Ui,
        mapping: &ChartMapping,
        options: &TradingLineOptions<'_>,
        line: &PriceLine,
        tags: &mut Vec<(f64, Color32)>,
    ) -> (Option<f64>, Option<Button>, Option<(Level, f64)>) {
        let key = ui.scope_id().with(self.id);
        if !self.draggable {
            ui.ctx().data_mut(|data| data.remove::<f64>(key));
        }
        let staged = ui.ctx().data(|data| data.get_temp::<f64>(key));
        let y = mapping.price_to_y(staged.unwrap_or(line.price));
        if !y.is_finite() || (staged.is_none() && !mapping.is_y_visible(y)) {
            return (None, None, None);
        }
        // A tick can snap beyond the pane; keep its drag target reachable until release.
        let y = y.clamp(mapping.rect.top(), mapping.rect.bottom());
        let font = FontId::proportional(ui.ctx().active_theme().text.ohlc);
        let painter = ui.painter().clone();
        let layout = |text: String, color| painter.layout_no_wrap(text, font.clone(), color);
        let body = (self.body)(staged.unwrap_or(line.price));
        let body = (!body.is_empty()).then(|| layout(body, self.body_color));
        let quantity = layout(format!("{}", self.quantity), line.color);
        let width = |galley: &egui::Galley| galley.size().x + 2.0 * PADDING;
        let total = body.as_deref().map_or(0.0, width)
            + width(&quantity)
            + HEIGHT * self.buttons.len() as f32;
        let exits_width = exit_buttons_width(ui, self.exits);
        let group = total + exits_width;
        let inset = LABEL_INSET.min((mapping.rect.width() - group - 8.0).max(8.0));
        let mut x = options.label_alignment.left(mapping.rect, group, inset) + exits_width;
        let rect = Rect::from_min_size(pos2(x, y - HEIGHT / 2.0), vec2(total, HEIGHT));

        let mut dragged = None;
        let mut value = staged.unwrap_or(line.price);
        let grab = ui.interact(
            Rect::from_min_max(
                pos2(
                    if options.extend_lines {
                        mapping.rect.left()
                    } else {
                        rect.left()
                    },
                    rect.top(),
                ),
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
        let node = pos2(mapping.rect.right() - NODE_INSET, y);
        let node_response = ui.interact(
            Rect::from_center_size(node, vec2(14.0, HEIGHT)).intersect(mapping.rect),
            key.with("node"),
            if self.draggable {
                Sense::drag()
            } else {
                Sense::hover()
            },
        );
        let grab = grab.union(node_response);
        if self.draggable {
            if grab.hovered() || grab.dragged() {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
            }
            if grab.dragged()
                && let Some(pointer) = grab.interact_pointer_pos()
            {
                let price =
                    mapping.y_to_price(pointer.y.clamp(mapping.rect.top(), mapping.rect.bottom()));
                value = to_step(price, options.tick_size, f64::round);
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
            if options.extend_lines {
                mapping.rect.left()
            } else {
                rect.left()
            },
            false,
            f32::INFINITY,
        );
        tags.push((value, line.color));
        let node = pos2(mapping.rect.right() - NODE_INSET, mapping.price_to_y(value));
        clipped.circle(
            node,
            3.5,
            ui.active_theme().chart.bg,
            Stroke::new(1.0, line.color),
        );

        let y = mapping
            .price_to_y(value)
            .clamp(mapping.rect.top(), mapping.rect.bottom());
        let rect = rect.translate(vec2(0.0, y - rect.center().y));
        let stroke = Stroke::new(1.0, line.color);
        clipped.rect(
            rect,
            4.0,
            label_fill(&ui.active_theme(), line.color),
            stroke,
            egui::StrokeKind::Inside,
        );
        x = rect.left();
        let w = width(&quantity);
        clipped.galley(
            pos2(x + PADDING, y - quantity.size().y / 2.0),
            quantity,
            line.color,
        );
        x += w;
        if let Some(body) = body {
            clipped.vline(x, rect.y_range(), stroke);
            let w = width(&body);
            clipped.galley(
                pos2(x + PADDING, y - body.size().y / 2.0),
                body,
                self.body_color,
            );
            x += w;
        }

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
                .on_hover_text(l10n::text(ui.ctx(), "Cancel"));
            if response.hovered() {
                clipped.rect_filled(cell.shrink(1.0), 1.0, line.color.gamma_multiply(0.2));
            }
            if response.clicked() {
                pressed = Some(*button);
            }
            paint_glyph(&clipped, *button, cell.center(), line.color);
        }
        // After the grab area, which runs under them with extended lines: the last widget wins.
        let group_left = rect.left() - exits_width;
        let (_, released) = ExitButtons {
            key,
            exits: self.exits,
            x: group_left,
            y,
            entry: value,
            line_left: if options.extend_lines {
                mapping.rect.left()
            } else {
                rect.left()
            },
            preview_right: rect.right(),
            pnl: self.exit_pnl,
        }
        .show(ui, mapping, options, tags);
        if let Some(side) = self.submit {
            let size = vec2(58.0, HEIGHT);
            let left = group_left - size.x - HEIGHT * 0.65;
            let at = if left >= mapping.rect.left() + 8.0 {
                pos2(left, rect.top())
            } else {
                pos2(
                    group_left.max(mapping.rect.left() + 8.0),
                    rect.bottom() + 4.0,
                )
            };
            let button = Rect::from_min_size(at, size);
            if ui
                .put(
                    button,
                    egui::Button::new(
                        egui::RichText::new(l10n::text(
                            ui.ctx(),
                            if side == OrderSide::Buy {
                                "Buy"
                            } else {
                                "Sell"
                            },
                        ))
                        .color(Color32::WHITE),
                    )
                    .fill(line.color)
                    .stroke(Stroke::NONE),
                )
                .clicked()
            {
                pressed = Some(Button::Submit);
            }
        }
        (dragged, pressed, released)
    }
}

/// The body of a label in `color`: the colour's tint over the chart, opaque so that the line and
/// the bars behind the label do not show through its text.
fn label_fill(theme: &Theme, color: Color32) -> Color32 {
    theme.chart.bg.blend(color.gamma_multiply(0.2))
}

/// A cross for cancel and close, two opposed arrows for reverse.
fn paint_glyph(painter: &egui::Painter, button: Button, center: Pos2, color: Color32) {
    let stroke = Stroke::new(1.5, color);
    let r = 3.5;
    let segment = |a, b| {
        painter.line_segment([a, b], stroke);
        painter.circle_filled(a, 0.75, color);
        painter.circle_filled(b, 0.75, color);
    };
    match button {
        Button::Submit => unreachable!(),
        Button::Cancel => {
            segment(center + vec2(-r, -r), center + vec2(r, r));
            segment(center + vec2(-r, r), center + vec2(r, -r));
        }
        Button::Reverse => {
            let r = 5.0;
            for (dx, dir) in [(-2.5, -1.0), (2.5, 1.0)] {
                let top = center + vec2(dx, dir * -r);
                let tip = center + vec2(dx, dir * r);
                segment(top, tip);
                segment(tip, tip + vec2(-2.0, dir * -2.0));
                segment(tip, tip + vec2(2.0, dir * -2.0));
            }
        }
    }
}

/// A parent-coloured connector with hollow bracket nodes and a filled parent node.
fn connector(ui: &Ui, mapping: &ChartMapping, entry: f64, exit: f64, color: Color32) {
    let painter = ui.painter().with_clip_rect(mapping.rect);
    let x = mapping.rect.right() - NODE_INSET;
    let a = pos2(x, mapping.price_to_y(entry));
    let b = pos2(x, mapping.price_to_y(exit));
    painter.line_segment([a, b], Stroke::new(1.0, color));
    painter.circle(b, 3.5, ui.active_theme().chart.bg, Stroke::new(1.0, color));
    painter.circle_filled(a, 7.0, color.gamma_multiply(0.2));
    painter.circle_filled(a, 3.0, color);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{OrderId, OrderStatus};

    /// Only a projected order's line is dashed, a label's body is an opaque tint, and the submit
    /// button has no outline.
    #[test]
    fn working_lines_are_solid_and_labels_are_opaque_tints() {
        for theme in [Theme::light(), Theme::dark()] {
            let accent = theme.ui.accent;
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
            let order = Order::new(
                OrderId::new("order"),
                OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Limit(95.0)),
                OrderStatus::Working,
            );
            let position = Position::new("SYM", 1.0, 95.0);
            for (submit, is_position) in [(false, false), (true, false), (false, true)] {
                let options = TradingLineOptions {
                    show_orders: true,
                    show_positions: true,
                    extend_lines: true,
                    label_alignment: TradingLabelAlignment::Right,
                    submit,
                    modify: false,
                    close: false,
                    reverse: false,
                    take_profit: false,
                    stop_loss: false,
                    order_exits: false,
                    pnl: None,
                    bracket_pnl: None,
                    last_price: None,
                    point_value: 1.0,
                    currency: "USD",
                    tick_size: 0.01,
                };
                for frame in 0..3 {
                    let mut y = 0.0;
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))),
                            ..Default::default()
                        },
                        |ui| {
                            chart.show(ui);
                            let mapping = chart.get_chart_mapping();
                            y = mapping.price_to_y(95.0);
                            let (orders, positions) = if is_position {
                                (vec![], vec![&position])
                            } else {
                                (vec![&order], vec![])
                            };
                            render_trading_lines(ui, &mapping, orders, positions, &options);
                        },
                    );
                    if frame == 2 {
                        let segments = output
                            .shapes
                            .iter()
                            .filter(|shape| {
                                matches!(&shape.shape, egui::Shape::LineSegment { points, stroke }
                                    if stroke.color == accent
                                        && (points[0].y - y).abs() < 0.1
                                        && (points[1].y - y).abs() < 0.1)
                            })
                            .count();
                        assert_eq!(segments > 1, submit, "{segments} segments");
                        let rects: Vec<_> = output
                            .shapes
                            .iter()
                            .filter_map(|shape| match &shape.shape {
                                egui::Shape::Rect(rect) => Some(rect),
                                _ => None,
                            })
                            .collect();
                        let body = rects
                            .iter()
                            .find(|rect| rect.stroke.color == accent && rect.stroke.width == 1.0)
                            .expect("label body");
                        assert!((19.0..=20.0).contains(&body.rect.height()));
                        assert_ne!(body.fill, theme.chart.bg);
                        assert_eq!(body.fill.a(), 255);
                        if submit {
                            let button = rects
                                .iter()
                                .find(|rect| rect.fill == accent)
                                .expect("submit button");
                            assert_eq!(button.stroke.width, 0.0);
                        }
                    }
                    output.drop_without_applying_deltas();
                }
            }
        }
    }
}
