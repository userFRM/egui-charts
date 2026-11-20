//! Order, bracket and position lines from a broker's state. Changes are host intents: rendering
//! never changes an order or a position.

mod position;

#[cfg(feature = "ui")]
pub(crate) use position::exit_request;
use position::show_position;

use egui::{Color32, CursorIcon, FontId, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::chart::coords::ChartMapping;
use crate::chart::rendering::{draw_dashed_polyline, render_price_line};
use crate::localization as l10n;
use crate::model::{
    Capabilities, LineStyle, Order, OrderId, OrderRequest, OrderSide, OrderType, Position,
    PositionBracket, PriceLine, TimeInForce,
};
use crate::scales::{TimezoneMode, decimals, to_step};
use crate::theme::{Theme, ThemeExt};

/// What the user asked for on a trading line. Confirm a change with the user, then send it to
/// the broker; [`Edit`](Self::Edit) and [`Protect`](Self::Protect) ask for a dialog first.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum TradingLineAction {
    /// Cancel the working order.
    Cancel(Order),
    /// Replace the working order with the request: the user dragged its price, the limit
    /// price of a stop-limit order, a take profit or its stop loss, removed one of them, or
    /// dragged a take profit or stop loss out of its label.
    Modify(Order, OrderRequest),
    /// Close the position at the market.
    Close(Position),
    /// Turn the position into one of the same size on the other side, at the market.
    Reverse(Position),
    /// Place the order the user dragged out of the position's TP or SL button: a limit order
    /// for a take profit or a stop order for a stop loss, on the other side for the position's
    /// size.
    Exit(Position, OrderRequest),
    /// Open the working order's Modify order dialog with this request, including any exit
    /// added from its menu. Send the confirmed request as a [`Modify`](Self::Modify).
    Edit(Order, OrderRequest),
    /// Ask for a take profit, a stop loss or both for the position, then place each confirmed
    /// exit as the order an [`Exit`](Self::Exit) asks for.
    Protect(Position),
}

/// What a profit or loss on the trading lines is written in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[non_exhaustive]
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
#[non_exhaustive]
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
#[derive(Debug)]
#[non_exhaustive]
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

/// The appearance of one order's or position's line. Unset fields keep the theme and
/// [`TradingLineOptions`] defaults. An order's appearance covers its entry and stop-limit
/// limit lines; its take profits and stop loss retain their own appearance.
///
/// ```
/// use egui_charts::chart::{TradingLineAppearance, TradingLineOptions};
/// use egui_charts::model::{LineStyle, OrderId};
///
/// let mut look = TradingLineAppearance::default();
/// look.color = Some(egui::Color32::from_rgb(156, 39, 176));
/// look.line_style = Some(LineStyle::Dashed);
/// look.text = Some("Plan".into());
/// look.hover_text = Some("Planned entry".into());
/// let looks = [(OrderId::new("plan-1"), look)];
/// let mut options = TradingLineOptions::new("USD", 0.25);
/// options.order_appearance = &looks;
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct TradingLineAppearance {
    /// The colour of the line, label and price tag, replacing the side's colour.
    pub color: Option<Color32>,
    /// The line's pattern, replacing solid or the projected order's dashed pattern.
    pub line_style: Option<LineStyle>,
    /// The description replacing an order's type or following a position's quantity.
    pub text: Option<String>,
    /// The text shown while the pointer hovers over the label.
    pub hover_text: Option<String>,
}

/// What [`render_trading_lines`] draws and offers.
#[derive(Debug, Clone)]
#[non_exhaustive]
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
    /// The account's capabilities for secondary-click menus on order and position labels.
    /// Unsupported entries are disabled and explain why. `None` offers no menus.
    pub menu: Option<Capabilities>,
    /// Individual order appearances, keyed by order id. Unlisted orders keep their defaults.
    pub order_appearance: &'a [(OrderId, TradingLineAppearance)],
    /// Individual position appearances, keyed by symbol. Unlisted positions keep their defaults.
    pub position_appearance: &'a [(String, TradingLineAppearance)],
    /// Entry labels of orders without a position bracket, and position labels, show the share
    /// closed by take profits and stop losses: two bars with the shares as hover text.
    pub coverage: bool,
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
    /// The zone in which good-till-date order labels write their expiry.
    pub timezone: TimezoneMode,
}

impl<'a> TradingLineOptions<'a> {
    /// Orders and positions with their labels at the right edge, offering no buttons, drags or
    /// menus, with no profit or loss, no coverage marks, no last price and a
    /// [`point_value`](Self::point_value) of one. `currency` names the account's currency and
    /// `tick_size` rounds dragged prices. Assign the other fields to offer more.
    ///
    /// ```
    /// use egui_charts::chart::TradingLineOptions;
    ///
    /// let mut options = TradingLineOptions::new("USD", 0.25);
    /// options.modify = true;
    /// ```
    pub fn new(currency: &'a str, tick_size: f64) -> Self {
        Self {
            show_orders: true,
            show_positions: true,
            extend_lines: false,
            label_alignment: TradingLabelAlignment::Right,
            submit: false,
            modify: false,
            close: false,
            reverse: false,
            take_profit: false,
            stop_loss: false,
            order_exits: false,
            menu: None,
            order_appearance: &[],
            position_appearance: &[],
            coverage: false,
            pnl: None,
            bracket_pnl: None,
            last_price: None,
            point_value: 1.0,
            currency,
            tick_size,
            timezone: TimezoneMode::Utc,
        }
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Level {
    Entry,
    TakeProfit,
    StopLoss,
    /// The limit price of a stop-limit order, drawn beside its stop.
    Limit,
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

/// Width of each coverage bar.
const COVERAGE_WIDTH: f32 = 10.0;
/// Width of the coverage mark's label cell.
pub(crate) const COVERAGE_CELL: f32 = COVERAGE_WIDTH + 2.0 * PADDING;

/// A closed share from zero to one, full within rounding and zero when undefined.
fn share_of(closed: f64, quantity: f64) -> f64 {
    let share = closed / quantity;
    if !share.is_finite() {
        0.0
    } else if share >= 1.0 - 1e-9 {
        1.0
    } else {
        share.max(0.0)
    }
}

/// The shares of the order's quantity closed by its take profits and stop losses.
pub(crate) fn exit_coverage(request: &OrderRequest) -> (f64, f64) {
    let levels = &request.take_profit_levels;
    let take_profit = if request.take_profit.is_some() {
        1.0
    } else {
        share_of(
            levels.iter().map(|level| level.quantity).sum(),
            request.quantity,
        )
    };
    let stop_loss = if request.stop_loss.is_some() {
        1.0
    } else {
        share_of(
            levels
                .iter()
                .filter(|level| level.stop_loss.is_some())
                .map(|level| level.quantity)
                .sum(),
            request.quantity,
        )
    };
    (take_profit, stop_loss)
}

/// The shares of a position closed by its working take-profit and stop-loss exits.
fn position_coverage(position: &Position, orders: &[&Order]) -> (f64, f64) {
    let closed = |bracket| {
        orders
            .iter()
            .filter(|order| {
                order.request.symbol == position.symbol
                    && order.request.position_bracket == Some(bracket)
                    && order.request.side.reduces(position.quantity)
            })
            .map(|order| (order.request.quantity - order.filled).max(0.0))
            .sum()
    };
    let size = position.quantity.abs();
    (
        share_of(closed(PositionBracket::TakeProfit), size),
        share_of(closed(PositionBracket::StopLoss), size),
    )
}

/// Paints take-profit coverage above stop-loss coverage, over full-width tracks.
pub(crate) fn paint_coverage(
    painter: &egui::Painter,
    theme: &Theme,
    center: Pos2,
    coverage: (f64, f64),
) {
    for (share, color, dy) in [
        (coverage.0, theme.ui.take_profit, -3.0),
        (coverage.1, theme.ui.stop_loss, 3.0),
    ] {
        let track = Rect::from_center_size(center + vec2(0.0, dy), vec2(COVERAGE_WIDTH, 3.0));
        painter.rect_filled(track, 1.0, theme.ui.border);
        if share > 0.0 {
            let fill = Rect::from_min_size(track.min, vec2(COVERAGE_WIDTH * share as f32, 3.0));
            painter.rect_filled(fill, 1.0, color);
        }
    }
}

/// Hover text for the quantity shares shown by a coverage mark.
pub(crate) fn coverage_text(ctx: &egui::Context, coverage: (f64, f64)) -> String {
    l10n::format_text(
        ctx,
        "Take profits cover {take_profit}% and stop losses {stop_loss}% of the quantity",
        &[
            ("take_profit", format!("{:.0}", coverage.0 * 100.0)),
            ("stop_loss", format!("{:.0}", coverage.1 * 100.0)),
        ],
    )
}

/// Draws the working orders with their brackets and the open positions of one symbol on the
/// chart, and returns what the user asked for. Pass only the orders and positions of the
/// chart's symbol, and call it after drawing the chart. The price axis is left to the caller; see
/// [`TradingLines::tags`]. A stop-limit order draws its stop and its limit as two lines joined
/// at the node column; each is dragged on its own, and exits follow the stop.
/// Equal prices share one line and move together.
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
        let held = held_exits(position, orders.iter().copied());
        position_options.take_profit &= !held.0;
        position_options.stop_loss &= !held.1;
        let pressed = show_position(
            ui,
            mapping,
            &position_options,
            position,
            held,
            options
                .coverage
                .then(|| position_coverage(position, &orders)),
            &mut tags,
        );
        action = action.or(pressed);
    }

    for order in orders.into_iter().filter(|_| options.show_orders) {
        let look = options
            .order_appearance
            .iter()
            .find(|(id, _)| id == &order.id)
            .map(|(_, look)| look);
        let entry_color = look
            .and_then(|look| look.color)
            .unwrap_or_else(|| side_color(order.request.side));
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
        // Each line: its kind, its take-profit level if any, its price and its quantity.
        let levels_of = &request.take_profit_levels;
        let mut levels = vec![(Level::Entry, None, initial_entry, open)];
        if let OrderType::StopLimit { stop, limit } = request.order_type
            && limit != stop
        {
            levels.push((Level::Limit, None, Some(limit), open));
        }
        if levels_of.is_empty() {
            levels.push((Level::TakeProfit, None, request.take_profit, open));
        }
        for (index, level) in levels_of.iter().enumerate() {
            levels.push((
                Level::TakeProfit,
                Some(index),
                Some(level.price),
                level.quantity,
            ));
        }
        for (index, level) in levels_of.iter().enumerate() {
            if level.stop_loss.is_some() {
                levels.push((
                    Level::StopLoss,
                    Some(index),
                    level.stop_loss,
                    level.quantity,
                ));
            }
        }
        let shared = if levels_of.iter().any(|level| level.stop_loss.is_some()) {
            let without: f64 = levels_of
                .iter()
                .filter(|level| level.stop_loss.is_none())
                .map(|level| level.quantity)
                .sum();
            without.min(open)
        } else {
            open
        };
        levels.push((Level::StopLoss, None, request.stop_loss, shared));
        let mut parent_price = initial_entry;
        for (level, index, price, quantity) in levels {
            let Some(price) = price.filter(|price| price.is_finite()) else {
                continue;
            };
            let price = if !matches!(level, Level::Entry | Level::Limit) {
                price
                    + parent_price
                        .zip(initial_entry)
                        .map_or(0.0, |(moved, original)| moved - original)
            } else {
                price
            };
            let appearance = if matches!(level, Level::Entry | Level::Limit) {
                entry_level
            } else {
                level
            };
            let (name, mut color) = match appearance {
                Level::Entry | Level::Limit => {
                    (l10n::text(ui.ctx(), kind), side_color(request.side))
                }
                Level::TakeProfit => (l10n::text(ui.ctx(), "TP"), theme.ui.take_profit),
                Level::StopLoss => (l10n::text(ui.ctx(), "SL"), theme.ui.stop_loss),
            };
            let look = look.filter(|_| matches!(level, Level::Entry | Level::Limit));
            if let Some(own) = look.and_then(|look| look.color) {
                color = own;
            }
            // A bracket's result if it is reached, from the entry or, for a market order, the
            // last price.
            let entry = parent
                .map(|p| p.average_price)
                .or(parent_price)
                .or(options.last_price);
            let ctx = ui.ctx().clone();
            let body = |price: f64| {
                if level == Level::Limit {
                    return l10n::text(&ctx, "Limit");
                }
                let body = if let Some(text) = look.and_then(|look| look.text.as_ref()) {
                    text.clone()
                } else {
                    match (appearance, options.bracket_pnl, entry) {
                        (Level::TakeProfit | Level::StopLoss, Some(unit), Some(entry)) => {
                            let gain = (price - entry)
                                * parent.map_or(direction(request.side), |p| p.quantity.signum());
                            let money = gain * quantity * options.point_value;
                            pnl_text(&ctx, unit, money, gain, entry, options).0
                        }
                        _ => name.clone(),
                    }
                };
                match request.expires.filter(|_| {
                    level == Level::Entry && request.time_in_force == TimeInForce::GoodTillDate
                }) {
                    Some(expires) => l10n::format_text(
                        &ctx,
                        "{kind} until {expiry}",
                        &[
                            ("kind", body),
                            (
                                "expiry",
                                options
                                    .timezone
                                    .wall_time(expires)
                                    .format("%Y-%m-%d %H:%M")
                                    .to_string(),
                            ),
                        ],
                    ),
                    None => body,
                }
            };
            let line = PriceLine::new(price, color).with_style(
                look.and_then(|look| look.line_style).unwrap_or(
                    if appearance == Level::Entry && options.submit {
                        LineStyle::Dashed
                    } else {
                        LineStyle::Solid
                    },
                ),
            );
            // Removing a bracket changes the order, so it needs the same support as a drag.
            let cancel = level == Level::Entry || (level != Level::Limit && options.modify);
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
                    exit_coverage(request).1 < 1.0,
                )
            } else {
                &[]
            };
            let menu =
                |ui: &mut Ui| order_menu(ui, order, options.menu.expect("menu capabilities"));
            let label = Label {
                id: ("trading_order", &order.id.0),
                body: &body,
                body_color: color,
                hover_text: look.and_then(|look| look.hover_text.as_deref()),
                quantity,
                coverage: (options.coverage
                    && level == Level::Entry
                    && request.position_bracket.is_none())
                .then(|| exit_coverage(request)),
                buttons: if cancel { &[Button::Cancel] } else { &[] },
                draggable: options.modify,
                submit: (options.submit && level == Level::Entry).then_some(request.side),
                exits,
                exit_pnl: &exit_pnl,
                menu: (options.menu.is_some()
                    && !options.submit
                    && matches!(level, Level::Entry | Level::Limit))
                .then_some(&menu as &dyn Fn(&mut Ui) -> Option<TradingLineAction>),
            };
            let first_tag = tags.len();
            let (dragged, pressed, released, chosen) = ui
                .push_id((level as u8, index), |ui| {
                    label.show(ui, mapping, options, &line, &mut tags)
                })
                .inner;
            let drawn = tags.get(first_tag).map_or(price, |tag| tag.0);
            if appearance == Level::Entry && tags.len() > first_tag {
                outlined.push(first_tag);
                if level == Level::Entry {
                    parent_price = Some(drawn);
                }
            }
            if level == Level::Limit {
                if let Some(stop) = parent_price {
                    connector(ui, mapping, stop, drawn, color);
                }
            } else if let Some(parent) = parent {
                connector(
                    ui,
                    mapping,
                    parent.average_price,
                    drawn,
                    options
                        .position_appearance
                        .iter()
                        .find(|(symbol, _)| symbol == &parent.symbol)
                        .and_then(|(_, look)| look.color)
                        .unwrap_or_else(|| {
                            side_color(if parent.quantity > 0.0 {
                                OrderSide::Buy
                            } else {
                                OrderSide::Sell
                            })
                        }),
                );
            } else if level != Level::Entry
                && let Some(entry) = entry
            {
                connector(ui, mapping, entry, drawn, entry_color);
            }
            if tags.len() == first_tag {
                continue;
            }
            if let Some(chosen) = chosen {
                action = action.or(Some(chosen));
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
            match (level, index) {
                (Level::Entry, _) => {
                    if let Some(price) = price {
                        let delta = price - entry_price(&request.order_type).unwrap_or(price);
                        // The other levels keep their distance, written at the tick's decimals.
                        let scale = 10f64.powi(decimals(options.tick_size) as i32);
                        let shift = |other: f64| ((other + delta) * scale).round() / scale;
                        changed.order_type = match request.order_type {
                            OrderType::Limit(_) => OrderType::Limit(price),
                            OrderType::Stop(_) => OrderType::Stop(price),
                            // A limit equal to its stop shares its line, so it moves with it.
                            OrderType::StopLimit { stop, limit } => OrderType::StopLimit {
                                stop: price,
                                limit: if limit == stop { price } else { limit },
                            },
                            other => other,
                        };
                        changed.take_profit = changed.take_profit.map(shift);
                        for take_profit in &mut changed.take_profit_levels {
                            take_profit.price = shift(take_profit.price);
                            take_profit.stop_loss = take_profit.stop_loss.map(shift);
                        }
                        changed.stop_loss = changed.stop_loss.map(shift);
                    }
                }
                (Level::TakeProfit, None) => changed.take_profit = price,
                (Level::TakeProfit, Some(index)) => match price {
                    Some(price) => changed.take_profit_levels[index].price = price,
                    None => remove_take_profit_level(&mut changed, index),
                },
                (Level::StopLoss, Some(index)) => {
                    changed.take_profit_levels[index].stop_loss = price
                }
                (Level::StopLoss, None) => changed.stop_loss = price,
                (Level::Limit, _) => {
                    if let (Some(price), OrderType::StopLimit { stop, .. }) =
                        (price, request.order_type)
                    {
                        changed.order_type = OrderType::StopLimit { stop, limit: price };
                    }
                }
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

/// Whether the position already has a working take profit and stop loss.
pub(crate) fn held_exits<'a>(
    position: &Position,
    orders: impl IntoIterator<Item = &'a Order>,
) -> (bool, bool) {
    let mut held = (false, false);
    for order in orders {
        if order.request.symbol == position.symbol
            && (order.request.side == OrderSide::Buy) != (position.quantity > 0.0)
        {
            match order.request.position_bracket {
                Some(PositionBracket::TakeProfit) => held.0 = true,
                Some(PositionBracket::StopLoss) => held.1 = true,
                None => {}
            }
        }
    }
    held
}

/// One menu entry, with the account's reason when unavailable.
fn menu_entry(ui: &mut Ui, label: &str, enabled: bool, why: &str) -> bool {
    ui.add_enabled(enabled, egui::Button::new(l10n::text(ui.ctx(), label)))
        .on_disabled_hover_text(l10n::text(ui.ctx(), why))
        .clicked()
}

fn order_menu(ui: &mut Ui, order: &Order, capabilities: Capabilities) -> Option<TradingLineAction> {
    if menu_entry(
        ui,
        "Modify order",
        capabilities.modify,
        "The account does not support modifying orders",
    ) {
        return Some(TradingLineAction::Edit(
            order.clone(),
            order.request.clone(),
        ));
    }
    if menu_entry(ui, "Cancel order", true, "") {
        return Some(TradingLineAction::Cancel(order.clone()));
    }
    let request = &order.request;
    let reference = match request.order_type {
        OrderType::Limit(price) | OrderType::Stop(price) => price,
        OrderType::StopLimit { limit, .. } => limit,
        _ => return None,
    };
    if request.position_bracket.is_none() {
        for (label, offered, profit) in [
            (
                "Add take profit",
                request.take_profit.is_none() && request.take_profit_levels.is_empty(),
                true,
            ),
            ("Add stop loss", exit_coverage(request).1 < 1.0, false),
        ] {
            if offered
                && menu_entry(
                    ui,
                    label,
                    capabilities.brackets && capabilities.modify,
                    "The account does not support adding exits to orders",
                )
            {
                let mut changed = request.clone();
                if profit {
                    changed.take_profit = Some(reference);
                } else {
                    changed.stop_loss = Some(reference);
                }
                return Some(TradingLineAction::Edit(order.clone(), changed));
            }
        }
    }
    None
}

/// Removes a take-profit level, adding its quantity to the level before it, or after it for the
/// first. The last level left becomes the order's single take profit. The removed level's own
/// stop goes with it; its quantity joins the other level's exits.
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
        request.stop_loss = only.stop_loss.or(request.stop_loss);
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
pub(crate) fn pnl_text(
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
        PnlUnit::Percent if entry != 0.0 && entry.is_finite() => {
            let value = round(gain / entry.abs() * 100.0, 2);
            (value, format!("{:.2}%", value.abs()))
        }
        // A percentage needs a finite, nonzero entry.
        PnlUnit::Percent => (0.0, "-".to_owned()),
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

/// The price an order waits for: its limit or stop price, or the stop of a stop-limit order.
pub(crate) fn entry_price(order_type: &OrderType) -> Option<f64> {
    match *order_type {
        OrderType::Limit(price)
        | OrderType::Stop(price)
        | OrderType::StopLimit { stop: price, .. } => Some(price),
        _ => None,
    }
}

type LabelMenu<'a> = &'a dyn Fn(&mut Ui) -> Option<TradingLineAction>;

/// The label at the right end of a trading line: TP and SL buttons, the open quantity, a
/// description and buttons.
struct Label<'a> {
    id: (&'a str, &'a str),
    /// The description at the line's price.
    body: &'a dyn Fn(f64) -> String,
    body_color: Color32,
    hover_text: Option<&'a str>,
    quantity: f64,
    /// The coverage mark after the quantity, if shown.
    coverage: Option<(f64, f64)>,
    buttons: &'a [Button],
    draggable: bool,
    submit: Option<OrderSide>,
    /// The TP and SL buttons before the label.
    exits: &'a [Level],
    /// The text after the name on a label of an exit dragged out of `exits`, from its price.
    exit_pnl: &'a dyn Fn(PnlUnit, f64) -> String,
    menu: Option<LabelMenu<'a>>,
}

type LabelResponse = (
    Option<f64>,
    Option<Button>,
    Option<(Level, f64)>,
    Option<TradingLineAction>,
);

impl Label<'_> {
    /// Draws the line and its label. Returns the price a finished drag ended at, the button the
    /// user clicked, the exit dragged out of the label with its price, and the menu action.
    fn show(
        &self,
        ui: &mut Ui,
        mapping: &ChartMapping,
        options: &TradingLineOptions<'_>,
        line: &PriceLine,
        tags: &mut Vec<(f64, Color32)>,
    ) -> LabelResponse {
        let key = ui.scope_id().with(self.id);
        if !self.draggable {
            ui.ctx().data_mut(|data| data.remove::<f64>(key));
        }
        let staged = ui.ctx().data(|data| data.get_temp::<f64>(key));
        let y = mapping.price_to_y(staged.unwrap_or(line.price));
        if !y.is_finite() || (staged.is_none() && !mapping.is_y_visible(y)) {
            return (None, None, None, None);
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
            + self.coverage.map_or(0.0, |_| COVERAGE_CELL)
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
        let grab = if let Some(text) = self.hover_text {
            grab.on_hover_text(text)
        } else {
            grab
        };
        if self.draggable {
            if grab.hovered() || grab.dragged_by(egui::PointerButton::Primary) {
                ui.ctx().set_cursor_icon(CursorIcon::ResizeVertical);
            }
            if grab.dragged_by(egui::PointerButton::Primary)
                && let Some(pointer) = grab.interact_pointer_pos()
            {
                let price =
                    mapping.y_to_price(pointer.y.clamp(mapping.rect.top(), mapping.rect.bottom()));
                value = to_step(price, options.tick_size, f64::round);
                if value.is_finite() {
                    ui.ctx().data_mut(|data| data.insert_temp(key, value));
                }
            }
            if grab.drag_stopped_by(egui::PointerButton::Primary) {
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
        let mut chosen = None;
        if let Some(menu) = self.menu {
            let response = ui.interact(
                rect.intersect(mapping.rect),
                key.with("menu"),
                Sense::click(),
            );
            let response = if let Some(text) = self.hover_text {
                response.on_hover_text(text)
            } else {
                response
            };
            response.context_menu(|ui| chosen = menu(ui));
        }
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
        if let Some(coverage) = self.coverage {
            clipped.vline(x, rect.y_range(), stroke);
            paint_coverage(
                &clipped,
                &ui.active_theme(),
                pos2(x + COVERAGE_CELL / 2.0, y),
                coverage,
            );
            let cell = Rect::from_min_size(pos2(x, rect.top()), vec2(COVERAGE_CELL, HEIGHT));
            ui.interact(
                cell.intersect(mapping.rect),
                key.with("coverage"),
                Sense::hover(),
            )
            .on_hover_text(coverage_text(ui.ctx(), coverage));
            x += COVERAGE_CELL;
        }
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
                    key.with(("button", index)),
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
        (dragged, pressed, released, chosen)
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

    /// Coverage uses quantities and remaining exits, ignores another symbol or side, and paints
    /// entry and position labels only. Both themes distinguish full, partial and empty tracks.
    #[test]
    fn coverage_marks_fill_by_the_share_of_the_quantity_closed() {
        use crate::model::TakeProfitLevel;
        for theme in [Theme::light(), Theme::dark()] {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
            let mut request = OrderRequest::new("SYM", OrderSide::Buy, 2.0, OrderType::Limit(95.0));
            request.take_profit_levels = vec![
                TakeProfitLevel::new(105.0, 1.0),
                TakeProfitLevel::new(108.0, 1.0),
            ];
            request.take_profit_levels[0].stop_loss = Some(92.0);
            let mut orders = vec![Order::new(OrderId::new("A"), request, OrderStatus::Working)];
            orders.push(Order::new(
                OrderId::new("B"),
                OrderRequest::new("SYM", OrderSide::Sell, 1.0, OrderType::Limit(102.0)),
                OrderStatus::Working,
            ));
            for (id, symbol, side, quantity, kind, role) in [
                (
                    "TP",
                    "SYM",
                    OrderSide::Sell,
                    1.5,
                    OrderType::Limit(104.0),
                    PositionBracket::TakeProfit,
                ),
                (
                    "SL",
                    "SYM",
                    OrderSide::Sell,
                    2.0,
                    OrderType::Stop(97.0),
                    PositionBracket::StopLoss,
                ),
                (
                    "other",
                    "OTHER",
                    OrderSide::Sell,
                    2.0,
                    OrderType::Limit(104.0),
                    PositionBracket::TakeProfit,
                ),
                (
                    "same side",
                    "SYM",
                    OrderSide::Buy,
                    2.0,
                    OrderType::Limit(104.0),
                    PositionBracket::TakeProfit,
                ),
            ] {
                let mut request = OrderRequest::new(symbol, side, quantity, kind);
                request.position_bracket = Some(role);
                let mut order = Order::new(OrderId::new(id), request, OrderStatus::Working);
                if id == "TP" {
                    order.filled = 0.5;
                }
                orders.push(order);
            }
            let position = Position::new("SYM", 2.0, 100.0);
            let mut options = TradingLineOptions::new("USD", 0.01);
            options.coverage = true;
            for frame in 0..3 {
                let mut ys = Vec::new();
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))),
                        ..Default::default()
                    },
                    |ui| {
                        chart.show(ui);
                        let mapping = chart.get_chart_mapping();
                        ys = [95.0, 100.0, 102.0, 104.0, 97.0]
                            .map(|price| mapping.price_to_y(price))
                            .to_vec();
                        render_trading_lines(ui, &mapping, &orders, [&position], &options);
                    },
                );
                if frame == 2 {
                    for (index, (take_profit, stop_loss, tracks)) in [
                        (Some(10.0), Some(5.0), 2),
                        (Some(5.0), Some(10.0), 2),
                        (None, None, 2),
                        (None, None, 0),
                        (None, None, 0),
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        let bars: Vec<_> = output
                            .shapes
                            .iter()
                            .filter_map(|shape| match &shape.shape {
                                egui::Shape::Rect(rect)
                                    if rect.rect.height() == 3.0
                                        && (rect.rect.center().y - ys[index]).abs() <= 4.0 =>
                                {
                                    Some(rect)
                                }
                                _ => None,
                            })
                            .collect();
                        assert_eq!(
                            bars.iter()
                                .filter(|rect| rect.fill == theme.ui.border)
                                .count(),
                            tracks,
                            "tracks on row {index}"
                        );
                        for (color, expected) in [
                            (theme.ui.take_profit, take_profit),
                            (theme.ui.stop_loss, stop_loss),
                        ] {
                            let widths: Vec<_> = bars
                                .iter()
                                .filter(|rect| rect.fill == color)
                                .map(|rect| rect.rect.width())
                                .collect();
                            assert_eq!(
                                widths,
                                expected.into_iter().collect::<Vec<_>>(),
                                "fills on row {index}"
                            );
                        }
                    }
                }
                output.drop_without_applying_deltas();
            }
        }
    }

    /// Percentage labels keep the sign of a gain at negative entries and omit undefined ratios.
    #[test]
    fn percent_profit_uses_the_entry_size_and_needs_a_nonzero_entry() {
        let ctx = egui::Context::default();
        let options = TradingLineOptions::new("USD", 0.25);
        for (entry, gain, expected, value) in [
            (-10.0, 1.0, "+10.00%", 10.0),
            (-10.0, -1.0, "-10.00%", -10.0),
            (0.0, 1.0, "-", 0.0),
            (0.0, 0.0, "-", 0.0),
            (-0.0, -1.0, "-", 0.0),
            (99.0, 1.0, "+1.01%", 1.01),
            (99.0, -1.0, "-1.01%", -1.01),
            (f64::INFINITY, 1.0, "-", 0.0),
            (f64::NAN, 1.0, "-", 0.0),
        ] {
            assert_eq!(
                pnl_text(&ctx, PnlUnit::Percent, gain, gain, entry, &options),
                (expected.to_owned(), value)
            );
        }
    }

    /// An individual appearance changes its line and label, while neighbours and exits keep theirs.
    #[test]
    fn own_looks_replace_the_defaults_of_their_line_only() {
        for theme in [Theme::light(), Theme::dark()] {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            ctx.all_styles_mut(|style| {
                style.interaction.tooltip_delay = 0.0;
                style.interaction.show_tooltips_only_when_still = false;
            });
            let mut chart = crate::widget::test_frames::chart(&[90.0], &[115.0]);
            let mut a = OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Limit(95.0));
            a.take_profit = Some(97.0);
            let orders = [
                Order::new(OrderId::new("A"), a, OrderStatus::Working),
                Order::new(
                    OrderId::new("B"),
                    OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Limit(100.0)),
                    OrderStatus::Working,
                ),
            ];
            let position = Position::new("SYM", 2.0, 105.0);
            let purple = Color32::from_rgb(156, 39, 176);
            let cyan = Color32::from_rgb(0, 151, 167);
            let plan = TradingLineAppearance {
                color: Some(purple),
                line_style: Some(LineStyle::Dashed),
                text: Some("Plan".into()),
                hover_text: Some("Planned entry".into()),
            };
            let hedge = TradingLineAppearance {
                color: Some(cyan),
                line_style: Some(LineStyle::Dotted),
                text: Some("Hedge".into()),
                hover_text: Some("Hedge position".into()),
            };
            let looks = [(OrderId::new("A"), plan)];
            let positions = [("SYM".to_owned(), hedge)];
            let mut options = TradingLineOptions::new("USD", 0.01);
            options.order_appearance = &looks;
            options.position_appearance = &positions;
            let mut at = [Pos2::ZERO; 2];
            for frame in 0..8 {
                let events = match frame {
                    3 | 4 => vec![egui::Event::PointerMoved(at[0])],
                    5 => vec![egui::Event::PointerGone],
                    6 | 7 => vec![egui::Event::PointerMoved(at[1])],
                    _ => vec![],
                };
                let mut ys = [0.0; 3];
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        chart.show(ui);
                        let mapping = chart.get_chart_mapping();
                        ys = [95.0, 100.0, 105.0].map(|price| mapping.price_to_y(price));
                        let lines =
                            render_trading_lines(ui, &mapping, &orders, [&position], &options);
                        if frame == 2 {
                            let buy = crate::theme::trading_side_color(&theme, OrderSide::Buy);
                            assert_eq!(
                                lines.tags,
                                [
                                    (105.0, cyan),
                                    (95.0, purple),
                                    (97.0, theme.ui.take_profit),
                                    (100.0, buy)
                                ]
                            );
                        }
                    },
                );
                let text = |name: &str| {
                    output.shapes.iter().find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == name => Some(text),
                        _ => None,
                    })
                };
                if frame == 2 {
                    at[0] = text("Plan")
                        .expect("order caption")
                        .visual_bounding_rect()
                        .center();
                    at[1] = text("Hedge")
                        .expect("position caption")
                        .visual_bounding_rect()
                        .center();
                    let limit = text("Limit").expect("default caption");
                    assert!((limit.visual_bounding_rect().center().y - ys[1]).abs() < 2.0);
                    for (i, color) in [
                        purple,
                        crate::theme::trading_side_color(&theme, OrderSide::Buy),
                        cyan,
                    ]
                    .into_iter()
                    .enumerate()
                    {
                        let segments = output.shapes.iter().filter(|shape| matches!(&shape.shape,
                            egui::Shape::LineSegment { points, stroke } if stroke.color == color
                                && (points[0].y - ys[i]).abs() < 0.1 && (points[1].y - ys[i]).abs() < 0.1)).count();
                        match i {
                            0 => assert!(segments > 1, "dashed order line"),
                            1 => assert_eq!(segments, 1, "default solid line"),
                            2 => assert!(
                                output.shapes.iter().any(|shape| matches!(&shape.shape,
                                egui::Shape::Mesh(mesh) if mesh.indices.len() > 6
                                    && mesh.vertices.iter().all(|vertex| vertex.color == cyan
                                        && (vertex.pos.y - ys[2]).abs() <= 1.0))),
                                "dotted position line"
                            ),
                            _ => unreachable!(),
                        }
                        assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                            egui::Shape::Rect(rect) if rect.stroke.color == color && rect.stroke.width == 1.0
                                && (rect.rect.center().y - ys[i]).abs() < 0.1)));
                    }
                    assert!(
                        output
                            .shapes
                            .iter()
                            .any(|shape| matches!(&shape.shape, egui::Shape::Rect(rect)
                        if rect.fill == cyan && (rect.rect.center().y - ys[2]).abs() < 0.1))
                    );
                }
                if frame == 4 {
                    assert!(text("Planned entry").is_some());
                }
                if frame == 7 {
                    assert!(text("Hedge position").is_some());
                }
                output.drop_without_applying_deltas();
            }
        }
    }

    /// Label menus keep their actions, capability gates and explanations independent of buttons.
    #[test]
    fn label_menus_offer_what_the_account_supports() {
        use crate::model::{Capabilities, TakeProfitLevel};
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        struct State {
            chart: crate::Chart,
            orders: Vec<Order>,
            position: Position,
            capabilities: Option<Capabilities>,
            point: Pos2,
            position_menu: bool,
            action: Option<TradingLineAction>,
        }
        let full = Capabilities {
            limit: true,
            stop: true,
            modify: true,
            brackets: true,
            close: true,
            reverse: true,
            ..Capabilities::default()
        };
        for row in 0..19 {
            let mut capabilities = full;
            match row {
                5 => capabilities.modify = false,
                6 => capabilities.brackets = false,
                11 => capabilities.close = false,
                12 => capabilities.reverse = false,
                13 => {
                    capabilities.limit = false;
                    capabilities.stop = false;
                }
                _ => {}
            }
            let mut request = OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Limit(95.0));
            if row == 7 {
                request.take_profit = Some(100.0);
                request.stop_loss = Some(90.0);
            }
            if row == 15 {
                request.order_type = OrderType::StopLimit {
                    stop: 94.004,
                    limit: 95.0,
                };
            }
            if row == 16 {
                request.order_type = OrderType::Market;
            }
            if row == 17 {
                request.position_bracket = Some(PositionBracket::TakeProfit);
            }
            if row == 18 {
                let mut first = TakeProfitLevel::new(100.0, 0.5);
                first.stop_loss = Some(90.0);
                let mut second = TakeProfitLevel::new(105.0, 0.5);
                second.stop_loss = Some(91.0);
                request.take_profit_levels = vec![first, second];
            }
            let order = Order::new(OrderId::new("7"), request, OrderStatus::Working);
            let position = Position::new("SYM", 2.0, 105.0);
            let mut orders = vec![order.clone()];
            if row == 14 {
                for (id, kind, bracket) in [
                    ("TP", OrderType::Limit(110.0), PositionBracket::TakeProfit),
                    ("SL", OrderType::Stop(100.0), PositionBracket::StopLoss),
                ] {
                    let mut request = OrderRequest::new("SYM", OrderSide::Sell, 2.0, kind);
                    request.position_bracket = Some(bracket);
                    orders.push(Order::new(OrderId::new(id), request, OrderStatus::Working));
                }
            }
            let state = State {
                chart: crate::widget::test_frames::chart(&[90.0], &[115.0]),
                orders,
                position: position.clone(),
                capabilities: (row != 0).then_some(capabilities),
                point: Pos2::ZERO,
                position_menu: (8..=14).contains(&row),
                action: None,
            };
            let mut harness = Harness::builder()
                .with_size(vec2(1000.0, 800.0))
                .build_ui_state(
                    |ui, state: &mut State| {
                        ui.ctx().all_styles_mut(|style| {
                            style.interaction.tooltip_delay = 0.0;
                            style.interaction.show_tooltips_only_when_still = false;
                        });
                        state.chart.show(ui);
                        let mapping = state.chart.get_chart_mapping();
                        let mut options = TradingLineOptions::new("USD", 0.01);
                        options.label_alignment = TradingLabelAlignment::Left;
                        options.menu = state.capabilities;
                        options.modify = row == 15;
                        let price = if state.position_menu {
                            105.0
                        } else if row == 15 {
                            94.004
                        } else {
                            95.0
                        };
                        state.point = pos2(mapping.rect.left() + 12.0, mapping.price_to_y(price));
                        let lines = render_trading_lines(
                            ui,
                            &mapping,
                            &state.orders,
                            [&state.position],
                            &options,
                        );
                        state.action = state.action.take().or(lines.action);
                    },
                    state,
                );
            harness.run();
            let point = harness.state().point;
            harness.hover_at(point);
            harness.run_steps(1);
            for pressed in [true, false] {
                harness.event(egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
                harness.run_steps(1);
            }
            harness.run();
            assert!(
                harness.state().action.is_none(),
                "secondary click changed row {row}"
            );
            let (label, why) = match row {
                0 => ("Cancel order", None),
                1 | 5 => (
                    "Modify order",
                    (row == 5).then_some("The account does not support modifying orders"),
                ),
                2 => ("Cancel order", None),
                3 | 6 | 7 | 15 | 16 | 17 => (
                    "Add take profit",
                    (row == 6).then_some("The account does not support adding exits to orders"),
                ),
                4 | 18 => ("Add stop loss", None),
                8 | 11 => (
                    "Close position",
                    (row == 11).then_some("The account does not support closing positions"),
                ),
                9 | 12 => (
                    "Reverse position",
                    (row == 12).then_some("The account does not support reversing positions"),
                ),
                10 => ("Protect position", None),
                13 => (
                    "Protect position",
                    Some("The account does not support take-profit or stop-loss orders"),
                ),
                14 => (
                    "Protect position",
                    Some("The position has every exit the account supports"),
                ),
                _ => unreachable!(),
            };
            if [0, 7, 16, 17, 18].contains(&row) {
                assert!(harness.query_by_label(label).is_none(), "row {row}");
                if row == 7 {
                    assert!(harness.query_by_label("Add stop loss").is_none());
                }
                continue;
            }
            let node = harness.get_by_label(label);
            if let Some(why) = why {
                assert!(node.accesskit_node().is_disabled(), "row {row}");
                node.hover();
                harness.run();
                harness.get_by_label(why);
                assert!(harness.state().action.is_none());
            } else {
                node.click();
                harness.run();
                let mut changed = order.request.clone();
                let expected = match row {
                    1 => TradingLineAction::Edit(order.clone(), changed),
                    2 => TradingLineAction::Cancel(order.clone()),
                    3 | 15 => {
                        changed.take_profit = Some(95.0);
                        TradingLineAction::Edit(order.clone(), changed)
                    }
                    4 => {
                        changed.stop_loss = Some(95.0);
                        TradingLineAction::Edit(order.clone(), changed)
                    }
                    8 => TradingLineAction::Close(position.clone()),
                    9 => TradingLineAction::Reverse(position.clone()),
                    10 => TradingLineAction::Protect(position.clone()),
                    _ => unreachable!(),
                };
                assert_eq!(harness.state().action, Some(expected), "row {row}");
            }
        }
    }

    /// Both stop-limit prices are visible and independent; exits keep following the stop.
    /// A limit equal to its stop shares its line and moves with it.
    #[test]
    fn stop_limit_orders_drag_their_stop_and_limit_apart() {
        for theme in [Theme::light(), Theme::dark()] {
            for (label, limit, drawn, target, expected, take_profit) in [
                (
                    "Limit",
                    101.5,
                    &[101.0, 101.5][..],
                    102.0,
                    OrderType::StopLimit {
                        stop: 101.0,
                        limit: 102.0,
                    },
                    103.0,
                ),
                (
                    "Stop limit",
                    101.5,
                    &[101.0, 101.5],
                    100.5,
                    OrderType::StopLimit {
                        stop: 100.5,
                        limit: 101.5,
                    },
                    102.5,
                ),
                (
                    "Stop limit",
                    101.0,
                    &[101.0],
                    103.0,
                    OrderType::StopLimit {
                        stop: 103.0,
                        limit: 103.0,
                    },
                    105.0,
                ),
            ] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
                let mut request = OrderRequest::new(
                    "SYM",
                    OrderSide::Buy,
                    1.0,
                    OrderType::StopLimit { stop: 101.0, limit },
                );
                request.take_profit = Some(103.0);
                let order = Order::new(OrderId::new("S"), request, OrderStatus::Working);
                let mut options = TradingLineOptions::new("USD", 0.01);
                options.modify = true;
                let mut start = Pos2::ZERO;
                let mut end = Pos2::ZERO;
                let mut action = None;
                for frame in 0..7 {
                    let events = match frame {
                        3 => vec![egui::Event::PointerMoved(start)],
                        4 => vec![egui::Event::PointerButton {
                            pos: start,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: egui::Modifiers::NONE,
                        }],
                        5 => vec![egui::Event::PointerMoved(end)],
                        6 => vec![egui::Event::PointerButton {
                            pos: end,
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            modifiers: egui::Modifiers::NONE,
                        }],
                        _ => vec![],
                    };
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            chart.show(ui);
                            let mapping = chart.get_chart_mapping();
                            end.y = mapping.price_to_y(target);
                            let lines = render_trading_lines(ui, &mapping, [&order], [], &options);
                            if frame == 2 {
                                let buy = crate::theme::trading_side_color(&theme, OrderSide::Buy);
                                assert_eq!(
                                    lines.tags,
                                    drawn
                                        .iter()
                                        .map(|&price| (price, buy))
                                        .chain([(103.0, theme.ui.take_profit)])
                                        .collect::<Vec<_>>(),
                                    "{label} {limit}"
                                );
                                assert_eq!(
                                    lines.outlined,
                                    (0..drawn.len()).collect::<Vec<_>>(),
                                    "{label} {limit}"
                                );
                            }
                            action = action.take().or(lines.action);
                        },
                    );
                    if frame == 2 {
                        start = output
                            .shapes
                            .iter()
                            .find_map(|shape| match &shape.shape {
                                egui::Shape::Text(text) if text.galley.text() == label => {
                                    Some(text.pos + text.galley.size() / 2.0)
                                }
                                _ => None,
                            })
                            .expect("price label");
                        end.x = start.x;
                    }
                    output.drop_without_applying_deltas();
                }
                let mut changed = order.request.clone();
                changed.order_type = expected;
                changed.take_profit = Some(take_profit);
                assert_eq!(
                    action,
                    Some(TradingLineAction::Modify(order, changed)),
                    "{label} {limit}"
                );
            }
        }
    }

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
                    timezone: TimezoneMode::Utc,
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
                    menu: None,
                    order_appearance: &[],
                    position_appearance: &[],
                    coverage: false,
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
