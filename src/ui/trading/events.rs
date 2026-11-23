//! The broker's pushes and answers: applying them to the controller, and the notifications and
//! execution sound they raise.

use super::account_panel::quantity_decimals;
use super::widgets::side_name;
use super::{Numbers, Sent, Trading, changed};
use crate::chart::TradingLineAction;
use crate::data::FeedError;
use crate::localization as l10n;
use crate::model::{
    OrderId, OrderPreview, OrderRequest, OrderStatus, OrderType, TimeInForce, TradingRules,
};
use crate::services::{AccountEvent, HistoryKind, HistoryPage};
use crate::ui_kit::notifications::{Toast, ToastKind, Toasts};
use crate::widget::settings::TradingNotifications;

impl Trading {
    /// Applies the broker's pushes and answers, reporting refused orders, cancellations and
    /// changes, rejected orders, fills and broker cancellations in `toasts`, each naming its
    /// order, and announcing when the broker stops being live and is live again.
    /// A lagged stream is replaced by a new one, on which the broker restates the account: the
    /// order ticket, a previewed order and the quantity calculator stay as the user left them,
    /// a change to an order or a position that waits for confirmation is dropped, and a
    /// confirmed position order that still waits for its trading rules is reported as refused,
    /// as while the broker is not live, and never sent. Call it every frame, even while the
    /// chart is hidden.
    pub fn poll(&mut self, ctx: &egui::Context, toasts: &mut Toasts) {
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        let lagged = self.listener.as_ref().is_some_and(|l| !l.is_active());
        let held = lagged && !self.is_connected();
        if lagged {
            self.listen();
        }
        for (key, answer) in self.inbox.drain().answers {
            if key == self.generation {
                match answer.downcast::<AccountEvent>() {
                    Ok(event) => self.apply(ctx, *event, toasts),
                    Err(_) => self.restating = false,
                }
                continue;
            }
            if let Some(index) = self.placing.iter().position(|(id, _, _)| *id == key) {
                let (_, _, mut order) = self.placing.remove(index);
                if let Ok(answer) = answer.downcast::<Result<TradingRules, FeedError>>() {
                    match *answer {
                        Ok(rules)
                            if rules.symbol == order.symbol
                                && rules.is_valid()
                                && rules.time_in_force[0] != TimeInForce::GoodTillDate =>
                        {
                            order.time_in_force = rules.time_in_force[0];
                            self.place(order);
                        }
                        Err(FeedError::NotSupported) => self.place(order),
                        Err(error) => {
                            let reason = l10n::format_text(
                                ctx,
                                "trading rules unavailable: {error}",
                                &[("error", error.to_string().as_str())],
                            );
                            self.notify(toasts, self.refusal(ctx, &Sent::Place(order), &reason));
                        }
                        Ok(_) => {
                            let reason =
                                l10n::text(ctx, "the broker returned invalid trading rules");
                            self.notify(toasts, self.refusal(ctx, &Sent::Place(order), &reason));
                        }
                    }
                }
                continue;
            }
            if self
                .preview
                .as_ref()
                .is_some_and(|preview| preview.key == key)
            {
                if let Ok(answer) = answer.downcast::<Result<OrderPreview, FeedError>>() {
                    match *answer {
                        Ok(estimate) => {
                            if let Some(preview) = &mut self.preview {
                                preview.estimate = Some(estimate);
                            }
                        }
                        Err(error) => {
                            self.preview = None;
                            self.notify(
                                toasts,
                                Toast::error(l10n::format_text(
                                    ctx,
                                    "Order preview unavailable: {error}",
                                    &[("error", error.to_string().as_str())],
                                )),
                            );
                        }
                    }
                }
                continue;
            }
            if let Some(kind) = self.account_panel.loading(key) {
                if let Ok(answer) = answer.downcast::<Result<HistoryPage, FeedError>>() {
                    self.history_page(ctx, kind, *answer, toasts);
                }
                continue;
            }
            if key == self.rules_key {
                if let Ok(answer) = answer.downcast::<Result<TradingRules, FeedError>>() {
                    match *answer {
                        Ok(rules)
                            if rules.symbol == self.rules.symbol
                                && rules.is_valid()
                                && rules.time_in_force[0] != TimeInForce::GoodTillDate =>
                        {
                            self.rules = rules;
                            self.rules_ready = true;
                        }
                        Err(FeedError::NotSupported) => self.rules_ready = true,
                        Err(error) => {
                            self.rules_failed = !self.connection_error(&error);
                            toasts.error(l10n::format_text(
                                ctx,
                                "Trading rules unavailable: {error}",
                                &[("error", error.to_string().as_str())],
                            ));
                        }
                        Ok(_) => {
                            self.rules_failed = true;
                            toasts.error("The broker returned invalid trading rules");
                        }
                    }
                }
                continue;
            }
            let Some(index) = self.sent.iter().position(|(sent, _)| *sent == key) else {
                continue;
            };
            let (_, sent) = self.sent.remove(index);
            let error = match answer.downcast::<Result<OrderId, FeedError>>() {
                Ok(placed) => placed.err(),
                Err(answer) => answer
                    .downcast::<Result<(), FeedError>>()
                    .ok()
                    .and_then(|done| done.err()),
            };
            if let Some(error) = error {
                if let Sent::Cancel(id, _) = &sent {
                    self.cancelling.remove(id);
                }
                self.notify(toasts, self.refusal(ctx, &sent, &error.to_string()));
            }
        }
        // A new stream counts as live unless the broker restated otherwise: rules held back
        // while the old stream was not live are asked for once the restated account shows it is.
        if held && self.is_connected() {
            self.ask_rules_again();
        }
    }

    pub(super) fn apply(&mut self, ctx: &egui::Context, event: AccountEvent, toasts: &mut Toasts) {
        if !valid_event(&event) {
            return;
        }
        // What the broker pushes while connecting or not live restates the account: only what
        // happens on a live stream is news.
        let announce = !self.restating && self.offline().is_none();
        let first_order = match &event {
            AccountEvent::Order(order) => !self.account_panel.fills.orders.contains_key(&order.id),
            _ => false,
        };
        self.account_panel.record(&event);
        match event {
            AccountEvent::Account(account) => {
                let (kept, withdrawn): (Vec<_>, Vec<_>) = std::mem::take(&mut self.placing)
                    .into_iter()
                    .partition(|(_, _, order)| account.capabilities.accepts(&order.order_type));
                self.placing = kept;
                for (_, _, order) in withdrawn {
                    self.refuse(
                        Sent::Place(order),
                        "the account no longer accepts this order type",
                    );
                }
                if !account.capabilities.preview {
                    self.preview = None;
                }
                if matches!(self.pending, Some(TradingLineAction::Modify(_, _)))
                    && !account.capabilities.modify
                    || matches!(self.pending, Some(TradingLineAction::Close(_)))
                        && (!account.capabilities.close
                            || self.close_quantity.is_some() && !account.capabilities.partial_close)
                    || matches!(self.pending, Some(TradingLineAction::Reverse(_)))
                        && !account.capabilities.reverse
                    || matches!(&self.pending,
                        Some(TradingLineAction::Exit(_, exit))
                        if !account.capabilities.accepts(&exit.order_type))
                {
                    self.pending = None;
                }
                if !account.capabilities.limit && !account.capabilities.stop {
                    self.protecting = None;
                }
                self.account = Some(account);
            }
            AccountEvent::Order(order) => {
                if matches!(&self.pending,
                    Some(TradingLineAction::Cancel(old) | TradingLineAction::Modify(old, _))
                    if old.id == order.id && changed(old, &order))
                {
                    self.pending = None;
                }
                let listed = self.orders.iter().any(|working| working.id == order.id);
                self.orders.retain(|working| working.id != order.id);
                let asked =
                    order.status != OrderStatus::Working && self.cancelling.remove(&order.id);
                let price = match order.request.order_type {
                    OrderType::Market => None,
                    OrderType::Limit(price) | OrderType::Stop(price) => Some(price),
                    OrderType::StopLimit { stop, .. } => Some(stop),
                    // Before it trails, a trailing stop waits at its activation price.
                    OrderType::TrailingStop { activation, .. } => {
                        order.trailing_stop.or(activation)
                    }
                };
                if announce
                    && first_order
                    && order.status == OrderStatus::Working
                    && let Some(price) = price
                {
                    let chart = (self.chart_logo.as_ref()).map_or("", |(chart, _)| chart.as_str());
                    self.notify(
                        toasts,
                        order_toast(
                            ctx,
                            &order.request,
                            price,
                            order.request.quantity,
                            &self.numbers(&order.request.symbol, chart),
                            false,
                            self.logo(&order.request.symbol),
                        ),
                    );
                }
                match &order.status {
                    OrderStatus::Working => self.orders.push(order),
                    OrderStatus::Rejected(reason) if announce => {
                        let chart =
                            (self.chart_logo.as_ref()).map_or("", |(chart, _)| chart.as_str());
                        let request = &order.request;
                        let numbers = self.numbers(&request.symbol, chart);
                        let mut toast = Toast::error(l10n::format_text(
                            ctx,
                            "Order rejected: {order}: {reason}",
                            &[
                                ("order", order_text(ctx, request, &numbers).as_str()),
                                ("reason", reason.as_str()),
                            ],
                        ))
                        .with_title(l10n::format_text(
                            ctx,
                            "{order} order rejected: {reason}",
                            &[
                                ("order", l10n::text(ctx, order_name(request)).as_str()),
                                ("reason", reason.as_str()),
                            ],
                        ));
                        let price = match request.order_type {
                            OrderType::Market => l10n::text(ctx, "market"),
                            OrderType::Limit(price) | OrderType::Stop(price) => {
                                numbers.price(price)
                            }
                            OrderType::StopLimit { stop, .. } => numbers.price(stop),
                            OrderType::TrailingStop { distance, .. } => {
                                numbers.trail(ctx, distance)
                            }
                        };
                        toast.trading = Some(crate::ui_kit::notifications::TradingToast {
                            symbol: request.symbol.clone(),
                            logo: self.logo(&request.symbol).map(str::to_owned),
                            side: request.side,
                            quantity: numbers
                                .quantity(request.quantity, quantity_decimals(request.quantity)),
                            price,
                            executed: false,
                        });
                        self.notify(toasts, toast);
                    }
                    OrderStatus::Cancelled if announce && listed && !asked => {
                        let chart =
                            (self.chart_logo.as_ref()).map_or("", |(chart, _)| chart.as_str());
                        let numbers = self.numbers(&order.request.symbol, chart);
                        let text = order_text(ctx, &order.request, &numbers);
                        self.notify(
                            toasts,
                            Toast::info(l10n::format_text(
                                ctx,
                                "Cancelled: {order}",
                                &[("order", text.as_str())],
                            )),
                        );
                    }
                    _ => {}
                }
            }
            AccountEvent::Position(position) => {
                let (kept, withdrawn): (Vec<_>, Vec<_>) = std::mem::take(&mut self.placing)
                    .into_iter()
                    .partition(|(_, old, _)| {
                        old.symbol != position.symbol
                            || (old.quantity == position.quantity
                                && old.average_price == position.average_price)
                    });
                self.placing = kept;
                for (_, _, order) in withdrawn {
                    self.refuse(
                        Sent::Place(order),
                        "the position changed before the order was sent",
                    );
                }
                if matches!(&self.pending,
                    Some(TradingLineAction::Close(old) | TradingLineAction::Reverse(old) | TradingLineAction::Exit(old, _))
                    if old.symbol == position.symbol
                        && (old.quantity != position.quantity || old.average_price != position.average_price))
                {
                    self.pending = None;
                }
                if self.protecting.as_ref().is_some_and(|(old, _, _)| {
                    old.symbol == position.symbol
                        && (old.quantity != position.quantity
                            || old.average_price != position.average_price)
                }) {
                    self.protecting = None;
                }
                self.positions.retain(|open| open.symbol != position.symbol);
                if position.quantity != 0.0 {
                    self.positions.push(position);
                }
            }
            AccountEvent::Execution(fill) if announce => {
                let order = self.account_panel.fills.orders.get(&fill.order);
                // The order's symbol, or the fill's own when the order is unknown.
                let symbol = self.account_panel.fills.symbol(&fill);
                let chart = (self.chart_logo.as_ref()).map_or("", |(chart, _)| chart.as_str());
                let numbers = self.numbers(symbol.unwrap_or(""), chart);
                let side = l10n::text(ctx, side_name(fill.side));
                let quantity = numbers.quantity(fill.quantity, quantity_decimals(fill.quantity));
                let price = numbers.price(fill.price);
                let text = match symbol {
                    Some(symbol) => l10n::format_text(
                        ctx,
                        "{side} {quantity} {symbol} at {price}",
                        &[
                            ("side", side.as_str()),
                            ("quantity", quantity.as_str()),
                            ("symbol", symbol),
                            ("price", price.as_str()),
                        ],
                    ),
                    None => l10n::format_text(
                        ctx,
                        "{side} {quantity} at {price}",
                        &[
                            ("side", side.as_str()),
                            ("quantity", quantity.as_str()),
                            ("price", price.as_str()),
                        ],
                    ),
                };
                let filled = self.account_panel.fills.filled_quantity(&fill.order);
                // Fills that add up to the order within rounding complete it.
                let mut toast = match order {
                    Some(order) => {
                        let mut executed = order.clone();
                        executed.side = fill.side;
                        order_toast(
                            ctx,
                            &executed,
                            fill.price,
                            fill.quantity,
                            &numbers,
                            true,
                            self.logo(&executed.symbol),
                        )
                    }
                    None => Toast::success(l10n::format_text(
                        ctx,
                        "Filled: {fill}",
                        &[("fill", text.as_str())],
                    )),
                };
                if let Some(order) = order
                    && filled < order.quantity * (1.0 - 1e-9)
                {
                    // Round binary fraction noise so the host's formatter gets 0.3 for 0.1 + 0.2.
                    let decimals = quantity_decimals(filled);
                    let scale = 10f64.powi(decimals as i32);
                    let filled = numbers.quantity((filled * scale).round() / scale, decimals);
                    let total = numbers.quantity(order.quantity, quantity_decimals(order.quantity));
                    toast.kind = ToastKind::Info;
                    toast.title = Some(l10n::format_text(
                        ctx,
                        "{order} order partially executed ({filled} of {quantity}) on",
                        &[
                            ("order", l10n::text(ctx, order_name(order)).as_str()),
                            ("filled", filled.as_str()),
                            ("quantity", total.as_str()),
                        ],
                    ));
                    toast.message = l10n::format_text(
                        ctx,
                        "Partially filled: {fill} ({filled} of {quantity})",
                        &[
                            ("fill", text.as_str()),
                            ("filled", filled.as_str()),
                            ("quantity", total.as_str()),
                        ],
                    );
                }
                self.notify(toasts, toast);
                if self.display.play_execution_sound {
                    self.sound = Some(self.display.execution_sound.clone());
                }
            }
            AccountEvent::Status(status) => {
                let was_connected = self.is_connected();
                self.connection = Some(status);
                match (was_connected, self.is_connected()) {
                    // Confirmed while the broker was live, these would be sent once their rules
                    // arrive, after it is back: refuse them now instead.
                    (true, false) => {
                        for (_, _, order) in std::mem::take(&mut self.placing) {
                            self.refuse(Sent::Place(order), super::NOT_LIVE);
                        }
                        if !self.restating {
                            self.connection_lost = true;
                            let lost =
                                "The broker disconnected: orders are not sent until it is back";
                            self.notify(toasts, Toast::error(l10n::text(ctx, lost)));
                        }
                    }
                    // Ask once more for what the broker may not have answered meanwhile.
                    (false, true) => {
                        self.ask_rules_again();
                        self.account_panel.resume();
                        if !self.restating && std::mem::take(&mut self.connection_lost) {
                            let back = l10n::text(ctx, "The broker is connected again");
                            self.notify(toasts, Toast::success(back));
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }

    /// Lists a page of older history the broker answered for `kind`, or stops paging there.
    fn history_page(
        &mut self,
        ctx: &egui::Context,
        kind: HistoryKind,
        answer: Result<HistoryPage, FeedError>,
        toasts: &mut Toasts,
    ) {
        match answer {
            Ok(page) => {
                let events = page.events.into_iter().filter(valid_event).collect();
                self.account_panel.page(kind, events, page.next);
            }
            Err(error) => {
                if error != FeedError::NotSupported && self.connection_error(&error) {
                    // Asked again once the broker is live.
                    self.account_panel.defer(kind);
                } else {
                    self.account_panel.end(kind);
                }
                if error != FeedError::NotSupported {
                    toasts.error(l10n::format_text(
                        ctx,
                        "Account history unavailable: {error}",
                        &[("error", error.to_string().as_str())],
                    ));
                }
            }
        }
    }

    /// Whether a request that failed with `error` may succeed once the broker is live again: the
    /// error is recoverable, or the broker reported meanwhile that it is not live.
    fn connection_error(&self, error: &FeedError) -> bool {
        error.is_recoverable() || !self.is_connected()
    }

    /// The sound to play for the fills since the last call, when the user turned on
    /// [`TradingSettings::play_execution_sound`](crate::widget::settings::TradingSettings::play_execution_sound): the name the user chose, or empty for the
    /// host's first sound. The host plays it; call this after [`poll`](Self::poll).
    pub fn take_execution_sound(&mut self) -> Option<String> {
        self.sound.take()
    }

    /// Shows `toast` if the user's [`TradingSettings::notifications`] level includes it.
    fn notify(&self, toasts: &mut Toasts, toast: Toast) {
        let shown = match self.display.notifications {
            TradingNotifications::All => true,
            TradingNotifications::Errors => toast.kind == ToastKind::Error,
            TradingNotifications::Off => false,
        };
        if shown {
            toasts.add(toast);
        }
    }

    /// Describes the refused action with the order's own number formatting.
    fn refusal(&self, ctx: &egui::Context, sent: &Sent, reason: &str) -> Toast {
        let describe = |request: &OrderRequest| {
            let chart = (self.chart_logo.as_ref()).map_or("", |(chart, _)| chart.as_str());
            order_text(ctx, request, &self.numbers(&request.symbol, chart))
        };
        let (english, order) = match sent {
            Sent::Place(request) => ("Order refused: {order}: {error}", describe(request)),
            Sent::Modify(request) => ("Change refused: {order}: {error}", describe(request)),
            Sent::Cancel(id, request) => (
                "Cancel refused: {order}: {error}",
                request.as_ref().map_or_else(
                    || l10n::format_text(ctx, "order {id}", &[("id", id.to_string())]),
                    describe,
                ),
            ),
        };
        Toast::error(l10n::format_text(
            ctx,
            english,
            &[("order", order.as_str()), ("error", reason)],
        ))
    }
}

/// Whether every price and quantity of `event` is finite, quantities are positive and an
/// order's filled quantity lies between 0 and its quantity, as [`Broker`](crate::services::Broker)
/// requires; the terminal skips events that break this.
fn valid_event(event: &AccountEvent) -> bool {
    let finite = |value: Option<f64>| value.is_none_or(f64::is_finite);
    let positive = |value: f64| value.is_finite() && value > 0.0;
    match event {
        AccountEvent::Account(account) => {
            account.balance.is_finite()
                && [
                    account.equity,
                    account.realized_pnl,
                    account.margin,
                    account.margin_buffer,
                    account.orders_margin,
                    account.available_funds,
                ]
                .into_iter()
                .all(finite)
        }
        AccountEvent::Order(order) => {
            let request = &order.request;
            positive(request.quantity)
                && order.filled.is_finite()
                && (0.0..=request.quantity).contains(&order.filled)
                && finite(order.fill_price)
                && finite(request.take_profit)
                && request.take_profit_levels.iter().all(|level| {
                    level.price.is_finite() && positive(level.quantity) && finite(level.stop_loss)
                })
                && finite(request.stop_loss)
                && finite(order.trailing_stop)
                && match request.order_type {
                    OrderType::Market => true,
                    OrderType::Limit(price) | OrderType::Stop(price) => price.is_finite(),
                    OrderType::StopLimit { stop, limit } => stop.is_finite() && limit.is_finite(),
                    OrderType::TrailingStop {
                        distance,
                        activation,
                    } => distance.is_valid() && finite(activation),
                }
        }
        AccountEvent::Position(position) => {
            position.quantity.is_finite()
                && position.average_price.is_finite()
                && [
                    position.unrealized_pnl,
                    position.last_price,
                    position.trade_value,
                    position.market_value,
                    position.leverage,
                    position.margin,
                ]
                .into_iter()
                .all(finite)
        }
        AccountEvent::Execution(fill) => positive(fill.quantity) && fill.price.is_finite(),
        AccountEvent::ClosedPosition(closed) => {
            positive(closed.quantity)
                && closed.average_price.is_finite()
                && closed.exit_price.is_finite()
                && closed.realized_pnl.is_finite()
        }
        AccountEvent::Balance(change) => change.balance.is_finite() && change.amount.is_finite(),
        _ => true,
    }
}

/// Names an order's side, quantity, symbol and type, retaining both stop-limit prices.
fn order_text(ctx: &egui::Context, request: &OrderRequest, numbers: &Numbers<'_>) -> String {
    use crate::model::PositionBracket;
    let (english, price, limit) = match (request.position_bracket, request.order_type) {
        (_, OrderType::Market) => ("{side} {quantity} {symbol} at market", None, None),
        (
            _,
            OrderType::TrailingStop {
                activation: Some(activation),
                ..
            },
        ) => (
            "{side} {quantity} {symbol} trailing stop {distance} from {price}",
            Some(activation),
            None,
        ),
        (_, OrderType::TrailingStop { .. }) => (
            "{side} {quantity} {symbol} trailing stop {distance}",
            None,
            None,
        ),
        (_, OrderType::StopLimit { stop, limit }) => (
            "{side} {quantity} {symbol} stop {price} limit {limit}",
            Some(stop),
            Some(limit),
        ),
        (Some(PositionBracket::TakeProfit), OrderType::Limit(price) | OrderType::Stop(price)) => (
            "{side} {quantity} {symbol} take profit {price}",
            Some(price),
            None,
        ),
        (Some(PositionBracket::StopLoss), OrderType::Limit(price) | OrderType::Stop(price)) => (
            "{side} {quantity} {symbol} stop loss {price}",
            Some(price),
            None,
        ),
        (None, OrderType::Limit(price)) => (
            "{side} {quantity} {symbol} limit {price}",
            Some(price),
            None,
        ),
        (None, OrderType::Stop(price)) => {
            ("{side} {quantity} {symbol} stop {price}", Some(price), None)
        }
    };
    let price_text =
        |value: Option<f64>| value.map(|value| numbers.price(value)).unwrap_or_default();
    let distance = match request.order_type {
        OrderType::TrailingStop { distance, .. } => numbers.trail(ctx, distance),
        _ => String::new(),
    };
    l10n::format_text(
        ctx,
        english,
        &[
            ("side", l10n::text(ctx, side_name(request.side))),
            (
                "quantity",
                numbers.quantity(request.quantity, quantity_decimals(request.quantity)),
            ),
            ("symbol", request.symbol.clone()),
            ("price", price_text(price)),
            ("limit", price_text(limit)),
            ("distance", distance),
        ],
    )
}

fn order_name(request: &OrderRequest) -> &'static str {
    use crate::model::PositionBracket;
    match request.position_bracket {
        Some(PositionBracket::TakeProfit) => "Take profit",
        Some(PositionBracket::StopLoss) => "Stop loss",
        None => match request.order_type {
            OrderType::Market => "Market",
            OrderType::Limit(_) => "Limit",
            OrderType::Stop(_) => "Stop",
            OrderType::StopLimit { .. } => "Stop limit",
            OrderType::TrailingStop { .. } => "Trailing stop",
        },
    }
}

fn order_toast(
    ctx: &egui::Context,
    request: &OrderRequest,
    price: f64,
    quantity: f64,
    numbers: &Numbers<'_>,
    executed: bool,
    logo: Option<&str>,
) -> Toast {
    let side = l10n::text(ctx, side_name(request.side));
    let price = numbers.price(price);
    let quantity = numbers.quantity(quantity, quantity_decimals(quantity));
    let fill = l10n::format_text(
        ctx,
        "{side} {quantity} {symbol} at {price}",
        &[
            ("side", side.as_str()),
            ("quantity", quantity.as_str()),
            ("symbol", request.symbol.as_str()),
            ("price", price.as_str()),
        ],
    );
    let mut toast = Toast::new(
        if executed {
            ToastKind::Success
        } else {
            ToastKind::Info
        },
        l10n::format_text(
            ctx,
            if executed {
                "Filled: {fill}"
            } else {
                "Placed: {fill}"
            },
            &[("fill", fill.as_str())],
        ),
    )
    .with_title(l10n::format_text(
        ctx,
        if executed {
            "{order} order executed on"
        } else {
            "{order} order placed on"
        },
        &[("order", l10n::text(ctx, order_name(request)).as_str())],
    ));
    toast.trading = Some(crate::ui_kit::notifications::TradingToast {
        symbol: request.symbol.clone(),
        logo: logo.map(str::to_owned),
        side: request.side,
        quantity,
        price,
        executed,
    });
    toast
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::selection::SeriesId;
    use crate::model::{Account, Capabilities, Execution, Order, OrderSide, Position};
    use crate::ui::trading::tests::Recorder;
    use crate::widget::Chart;
    use crate::widget::settings::TradingSettings;
    use std::sync::Arc;

    /// Broker cancellations are news once, but a user's cancellation stays silent whether its
    /// push arrives before or after the answer; only All includes that information notice.
    #[test]
    fn broker_cancellations_are_announced_once() {
        let ctx = egui::Context::default();
        let order = |id, side, quantity, kind| {
            Order::new(
                OrderId::new(id),
                OrderRequest::new("SYM", side, quantity, kind),
                OrderStatus::Working,
            )
        };
        let a = order("1", OrderSide::Buy, 1.0, OrderType::Limit(95.0));
        let b = order("2", OrderSide::Buy, 1.0, OrderType::Limit(94.0));
        let c = order("3", OrderSide::Sell, 2.0, OrderType::Stop(90.0));
        let cancelled = |order: &Order| {
            AccountEvent::Order(Order::new(
                order.id.clone(),
                order.request.clone(),
                OrderStatus::Cancelled,
            ))
        };
        for level in TradingNotifications::ALL {
            let broker = Arc::new(Recorder {
                state: [&a, &b, &c]
                    .map(|order| AccountEvent::Order(order.clone()))
                    .into(),
                ..Recorder::default()
            });
            let mut trading = Trading::default();
            trading.display.notifications = level;
            trading.set_service(broker.clone());
            let mut toasts = Toasts::new();
            trading.poll(&ctx, &mut toasts);
            trading.execute(Some(TradingLineAction::Cancel(a.clone())));
            trading.poll(&ctx, &mut toasts);
            trading.apply(&ctx, cancelled(&a), &mut toasts);
            trading.execute(Some(TradingLineAction::Cancel(b.clone())));
            trading.apply(&ctx, cancelled(&b), &mut toasts);
            trading.poll(&ctx, &mut toasts);
            for _ in 0..2 {
                trading.apply(&ctx, cancelled(&c), &mut toasts);
            }
            let unseen = order("4", OrderSide::Sell, 2.0, OrderType::Stop(90.0));
            trading.apply(&ctx, cancelled(&unseen), &mut toasts);
            let messages: Vec<_> = toasts
                .toasts()
                .iter()
                .map(|toast| toast.message.as_str())
                .collect();
            let expected = if level == TradingNotifications::All {
                vec!["Cancelled: Sell 2 SYM stop 90.00"]
            } else {
                vec![]
            };
            assert_eq!(messages, expected, "{level:?}");
            assert_eq!(
                *broker.cancelled.lock().unwrap(),
                [a.id.clone(), b.id.clone()]
            );
        }
    }

    /// Only live connection transitions are news: no initial or restated status is announced,
    /// repeated offline/live statuses are quiet, and Errors retains the loss alone.
    #[test]
    fn losing_and_regaining_the_broker_raise_one_notice_each() {
        use crate::data::FeedStatus;
        let ctx = egui::Context::default();
        for level in TradingNotifications::ALL {
            let broker = Arc::new(Recorder {
                state: vec![AccountEvent::Status(FeedStatus::Connecting)],
                ..Recorder::default()
            });
            let mut trading = Trading::default();
            trading.display.notifications = level;
            trading.set_service(broker);
            let mut toasts = Toasts::new();
            trading.poll(&ctx, &mut toasts);
            for status in [
                FeedStatus::Live,
                FeedStatus::Disconnected,
                FeedStatus::Connecting,
                FeedStatus::Live,
                FeedStatus::Live,
            ] {
                trading.apply(&ctx, AccountEvent::Status(status), &mut toasts);
            }
            trading.listen();
            trading.poll(&ctx, &mut toasts);
            trading.apply(&ctx, AccountEvent::Status(FeedStatus::Live), &mut toasts);
            let messages: Vec<_> = toasts
                .toasts()
                .iter()
                .map(|toast| toast.message.as_str())
                .collect();
            let lost = "The broker disconnected: orders are not sent until it is back";
            let expected = match level {
                TradingNotifications::All => vec![lost, "The broker is connected again"],
                TradingNotifications::Errors => vec![lost],
                TradingNotifications::Off => vec![],
            };
            assert_eq!(messages, expected, "{level:?}");
        }
    }

    /// Refusals identify the action and the unchanged order, including both stop-limit prices
    /// and an id fallback after an order has left the working list.
    #[test]
    fn refusals_name_the_action_and_the_order() {
        use crate::data::Reply;
        use crate::services::{Broker, Events};
        struct Closed;
        impl Broker for Closed {
            fn connect(&self, _: Events<AccountEvent>) {}
            fn place(&self, _: OrderRequest, reply: Reply<OrderId>) {
                reply.send(Err(FeedError::Other("market closed".into())));
            }
            fn cancel(&self, _: OrderId, reply: Reply<()>) {
                reply.send(Err(FeedError::Other("market closed".into())));
            }
            fn modify(&self, _: OrderId, _: OrderRequest, reply: Reply<()>) {
                reply.send(Err(FeedError::Other("market closed".into())));
            }
        }
        let ctx = egui::Context::default();
        let mut trading = Trading::default();
        trading.set_service(Arc::new(Closed));
        trading.poll(&ctx, &mut Toasts::new());
        let listed = Order::new(
            OrderId::new("7"),
            OrderRequest::new("ES", OrderSide::Sell, 1.0, OrderType::Stop(4400.0)),
            OrderStatus::Working,
        );
        let mut messages = Vec::new();
        for row in 0..6 {
            match row {
                0 => trading.place(OrderRequest::new(
                    "ES",
                    OrderSide::Buy,
                    2.0,
                    OrderType::Limit(4500.25),
                )),
                1 => trading.place(OrderRequest::new(
                    "ES",
                    OrderSide::Sell,
                    3.0,
                    OrderType::Market,
                )),
                2 => trading.place(OrderRequest::new(
                    "ES",
                    OrderSide::Buy,
                    1.0,
                    OrderType::StopLimit {
                        stop: 4510.0,
                        limit: 4512.5,
                    },
                )),
                3 => trading.execute(Some(TradingLineAction::Cancel(listed.clone()))),
                4 => trading.cancel_order(OrderId::new("9"), None),
                _ => {
                    let mut moved = listed.request.clone();
                    moved.order_type = OrderType::Stop(4390.0);
                    trading.execute(Some(TradingLineAction::Modify(listed.clone(), moved)));
                }
            }
            let mut toasts = Toasts::new();
            trading.poll(&ctx, &mut toasts);
            messages.push(
                toasts
                    .toasts()
                    .iter()
                    .map(|toast| toast.message.clone())
                    .collect::<Vec<_>>(),
            );
        }
        assert_eq!(
            messages,
            [
                ["Order refused: Buy 2 ES limit 4,500.25: market closed"],
                ["Order refused: Sell 3 ES at market: market closed"],
                ["Order refused: Buy 1 ES stop 4,510.00 limit 4,512.50: market closed"],
                ["Cancel refused: Sell 1 ES stop 4,400.00: market closed"],
                ["Cancel refused: order 9: market closed"],
                ["Change refused: Sell 1 ES stop 4,400.00: market closed"],
            ]
        );
    }

    /// The fills of one side in one bar are one mark: filled triangles stacked outward just under
    /// the bar's low for buys and just over its high for sells, up to three and then a "+N" line;
    /// a fill of another symbol is not drawn. Labels, on by default, give the side and total
    /// quantity; hovering a mark lists up to 20 of its fills; the position's label is drawn over
    /// the marks. A partial fill, the fill that completes an order and a rejection each raise a
    /// notification, the rejection with the broker's reason. The display options hide the fills,
    /// the orders and the positions.
    #[test]
    fn fills_are_drawn_on_their_bar_and_announced() {
        use crate::theme::ThemeExt;
        let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
        chart.symbol = "SYM".into();
        let time = chart.bars(SeriesId::MAIN)[95].time + chrono::Duration::hours(12);
        let order = |id, symbol, status, filled| {
            let request = OrderRequest::new(symbol, OrderSide::Buy, 3.0, OrderType::Limit(95.0));
            let mut order = Order::new(OrderId::new(id), request, status);
            order.filled = filled;
            order
        };
        let fill = |id, side, price, quantity| {
            AccountEvent::Execution(Execution::new(
                OrderId::new(id),
                time,
                side,
                price,
                quantity,
            ))
        };
        let broker = Arc::new(Recorder {
            state: vec![
                AccountEvent::Order(order("1", "SYM", OrderStatus::Working, 0.0)),
                fill("1", OrderSide::Buy, 95.0, 1.0),
                AccountEvent::Order(order("1", "SYM", OrderStatus::Filled, 3.0)),
                fill("1", OrderSide::Buy, 98.0, 2.0),
                AccountEvent::Order(order("2", "OTHER", OrderStatus::Filled, 3.0)),
                fill("2", OrderSide::Sell, 95.0, 3.0),
                AccountEvent::Order(order("5", "SYM", OrderStatus::Filled, 3.0)),
                fill("5", OrderSide::Sell, 95.0, 3.0),
                AccountEvent::Order(order(
                    "3",
                    "SYM",
                    OrderStatus::Rejected("Insufficient margin".into()),
                    0.0,
                )),
                AccountEvent::Order(order("4", "SYM", OrderStatus::Working, 0.0)),
                AccountEvent::Position(Position::new("SYM", 3.0, 97.0)),
            ],
            ..Recorder::default()
        });
        // Each notification level shows its share of the events; fills ask for the chosen sound
        // once, and only when the user turned it on.
        let all = [
            "Placed: Buy 3 SYM at 95.00",
            "Partially filled: Buy 1 SYM at 95.00 (1 of 3)",
            "Filled: Buy 2 SYM at 98.00",
            "Filled: Sell 3 OTHER at 95.00",
            "Filled: Sell 3 SYM at 95.00",
            "Order rejected: Buy 3 SYM limit 95.00: Insufficient margin",
            "Placed: Buy 3 SYM at 95.00",
        ];
        for (level, expected, sound) in [
            (TradingNotifications::Errors, &all[5..6], true),
            (TradingNotifications::Off, &[][..], false),
            (TradingNotifications::All, &all[..], false),
        ] {
            let mut trading = Trading::default();
            trading.display.notifications = level;
            trading.display.play_execution_sound = sound;
            trading.display.execution_sound = "Bell".into();
            trading.set_service(Arc::new(Recorder::default()));
            trading.poll(&egui::Context::default(), &mut Toasts::new());
            let mut toasts = Toasts::new();
            for event in broker.state.clone() {
                trading.apply(&egui::Context::default(), event, &mut toasts);
            }
            let messages: Vec<_> = toasts.toasts().iter().map(|t| t.message.as_str()).collect();
            assert_eq!(messages, expected, "{level:?}");
            assert_eq!(
                trading.take_execution_sound(),
                sound.then(|| "Bell".to_owned())
            );
            assert_eq!(trading.take_execution_sound(), None);
        }
        let mut trading = Trading::default();
        trading.set_service(broker);
        trading.poll(&egui::Context::default(), &mut Toasts::new());

        let ctx = egui::Context::default();
        let hidden = TradingSettings {
            show_positions: false,
            show_orders: false,
            show_executions: false,
            ..TradingSettings::default()
        };
        let unlabelled = TradingSettings {
            show_execution_labels: false,
            ..TradingSettings::default()
        };
        let mut clock = 0.0;
        let crowd = trading.account_panel.fills.executions[0].clone();
        let default = TradingSettings::default;
        for (display, drawn, labels, neighbouring, inverted, crowded) in [
            (default(), true, true, false, false, false),
            (unlabelled.clone(), true, false, false, false, false),
            (default(), true, true, true, false, false),
            (default(), true, true, false, true, false),
            (default(), true, true, false, false, true),
            (unlabelled, true, false, false, false, true),
            (hidden, false, false, false, false, false),
        ] {
            let clones = if crowded { 20 } else { 0 };
            let kept = trading.account_panel.fills.executions.len();
            trading
                .account_panel
                .fills
                .executions
                .extend(std::iter::repeat_n(crowd.clone(), clones));
            trading.display = display;
            chart.price_scale_mut().options_mut().invert_scale = inverted;
            trading.account_panel.fills.executions[0].time =
                time - chrono::Duration::days(i64::from(neighbouring));
            let (mut tips, mut texts, mut hovered, mut covered) =
                (Vec::new(), Vec::new(), false, false);
            for frame in 0..4 {
                clock += 1.0;
                let (x, low) = (
                    chart.index_to_x(95.0),
                    chart
                        .get_chart_mapping()
                        .price_to_y(if inverted { 110.0 } else { 90.0 }),
                );
                // The pointer rests on the buy mark once it has been drawn.
                let events = x
                    .filter(|_| frame == 1)
                    .map(|x| egui::Event::PointerMoved(egui::pos2(x, low + 10.0)));
                let input = egui::RawInput {
                    time: Some(clock),
                    events: events.into_iter().collect(),
                    ..Default::default()
                };
                let output = ctx.run_ui(input, |ui| {
                    chart.show(ui);
                    trading.show(ui, &mut chart);
                });
                let (buy, sell) = (
                    ctx.active_theme().ui.accent,
                    ctx.active_theme().chart.candle_down,
                );
                let triangles: Vec<_> = output
                    .shapes
                    .iter()
                    .enumerate()
                    .filter_map(|(at, shape)| match &shape.shape {
                        egui::Shape::Path(path)
                            if path.closed
                                && path.points.len() == 3
                                && (path.fill == buy || path.fill == sell) =>
                        {
                            Some((at, path.points.clone()))
                        }
                        _ => None,
                    })
                    .collect();
                tips = triangles.iter().map(|(_, points)| points.clone()).collect();
                texts = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text)
                            if !text.galley.text().contains(" at ")
                                && (text.galley.text().starts_with("Buy ")
                                    || text.galley.text().starts_with("Sell ")
                                    || (text.galley.text().strip_prefix('+'))
                                        .is_some_and(|more| more.parse::<usize>().is_ok())) =>
                        {
                            Some(text.galley.text().to_owned())
                        }
                        _ => None,
                    })
                    .collect();
                // 1_700_000_000 is 2023-11-14 22:13:20 UTC; 95 days and 12 hours later. The fill
                // moved to bar 94 is listed by its own mark.
                let mut tooltip = "Buy 2 at 98.00, Sun 18 Feb '24 10:13:20".to_owned();
                if !neighbouring {
                    tooltip.insert_str(0, "Buy 1 at 95.00, Sun 18 Feb '24 10:13:20\n");
                }
                if crowded {
                    tooltip += &"\nBuy 1 at 95.00, Sun 18 Feb '24 10:13:20".repeat(18);
                    tooltip += "\nand 2 more";
                }
                hovered = output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == tooltip)
                });
                // The last shape filled in the buy colour is a quantity box of the lines; the
                // price axis tags come before the marks.
                let label = output.shapes.iter().rposition(
                    |shape| matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == buy),
                );
                covered = triangles
                    .last()
                    .zip(label)
                    .is_some_and(|((mark, _), label)| *mark > label);
                output.drop_without_applying_deltas();
            }
            let x = chart.index_to_x(95.0).unwrap();
            let mapping = chart.get_chart_mapping();
            let (a, b) = (mapping.price_to_y(90.0), mapping.price_to_y(110.0));
            let (low, high) = (a.max(b), a.min(b));
            // Tip, then the base's corners: 14 wide and 17 tall, 4 points from the bar. The buys
            // stack 19 points apart, leaving 2 points between them; the sell mirrors over the high.
            // The newest marks come first, a bar's sells after its buys.
            let triangle = |x: f32, tip: f32, base: f32| {
                vec![
                    egui::pos2(x, tip),
                    egui::pos2(x + 7.0, base),
                    egui::pos2(x - 7.0, base),
                ]
            };
            let mut expected = vec![
                triangle(x, high - 4.0, high - 21.0),
                triangle(x, low + 4.0, low + 21.0),
            ];
            if neighbouring {
                // The moved fill stays under its own bar, 94.
                let x = chart.index_to_x(94.0).unwrap();
                expected.push(triangle(x, low + 4.0, low + 21.0));
            } else {
                expected.push(triangle(x, low + 23.0, low + 40.0));
            }
            if crowded {
                expected.push(triangle(x, low + 42.0, low + 59.0));
            }
            assert_eq!(tips, if drawn { expected } else { Vec::new() });
            // 22 buys with a total quantity of 23, 3 of them drawn.
            let labels_expected: &[&str] = match (labels, crowded, neighbouring) {
                (true, true, _) => &["Sell 3", "+19", "Buy 23"],
                (false, true, _) => &["+19"],
                (true, false, true) => &["Sell 3", "Buy 2", "Buy 1"],
                (true, false, false) => &["Sell 3", "Buy 3"],
                (false, false, _) => &[],
            };
            assert_eq!(texts, labels_expected);
            assert_eq!(hovered, drawn, "hovering a mark lists its fills");
            assert!(!covered, "the position's label is drawn over the marks");
            assert_eq!(chart.trading_tags.is_empty(), !drawn);
            trading.account_panel.fills.executions.truncate(kept);
        }

        // A finer fill stays on the only revealed candle, with an exclusive replay cutoff.
        let time = chrono::DateTime::from_timestamp(1_704_067_200, 0).unwrap();
        trading.display.show_executions = true;
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for width in [1440.0, 320.0] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut chart = Chart::new(crate::model::BarData::from_bars(vec![
                    crate::model::Bar::new(time, 100.0, 110.0, 90.0, 100.0, 1.0),
                ]));
                chart.symbol = "SYM".into();
                chart.set_timeframe(crate::model::Timeframe::Min1);
                chart.set_replay_time(Some(time + chrono::Duration::minutes(1)));
                for (seconds, expected) in [(0, true), (30, true), (-1, false), (60, false)] {
                    for fill in &mut trading.account_panel.fills.executions {
                        fill.time = time + chrono::Duration::seconds(seconds);
                    }
                    let mut marked = false;
                    for _ in 0..3 {
                        let output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(width, 600.0),
                                )),
                                ..Default::default()
                            },
                            |ui| {
                                chart.show(ui);
                                trading.show(ui, &mut chart);
                            },
                        );
                        marked |= output.shapes.iter().any(|shape| {
                            matches!(&shape.shape, egui::Shape::Text(text)
                                if text.galley.text() == "Buy 3")
                        });
                        output.drop_without_applying_deltas();
                        chart.fit_content();
                    }
                    assert_eq!(marked, expected, "fill at {seconds} seconds, width {width}");
                }
            }
        }
    }

    /// Notifications write each order's prices and quantities as the account tables do: prices
    /// in the decimals of the order's symbol rather than the chart's, both through the host's
    /// formatter for the order's symbol or else in the built-in text, in the title, the message
    /// and on the card, a rejection with its reason in the title, and a partial fill's running
    /// total without the noise of adding fills up.
    /// A fill whose order is unknown uses its own symbol and that symbol's host text.
    /// A regression announces a five-decimal symbol at the chart's two, skips the formatter, or
    /// writes fills of 0.1 and 0.2 as 0.30000000000000004 filled.
    #[test]
    fn notices_write_numbers_in_the_order_symbols_text() {
        use crate::model::Instrument;
        use crate::ui::trading::TradingFormatter;
        struct Host;
        impl TradingFormatter for Host {
            fn price(&self, symbol: &str, price: f64) -> Option<String> {
                (symbol == "ZN")
                    .then(|| format!("{}'{:02}", price.trunc(), (price.fract() * 32.0).round()))
            }

            fn quantity(&self, symbol: &str, quantity: f64) -> Option<String> {
                (symbol == "EURUSD").then(|| format!("{quantity} lots"))
            }
        }
        let order = |id, symbol, side, quantity, price| {
            let request = OrderRequest::new(symbol, side, quantity, OrderType::Limit(price));
            AccountEvent::Order(Order::new(OrderId::new(id), request, OrderStatus::Working))
        };
        let fill = |id, side, price, quantity| {
            let time = chrono::Utc::now();
            AccountEvent::Execution(Execution::new(
                OrderId::new(id),
                time,
                side,
                price,
                quantity,
            ))
        };
        let mut trading = Trading::default();
        trading.set_instrument(Instrument::new("EURUSD").with_price_decimals(5));
        trading.set_formatter(Some(Arc::new(Host)));
        trading.set_service(Arc::new(Recorder::default()));
        let ctx = egui::Context::default();
        trading.poll(&ctx, &mut Toasts::new());
        let mut toasts = Toasts::new();
        let mut own = Execution::new(
            OrderId::new("9"),
            chrono::Utc::now(),
            OrderSide::Sell,
            110.5,
            2_000.0,
        );
        own.symbol = Some("ZN".into());
        for event in [
            order("1", "EURUSD", OrderSide::Buy, 1.0, 1.08765),
            fill("1", OrderSide::Buy, 1.08765, 0.1),
            fill("1", OrderSide::Buy, 1.08765, 0.2),
            fill("1", OrderSide::Buy, 1.0876, 0.7),
            order("2", "ZN", OrderSide::Sell, 2_000.0, 110.5),
            fill("2", OrderSide::Sell, 110.5, 2_000.0),
            AccountEvent::Order(Order::new(
                OrderId::new("3"),
                OrderRequest::new("ZN", OrderSide::Sell, 2_000.0, OrderType::Limit(110.5)),
                OrderStatus::Rejected("Insufficient margin".into()),
            )),
            AccountEvent::Order(Order::new(
                OrderId::new("4"),
                OrderRequest::new("EURUSD", OrderSide::Buy, 1.0, OrderType::Market),
                OrderStatus::Rejected("Market closed".into()),
            )),
            AccountEvent::Execution(own),
        ] {
            trading.apply(&ctx, event, &mut toasts);
        }
        let notices: Vec<_> = (toasts.toasts().iter())
            .map(|toast| {
                let card = toast
                    .trading
                    .as_ref()
                    .map(|card| (card.quantity.as_str(), card.price.as_str()));
                (toast.title.as_deref(), toast.message.as_str(), card)
            })
            .collect();
        assert_eq!(
            notices,
            [
                (
                    Some("Limit order placed on"),
                    "Placed: Buy 1 lots EURUSD at 1.08765",
                    Some(("1 lots", "1.08765")),
                ),
                (
                    Some("Limit order partially executed (0.1 lots of 1 lots) on"),
                    "Partially filled: Buy 0.1 lots EURUSD at 1.08765 (0.1 lots of 1 lots)",
                    Some(("0.1 lots", "1.08765")),
                ),
                (
                    Some("Limit order partially executed (0.3 lots of 1 lots) on"),
                    "Partially filled: Buy 0.2 lots EURUSD at 1.08765 (0.3 lots of 1 lots)",
                    Some(("0.2 lots", "1.08765")),
                ),
                (
                    Some("Limit order executed on"),
                    "Filled: Buy 0.7 lots EURUSD at 1.08760",
                    Some(("0.7 lots", "1.08760")),
                ),
                (
                    Some("Limit order placed on"),
                    "Placed: Sell 2,000 ZN at 110'16",
                    Some(("2,000", "110'16")),
                ),
                (
                    Some("Limit order executed on"),
                    "Filled: Sell 2,000 ZN at 110'16",
                    Some(("2,000", "110'16")),
                ),
                (
                    Some("Limit order rejected: Insufficient margin"),
                    "Order rejected: Sell 2,000 ZN limit 110'16: Insufficient margin",
                    Some(("2,000", "110'16")),
                ),
                (
                    Some("Market order rejected: Market closed"),
                    "Order rejected: Buy 1 lots EURUSD at market: Market closed",
                    Some(("1 lots", "market")),
                ),
                (None, "Filled: Sell 2,000 ZN at 110'16", None),
            ]
        );
        assert!(
            toasts.toasts()[6..8]
                .iter()
                .all(|toast| toast.kind == ToastKind::Error)
        );
    }

    /// What a broker pushes as the account's state when a stream starts, on connecting, on
    /// connecting again after a disconnect and after the stream lagged, raises no notification
    /// and asks for no sound; a fill, a working order and a rejection pushed later are each
    /// announced once. A stream that lags while the broker reconnects keeps the order ticket and
    /// the quantity calculator as they were, drops a confirmation that names a position, lists
    /// the account the broker restates and asks for the ticket's trading rules once the new
    /// stream is live; disconnecting from the broker and setting one each reset the ticket.
    #[test]
    fn a_new_stream_does_not_announce_the_accounts_state() {
        use crate::data::FeedStatus;
        use crate::data::inbox::Inbox;
        use crate::model::TimeInForce;
        use crate::services::{Broker, PaperAccount};
        let ctx = egui::Context::default();
        let paper = Arc::new(PaperAccount::new("USD", 100_000.0));
        paper.set_price("SYM", 100.0);
        let place = |side, quantity, order_type| {
            let mut request = OrderRequest::new("SYM", side, quantity, order_type);
            request.time_in_force = TimeInForce::GoodTillCancelled;
            paper.place(request, Arc::new(Inbox::default()).reply(0));
        };
        let announced = |trading: &mut Trading| {
            let mut toasts = Toasts::new();
            trading.poll(&ctx, &mut toasts);
            let messages: Vec<_> = (toasts.toasts().iter())
                .map(|toast| toast.message.clone())
                .collect();
            (messages, trading.take_execution_sound())
        };
        let mut trading = Trading::default();
        trading.display.play_execution_sound = true;
        trading.display.execution_sound = "Bell".into();
        trading.set_service(paper.clone());
        assert_eq!(announced(&mut trading), (vec![], None));
        place(OrderSide::Buy, 1.0, OrderType::Market);
        place(OrderSide::Buy, 1.0, OrderType::Limit(90.0));
        place(OrderSide::Buy, 10_000.0, OrderType::Market);
        let (messages, sound) = announced(&mut trading);
        assert_eq!(
            messages[..2],
            ["Filled: Buy 1 SYM at 100.00", "Placed: Buy 1 SYM at 90.00"]
        );
        assert!(
            messages[2]
                .starts_with("Order rejected: Buy 10,000 SYM at market: Insufficient buying power")
        );
        assert_eq!((messages.len(), sound.as_deref()), (3, Some("Bell")));

        let reset = format!("{:?}", trading.ticket);
        let fill_in = |trading: &mut Trading| {
            let request = OrderRequest::new("SYM", OrderSide::Buy, 5.0, OrderType::Market);
            trading.open_ticket(request);
        };
        fill_in(&mut trading);
        trading.disconnect();
        assert_eq!(format!("{:?}", trading.ticket), reset, "disconnected");
        fill_in(&mut trading);
        trading.set_service(paper.clone());
        assert_eq!(announced(&mut trading), (vec![], None), "connected again");
        assert_eq!(format!("{:?}", trading.ticket), reset, "a new broker");
        // While the broker reconnects, the user fills in a sell in the ticket, whose rules wait
        // for it, with the quantity calculator open, and is asked to confirm closing the position.
        let connecting = AccountEvent::Status(FeedStatus::Connecting);
        trading.apply(&ctx, connecting, &mut Toasts::new());
        let mut request = OrderRequest::new("SYM", OrderSide::Sell, 2.0, OrderType::Limit(105.0));
        request.stop_loss = Some(110.0);
        trading.open_ticket(request);
        trading.calculator = Some((egui::Pos2::ZERO, "3".into()));
        trading.pending = Some(TradingLineAction::Close(trading.positions[0].clone()));
        let ticket = format!("{:?}", trading.ticket);
        // Revaluing the open position pushes it and the account twice per price, which overflows
        // the stream between two frames; the position closes while the stream lags.
        let generation = trading.generation;
        for price in [101.0, 100.0].repeat(2_501) {
            paper.set_price("SYM", price);
        }
        place(OrderSide::Sell, 1.0, OrderType::Market);
        assert_eq!(announced(&mut trading), (vec![], None), "lagged");
        assert_ne!(trading.generation, generation);
        let working: Vec<_> = (trading.orders.iter())
            .map(|order| &order.request.order_type)
            .collect();
        assert_eq!(
            (trading.positions.len(), working),
            (0, vec![&OrderType::Limit(90.0)])
        );
        assert!(trading.pending.is_none());
        assert_eq!(format!("{:?}", trading.ticket), ticket);
        assert_eq!(trading.calculator, Some((egui::Pos2::ZERO, "3".to_owned())));
        // The broker restated no status, so the new stream is live.
        assert_eq!(
            announced(&mut trading),
            (vec![], None),
            "the ticket's rules"
        );
        assert!(trading.rules_ready && trading.rules.symbol == "SYM");

        paper.set_price("SYM", 90.0);
        assert_eq!(
            announced(&mut trading),
            (
                vec!["Filled: Buy 1 SYM at 90.00".to_owned()],
                Some("Bell".to_owned())
            )
        );
    }

    /// Resetting a connected account clears the old orders, positions and fills at the next poll,
    /// and later fills reach both the account panel and the chart's positions.
    #[test]
    fn a_reset_paper_account_restates_itself_on_the_next_poll() {
        use crate::data::inbox::Inbox;
        use crate::model::{TimeInForce, TradingRules};
        use crate::services::{Broker, PaperAccount};
        let ctx = egui::Context::default();
        let paper = Arc::new(PaperAccount::new("USD", 100_000.0));
        let mut rules = TradingRules::new("SYM");
        (rules.quantity_step, rules.min_quantity) = (0.01, 0.01);
        rules.time_in_force = vec![TimeInForce::GoodTillCancelled];
        assert!(paper.set_rules(rules));
        paper.set_price("SYM", 100.0);
        let mut trading = Trading::default();
        (trading.display.instant_orders, trading.quantity) = (true, 0.5);
        trading.set_service(paper.clone());
        trading.poll(&ctx, &mut Toasts::new());
        trading.trade("SYM".into(), OrderSide::Buy);
        trading.poll(&ctx, &mut Toasts::new());
        let held = |trading: &Trading| -> Vec<(String, f64)> {
            (trading.positions.iter())
                .map(|p| (p.symbol.clone(), p.quantity))
                .collect()
        };
        trading.trade("SYM".into(), OrderSide::Buy);
        let mut limit = OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Limit(90.0));
        limit.time_in_force = TimeInForce::GoodTillCancelled;
        paper.place(limit, Arc::new(Inbox::default()).reply(0));
        trading.poll(&ctx, &mut Toasts::new());
        assert_eq!(
            (held(&trading), trading.orders.len()),
            (vec![("SYM".into(), 0.5)], 1)
        );

        let old_id = trading.account_panel.fills.executions[0].order.clone();
        paper.reset();
        trading.poll(&ctx, &mut Toasts::new());
        assert_eq!((held(&trading), trading.orders.len()), (vec![], 0));
        assert!(trading.account_panel.fills.executions.is_empty());
        assert_eq!(trading.account.as_ref().map(|a| a.balance), Some(100_000.0));

        trading.trade("SYM".into(), OrderSide::Buy);
        trading.poll(&ctx, &mut Toasts::new());
        assert_eq!(held(&trading), vec![("SYM".into(), 0.5)]);
        assert_eq!(trading.account_panel.fills.executions.len(), 1);
        assert_ne!(trading.account_panel.fills.executions[0].order, old_id);
    }

    /// A close the user confirmed waits for the trading rules of its symbol; when the stream lags
    /// before their answer is read, the close is reported as refused, as when the broker stops
    /// being live, and never sent, so the position stays open. A regression drops the close
    /// without a word, or sends it on a stream that restated the account.
    #[test]
    fn a_lagged_stream_refuses_a_close_waiting_for_rules() {
        use crate::data::inbox::Inbox;
        use crate::model::TimeInForce;
        use crate::services::{Broker, PaperAccount};
        let ctx = egui::Context::default();
        let paper = Arc::new(PaperAccount::new("USD", 100_000.0));
        paper.set_price("SYM", 100.0);
        let mut buy = OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Market);
        buy.time_in_force = TimeInForce::GoodTillCancelled;
        paper.place(buy, Arc::new(Inbox::default()).reply(0));
        let mut trading = Trading::default();
        trading.set_service(paper.clone());
        trading.poll(&ctx, &mut Toasts::new());
        let position = trading.positions[0].clone();
        trading.execute(Some(TradingLineAction::Close(position)));
        assert_eq!(
            trading.placing.len(),
            1,
            "the close waits for the rules of SYM"
        );
        // Revaluing the position pushes it and the account per price: 10,004 pushes overflow
        // the stream before the next frame reads the rules.
        for price in [101.0, 100.0].repeat(2_501) {
            paper.set_price("SYM", price);
        }
        let mut toasts = Toasts::new();
        trading.poll(&ctx, &mut toasts);
        trading.poll(&ctx, &mut toasts);
        let messages: Vec<_> = (toasts.toasts().iter())
            .map(|toast| toast.message.as_str())
            .collect();
        assert_eq!(
            messages,
            ["Order refused: Sell 1 SYM at market: the broker is not connected"]
        );
        let held: Vec<_> = (trading.positions.iter())
            .map(|position| (position.symbol.as_str(), position.quantity))
            .collect();
        assert_eq!((held, trading.orders.len()), (vec![("SYM", 1.0)], 0));
    }

    /// Each fill's triangles sit under its own bar, also among fills on the bars beside it, with
    /// labels on or off: a run of one-lot buys on ten bars in a row, and four buys on each of two
    /// neighbouring bars, three triangles and "+1" each.
    #[test]
    fn fills_on_nearby_bars_stay_on_their_bars() {
        use crate::theme::ThemeExt;
        let rows: [(bool, Vec<(usize, usize)>); 2] = [
            (true, (90..100).map(|bar| (bar, 1)).collect()),
            (false, vec![(95, 4), (96, 4)]),
        ];
        for (labels, bars) in rows {
            let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
            chart.symbol = "SYM".into();
            let request = OrderRequest::new("SYM", OrderSide::Buy, 100.0, OrderType::Limit(95.0));
            let mut state = vec![AccountEvent::Order(Order::new(
                OrderId::new("1"),
                request,
                OrderStatus::Filled,
            ))];
            for &(bar, count) in &bars {
                let time = chart.bars(SeriesId::MAIN)[bar].time + chrono::Duration::hours(12);
                state.extend((0..count).map(|_| {
                    AccountEvent::Execution(Execution::new(
                        OrderId::new("1"),
                        time,
                        OrderSide::Buy,
                        95.0,
                        1.0,
                    ))
                }));
            }
            let mut trading = Trading::default();
            trading.display.show_execution_labels = labels;
            trading.set_service(Arc::new(Recorder {
                state,
                ..Recorder::default()
            }));
            trading.poll(&egui::Context::default(), &mut Toasts::new());
            let ctx = egui::Context::default();
            let mut tips = Vec::new();
            for frame in 0..3 {
                let input = egui::RawInput {
                    time: Some(f64::from(frame)),
                    ..Default::default()
                };
                let output = ctx.run_ui(input, |ui| {
                    chart.show(ui);
                    trading.show(ui, &mut chart);
                });
                let buy = ctx.active_theme().ui.accent;
                tips = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Path(path)
                            if path.closed && path.points.len() == 3 && path.fill == buy =>
                        {
                            Some(path.points[0])
                        }
                        _ => None,
                    })
                    .collect();
                output.drop_without_applying_deltas();
            }
            let low = chart.get_chart_mapping().price_to_y(90.0);
            // Under each bar its own stack: tips 4 points under the low, 19 points apart.
            let mut expected: Vec<_> = bars
                .iter()
                .flat_map(|&(bar, count)| {
                    let x = chart.index_to_x(bar as f64).unwrap();
                    (0..count.min(3)).map(move |i| egui::pos2(x, low + 4.0 + 19.0 * i as f32))
                })
                .collect();
            let by_place =
                |a: &egui::Pos2, b: &egui::Pos2| a.x.total_cmp(&b.x).then(a.y.total_cmp(&b.y));
            tips.sort_by(by_place);
            expected.sort_by(by_place);
            assert_eq!(tips, expected, "labels {labels}, bars {bars:?}");
        }
    }

    #[test]
    fn order_notifications_keep_roles_and_do_not_repeat_updates() {
        use crate::model::PositionBracket;
        for (kind, role, name) in [
            (OrderType::Limit(101.0), None, "Limit"),
            (OrderType::Stop(99.0), None, "Stop"),
            (
                OrderType::StopLimit {
                    stop: 101.0,
                    limit: 102.0,
                },
                None,
                "Stop limit",
            ),
            (
                OrderType::Limit(105.0),
                Some(PositionBracket::TakeProfit),
                "Take profit",
            ),
            (
                OrderType::Stop(95.0),
                Some(PositionBracket::StopLoss),
                "Stop loss",
            ),
            (
                OrderType::TrailingStop {
                    distance: crate::model::TrailDistance::Price(2.0),
                    activation: Some(105.0),
                },
                None,
                "Trailing stop",
            ),
        ] {
            let mut trading = Trading::default();
            trading
                .set_instrument(crate::model::Instrument::new("SYM").with_logo("bytes://sym.svg"));
            let mut toasts = Toasts::new();
            let mut request = OrderRequest::new("SYM", OrderSide::Sell, 10.0, kind);
            request.position_bracket = role;
            let order = Order::new(OrderId::new("one"), request, OrderStatus::Working);
            trading.apply(
                &egui::Context::default(),
                AccountEvent::Order(order.clone()),
                &mut toasts,
            );
            trading.apply(
                &egui::Context::default(),
                AccountEvent::Order(order),
                &mut toasts,
            );
            assert_eq!(toasts.toasts().len(), 1);
            let toast = &toasts.toasts()[0];
            assert_eq!(
                toast.title.as_deref(),
                Some(format!("{name} order placed on").as_str())
            );
            let details = toast.trading.as_ref().unwrap();
            assert_eq!(details.logo.as_deref(), Some("bytes://sym.svg"));
            assert_eq!(details.side, OrderSide::Sell);
            assert_eq!(details.quantity, "10");
            assert!(!details.executed);
        }
    }

    #[test]
    fn invalid_broker_numbers_do_not_replace_account_state() {
        let mut trading = Trading::default();
        let mut toasts = Toasts::new();
        let account = Account::new("USD", 1000.0, Capabilities::default());
        let order = Order::new(
            OrderId::new("7"),
            OrderRequest::new("SYM", OrderSide::Buy, 2.0, OrderType::Limit(100.0)),
            OrderStatus::Working,
        );
        let position = Position::new("SYM", 2.0, 100.0);
        for event in [
            AccountEvent::Account(account.clone()),
            AccountEvent::Order(order.clone()),
            AccountEvent::Position(position.clone()),
        ] {
            trading.apply(&egui::Context::default(), event, &mut toasts);
        }
        for bad in [f64::NAN, f64::INFINITY] {
            let mut invalid_order = order.clone();
            invalid_order.request.stop_loss = Some(bad);
            let mut invalid_level = order.clone();
            invalid_level.request.take_profit_levels =
                vec![crate::model::TakeProfitLevel::new(bad, 2.0)];
            let mut invalid_stop = order.clone();
            let mut level = crate::model::TakeProfitLevel::new(110.0, 2.0);
            level.stop_loss = Some(bad);
            invalid_stop.request.take_profit_levels = vec![level];
            for event in [
                AccountEvent::Order(invalid_stop),
                AccountEvent::Account(Account::new("USD", bad, Capabilities::default())),
                AccountEvent::Account({
                    let mut invalid = account.clone();
                    invalid.margin_buffer = Some(bad);
                    invalid
                }),
                AccountEvent::Order(invalid_order),
                AccountEvent::Order(invalid_level),
                AccountEvent::Position(Position::new("SYM", bad, 100.0)),
            ] {
                trading.apply(&egui::Context::default(), event, &mut toasts);
                assert_eq!(trading.account, Some(account.clone()));
                assert_eq!(trading.orders.as_slice(), std::slice::from_ref(&order));
                assert_eq!(
                    trading.positions.as_slice(),
                    std::slice::from_ref(&position)
                );
            }
        }
        let closed = |quantity, entry, exit, pnl| {
            AccountEvent::ClosedPosition(crate::model::ClosedPosition::new(
                "SYM",
                OrderSide::Sell,
                quantity,
                entry,
                exit,
                pnl,
                chrono::Utc::now(),
            ))
        };
        // A short of 1 from -5 to -7.5 earns 2.5; prices need not be positive.
        assert!(valid_event(&closed(1.0, -5.0, -7.5, 2.5)));
        for quantity in [0.0, -1.0, f64::NAN, f64::INFINITY] {
            assert!(!valid_event(&closed(quantity, -5.0, -7.5, 2.5)));
        }
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            for event in [
                closed(1.0, bad, -7.5, 2.5),
                closed(1.0, -5.0, bad, 2.5),
                closed(1.0, -5.0, -7.5, bad),
            ] {
                assert!(!valid_event(&event));
            }
        }
    }

    #[test]
    fn account_changes_discard_stale_confirmations_and_feedback() {
        let mut trading = Trading::default();
        let position = Position::new("SYM", 2.0, 100.0);
        let order = Order::new(
            OrderId::new("7"),
            OrderRequest::new("SYM", OrderSide::Buy, 2.0, OrderType::Limit(100.0)),
            OrderStatus::Working,
        );
        for action in [
            TradingLineAction::Close(position.clone()),
            TradingLineAction::Reverse(position.clone()),
            TradingLineAction::Cancel(order.clone()),
            TradingLineAction::Modify(order.clone(), order.request.clone()),
        ] {
            trading.load_rules("SYM");
            trading.pending = Some(action.clone());
            let mut marked = position.clone();
            marked.unrealized_pnl = Some(5.0);
            trading.apply(
                &egui::Context::default(),
                AccountEvent::Position(marked),
                &mut Toasts::new(),
            );
            assert!(
                trading.pending.is_some(),
                "a price tick preserves confirmation"
            );
            trading.apply(
                &egui::Context::default(),
                AccountEvent::Position(Position::new("SYM", 0.0, 100.0)),
                &mut Toasts::new(),
            );
            let mut filled = order.clone();
            filled.filled = 1.0;
            trading.apply(
                &egui::Context::default(),
                AccountEvent::Order(filled),
                &mut Toasts::new(),
            );
            assert!(
                trading.pending.is_none(),
                "changed exposure invalidates confirmation"
            );
            trading.pending = Some(action);
            trading.load_rules("OTHER");
            assert!(
                trading.pending.is_none(),
                "a symbol change invalidates confirmation"
            );
        }
        let trailing = OrderType::TrailingStop {
            distance: crate::model::TrailDistance::Price(2.0),
            activation: None,
        };
        let mut moved = Order::new(
            OrderId::new("8"),
            OrderRequest::new("SYM", OrderSide::Sell, 2.0, trailing),
            OrderStatus::Working,
        );
        moved.trailing_stop = Some(98.0);
        trading.load_rules("SYM");
        trading.pending = Some(TradingLineAction::Cancel(moved.clone()));
        moved.trailing_stop = Some(99.0);
        trading.apply(
            &egui::Context::default(),
            AccountEvent::Order(moved.clone()),
            &mut Toasts::new(),
        );
        assert!(
            trading.pending.is_some(),
            "a moving trailing stop preserves confirmation"
        );
        moved.filled = 1.0;
        trading.apply(
            &egui::Context::default(),
            AccountEvent::Order(moved),
            &mut Toasts::new(),
        );
        assert!(
            trading.pending.is_none(),
            "a fill of a trailing stop invalidates confirmation"
        );
        trading.sound = Some("Bell".into());
        trading.calculator = Some((egui::Pos2::ZERO, "3".into()));
        trading.set_service(Arc::new(Recorder::default()));
        assert_eq!(trading.take_execution_sound(), None);
        assert!(trading.calculator.is_none());
    }

    /// The account panel's tab marks a broker that reports that it is connecting or
    /// disconnected until it reports that it is live, also at the demo's minimized panel height,
    /// and `is_connected` is false meanwhile. A broker that never reports a status counts as
    /// connected, and a new stream starts without the old one's status. A regression keeps
    /// dropping the status, keeps reporting a dropped broker as connected, carries a status into
    /// the next stream or hides the mark in a minimized panel.
    #[test]
    fn the_account_panel_and_is_connected_follow_the_brokers_status() {
        use crate::data::FeedStatus;
        // The connection labels the account panel draws in a panel `height` points high.
        let marks = |trading: &mut Trading, height: f32| {
            let ctx = egui::Context::default();
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, height),
                )),
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| {
                trading.show_account_panel(ui, "SYM");
            });
            let marks: Vec<String> = (output.shapes.iter())
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text)
                        if ["Live", "Connecting...", "Offline"].contains(&text.galley.text()) =>
                    {
                        Some(text.galley.text().to_owned())
                    }
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            marks
        };
        let check = |trading: &mut Trading,
                     step: &str,
                     broker: bool,
                     connected: bool,
                     mark: Option<&str>| {
            assert_eq!(
                (trading.has_broker(), trading.is_connected()),
                (broker, connected),
                "{step}"
            );
            // 42 points is the demo's minimized panel, which draws only the tab strip.
            for height in [42.0, 320.0] {
                assert_eq!(
                    marks(trading, height),
                    Vec::from_iter(mark),
                    "{step} at {height}"
                );
            }
        };
        let mut trading = Trading::default();
        check(&mut trading, "no broker", false, false, None);
        trading.set_service(Arc::new(Recorder::default()));
        trading.poll(&egui::Context::default(), &mut Toasts::new());
        check(&mut trading, "no status", true, true, None);
        for (status, connected, mark) in [
            (FeedStatus::Connecting, false, Some("Connecting...")),
            (FeedStatus::Live, true, None),
            (FeedStatus::Disconnected, false, Some("Offline")),
            (FeedStatus::Live, true, None),
            (FeedStatus::Disconnected, false, Some("Offline")),
        ] {
            trading.apply(
                &egui::Context::default(),
                AccountEvent::Status(status),
                &mut Toasts::new(),
            );
            check(&mut trading, &format!("{status:?}"), true, connected, mark);
        }
        // A new stream, as after a lag, starts without the old stream's status.
        trading.listen();
        check(&mut trading, "new stream", true, true, None);
        trading.disconnect();
        check(&mut trading, "disconnected", false, false, None);
    }

    /// Rules and older history wait while the broker reports that it is not live; one that was
    /// on its way when it stopped being live, or failed meanwhile or with an error of the
    /// connection, is asked again once when it is live again, and a refused or unsupported one
    /// keeps its handling. A regression leaves the ticket waiting and the history stopped after
    /// an outage, asks a broker that is not live, asks again on every frame or status, or asks
    /// again for what the broker refused.
    #[test]
    fn rules_and_history_are_asked_again_once_the_broker_is_live() {
        use crate::data::FeedStatus;
        use crate::services::HistoryRequest;
        let ctx = egui::Context::default();
        let first = HistoryRequest::new(HistoryKind::Journal, 100);
        let report = |trading: &mut Trading, status| {
            trading.apply(
                &egui::Context::default(),
                AccountEvent::Status(status),
                &mut Toasts::new(),
            );
        };
        for (name, error, again) in [
            ("asked while offline", None, true),
            (
                "failed while offline",
                Some(FeedError::Other("not connected".into())),
                true,
            ),
            (
                "failed after reconnecting",
                Some(FeedError::Unanswered),
                true,
            ),
            (
                "network error",
                Some(FeedError::Network("reset".into())),
                true,
            ),
            ("refused", Some(FeedError::Other("refused".into())), false),
            ("unsupported", Some(FeedError::NotSupported), false),
        ] {
            let broker = Arc::new(Recorder::default());
            let mut trading = Trading::default();
            trading.set_service(broker.clone());
            trading.poll(&ctx, &mut Toasts::new());
            let ask = |trading: &mut Trading| {
                trading.load_rules("ES");
                trading.load_older(HistoryKind::Journal);
            };
            // Answers the oldest rules and history requests with `error`.
            let fail = |trading: &mut Trading, error: FeedError| {
                (broker.rules.lock().unwrap().remove(0).1).send(Err(error.clone()));
                let (asked, page) = broker.histories.lock().unwrap().remove(0);
                assert_eq!(asked, first, "{name}");
                page.send(Err(error));
                trading.poll(&ctx, &mut Toasts::new());
            };
            let counts = || {
                (
                    broker.rules.lock().unwrap().len(),
                    broker.histories.lock().unwrap().len(),
                )
            };
            match name {
                "asked while offline" => {
                    report(&mut trading, FeedStatus::Disconnected);
                    ask(&mut trading);
                    assert_eq!(counts(), (0, 0), "{name}");
                }
                "failed while offline" => {
                    ask(&mut trading);
                    report(&mut trading, FeedStatus::Disconnected);
                    fail(&mut trading, error.clone().unwrap());
                }
                "failed after reconnecting" => {
                    ask(&mut trading);
                    report(&mut trading, FeedStatus::Disconnected);
                }
                _ => {
                    ask(&mut trading);
                    fail(&mut trading, error.clone().unwrap());
                    report(&mut trading, FeedStatus::Disconnected);
                }
            }
            // The view asks every frame; nothing goes out while the broker is not live.
            let before = counts();
            trading.load_older(HistoryKind::Journal);
            assert_eq!(counts(), before, "{name}");
            report(&mut trading, FeedStatus::Live);
            if name == "failed after reconnecting" {
                fail(&mut trading, error.clone().unwrap());
            }
            for _ in 0..2 {
                trading.load_older(HistoryKind::Journal);
                report(&mut trading, FeedStatus::Live);
            }
            let rules: Vec<_> = broker.rules.lock().unwrap().drain(..).collect();
            let pages: Vec<_> = (broker.histories.lock().unwrap().drain(..))
                .map(|(request, _)| request)
                .collect();
            let symbols: Vec<_> = rules.iter().map(|(symbol, _)| symbol.as_str()).collect();
            assert_eq!(symbols, if again { vec!["ES"] } else { vec![] }, "{name}");
            assert_eq!(
                pages,
                if again { vec![first.clone()] } else { vec![] },
                "{name}"
            );
            for (_, reply) in rules {
                reply.send(Ok(TradingRules::new("ES")));
            }
            trading.poll(&ctx, &mut Toasts::new());
            assert_eq!(
                trading.rules_ready,
                name != "refused",
                "{name}: the ticket's rules"
            );
        }
    }
}
