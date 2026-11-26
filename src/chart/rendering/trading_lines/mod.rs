//! Order, bracket and position lines from a broker's state. Changes are host intents: rendering
//! never changes an order or a position.

mod format;
mod position;

pub use format::TradingFormatter;
pub(crate) use format::quantity_text;
#[cfg(feature = "ui")]
pub(crate) use format::{grouped, quantity_decimals};

#[cfg(feature = "ui")]
pub(crate) use position::exit_request;
use position::show_position;

use egui::{Color32, CursorIcon, FontId, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};

use crate::chart::coords::ChartMapping;
use crate::chart::rendering::{draw_dashed_polyline, render_price_line};
use crate::localization as l10n;
use crate::model::{
    Capabilities, LineStyle, Order, OrderId, OrderRequest, OrderSide, OrderType, Position,
    PositionBracket, PriceLine, TimeInForce, settled,
};
use crate::scales::{TickGrid, TimezoneMode, decimals};
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

/// The appearance and permissions of one order's or position's line. Unset fields keep the
/// theme and [`TradingLineOptions`] defaults. An order's colours and texts cover its entry and
/// stop-limit limit lines, and its take profits and stop loss retain their own; its permissions
/// cover the whole order.
///
/// ```
/// use egui_charts::chart::{TradingLineAppearance, TradingLineOptions};
/// use egui_charts::model::{LineStyle, OrderId};
///
/// let mut look = TradingLineAppearance::default();
/// look.color = Some(egui::Color32::from_rgb(156, 39, 176));
/// look.line_style = Some(LineStyle::Dashed);
/// look.line_width = Some(2.0);
/// look.line_length = Some(0.5);
/// look.text = Some("Plan".into());
/// look.hover_text = Some("Planned entry".into());
/// look.cancel_hover_text = Some("Remove plan".into());
/// look.label_color = Some(egui::Color32::WHITE);
/// look.modify = Some(false);
/// let looks = [(OrderId::new("plan-1"), look)];
/// let mut options = TradingLineOptions::new("USD", 0.25);
/// options.order_appearance = &looks;
/// ```
#[derive(Clone, Debug, Default, PartialEq)]
#[non_exhaustive]
pub struct TradingLineAppearance {
    /// The colour of the line, its price tag and, unless set apart below, the label, quantity
    /// and buttons, replacing the side's colour.
    pub color: Option<Color32>,
    /// The colour of the label's outline, tint, dividers and description, replacing
    /// [`color`](Self::color) on the label only.
    pub label_color: Option<Color32>,
    /// The colour of the quantity, replacing [`color`](Self::color): its text on an order's
    /// label, and its cell on a position's label, whose text stays white.
    pub quantity_color: Option<Color32>,
    /// The colour of the cancel, close and reverse buttons and of a projected order's Buy or
    /// Sell button, replacing [`color`](Self::color). TP and SL buttons keep their colours.
    pub button_color: Option<Color32>,
    /// The line's pattern, replacing solid or the projected order's dashed pattern.
    pub line_style: Option<LineStyle>,
    /// The width of the line, replacing [`TradingLineOptions::line_width`].
    pub line_width: Option<f32>,
    /// How far the line reaches from the pane's right edge, replacing
    /// [`TradingLineOptions::line_length`].
    pub line_length: Option<f32>,
    /// The description replacing an order's type or following a position's quantity.
    pub text: Option<String>,
    /// The text shown while the pointer hovers over the label.
    pub hover_text: Option<String>,
    /// The hover text of an order's cancel button, replacing "Cancel". Its take profit and
    /// stop loss keep theirs.
    pub cancel_hover_text: Option<String>,
    /// The hover text of a position's close button, replacing "Close position".
    pub close_hover_text: Option<String>,
    /// The hover text of a position's reverse button, replacing "Reverse position".
    pub reverse_hover_text: Option<String>,
    /// The hover text of the TP button dragged out of the label to place a take profit,
    /// replacing "Drag to place a take profit".
    pub take_profit_hover_text: Option<String>,
    /// The hover text of the SL button dragged out of the label to place a stop loss,
    /// replacing "Drag to place a stop loss".
    pub stop_loss_hover_text: Option<String>,
    /// Whether the order's lines can be dragged, its take profits and stop loss removed and
    /// exits dragged out of its label, replacing [`TradingLineOptions::modify`] for this
    /// order, its take profits and stop loss included. `Some(false)` also disables Modify
    /// order, Add take profit and Add stop loss in its label menu. A trailing stop is never
    /// dragged. Positions ignore it.
    pub modify: Option<bool>,
    /// `Some(false)` hides the order's cancel button and disables Cancel order in its label
    /// menu. The button shows otherwise. Positions ignore it.
    pub cancel: Option<bool>,
    /// Whether the position's label offers its close button and, with
    /// [`TradingLineOptions::reverse`], its reverse button, replacing
    /// [`TradingLineOptions::close`] for this position. `Some(false)` also disables Close
    /// position and Reverse position in its label menu. Orders ignore it.
    pub close: Option<bool>,
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
    /// Width in points of order, bracket and position lines. 1 by default. A negative or
    /// non-finite width draws 1; zero draws the labels and tags without lines.
    pub line_width: f32,
    /// How far lines reach left from the pane's right edge, as a share of the pane width from
    /// 0 to 1, while [`extend_lines`](Self::extend_lines) is off. A line always reaches its
    /// label, so 0, the default, starts each line at its label. Shares outside 0 to 1 are
    /// clamped, and a non-finite share counts as 0.
    pub line_length: f32,
    /// Horizontal placement of order and position labels.
    pub label_alignment: TradingLabelAlignment,
    /// Treat the orders as projected, not yet placed: their entry lines are dashed and they offer
    /// a Buy or Sell submit button.
    pub submit: bool,
    /// Order and bracket lines can be dragged, and brackets removed. An order's
    /// [`TradingLineAppearance::modify`] replaces it for that order.
    pub modify: bool,
    /// Position labels offer a close button, and a reverse button with
    /// [`reverse`](Self::reverse). A position's [`TradingLineAppearance::close`] replaces it for
    /// that position.
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
    /// Host text for the quantities of order, bracket and position labels, asked with the
    /// order's or position's symbol. Without it, or where it returns `None`, a quantity is
    /// written at its own decimals, up to eight, with thousands separators, as the trading
    /// panel's account tables write it.
    pub formatter: Option<&'a dyn TradingFormatter>,
    /// The positive tick size used to round dragged prices.
    pub tick_size: f64,
    /// Coarser ticks by price range, as
    /// [`Instrument::tick_tiers`](crate::model::Instrument::tick_tiers): dragged prices round to
    /// the tick of their range, and profit in ticks counts them. Empty in [`new`](Self::new).
    pub tick_tiers: &'a [(f64, f64)],
    /// The zone in which good-till-date order labels write their expiry.
    pub timezone: TimezoneMode,
}

impl<'a> TradingLineOptions<'a> {
    /// Orders and positions with 1-point lines starting at their labels at the right edge,
    /// offering no buttons, drags or menus, with no profit or loss, no coverage marks, no last
    /// price, no host formatter and a
    /// [`point_value`](Self::point_value) of one. `currency` names the account's currency and
    /// `tick_size` rounds dragged prices. Assign the other fields to offer more.
    ///
    /// ```
    /// use egui_charts::chart::TradingLineOptions;
    ///
    /// let mut options = TradingLineOptions::new("USD", 0.25);
    /// options.modify = true;
    /// options.line_width = 2.0;
    /// ```
    pub fn new(currency: &'a str, tick_size: f64) -> Self {
        Self {
            show_orders: true,
            show_positions: true,
            extend_lines: false,
            line_width: 1.0,
            line_length: 0.0,
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
            formatter: None,
            tick_size,
            tick_tiers: &[],
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
        // Permissions cover the whole order, its exits included.
        let modify = look.and_then(|look| look.modify).unwrap_or(options.modify);
        let cancellable = look.and_then(|look| look.cancel) != Some(false);
        let menu_modify = look.and_then(|look| look.modify) != Some(false);
        let entry_color = look
            .and_then(|look| look.color)
            .unwrap_or_else(|| side_color(order.request.side));
        let request = &order.request;
        let open = settled(request.quantity - order.filled);
        let kind = match request.order_type {
            OrderType::Limit(_) => "Limit",
            OrderType::Stop(_) => "Stop",
            OrderType::StopLimit { .. } => "Stop limit",
            OrderType::TrailingStop { .. } => "Trailing stop",
            _ => "Market",
        };
        // The broker moves a trailing stop; until it reports the stop, the line waits at the
        // activation price.
        let trailing = match request.order_type {
            OrderType::TrailingStop { activation, .. } => Some(order.trailing_stop.or(activation)),
            _ => None,
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
        let initial_entry = trailing.unwrap_or_else(|| entry_price(&request.order_type));
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
            settled(without).min(open)
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
                            let sign =
                                parent.map_or(direction(request.side), |p| p.quantity.signum());
                            let gain = (price - entry) * sign;
                            let money = gain * quantity * options.point_value;
                            pnl_text(&ctx, unit, money, gain, entry, sign, options).0
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
            let width = line_width(
                look.and_then(|look| look.line_width)
                    .unwrap_or(options.line_width),
            );
            let line = PriceLine::new(price, color).with_width(width).with_style(
                look.and_then(|look| look.line_style).unwrap_or(
                    if appearance == Level::Entry && options.submit {
                        LineStyle::Dashed
                    } else {
                        LineStyle::Solid
                    },
                ),
            );
            // Removing a bracket changes the order, so it needs the same support as a drag.
            let cancel = if level == Level::Entry {
                cancellable
            } else {
                level != Level::Limit && modify
            };
            // What an exit dragged out of the entry's label makes if the price reaches it.
            let exit_pnl = |unit, at: f64| {
                let sign = direction(request.side);
                let gain = (at - price) * sign;
                let money = gain * open * options.point_value;
                pnl_text(&ctx, unit, money, gain, price, sign, options).0
            };
            let exits = if level == Level::Entry
                && options.order_exits
                && modify
                && request.position_bracket.is_none()
                && initial_entry.is_some()
                && trailing.is_none()
            {
                offered_exits(
                    request.take_profit.is_none() && request.take_profit_levels.is_empty(),
                    exit_coverage(request).1 < 1.0,
                )
            } else {
                &[]
            };
            let menu = |ui: &mut Ui| {
                order_menu(
                    ui,
                    order,
                    options.menu.expect("menu capabilities"),
                    menu_modify,
                    cancellable,
                )
            };
            let label = Label {
                id: ("trading_order", &order.id.0),
                body: &body,
                label_color: look.and_then(|look| look.label_color).unwrap_or(color),
                quantity_color: look.and_then(|look| look.quantity_color).unwrap_or(color),
                button_color: look.and_then(|look| look.button_color).unwrap_or(color),
                hover_text: look.and_then(|look| look.hover_text.as_deref()),
                cancel_hover_text: look.and_then(|look| look.cancel_hover_text.as_deref()),
                take_profit_hover_text: look
                    .and_then(|look| look.take_profit_hover_text.as_deref()),
                stop_loss_hover_text: look.and_then(|look| look.stop_loss_hover_text.as_deref()),
                line_length: look
                    .and_then(|look| look.line_length)
                    .unwrap_or(options.line_length),
                quantity: quantity_text(options.formatter, &request.symbol, quantity),
                coverage: (options.coverage
                    && level == Level::Entry
                    && request.position_bracket.is_none())
                .then(|| exit_coverage(request)),
                buttons: if cancel { &[Button::Cancel] } else { &[] },
                draggable: modify && trailing.is_none(),
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
                        let entry = entry_price(&request.order_type).unwrap_or(price);
                        let delta = price - entry;
                        // The other levels keep their distance, written at the tick's decimals,
                        // or their count of ticks across price ranges.
                        let scale = 10f64.powi(decimals(options.tick_size) as i32);
                        let grid = TickGrid::new(options.tick_size, options.tick_tiers);
                        let shift = |other: f64| {
                            if grid.is_flat() {
                                ((other + delta) * scale).round() / scale
                            } else {
                                grid.offset(other, grid.steps_between(entry, price))
                            }
                        };
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

/// The label menu of an order. `modify` and `cancel` are false when the order's own appearance
/// refuses them; the account's reason wins where both refuse.
fn order_menu(
    ui: &mut Ui,
    order: &Order,
    capabilities: Capabilities,
    modify: bool,
    cancel: bool,
) -> Option<TradingLineAction> {
    const FIXED: &str = "The order cannot be changed on the chart";
    if menu_entry(
        ui,
        "Modify order",
        capabilities.modify && modify,
        if capabilities.modify {
            FIXED
        } else {
            "The account does not support modifying orders"
        },
    ) {
        return Some(TradingLineAction::Edit(
            order.clone(),
            order.request.clone(),
        ));
    }
    if menu_entry(
        ui,
        "Cancel order",
        cancel,
        "The order cannot be cancelled on the chart",
    ) {
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
                    capabilities.brackets && capabilities.modify && modify,
                    if capabilities.brackets && capabilities.modify {
                        FIXED
                    } else {
                        "The account does not support adding exits to orders"
                    },
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
/// written in `unit`, and the value written, whose sign colours it. `direction` is 1 when the
/// gain is made above the entry and -1 below it, where ticks are counted.
pub(crate) fn pnl_text(
    ctx: &egui::Context,
    unit: PnlUnit,
    money: f64,
    gain: f64,
    entry: f64,
    direction: f64,
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
            let grid = TickGrid::new(options.tick_size, options.tick_tiers);
            let value = if grid.is_flat() {
                (gain / options.tick_size).round()
            } else {
                direction * grid.steps_between(entry, entry + direction * gain)
            };
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
    /// Host hover texts of the TP and SL buttons.
    take_profit_hover_text: Option<&'a str>,
    stop_loss_hover_text: Option<&'a str>,
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
            let own = if take_profit {
                self.take_profit_hover_text
            } else {
                self.stop_loss_hover_text
            };
            let response = ui
                .interact(rect.intersect(mapping.rect), id, Sense::drag())
                .on_hover_text(own.map_or_else(
                    || {
                        l10n::text(
                            ui.ctx(),
                            if take_profit {
                                "Drag to place a take profit"
                            } else {
                                "Drag to place a stop loss"
                            },
                        )
                    },
                    str::to_owned,
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
                    TickGrid::new(options.tick_size, options.tick_tiers).snap(mapping.y_to_price(y))
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
            let line = PriceLine::new(price, exit_color)
                .with_width(line_width(options.line_width))
                .with_style(LineStyle::Solid);
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
    /// The outline, tint, dividers and description.
    label_color: Color32,
    quantity_color: Color32,
    /// The cancel and submit buttons and the buttons' hover highlight.
    button_color: Color32,
    hover_text: Option<&'a str>,
    cancel_hover_text: Option<&'a str>,
    take_profit_hover_text: Option<&'a str>,
    stop_loss_hover_text: Option<&'a str>,
    line_length: f32,
    /// The open quantity's text.
    quantity: String,
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
        let body = (!body.is_empty()).then(|| layout(body, self.label_color));
        let quantity = layout(self.quantity.clone(), self.quantity_color);
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
        let line_left = line_left(
            mapping.rect,
            rect.left(),
            options.extend_lines,
            self.line_length,
        );

        let mut dragged = None;
        let mut value = staged.unwrap_or(line.price);
        let grab = ui.interact(
            Rect::from_min_max(
                pos2(line_left, rect.top()),
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
                value = TickGrid::new(options.tick_size, options.tick_tiers).snap(price);
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
            line_left,
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
        let stroke = Stroke::new(1.0, self.label_color);
        clipped.rect(
            rect,
            4.0,
            label_fill(&ui.active_theme(), self.label_color),
            stroke,
            egui::StrokeKind::Inside,
        );
        x = rect.left();
        let w = width(&quantity);
        clipped.galley(
            pos2(x + PADDING, y - quantity.size().y / 2.0),
            quantity,
            self.quantity_color,
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
                self.label_color,
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
                .on_hover_text(
                    self.cancel_hover_text
                        .map_or_else(|| l10n::text(ui.ctx(), "Cancel"), str::to_owned),
                );
            if response.hovered() {
                clipped.rect_filled(cell.shrink(1.0), 1.0, self.button_color.gamma_multiply(0.2));
            }
            if response.clicked() {
                pressed = Some(*button);
            }
            paint_glyph(&clipped, *button, cell.center(), self.button_color);
        }
        // After the grab area, which runs under them with extended lines: the last widget wins.
        let group_left = rect.left() - exits_width;
        let (_, released) = ExitButtons {
            key,
            exits: self.exits,
            x: group_left,
            y,
            entry: value,
            line_left,
            preview_right: rect.right(),
            pnl: self.exit_pnl,
            take_profit_hover_text: self.take_profit_hover_text,
            stop_loss_hover_text: self.stop_loss_hover_text,
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
                    .fill(self.button_color)
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

/// A line width a host set, or 1 when it is negative or not finite.
fn line_width(width: f32) -> f32 {
    if width.is_finite() && width >= 0.0 {
        width
    } else {
        1.0
    }
}

/// Where a line starts: the pane's left edge with extended lines, otherwise its label's left
/// edge or, further left, `length` of the pane width from its right edge.
fn line_left(pane: Rect, label_left: f32, extend: bool, length: f32) -> f32 {
    if extend {
        return pane.left();
    }
    let share = if length.is_finite() {
        length.clamp(0.0, 1.0)
    } else {
        0.0
    };
    label_left.min(pane.right() - share * pane.width())
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

    /// Order, bracket and position labels keep the account tables' quantity text.
    #[test]
    fn labels_write_quantities_in_the_host_text_or_at_their_own_decimals() {
        struct Lots;
        impl TradingFormatter for Lots {
            fn quantity(&self, symbol: &str, quantity: f64) -> Option<String> {
                (symbol == "SYM").then(|| format!("{quantity} lots"))
            }
        }
        type Row<'a> = (
            Option<&'a dyn TradingFormatter>,
            f64,
            f64,
            f64,
            &'a str,
            &'a str,
        );
        let rows: [Row<'_>; 3] = [
            (None, 0.3, 0.1, 0.1 + 0.2, "0.2", "0.3"),
            (None, 2500.0, 0.0, 1500.0, "2,500", "1,500"),
            (Some(&Lots), 0.3, 0.1, 2.0, "0.2 lots", "2 lots"),
        ];
        for (formatter, quantity, filled, position_quantity, order_text, position_text) in rows {
            let ctx = egui::Context::default();
            let mut chart = crate::widget::test_frames::chart(&[90.0], &[115.0]);
            let mut request =
                OrderRequest::new("SYM", OrderSide::Buy, quantity, OrderType::Limit(95.0));
            request.take_profit = Some(100.0);
            request.stop_loss = Some(93.0);
            let mut order = Order::new(OrderId::new("A"), request, OrderStatus::Working);
            order.filled = filled;
            let position = Position::new("SYM", position_quantity, 108.0);
            let mut options = TradingLineOptions::new("USD", 0.01);
            options.formatter = formatter;
            for frame in 0..3 {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))),
                        ..Default::default()
                    },
                    |ui| {
                        chart.show(ui);
                        render_trading_lines(
                            ui,
                            &chart.get_chart_mapping(),
                            [&order],
                            [&position],
                            &options,
                        );
                    },
                );
                if frame == 2 {
                    let texts: Vec<_> = output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) => Some(text.galley.text()),
                            _ => None,
                        })
                        .collect();
                    assert_eq!(
                        texts.iter().filter(|&&text| text == order_text).count(),
                        3,
                        "{order_text}: {texts:?}"
                    );
                    assert_eq!(
                        texts.iter().filter(|&&text| text == position_text).count(),
                        1,
                        "{position_text}: {texts:?}"
                    );
                }
                output.drop_without_applying_deltas();
            }
        }
    }

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
                pnl_text(&ctx, PnlUnit::Percent, gain, gain, entry, 1.0, &options),
                (expected.to_owned(), value)
            );
        }
    }

    /// Profit in ticks counts the ticks between the entry and the price reached: a sell from 25.00
    /// down to 24.90 gains five 0.02 ticks, not two of the 0.05 ticks above its entry.
    #[test]
    fn tick_profit_counts_the_ticks_of_the_prices_passed() {
        let ctx = egui::Context::default();
        let mut options = TradingLineOptions::new("USD", 0.01);
        options.tick_tiers = &[(10.0, 0.02), (25.0, 0.05)];
        for (entry, gain, direction, expected) in [
            (25.0, 0.1, -1.0, ("+5 ticks", 5.0)),
            (25.0, 0.1, 1.0, ("+2 ticks", 2.0)),
            (25.0, -0.1, 1.0, ("-5 ticks", -5.0)),
        ] {
            assert_eq!(
                pnl_text(&ctx, PnlUnit::Ticks, 0.0, gain, entry, direction, &options),
                (expected.0.to_owned(), expected.1),
                "{entry} {gain} {direction}"
            );
        }
    }

    /// An individual appearance changes its line and label, while neighbours and exits keep theirs.
    #[test]
    fn own_looks_replace_the_defaults_of_their_line_only() {
        let pane = Rect::from_min_max(pos2(10.0, 0.0), pos2(210.0, 100.0));
        for (length, expected) in [
            (0.0, 160.0),
            (0.1, 160.0),
            (0.5, 110.0),
            (1.0, 10.0),
            (2.0, 10.0),
            (-1.0, 160.0),
            (f32::NAN, 160.0),
            (f32::INFINITY, 160.0),
            (f32::NEG_INFINITY, 160.0),
        ] {
            assert_eq!(line_left(pane, 160.0, false, length), expected);
            assert_eq!(line_left(pane, 160.0, true, length), 10.0);
        }
        for (width, expected) in [
            (0.0, 0.0),
            (2.0, 2.0),
            (-1.0, 1.0),
            (f32::NAN, 1.0),
            (f32::INFINITY, 1.0),
            (f32::NEG_INFINITY, 1.0),
        ] {
            assert_eq!(line_width(width), expected);
        }
        for (row_width, row_length) in [(1.0, 0.0), (2.0, 0.5)] {
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
                let orange = Color32::from_rgb(245, 124, 1);
                let lime = Color32::from_rgb(124, 179, 66);
                let red = Color32::from_rgb(198, 40, 40);
                let olive = Color32::from_rgb(130, 119, 23);
                let pink = Color32::from_rgb(216, 27, 96);
                let blue = Color32::from_rgb(30, 136, 229);
                let plan = TradingLineAppearance {
                    color: Some(purple),
                    label_color: Some(orange),
                    quantity_color: Some(lime),
                    button_color: Some(red),
                    line_style: Some(LineStyle::Dashed),
                    line_width: Some(3.0),
                    line_length: Some(0.9),
                    cancel_hover_text: Some("Remove plan".into()),
                    text: Some("Plan".into()),
                    hover_text: Some("Planned entry".into()),
                    ..Default::default()
                };
                let hedge = TradingLineAppearance {
                    color: Some(cyan),
                    label_color: Some(olive),
                    quantity_color: Some(pink),
                    button_color: Some(blue),
                    line_style: Some(LineStyle::Dotted),
                    line_width: Some(4.0),
                    close_hover_text: Some("Exit hedge".into()),
                    reverse_hover_text: Some("Flip hedge".into()),
                    text: Some("Hedge".into()),
                    hover_text: Some("Hedge position".into()),
                    ..Default::default()
                };
                let looks = [(OrderId::new("A"), plan)];
                let positions = [("SYM".to_owned(), hedge)];
                let mut options = TradingLineOptions::new("USD", 0.01);
                options.order_appearance = &looks;
                options.position_appearance = &positions;
                options.line_width = row_width;
                options.line_length = row_length;
                options.close = true;
                options.reverse = true;
                let mut at = [Pos2::ZERO; 6];
                for frame in 0..20 {
                    let events = match frame {
                        3 | 4 => vec![egui::Event::PointerMoved(at[0])],
                        5 | 8 | 11 | 14 | 17 => vec![egui::Event::PointerGone],
                        6 | 7 => vec![egui::Event::PointerMoved(at[1])],
                        9 | 10 => vec![egui::Event::PointerMoved(at[2])],
                        12 | 13 => vec![egui::Event::PointerMoved(at[3])],
                        15 | 16 => vec![egui::Event::PointerMoved(at[4])],
                        18 | 19 => vec![egui::Event::PointerMoved(at[5])],
                        _ => vec![],
                    };
                    let mut ys = [0.0; 4];
                    let mut pane = Rect::NOTHING;
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            chart.show(ui);
                            let mapping = chart.get_chart_mapping();
                            pane = mapping.rect;
                            ys = [95.0, 100.0, 105.0, 97.0].map(|price| mapping.price_to_y(price));
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
                        let buy = crate::theme::trading_side_color(&theme, OrderSide::Buy);
                        let label = |color, y: f32| {
                            output
                                .shapes
                                .iter()
                                .find_map(|shape| match &shape.shape {
                                    egui::Shape::Rect(rect)
                                        if rect.stroke.color == color
                                            && rect.stroke.width == 1.0
                                            && (rect.rect.center().y - y).abs() < 0.1 =>
                                    {
                                        Some(rect.rect)
                                    }
                                    _ => None,
                                })
                                .expect("line label")
                        };
                        for (i, color, width, length) in [
                            (0, purple, 3.0, 0.9),
                            (1, buy, row_width, row_length),
                            (3, theme.ui.take_profit, row_width, row_length),
                        ] {
                            let segments: Vec<_> = output
                                .shapes
                                .iter()
                                .filter_map(|shape| match &shape.shape {
                                    egui::Shape::LineSegment { points, stroke }
                                        if stroke.color == color
                                            && (points[0].y - ys[i]).abs() < 0.1
                                            && (points[1].y - ys[i]).abs() < 0.1 =>
                                    {
                                        Some((points, stroke))
                                    }
                                    _ => None,
                                })
                                .collect();
                            if i == 0 {
                                assert!(segments.len() > 1, "dashed order line");
                            } else {
                                assert_eq!(segments.len(), 1, "default solid line");
                            }
                            assert!(
                                segments.iter().all(|(_, stroke)| stroke.width == width),
                                "line width at {}",
                                ys[i]
                            );
                            let start = segments
                                .iter()
                                .map(|(points, _)| points[0].x)
                                .fold(f32::INFINITY, f32::min);
                            let expected = if length == 0.0 {
                                label(color, ys[i]).left()
                            } else {
                                pane.right() - length * pane.width()
                            };
                            assert!(
                                (start - expected).abs() < 0.5,
                                "line start {start}, expected {expected}"
                            );
                        }
                        let dots = output
                            .shapes
                            .iter()
                            .find_map(|shape| match &shape.shape {
                                egui::Shape::Mesh(mesh)
                                    if mesh.indices.len() > 6
                                        && mesh.vertices.iter().all(|vertex| {
                                            vertex.color == cyan
                                                && (vertex.pos.y - ys[2]).abs() <= 4.0 / 2.0 + 1.0
                                        }) =>
                                {
                                    Some(mesh)
                                }
                                _ => None,
                            })
                            .expect("dotted position line");
                        let top = dots
                            .vertices
                            .iter()
                            .map(|vertex| vertex.pos.y)
                            .fold(f32::INFINITY, f32::min);
                        let bottom = dots
                            .vertices
                            .iter()
                            .map(|vertex| vertex.pos.y)
                            .fold(f32::NEG_INFINITY, f32::max);
                        assert!(bottom - top >= 3.5, "position line width");
                        if row_length == 0.5 {
                            let start = dots
                                .vertices
                                .iter()
                                .map(|vertex| vertex.pos.x)
                                .fold(f32::INFINITY, f32::min);
                            assert!(
                                (start - (pane.right() - 0.5 * pane.width())).abs() < 2.0,
                                "position line start"
                            );
                        }
                        let plan = label(orange, ys[0]);
                        let default = label(buy, ys[1]);
                        let hedge = label(olive, ys[2]);
                        let text_color = |text: &egui::epaint::TextShape| {
                            text.galley.job.sections[0].format.color
                        };
                        assert_eq!(text_color(text("Plan").unwrap()), orange);
                        assert_eq!(text_color(text("Hedge").unwrap()), olive);
                        for (y, expected) in [(ys[0], lime), (ys[1], buy)] {
                            let quantity = output
                                .shapes
                                .iter()
                                .find_map(|shape| match &shape.shape {
                                    egui::Shape::Text(text)
                                        if text.galley.text() == "1"
                                            && (text.visual_bounding_rect().center().y - y)
                                                .abs()
                                                < 2.0 =>
                                    {
                                        Some(text_color(text))
                                    }
                                    _ => None,
                                })
                                .expect("order quantity");
                            assert_eq!(quantity, expected, "quantity at {y}");
                        }
                        // Cancel and close glyphs.
                        for (y, expected) in [(ys[0], red), (ys[1], buy), (ys[2], blue)] {
                            let glyphs: Vec<_> = output
                                .shapes
                                .iter()
                                .filter_map(|shape| match &shape.shape {
                                    egui::Shape::LineSegment { points, stroke }
                                        if stroke.width == 1.5 && (points[0].y - y).abs() < 6.0 =>
                                    {
                                        Some(stroke.color)
                                    }
                                    _ => None,
                                })
                                .collect();
                            assert!(
                                !glyphs.is_empty() && glyphs.iter().all(|&color| color == expected),
                                "glyph at {y}: {glyphs:?}"
                            );
                        }
                        at[2] = pos2(plan.right() - HEIGHT / 2.0, ys[0]);
                        at[3] = pos2(default.right() - HEIGHT / 2.0, ys[1]);
                        at[4] = pos2(hedge.right() - HEIGHT / 2.0, ys[2]);
                        at[5] = pos2(hedge.left() - 1.6 * HEIGHT + HEIGHT / 2.0, ys[2]);
                        assert!(
                            output
                                .shapes
                                .iter()
                                .any(|shape| matches!(&shape.shape, egui::Shape::Rect(rect)
                        if rect.fill == pink && (rect.rect.center().y - ys[2]).abs() < 0.1))
                        );
                        assert!(
                            !output
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
                    for (hover_frame, expected, replaced) in [
                        (10, "Remove plan", "Cancel"),
                        (13, "Cancel", "Remove plan"),
                        (16, "Exit hedge", "Close position"),
                        (19, "Flip hedge", "Reverse position"),
                    ] {
                        if frame == hover_frame {
                            assert!(text(expected).is_some(), "{expected}");
                            assert!(text(replaced).is_none(), "{replaced}");
                        }
                    }
                    output.drop_without_applying_deltas();
                }
            }
        }

        // A fixed order and a position without close sit beside default lines, and a plan
        // names its exit buttons.
        let theme = Theme::dark();
        let ctx = egui::Context::default();
        crate::theme::apply_to_egui(&ctx, &theme);
        ctx.all_styles_mut(|style| {
            style.interaction.tooltip_delay = 0.0;
            style.interaction.show_tooltips_only_when_still = false;
        });
        let mut chart = crate::widget::test_frames::chart(&[90.0], &[115.0]);
        let limit = |price: f64, take_profit: Option<f64>| {
            let mut request =
                OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Limit(price));
            request.take_profit = take_profit;
            request
        };
        let orders = [
            Order::new(
                OrderId::new("F"),
                limit(95.0, Some(97.0)),
                OrderStatus::Working,
            ),
            Order::new(OrderId::new("P"), limit(100.0, None), OrderStatus::Working),
            Order::new(
                OrderId::new("D"),
                limit(110.0, Some(112.0)),
                OrderStatus::Working,
            ),
        ];
        let position = Position::new("SYM", 2.0, 105.0);
        let looks = [
            (
                OrderId::new("F"),
                TradingLineAppearance {
                    modify: Some(false),
                    cancel: Some(false),
                    ..Default::default()
                },
            ),
            (
                OrderId::new("P"),
                TradingLineAppearance {
                    take_profit_hover_text: Some("Plan target".into()),
                    stop_loss_hover_text: Some("Plan stop".into()),
                    ..Default::default()
                },
            ),
        ];
        let positions = [(
            "SYM".to_owned(),
            TradingLineAppearance {
                close: Some(false),
                take_profit_hover_text: Some("Hedge target".into()),
                stop_loss_hover_text: Some("Hedge stop".into()),
                ..Default::default()
            },
        )];
        let mut options = TradingLineOptions::new("USD", 0.01);
        options.order_appearance = &looks;
        options.position_appearance = &positions;
        options.modify = true;
        options.order_exits = true;
        options.close = true;
        options.reverse = true;
        options.take_profit = true;
        options.stop_loss = true;
        let buy = crate::theme::trading_side_color(&theme, OrderSide::Buy);
        let mut at = [Pos2::ZERO; 7];
        let mut drag_to = Pos2::ZERO;
        for frame in 0..26 {
            let events = match frame {
                3 | 4 => vec![egui::Event::PointerMoved(at[0])],
                6 | 7 => vec![egui::Event::PointerMoved(at[1])],
                9 | 10 => vec![egui::Event::PointerMoved(at[2])],
                12 | 13 => vec![egui::Event::PointerMoved(at[3])],
                15 | 16 => vec![egui::Event::PointerMoved(at[4])],
                18 | 19 => vec![egui::Event::PointerMoved(at[5])],
                5 | 8 | 11 | 14 | 17 | 20 => vec![egui::Event::PointerGone],
                21 => vec![egui::Event::PointerMoved(at[6])],
                22 | 24 => vec![egui::Event::PointerButton {
                    pos: if frame == 22 { at[6] } else { drag_to },
                    button: egui::PointerButton::Primary,
                    pressed: frame == 22,
                    modifiers: egui::Modifiers::NONE,
                }],
                23 => vec![egui::Event::PointerMoved(drag_to)],
                _ => vec![],
            };
            let mut ys = [0.0; 6];
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1000.0, 800.0))),
                    events,
                    ..Default::default()
                },
                |ui| {
                    chart.show(ui);
                    let mapping = chart.get_chart_mapping();
                    ys = [95.0, 97.0, 100.0, 105.0, 110.0, 112.0]
                        .map(|price| mapping.price_to_y(price));
                    drag_to.y = mapping.price_to_y(93.0);
                    let lines = render_trading_lines(ui, &mapping, &orders, [&position], &options);
                    assert!(lines.action.is_none(), "frame {frame}: {:?}", lines.action);
                },
            );
            let [y95, y97, y100, y105, y110, y112] = ys;
            let text = |name: &str| {
                output.shapes.iter().find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == name => Some(text),
                    _ => None,
                })
            };
            let text_at = |name: &str, y: f32| {
                output.shapes.iter().find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text)
                        if text.galley.text() == name
                            && (text.visual_bounding_rect().center().y - y).abs() < 2.0 =>
                    {
                        Some(text.visual_bounding_rect().center())
                    }
                    _ => None,
                })
            };
            let label = |color, y: f32| {
                output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Rect(rect)
                            if rect.stroke.color == color
                                && rect.stroke.width == 1.0
                                && (rect.rect.center().y - y).abs() < 0.1 =>
                        {
                            Some(rect.rect)
                        }
                        _ => None,
                    })
                    .expect("line label")
            };
            if frame == 2 {
                // The same texts, less one cancel or remove cell of the fixed order.
                for (fixed, default, color) in [(y95, y110, buy), (y97, y112, theme.ui.take_profit)]
                {
                    let (fixed, default) = (label(color, fixed), label(color, default));
                    assert!((fixed.width() - (default.width() - HEIGHT)).abs() < 0.01);
                }
                assert!(text_at("TP", y95).is_none() && text_at("SL", y95).is_none());
                assert!(text_at("TP", y110).is_none() && text_at("SL", y110).is_some());
                assert!(
                    !output.shapes.iter().any(|shape| matches!(
                        &shape.shape,
                        egui::Shape::LineSegment { points, stroke }
                            if stroke.width == 1.5 && (points[0].y - y105).abs() < 6.0
                    )),
                    "no close button"
                );
                at = [
                    text_at("TP", y100).expect("plan take profit"),
                    text_at("SL", y100).expect("plan stop loss"),
                    text_at("SL", y110).expect("default stop loss"),
                    text_at("TP", y105).expect("position take profit"),
                    text_at("SL", y105).expect("position stop loss"),
                    label(buy, y105).center(),
                    text_at("Limit", y95).expect("fixed order"),
                ];
                drag_to.x = at[6].x;
            }
            for (hover_frame, expected, replaced) in [
                (4, "Plan target", "Drag to place a take profit"),
                (7, "Plan stop", "Drag to place a stop loss"),
                (10, "Drag to place a stop loss", "Plan stop"),
                (13, "Hedge target", "Drag to place a take profit"),
                (16, "Hedge stop", "Drag to place a stop loss"),
            ] {
                if frame == hover_frame {
                    assert!(text(expected).is_some(), "{expected}");
                    assert!(text(replaced).is_none(), "{replaced}");
                }
            }
            if frame == 19 {
                assert!(
                    !output.shapes.iter().any(|shape| matches!(
                        &shape.shape,
                        egui::Shape::Rect(rect) if rect.stroke.color == buy
                            && rect.rect.size() == vec2(HEIGHT, HEIGHT)
                    )),
                    "no reverse button"
                );
            }
            output.drop_without_applying_deltas();
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
        for row in 0..25 {
            let mut capabilities = full;
            match row {
                5 | 24 => capabilities.modify = false,
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
                position_menu: (8..=14).contains(&row) || (22..=23).contains(&row),
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
                        let mut look = TradingLineAppearance::default();
                        match row {
                            19 | 20 => look.modify = Some(false),
                            21 => look.cancel = Some(false),
                            22 | 23 => look.close = Some(false),
                            24 => look.modify = Some(true),
                            _ => {}
                        }
                        let order_looks = [(OrderId::new("7"), look.clone())];
                        let position_looks = [("SYM".to_owned(), look)];
                        options.order_appearance = &order_looks;
                        options.position_appearance = &position_looks;
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
                1 | 5 | 24 => (
                    "Modify order",
                    (row != 1).then_some("The account does not support modifying orders"),
                ),
                19 => (
                    "Modify order",
                    Some("The order cannot be changed on the chart"),
                ),
                20 => (
                    "Add take profit",
                    Some("The order cannot be changed on the chart"),
                ),
                21 => (
                    "Cancel order",
                    Some("The order cannot be cancelled on the chart"),
                ),
                22 => (
                    "Close position",
                    Some("The position cannot be closed or reversed on the chart"),
                ),
                23 => (
                    "Reverse position",
                    Some("The position cannot be closed or reversed on the chart"),
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
        // Above 25 prices move in 0.05: a drop at 102.03 rounds to 102.05, and the exits keep
        // their count of ticks from the stop.
        const TIERS: &[(f64, f64)] = &[(10.0, 0.02), (25.0, 0.05)];
        for theme in [Theme::light(), Theme::dark()] {
            for (label, limit, drawn, target, expected, take_profit, tiers) in [
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
                    &[][..],
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
                    &[],
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
                    &[],
                ),
                (
                    "Limit",
                    101.5,
                    &[101.0, 101.5],
                    102.03,
                    OrderType::StopLimit {
                        stop: 101.0,
                        limit: 102.05,
                    },
                    103.0,
                    TIERS,
                ),
                (
                    "Stop limit",
                    101.5,
                    &[101.0, 101.5],
                    100.53,
                    OrderType::StopLimit {
                        stop: 100.55,
                        limit: 101.5,
                    },
                    102.55,
                    TIERS,
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
                options.tick_tiers = tiers;
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

    /// A trailing stop's line sits at the stop the broker reports and moves with it, waits at
    /// the activation price before that, is not drawn without either, and cannot be dragged.
    #[test]
    fn trailing_stop_lines_follow_the_reported_stop() {
        for theme in [Theme::light(), Theme::dark()] {
            let sell = crate::theme::trading_side_color(&theme, OrderSide::Sell);
            for (activation, stop, drawn) in [
                (Some(105.0), Some(98.5), Some(98.5)),
                (Some(105.0), Some(99.5), Some(99.5)),
                (Some(105.0), None, Some(105.0)),
                (None, None, None),
            ] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
                let order_type = OrderType::TrailingStop {
                    distance: crate::model::TrailDistance::Price(3.0),
                    activation,
                };
                let request = OrderRequest::new("SYM", OrderSide::Sell, 1.0, order_type);
                let mut order = Order::new(OrderId::new("T"), request, OrderStatus::Working);
                order.trailing_stop = stop;
                let mut options = TradingLineOptions::new("USD", 0.01);
                options.modify = true;
                let mut start = Pos2::ZERO;
                let mut action = None;
                for frame in 0..7 {
                    let end = start + vec2(0.0, 40.0);
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
                            let lines = render_trading_lines(ui, &mapping, [&order], [], &options);
                            if frame == 2 {
                                let expected: Vec<_> =
                                    drawn.into_iter().map(|price| (price, sell)).collect();
                                assert_eq!(lines.tags, expected, "{activation:?} {stop:?}");
                            }
                            action = action.take().or(lines.action);
                        },
                    );
                    if frame == 2 {
                        let label = output.shapes.iter().find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.text() == "Trailing stop" => {
                                Some(text.pos + text.galley.size() / 2.0)
                            }
                            _ => None,
                        });
                        assert_eq!(label.is_some(), drawn.is_some());
                        start = label.unwrap_or(start);
                    }
                    output.drop_without_applying_deltas();
                }
                assert_eq!(action, None, "{activation:?} {stop:?}");
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
                    line_width: 1.0,
                    line_length: 0.0,
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
                    formatter: None,
                    tick_size: 0.01,
                    tick_tiers: &[],
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
