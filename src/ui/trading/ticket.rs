//! The order ticket: one order, sized by the symbol's trading rules, with optional brackets.

use egui::{Color32, RichText, Ui};
use serde::{Deserialize, Serialize};

use crate::model::{Capabilities, OrderRequest, OrderSide, OrderType, TimeInForce, TradingRules};
use crate::theme::ThemeExt;
use crate::ui_kit::form::FormGrid;

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
}

/// A take-profit or stop-loss as entered in the ticket.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bracket {
    /// How [`value`](Self::value) is read.
    pub mode: BracketMode,
    /// A price, a number of ticks or a percentage.
    pub value: f64,
}

impl Bracket {
    /// The exit price for an order on `side` entering at `entry`: a take profit (`profit`) lies
    /// above the entry of a buy and below that of a sell, a stop loss the other way.
    pub(crate) fn price(&self, side: OrderSide, entry: f64, tick: f64, profit: bool) -> f64 {
        let sign = if (side == OrderSide::Buy) == profit {
            1.0
        } else {
            -1.0
        };
        match self.mode {
            BracketMode::Price => self.value,
            BracketMode::Ticks => entry + sign * self.value * tick,
            BracketMode::Percent => entry * (1.0 + sign * self.value / 100.0),
        }
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
    /// The loss in the account's currency if the stop loss fills.
    Risk,
}

/// What the ticket needs from the chart and the account for one frame.
pub(crate) struct Market<'a> {
    /// The last price, the entry of a market order.
    pub last: Option<f64>,
    /// The symbol's tick size.
    pub tick: f64,
    /// Decimals of prices.
    pub decimals: usize,
    /// The account's currency.
    pub currency: &'a str,
    pub capabilities: Capabilities,
    pub rules: &'a TradingRules,
    pub rules_ready: bool,
}

/// The state of the order ticket window.
pub(crate) struct OrderTicket {
    pub open: bool,
    pub symbol: String,
    side: OrderSide,
    kind: Kind,
    /// The limit or stop price, and the stop price of a stop-limit order.
    price: f64,
    /// The limit price of a stop-limit order.
    limit: f64,
    sizing: Sizing,
    quantity: f64,
    /// The value or risk entered for [`Sizing::Value`] and [`Sizing::Risk`].
    amount: f64,
    take_profit: (bool, Bracket),
    stop_loss: (bool, Bracket),
    time_in_force: TimeInForce,
    preset_name: String,
}

impl Default for OrderTicket {
    fn default() -> Self {
        let bracket = Bracket {
            mode: BracketMode::Ticks,
            value: 100.0,
        };
        Self {
            open: false,
            symbol: String::new(),
            side: OrderSide::Buy,
            kind: Kind::Market,
            price: 0.0,
            limit: 0.0,
            sizing: Sizing::Units,
            quantity: 1.0,
            amount: 1_000.0,
            take_profit: (false, bracket),
            stop_loss: (false, bracket),
            time_in_force: TimeInForce::Day,
            preset_name: String::new(),
        }
    }
}

impl OrderTicket {
    /// Fills the ticket from `request` and opens it.
    pub fn open(&mut self, request: &OrderRequest) {
        self.open = true;
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

    /// The price the order enters at: its limit price, its stop price, or the last price.
    fn entry(&self, last: Option<f64>) -> Option<f64> {
        match self.kind {
            Kind::Market => last,
            Kind::Limit | Kind::Stop => Some(self.price),
            Kind::StopLimit => Some(self.limit),
        }
        .filter(|price| price.is_finite() && *price > 0.0)
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
            .entry(market.last)
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
        let brackets = market.capabilities.brackets;
        let exit = |(enabled, bracket): (bool, Bracket), profit| {
            (brackets && enabled)
                .then(|| snap(bracket.price(self.side, entry, market.tick, profit)))
        };
        let take_profit = exit(self.take_profit, true);
        let stop_loss = exit(self.stop_loss, false);
        // A buy exits above its entry for a profit and below it for a loss; a sell the other way.
        let above = |price: f64| (price > entry) == (self.side == OrderSide::Buy);
        if take_profit.is_some_and(|price| {
            !price.is_finite() || price <= 0.0 || price == entry || !above(price)
        }) {
            return Err("The take profit must lie on the profit side of the entry".into());
        }
        if stop_loss.is_some_and(|price| {
            !price.is_finite() || price <= 0.0 || price == entry || above(price)
        }) {
            return Err("The stop loss must lie on the loss side of the entry".into());
        }

        let rules = market.rules;
        let quantity = match self.sizing {
            Sizing::Units => self.quantity,
            Sizing::Value => self.amount / (entry * rules.point_value),
            Sizing::Risk => {
                let stop = stop_loss.ok_or("Sizing by risk needs a stop loss")?;
                self.amount / ((entry - stop).abs() * rules.point_value)
            }
        };
        let quantity = order_quantity(quantity, rules)?;
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

    /// Draws the ticket, first at `anchor`, its top right corner. Returns the order once the user
    /// submits it.
    pub fn show(
        &mut self,
        ctx: &egui::Context,
        anchor: egui::Pos2,
        market: &Market<'_>,
        presets: &mut Vec<OrderPreset>,
        one_click: &mut bool,
    ) -> Option<OrderRequest> {
        let mut open = self.open;
        let mut submitted = None;
        egui::Window::new(format!("Order {}", self.symbol))
            .id(egui::Id::unique("order_ticket"))
            .collapsible(false)
            .resizable(false)
            .default_width(280.0)
            .pivot(egui::Align2::RIGHT_TOP)
            .default_pos(anchor)
            .open(&mut open)
            .show(ctx, |ui| {
                submitted = egui::ScrollArea::vertical()
                    .max_height((ctx.content_rect().height() - 80.0).max(0.0))
                    .show(ui, |ui| self.form(ui, market, presets, one_click))
                    .inner;
            });
        self.open = open && submitted.is_none();
        submitted
    }

    fn form(
        &mut self,
        ui: &mut Ui,
        market: &Market<'_>,
        presets: &mut Vec<OrderPreset>,
        one_click: &mut bool,
    ) -> Option<OrderRequest> {
        if !market.rules.time_in_force.contains(&self.time_in_force) {
            self.time_in_force = market.rules.time_in_force[0];
        }
        let theme = ui.ctx().active_theme();
        let side_color = |side| match side {
            OrderSide::Buy => theme.ui.accent,
            OrderSide::Sell => theme.ui.error,
        };
        ui.columns(2, |columns| {
            let sides = [(OrderSide::Sell, "Sell"), (OrderSide::Buy, "Buy")];
            for (ui, (side, label)) in columns.iter_mut().zip(sides) {
                let selected = self.side == side;
                let text = RichText::new(label).strong().color(if selected {
                    Color32::WHITE
                } else {
                    side_color(side)
                });
                let button = egui::Button::new(text)
                    .fill(if selected {
                        side_color(side)
                    } else {
                        Color32::TRANSPARENT
                    })
                    .stroke((1.0, side_color(side)));
                if ui.add_sized([ui.available_width(), 28.0], button).clicked() {
                    self.side = side;
                }
            }
        });
        ui.horizontal_wrapped(|ui| {
            for (kind, label) in Kind::ALL {
                if kind.accepted(&market.capabilities) {
                    ui.selectable_value(&mut self.kind, kind, label);
                }
            }
        });
        if self.kind != Kind::Market && self.price <= 0.0 {
            // A ticket opened for a market order gets the last price as a starting point.
            self.price = market.last.unwrap_or_default();
            self.limit = self.price;
        }
        let price = |value| {
            egui::DragValue::new(value)
                .speed(market.tick)
                .range(0.0..=f64::MAX)
                .fixed_decimals(market.decimals)
        };
        let rules = market.rules;
        let step_decimals = decimals(rules.quantity_step);
        FormGrid::new("order_ticket_form").show(ui, |ui| {
            match self.kind {
                Kind::Market => {}
                Kind::Limit => {
                    ui.label("Limit price");
                    ui.add(price(&mut self.price));
                    ui.end_row();
                }
                Kind::Stop | Kind::StopLimit => {
                    ui.label("Stop price");
                    ui.add(price(&mut self.price));
                    ui.end_row();
                }
            }
            if self.kind == Kind::StopLimit {
                ui.label("Limit price");
                ui.add(price(&mut self.limit));
                ui.end_row();
            }
            egui::ComboBox::from_id_salt("order_ticket_sizing")
                .selected_text(match self.sizing {
                    Sizing::Units => "Units",
                    Sizing::Value => "Value",
                    Sizing::Risk => "Risk",
                })
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.sizing, Sizing::Units, "Units");
                    ui.selectable_value(&mut self.sizing, Sizing::Value, "Value");
                    ui.selectable_value(&mut self.sizing, Sizing::Risk, "Risk");
                });
            if self.sizing == Sizing::Units {
                ui.add(
                    egui::DragValue::new(&mut self.quantity)
                        .speed(rules.quantity_step)
                        .range(rules.min_quantity..=rules.max_quantity.unwrap_or(f64::MAX))
                        .fixed_decimals(step_decimals),
                );
            } else {
                ui.add(
                    egui::DragValue::new(&mut self.amount)
                        .speed(1.0)
                        .range(0.0..=f64::MAX)
                        .suffix(format!(" {}", market.currency)),
                );
            }
            ui.end_row();
            if market.capabilities.brackets {
                for (label, bracket) in [
                    ("Take profit", &mut self.take_profit),
                    ("Stop loss", &mut self.stop_loss),
                ] {
                    ui.checkbox(&mut bracket.0, label);
                    ui.add_enabled_ui(bracket.0, |ui| {
                        ui.horizontal(|ui| {
                            egui::ComboBox::from_id_salt(label)
                                .width(64.0)
                                .selected_text(match bracket.1.mode {
                                    BracketMode::Price => "Price",
                                    BracketMode::Ticks => "Ticks",
                                    BracketMode::Percent => "%",
                                })
                                .show_ui(ui, |ui| {
                                    let mode = &mut bracket.1.mode;
                                    ui.selectable_value(mode, BracketMode::Price, "Price");
                                    ui.selectable_value(mode, BracketMode::Ticks, "Ticks");
                                    ui.selectable_value(mode, BracketMode::Percent, "%");
                                });
                            let (speed, places) = match bracket.1.mode {
                                BracketMode::Price => (market.tick, market.decimals),
                                BracketMode::Ticks => (1.0, 0),
                                BracketMode::Percent => (0.1, 2),
                            };
                            ui.add(
                                egui::DragValue::new(&mut bracket.1.value)
                                    .speed(speed)
                                    .range(0.0..=f64::MAX)
                                    .fixed_decimals(places),
                            );
                        });
                    });
                    ui.end_row();
                }
            }
            if rules.time_in_force.len() > 1 {
                ui.label("Time in force");
                egui::ComboBox::from_id_salt("order_ticket_tif")
                    .selected_text(self.time_in_force.label())
                    .show_ui(ui, |ui| {
                        for tif in &rules.time_in_force {
                            ui.selectable_value(&mut self.time_in_force, *tif, tif.label());
                        }
                    });
                ui.end_row();
            }
        });

        let request = self.request(market);
        let entry = self.entry(market.last);
        if let (Ok(request), Some(entry)) = (&request, entry) {
            let money = |value: f64| format!("{value:.2} {}", market.currency);
            let per_price = request.quantity * rules.point_value;
            ui.label(format!("Value  {}", money(entry * per_price)));
            if let Some(exit) = request.take_profit {
                ui.colored_label(
                    theme.ui.success,
                    format!("Reward  {}", money((exit - entry).abs() * per_price)),
                );
            }
            if let Some(exit) = request.stop_loss {
                ui.colored_label(
                    theme.ui.error,
                    format!("Risk  {}", money((exit - entry).abs() * per_price)),
                );
            }
        }

        ui.separator();
        ui.horizontal(|ui| {
            egui::ComboBox::from_id_salt("order_ticket_presets")
                .selected_text("Presets")
                .show_ui(ui, |ui| {
                    if presets.is_empty() {
                        ui.weak("No presets saved");
                    }
                    let mut remove = None;
                    for (index, preset) in presets.iter().enumerate() {
                        ui.horizontal(|ui| {
                            if ui.selectable_label(false, &preset.name).clicked() {
                                self.apply(preset);
                            }
                            if ui
                                .small_button("✕")
                                .on_hover_text("Delete preset")
                                .clicked()
                            {
                                remove = Some(index);
                            }
                        });
                    }
                    if let Some(index) = remove {
                        presets.remove(index);
                    }
                });
            ui.add(
                egui::TextEdit::singleline(&mut self.preset_name)
                    .hint_text("Preset name")
                    .desired_width(100.0),
            );
            let name = self.preset_name.trim();
            if ui
                .add_enabled(!name.is_empty(), egui::Button::new("Save"))
                .clicked()
            {
                let preset = self.preset(name.to_owned(), market);
                presets.retain(|existing| existing.name != preset.name);
                presets.push(preset);
                self.preset_name.clear();
            }
        });
        ui.checkbox(one_click, "One-click trading")
            .on_hover_text("Buy and Sell place a market order at once, without this ticket");

        let mut submitted = None;
        match request {
            Ok(request) => {
                let text = format!(
                    "{} {} {} {}",
                    match request.side {
                        OrderSide::Buy => "Buy",
                        OrderSide::Sell => "Sell",
                    },
                    request.quantity.to_string(),
                    request.symbol,
                    match request.order_type {
                        OrderType::Market => "MKT".to_owned(),
                        OrderType::Limit(price) => format!("@ {price:.*} LMT", market.decimals),
                        OrderType::Stop(price) => format!("@ {price:.*} STP", market.decimals),
                        OrderType::StopLimit { stop, .. } =>
                            format!("@ {stop:.*} STP LMT", market.decimals),
                    }
                );
                let button = egui::Button::new(RichText::new(text).strong().color(Color32::WHITE))
                    .fill(side_color(self.side));
                if ui.add_sized([ui.available_width(), 32.0], button).clicked() {
                    submitted = Some(request);
                }
            }
            Err(reason) => {
                ui.colored_label(theme.ui.error, reason);
            }
        }
        submitted
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

/// `value` as the whole multiple of `step` that `round` picks from `value / step`, written at the
/// step's decimals.
pub(crate) fn to_step(value: f64, step: f64, round: impl Fn(f64) -> f64) -> f64 {
    let scale = 10f64.powi(decimals(step) as i32);
    (round(value / step) * step * scale).round() / scale
}

/// Decimals needed to write `step`, such as 2 for 0.25.
fn decimals(step: f64) -> usize {
    let text = format!("{step}");
    text.split_once('.')
        .map_or(0, |(_, fraction)| fraction.len())
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
            ticket.open(&request);
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
    fn short_tickets_scroll_to_the_submit_button() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
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
                tick: 0.25,
                decimals: 2,
                currency: "USD",
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
            };
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
                        ticket.show(
                            ui.ctx(),
                            egui::pos2(310.0, 8.0),
                            &market,
                            &mut vec![],
                            &mut false,
                        );
                    },
                );
                for shape in &output.shapes {
                    if let egui::Shape::Text(text) = &shape.shape
                        && text.galley.text().starts_with("Buy 1 ES @")
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
        }
    }

    /// Guards sizing and brackets against the published definitions: value sizing buys
    /// value / (price * point value) units, risk sizing loses at most the risk at the stop, both
    /// round down to the quantity step, and ticks and percentages place a take profit above a
    /// buy's entry and below a sell's (a stop loss the other way). A sign flip sends a bracket to
    /// the wrong side; rounding to nearest risks more than asked.
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
            tick: 0.25,
            decimals: 2,
            currency: "USD",
            capabilities,
            rules: &rules,
            rules_ready: true,
        };
        let ticks = |value| Bracket {
            mode: BracketMode::Ticks,
            value,
        };
        let percent = |value| Bracket {
            mode: BracketMode::Percent,
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
            // Stop 8 ticks = 2 points below: 1,000 / (2 * 50) = 10 units.
            (
                OrderSide::Buy,
                Sizing::Risk,
                1_000.0,
                Some(ticks(16.0)),
                Some(ticks(8.0)),
                Ok((10.0, Some(4_004.0), Some(3_998.0))),
            ),
            // A sell's stop lies above: 1% of 4,000 = 40 points, 1,000 / (40 * 50) = 0.5.
            (
                OrderSide::Sell,
                Sizing::Risk,
                1_000.0,
                Some(percent(2.0)),
                Some(percent(1.0)),
                Ok((0.5, Some(3_920.0), Some(4_040.0))),
            ),
            // 900 / 2,000 = 0.45, below the minimum of 0.5.
            (
                OrderSide::Sell,
                Sizing::Risk,
                900.0,
                None,
                Some(percent(1.0)),
                Err(()),
            ),
            // Stop 4 ticks = 1 point below: 1,500 / 50 = 30 units, above the maximum of 10.
            (
                OrderSide::Buy,
                Sizing::Risk,
                1_500.0,
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
            (OrderSide::Buy, Sizing::Risk, 1_000.0, None, None, Err(())),
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
            // Percent exits and explicit prices snap before sizing by their risk.
            (
                OrderSide::Buy,
                Sizing::Risk,
                25.0,
                Some(percent(0.014)),
                Some(Bracket {
                    mode: BracketMode::Price,
                    value: 3999.6,
                }),
                Ok((1.0, Some(4000.5), Some(3999.5))),
            ),
        ];
        for (side, sizing, amount, take_profit, stop_loss, expected) in cases {
            let ticket = OrderTicket {
                symbol: "ES".into(),
                side,
                sizing,
                quantity: amount,
                amount,
                take_profit: (take_profit.is_some(), take_profit.unwrap_or(ticks(1.0))),
                stop_loss: (stop_loss.is_some(), stop_loss.unwrap_or(ticks(1.0))),
                ..OrderTicket::default()
            };
            let actual = ticket
                .request(&market)
                .map(|r| (r.quantity, r.take_profit, r.stop_loss))
                .map_err(|_| ());
            assert_eq!(actual, expected, "{side:?} {sizing:?} {amount}");
        }
    }
}
