//! Trading from the chart through the host's [`Broker`]: the broker picker, the order ticket,
//! the Buy and Sell buttons, depth ladder and the account panel. The broker holds the account;
//! this module shows what it pushes and sends the orders the user confirms.

use crate::localization as l10n;
mod account_panel;
mod broker_picker;
mod dom;
mod executions;
mod ticket;

pub use account_panel::{AccountPanel, AccountPanelAction};
pub use broker_picker::{BrokerPicker, BrokerPickerAction, BrokerTile};
pub use ticket::{Bracket, BracketMode, OrderPreset};

use std::sync::Arc;

use crate::chart::selection::SeriesId;
use crate::chart::{TradingLineAction, TradingLineOptions, render_trading_lines, to_step};
use crate::data::{FeedError, inbox::Inbox};
use crate::model::{
    Account, Capabilities, Order, OrderId, OrderRequest, OrderSide, OrderStatus, OrderType,
    Position, Quote, TradingRules,
};
use crate::services::{AccountEvent, Broker, Listener};
use crate::theme::ThemeExt;
use crate::ui_kit::modal::dialogs::ConfirmDialog;
use crate::ui_kit::notifications::{Toast, ToastKind, Toasts};
use crate::widget::Chart;
use crate::widget::settings::{TradingNotifications, TradingSettings};
use ticket::{Action, Market, OrderTicket, order_quantity};

/// The trading controls of one chart, connected to the host's [`Broker`].
///
/// Call [`poll`](Self::poll) every frame, [`show_ticket`](Self::show_ticket) before laying out
/// the chart, so the order ticket takes its panel beside it, and [`show`](Self::show) after the
/// chart. Open the order ticket with [`open_ticket`](Self::open_ticket), for example from the
/// chart menu's [`Order`](crate::ui::context_menu::ContextMenuAction::Order) entries, and route
/// the chart's Buy and Sell buttons ([`Chart::take_quote_action`]) to [`trade`](Self::trade).
/// Orders reach the broker only from the ticket's submit button, or once the user confirms them
/// in the depth ladder, or at once from Buy and Sell and the ladder when the user turned on
/// [`TradingSettings::instant_orders`].
///
/// ```no_run
/// # fn frame(ui: &mut egui::Ui, chart: &mut egui_charts::Chart,
/// #     trading: &mut egui_charts::ui::trading::Trading,
/// #     toasts: &mut egui_charts::ui_kit::notifications::Toasts) {
/// use egui_charts::model::OrderSide;
/// use egui_charts::widget::QuoteSide;
///
/// trading.poll(ui.ctx(), toasts);
/// trading.show_ticket(ui, chart);
/// chart.show(ui);
/// if let Some(side) = chart.take_quote_action() {
///     if side == QuoteSide::Ticket {
///         trading.open_ticket(egui_charts::model::OrderRequest::new(chart.symbol(),
///             OrderSide::Buy, trading.quantity, egui_charts::model::OrderType::Market));
///     } else {
///         let side = if side == QuoteSide::Buy { OrderSide::Buy } else { OrderSide::Sell };
///         trading.trade(chart.symbol().to_owned(), side);
///     }
/// }
/// trading.show(ui, chart);
/// # }
/// ```
pub struct Trading {
    service: Option<Arc<dyn Broker>>,
    inbox: Arc<Inbox>,
    listener: Option<Listener>,
    generation: u64,
    account: Option<Account>,
    orders: Vec<Order>,
    positions: Vec<Position>,
    rules: TradingRules,
    rules_key: u64,
    rules_ready: bool,
    ticket: OrderTicket,
    /// A change the user asked for on a line, waiting for confirmation.
    pending: Option<TradingLineAction>,
    /// Decimals of the chart's prices, for notifications.
    decimals: usize,
    /// The latest quote of a symbol, from [`set_quote`](Self::set_quote).
    quote: Option<(String, Quote)>,
    /// The quantity the ticket opens with, and that instant orders and the chart menu's orders
    /// use. Positive, in the symbol's trading units. The chart's legend shows it under the quote
    /// buttons' spread, where a click opens a calculator that changes it.
    pub quantity: f64,
    /// The quantity and brackets the user saved in the ticket. Persist them in the host.
    pub presets: Vec<OrderPreset>,
    /// The account panel's names and the history it lists.
    pub account_panel: AccountPanel,
    /// The chart settings' trading options: what [`show`](Self::show) draws, whether Buy and
    /// Sell trade at once, which events raise notifications and whether fills ask for a sound.
    /// Copy the chart settings' [`trading`](crate::widget::settings::ChartSettingsState::trading)
    /// options here when the user applies them.
    pub display: TradingSettings,
    /// The sound asked for by fills since the host last took it.
    sound: Option<String>,
    /// The open quantity calculator: where it sits and the text of its field.
    calculator: Option<(egui::Pos2, String)>,
}

impl Default for Trading {
    fn default() -> Self {
        Self {
            service: None,
            inbox: Arc::default(),
            listener: None,
            generation: 0,
            account: None,
            orders: Vec::new(),
            positions: Vec::new(),
            rules: TradingRules::new(""),
            rules_key: 1,
            rules_ready: false,
            ticket: OrderTicket::default(),
            pending: None,
            decimals: 2,
            quote: None,
            quantity: 1.0,
            presets: Vec::new(),
            account_panel: AccountPanel::default(),
            display: TradingSettings::default(),
            sound: None,
            calculator: None,
        }
    }
}

impl Trading {
    /// Sets the latest quote of `symbol`, such as the one `WatchlistPanel::quote` holds for the
    /// chart's symbol. While the chart shows `symbol`, the ticket enters a market buy at the
    /// quote's ask and a market sell at its bid; without them, at the last price.
    pub fn set_quote(&mut self, symbol: &str, quote: Quote) {
        self.quote = Some((symbol.to_owned(), quote));
    }

    /// Connects to `service`, dropping the previous account's state.
    pub fn set_service(&mut self, service: Arc<dyn Broker>) {
        self.listener = None;
        self.service = Some(service);
        self.inbox = Arc::default();
        self.ticket.open = false;
        self.connect();
    }

    /// Disconnects from the broker, dropping the account's state.
    pub fn disconnect(&mut self) {
        self.listener = None;
        self.service = None;
        self.inbox = Arc::default();
        self.display.instant_orders = false;
        self.ticket.open = false;
        self.connect();
    }

    /// Whether a broker is connected.
    pub fn is_connected(&self) -> bool {
        self.service.is_some()
    }

    fn connect(&mut self) {
        self.pending = None;
        self.sound = None;
        self.calculator = None;
        self.ticket = OrderTicket::default();
        self.rules = TradingRules::new("");
        self.rules_ready = false;
        self.account = None;
        self.orders.clear();
        self.positions.clear();
        self.account_panel.clear();
        // Stream keys are even; symbol rules use odd keys, and order replies use zero.
        self.generation += 2;
        let (listener, events) = Listener::new(&self.inbox, self.generation);
        self.listener = Some(listener);
        if let Some(service) = &self.service {
            service.connect(events);
        }
    }

    /// The order types and features the account accepts; none before the broker reports them.
    pub fn capabilities(&self) -> Capabilities {
        self.account
            .as_ref()
            .map(|account| account.capabilities)
            .unwrap_or_default()
    }

    /// Opens the order ticket filled from `request`, for the user to check and submit.
    pub fn open_ticket(&mut self, request: OrderRequest) {
        self.load_rules(&request.symbol);
        self.ticket.open(&request);
    }

    /// Opens the order ticket for `symbol`, or closes it when it is open, as the toolbar's Trade
    /// button does.
    pub fn toggle_ticket(&mut self, symbol: String) {
        if self.ticket.open {
            self.ticket.open = false;
        } else {
            self.open_ticket(OrderRequest::new(
                symbol,
                OrderSide::Buy,
                self.quantity,
                OrderType::Market,
            ));
        }
    }

    /// Handles the Buy or Sell button: a market order of [`quantity`](Self::quantity) at once
    /// with [`TradingSettings::instant_orders`], otherwise the ticket for one.
    pub fn trade(&mut self, symbol: String, side: OrderSide) {
        self.load_rules(&symbol);
        let mut request = OrderRequest::new(symbol, side, self.quantity, OrderType::Market);
        if self.display.instant_orders
            && self.capabilities().market
            && self.rules_ready
            && let Ok(quantity) = order_quantity(self.quantity, &self.rules)
        {
            self.ticket.open = false;
            request.quantity = quantity;
            request.time_in_force = self.rules.time_in_force[0];
            self.place(request);
        } else {
            self.open_ticket(request);
        }
    }

    fn place(&self, request: OrderRequest) {
        if let Some(service) = &self.service {
            service.place(request, self.inbox.reply(0));
        }
    }

    fn load_rules(&mut self, symbol: &str) {
        if self.rules.symbol != symbol {
            self.pending = None;
            self.rules = TradingRules::new(symbol);
            self.rules_ready = false;
            self.rules_key += 2;
            if let Some(service) = &self.service {
                service.rules(symbol, self.inbox.reply(self.rules_key));
            }
        }
    }

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

    fn apply(&mut self, event: AccountEvent, toasts: &mut Toasts) {
        let finite = |value: Option<f64>| value.is_none_or(f64::is_finite);
        let positive = |value: f64| value.is_finite() && value > 0.0;
        let valid = match &event {
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
                        OrderType::StopLimit { stop, limit } => {
                            stop.is_finite() && limit.is_finite()
                        }
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
            AccountEvent::Balance(change) => {
                change.balance.is_finite() && change.amount.is_finite()
            }
            _ => true,
        };
        if !valid {
            return;
        }
        let first_order = match &event {
            AccountEvent::Order(order) => !self.account_panel.fills.orders.contains_key(&order.id),
            _ => false,
        };
        self.account_panel.record(&event);
        match event {
            AccountEvent::Account(account) => {
                if matches!(self.pending, Some(TradingLineAction::Modify(_, _)))
                    && !account.capabilities.modify
                    || matches!(
                        self.pending,
                        Some(TradingLineAction::Close(_) | TradingLineAction::Reverse(_))
                    ) && !account.capabilities.market
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
                let mut text = format!("{} {}", ticket::side_name(fill.side), fill.quantity);
                if let Some(order) = order {
                    text += &format!(" {}", order.symbol);
                }
                text += &format!(" at {:.*}", self.decimals, fill.price);
                let filled = self
                    .account_panel
                    .fills
                    .executions
                    .iter()
                    .filter(|earlier| earlier.order == fill.order)
                    .map(|earlier| earlier.quantity)
                    .sum::<f64>();
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

    /// The sound to play for the fills since the last call, when the user turned on
    /// [`TradingSettings::play_execution_sound`]: the name the user chose, or empty for the
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

    /// What the order ticket needs from `chart` and the account this frame.
    fn market<'a>(&'a self, chart: &'a Chart) -> Market<'a> {
        let symbol = chart.symbol();
        let instrument = chart
            .instrument()
            .filter(|instrument| instrument.symbol == symbol);
        let decimals = instrument
            .and_then(crate::scales::instrument_price_precision)
            .map_or(2, usize::from);
        let tick = instrument
            .and_then(|instrument| instrument.tick_size)
            .filter(|tick| tick.is_finite() && *tick > 0.0)
            .unwrap_or(10f64.powi(-(decimals as i32)));
        let replay = chart.replay_time().is_some();
        let quote = (self.quote.as_ref())
            .filter(|(quoted, _)| quoted == symbol && !replay)
            .map(|(_, quote)| quote);
        let book = chart.order_book(SeriesId::MAIN).filter(|_| !replay);
        Market {
            last: crate::model::last_with_value(chart.bars(SeriesId::MAIN)).map(|bar| bar.close),
            bid: book
                .and_then(|book| book.best_bid().map(|level| level.price))
                .or_else(|| quote.and_then(|quote| quote.bid)),
            ask: book
                .and_then(|book| book.best_ask().map(|level| level.price))
                .or_else(|| quote.and_then(|quote| quote.ask)),
            tick,
            decimals,
            currency: self
                .account
                .as_ref()
                .map_or("", |account| account.currency.as_str()),
            balance: self.account.as_ref().map(|account| account.balance),
            capabilities: self.capabilities(),
            rules: &self.rules,
            rules_ready: self.rules_ready && self.rules.symbol == symbol,
            book,
            orders: &self.orders,
            positions: &self.positions,
            trades: &[],
            last_fill: self.account_panel.fills.price(symbol),
        }
    }

    /// Draws the order ticket, while it is open, as a panel at the right of `ui` for the chart's
    /// symbol. Call it before laying out the chart, so the chart takes the space left. Sends the
    /// order the user submits, and the orders the user places from the depth ladder; a change to a
    /// working order or the position waits for the confirmation [`show`](Self::show) asks for.
    pub fn show_ticket(&mut self, ui: &mut egui::Ui, chart: &Chart) {
        if !self.ticket.open {
            return;
        }
        let symbol = chart.symbol();
        if self.ticket.symbol != symbol {
            // The ticket follows the chart; prices of the previous symbol mean nothing here.
            self.ticket.change_symbol(symbol);
        }
        if self.account_panel.editing.is_none() {
            self.load_rules(symbol);
        }
        let mut ticket = std::mem::take(&mut self.ticket);
        let mut presets = std::mem::take(&mut self.presets);
        let mut one_click = self.display.instant_orders;
        let trades: Vec<_> = if chart.replay_time().is_some() {
            Vec::new()
        } else {
            chart.trades(SeriesId::MAIN).collect()
        };
        let mut market = self.market(chart);
        market.last = trades.last().map(|trade| trade.price).or(market.last);
        market.trades = &trades;
        let theme = ui.active_theme();
        let action = egui::Panel::right("order_ticket")
            .resizable(false)
            .exact_size(ui.available_width().min(340.0))
            .frame(
                egui::Frame::new()
                    .fill(theme.ui.panel_bg)
                    .inner_margin(egui::Margin::symmetric(16, 8)),
            )
            .show(ui, |ui| {
                ticket.show(ui, &market, &mut presets, &mut one_click)
            })
            .inner;
        // Later panels may grow to their contents, but must not cover the docked ticket.
        ui.shrink_clip_rect(ui.available_rect_before_wrap());
        (self.ticket, self.presets, self.display.instant_orders) = (ticket, presets, one_click);
        match action {
            Some(Action::Place(request)) => self.place(request),
            Some(Action::Cancel(ids)) => {
                if let Some(service) = &self.service {
                    for id in ids {
                        service.cancel(id, self.inbox.reply(0));
                    }
                }
            }
            Some(Action::Confirm(change)) => self.pending = Some(change),
            None => {}
        }
    }

    /// Draws the chart symbol's working orders, brackets, positions and execution marks,
    /// respecting [`display`](Self::display). Hovering a mark lists every fill grouped in it.
    /// Asks the user to confirm a change made on a line or in the depth ladder before sending it.
    pub fn show(&mut self, ui: &mut egui::Ui, chart: &mut Chart) {
        chart.trading_bid_ask = self.display.bid_ask_labels;
        let symbol = chart.symbol().to_owned();
        let symbol = symbol.as_str();
        let market = self.market(chart);
        let (decimals, tick, last) = (market.decimals, market.tick, market.last);
        self.decimals = decimals;
        if self.account_panel.editing.is_none() {
            self.load_rules(symbol);
        }
        let currency = self
            .account
            .as_ref()
            .map_or("", |account| account.currency.as_str());
        let capabilities = self.capabilities();

        if self.service.is_some() {
            let mapping = chart.get_chart_mapping();
            let display = &self.display;
            let options = TradingLineOptions {
                show_orders: display.show_orders,
                show_positions: display.show_positions,
                submit: false,
                modify: capabilities.modify,
                close: capabilities.market,
                reverse: display.reverse_button,
                take_profit: capabilities.limit,
                stop_loss: capabilities.stop,
                pnl: display.position_pnl.then_some(display.position_pnl_unit),
                bracket_pnl: display.bracket_pnl.then_some(display.bracket_pnl_unit),
                last_price: last,
                point_value: if self.rules.symbol == symbol {
                    self.rules.point_value
                } else {
                    1.0
                },
                currency,
                tick_size: tick,
            };
            let mut shown_orders = self.orders.clone();
            if let Some(TradingLineAction::Modify(old, request)) = &self.pending
                && let Some(order) = shown_orders.iter_mut().find(|order| order.id == old.id)
            {
                order.request = request.clone();
            }
            if let Some(TradingLineAction::Exit(_, request)) = &self.pending {
                shown_orders.push(Order::new(
                    OrderId::new("pending-exit"),
                    request.clone(),
                    OrderStatus::Working,
                ));
            }
            let lines = {
                let _theme = chart
                    .theme_override
                    .as_ref()
                    .map(|theme| crate::theme::ThemeScope::new(ui.ctx(), theme));
                // Under the lines, so that no mark covers an order's or a position's label.
                if display.show_executions {
                    let fills: Vec<_> = self
                        .account_panel
                        .fills
                        .executions
                        .iter()
                        .filter(|fill| {
                            self.account_panel
                                .fills
                                .orders
                                .get(&fill.order)
                                .is_some_and(|request| request.symbol == symbol)
                        })
                        .collect();
                    executions::show(ui, chart, &fills, display.show_execution_labels, decimals);
                }
                render_trading_lines(
                    ui,
                    &mapping,
                    shown_orders.iter().filter(|o| o.request.symbol == symbol),
                    self.positions.iter().filter(|p| p.symbol == symbol),
                    &options,
                )
            };
            chart.trading_tags = lines.tags;
            chart.trading_outlined = lines.outlined;
            if self.display.instant_orders {
                self.execute(lines.action);
            } else {
                self.pending = self.pending.take().or(lines.action);
            }
        }
        if let Some(action) = &self.pending {
            let money = |value: f64| format!("{value:.decimals$}");
            let (title, message, confirm) = describe(action, &money);
            let mut open = true;
            let result = ConfirmDialog::new("trading_confirm", title, message)
                .confirm_text(confirm)
                .cancel_text("Discard")
                .show(ui.ctx(), &mut open);
            if result.is_confirmed() {
                let action = self.pending.take();
                self.execute(action);
            } else if result.is_cancelled() || !open {
                self.pending = None;
            }
        }

        // The legend shows the order quantity under the spread; clicking it toggles the calculator.
        chart.order_quantity = self.service.is_some().then_some(self.quantity);
        let toggled = chart.quantity_clicked.take();
        if let Some(anchor) = toggled {
            self.calculator = match self.calculator {
                Some(_) => None,
                None => Some((
                    anchor.left_bottom() + egui::vec2(0.0, 4.0),
                    self.quantity.to_string(),
                )),
            };
        }
        if let Some((at, text)) = &mut self.calculator {
            let area = egui::Area::new(ui.scope_id().with("order_quantity"))
                .order(egui::Order::Foreground)
                .fixed_pos(*at)
                .show(ui.ctx(), |ui| {
                    egui::Frame::popup(ui.style()).show(ui, |ui| {
                        calculator(ui, text, &mut self.quantity, &self.rules);
                    });
                });
            let escape = ui.input(|input| input.key_pressed(egui::Key::Escape));
            if escape || toggled.is_none() && area.response.clicked_elsewhere() {
                self.calculator = None;
            }
        }

        if !self.ticket.open {
            return;
        }
        let market = self.market(chart);
        if let Ok(request) = self.ticket.request(&market)
            && request.order_type != OrderType::Market
        {
            let _theme = chart
                .theme_override
                .as_ref()
                .map(|theme| crate::theme::ThemeScope::new(ui.ctx(), theme));
            let mapping = chart.get_chart_mapping();
            let options = TradingLineOptions {
                show_orders: true,
                show_positions: false,
                submit: true,
                modify: true,
                close: false,
                reverse: false,
                take_profit: false,
                stop_loss: false,
                pnl: None,
                bracket_pnl: self
                    .display
                    .bracket_pnl
                    .then_some(self.display.bracket_pnl_unit),
                last_price: last,
                point_value: self.rules.point_value,
                currency,
                tick_size: tick,
            };
            let order = Order::new(
                OrderId::new("projected"),
                request.clone(),
                OrderStatus::Working,
            );
            let lines = ui
                .push_id("projected_order", |ui| {
                    render_trading_lines(ui, &mapping, [&order], [], &options)
                })
                .inner;
            let offset = chart.trading_tags.len();
            chart.trading_tags.extend(lines.tags);
            chart
                .trading_outlined
                .extend(lines.outlined.into_iter().map(|index| offset + index));
            match lines.action {
                Some(TradingLineAction::Cancel(_)) => self.ticket.open = false,
                Some(TradingLineAction::Modify(_, changed)) => self.ticket.open(&changed),
                _ => {}
            }
            if let Some(request) = lines.submitted {
                self.ticket.open = false;
                self.place(request);
            }
        }
    }

    /// Sends a confirmed change to the broker. Closing and reversing a position are market
    /// orders on the other side, for the position's size and for twice it.
    fn execute(&self, action: Option<TradingLineAction>) {
        let Some(service) = &self.service else {
            return;
        };
        let reverse = matches!(action, Some(TradingLineAction::Reverse(_)));
        match action {
            Some(TradingLineAction::Cancel(order)) => service.cancel(order.id, self.inbox.reply(0)),
            Some(TradingLineAction::Modify(order, request)) => {
                service.modify(order.id, request, self.inbox.reply(0));
            }
            Some(TradingLineAction::Exit(_, mut exit)) => {
                if self.rules_ready && self.rules.symbol == exit.symbol {
                    exit.time_in_force = self.rules.time_in_force[0];
                }
                service.place(exit, self.inbox.reply(0));
            }
            Some(TradingLineAction::Close(position) | TradingLineAction::Reverse(position))
                if position.quantity != 0.0 =>
            {
                let side = if position.quantity > 0.0 {
                    OrderSide::Sell
                } else {
                    OrderSide::Buy
                };
                let quantity = position.quantity.abs() * if reverse { 2.0 } else { 1.0 };
                let mut order =
                    OrderRequest::new(position.symbol, side, quantity, OrderType::Market);
                if self.rules_ready && self.rules.symbol == order.symbol {
                    order.time_in_force = self.rules.time_in_force[0];
                }
                service.place(order, self.inbox.reply(0));
            }
            _ => {}
        }
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
    let side = ticket::side_name(request.side);
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

/// The quantity calculator: a field for the quantity, minus and plus by the quantity step, preset
/// quantities, a button that clears the field and one that resets the quantity to the minimum.
/// A positive number in the field becomes the order quantity.
fn calculator(ui: &mut egui::Ui, text: &mut String, quantity: &mut f64, rules: &TradingRules) {
    use crate::icons::icons;
    use crate::theme::ThemeExt;
    let theme = ui.ctx().active_theme();
    ui.set_width(200.0);
    ui.label(egui::RichText::new(l10n::text(ui.ctx(), "Quantity")).color(theme.ui.text_secondary));
    if ui
        .add(egui::TextEdit::singleline(text).desired_width(f32::INFINITY))
        .changed()
        && let Ok(value) = text.trim().parse::<f64>()
        && value.is_finite()
        && value > 0.0
    {
        *quantity = value;
    }
    let step = rules.quantity_step;
    let icon =
        |icon: &crate::icons::Icon| icon.as_image_tinted(egui::vec2(14.0, 14.0), theme.ui.text);
    let mut set = None;
    egui::Grid::new("order_quantity_buttons")
        .num_columns(2)
        .spacing([6.0, 6.0])
        .show(ui, |ui| {
            let button = |ui: &mut egui::Ui, button: egui::Button<'_>| {
                ui.add_sized([97.0, 28.0], button).clicked()
            };
            if button(ui, egui::Button::image(icon(&icons::MINUS))) {
                set = Some(to_step(*quantity - step, step, f64::round).max(rules.min_quantity));
            }
            if button(ui, egui::Button::image(icon(&icons::PLUS))) {
                set = Some(to_step(*quantity + step, step, f64::round));
            }
            ui.end_row();
            for pair in [[1.0, 5.0], [25.0, 100.0], [500.0, 1000.0]] {
                for preset in pair {
                    if button(ui, egui::Button::new(format!("{preset}"))) {
                        set = Some(preset);
                    }
                }
                ui.end_row();
            }
            if button(ui, egui::Button::new(l10n::text(ui.ctx(), "C"))) {
                text.clear();
            }
            if button(ui, egui::Button::image(icon(&icons::ROTATE_CCW))) {
                set = Some(rules.min_quantity);
            }
            ui.end_row();
        });
    if let Some(value) = set.filter(|value| value.is_finite() && *value > 0.0) {
        *quantity = value;
        *text = value.to_string();
    }
}

/// Whether every number of `rules` is positive and finite and a time in force is offered.
fn valid(rules: &TradingRules) -> bool {
    let positive = |value: f64| value.is_finite() && value > 0.0;
    positive(rules.quantity_step)
        && positive(rules.min_quantity)
        && positive(rules.point_value)
        && rules
            .max_quantity
            .is_none_or(|max| positive(max) && max >= rules.min_quantity)
        && rules.leverage.is_none_or(positive)
        && !rules.time_in_force.is_empty()
}

/// The title, question and confirm button text of the confirmation for `action`.
fn describe(
    action: &TradingLineAction,
    price: &dyn Fn(f64) -> String,
) -> (&'static str, String, &'static str) {
    let order = |request: &OrderRequest| {
        let mut text = format!(
            "{} {} {}",
            request.side.label(),
            request.quantity,
            request.symbol
        );
        match request.order_type {
            OrderType::Limit(limit) => text += &format!(" limit {}", price(limit)),
            OrderType::Stop(stop) => text += &format!(" stop {}", price(stop)),
            OrderType::StopLimit { stop, limit } => {
                text += &format!(" stop {} limit {}", price(stop), price(limit));
            }
            _ => text += " at market",
        }
        if let Some(take_profit) = request.take_profit {
            text += &format!(", take profit {}", price(take_profit));
        }
        if let Some(stop_loss) = request.stop_loss {
            text += &format!(", stop loss {}", price(stop_loss));
        }
        text
    };
    let position = |position: &Position| {
        format!(
            "the {} position of {} {}",
            if position.quantity > 0.0 {
                "long"
            } else {
                "short"
            },
            position.quantity.abs(),
            position.symbol
        )
    };
    match action {
        TradingLineAction::Cancel(working) => (
            "Cancel order",
            format!("Cancel the order to {}?", order(&working.request)),
            "Cancel order",
        ),
        TradingLineAction::Modify(_, changed) => (
            "Modify order",
            format!("Change the order to {}?", order(changed)),
            "Modify order",
        ),
        TradingLineAction::Close(open) => (
            "Close position",
            format!("Close {} at market?", position(open)),
            "Close position",
        ),
        TradingLineAction::Reverse(open) => (
            "Reverse position",
            format!("Reverse {} at market?", position(open)),
            "Reverse position",
        ),
        TradingLineAction::Exit(open, exit) => {
            let (title, name) = match exit.order_type {
                OrderType::Limit(_) => ("Place take profit", "take profit"),
                _ => ("Place stop loss", "stop loss"),
            };
            (
                title,
                format!("Protect {} with a {name}: {}?", position(open), order(exit)),
                title,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Reply;
    use crate::model::Execution;
    use crate::services::Events;
    use std::sync::Mutex;

    /// A broker that pushes `state` on connecting and records the calls it gets.
    #[derive(Default)]
    struct Recorder {
        state: Vec<AccountEvent>,
        placed: Mutex<Vec<OrderRequest>>,
        cancelled: Mutex<Vec<OrderId>>,
        modified: Mutex<Vec<(OrderId, OrderRequest)>>,
        rules: Mutex<Vec<(String, Reply<TradingRules>)>>,
    }

    impl Broker for Recorder {
        fn connect(&self, events: Events<AccountEvent>) {
            let capabilities = Capabilities {
                market: true,
                limit: true,
                stop: true,
                modify: true,
                ..Capabilities::default()
            };
            events.push(AccountEvent::Account(Account::new(
                "USD",
                0.0,
                capabilities,
            )));
            self.state
                .iter()
                .for_each(|event| events.push(event.clone()));
        }
        fn rules(&self, symbol: &str, reply: Reply<TradingRules>) {
            self.rules.lock().unwrap().push((symbol.to_owned(), reply));
        }
        fn modify(&self, id: OrderId, order: OrderRequest, reply: Reply<()>) {
            self.modified.lock().unwrap().push((id, order));
            reply.send(Ok(()));
        }
        fn place(&self, order: OrderRequest, reply: Reply<OrderId>) {
            self.placed.lock().unwrap().push(order);
            reply.send(Ok(OrderId::new("1")));
        }
        fn cancel(&self, id: OrderId, reply: Reply<()>) {
            self.cancelled.lock().unwrap().push(id);
            reply.send(Ok(()));
        }
    }

    /// Guards the money path of the Buy and Sell buttons: without instant orders they only
    /// open the ticket and send nothing; with it they send exactly one market order of the
    /// default quantity. A regression either trades without the user's confirmation or loses
    /// the instant order.
    #[test]
    fn buy_and_sell_trade_at_once_only_with_instant_orders() {
        let broker = Arc::new(Recorder::default());
        let mut trading = Trading::default();
        trading.set_service(broker.clone());
        trading.poll(&egui::Context::default(), &mut Toasts::new());
        trading.quantity = 3.0;

        trading.trade("AAPL".into(), OrderSide::Sell);
        assert!(broker.placed.lock().unwrap().is_empty());
        assert!(trading.ticket.open);

        trading.display.instant_orders = true;
        trading.trade("AAPL".into(), OrderSide::Buy);
        assert!(broker.placed.lock().unwrap().is_empty(), "wait for rules");
        let mut rules = TradingRules::new("AAPL");
        rules.quantity_step = 2.0;
        rules.min_quantity = 2.0;
        rules.max_quantity = Some(4.0);
        rules.time_in_force = vec![crate::model::TimeInForce::GoodTillCancelled];
        broker
            .rules
            .lock()
            .unwrap()
            .pop()
            .unwrap()
            .1
            .send(Ok(rules));
        trading.poll(&egui::Context::default(), &mut Toasts::new());
        for (quantity, accepted) in [(3.0, true), (1.0, false), (6.0, false), (f64::NAN, false)] {
            trading.quantity = quantity;
            trading.trade("AAPL".into(), OrderSide::Buy);
            let placed = std::mem::take(&mut *broker.placed.lock().unwrap());
            if accepted {
                let mut expected =
                    OrderRequest::new("AAPL", OrderSide::Buy, 2.0, OrderType::Market);
                expected.time_in_force = crate::model::TimeInForce::GoodTillCancelled;
                assert_eq!(placed, [expected]);
                assert!(!trading.ticket.open);
            } else {
                assert!(placed.is_empty());
            }
        }
        trading.load_rules("ES");
        trading.load_rules("NQ");
        let mut replies = broker.rules.lock().unwrap();
        let current = replies.pop().unwrap().1;
        let stale = replies.pop().unwrap().1;
        stale.send(Err(FeedError::NotSupported));
        trading.poll(&egui::Context::default(), &mut Toasts::new());
        assert!(
            !trading.rules_ready,
            "an old answer cannot enable the new symbol"
        );
        current.send(Err(FeedError::NotSupported));
        trading.poll(&egui::Context::default(), &mut Toasts::new());
        assert!(trading.rules_ready);
    }

    #[test]
    fn ladder_changes_still_wait_for_confirmation_with_instant_orders() {
        let broker = Arc::new(Recorder::default());
        let mut trading = Trading::default();
        trading.set_service(broker.clone());
        let ctx = egui::Context::default();
        trading.load_rules("SYM");
        trading.poll(&ctx, &mut Toasts::new());
        trading.display.instant_orders = true;
        let order = Order::new(
            OrderId::new("working"),
            OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Limit(95.0)),
            OrderStatus::Working,
        );
        let mut changed = order.request.clone();
        changed.order_type = OrderType::Limit(96.0);
        let action = TradingLineAction::Modify(order, changed);
        // The ladder hands confirmed changes to this queue through Action::Confirm.
        trading.pending = Some(action.clone());
        let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
        chart.symbol = "SYM".into();
        ctx.run_ui(Default::default(), |ui| {
            chart.show(ui);
            trading.show(ui, &mut chart);
        })
        .drop_without_applying_deltas();
        assert_eq!(trading.pending, Some(action));
        assert!(broker.modified.lock().unwrap().is_empty());
    }

    /// Line gestures wait for the actual confirmation button and send one broker call.
    #[test]
    fn line_changes_wait_for_confirmation_and_send_one_call() {
        use crate::theme::{Theme, apply_to_egui};
        for theme in [Theme::light(), Theme::dark()] {
            for width in [800.0, 320.0, 260.0] {
                for instant in [false, true] {
                    for (operation, edge, handle) in [
                        ("Cancel order", false, false),
                        ("Modify order", false, false),
                        ("Modify order", false, true),
                        ("Modify order", true, false),
                        ("Close position", false, false),
                        ("Reverse position", false, false),
                        ("Place take profit", false, false),
                        ("Place stop loss", false, false),
                    ] {
                        let mut request =
                            OrderRequest::new("SYM", OrderSide::Sell, 1.0, OrderType::Limit(105.0));
                        request.take_profit = Some(102.0);
                        request.stop_loss = Some(108.0);
                        let order = Order::new(OrderId::new("7"), request, OrderStatus::Working);
                        let position = Position::new("SYM", -2.0, 100.0);
                        let broker = Arc::new(Recorder {
                            state: vec![
                                AccountEvent::Order(order.clone()),
                                AccountEvent::Position(position.clone()),
                            ],
                            ..Recorder::default()
                        });
                        let mut trading = Trading::default();
                        trading.set_service(broker.clone());
                        trading.display.instant_orders = instant;
                        let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
                        chart.symbol = "SYM".into();
                        if edge {
                            let mut instrument = crate::model::Instrument::new("SYM");
                            instrument.tick_size = Some(64.0);
                            chart.set_instrument(Some(instrument));
                        }
                        let ctx = egui::Context::default();
                        apply_to_egui(
                            &ctx,
                            &if theme == Theme::light() {
                                Theme::dark()
                            } else {
                                Theme::light()
                            },
                        );
                        chart.set_theme_override(Some(theme.clone()));
                        let frame = |trading: &mut Trading, chart: &mut Chart, events| {
                            let output = ctx.run_ui(
                                egui::RawInput {
                                    screen_rect: Some(egui::Rect::from_min_size(
                                        egui::Pos2::ZERO,
                                        egui::vec2(width, 600.0),
                                    )),
                                    events,
                                    ..Default::default()
                                },
                                |ui| {
                                    trading.poll(ui.ctx(), &mut Toasts::new());
                                    chart.show(ui);
                                    trading.show(ui, chart);
                                },
                            );
                            let texts: Vec<_> = output
                                .shapes
                                .iter()
                                .filter_map(|shape| match &shape.shape {
                                    egui::Shape::Text(text) => Some((
                                        text.galley.text().to_owned(),
                                        text.galley.rect.translate(text.pos.to_vec2()),
                                    )),
                                    _ => None,
                                })
                                .collect();
                            output.drop_without_applying_deltas();
                            texts
                        };
                        let find = |texts: Vec<(String, egui::Rect)>, wanted: &str| {
                            texts
                                .into_iter()
                                .rev()
                                .find(|(text, _)| text == wanted)
                                .map(|(_, rect)| rect)
                        };
                        let click = |pos, pressed| egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        };
                        frame(&mut trading, &mut chart, vec![]);
                        frame(&mut trading, &mut chart, vec![]);
                        assert!(
                            chart
                                .trading_tags
                                .iter()
                                .any(|(_, color)| *color == theme.chart.candle_down)
                        );
                        let mapping = chart.get_chart_mapping();
                        let y = mapping.price_to_y(if operation.ends_with("order") {
                            105.0
                        } else {
                            100.0
                        });
                        // Hovering the label shows the reverse button.
                        let texts = frame(&mut trading, &mut chart, vec![]);
                        let right = texts
                            .iter()
                            .filter(|(_, rect)| {
                                (rect.center().y - y).abs() < 2.0
                                    && rect.right() < mapping.rect.right() - 20.0
                            })
                            .map(|(_, rect)| rect.right())
                            .max_by(f32::total_cmp)
                            .unwrap();
                        let close = egui::pos2(right + 17.0, y);
                        let texts = frame(
                            &mut trading,
                            &mut chart,
                            vec![egui::Event::PointerMoved(close)],
                        );
                        let take_profit = find(texts.clone(), "TP").expect("a TP button");
                        let x = match operation {
                            "Modify order" if handle => mapping.rect.right() - 42.0,
                            "Modify order" => find(texts.clone(), "Limit").unwrap().center().x,
                            "Reverse position" => take_profit.left() - 30.2,
                            "Place take profit" => take_profit.center().x,
                            "Place stop loss" => {
                                find(texts, "SL").expect("an SL button").center().x
                            }
                            _ => close.x,
                        };
                        let start =
                            if operation == "Reverse position" && x - 11.0 < mapping.rect.left() {
                                egui::pos2(take_profit.left() - 6.0 + 11.0, y + 26.0)
                            } else {
                                egui::pos2(x, y)
                            };
                        assert!(mapping.rect.contains(start), "{operation} at {width}");
                        frame(
                            &mut trading,
                            &mut chart,
                            vec![egui::Event::PointerMoved(start)],
                        );
                        frame(&mut trading, &mut chart, vec![click(start, true)]);
                        let end = if edge {
                            egui::pos2(x, mapping.rect.top())
                        } else if operation == "Modify order" {
                            egui::pos2(x, mapping.price_to_y(103.07))
                        } else if operation == "Place take profit" {
                            egui::pos2(x, mapping.price_to_y(95.0))
                        } else if operation == "Place stop loss" {
                            egui::pos2(x, mapping.price_to_y(104.0))
                        } else {
                            start
                        };
                        frame(
                            &mut trading,
                            &mut chart,
                            vec![egui::Event::PointerMoved(end)],
                        );
                        frame(&mut trading, &mut chart, vec![click(end, false)]);
                        if !instant {
                            assert!(
                                trading.pending.is_some(),
                                "{operation} at {width}, handle={handle}, instant={instant}"
                            );
                            assert!(broker.placed.lock().unwrap().is_empty());
                            assert!(broker.cancelled.lock().unwrap().is_empty());
                            assert!(broker.modified.lock().unwrap().is_empty());
                            if operation == "Modify order" && !edge && !handle {
                                let discard =
                                    find(frame(&mut trading, &mut chart, vec![]), "Discard")
                                        .unwrap()
                                        .center();
                                frame(
                                    &mut trading,
                                    &mut chart,
                                    vec![egui::Event::PointerMoved(discard)],
                                );
                                frame(&mut trading, &mut chart, vec![click(discard, true)]);
                                frame(&mut trading, &mut chart, vec![click(discard, false)]);
                                frame(&mut trading, &mut chart, vec![]);
                                assert!(trading.pending.is_none());
                                assert!(broker.modified.lock().unwrap().is_empty());
                                assert!(
                                    chart.trading_tags.iter().any(|(price, _)| *price == 105.0),
                                    "discard restores the broker's price"
                                );
                                for events in [
                                    vec![egui::Event::PointerMoved(start)],
                                    vec![click(start, true)],
                                    vec![egui::Event::PointerMoved(end)],
                                    vec![click(end, false)],
                                ] {
                                    frame(&mut trading, &mut chart, events);
                                }
                            }
                            frame(&mut trading, &mut chart, vec![]);
                            let confirm = find(frame(&mut trading, &mut chart, vec![]), operation)
                                .expect(operation)
                                .center();
                            assert!(
                                confirm.x >= 0.0 && confirm.x <= width,
                                "confirmation stays reachable"
                            );
                            frame(
                                &mut trading,
                                &mut chart,
                                vec![egui::Event::PointerMoved(confirm)],
                            );
                            frame(&mut trading, &mut chart, vec![click(confirm, true)]);
                            frame(&mut trading, &mut chart, vec![click(confirm, false)]);
                            frame(&mut trading, &mut chart, vec![]);
                            assert!(trading.pending.is_none());
                        } else {
                            assert!(trading.pending.is_none());
                        }
                        match operation {
                            "Cancel order" => {
                                assert_eq!(*broker.cancelled.lock().unwrap(), [order.id])
                            }
                            "Modify order" => {
                                let changed = broker.modified.lock().unwrap();
                                assert_eq!(changed.len(), 1);
                                assert_eq!(changed[0].0, order.id);
                                // Prices are written at the tick's decimals, brackets included.
                                let (price, take_profit, stop_loss) = if edge {
                                    (128.0, 125.0, 131.0)
                                } else {
                                    (103.07, 100.07, 106.07)
                                };
                                assert_eq!(changed[0].1.order_type, OrderType::Limit(price));
                                assert_eq!(changed[0].1.take_profit, Some(take_profit));
                                assert_eq!(changed[0].1.stop_loss, Some(stop_loss));
                            }
                            _ => {
                                // Closing and the exits are for the position's size, reversing for
                                // twice it.
                                let (quantity, order_type) = match operation {
                                    "Reverse position" => (4.0, OrderType::Market),
                                    "Place take profit" => (2.0, OrderType::Limit(95.0)),
                                    "Place stop loss" => (2.0, OrderType::Stop(104.0)),
                                    _ => (2.0, OrderType::Market),
                                };
                                let mut expected =
                                    OrderRequest::new("SYM", OrderSide::Buy, quantity, order_type);
                                expected.position_bracket = match operation {
                                    "Place take profit" => {
                                        Some(crate::model::PositionBracket::TakeProfit)
                                    }
                                    "Place stop loss" => {
                                        Some(crate::model::PositionBracket::StopLoss)
                                    }
                                    _ => None,
                                };
                                assert_eq!(*broker.placed.lock().unwrap(), [expected]);
                            }
                        }
                    }
                }
            }
        }
    }

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

    /// The legend shows the order quantity under the spread; clicking it opens the calculator,
    /// whose presets set the quantity that Buy and Sell use, and a click elsewhere closes it.
    #[test]
    fn the_quantity_box_opens_a_calculator_that_sets_the_order_quantity() {
        let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
        chart.symbol = "SYM".into();
        chart.set_quote(Some(crate::model::Level1Quote {
            symbol: "SYM".into(),
            ts: chart.bars(SeriesId::MAIN)[99].time,
            bid: 99.0,
            ask: 101.0,
            bid_size: 1.0,
            ask_size: 1.0,
        }));
        let mut trading = Trading::default();
        trading.set_service(Arc::new(Recorder::default()));
        trading.quantity = 3.25;
        let ctx = egui::Context::default();
        let frame = |trading: &mut Trading, chart: &mut Chart, events: Vec<egui::Event>| {
            let output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    trading.poll(ui.ctx(), &mut Toasts::new());
                    chart.show(ui);
                    trading.show(ui, chart);
                },
            );
            let texts: Vec<_> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some((
                        text.galley.text().to_owned(),
                        text.galley.rect.translate(text.pos.to_vec2()).center(),
                    )),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            texts
        };
        // A click at `at` and the frame after it.
        let click = |at| {
            let button = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            [
                vec![egui::Event::PointerMoved(at)],
                vec![button(true)],
                vec![button(false)],
                vec![],
            ]
        };
        let find = |texts: &[(String, egui::Pos2)], wanted: &str| {
            texts
                .iter()
                .find(|(text, _)| text == wanted)
                .map(|(_, at)| *at)
        };
        frame(&mut trading, &mut chart, vec![]);
        let texts = frame(&mut trading, &mut chart, vec![]);
        let quantity = find(&texts, "3.25").expect("the quantity under the spread");
        assert!(find(&texts, "1000").is_none());

        let mut texts = Vec::new();
        for events in click(quantity) {
            texts = frame(&mut trading, &mut chart, events);
        }
        let preset = find(&texts, "25").expect("the calculator's presets");
        for events in click(preset) {
            texts = frame(&mut trading, &mut chart, events);
        }
        assert_eq!(trading.quantity, 25.0);
        assert!(find(&texts, "1000").is_some(), "a preset keeps it open");

        for events in click(egui::pos2(300.0, 300.0)) {
            texts = frame(&mut trading, &mut chart, events);
        }
        assert!(
            find(&texts, "1000").is_none(),
            "a click elsewhere closes it"
        );
        assert!(
            find(&texts, "25").is_some(),
            "the legend shows the new quantity"
        );
    }

    #[test]
    fn coincident_order_and_position_axis_tags_keep_their_own_style() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for (show_positions, show_orders) in
                [(true, true), (false, true), (true, false), (false, false)]
            {
                let mut exit =
                    OrderRequest::new("SYM", OrderSide::Sell, 2.0, OrderType::Limit(105.0));
                exit.position_bracket = Some(crate::model::PositionBracket::TakeProfit);
                let broker = Arc::new(Recorder {
                    state: vec![
                        AccountEvent::Order(Order::new(
                            OrderId::new("order"),
                            OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Limit(100.0)),
                            OrderStatus::Working,
                        )),
                        AccountEvent::Position(Position::new("SYM", 2.0, 100.0)),
                        AccountEvent::Order(Order::new(
                            OrderId::new("exit"),
                            exit,
                            OrderStatus::Working,
                        )),
                    ],
                    ..Recorder::default()
                });
                let mut trading = Trading::default();
                trading.set_service(broker);
                trading.display.show_positions = show_positions;
                trading.display.show_orders = show_orders;
                let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
                chart.symbol = "SYM".into();
                chart.set_theme_override(Some(theme.clone()));
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                for pass in 0..3 {
                    let output = ctx.run_ui(egui::RawInput::default(), |ui| {
                        trading.poll(ui.ctx(), &mut Toasts::new());
                        chart.show(ui);
                        trading.show(ui, &mut chart);
                    });
                    if pass == 2 {
                        let edge = chart.get_chart_mapping().rect.right();
                        let tags: Vec<_> = output
                            .shapes
                            .iter()
                            .filter_map(|shape| match &shape.shape {
                                egui::Shape::Rect(rect) if rect.rect.center().x > edge => {
                                    Some(rect)
                                }
                                _ => None,
                            })
                            .collect();
                        assert_eq!(
                            tags.iter().any(|rect| rect.fill == theme.ui.accent),
                            show_positions,
                            "position tag stays filled"
                        );
                        assert_eq!(
                            tags.iter().any(|rect| rect.stroke.color == theme.ui.accent
                                && rect.stroke.width == 1.0),
                            show_orders,
                            "order tag stays outlined"
                        );
                    }
                    if pass == 2 {
                        let has_text = |wanted: &str| {
                            output.shapes.iter().any(|shape| {
                        matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == wanted)
                    })
                        };
                        assert_eq!(
                            has_text("+10.00 USD"),
                            show_orders,
                            "brackets retain the hidden position's entry price"
                        );
                        assert!(
                            !has_text("TP"),
                            "hiding orders cannot offer a duplicate exit"
                        );
                        assert_eq!(has_text("SL"), show_positions);
                    }
                    output.drop_without_applying_deltas();
                }
            }
        }
    }

    #[test]
    fn projected_orders_only_send_from_their_submit_button() {
        for (width, quantity) in [(1000.0, 20.0), (320.0, 1_000_000.0)] {
            for side in [OrderSide::Buy, OrderSide::Sell] {
                for submit in [false, true] {
                    let broker = Arc::new(Recorder::default());
                    let mut trading = Trading::default();
                    trading.set_service(broker.clone());
                    trading.poll(&egui::Context::default(), &mut Toasts::new());
                    trading.display.instant_orders = true;
                    let request = OrderRequest::new("SYM", side, quantity, OrderType::Limit(94.0));
                    trading.open_ticket(request.clone());
                    trading.rules_ready = true;
                    let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
                    chart.symbol = "SYM".into();
                    let ctx = egui::Context::default();
                    let frame = |trading: &mut Trading, chart: &mut Chart, events| {
                        let output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(width, 800.0),
                                )),
                                events,
                                ..Default::default()
                            },
                            |ui| {
                                chart.show(ui);
                                trading.show(ui, chart);
                            },
                        );
                        let y = chart.get_chart_mapping().price_to_y(94.0);
                        let at = output.shapes.iter().find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text)
                                if text.galley.text() == ticket::side_name(side)
                                    && (y - 12.0..y + 50.0).contains(&text.pos.y) =>
                            {
                                Some(text.pos + text.galley.size() / 2.0)
                            }
                            _ => None,
                        });
                        output.drop_without_applying_deltas();
                        at
                    };
                    frame(&mut trading, &mut chart, vec![]);
                    let button =
                        frame(&mut trading, &mut chart, vec![]).expect("projected submit button");
                    assert!(
                        chart.get_chart_mapping().rect.contains(button),
                        "submit remains inside the plot at {width}"
                    );
                    assert!(broker.placed.lock().unwrap().is_empty());
                    let map = chart.get_chart_mapping();
                    let at = if submit {
                        button
                    } else {
                        egui::pos2(map.rect.right() - 95.0, map.price_to_y(94.0))
                    };
                    frame(
                        &mut trading,
                        &mut chart,
                        vec![egui::Event::PointerMoved(at)],
                    );
                    for pressed in [true, false] {
                        frame(
                            &mut trading,
                            &mut chart,
                            vec![egui::Event::PointerButton {
                                pos: at,
                                button: egui::PointerButton::Primary,
                                pressed,
                                modifiers: Default::default(),
                            }],
                        );
                    }
                    frame(&mut trading, &mut chart, vec![]);
                    assert!(!trading.ticket.open);
                    assert_eq!(
                        *broker.placed.lock().unwrap(),
                        if submit { vec![request] } else { vec![] }
                    );
                    assert!(
                        broker.cancelled.lock().unwrap().is_empty(),
                        "a projected cancel never reaches the broker"
                    );
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
    fn chart_frames_keep_the_edited_orders_symbol_rules() {
        let broker = Arc::new(Recorder::default());
        let mut trading = Trading::default();
        trading.set_service(broker.clone());
        let order = Order::new(
            OrderId::new("1"),
            OrderRequest::new("OTHER", OrderSide::Buy, 2.0, OrderType::Limit(100.0)),
            OrderStatus::Working,
        );
        trading.account_panel.editing = Some((order.clone(), order.request));
        trading.load_rules("OTHER");
        let (_, reply) = broker.rules.lock().unwrap().pop().unwrap();
        let mut rules = TradingRules::new("OTHER");
        rules.quantity_step = 2.0;
        reply.send(Ok(rules));
        let mut chart = crate::ChartBuilder::new().with_symbol("MAIN").build();
        let ctx = egui::Context::default();
        trading.poll(&ctx, &mut Toasts::new());
        egui::__run_test_ui(|ui| trading.show(ui, &mut chart));
        assert_eq!(trading.rules.symbol, "OTHER");
        assert!(trading.rules_ready);
        assert!(broker.rules.lock().unwrap().is_empty());
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

    #[test]
    fn docked_tickets_keep_later_panels_out_of_their_bounds() {
        for width in [320.0, 480.0, 800.0] {
            let ctx = egui::Context::default();
            let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
            chart.symbol = "SYM".into();
            let mut trading = Trading::default();
            trading.open_ticket(OrderRequest::new(
                "SYM",
                OrderSide::Buy,
                1.0,
                OrderType::Market,
            ));
            let mut remaining = egui::Rect::NOTHING;
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(width, 360.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    trading.show_ticket(ui, &chart);
                    remaining = ui.clip_rect();
                },
            );
            let symbol = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "SYM" => Some(text.pos.x),
                _ => None,
            });
            output.drop_without_applying_deltas();
            assert!(
                remaining.right() <= symbol.expect("the ticket's symbol"),
                "later panels stay beside the ticket at {width}"
            );
        }
    }
}
