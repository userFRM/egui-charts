//! The broker's pushes and answers: applying them to the controller, and the notifications and
//! execution sound they raise.

use super::widgets::side_name;
use super::{Trading, valid};
use crate::chart::TradingLineAction;
use crate::data::FeedError;
use crate::model::{OrderId, OrderPreview, OrderRequest, OrderStatus, OrderType, TradingRules};
use crate::services::{AccountEvent, HistoryKind, HistoryPage};
use crate::ui_kit::notifications::{Toast, ToastKind, Toasts};
use crate::widget::settings::TradingNotifications;

impl Trading {
    /// Applies the broker's pushes and answers, reconnects a lagged stream, and reports refused
    /// orders and fills in `toasts`. Call it every frame, even while the chart is hidden.
    pub fn poll(&mut self, ctx: &egui::Context, toasts: &mut Toasts) {
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        if self.listener.as_ref().is_some_and(|l| !l.is_active()) {
            self.connect();
        }
        for (key, answer) in self.inbox.drain().answers {
            if key == self.generation {
                if let Ok(event) = answer.downcast::<AccountEvent>() {
                    self.apply(*event, toasts);
                }
                continue;
            }
            if let Some(index) = self.placing.iter().position(|(id, _, _)| *id == key) {
                let (_, _, mut order) = self.placing.remove(index);
                if let Ok(answer) = answer.downcast::<Result<TradingRules, FeedError>>() {
                    match *answer {
                        Ok(rules) if rules.symbol == order.symbol && valid(&rules) => {
                            order.time_in_force = rules.time_in_force[0];
                            self.place(order);
                        }
                        Err(FeedError::NotSupported) => self.place(order),
                        Err(error) => self.notify(
                            toasts,
                            Toast::error(format!("Trading rules unavailable: {error}")),
                        ),
                        Ok(_) => self.notify(
                            toasts,
                            Toast::error("The broker returned invalid trading rules"),
                        ),
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
                                Toast::error(format!("Order preview unavailable: {error}")),
                            );
                        }
                    }
                }
                continue;
            }
            if let Some(kind) = self.account_panel.loading(key) {
                if let Ok(answer) = answer.downcast::<Result<HistoryPage, FeedError>>() {
                    self.history_page(kind, *answer, toasts);
                }
                continue;
            }
            if key == self.rules_key {
                if let Ok(answer) = answer.downcast::<Result<TradingRules, FeedError>>() {
                    match *answer {
                        Ok(rules) if rules.symbol == self.rules.symbol && valid(&rules) => {
                            self.rules = rules;
                            self.rules_ready = true;
                        }
                        Err(FeedError::NotSupported) => self.rules_ready = true,
                        Err(error) => toasts.error(format!("Trading rules unavailable: {error}")),
                        Ok(_) => toasts.error("The broker returned invalid trading rules"),
                    }
                }
                continue;
            }
            if key != 0 {
                continue;
            }
            let error = match answer.downcast::<Result<OrderId, FeedError>>() {
                Ok(placed) => placed.err(),
                Err(answer) => answer
                    .downcast::<Result<(), FeedError>>()
                    .ok()
                    .and_then(|done| done.err()),
            };
            if let Some(error) = error {
                self.notify(toasts, Toast::error(format!("Order refused: {error}")));
            }
        }
    }

    pub(super) fn apply(&mut self, event: AccountEvent, toasts: &mut Toasts) {
        if !valid_event(&event) {
            return;
        }
        let first_order = match &event {
            AccountEvent::Order(order) => !self.account_panel.fills.orders.contains_key(&order.id),
            _ => false,
        };
        self.account_panel.record(&event);
        match event {
            AccountEvent::Account(account) => {
                self.placing
                    .retain(|(_, _, order)| account.capabilities.accepts(&order.order_type));
                if !account.capabilities.preview {
                    self.preview = None;
                }
                if matches!(self.pending, Some(TradingLineAction::Modify(_, _)))
                    && !account.capabilities.modify
                    || matches!(self.pending, Some(TradingLineAction::Close(_)))
                        && !account.capabilities.close
                    || matches!(self.pending, Some(TradingLineAction::Reverse(_)))
                        && !account.capabilities.reverse
                    || matches!(&self.pending,
                        Some(TradingLineAction::Exit(_, exit))
                        if !account.capabilities.accepts(&exit.order_type))
                {
                    self.pending = None;
                }
                self.account = Some(account);
            }
            AccountEvent::Order(order) => {
                if matches!(&self.pending,
                    Some(TradingLineAction::Cancel(old) | TradingLineAction::Modify(old, _))
                    if old.id == order.id && old != &order)
                {
                    self.pending = None;
                }
                self.orders.retain(|working| working.id != order.id);
                if first_order
                    && order.status == OrderStatus::Working
                    && order.request.order_type != OrderType::Market
                {
                    let price = match order.request.order_type {
                        OrderType::Limit(price) | OrderType::Stop(price) => price,
                        OrderType::StopLimit { stop, .. } => stop,
                        _ => unreachable!(),
                    };
                    self.notify(
                        toasts,
                        order_toast(
                            &order.request,
                            price,
                            order.request.quantity,
                            self.decimals,
                            false,
                        ),
                    );
                }
                match &order.status {
                    OrderStatus::Working => self.orders.push(order),
                    OrderStatus::Rejected(reason) => {
                        let toast = Toast::error(format!("Order rejected: {reason}"));
                        self.notify(toasts, toast);
                    }
                    _ => {}
                }
            }
            AccountEvent::Position(position) => {
                self.placing.retain(|(_, old, _)| {
                    old.symbol != position.symbol
                        || (old.quantity == position.quantity
                            && old.average_price == position.average_price)
                });
                if matches!(&self.pending,
                    Some(TradingLineAction::Close(old) | TradingLineAction::Reverse(old) | TradingLineAction::Exit(old, _))
                    if old.symbol == position.symbol
                        && (old.quantity != position.quantity || old.average_price != position.average_price))
                {
                    self.pending = None;
                }
                self.positions.retain(|open| open.symbol != position.symbol);
                if position.quantity != 0.0 {
                    self.positions.push(position);
                }
            }
            AccountEvent::Execution(fill) => {
                let order = self.account_panel.fills.orders.get(&fill.order);
                let mut text = format!("{} {}", side_name(fill.side), fill.quantity);
                if let Some(order) = order {
                    text += &format!(" {}", order.symbol);
                }
                text += &format!(" at {:.*}", self.decimals, fill.price);
                let filled = self.account_panel.fills.filled_quantity(&fill.order);
                // Fills that add up to the order within rounding complete it.
                let mut toast = match order {
                    Some(order) => {
                        let mut executed = order.clone();
                        executed.side = fill.side;
                        order_toast(&executed, fill.price, fill.quantity, self.decimals, true)
                    }
                    None => Toast::success(format!("Filled: {text}")),
                };
                if let Some(order) = order
                    && filled < order.quantity * (1.0 - 1e-9)
                {
                    toast.kind = ToastKind::Info;
                    toast.title = Some(format!(
                        "{} order partially executed ({filled} of {}) on",
                        order_name(order),
                        order.quantity
                    ));
                    toast.message =
                        format!("Partially filled: {text} ({filled} of {})", order.quantity);
                }
                self.notify(toasts, toast);
                if self.display.play_execution_sound {
                    self.sound = Some(self.display.execution_sound.clone());
                }
            }
            _ => {}
        }
    }

    /// Lists a page of older history the broker answered for `kind`, or stops paging there.
    fn history_page(
        &mut self,
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
                self.account_panel.end(kind);
                if error != FeedError::NotSupported {
                    toasts.error(format!("Account history unavailable: {error}"));
                }
            }
        }
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
                && finite(request.stop_loss)
                && match request.order_type {
                    OrderType::Market => true,
                    OrderType::Limit(price) | OrderType::Stop(price) => price.is_finite(),
                    OrderType::StopLimit { stop, limit } => stop.is_finite() && limit.is_finite(),
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
        AccountEvent::Balance(change) => change.balance.is_finite() && change.amount.is_finite(),
        _ => true,
    }
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
        },
    }
}

fn order_toast(
    request: &OrderRequest,
    price: f64,
    quantity: f64,
    decimals: usize,
    executed: bool,
) -> Toast {
    let side = side_name(request.side);
    let mut toast = Toast::new(
        if executed {
            ToastKind::Success
        } else {
            ToastKind::Info
        },
        format!(
            "{}: {side} {quantity} {} at {price:.decimals$}",
            if executed { "Filled" } else { "Placed" },
            request.symbol
        ),
    )
    .with_title(format!(
        "{} order {} on",
        order_name(request),
        if executed { "executed" } else { "placed" }
    ));
    toast.trading = Some(crate::ui_kit::notifications::TradingToast {
        symbol: request.symbol.clone(),
        side: request.side,
        quantity,
        price: format!("{price:.decimals$}"),
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

    /// The fills of one side in one bar are one mark: an arrow just under the bar's low for buys,
    /// with a chevron per fill, and just over its high for sells; a fill of another symbol is not
    /// drawn. Labels, off by default, give the total quantity at the quantity-weighted average
    /// price; hovering a mark lists its fills; the position's label is drawn over the marks. A
    /// partial fill, the fill that completes an order and a rejection each raise a notification,
    /// the rejection with the broker's reason. The display options hide the fills, the orders and
    /// the positions.
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
            "Order rejected: Insufficient margin",
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
            trading.set_service(broker.clone());
            let mut toasts = Toasts::new();
            trading.poll(&egui::Context::default(), &mut toasts);
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
        let labelled = TradingSettings {
            show_execution_labels: true,
            ..TradingSettings::default()
        };
        let mut clock = 0.0;
        for (display, drawn, labels, neighbouring, inverted) in [
            (TradingSettings::default(), true, false, false, false),
            (labelled.clone(), true, true, false, false),
            (labelled.clone(), true, true, true, false),
            (labelled, true, true, false, true),
            (hidden, false, false, false, false),
        ] {
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
                let chevrons: Vec<_> = output
                    .shapes
                    .iter()
                    .enumerate()
                    .filter_map(|(at, shape)| match &shape.shape {
                        egui::Shape::Path(path)
                            if path.points.len() == 3
                                && (path.stroke.color == egui::epaint::ColorMode::Solid(buy)
                                    || path.stroke.color
                                        == egui::epaint::ColorMode::Solid(sell)) =>
                        {
                            Some((at, path.points[1]))
                        }
                        _ => None,
                    })
                    .collect();
                tips = chevrons.iter().map(|(_, tip)| *tip).collect();
                texts = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text().contains(" @ ") => {
                            Some(text.galley.text().to_owned())
                        }
                        _ => None,
                    })
                    .collect();
                // 1_700_000_000 is 2023-11-14 22:13:20 UTC; 95 days and 12 hours later.
                let tooltip = format!(
                    "Buy 1 at 95.00, {} 10:13:20\nBuy 2 at 98.00, Sun 18 Feb '24 10:13:20",
                    if neighbouring {
                        "Sat 17 Feb '24"
                    } else {
                        "Sun 18 Feb '24"
                    }
                );
                hovered = output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == tooltip)
                });
                // The last shape filled in the buy colour is a quantity box of the lines; the
                // price axis tags come before the marks.
                let label = output.shapes.iter().rposition(
                    |shape| matches!(&shape.shape, egui::Shape::Rect(rect) if rect.fill == buy),
                );
                covered = chevrons
                    .last()
                    .zip(label)
                    .is_some_and(|((mark, _), label)| *mark > label);
                output.drop_without_applying_deltas();
            }
            let x = chart.index_to_x(95.0).unwrap();
            let mapping = chart.get_chart_mapping();
            let (a, b) = (mapping.price_to_y(90.0), mapping.price_to_y(110.0));
            let (low, high) = (a.max(b), a.min(b));
            // The two buys stack their chevrons 4 points apart from 4 points under the low; the
            // sell hangs 4 points over the high.
            let expected = [
                egui::pos2(x, low + 4.0),
                egui::pos2(x, low + 8.0),
                egui::pos2(x, high - 4.0),
            ];
            assert_eq!(tips, if drawn { &expected[..] } else { &[] });
            let labels_expected: &[&str] = if labels {
                &["3 @ 97.00", "3 @ 95.00"]
            } else {
                &[]
            };
            assert_eq!(texts, labels_expected);
            assert_eq!(hovered, drawn, "hovering a mark lists its fills");
            assert!(!covered, "the position's label is drawn over the marks");
            assert_eq!(chart.trading_tags.is_empty(), !drawn);
        }

        // A finer fill stays on the only revealed candle, with an exclusive replay cutoff.
        let time = chrono::DateTime::from_timestamp(1_704_067_200, 0).unwrap();
        trading.display.show_executions = true;
        trading.display.show_execution_labels = true;
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
                                if text.galley.text() == "3 @ 97.00")
                        });
                        output.drop_without_applying_deltas();
                        chart.fit_content();
                    }
                    assert_eq!(marked, expected, "fill at {seconds} seconds, width {width}");
                }
            }
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
        ] {
            let mut trading = Trading::default();
            let mut toasts = Toasts::new();
            let mut request = OrderRequest::new("SYM", OrderSide::Sell, 10.0, kind);
            request.position_bracket = role;
            let order = Order::new(OrderId::new("one"), request, OrderStatus::Working);
            trading.apply(AccountEvent::Order(order.clone()), &mut toasts);
            trading.apply(AccountEvent::Order(order), &mut toasts);
            assert_eq!(toasts.toasts().len(), 1);
            let toast = &toasts.toasts()[0];
            assert_eq!(
                toast.title.as_deref(),
                Some(format!("{name} order placed on").as_str())
            );
            let details = toast.trading.as_ref().unwrap();
            assert_eq!(details.side, OrderSide::Sell);
            assert_eq!(details.quantity, 10.0);
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
            trading.apply(event, &mut toasts);
        }
        for bad in [f64::NAN, f64::INFINITY] {
            let mut invalid_order = order.clone();
            invalid_order.request.stop_loss = Some(bad);
            for event in [
                AccountEvent::Account(Account::new("USD", bad, Capabilities::default())),
                AccountEvent::Order(invalid_order),
                AccountEvent::Position(Position::new("SYM", bad, 100.0)),
            ] {
                trading.apply(event, &mut toasts);
                assert_eq!(trading.account, Some(account.clone()));
                assert_eq!(trading.orders.as_slice(), std::slice::from_ref(&order));
                assert_eq!(
                    trading.positions.as_slice(),
                    std::slice::from_ref(&position)
                );
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
            trading.apply(AccountEvent::Position(marked), &mut Toasts::new());
            assert!(
                trading.pending.is_some(),
                "a price tick preserves confirmation"
            );
            trading.apply(
                AccountEvent::Position(Position::new("SYM", 0.0, 100.0)),
                &mut Toasts::new(),
            );
            let mut filled = order.clone();
            filled.filled = 1.0;
            trading.apply(AccountEvent::Order(filled), &mut Toasts::new());
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
        trading.sound = Some("Bell".into());
        trading.calculator = Some((egui::Pos2::ZERO, "3".into()));
        trading.set_service(Arc::new(Recorder::default()));
        assert_eq!(trading.take_execution_sound(), None);
        assert!(trading.calculator.is_none());
    }
}
