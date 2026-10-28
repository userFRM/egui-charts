//! The order ticket: a panel beside the chart with an Order tab, one order sized by the symbol's
//! trading rules with optional exits, and a DOM tab, the depth ladder.

use crate::localization as l10n;
use egui::{
    Align, Align2, Color32, FontId, Layout, Rect, Response, RichText, Role, Sense, Stroke,
    StrokeKind, Ui, UiBuilder, WidgetInfo, pos2, vec2,
};
use serde::{Deserialize, Serialize};

use super::account_panel::grouped;
use super::dom::Ladder;
use crate::chart::{TradingLineAction, decimals, to_step};
use crate::icons::icons;
use crate::model::{
    Capabilities, Order, OrderBook, OrderId, OrderPreview, OrderRequest, OrderSide, OrderType,
    Position, Tick, TimeInForce, TradingRules,
};
use crate::theme::{Theme, ThemeExt};
use crate::ui_kit::buttons::IconButton;
use crate::ui_kit::modal::dialogs::ConfirmDialog;

/// How a take-profit or stop-loss distance is entered.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BracketMode {
    /// The exit price itself.
    #[default]
    Price,
    /// Ticks of the symbol's tick size away from the entry.
    Ticks,
    /// Percent of the entry price away from the entry.
    Percent,
    /// The profit or loss at the exit in the account's currency.
    Money,
}

impl BracketMode {
    const ALL: [Self; 4] = [Self::Price, Self::Ticks, Self::Percent, Self::Money];

    fn name(self, currency: &str) -> &str {
        match self {
            Self::Price => "price",
            Self::Ticks => "ticks",
            Self::Percent => "%",
            Self::Money if currency.is_empty() => "money",
            Self::Money => currency,
        }
    }
}

/// A take-profit or stop-loss as entered in the ticket.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bracket {
    /// How [`value`](Self::value) is read.
    pub mode: BracketMode,
    /// A price, a number of ticks, a percentage or an amount of money.
    pub value: f64,
}

impl Bracket {
    /// The direction from the entry to the exit: up for a take profit (`profit`) of a buy and a
    /// stop loss of a sell, down for the other two.
    fn sign(side: OrderSide, profit: bool) -> f64 {
        if (side == OrderSide::Buy) == profit {
            1.0
        } else {
            -1.0
        }
    }

    /// The exit price for an order on `side` entering at `entry`, where `per_point` is the profit
    /// or loss of the order's quantity per price unit: a take profit (`profit`) lies above the
    /// entry of a buy and below that of a sell, a stop loss the other way.
    pub(crate) fn price(
        &self,
        side: OrderSide,
        entry: f64,
        tick: f64,
        per_point: f64,
        profit: bool,
    ) -> f64 {
        let sign = Self::sign(side, profit);
        match self.mode {
            BracketMode::Price => self.value,
            BracketMode::Ticks => entry + sign * self.value * tick,
            BracketMode::Percent => entry * (1.0 + sign * self.value / 100.0),
            BracketMode::Money => entry + sign * self.value / per_point,
        }
    }

    /// The bracket in `mode` whose [`price`](Self::price) is `price`.
    fn at(
        mode: BracketMode,
        price: f64,
        side: OrderSide,
        entry: f64,
        tick: f64,
        per_point: f64,
        profit: bool,
    ) -> Self {
        let distance = Self::sign(side, profit) * (price - entry);
        let value = match mode {
            BracketMode::Price => price,
            BracketMode::Ticks => (distance / tick).round(),
            BracketMode::Percent => distance / entry * 100.0,
            BracketMode::Money => distance * per_point,
        };
        Self { mode, value }
    }
}

/// A named quantity and brackets the ticket applies in one step. Hosts persist
/// [`Trading::presets`](super::Trading::presets) themselves.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OrderPreset {
    /// Shown in the ticket's preset list.
    pub name: String,
    /// In the symbol's trading units.
    pub quantity: f64,
    /// The take profit, if the preset sets one.
    pub take_profit: Option<Bracket>,
    /// The stop loss, if the preset sets one.
    pub stop_loss: Option<Bracket>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Market,
    Limit,
    Stop,
    StopLimit,
}

impl Kind {
    const ALL: [(Self, &'static str); 4] = [
        (Self::Market, "Market"),
        (Self::Limit, "Limit"),
        (Self::Stop, "Stop"),
        (Self::StopLimit, "Stop limit"),
    ];

    fn accepted(self, capabilities: &Capabilities) -> bool {
        match self {
            Self::Market => capabilities.market,
            Self::Limit => capabilities.limit,
            Self::Stop => capabilities.stop,
            Self::StopLimit => capabilities.stop_limit,
        }
    }
}

/// How the quantity is entered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Sizing {
    /// The quantity itself.
    Units,
    /// The order's value in the account's currency.
    Value,
    /// The loss at the stop loss, in percent of the account's balance.
    Risk,
}

/// The ticket's two tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Tab {
    Order,
    Dom,
}

/// What the ticket needs from the chart and the account for one frame.
pub(crate) struct Market<'a> {
    /// The last price, the entry of a market order without a quote.
    pub last: Option<f64>,
    /// The best bid and ask, the entries of a market sell and buy.
    pub bid: Option<f64>,
    pub ask: Option<f64>,
    /// The symbol's tick size.
    pub tick: f64,
    /// Decimals of prices.
    pub decimals: usize,
    /// The account's currency.
    pub currency: &'a str,
    /// The account's balance: risk sizing takes a percentage of it,
    /// and an order's margin is set against it.
    pub balance: Option<f64>,
    pub capabilities: Capabilities,
    pub rules: &'a TradingRules,
    pub rules_ready: bool,
    /// The order book of the chart's symbol, for the depth ladder.
    pub book: Option<&'a OrderBook>,
    /// The account's working orders and open positions, of every symbol.
    pub orders: &'a [Order],
    pub positions: &'a [Position],
    /// The retained feed trades, oldest first.
    pub trades: &'a [&'a Tick],
    /// The latest account fill for this symbol.
    pub last_fill: Option<f64>,
}

/// What the user asked the ticket for.
#[derive(Debug, PartialEq)]
pub(crate) enum Action {
    /// Send this order now.
    Place(OrderRequest),
    /// Ask the broker to estimate this order without placing it.
    Preview(OrderRequest),
    /// Cancel these working orders now.
    Cancel(Vec<OrderId>),
    /// Ask the user to confirm this change before sending it.
    Confirm(TradingLineAction),
}

/// The state of the order ticket panel.
pub(crate) struct OrderTicket {
    pub open: bool,
    pub symbol: String,
    side: OrderSide,
    kind: Kind,
    /// The limit or stop price, and the stop price of a stop-limit order.
    price: f64,
    /// The limit price of a stop-limit order.
    limit: f64,
    /// The price field takes ticks from the ask or bid instead of the price.
    price_in_ticks: bool,
    sizing: Sizing,
    quantity: f64,
    /// The value or risk entered for [`Sizing::Value`] and [`Sizing::Risk`].
    amount: f64,
    take_profit: (bool, Bracket),
    stop_loss: (bool, Bracket),
    time_in_force: TimeInForce,
    preset_name: String,
    tab: Tab,
    exits_open: bool,
    ladder: Ladder,
    /// An order from the ladder, waiting for the user to confirm it.
    confirm: Option<OrderRequest>,
    cancel: Option<Vec<OrderId>>,
    /// Why the ladder refused its last order, shown on the DOM tab.
    dom_error: Option<String>,
}

impl Default for OrderTicket {
    fn default() -> Self {
        let bracket = Bracket {
            mode: BracketMode::Price,
            value: 0.0,
        };
        Self {
            open: false,
            symbol: String::new(),
            side: OrderSide::Buy,
            kind: Kind::Market,
            price: 0.0,
            limit: 0.0,
            price_in_ticks: false,
            sizing: Sizing::Units,
            quantity: 1.0,
            amount: 1_000.0,
            take_profit: (false, bracket),
            stop_loss: (false, bracket),
            time_in_force: TimeInForce::Day,
            preset_name: String::new(),
            tab: Tab::Order,
            exits_open: true,
            ladder: Ladder::default(),
            confirm: None,
            cancel: None,
            dom_error: None,
        }
    }
}

impl OrderTicket {
    /// Fills the ticket from `request` and opens it on its Order tab.
    pub fn open(&mut self, request: &OrderRequest) {
        self.confirm = None;
        self.cancel = None;
        if self.symbol != request.symbol {
            self.ladder = Ladder::default();
            self.take_profit = (
                false,
                Bracket {
                    mode: BracketMode::Price,
                    value: 0.0,
                },
            );
            self.stop_loss = self.take_profit;
        }
        self.open = true;
        self.tab = Tab::Order;
        self.symbol.clone_from(&request.symbol);
        self.side = request.side;
        self.sizing = Sizing::Units;
        self.quantity = request.quantity;
        (self.kind, self.price, self.limit) = match request.order_type {
            OrderType::Limit(price) => (Kind::Limit, price, price),
            OrderType::Stop(price) => (Kind::Stop, price, price),
            OrderType::StopLimit { stop, limit } => (Kind::StopLimit, stop, limit),
            _ => (Kind::Market, 0.0, 0.0),
        };
        self.time_in_force = request.time_in_force;
        for (target, value) in [
            (&mut self.take_profit, request.take_profit),
            (&mut self.stop_loss, request.stop_loss),
        ] {
            target.0 = value.is_some();
            if let Some(value) = value {
                target.1 = Bracket {
                    mode: BracketMode::Price,
                    value,
                };
            }
        }
    }

    /// Discards price and confirmation state when the chart changes symbols.
    pub(super) fn change_symbol(&mut self, symbol: &str) {
        let request = OrderRequest::new(symbol, self.side, self.quantity, OrderType::Market);
        self.open(&request);
    }

    /// The ask for a buy and the bid for a sell, if the market quotes them.
    fn touch(&self, market: &Market<'_>) -> Option<f64> {
        match self.side {
            OrderSide::Buy => market.ask,
            OrderSide::Sell => market.bid,
        }
        .filter(|price| price.is_finite())
    }

    /// The price the order enters at: its limit price, its stop price, or for a market order the
    /// ask of a buy or the bid of a sell, or the last price without them.
    fn entry(&self, market: &Market<'_>) -> Option<f64> {
        match self.kind {
            Kind::Market => self.touch(market).or(market.last),
            Kind::Limit | Kind::Stop => Some(self.price),
            Kind::StopLimit => Some(self.limit),
        }
        .filter(|price| price.is_finite() && *price > 0.0)
    }

    /// The enabled take profit or stop loss of an order entering at `entry`, snapped to the tick,
    /// or none without brackets. Fails when it is not on its side of the entry. Without a
    /// `quantity`, a stop loss in money cannot be priced.
    fn exit(
        &self,
        market: &Market<'_>,
        side: OrderSide,
        entry: f64,
        quantity: Option<f64>,
        profit: bool,
    ) -> Result<Option<f64>, String> {
        let (enabled, bracket) = if profit {
            self.take_profit
        } else {
            self.stop_loss
        };
        if !(market.capabilities.brackets && enabled) {
            return Ok(None);
        }
        let per_point = match (bracket.mode, quantity) {
            (BracketMode::Money, None) => {
                return Err("Sizing by risk needs the stop loss as a price, ticks or %".into());
            }
            (_, quantity) => quantity.unwrap_or(1.0) * market.rules.point_value,
        };
        let price = to_step(
            bracket.price(side, entry, market.tick, per_point, profit),
            market.tick,
            f64::round,
        );
        // A buy exits above its entry for a profit and below it for a loss; a sell the other way.
        let above = (price > entry) == (side == OrderSide::Buy);
        if !price.is_finite() || price <= 0.0 || price == entry || above != profit {
            return Err(if profit {
                "The take profit must lie on the profit side of the entry"
            } else {
                "The stop loss must lie on the loss side of the entry"
            }
            .into());
        }
        Ok(Some(price))
    }

    /// `order` from the depth ladder with the ticket's enabled take profit and stop loss, priced
    /// from the order's own side, entry and quantity.
    fn with_brackets(
        &self,
        mut order: OrderRequest,
        market: &Market<'_>,
    ) -> Result<OrderRequest, String> {
        let entry = match order.order_type {
            OrderType::Limit(price) | OrderType::Stop(price) => Some(price),
            OrderType::StopLimit { limit, .. } => Some(limit),
            OrderType::Market => match order.side {
                OrderSide::Buy => market.ask,
                OrderSide::Sell => market.bid,
            }
            .or(market.last),
        }
        .map(|price| to_step(price, market.tick, f64::round))
        .filter(|price| price.is_finite() && *price > 0.0)
        .ok_or("Enter a positive price")?;
        order.stop_loss = self.exit(market, order.side, entry, Some(order.quantity), false)?;
        order.take_profit = self.exit(market, order.side, entry, Some(order.quantity), true)?;
        Ok(order)
    }

    /// The order the ticket describes, or what is wrong with it.
    pub fn request(&self, market: &Market<'_>) -> Result<OrderRequest, String> {
        if !market.rules_ready {
            return Err("Waiting for the symbol's trading rules".into());
        }
        if !self.kind.accepted(&market.capabilities) {
            return Err("The account does not accept this order type".into());
        }
        let snap = |value| to_step(value, market.tick, f64::round);
        let entry = self
            .entry(market)
            .map(snap)
            .filter(|price| price.is_finite() && *price > 0.0)
            .ok_or("Enter a positive price")?;
        let order_type = match self.kind {
            Kind::Market => OrderType::Market,
            Kind::Limit => OrderType::Limit(snap(self.price)),
            Kind::Stop => OrderType::Stop(snap(self.price)),
            Kind::StopLimit if self.price.is_finite() && snap(self.price) > 0.0 => {
                OrderType::StopLimit {
                    stop: snap(self.price),
                    limit: snap(self.limit),
                }
            }
            Kind::StopLimit => return Err("Enter a positive stop price".into()),
        };
        let rules = market.rules;
        // Units and value size the order before its exits; risk sizing needs the stop loss first.
        let mut quantity = match self.sizing {
            Sizing::Units => Some(order_quantity(self.quantity, rules)?),
            Sizing::Value => Some(order_quantity(
                self.amount / (entry * rules.point_value),
                rules,
            )?),
            Sizing::Risk => None,
        };
        let stop_loss = self.exit(market, self.side, entry, quantity, false)?;
        let quantity = match quantity.take() {
            Some(quantity) => quantity,
            None => {
                let stop = stop_loss.ok_or("Sizing by risk needs a stop loss")?;
                let balance = market
                    .balance
                    .filter(|balance| balance.is_finite() && *balance > 0.0)
                    .ok_or("Sizing by risk needs the account's balance")?;
                let risk = balance * self.amount / 100.0;
                order_quantity(risk / ((entry - stop).abs() * rules.point_value), rules)?
            }
        };
        let take_profit = self.exit(market, self.side, entry, Some(quantity), true)?;

        let mut request = OrderRequest::new(&self.symbol, self.side, quantity, order_type);
        request.take_profit = take_profit;
        request.stop_loss = stop_loss;
        request.time_in_force = if rules.time_in_force.contains(&self.time_in_force) {
            self.time_in_force
        } else {
            rules.time_in_force[0]
        };
        Ok(request)
    }

    /// Draws the ticket's contents into `ui`, the panel the host gives it. Returns what the user
    /// asked for: an order submitted on the Order tab, or an order, change or cancellation from
    /// the depth ladder.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        market: &Market<'_>,
        presets: &mut Vec<OrderPreset>,
        one_click: &mut bool,
    ) -> Option<Action> {
        if !market.rules.time_in_force.contains(&self.time_in_force) {
            self.time_in_force = market.rules.time_in_force[0];
        }
        ui.spacing_mut().item_spacing.y = 8.0;
        self.header(ui, market, presets, one_click);
        self.switch(ui);
        let mut action = match self.tab {
            Tab::Order => {
                self.dom_error = None;
                let action = egui::Panel::bottom(ui.make_persistent_id("order_ticket_send"))
                    .frame(egui::Frame::NONE.inner_margin(egui::Margin::symmetric(0, 8)))
                    .show_separator_line(false)
                    .show(ui, |ui| self.send(ui, market))
                    .inner;
                egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| self.form(ui, market));
                action
            }
            Tab::Dom => {
                if let Some(reason) = &self.dom_error {
                    ui.label(
                        RichText::new(l10n::text(ui.ctx(), reason))
                            .color(ui.active_theme().ui.error)
                            .size(12.0),
                    );
                }
                let height = ui.available_height().max(360.0);
                let ladder = egui::ScrollArea::vertical()
                    .auto_shrink(false)
                    .show(ui, |ui| {
                        ui.set_height(height);
                        self.ladder.show(
                            ui,
                            &self.symbol,
                            market,
                            &mut self.quantity,
                            &mut self.time_in_force,
                        )
                    })
                    .inner;
                // A new order from the ladder waits for confirmation without one-click trading.
                match ladder {
                    Some(Action::Place(request)) => {
                        let bracketed = self.with_brackets(request, market);
                        self.dom_error = bracketed.as_ref().err().cloned();
                        match bracketed {
                            Ok(request) if *one_click => Some(Action::Place(request)),
                            Ok(request) => {
                                self.confirm = Some(request);
                                None
                            }
                            Err(_) => None,
                        }
                    }
                    Some(Action::Cancel(ids)) => {
                        self.cancel = Some(ids);
                        None
                    }
                    action => action,
                }
            }
        };
        if self.confirm.as_ref().is_some_and(|request| {
            request.symbol != self.symbol
                || !market.rules_ready
                || !market.capabilities.accepts(&request.order_type)
                || !market.rules.time_in_force.contains(&request.time_in_force)
                || order_quantity(request.quantity, market.rules) != Ok(request.quantity)
                || (request.stop_loss.is_some() || request.take_profit.is_some())
                    && !market.capabilities.brackets
        }) {
            self.confirm = None;
        }
        if let Some(ids) = &mut self.cancel {
            ids.retain(|id| market.orders.iter().any(|order| order.id == *id));
            if ids.is_empty() {
                self.cancel = None;
            }
        }
        if let Some(ids) = &self.cancel {
            let mut open = true;
            let result = ConfirmDialog::new(
                "dom_cancel_confirm",
                "Cancel orders",
                format!("Cancel {} working orders for {}?", ids.len(), self.symbol),
            )
            .confirm_text("Cancel orders")
            .cancel_text("Back")
            .show(ui.ctx(), &mut open);
            if result.is_confirmed() {
                action = self.cancel.take().map(Action::Cancel);
            } else if result.is_cancelled() || !open {
                self.cancel = None;
            }
        }
        if let Some(request) = &self.confirm {
            let mut open = true;
            let message = confirmation(
                request,
                market.decimals,
                decimals(market.rules.quantity_step),
            );
            let result = ConfirmDialog::new("order_ticket_confirm", "Place order", message)
                .confirm_text("Place order")
                .cancel_text("Back")
                .show(ui.ctx(), &mut open);
            if result.is_confirmed() {
                action = self.confirm.take().map(Action::Place);
            } else if result.is_cancelled() || !open {
                self.confirm = None;
            }
        }
        action
    }

    /// The symbol, the menu of presets and one-click trading, and the close button.
    fn header(
        &mut self,
        ui: &mut Ui,
        market: &Market<'_>,
        presets: &mut Vec<OrderPreset>,
        one_click: &mut bool,
    ) {
        ui.horizontal(|ui| {
            ui.set_min_height(32.0);
            ui.label(RichText::new(&self.symbol).strong().size(15.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if IconButton::new(&icons::CLOSE)
                    .tooltip("Close")
                    .show(ui)
                    .clicked()
                {
                    self.open = false;
                }
                let more = IconButton::new(&icons::UI_MORE_HORIZONTAL)
                    .tooltip("More")
                    .show(ui);
                egui::Popup::menu(&more)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show(|ui| self.more(ui, market, presets, one_click));
            });
        });
    }

    fn more(
        &mut self,
        ui: &mut Ui,
        market: &Market<'_>,
        presets: &mut Vec<OrderPreset>,
        one_click: &mut bool,
    ) {
        ui.checkbox(one_click, l10n::text(ui.ctx(), "One-click trading"))
            .on_hover_text(l10n::text(
                ui.ctx(),
                "Buy and Sell, and the depth ladder, place orders at once, without confirmation",
            ));
        ui.separator();
        ui.weak(l10n::text(ui.ctx(), "Presets"));
        if presets.is_empty() {
            ui.weak(l10n::text(ui.ctx(), "No presets saved"));
        }
        let mut remove = None;
        for (index, preset) in presets.iter().enumerate() {
            ui.horizontal(|ui| {
                if ui
                    .selectable_label(false, l10n::text(ui.ctx(), &preset.name))
                    .clicked()
                {
                    self.apply(preset);
                }
                if ui
                    .small_button("✕")
                    .on_hover_text(l10n::text(ui.ctx(), "Delete preset"))
                    .clicked()
                {
                    remove = Some(index);
                }
            });
        }
        if let Some(index) = remove {
            presets.remove(index);
        }
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.preset_name)
                    .hint_text(l10n::text(ui.ctx(), "Preset name"))
                    .desired_width(120.0),
            );
            let name = self.preset_name.trim();
            if ui
                .add_enabled(
                    !name.is_empty(),
                    egui::Button::new(l10n::text(ui.ctx(), "Save")),
                )
                .clicked()
            {
                let preset = self.preset(name.to_owned(), market);
                presets.retain(|existing| existing.name != preset.name);
                presets.push(preset);
                self.preset_name.clear();
            }
        });
    }

    /// The Order and DOM switch.
    fn switch(&mut self, ui: &mut Ui) {
        let theme = ui.active_theme();
        let (track, selected_fill) = track(ui);
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::hover());
        ui.painter().rect_filled(rect, 8.0, track);
        let width = rect.width() / 2.0;
        for (index, (tab, label)) in [(Tab::Order, "Order"), (Tab::Dom, "DOM")]
            .into_iter()
            .enumerate()
        {
            let cell = Rect::from_min_size(
                rect.min + vec2(index as f32 * width, 0.0),
                vec2(width, rect.height()),
            );
            let selected = self.tab == tab;
            if hit(ui, cell, ("order_ticket_tab", index), label, Some(selected)).clicked() {
                self.tab = tab;
            }
            if selected {
                ui.painter()
                    .rect_filled(cell.shrink(3.0), 6.0, selected_fill);
            }
            text(
                ui,
                cell.center(),
                Align2::CENTER_CENTER,
                label,
                14.0,
                if selected {
                    theme.ui.text
                } else {
                    theme.ui.text_secondary
                },
            );
        }
    }

    /// The Order tab above the send button.
    fn form(&mut self, ui: &mut Ui, market: &Market<'_>) {
        let theme = ui.active_theme();
        self.quote(ui, market);
        self.kinds(ui, market);
        if let Some(entry) = self.entry(market) {
            for (bracket, profit) in [(&mut self.take_profit, true), (&mut self.stop_loss, false)] {
                if !bracket.0 && bracket.1.mode == BracketMode::Price && bracket.1.value == 0.0 {
                    bracket.1.value = to_step(
                        entry + Bracket::sign(self.side, profit) * 100.0 * market.tick,
                        market.tick,
                        f64::round,
                    );
                }
            }
        }
        let request = self.request(market);
        let entry = self
            .entry(market)
            .map(|price| to_step(price, market.tick, f64::round));
        let rules = market.rules;
        let quantity = request
            .as_ref()
            .map_or(self.quantity, |request| request.quantity);
        let per_point = quantity * rules.point_value;
        let currency = market.currency;

        // Prices of limit, stop and stop-limit orders.
        let prices: &[(&str, bool)] = match self.kind {
            Kind::Market => &[],
            Kind::Limit | Kind::Stop => &[("Price", false)],
            Kind::StopLimit => &[("Stop price", false), ("Limit price", true)],
        };
        let touch = self.touch(market).or(market.last);
        for &(label, limit) in prices {
            muted(ui, label);
            let in_ticks = self.price_in_ticks && touch.is_some();
            let price = if limit {
                &mut self.limit
            } else {
                &mut self.price
            };
            let mut swapped = false;
            field(ui, |ui| {
                match touch.filter(|_| in_ticks) {
                    Some(touch) => {
                        let mut ticks = ((*price - touch) / market.tick).round();
                        if number(ui, (label, "ticks"), &mut ticks, 0).changed() {
                            *price = to_step(touch + ticks * market.tick, market.tick, f64::round);
                        }
                    }
                    None => {
                        number(ui, label, price, market.decimals);
                    }
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(touch) = touch {
                        swapped = swap(ui, "Enter the price or its distance in ticks");
                        let suffix = if in_ticks {
                            grouped(*price, market.decimals, false)
                        } else {
                            let reference = match (self.side, market.ask, market.bid) {
                                (OrderSide::Buy, Some(_), _) => "Ask",
                                (OrderSide::Sell, _, Some(_)) => "Bid",
                                _ => "Last",
                            };
                            let ticks = ((*price - touch) / market.tick).round();
                            match ticks {
                                0.0 => reference.to_owned(),
                                ticks if ticks < 0.0 => format!("{reference} - {}", -ticks),
                                ticks => format!("{reference} + {ticks}"),
                            }
                        };
                        muted(ui, &suffix);
                    }
                });
            });
            if swapped {
                self.price_in_ticks = !self.price_in_ticks;
            }
        }

        // The quantity, and the risk at the stop loss.
        let risk_money = request.as_ref().ok().and_then(|request| {
            Some((entry? - request.stop_loss?).abs() * request.quantity * rules.point_value)
        });
        let units = |ticket: &mut Self, ui: &mut Ui| {
            let response = dropdown(
                ui,
                if ticket.sizing == Sizing::Value {
                    format!("Amount, {currency}")
                } else {
                    "Units".to_owned()
                },
            );
            let old = ticket.sizing;
            egui::Popup::menu(&response).show(|ui| {
                ui.selectable_value(
                    &mut ticket.sizing,
                    Sizing::Units,
                    l10n::text(ui.ctx(), "Units"),
                );
                ui.selectable_value(
                    &mut ticket.sizing,
                    Sizing::Value,
                    l10n::format_text(
                        ui.ctx(),
                        "Amount, {currency}",
                        &[("currency", currency.to_string())],
                    ),
                );
            });
            if ticket.sizing != old {
                if ticket.sizing == Sizing::Value {
                    ticket.amount = entry.unwrap_or_default() * per_point;
                } else {
                    ticket.quantity = quantity;
                }
            }
        };
        let quantity_field = |ticket: &mut Self, ui: &mut Ui, suffix: bool| {
            let mut swapped = false;
            field(ui, |ui| {
                if ticket.sizing == Sizing::Value {
                    number(ui, "amount", &mut ticket.amount, 2);
                } else {
                    let mut shown = quantity;
                    let places = decimals(rules.quantity_step);
                    if number(ui, "units", &mut shown, places).changed() {
                        (ticket.sizing, ticket.quantity) = (Sizing::Units, shown);
                    }
                }
                if !suffix {
                    return;
                }
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if let Some(entry) = entry {
                        swapped = swap(ui, "Enter the units or the amount");
                        muted(
                            ui,
                            &if ticket.sizing == Sizing::Value {
                                format!(
                                    "{} units",
                                    grouped(quantity, decimals(rules.quantity_step), false)
                                )
                            } else {
                                format!("{} {currency}", grouped(entry * per_point, 2, false))
                            },
                        );
                    }
                });
            });
            if swapped {
                // The order keeps its size: the amount becomes its value, or the units its quantity.
                if ticket.sizing == Sizing::Value {
                    (ticket.sizing, ticket.quantity) = (Sizing::Units, quantity);
                } else if let Some(entry) = entry {
                    (ticket.sizing, ticket.amount) = (Sizing::Value, entry * per_point);
                }
            }
        };
        if market.capabilities.brackets {
            let risk_enabled = self.stop_loss.0 && market.balance.is_some();
            ui.columns(2, |columns| {
                units(self, &mut columns[0]);
                quantity_field(self, &mut columns[0], false);
                muted(&mut columns[1], "Risk, % balance");
                columns[1].add_enabled_ui(risk_enabled, |ui| {
                    field(ui, |ui| {
                        let mut risk = if self.sizing == Sizing::Risk {
                            self.amount
                        } else {
                            (risk_money.zip(market.balance))
                                .map_or(0.0, |(risk, balance)| risk / balance * 100.0)
                        };
                        if number(ui, "risk", &mut risk, 2).changed() {
                            (self.sizing, self.amount) = (Sizing::Risk, risk);
                        }
                    });
                });
            });
        } else {
            units(self, ui);
            quantity_field(self, ui, true);
        }

        if market.capabilities.brackets {
            self.exits(ui, market, request.as_ref().ok(), entry, per_point);
        }

        ui.add_space(8.0);
        title(ui, "Extra settings");
        muted(ui, "Time in force");
        time_in_force_menu(ui, "order_ticket_tif", &mut self.time_in_force, rules);

        ui.add_space(8.0);
        title(ui, "Order info");
        let Some((request, entry)) = request.as_ref().ok().zip(entry) else {
            return;
        };
        let per_point = request.quantity * rules.point_value;
        let value = entry * per_point;
        if let Some(leverage) = rules.leverage {
            let margin = value / leverage;
            let available = market.balance.unwrap_or_default();
            let row = info(
                ui,
                "Est. margin",
                &format!(
                    "{} / {}",
                    grouped(margin, 2, false),
                    grouped(available, 2, false)
                ),
                "",
                theme.ui.text,
            );
            let help =
                Rect::from_center_size(pos2(row.left() + 82.0, row.center().y), vec2(14.0, 14.0));
            icons::WIDGET_BAR_HELP
                .as_image_tinted(help.size(), theme.ui.text_muted)
                .paint_at(ui, help);
            ui.interact(
                help,
                ui.make_persistent_id("order_ticket_margin_help"),
                Sense::hover(),
            )
            .on_hover_text(l10n::text(
                ui.ctx(),
                "Estimated margin: trade value divided by leverage, compared with account balance. \
                 This does not include margin already used by other positions or working orders.",
            ));
            let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 4.0), Sense::hover());
            ui.painter().rect_filled(bar, 2.0, theme.ui.border);
            let used = if available > 0.0 {
                (margin / available).clamp(0.0, 1.0) as f32
            } else {
                1.0
            };
            let color = if margin > available {
                theme.ui.error
            } else {
                theme.ui.accent
            };
            let fill = Rect::from_min_size(bar.min, vec2(bar.width() * used, bar.height()));
            ui.painter().rect_filled(fill, 2.0, color);
            info(ui, "Leverage", &format!("{leverage}:1"), "", theme.ui.text);
        }
        let tick_value = market.tick * per_point;
        let places = market.decimals.max(2);
        info(
            ui,
            "Tick value",
            &grouped(tick_value, places, false),
            currency,
            theme.ui.text,
        );
        info(
            ui,
            "Trade value",
            &grouped(value, 2, false),
            currency,
            theme.ui.text,
        );
        if let Some(exit) = request.take_profit {
            let reward = grouped((exit - entry).abs() * per_point, 2, false);
            info(ui, "Reward", &reward, currency, theme.ui.take_profit);
        }
        if let Some(exit) = request.stop_loss {
            let risk = grouped((exit - entry).abs() * per_point, 2, false);
            info(ui, "Risk", &risk, currency, theme.ui.stop_loss);
        }
    }

    /// The Sell and Buy halves with the bid and ask, the chosen side filled, and the spread.
    fn quote(&mut self, ui: &mut Ui, market: &Market<'_>) {
        let theme = ui.active_theme();
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 56.0), Sense::hover());
        ui.painter().rect_filled(rect, 8.0, track(ui).0);
        let width = rect.width() / 2.0;
        let sides = [
            (OrderSide::Sell, "Sell", market.bid, Align2::LEFT_TOP),
            (OrderSide::Buy, "Buy", market.ask, Align2::RIGHT_TOP),
        ];
        for (index, (side, label, price, align)) in sides.into_iter().enumerate() {
            let half = Rect::from_min_size(
                rect.min + vec2(index as f32 * width, 0.0),
                vec2(width, rect.height()),
            );
            let selected = self.side == side;
            if hit(
                ui,
                half,
                ("order_ticket_side", index),
                label,
                Some(selected),
            )
            .clicked()
            {
                self.side = side;
            }
            let color = side_color(&theme, side);
            if selected {
                let corners = if index == 0 {
                    egui::CornerRadius {
                        nw: 8,
                        sw: 8,
                        ..Default::default()
                    }
                } else {
                    egui::CornerRadius {
                        ne: 8,
                        se: 8,
                        ..Default::default()
                    }
                };
                ui.painter()
                    .rect_filled(half, corners, color.gamma_multiply(0.25));
            }
            let text_color = if selected { color } else { theme.ui.text };
            let inner = half.shrink2(vec2(12.0, 8.0));
            let anchor = align.pos_in_rect(&inner);
            text(ui, anchor, align, label, 13.0, text_color);
            let price = price.or(market.last).map_or("-".to_owned(), |price| {
                grouped(price, market.decimals, false)
            });
            let below = anchor + vec2(0.0, 18.0);
            text(ui, below, align, &price, 17.0, text_color);
        }
        if let (Some(bid), Some(ask)) = (market.bid, market.ask) {
            let spread = grouped(ask - bid, market.decimals, false);
            let galley = ui.painter().layout_no_wrap(
                l10n::text(ui.ctx(), spread),
                FontId::proportional(11.0),
                theme.ui.text,
            );
            let chip = Rect::from_center_size(
                pos2(rect.center().x, rect.top() + 20.0),
                galley.size() + vec2(10.0, 4.0),
            );
            ui.painter().rect_filled(chip, 4.0, theme.ui.panel_bg);
            ui.painter()
                .galley(chip.center() - galley.size() / 2.0, galley, theme.ui.text);
        }
    }

    /// The order types the account accepts, as tabs with an underline.
    fn kinds(&mut self, ui: &mut Ui, market: &Market<'_>) {
        let theme = ui.active_theme();
        let kinds: Vec<_> = Kind::ALL
            .into_iter()
            .filter(|(kind, _)| kind.accepted(&market.capabilities))
            .collect();
        if kinds.is_empty() {
            return;
        }
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::hover());
        ui.painter().hline(
            rect.x_range(),
            rect.bottom() - 1.0,
            Stroke::new(2.0, theme.ui.border),
        );
        let width = rect.width() / kinds.len() as f32;
        for (index, (kind, label)) in kinds.into_iter().enumerate() {
            let cell = Rect::from_min_size(
                rect.min + vec2(index as f32 * width, 0.0),
                vec2(width, rect.height()),
            );
            let selected = self.kind == kind;
            let response = hit(
                ui,
                cell,
                ("order_ticket_kind", index),
                label,
                Some(selected),
            );
            if response.clicked() {
                if self.kind == Kind::Market && kind != Kind::Market && self.price == 0.0 {
                    self.price = self.touch(market).or(market.last).unwrap_or_default();
                    self.limit = self.price;
                }
                self.kind = kind;
            }
            let color = if selected || response.hovered() {
                theme.ui.text
            } else {
                theme.ui.text_secondary
            };
            text(ui, cell.center(), Align2::CENTER_CENTER, label, 14.0, color);
            if selected {
                let line = Rect::from_min_max(
                    pos2(cell.left() + 4.0, cell.bottom() - 3.0),
                    pos2(cell.right() - 4.0, cell.bottom()),
                );
                ui.painter().rect_filled(line, 1.5, theme.ui.text);
            }
        }
    }

    /// The collapsible exits: a take profit and a stop loss, each with a toggle and the way its
    /// distance is entered.
    fn exits(
        &mut self,
        ui: &mut Ui,
        market: &Market<'_>,
        request: Option<&OrderRequest>,
        entry: Option<f64>,
        per_point: f64,
    ) {
        let theme = ui.active_theme();
        ui.add_space(8.0);
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
        if hit(
            ui,
            row,
            "order_ticket_exits",
            "Exits",
            Some(self.exits_open),
        )
        .clicked()
        {
            self.exits_open = !self.exits_open;
        }
        let galley = ui.painter().layout_no_wrap(
            l10n::text(ui.ctx(), "Exits"),
            FontId::proportional(15.0),
            theme.ui.text,
        );
        let width = galley.size().x;
        ui.painter().galley(
            row.left_center() - vec2(0.0, galley.size().y / 2.0),
            galley,
            theme.ui.text,
        );
        let ratio = request.and_then(|request| {
            let entry = entry?;
            let reward = (request.take_profit? - entry).abs();
            let risk = (request.stop_loss? - entry).abs();
            (risk > 0.0).then(|| reward / risk)
        });
        if let Some(ratio) = ratio {
            let ratio = format!("  ·  Risk/Reward {ratio:.2}");
            let at = row.left_center() + vec2(width, 0.0);
            text(
                ui,
                at,
                Align2::LEFT_CENTER,
                &ratio,
                13.0,
                theme.ui.text_muted,
            );
        }
        chevron(
            ui,
            pos2(row.right() - 8.0, row.center().y),
            theme.ui.text,
            self.exits_open,
        );
        if !self.exits_open {
            return;
        }
        let side = self.side;
        let tick = market.tick;
        for (label, profit) in [("Take profit", true), ("Stop loss", false)] {
            let bracket = if profit {
                &mut self.take_profit
            } else {
                &mut self.stop_loss
            };
            let exit = entry.map(|entry| {
                let price = bracket.1.price(side, entry, tick, per_point, profit);
                (entry, to_step(price, tick, f64::round))
            });
            let mut mode = bracket.1.mode;
            ui.horizontal(|ui| {
                let response = dropdown(
                    ui,
                    format!("{label}, {}", bracket.1.mode.name(market.currency)),
                );
                egui::Popup::menu(&response).show(|ui| {
                    for option in BracketMode::ALL {
                        ui.selectable_value(
                            &mut mode,
                            option,
                            l10n::text(ui.ctx(), option.name(market.currency)),
                        );
                    }
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    toggle(ui, &mut bracket.0, label);
                });
            });
            let mut swapped = false;
            ui.add_enabled_ui(bracket.0, |ui| {
                field(ui, |ui| {
                    let places = match bracket.1.mode {
                        BracketMode::Price => market.decimals,
                        BracketMode::Ticks => 0,
                        BracketMode::Percent | BracketMode::Money => 2,
                    };
                    number(ui, label, &mut bracket.1.value, places);
                    let Some((entry, price)) = exit else {
                        return;
                    };
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        swapped = swap(ui, "Enter the price or its distance in ticks");
                        muted(
                            ui,
                            &if bracket.1.mode == BracketMode::Price {
                                format!("{} ticks", ((price - entry) / tick).abs().round())
                            } else {
                                grouped(price, market.decimals, false)
                            },
                        );
                    });
                });
            });
            if swapped {
                mode = if mode == BracketMode::Price {
                    BracketMode::Ticks
                } else {
                    BracketMode::Price
                };
            }
            if mode != bracket.1.mode {
                // The exit stays where it is: its distance is written in the new way.
                bracket.1 = match exit {
                    Some((entry, price)) => {
                        Bracket::at(mode, price, side, entry, tick, per_point, profit)
                    }
                    None => Bracket { mode, ..bracket.1 },
                };
            }
        }
    }

    /// The full-width send button in the side's colour, or what keeps the order from being sent.
    fn send(&mut self, ui: &mut Ui, market: &Market<'_>) -> Option<Action> {
        let theme = ui.active_theme();
        let request = self.request(market);
        if let Err(reason) = &request {
            ui.label(
                RichText::new(l10n::text(ui.ctx(), reason))
                    .color(theme.ui.error)
                    .size(12.0),
            );
        }
        let mut preview = None;
        if market.capabilities.preview
            && ui
                .add_enabled(
                    request.is_ok(),
                    egui::Button::new(l10n::text(ui.ctx(), "Preview order"))
                        .min_size(vec2(ui.available_width(), 32.0)),
                )
                .on_disabled_hover_text(l10n::text(ui.ctx(), "Fix the order to preview it"))
                .clicked()
        {
            preview = request.clone().ok().map(Action::Preview);
        }
        let (rect, response) =
            ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::click());
        let name = side_name(self.side);
        let detail = request.as_ref().map_or(String::new(), |request| {
            summary(
                request,
                market.decimals,
                decimals(market.rules.quantity_step),
            )
        });
        let translated_label = format!(
            "{} {}",
            l10n::text(ui.ctx(), name),
            l10n::text(ui.ctx(), &detail)
        );
        response
            .widget_info(|| WidgetInfo::labeled(Role::Button, request.is_ok(), &translated_label));
        let mut fill = side_fill(&theme, self.side);
        if request.is_err() {
            fill = fill.gamma_multiply(0.5);
        } else if response.hovered() {
            fill = fill.gamma_multiply(0.9);
        }
        ui.painter().rect_filled(rect, 8.0, fill);
        let top = rect.center_top() + vec2(0.0, 8.0);
        text(ui, top, Align2::CENTER_TOP, name, 16.0, Color32::WHITE);
        let bottom = rect.center_bottom() - vec2(0.0, 8.0);
        text(
            ui,
            bottom,
            Align2::CENTER_BOTTOM,
            &detail,
            12.0,
            Color32::WHITE,
        );
        (response.clicked())
            .then_some(request.ok())
            .flatten()
            .map(Action::Place)
            .or(preview)
    }

    fn apply(&mut self, preset: &OrderPreset) {
        self.sizing = Sizing::Units;
        self.quantity = preset.quantity;
        self.take_profit = preset
            .take_profit
            .map_or((false, self.take_profit.1), |bracket| (true, bracket));
        self.stop_loss = preset
            .stop_loss
            .map_or((false, self.stop_loss.1), |bracket| (true, bracket));
    }

    fn preset(&self, name: String, market: &Market<'_>) -> OrderPreset {
        let quantity = self
            .request(market)
            .map_or(self.quantity, |request| request.quantity);
        OrderPreset {
            name,
            quantity,
            take_profit: self.take_profit.0.then_some(self.take_profit.1),
            stop_loss: self.stop_loss.0.then_some(self.stop_loss.1),
        }
    }
}

/// `quantity` rounded down to a whole multiple of `step`, at the step's decimals.
fn round_down(quantity: f64, step: f64) -> f64 {
    // The tolerance keeps 0.3 / 0.1 = 2.9999999999999996 at 3 steps.
    to_step(quantity, step, |steps| (steps + 1e-9).floor())
}

/// Rounds a quantity down to the symbol's step and checks its bounds.
pub(super) fn order_quantity(quantity: f64, rules: &TradingRules) -> Result<f64, String> {
    let quantity = round_down(quantity, rules.quantity_step);
    if !quantity.is_finite() || quantity < rules.min_quantity {
        return Err(format!(
            "The quantity is below the minimum of {}",
            rules.min_quantity
        ));
    }
    if rules.max_quantity.is_some_and(|max| quantity > max) {
        return Err(format!(
            "The quantity is above the maximum of {}",
            rules.max_quantity.unwrap_or_default()
        ));
    }
    Ok(quantity)
}

pub(super) use crate::theme::trading_side_color as side_color;

/// The fill of a button that places an order on a side, under white text.
pub(super) fn side_fill(theme: &Theme, side: OrderSide) -> Color32 {
    match side {
        OrderSide::Buy => theme.ui.accent,
        OrderSide::Sell => theme.ui.danger_bg,
    }
}

/// The fill of a switch's track and of its chosen segment.
fn track(ui: &Ui) -> (Color32, Color32) {
    let colors = ui.active_theme().ui;
    if ui.visuals().dark_mode {
        (colors.panel_bg_floating, colors.panel_bg_secondary)
    } else {
        (colors.panel_bg_secondary, colors.panel_bg)
    }
}

pub(super) fn side_name(side: OrderSide) -> &'static str {
    match side {
        OrderSide::Buy => "Buy",
        OrderSide::Sell => "Sell",
    }
}

/// An order as the send button and the confirmation write it: "1 ES @ 4,000.25 LIMIT".
fn summary(request: &OrderRequest, decimals: usize, quantity_decimals: usize) -> String {
    let at = |price: f64, kind: &str| format!(" @ {} {kind}", grouped(price, decimals, false));
    let order_type = match request.order_type {
        OrderType::Limit(price) => at(price, "LIMIT"),
        OrderType::Stop(price) => at(price, "STOP"),
        OrderType::StopLimit { stop, .. } => at(stop, "STOP LIMIT"),
        _ => " MKT".to_owned(),
    };
    format!(
        "{} {}{order_type}",
        grouped(request.quantity, quantity_decimals, false),
        request.symbol
    )
}

/// The question that confirms an order: "Buy 1 ES @ 4,000.25 LIMIT?", then its exits.
fn confirmation(request: &OrderRequest, decimals: usize, quantity_decimals: usize) -> String {
    let mut text = format!(
        "{} {}?",
        side_name(request.side),
        summary(request, decimals, quantity_decimals)
    );
    for (label, price) in [
        ("Take profit", request.take_profit),
        ("Stop loss", request.stop_loss),
    ] {
        if let Some(price) = price {
            text.push_str(&format!("\n{label} {}", grouped(price, decimals, false)));
        }
    }
    text
}

/// What the dialog shows for a previewed order: its confirmation, then the broker's estimates
/// and warnings.
pub(super) fn preview_message(
    request: &OrderRequest,
    estimate: &OrderPreview,
    rules: &TradingRules,
    decimals: usize,
    currency: &str,
) -> String {
    let mut text = confirmation(request, decimals, self::decimals(rules.quantity_step));
    for (label, value) in [
        ("Cost", estimate.cost),
        ("Commission", estimate.commission),
        ("Margin", estimate.margin),
    ] {
        if let Some(value) = value.filter(|value| value.is_finite()) {
            text.push_str(&format!(
                "\n{label}: {} {currency}",
                grouped(value, 2, false)
            ));
        }
    }
    for warning in &estimate.warnings {
        text.push_str(&format!("\n{warning}"));
    }
    text
}

/// The name of a time in force in the ticket's menu.
fn time_in_force_name(time_in_force: TimeInForce) -> &'static str {
    match time_in_force {
        TimeInForce::Day => "Day",
        TimeInForce::Week => "Week",
        TimeInForce::Month => "Month",
        TimeInForce::GoodTillCancelled => "Good till cancelled",
        TimeInForce::ImmediateOrCancel => "Immediate or cancel",
        TimeInForce::FillOrKill => "Fill or kill",
    }
}

/// A full-width menu of the times in force the broker accepts for the symbol.
pub(super) fn time_in_force_menu(
    ui: &mut Ui,
    id: &str,
    value: &mut TimeInForce,
    rules: &TradingRules,
) {
    let theme = ui.active_theme();
    ui.spacing_mut().interact_size.y = 36.0;
    // Drawn like the ticket's fields: the panel's fill in a rounded outline.
    let widgets = &mut ui.visuals_mut().widgets;
    for state in [
        &mut widgets.inactive,
        &mut widgets.hovered,
        &mut widgets.active,
    ] {
        state.weak_bg_fill = theme.ui.panel_bg;
        state.corner_radius = 6.into();
    }
    widgets.inactive.bg_stroke = Stroke::new(1.0, theme.ui.border_control);
    egui::ComboBox::from_id_salt(id)
        .width(ui.available_width())
        .selected_text(l10n::text(ui.ctx(), time_in_force_name(*value)))
        .icon(|ui, rect, visuals, open| {
            chevron(ui, rect.center(), visuals.fg_stroke.color, open);
        })
        .show_ui(ui, |ui| {
            for option in &rules.time_in_force {
                ui.selectable_value(
                    value,
                    *option,
                    l10n::text(ui.ctx(), time_in_force_name(*option)),
                );
            }
        });
}

/// A click area over `rect`, announced as `label`.
fn hit(
    ui: &mut Ui,
    rect: Rect,
    id: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    selected: Option<bool>,
) -> Response {
    let response = ui.interact(rect, ui.make_persistent_id(id), Sense::click());
    let translated_label = l10n::text(ui.ctx(), label);
    response.widget_info(|| match selected {
        Some(selected) => {
            WidgetInfo::selected(Role::Button, ui.is_enabled(), selected, &translated_label)
        }
        None => WidgetInfo::labeled(Role::Button, ui.is_enabled(), &translated_label),
    });
    response
}

pub(super) fn text(ui: &Ui, at: egui::Pos2, align: Align2, text: &str, size: f32, color: Color32) {
    ui.painter().text(
        at,
        align,
        l10n::text(ui.ctx(), text),
        FontId::proportional(size),
        color,
    );
}

pub(super) fn muted(ui: &mut Ui, text: &str) {
    let color = ui.active_theme().ui.text_muted;
    ui.label(
        RichText::new(l10n::text(ui.ctx(), text))
            .color(color)
            .size(13.0),
    );
}

fn title(ui: &mut Ui, text: &str) {
    ui.label(
        RichText::new(l10n::text(ui.ctx(), text))
            .strong()
            .size(15.0),
    );
}

/// A label with a small chevron that opens a menu.
fn dropdown(ui: &mut Ui, label: String) -> Response {
    let color = ui.active_theme().ui.text_muted;
    let galley = ui.painter().layout_no_wrap(
        l10n::text(ui.ctx(), label),
        FontId::proportional(13.0),
        color,
    );
    let size = vec2(galley.size().x + 16.0, galley.size().y.max(18.0));
    let (rect, response) = ui.allocate_exact_size(size, Sense::click());
    response.widget_info(|| WidgetInfo::labeled(Role::ComboBox, ui.is_enabled(), galley.text()));
    let color = if response.hovered() {
        ui.active_theme().ui.text
    } else {
        color
    };
    let at = rect.left_center() - vec2(0.0, galley.size().y / 2.0);
    ui.painter().galley(at, galley, color);
    chevron(ui, pos2(rect.right() - 5.0, rect.center().y), color, false);
    response
}

/// A chevron pointing up or down.
pub(super) fn chevron(ui: &Ui, center: egui::Pos2, color: Color32, up: bool) {
    let rise = if up { -2.0 } else { 2.0 };
    ui.painter().line(
        vec![
            center + vec2(-4.0, -rise),
            center + vec2(0.0, rise),
            center + vec2(4.0, -rise),
        ],
        Stroke::new(1.3, color),
    );
}

/// A bordered input box the width of `ui`, laid out from the left.
pub(super) fn field(ui: &mut Ui, add: impl FnOnce(&mut Ui)) {
    let theme = ui.active_theme();
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), Sense::hover());
    let stroke = Stroke::new(1.0, theme.ui.border_control);
    ui.painter()
        .rect(rect, 6.0, theme.ui.panel_bg, stroke, StrokeKind::Inside);
    let inner = rect.shrink2(vec2(10.0, 0.0));
    let layout = Layout::left_to_right(Align::Center);
    ui.scope_builder(UiBuilder::new().max_rect(inner).layout(layout), |ui| {
        add(ui);
    });
}

/// A number typed in place at `places` decimals. Invalid input invalidates the order until
/// corrected instead of silently submitting the previously entered value.
pub(super) fn number(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    value: &mut f64,
    places: usize,
) -> Response {
    let id = ui.make_persistent_id(id);
    let editing = ui.memory(|memory| memory.has_focus(id));
    let mut text = match ui.data(|data| data.get_temp::<String>(id)) {
        Some(text) if editing => text,
        _ => format!("{value:.places$}"),
    };
    let width = (ui.available_width() * 0.5).max(48.0);
    let response = ui.add(
        egui::TextEdit::singleline(&mut text)
            .id(id)
            .frame(egui::Frame::NONE)
            .desired_width(width)
            .font(FontId::proportional(14.0)),
    );
    if response.changed() {
        *value = text
            .trim()
            .replace(',', "")
            .parse::<f64>()
            .ok()
            .filter(|value| value.is_finite())
            .unwrap_or(f64::NAN);
    }
    if response.has_focus() {
        ui.data_mut(|data| data.insert_temp(id, text));
    }
    response
}

/// Two opposite arrows that swap what a field takes.
fn swap(ui: &mut Ui, tooltip: &str) -> bool {
    let theme = ui.active_theme();
    let (rect, response) = ui.allocate_exact_size(vec2(22.0, 22.0), Sense::click());
    let translated_label = l10n::text(ui.ctx(), tooltip);
    response.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), &translated_label));
    let color = if response.hovered() {
        theme.ui.text
    } else {
        theme.ui.text_muted
    };
    let stroke = Stroke::new(1.2, color);
    let c = rect.center();
    let painter = ui.painter();
    painter.line_segment([c + vec2(-6.0, -3.0), c + vec2(6.0, -3.0)], stroke);
    painter.line(
        vec![c + vec2(3.0, -6.0), c + vec2(6.0, -3.0), c + vec2(3.0, 0.0)],
        stroke,
    );
    painter.line_segment([c + vec2(6.0, 3.0), c + vec2(-6.0, 3.0)], stroke);
    painter.line(
        vec![
            c + vec2(-3.0, 0.0),
            c + vec2(-6.0, 3.0),
            c + vec2(-3.0, 6.0),
        ],
        stroke,
    );
    response
        .on_hover_text(l10n::text(ui.ctx(), tooltip))
        .clicked()
}

/// An on and off switch.
fn toggle(ui: &mut Ui, on: &mut bool, label: &str) {
    let theme = ui.active_theme();
    let (rect, mut response) = ui.allocate_exact_size(vec2(36.0, 20.0), Sense::click());
    if response.clicked() {
        *on = !*on;
        response.mark_changed();
    }
    let translated_label = l10n::text(ui.ctx(), label);
    response.widget_info(|| {
        WidgetInfo::selected(Role::Switch, ui.is_enabled(), *on, &translated_label)
    });
    let t = ui.ctx().animate_bool_responsive(response.id, *on);
    let track = if *on {
        theme.ui.text
    } else {
        theme.ui.border_control
    };
    ui.painter().rect_filled(rect, rect.height() / 2.0, track);
    let x = egui::lerp(rect.left() + 10.0..=rect.right() - 10.0, t);
    ui.painter()
        .circle_filled(pos2(x, rect.center().y), 7.0, theme.ui.panel_bg);
}

/// A row of the order info: `label` at the left, `value` and `unit` at the right. Returns the
/// row's rectangle.
fn info(ui: &mut Ui, label: &str, value: &str, unit: &str, color: Color32) -> Rect {
    let theme = ui.active_theme();
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 20.0), Sense::hover());
    let left = rect.left_center();
    text(
        ui,
        left,
        Align2::LEFT_CENTER,
        label,
        13.0,
        theme.ui.text_muted,
    );
    let mut right = rect.right_center();
    if !unit.is_empty() {
        let galley = ui.painter().layout_no_wrap(
            l10n::text(ui.ctx(), unit),
            FontId::proportional(11.0),
            theme.ui.text_muted,
        );
        right.x -= galley.size().x;
        ui.painter().galley(
            right - vec2(0.0, galley.size().y / 2.0),
            galley,
            theme.ui.text_muted,
        );
        right.x -= 4.0;
    }
    let galley = ui.painter().layout_no_wrap(
        l10n::text(ui.ctx(), value),
        FontId::proportional(13.0),
        color,
    );
    let at = right - vec2(galley.size().x, galley.size().y / 2.0);
    ui.painter().galley(at, galley, color);
    rect
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opening_a_ticket_replaces_brackets_and_prices() {
        let mut ticket = OrderTicket::default();
        let mut request = OrderRequest::new("ES", OrderSide::Buy, 2.0, OrderType::Limit(100.0));
        request.take_profit = Some(105.0);
        request.stop_loss = Some(95.0);
        for brackets in [true, false] {
            if !brackets {
                request = OrderRequest::new("NQ", OrderSide::Sell, 1.0, OrderType::Market);
            }
            ticket.confirm = Some(request.clone());
            ticket.cancel = Some(vec![OrderId::new("old")]);
            ticket.open(&request);
            assert!(ticket.confirm.is_none());
            assert!(ticket.cancel.is_none());
            assert_eq!(ticket.take_profit.0, brackets);
            assert_eq!(ticket.stop_loss.0, brackets);
            if brackets {
                assert_eq!(
                    ticket.take_profit.1,
                    Bracket {
                        mode: BracketMode::Price,
                        value: 105.0
                    }
                );
                assert_eq!(
                    ticket.stop_loss.1,
                    Bracket {
                        mode: BracketMode::Price,
                        value: 95.0
                    }
                );
            } else {
                assert_eq!((ticket.price, ticket.limit), (0.0, 0.0));
            }
        }
    }

    #[test]
    fn short_tickets_keep_order_controls_reachable() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for tab in [Tab::Order, Tab::Dom] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut ticket = OrderTicket::default();
                ticket.open(&OrderRequest::new(
                    "ES",
                    OrderSide::Buy,
                    1.0,
                    OrderType::StopLimit {
                        stop: 100.0,
                        limit: 101.0,
                    },
                ));
                let mut rules = TradingRules::new("ES");
                rules.time_in_force.push(TimeInForce::GoodTillCancelled);
                let market = Market {
                    last: Some(100.0),
                    bid: None,
                    ask: None,
                    tick: 0.25,
                    decimals: 2,
                    currency: "USD",
                    balance: None,
                    rules: &rules,
                    rules_ready: true,
                    capabilities: Capabilities {
                        market: true,
                        limit: true,
                        stop: true,
                        stop_limit: true,
                        brackets: true,
                        ..Capabilities::default()
                    },
                    book: None,
                    orders: &[],
                    positions: &[],
                    trades: &[],
                    last_fill: None,
                };
                ticket.tab = tab;
                let mut visible_submit = false;
                for pass in 0..12 {
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(320.0, 240.0),
                            )),
                            events: if pass > 2 {
                                vec![
                                    egui::Event::PointerMoved(egui::pos2(160.0, 130.0)),
                                    egui::Event::MouseWheel {
                                        unit: egui::MouseWheelUnit::Point,
                                        source: egui::MouseWheelSource::Wheel,
                                        phase: egui::TouchPhase::Move,
                                        delta: egui::vec2(0.0, -400.0),
                                        modifiers: Default::default(),
                                    },
                                ]
                            } else {
                                vec![]
                            },
                            ..Default::default()
                        },
                        |ui| {
                            ticket.show(ui, &market, &mut vec![], &mut false);
                        },
                    );
                    for shape in &output.shapes {
                        if let egui::Shape::Text(text) = &shape.shape
                            && (text.galley.text().starts_with("1 ES @")
                                || text.galley.text() == "Buy Mkt")
                        {
                            let rect = text.galley.rect.translate(text.pos.to_vec2());
                            visible_submit |= shape.clip_rect.contains_rect(rect)
                                && ctx.content_rect().contains_rect(rect);
                        }
                    }
                    output.drop_without_applying_deltas();
                }
                assert!(
                    visible_submit,
                    "the submit button must be reachable in a short window"
                );
                for price in [0.0, -1.0] {
                    ticket.open(&OrderRequest::new(
                        "ES",
                        OrderSide::Buy,
                        1.0,
                        OrderType::Limit(price),
                    ));
                    ctx.run_ui(egui::RawInput::default(), |ui| {
                        ticket.show(ui, &market, &mut vec![], &mut false);
                    })
                    .drop_without_applying_deltas();
                    assert_eq!(
                        ticket.price, price,
                        "an invalid entered price must not become the market price"
                    );
                    assert!(ticket.request(&market).is_err());
                }
            }
        }
    }

    /// A pending entry is sized by its own price like a market entry, the risk control is offered
    /// for every order type, and an order from the ladder takes the ticket's brackets from its own
    /// side, entry and quantity and shows them in its confirmation. A regression sizes pending
    /// orders from the market price, hides the control, or sends ladder orders without exits.
    #[test]
    fn pending_entries_are_sized_by_risk_and_ladder_orders_carry_the_brackets() {
        let rules = TradingRules::new("ES");
        let mut capabilities = Capabilities::default();
        (capabilities.limit, capabilities.stop, capabilities.brackets) = (true, true, true);
        let market = Market {
            last: Some(100.0),
            bid: Some(99.75),
            ask: Some(100.25),
            tick: 0.25,
            decimals: 2,
            currency: "USD",
            balance: Some(10_000.0),
            capabilities,
            rules: &rules,
            rules_ready: true,
            book: None,
            orders: &[],
            positions: &[],
            trades: &[],
            last_fill: None,
        };
        let ticks = |value| {
            (
                true,
                Bracket {
                    mode: BracketMode::Ticks,
                    value,
                },
            )
        };
        // 1% of 10,000 = 100 at a stop 8 ticks = 2 points from the entry: 100 / 2 = 50 units.
        let mut ticket = OrderTicket {
            symbol: "ES".into(),
            kind: Kind::Limit,
            price: 99.0,
            sizing: Sizing::Risk,
            amount: 1.0,
            take_profit: ticks(16.0),
            stop_loss: ticks(8.0),
            ..OrderTicket::default()
        };
        let request = ticket.request(&market).unwrap();
        assert_eq!(
            (request.quantity, request.take_profit, request.stop_loss),
            (50.0, Some(103.0), Some(97.0))
        );
        // The control is on the Order tab for the pending kind.
        let ctx = egui::Context::default();
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            ticket.show(ui, &market, &mut vec![], &mut false);
        });
        let risk = output.shapes.iter().any(|shape| {
            matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "Risk, % balance")
        });
        output.drop_without_applying_deltas();
        assert!(risk, "limit orders offer the risk control");
        // A sell stop from the ladder: exits lie the other way from the buy above.
        let order = OrderRequest::new("ES", OrderSide::Sell, 3.0, OrderType::Stop(98.0));
        let order = ticket.with_brackets(order, &market).unwrap();
        assert_eq!(
            (order.take_profit, order.stop_loss),
            (Some(94.0), Some(100.0))
        );
        assert_eq!(
            confirmation(&order, 2, 0),
            "Sell 3 ES @ 98.00 STOP?\nTake profit 94.00\nStop loss 100.00"
        );
        ticket.stop_loss = ticks(0.0);
        let order = OrderRequest::new("ES", OrderSide::Sell, 3.0, OrderType::Stop(98.0));
        assert!(ticket.with_brackets(order, &market).is_err());
    }

    /// Guards the depth ladder's money path: a click in the bid column buys and one in the ask
    /// column sells, with a limit order at or inside the touch and a stop order beyond it, and the
    /// order waits for confirmation unless one-click trading is on. Swapping limit and stop sends
    /// an order that fills at once; skipping the confirmation trades on a stray click.
    #[test]
    fn ladder_clicks_place_limits_inside_the_market_and_stops_beyond_it() {
        let rules = TradingRules::new("ES");
        let mut capabilities = Capabilities::default();
        (capabilities.market, capabilities.limit, capabilities.stop) = (true, true, true);
        let working = [Order::new(
            OrderId::new("bid"),
            OrderRequest::new("ES", OrderSide::Buy, 1.0, OrderType::Limit(99.5)),
            crate::model::OrderStatus::Working,
        )];
        let market = Market {
            last: Some(100.0),
            bid: Some(99.75),
            ask: Some(100.25),
            tick: 0.25,
            decimals: 2,
            currency: "USD",
            balance: None,
            capabilities,
            rules: &rules,
            rules_ready: true,
            book: None,
            orders: &working,
            positions: &[],
            trades: &[],
            last_fill: None,
        };
        let cases = [
            (
                "Bid size",
                "99.50",
                true,
                OrderSide::Buy,
                OrderType::Limit(99.5),
            ),
            (
                "Bid size",
                "100.25",
                true,
                OrderSide::Buy,
                OrderType::Limit(100.25),
            ),
            (
                "Bid size",
                "100.50",
                true,
                OrderSide::Buy,
                OrderType::Stop(100.5),
            ),
            (
                "Ask size",
                "100.50",
                true,
                OrderSide::Sell,
                OrderType::Limit(100.5),
            ),
            (
                "Ask size",
                "99.75",
                true,
                OrderSide::Sell,
                OrderType::Limit(99.75),
            ),
            (
                "Ask size",
                "99.50",
                true,
                OrderSide::Sell,
                OrderType::Stop(99.5),
            ),
            (
                "Bid size",
                "99.50",
                false,
                OrderSide::Buy,
                OrderType::Limit(99.5),
            ),
            ("CXL All", "", true, OrderSide::Buy, OrderType::Market),
        ];
        for (column, row, one_click, side, order_type) in cases {
            let ctx = egui::Context::default();
            let mut ticket = OrderTicket {
                open: true,
                symbol: "ES".into(),
                tab: Tab::Dom,
                ..OrderTicket::default()
            };
            let mut one_click = one_click;
            let mut frame = |events| {
                let mut action = None;
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            vec2(340.0, 900.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| action = ticket.show(ui, &market, &mut vec![], &mut one_click),
                );
                let centre = |label: &str| {
                    output.shapes.iter().find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == label => {
                            Some(text.galley.rect.translate(text.pos.to_vec2()).center())
                        }
                        _ => None,
                    })
                };
                let cell = if row.is_empty() {
                    centre(column)
                } else {
                    centre(column).zip(centre(row)).map(|(x, y)| pos2(x.x, y.y))
                };
                output.drop_without_applying_deltas();
                (action, cell)
            };
            let cell = frame(vec![]).1.expect(row);
            frame(vec![egui::Event::PointerMoved(cell)]);
            let click = |pressed| egui::Event::PointerButton {
                pos: cell,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: Default::default(),
            };
            frame(vec![click(true)]);
            let (action, _) = frame(vec![click(false)]);
            let mut expected = OrderRequest::new("ES", side, 1.0, order_type);
            expected.time_in_force = TimeInForce::Day;
            if row.is_empty() {
                assert_eq!(
                    action, None,
                    "bulk cancellation always waits for confirmation"
                );
                assert_eq!(ticket.cancel, Some(vec![OrderId::new("bid")]));
            } else if one_click {
                assert_eq!(action, Some(Action::Place(expected)), "{column} at {row}");
            } else {
                assert_eq!(action, None, "{column} at {row} waits for confirmation");
                assert_eq!(ticket.confirm, Some(expected));
            }
        }
    }

    /// Guards sizing and brackets against the published definitions: value sizing buys
    /// value / (price * point value) units, risk sizing loses at most the risk at the stop, both
    /// round down to the quantity step, and ticks, percentages and money place a take profit
    /// above a buy's entry and below a sell's (a stop loss the other way). A sign flip sends a
    /// bracket to the wrong side; rounding to nearest risks more than asked. Writing an exit in
    /// another way keeps its price, or switching how it is entered moves it.
    #[test]
    fn tickets_size_and_place_brackets_by_the_rules() {
        let mut rules = TradingRules::new("ES");
        rules.quantity_step = 0.5;
        rules.min_quantity = 0.5;
        rules.max_quantity = Some(10.0);
        rules.point_value = 50.0;
        let mut capabilities = Capabilities::default();
        (
            capabilities.market,
            capabilities.limit,
            capabilities.brackets,
        ) = (true, true, true);
        let market = Market {
            last: Some(4_000.0),
            bid: None,
            ask: None,
            tick: 0.25,
            decimals: 2,
            currency: "USD",
            balance: Some(100_000.0),
            capabilities,
            rules: &rules,
            rules_ready: true,
            book: None,
            orders: &[],
            positions: &[],
            trades: &[],
            last_fill: None,
        };
        let ticks = |value| Bracket {
            mode: BracketMode::Ticks,
            value,
        };
        let percent = |value| Bracket {
            mode: BracketMode::Percent,
            value,
        };
        let money = |value| Bracket {
            mode: BracketMode::Money,
            value,
        };
        // (side, sizing, amount or quantity, take profit, stop loss) -> (quantity, tp, sl)
        let cases = [
            // 1,000,000 / (4,000 * 50) = 5 units.
            (
                OrderSide::Buy,
                Sizing::Value,
                1_000_000.0,
                None,
                None,
                Ok((5.0, None, None)),
            ),
            // 1,100,000 / 200,000 = 5.5, a whole number of 0.5 steps.
            (
                OrderSide::Buy,
                Sizing::Value,
                1_100_000.0,
                None,
                None,
                Ok((5.5, None, None)),
            ),
            // 1,140,000 / 200,000 = 5.7, rounded down to 5.5.
            (
                OrderSide::Sell,
                Sizing::Value,
                1_140_000.0,
                None,
                None,
                Ok((5.5, None, None)),
            ),
            // 1% of 100,000 = 1,000 at a stop 8 ticks = 2 points below: 1,000 / (2 * 50) = 10.
            (
                OrderSide::Buy,
                Sizing::Risk,
                1.0,
                Some(ticks(16.0)),
                Some(ticks(8.0)),
                Ok((10.0, Some(4_004.0), Some(3_998.0))),
            ),
            // A sell's stop lies above: 1% of 4,000 = 40 points, 1,000 / (40 * 50) = 0.5.
            (
                OrderSide::Sell,
                Sizing::Risk,
                1.0,
                Some(percent(2.0)),
                Some(percent(1.0)),
                Ok((0.5, Some(3_920.0), Some(4_040.0))),
            ),
            // 900 / 2,000 = 0.45, below the minimum of 0.5.
            (
                OrderSide::Sell,
                Sizing::Risk,
                0.9,
                None,
                Some(percent(1.0)),
                Err(()),
            ),
            // Stop 4 ticks = 1 point below: 1,500 / 50 = 30 units, above the maximum of 10.
            (
                OrderSide::Buy,
                Sizing::Risk,
                1.5,
                None,
                Some(ticks(4.0)),
                Err(()),
            ),
            (
                OrderSide::Buy,
                Sizing::Units,
                2.0,
                None,
                None,
                Ok((2.0, None, None)),
            ),
            (OrderSide::Buy, Sizing::Risk, 1.0, None, None, Err(())),
            (
                OrderSide::Buy,
                Sizing::Units,
                1.0,
                None,
                Some(ticks(0.0)),
                Err(()),
            ),
            (
                OrderSide::Sell,
                Sizing::Units,
                1.0,
                Some(ticks(0.0)),
                None,
                Err(()),
            ),
            // Percent exits and explicit prices snap before sizing by their risk: 0.025% of
            // 100,000 = 25 at 0.5 points: 25 / (0.5 * 50) = 1.
            (
                OrderSide::Buy,
                Sizing::Risk,
                0.025,
                Some(percent(0.014)),
                Some(Bracket {
                    mode: BracketMode::Price,
                    value: 3999.6,
                }),
                Ok((1.0, Some(4000.5), Some(3999.5))),
            ),
            // 2 units of 50 a point: 500 is 5 points of profit, 250 is 2.5 points of loss.
            (
                OrderSide::Buy,
                Sizing::Units,
                2.0,
                Some(money(500.0)),
                Some(money(250.0)),
                Ok((2.0, Some(4_005.0), Some(3_997.5))),
            ),
            // A stop loss in money needs the quantity that risk sizing takes from it.
            (
                OrderSide::Sell,
                Sizing::Risk,
                1.0,
                None,
                Some(money(250.0)),
                Err(()),
            ),
        ];
        for (side, sizing, amount, take_profit, stop_loss, expected) in cases {
            let ticket = |take_profit: Option<Bracket>, stop_loss: Option<Bracket>| OrderTicket {
                symbol: "ES".into(),
                side,
                sizing,
                quantity: amount,
                amount,
                take_profit: (take_profit.is_some(), take_profit.unwrap_or(ticks(1.0))),
                stop_loss: (stop_loss.is_some(), stop_loss.unwrap_or(ticks(1.0))),
                ..OrderTicket::default()
            };
            let actual = ticket(take_profit, stop_loss)
                .request(&market)
                .map(|r| (r.quantity, r.take_profit, r.stop_loss))
                .map_err(|_| ());
            assert_eq!(actual, expected, "{side:?} {sizing:?} {amount}");
            let Ok((quantity, Some(take_profit), Some(stop_loss))) = expected else {
                continue;
            };
            if sizing == Sizing::Risk {
                continue;
            }
            for mode in BracketMode::ALL {
                let at = |price, profit| {
                    let per_point = quantity * rules.point_value;
                    Bracket::at(mode, price, side, 4_000.0, 0.25, per_point, profit)
                };
                let actual = ticket(Some(at(take_profit, true)), Some(at(stop_loss, false)))
                    .request(&market)
                    .map(|r| (r.quantity, r.take_profit, r.stop_loss));
                assert_eq!(actual, Ok(expected.unwrap()), "{side:?} {mode:?}");
            }
        }
        // A quoted market order enters a buy at the ask and a sell at the bid: 4 ticks = 1 point.
        let quoted = Market {
            bid: Some(3_999.75),
            ask: Some(4_000.25),
            ..market
        };
        for (side, take_profit) in [(OrderSide::Buy, 4_001.25), (OrderSide::Sell, 3_998.75)] {
            let ticket = OrderTicket {
                symbol: "ES".into(),
                side,
                take_profit: (true, ticks(4.0)),
                ..OrderTicket::default()
            };
            let request = ticket.request(&quoted).unwrap();
            assert_eq!(request.take_profit, Some(take_profit), "{side:?}");
        }
    }
}
