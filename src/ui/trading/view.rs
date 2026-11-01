//! The trading controller's chart view: the order ticket's panel, the orders, positions and fills
//! drawn on the chart, the image exports, and the confirmations of what the user changes there.

use super::format::Numbers;
use super::ticket::{self, Action, Market};
use super::{Preview, Trading, executions};
use crate::chart::selection::SeriesId;
use crate::chart::{TradingLineAction, TradingLineOptions, render_trading_lines, to_step};
use crate::localization as l10n;
use crate::model::{
    Order, OrderId, OrderRequest, OrderSide, OrderStatus, OrderType, Position, TradingRules,
};
use crate::theme::ThemeExt;
use crate::ui_kit::modal::dialogs::ConfirmDialog;
use crate::widget::Chart;

impl Trading {
    /// What the order ticket needs from `chart` and the account this frame. Order tiers take
    /// `colors` unless the chart's book view sets its own.
    fn market<'a>(&'a self, chart: &'a Chart, colors: &crate::theme::ChartColors) -> Market<'a> {
        let symbol = chart.symbol();
        let instrument = chart
            .instrument()
            .filter(|instrument| instrument.symbol == symbol)
            .or_else(|| self.instruments.get(symbol));
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
        let view = chart.order_book_view();
        let queue = chart.order_queue(SeriesId::MAIN).filter(|_| !replay);
        let filtered = view.filter != crate::data::OrderFilter::default();
        let ladder = match (queue, filtered) {
            (Some(queue), true) => queue
                .filtered(symbol, &view.filter)
                .map(|b| Some(std::borrow::Cow::Owned(b))),
            (None, true) if !replay => Err(crate::data::OrderFilterUnavailable::AggregateDepth),
            _ => Ok(book.map(std::borrow::Cow::Borrowed)),
        };
        let highlights = queue
            .zip(view.tiers)
            .map(|(queue, tiers)| {
                (queue.largest_orders(&view.filter).into_iter())
                    .filter_map(|o| {
                        let tier = tiers.tier(o.size)?;
                        Some((
                            o.side == OrderSide::Buy,
                            o.price,
                            view.tier_color(tier, colors),
                        ))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Market {
            symbol,
            formatter: self.formatter.as_deref(),
            pip_value: self.rules.pip_value.or_else(|| {
                let pip = instrument.and_then(|instrument| instrument.pip_size);
                pip.filter(|pip| pip.is_finite() && *pip > 0.0)
                    .map(|pip| pip * self.rules.point_value)
            }),
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
            offline: self.offline(),
            ladder_unavailable: ladder.as_ref().err().copied(),
            ladder: ladder.unwrap_or(None),
            highlights,
            orders: &self.orders,
            positions: &self.positions,
            trades: &[],
            session_open: None,
            last_fill: self.account_panel.fills.price(symbol),
            logo: instrument.and_then(|instrument| instrument.logo.as_deref()),
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
        let theme = ui.active_theme();
        let mut market = self.market(chart, &theme.chart);
        market.last = trades.last().map(|trade| trade.price).or(market.last);
        market.trades = &trades;
        market.session_open = trades
            .last()
            .and_then(|trade| session_open(chart, trade.ts));
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
        self.perform(action);
    }

    /// Sends what the ticket asked for.
    fn perform(&mut self, action: Option<Action>) {
        match action {
            Some(Action::Place(request)) => self.place(request),
            Some(Action::Preview(request)) => {
                if let Some(service) = self
                    .service
                    .as_ref()
                    .filter(|_| self.capabilities().preview)
                {
                    self.request_key += 2;
                    service.preview(request.clone(), self.inbox.reply(self.request_key));
                    self.preview = Some(Preview {
                        key: self.request_key,
                        request,
                        estimate: None,
                    });
                }
            }
            Some(Action::Cancel(ids)) => {
                if let Some(service) = self.live_service() {
                    for id in ids {
                        service.cancel(id, self.inbox.reply(0));
                    }
                }
            }
            Some(Action::Confirm(change)) => self.pending = Some(*change),
            None => {}
        }
    }

    fn draw_overlays(&self, ui: &mut egui::Ui, chart: &Chart) -> crate::chart::TradingLines {
        let symbol = chart.symbol();
        let market = self.market(chart, &ui.active_theme().chart);
        let (decimals, tick, last) = (market.decimals, market.tick, market.last);
        let capabilities = self.capabilities();
        let mapping = chart.get_chart_mapping();
        let display = &self.display;
        let options = TradingLineOptions {
            show_orders: display.show_orders,
            extend_lines: display.extend_lines,
            label_alignment: display.label_alignment,
            show_positions: display.show_positions,
            submit: false,
            modify: capabilities.modify,
            close: capabilities.close,
            reverse: display.reverse_button && capabilities.reverse,
            take_profit: capabilities.limit,
            stop_loss: capabilities.stop,
            order_exits: capabilities.brackets && capabilities.modify,
            pnl: display.position_pnl.then_some(display.position_pnl_unit),
            bracket_pnl: display.bracket_pnl.then_some(display.bracket_pnl_unit),
            last_price: last,
            point_value: if self.rules.symbol == symbol {
                self.rules.point_value
            } else {
                1.0
            },
            currency: self
                .account
                .as_ref()
                .map_or("", |account| account.currency.as_str()),
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
        {
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
                // `Trading::numbers` takes the ticket's decimals; prices here keep the market's.
                let numbers = Numbers {
                    symbol,
                    decimals,
                    formatter: self.formatter.as_deref(),
                };
                executions::show(ui, chart, &fills, display.show_execution_labels, numbers);
            }
            render_trading_lines(
                ui,
                &mapping,
                shown_orders.iter().filter(|o| o.request.symbol == symbol),
                self.positions.iter().filter(|p| p.symbol == symbol),
                &options,
            )
        }
    }

    /// Exports the chart with its visible trading overlays when
    /// [`TradingSettings::include_in_image`](crate::widget::settings::TradingSettings::include_in_image) is enabled. Account data and display settings
    /// stay unchanged; the host delivers the returned frame.
    pub fn export_image(
        &self,
        chart: &Chart,
        ctx: &egui::Context,
        options: &crate::widget::ImageExportOptions,
    ) -> Result<crate::widget::ChartImage, String> {
        chart.export_layout_image(ctx, options, |ui, chart| self.draw_export(ui, chart))
    }

    /// Encodes [`Self::export_image`] as PNG bytes for host file, clipboard or sharing delivery.
    pub fn export_png(
        &self,
        chart: &Chart,
        ctx: &egui::Context,
        options: &crate::widget::ImageExportOptions,
    ) -> Result<Vec<u8>, String> {
        chart.export_layout_png(ctx, options, |ui, chart| self.draw_export(ui, chart))
    }

    /// Writes [`Self::export_image`] as SVG, as [`Chart::export_svg`] does.
    pub fn export_svg(
        &self,
        chart: &Chart,
        ctx: &egui::Context,
        options: &crate::widget::ImageExportOptions,
    ) -> Result<String, String> {
        chart.export_layout_svg(ctx, options, |ui, chart| self.draw_export(ui, chart))
    }

    fn draw_export(&self, ui: &mut egui::Ui, chart: &mut Chart) {
        chart.show(ui);
        if self.display.include_in_image {
            chart.trading_bid_ask = self.display.bid_ask_labels;
            let lines = self.draw_overlays(ui, chart);
            chart.trading_tags = lines.tags;
            chart.trading_outlined = lines.outlined;
        }
    }

    /// Draws the chart symbol's working orders, brackets, positions and execution marks,
    /// respecting [`display`](Self::display). Hovering a mark lists the fills of its bar.
    /// Asks the user to confirm a change made on a line or in the depth ladder before sending it.
    pub fn show(&mut self, ui: &mut egui::Ui, chart: &mut Chart) {
        chart.trading_bid_ask = self.display.bid_ask_labels;
        let colors = ui.active_theme().chart;
        let symbol = chart.symbol().to_owned();
        let symbol = symbol.as_str();
        let market = self.market(chart, &colors);
        let (decimals, tick, last) = (market.decimals, market.tick, market.last);
        let logo = market.logo.map(str::to_owned);
        self.chart_logo = Some((symbol.to_owned(), logo));
        self.decimals = decimals;
        if self.account_panel.editing.is_none() {
            self.load_rules(symbol);
        }

        if self.service.is_some() {
            let lines = self.draw_overlays(ui, chart);
            chart.trading_tags = lines.tags;
            chart.trading_outlined = lines.outlined;
            if self.display.instant_orders {
                self.execute(lines.action);
            } else {
                self.pending = self.pending.take().or(lines.action);
            }
        }
        if let Some(action) = &self.pending {
            let numbers = self.numbers(symbol, symbol);
            let money = |value: f64| numbers.price(value);
            let (title, message, confirm) = describe(ui.ctx(), action, &money);
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

        if let Some(Preview {
            request,
            estimate: Some(estimate),
            ..
        }) = &self.preview
        {
            let mut open = true;
            let result = ConfirmDialog::new(
                "order_preview",
                "Order preview",
                ticket::preview_message(
                    request,
                    estimate,
                    &self.rules,
                    &self.numbers(&request.symbol, symbol),
                    self.account
                        .as_ref()
                        .map_or("", |account| &account.currency),
                ),
            )
            .confirm_text("Place order")
            .cancel_text("Back")
            .show(ui.ctx(), &mut open);
            if result.is_confirmed() {
                if let Some(preview) = self.preview.take() {
                    self.place(preview.request);
                }
            } else if result.is_cancelled() || !open {
                self.preview = None;
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
        let market = self.market(chart, &colors);
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
                extend_lines: self.display.extend_lines,
                label_alignment: self.display.label_alignment,
                show_positions: false,
                submit: true,
                modify: true,
                close: false,
                reverse: false,
                take_profit: false,
                stop_loss: false,
                order_exits: self.capabilities().brackets,
                pnl: None,
                bracket_pnl: self
                    .display
                    .bracket_pnl
                    .then_some(self.display.bracket_pnl_unit),
                last_price: last,
                point_value: self.rules.point_value,
                currency: self
                    .account
                    .as_ref()
                    .map_or("", |account| account.currency.as_str()),
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

    /// Sends a confirmed change to the broker, or reports it as refused while the broker is not
    /// live. Closing and reversing a position are market orders on the other side, for the
    /// position's size and for twice it.
    pub(super) fn execute(&mut self, action: Option<TradingLineAction>) {
        // With instant orders `show` passes the lines' action of every frame, mostly none.
        if action.is_none() {
            return;
        }
        let Some(service) = self.live_service() else {
            return;
        };
        let reverse = matches!(action, Some(TradingLineAction::Reverse(_)));
        let (position, mut order) = match action {
            Some(TradingLineAction::Cancel(order)) => {
                return service.cancel(order.id, self.inbox.reply(0));
            }
            Some(TradingLineAction::Modify(order, request)) => {
                return service.modify(order.id, request, self.inbox.reply(0));
            }
            Some(TradingLineAction::Exit(position, exit)) => (position, exit),
            Some(TradingLineAction::Close(position) | TradingLineAction::Reverse(position))
                if position.quantity != 0.0
                    && if reverse {
                        self.capabilities().reverse
                    } else {
                        self.capabilities().close
                    } =>
            {
                let side = if position.quantity > 0.0 {
                    OrderSide::Sell
                } else {
                    OrderSide::Buy
                };
                let quantity = position.quantity.abs() * if reverse { 2.0 } else { 1.0 };
                let order = OrderRequest::new(&position.symbol, side, quantity, OrderType::Market);
                (position, order)
            }
            _ => return,
        };
        if self.rules_ready && self.rules.symbol == order.symbol {
            order.time_in_force = self.rules.time_in_force[0];
            self.place(order);
        } else {
            self.request_key += 2;
            service.rules(&order.symbol, self.inbox.reply(self.request_key));
            self.placing.push((self.request_key, position, order));
        }
    }
}

/// The open of the exchange session containing `time`, by the chart's trading hours and zone.
fn session_open(
    chart: &Chart,
    time: chrono::DateTime<chrono::Utc>,
) -> Option<chrono::DateTime<chrono::Utc>> {
    let sessions = (chart.bar_clock().sessions()).with_timeframe(crate::model::Timeframe::Min1);
    (sessions.opens([time]).into_iter().rev())
        .find(|(open, _)| *open <= time)
        .map(|(open, _)| open)
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

/// The title, question and confirm button text of the confirmation for `action`.
fn describe(
    ctx: &egui::Context,
    action: &TradingLineAction,
    price: &dyn Fn(f64) -> String,
) -> (&'static str, String, &'static str) {
    let order = |request: &OrderRequest| {
        let quantity = request.quantity.to_string();
        let values = [
            ("quantity", quantity.as_str()),
            ("symbol", request.symbol.as_str()),
        ];
        let key = match (request.side, request.order_type) {
            (OrderSide::Buy, OrderType::Limit(_)) => "BUY {quantity} {symbol} limit {price}",
            (OrderSide::Sell, OrderType::Limit(_)) => "SELL {quantity} {symbol} limit {price}",
            (OrderSide::Buy, OrderType::Stop(_)) => "BUY {quantity} {symbol} stop {price}",
            (OrderSide::Sell, OrderType::Stop(_)) => "SELL {quantity} {symbol} stop {price}",
            (OrderSide::Buy, OrderType::StopLimit { .. }) => {
                "BUY {quantity} {symbol} stop {stop} limit {limit}"
            }
            (OrderSide::Sell, OrderType::StopLimit { .. }) => {
                "SELL {quantity} {symbol} stop {stop} limit {limit}"
            }
            (OrderSide::Buy, _) => "BUY {quantity} {symbol} at market",
            (OrderSide::Sell, _) => "SELL {quantity} {symbol} at market",
        };
        let mut text = match request.order_type {
            OrderType::Limit(limit) | OrderType::Stop(limit) => l10n::format_text(
                ctx,
                key,
                &[values[0], values[1], ("price", price(limit).as_str())],
            ),
            OrderType::StopLimit { stop, limit } => l10n::format_text(
                ctx,
                key,
                &[
                    values[0],
                    values[1],
                    ("stop", price(stop).as_str()),
                    ("limit", price(limit).as_str()),
                ],
            ),
            _ => l10n::format_text(ctx, key, &values),
        };
        if let Some(take_profit) = request.take_profit {
            text.push_str(&l10n::format_text(
                ctx,
                ", take profit {price}",
                &[("price", price(take_profit).as_str())],
            ));
        }
        for level in &request.take_profit_levels {
            text.push_str(&l10n::format_text(
                ctx,
                ", take profit {price} for {quantity}",
                &[
                    ("price", price(level.price).as_str()),
                    ("quantity", level.quantity.to_string().as_str()),
                ],
            ));
        }
        if let Some(stop_loss) = request.stop_loss {
            text.push_str(&l10n::format_text(
                ctx,
                ", stop loss {price}",
                &[("price", price(stop_loss).as_str())],
            ));
        }
        text
    };
    let position = |position: &Position| {
        l10n::format_text(
            ctx,
            if position.quantity > 0.0 {
                "the long position of {quantity} {symbol}"
            } else {
                "the short position of {quantity} {symbol}"
            },
            &[
                ("quantity", position.quantity.abs().to_string().as_str()),
                ("symbol", position.symbol.as_str()),
            ],
        )
    };
    match action {
        TradingLineAction::Cancel(working) => (
            "Cancel order",
            l10n::format_text(
                ctx,
                "Cancel the order to {order}?",
                &[("order", order(&working.request).as_str())],
            ),
            "Cancel order",
        ),
        TradingLineAction::Modify(_, changed) => (
            "Modify order",
            l10n::format_text(
                ctx,
                "Change the order to {order}?",
                &[("order", order(changed).as_str())],
            ),
            "Modify order",
        ),
        TradingLineAction::Close(open) => (
            "Close position",
            l10n::format_text(
                ctx,
                "Close {position} at market?",
                &[("position", position(open).as_str())],
            ),
            "Close position",
        ),
        TradingLineAction::Reverse(open) => (
            "Reverse position",
            l10n::format_text(
                ctx,
                "Reverse {position} at market?",
                &[("position", position(open).as_str())],
            ),
            "Reverse position",
        ),
        TradingLineAction::Exit(open, exit) => {
            let (title, key) = match exit.order_type {
                OrderType::Limit(_) => (
                    "Place take profit",
                    "Protect {position} with a take profit: {order}?",
                ),
                _ => (
                    "Place stop loss",
                    "Protect {position} with a stop loss: {order}?",
                ),
            };
            (
                title,
                l10n::format_text(
                    ctx,
                    key,
                    &[
                        ("position", position(open).as_str()),
                        ("order", order(exit).as_str()),
                    ],
                ),
                title,
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::FeedError;
    use crate::model::{Account, Capabilities, Instrument, OrderPreview, TakeProfitLevel};
    use crate::services::AccountEvent;
    use crate::ui::trading::TradingFormatter;
    use crate::ui::trading::tests::Recorder;
    use crate::ui::trading::widgets::side_name;
    use crate::ui_kit::notifications::Toasts;
    use crate::widget::settings::TradingSettings;
    use std::sync::Arc;

    /// Confirmation sentences and their side and order descriptions use template keys.
    #[test]
    fn confirmations_reach_the_translator() {
        let ctx = egui::Context::default();
        l10n::mark_keys(&ctx);
        let request = |side, kind| OrderRequest::new("1301", side, 2.0, kind);
        let mut bracketed = request(OrderSide::Buy, OrderType::Limit(101.0));
        bracketed.take_profit = Some(110.0);
        bracketed.stop_loss = Some(95.0);
        let working = Order::new(OrderId::new("7"), bracketed, OrderStatus::Working);
        let long = Position::new("1301", 2.0, 100.0);
        for (action, expected) in [
            (
                TradingLineAction::Cancel(working.clone()),
                "~~2~1301~101.00~~110.00~~95.00~~",
            ),
            (
                TradingLineAction::Modify(
                    working,
                    request(
                        OrderSide::Sell,
                        OrderType::StopLimit {
                            stop: 99.0,
                            limit: 98.0,
                        },
                    ),
                ),
                "~~2~1301~99.00~98.00~~",
            ),
            (TradingLineAction::Close(long.clone()), "~~2~1301~~"),
            (
                TradingLineAction::Reverse(Position::new("1301", -3.0, 100.0)),
                "~~3~1301~~",
            ),
            (
                TradingLineAction::Exit(
                    long.clone(),
                    request(OrderSide::Sell, OrderType::Limit(110.0)),
                ),
                "~~2~1301~~~2~1301~110.00~~",
            ),
            (
                TradingLineAction::Exit(long, request(OrderSide::Sell, OrderType::Stop(95.0))),
                "~~2~1301~~~2~1301~95.00~~",
            ),
        ] {
            let (_, message, _) = describe(&ctx, &action, &|v| format!("{v:.2}"));
            assert_eq!(message, expected, "{action:?}");
        }
    }

    /// Rows of differently formatted symbols keep their own price decimals, host text and pip
    /// value instead of the chart symbol's decimals or a fixed two. A regression writes every
    /// symbol with the chart's decimals, ignores the host's formatter, loses the pip value or shows
    /// another symbol's logo.
    #[test]
    fn symbols_keep_their_own_decimals_host_text_pip_values_and_logos() {
        struct Host;
        impl TradingFormatter for Host {
            fn quantity(&self, symbol: &str, quantity: f64) -> Option<String> {
                (symbol == "BTC").then(|| format!("{quantity} coins"))
            }
        }
        let mut trading = Trading {
            decimals: 5,
            chart_logo: Some(("EURUSD".into(), Some("bytes://chart.svg".into()))),
            ..Trading::default()
        };
        trading.set_instrument(
            Instrument::new("USDJPY")
                .with_price_decimals(3)
                .with_pip_size(0.01)
                .with_logo("bytes://usdjpy.svg"),
        );
        trading.set_instrument(Instrument::new("EURUSD").with_logo("bytes://host.svg"));
        trading.set_formatter(Some(Arc::new(Host)));
        assert_eq!(
            ["EURUSD", "USDJPY", "UNKNOWN"].map(|symbol| trading.logo(symbol)),
            [Some("bytes://chart.svg"), Some("bytes://usdjpy.svg"), None]
        );
        let price = |trading: &Trading, symbol| trading.numbers(symbol, "EURUSD").price(1234.5);
        assert_eq!(price(&trading, "EURUSD"), "1,234.50000");
        assert_eq!(price(&trading, "USDJPY"), "1,234.500");
        assert_eq!(price(&trading, "UNKNOWN"), "1,234.50");
        let quantity = |symbol| trading.numbers(symbol, "EURUSD").quantity(2.0, 0);
        assert_eq!(
            (quantity("BTC"), quantity("USDJPY")),
            ("2 coins".into(), "2".into())
        );

        let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
        chart.set_symbol("USDJPY");
        trading.rules = TradingRules::new("USDJPY");
        trading.rules.point_value = 1000.0;
        let colors = crate::theme::Theme::dark().chart;
        assert_eq!(
            trading.market(&chart, &colors).logo,
            Some("bytes://usdjpy.svg")
        );
        assert_eq!(trading.market(&chart, &colors).pip_value, Some(10.0));
        trading.rules.pip_value = Some(0.5);
        assert_eq!(trading.market(&chart, &colors).pip_value, Some(0.5));
        chart.set_symbol("UNKNOWN");
        trading.rules = TradingRules::new("UNKNOWN");
        assert_eq!(trading.market(&chart, &colors).pip_value, None);
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
        trading.perform(Some(Action::Confirm(Box::new(action.clone()))));
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
        use crate::chart::TradingLabelAlignment::{Center, Left, Right};
        use crate::theme::{Theme, apply_to_egui};
        for theme in [Theme::light(), Theme::dark()] {
            for (width, alignment) in [800.0, 320.0, 260.0]
                .into_iter()
                .flat_map(|width| [Left, Center, Right].map(|alignment| (width, alignment)))
            {
                for instant in [false, true] {
                    for (operation, edge, handle, gesture) in [
                        ("Cancel order", false, false, ""),
                        ("Modify order", false, false, ""),
                        ("Modify order", false, true, ""),
                        ("Modify order", true, false, ""),
                        ("Close position", false, false, ""),
                        ("Reverse position", false, false, ""),
                        ("Place take profit", false, false, ""),
                        ("Place stop loss", false, false, ""),
                        ("Modify order", false, false, "levels entry"),
                        ("Modify order", false, true, "levels drag"),
                        ("Modify order", false, false, "levels remove"),
                        ("Modify order", false, false, "add TP"),
                        ("Modify order", false, false, "add SL"),
                    ] {
                        // The gestures behave alike in both themes; one keeps the test's time down.
                        if !gesture.is_empty() && theme != Theme::light() {
                            continue;
                        }
                        let mut request =
                            OrderRequest::new("SYM", OrderSide::Sell, 1.0, OrderType::Limit(105.0));
                        let add = gesture.starts_with("add");
                        if gesture.starts_with("levels") {
                            request.take_profit_levels = vec![
                                TakeProfitLevel::new(102.0, 0.5),
                                TakeProfitLevel::new(101.0, 0.5),
                            ];
                        } else if !add {
                            request.take_profit = Some(102.0);
                        }
                        if !add {
                            request.stop_loss = Some(108.0);
                        }
                        let order = Order::new(OrderId::new("7"), request, OrderStatus::Working);
                        let position = Position::new("SYM", -2.0, 100.0);
                        let broker = Arc::new(Recorder {
                            state: vec![
                                AccountEvent::Order(order.clone()),
                                AccountEvent::Position(position.clone()),
                            ],
                            brackets: true,
                            ..Recorder::default()
                        });
                        let mut trading = Trading::default();
                        trading.set_service(broker.clone());
                        trading.display.instant_orders = instant;
                        trading.display.label_alignment = alignment;
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
                            for (_, reply) in std::mem::take(&mut *broker.rules.lock().unwrap()) {
                                reply.send(Err(FeedError::NotSupported));
                            }
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
                                    egui::Shape::Rect(rect)
                                        if rect.rect.size() == egui::vec2(20.0, 20.0)
                                            && rect.stroke.width == 1.0
                                            && rect.stroke.color == theme.chart.candle_down =>
                                    {
                                        Some(("Reverse position".into(), rect.rect))
                                    }
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
                        let y = mapping.price_to_y(
                            if matches!(gesture, "levels drag" | "levels remove") {
                                101.0
                            } else if operation.ends_with("order") {
                                105.0
                            } else {
                                100.0
                            },
                        );
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
                        // The position has its own buttons on another row.
                        let on_row = |wanted: &str| {
                            texts
                                .iter()
                                .find(|(text, rect)| {
                                    text == wanted && (rect.center().y - y).abs() < 2.0
                                })
                                .map(|(_, rect)| rect.center().x)
                        };
                        if gesture.is_empty() && operation.ends_with("order") {
                            assert_eq!(
                                (on_row("TP"), on_row("SL")),
                                (None, None),
                                "an order with both exits offers no button"
                            );
                        }
                        let x = match operation {
                            _ if gesture == "levels remove" => close.x,
                            _ if gesture == "add TP" => on_row("TP").expect("the order's TP"),
                            _ if gesture == "add SL" => on_row("SL").expect("the order's SL"),
                            "Modify order" if handle => mapping.rect.right() - 42.0,
                            "Modify order" => find(texts.clone(), "Limit").unwrap().center().x,
                            "Place take profit" => take_profit.center().x,
                            "Place stop loss" => {
                                find(texts.clone(), "SL").expect("an SL button").center().x
                            }
                            _ => close.x,
                        };
                        let start = if operation == "Reverse position" {
                            find(texts.clone(), "Reverse position")
                                .expect("reverse button")
                                .center()
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
                        } else if gesture == "levels remove" {
                            start
                        } else if gesture == "levels drag" {
                            egui::pos2(x, mapping.price_to_y(100.5))
                        } else if gesture == "add TP" {
                            egui::pos2(x, mapping.price_to_y(101.5))
                        } else if gesture == "add SL" {
                            egui::pos2(x, mapping.price_to_y(108.5))
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
                                "{operation} at {width}, {alignment:?}, handle={handle}, instant={instant}, start={start:?}, TP={take_profit:?}, pane={:?}",
                                mapping.rect
                            );
                            assert!(broker.placed.lock().unwrap().is_empty());
                            assert!(broker.cancelled.lock().unwrap().is_empty());
                            assert!(broker.modified.lock().unwrap().is_empty());
                            if operation == "Modify order" && !edge && !handle && gesture.is_empty()
                            {
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
                                let mut expected = order.request.clone();
                                match gesture {
                                    "levels entry" => {
                                        expected.order_type = OrderType::Limit(price);
                                        expected.take_profit_levels = vec![
                                            TakeProfitLevel::new(100.07, 0.5),
                                            TakeProfitLevel::new(99.07, 0.5),
                                        ];
                                        expected.stop_loss = Some(stop_loss);
                                    }
                                    "levels drag" => expected.take_profit_levels[1].price = 100.5,
                                    "levels remove" => {
                                        expected.take_profit = Some(102.0);
                                        expected.take_profit_levels.clear();
                                    }
                                    "add TP" => expected.take_profit = Some(101.5),
                                    "add SL" => expected.stop_loss = Some(108.5),
                                    _ => {
                                        expected.order_type = OrderType::Limit(price);
                                        expected.take_profit = Some(take_profit);
                                        expected.stop_loss = Some(stop_loss);
                                    }
                                }
                                assert_eq!(changed[0].1, expected, "{gesture}");
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

    #[test]
    fn trading_display_preferences_position_lines_and_control_image_exports() {
        use crate::chart::TradingLabelAlignment::{Center, Left, Right};
        let defaults: TradingSettings = serde_json::from_str("{}").unwrap();
        assert!(
            !defaults.show_execution_labels,
            "settings saved before labels were on by default keep them off"
        );
        assert!(TradingSettings::default().show_execution_labels);
        assert!(defaults.extend_lines);
        assert_eq!(defaults.label_alignment, Right);
        assert!(!defaults.include_in_image);
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
            chart.symbol = "SYM".into();
            let broker = Arc::new(Recorder {
                state: vec![AccountEvent::Order(Order::new(
                    OrderId::new("working"),
                    OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Limit(95.0)),
                    OrderStatus::Working,
                ))],
                ..Default::default()
            });
            let mut trading = Trading::default();
            trading.set_service(broker.clone());
            trading.poll(&ctx, &mut Toasts::new());
            let mut centres = Vec::new();
            for alignment in [Left, Center, Right] {
                for extended in [false, true] {
                    trading.display.label_alignment = alignment;
                    trading.display.extend_lines = extended;
                    let json = serde_json::to_string(&trading.display).unwrap();
                    assert_eq!(
                        serde_json::from_str::<TradingSettings>(&json).unwrap(),
                        trading.display
                    );
                    let mut label = egui::Rect::NOTHING;
                    let mut line_start = f32::INFINITY;
                    for frame in 0..3 {
                        let output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    egui::vec2(1440.0, 900.0),
                                )),
                                ..Default::default()
                            },
                            |ui| {
                                chart.show(ui);
                                trading.show(ui, &mut chart);
                            },
                        );
                        if frame < 2 {
                            output.drop_without_applying_deltas();
                            continue;
                        }
                        let y = chart.get_chart_mapping().price_to_y(95.0);
                        for shape in &output.shapes {
                            match &shape.shape {
                                egui::Shape::Rect(rect)
                                    if rect.stroke.color == theme.ui.accent
                                        && rect.rect.height() == 20.0
                                        && rect.rect.width() > 40.0 =>
                                {
                                    label = rect.rect
                                }
                                egui::Shape::LineSegment { points, stroke }
                                    if stroke.color == theme.ui.accent
                                        && (points[0].y - y).abs() < 0.1
                                        && (points[1].y - y).abs() < 0.1 =>
                                {
                                    line_start = line_start.min(points[0].x.min(points[1].x));
                                }
                                _ => {}
                            }
                        }
                        output.drop_without_applying_deltas();
                    }
                    assert!(label.is_positive(), "order label");
                    let pane = chart.get_chart_mapping().rect;
                    assert!(pane.contains_rect(label));
                    let expected = if extended { pane.left() } else { label.left() };
                    assert!(
                        (line_start - expected).abs() < 0.1,
                        "line starts at {line_start}, expected {expected}"
                    );
                    if !extended {
                        centres.push(label.center().x);
                        match alignment {
                            Left => assert!((label.left() - pane.left() - 8.0).abs() < 0.1),
                            Center => assert!((label.center().x - pane.center().x).abs() < 0.1),
                            Right => assert!(label.right() < pane.right()),
                        }
                    }
                }
            }
            assert!(centres.windows(2).all(|pair| pair[0] < pair[1]));
            let options = crate::widget::ImageExportOptions {
                size: [480, 320],
                include_timestamp: false,
                ..Default::default()
            };
            let before = chart.get_time_scale_state();
            let plain = chart.export_png(&ctx, &options).unwrap();
            assert_eq!(trading.export_png(&chart, &ctx, &options).unwrap(), plain);
            trading.display.include_in_image = true;
            let with_orders = trading.export_png(&chart, &ctx, &options).unwrap();
            assert_ne!(with_orders, plain, "visible broker orders are exported");
            trading.display.show_orders = false;
            assert_eq!(
                trading.export_png(&chart, &ctx, &options).unwrap(),
                plain,
                "hidden orders stay hidden"
            );
            assert_eq!(chart.get_time_scale_state(), before);
            assert!(broker.placed.lock().unwrap().is_empty());
            trading.disconnect();
            trading.display.bid_ask_labels = true;
            chart.set_quote(Some(crate::model::Level1Quote {
                symbol: "SYM".into(),
                ts: chart.bars(SeriesId::MAIN)[99].time,
                bid: 99.0,
                ask: 101.0,
                bid_size: 1.0,
                ask_size: 1.0,
            }));
            let plain = chart.export_png(&ctx, &options).unwrap();
            for include in [false, true] {
                trading.display.include_in_image = include;
                assert_eq!(
                    trading.export_png(&chart, &ctx, &options).unwrap() != plain,
                    include,
                    "bid and ask labels do not require a broker connection"
                );
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
                                if text.galley.text() == side_name(side)
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

    /// Guards the capability gates: an account that does not report closing, reversing or
    /// previewing never gets those requests, and a preview alone never places an order. A
    /// regression sends a market order the broker said it cannot take, or places an order the
    /// user only asked to price.
    #[test]
    fn close_reverse_and_preview_follow_the_accounts_capabilities() {
        let broker = Arc::new(Recorder::default());
        let mut trading = Trading::default();
        trading.set_service(broker.clone());
        let ctx = egui::Context::default();
        let mut toasts = Toasts::new();
        trading.poll(&ctx, &mut toasts);
        trading.rules = TradingRules::new("ES");
        trading.rules_ready = true;
        let position = Position::new("ES", 2.0, 100.0);
        let order = OrderRequest::new("ES", OrderSide::Buy, 1.0, OrderType::Limit(99.0));
        // (close, reverse, preview) -> the market quantities placed and the previews asked for.
        for (close, reverse, preview, placed, previewed) in [
            (true, true, true, vec![2.0, 4.0], 1),
            (true, false, false, vec![2.0], 0),
            (false, true, false, vec![4.0], 0),
            (false, false, false, vec![], 0),
        ] {
            let capabilities = Capabilities {
                market: true,
                close,
                reverse,
                preview,
                ..Capabilities::default()
            };
            trading.apply(
                &egui::Context::default(),
                AccountEvent::Account(Account::new("USD", 0.0, capabilities)),
                &mut toasts,
            );
            trading.execute(Some(TradingLineAction::Close(position.clone())));
            trading.execute(Some(TradingLineAction::Reverse(position.clone())));
            trading.perform(Some(Action::Preview(order.clone())));
            let sent: Vec<f64> = std::mem::take(&mut *broker.placed.lock().unwrap())
                .iter()
                .map(|order| order.quantity)
                .collect();
            assert_eq!(sent, placed, "{close} {reverse} {preview}");
            let asked = std::mem::take(&mut *broker.previews.lock().unwrap());
            assert_eq!(asked.len(), previewed, "{close} {reverse} {preview}");
            for (request, reply) in asked {
                assert_eq!(request, order);
                reply.send(Ok(OrderPreview::default()));
                trading.poll(&ctx, &mut toasts);
                assert_eq!(
                    trading.preview.as_ref().map(|p| p.estimate.is_some()),
                    Some(true)
                );
            }
        }
        // A refused preview clears the pending one and tells the user.
        trading.apply(
            &egui::Context::default(),
            AccountEvent::Account(Account::new(
                "USD",
                0.0,
                Capabilities {
                    preview: true,
                    ..Capabilities::default()
                },
            )),
            &mut toasts,
        );
        trading.perform(Some(Action::Preview(order)));
        let (_, reply) = broker.previews.lock().unwrap().pop().unwrap();
        reply.send(Err(FeedError::NotSupported));
        trading.poll(&ctx, &mut toasts);
        assert!(trading.preview.is_none());
        assert!(
            toasts
                .toasts()
                .iter()
                .any(|t| t.message.contains("preview"))
        );
        assert!(broker.placed.lock().unwrap().is_empty());
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

    /// While the broker reports that it is not live the ticket shows its status and why it
    /// cannot send, and an order, cancellation or change the user confirms is reported as
    /// refused, once each, instead of sent; a close confirmed before the drop that waits for its
    /// symbol's rules is refused too and not sent when they arrive after the broker is back. Once
    /// the broker is live the same actions send one call each. A regression sends to a broker
    /// that is not live, sends a confirmation from before the drop after it, refuses without a
    /// word or leaves the ticket looking ready.
    #[test]
    fn nothing_is_sent_while_the_broker_is_not_live() {
        use crate::data::FeedStatus;
        let ctx = egui::Context::default();
        let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
        chart.symbol = "SYM".into();
        let working = Order::new(
            OrderId::new("7"),
            OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Limit(95.0)),
            OrderStatus::Working,
        );
        let mut moved = working.request.clone();
        moved.order_type = OrderType::Limit(96.0);
        let place = OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Market);
        let broker = Arc::new(Recorder {
            state: vec![AccountEvent::Order(working.clone())],
            ..Recorder::default()
        });
        let mut trading = Trading::default();
        trading.set_service(broker.clone());
        trading.poll(&ctx, &mut Toasts::new());
        trading.open_ticket(place.clone());
        let (_, reply) = broker.rules.lock().unwrap().pop().unwrap();
        reply.send(Ok(TradingRules::new("SYM")));
        trading.poll(&ctx, &mut Toasts::new());
        // Confirmed while live, the close waits for the rules of its own symbol.
        trading.execute(Some(TradingLineAction::Close(Position::new(
            "OTHER", 2.0, 100.0,
        ))));
        let (symbol, other) = broker.rules.lock().unwrap().pop().unwrap();
        assert_eq!(symbol, "OTHER");
        // The ticket's status and its reason not to send, as drawn.
        let shown = |trading: &mut Trading| {
            let input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(800.0, 600.0),
                )),
                ..Default::default()
            };
            let output = ctx.run_ui(input, |ui| trading.show_ticket(ui, &chart));
            let mut texts: Vec<String> = (output.shapes.iter())
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text)
                        if [
                            "Connecting...",
                            "Offline",
                            "Waiting for the broker to connect",
                        ]
                        .contains(&text.galley.text()) =>
                    {
                        Some(text.galley.text().to_owned())
                    }
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            texts.sort();
            texts
        };
        let confirm = |trading: &mut Trading| {
            trading.perform(Some(Action::Place(place.clone())));
            trading.perform(Some(Action::Cancel(vec![working.id.clone()])));
            trading.execute(Some(TradingLineAction::Modify(
                working.clone(),
                moved.clone(),
            )));
        };

        let mut toasts = Toasts::new();
        trading.apply(
            &egui::Context::default(),
            AccountEvent::Status(FeedStatus::Disconnected),
            &mut toasts,
        );
        assert_eq!(
            shown(&mut trading),
            ["Offline", "Waiting for the broker to connect"]
        );
        confirm(&mut trading);
        // With instant orders `show` passes the lines' action every frame, mostly none.
        trading.execute(None);
        trading.poll(&ctx, &mut toasts);
        let messages: Vec<_> = (toasts.toasts().iter())
            .map(|toast| toast.message.as_str())
            .collect();
        assert_eq!(messages, ["Order refused: the broker is not connected"; 4]);

        trading.apply(
            &egui::Context::default(),
            AccountEvent::Status(FeedStatus::Live),
            &mut toasts,
        );
        other.send(Ok(TradingRules::new("OTHER")));
        trading.poll(&ctx, &mut toasts);
        assert!(shown(&mut trading).is_empty());
        assert!(
            broker.placed.lock().unwrap().is_empty(),
            "the close confirmed before the drop"
        );
        assert!(broker.cancelled.lock().unwrap().is_empty());
        assert!(broker.modified.lock().unwrap().is_empty());
        confirm(&mut trading);
        trading.poll(&ctx, &mut toasts);
        assert_eq!(*broker.placed.lock().unwrap(), [place]);
        assert_eq!(
            *broker.cancelled.lock().unwrap(),
            std::slice::from_ref(&working.id)
        );
        assert_eq!(*broker.modified.lock().unwrap(), [(working.id, moved)]);
        assert_eq!(toasts.toasts().len(), 4, "the broker accepted them");
    }

    /// At the narrowest ticket a long symbol gives way, so that the broker's status stays clear of
    /// the More and Close buttons. A regression covers the status with the buttons, the one sign
    /// in the header of why the ticket does not send, or draws it twice.
    #[test]
    fn a_long_symbol_gives_way_to_the_tickets_status_and_buttons() {
        use crate::data::FeedStatus;
        let width = 320.0;
        let ctx = egui::Context::default();
        let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
        chart.symbol = "EXCHANGE:A.VERY.LONG.SYMBOL.NAME.INDEED".into();
        let mut trading = Trading::default();
        trading.set_service(Arc::new(Recorder::default()));
        trading.open_ticket(OrderRequest::new(
            chart.symbol(),
            OrderSide::Buy,
            1.0,
            OrderType::Market,
        ));
        trading.poll(&ctx, &mut Toasts::new());
        trading.apply(
            &egui::Context::default(),
            AccountEvent::Status(FeedStatus::Disconnected),
            &mut Toasts::new(),
        );
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(width, 600.0),
            )),
            ..Default::default()
        };
        let output = ctx.run_ui(input, |ui| trading.show_ticket(ui, &chart));
        let status: Vec<f32> = (output.shapes.iter())
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) if text.galley.text() == "Offline" => {
                    Some(text.visual_bounding_rect().right())
                }
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        assert_eq!(status.len(), 1, "{status:?}");
        // The panel's margin of 16 points, then the two buttons of the header.
        assert!(status[0] <= width - 16.0 - 50.0, "{status:?}");
    }
}
