//! The trading controller's chart view: the order ticket's panel or window, the orders,
//! positions and fills drawn on the chart, the image exports, and confirmations of their changes.

use super::account_panel::quantity_decimals;
use super::format::Numbers;
use super::ticket::{
    self, Action, Market, OrderPreset, OrderTicket, QuantityLimit, order_quantity,
};
use super::{NOT_LIVE, Preview, Sent, Trading, executions};
use crate::chart::selection::SeriesId;
use crate::chart::{
    TradingLineAction, TradingLineOptions, exit_request, held_exits, render_trading_lines,
};
use crate::localization as l10n;
use crate::model::{
    Order, OrderId, OrderRequest, OrderSide, OrderStatus, OrderType, Position, PositionBracket,
    TradingRules,
};
use crate::scales::{decimals, to_step};
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::dialog::DialogFooter;
use crate::ui_kit::modal::Modal;
use crate::ui_kit::modal::dialogs::ConfirmDialog;
use crate::widget::Chart;

impl Trading {
    /// What the order ticket needs from `chart` and the account this frame. Order tiers take
    /// `colors` unless the chart's book view sets its own.
    fn market<'a>(&'a self, chart: &'a Chart, colors: &crate::theme::ChartColors) -> Market<'a> {
        let symbol = chart.symbol();
        let instrument = chart.instrument().or_else(|| self.instruments.get(symbol));
        let decimals = instrument
            .and_then(crate::scales::instrument_price_precision)
            .map_or(2, usize::from);
        let valid = instrument.filter(|instrument| {
            instrument
                .tick_size
                .is_some_and(|tick| tick.is_finite() && tick > 0.0)
        });
        let tick = valid
            .and_then(|instrument| instrument.tick_size)
            .unwrap_or(10f64.powi(-(decimals as i32)));
        // Price ranges refine the instrument's tick, not the decimals' step.
        let tick_tiers = valid.map_or(&[][..], |instrument| &instrument.tick_tiers);
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
                            o.size,
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
            tick_tiers,
            decimals,
            currency: self
                .account
                .as_ref()
                .map_or("", |account| account.currency.as_str()),
            balance: self.account.as_ref().map(|account| account.balance),
            available_funds: self
                .account
                .as_ref()
                .and_then(|account| account.available_funds),
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
            pnl_unit: self.display.position_pnl_unit,
            logo: instrument.and_then(|instrument| instrument.logo.as_deref()),
            account: &self.account_id,
            timezone: chart.display_timezone(),
        }
    }

    fn trade_preset(
        &mut self,
        ctx: &egui::Context,
        chart: &Chart,
        colors: &crate::theme::ChartColors,
        side: OrderSide,
        preset: &OrderPreset,
    ) {
        let mut request = OrderRequest::new(chart.symbol(), side, self.quantity, OrderType::Market);
        if self.display.instant_orders && self.capabilities().market && self.rules_ready {
            request.time_in_force = self.rules.time_in_force[0];
            let placed = {
                let market = self.market(chart, colors);
                let mut ticket = OrderTicket::default();
                ticket.open(&request);
                ticket
                    .apply(preset, &market)
                    .then(|| ticket.request(ctx, &market).ok())
                    .flatten()
                    .filter(|request| request.order_type == OrderType::Market)
            };
            if let Some(request) = placed {
                self.ticket.open = false;
                self.place(request);
                return;
            }
        }
        self.open_ticket(request);
        let mut ticket = std::mem::take(&mut self.ticket);
        ticket.apply(preset, &self.market(chart, colors));
        self.ticket = ticket;
    }

    /// Draws the open order ticket for the chart's symbol, at the right of `ui` or, with
    /// [`undocked_ticket`](crate::widget::settings::TradingSettings::undocked_ticket), in a
    /// movable window over it. Call before laying out the chart so it takes the space left;
    /// undocking leaves its full width. The ticket header changes `display.undocked_ticket`,
    /// as its More menu changes `compact_ticket`; read both back when saving. Sends submitted
    /// orders and depth ladder orders; changes to working orders and positions wait for the
    /// confirmation [`show`](Self::show) asks for.
    pub fn show_ticket(&mut self, ui: &mut egui::Ui, chart: &Chart) {
        if !self.ticket.open {
            return;
        }
        let symbol = chart.symbol();
        if self.ticket.symbol != symbol {
            // The ticket follows the chart; prices of the previous symbol mean nothing here.
            self.ticket.change_symbol(symbol);
        }
        if self.account_panel.editing.is_none() && !self.asks_close_quantity() {
            self.load_rules(symbol);
        }
        let mut ticket = std::mem::take(&mut self.ticket);
        let mut presets = std::mem::take(&mut self.presets);
        let mut one_click = self.display.instant_orders;
        let mut compact = self.display.compact_ticket;
        let mut undocked = self.display.undocked_ticket;
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
        let action = if self.display.undocked_ticket {
            let area = ui.available_rect_before_wrap();
            egui::Window::new(l10n::text(ui.ctx(), "Order ticket"))
                .id(egui::Id::unique("order_ticket_window"))
                .collapsible(false)
                .resizable(false)
                .fixed_size(egui::vec2(
                    area.width().min(340.0),
                    area.height().min(720.0),
                ))
                .pivot(egui::Align2::RIGHT_TOP)
                .default_pos(area.right_top() + egui::vec2(-8.0, 8.0))
                .constrain_to(area)
                .frame(
                    egui::Frame::window(ui.style())
                        .fill(theme.ui.panel_bg)
                        .inner_margin(egui::Margin::symmetric(16, 8)),
                )
                .show(ui.ctx(), |ui| {
                    ticket.show(
                        ui,
                        &market,
                        &mut presets,
                        &mut one_click,
                        &mut compact,
                        &mut undocked,
                    )
                })
                .and_then(|response| response.inner)
                .flatten()
        } else {
            let action = egui::Panel::right("order_ticket")
                .resizable(false)
                .exact_size(ui.available_width().min(340.0))
                .frame(
                    egui::Frame::new()
                        .fill(theme.ui.panel_bg)
                        .inner_margin(egui::Margin::symmetric(16, 8)),
                )
                .show(ui, |ui| {
                    ticket.show(
                        ui,
                        &market,
                        &mut presets,
                        &mut one_click,
                        &mut compact,
                        &mut undocked,
                    )
                })
                .inner;
            // Later panels may grow to their contents, but must not cover the docked ticket.
            ui.shrink_clip_rect(ui.available_rect_before_wrap());
            action
        };
        (
            self.ticket,
            self.presets,
            self.display.instant_orders,
            self.display.compact_ticket,
            self.display.undocked_ticket,
        ) = (ticket, presets, one_click, compact, undocked);
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
                for id in ids {
                    let request = self
                        .orders
                        .iter()
                        .find(|order| order.id == id)
                        .map(|order| order.request.clone());
                    self.cancel_order(id, request);
                }
            }
            Some(Action::Confirm(change)) => self.ask(*change, false),
            // The ticket opens its own edits.
            Some(Action::Edit(_)) | None => {}
        }
    }

    /// Starts a confirmation, optionally asking for a Close quantity. The position's rules
    /// temporarily take the chart's rules slot, as they do for the order editor.
    pub(super) fn ask(&mut self, action: TradingLineAction, quantity: bool) {
        self.close_quantity = None;
        if let TradingLineAction::Close(position) = &action
            && quantity
            && self.capabilities().close
            && self.capabilities().partial_close
        {
            self.account_panel.editing = None;
            // Changing the rules discards pending actions, so load them before setting this one.
            self.load_rules(&position.symbol);
            self.close_quantity = Some(position.quantity.abs());
        }
        self.pending = Some(action);
    }

    /// Keeps the position's rules while its quantity dialog is open.
    fn asks_close_quantity(&self) -> bool {
        self.close_quantity.is_some() && matches!(self.pending, Some(TradingLineAction::Close(_)))
    }

    fn draw_overlays(&self, ui: &mut egui::Ui, chart: &Chart) -> crate::chart::TradingLines {
        let symbol = chart.symbol();
        let market = self.market(chart, &ui.active_theme().chart);
        let (decimals, tick, last) = (market.decimals, market.tick, market.last);
        let capabilities = self.capabilities();
        let mapping = chart.get_chart_mapping();
        let display = &self.display;
        let options = TradingLineOptions {
            timezone: chart.display_timezone(),
            show_orders: display.show_orders,
            extend_lines: display.extend_lines,
            line_width: 1.0,
            line_length: 0.0,
            label_alignment: display.label_alignment,
            separate_labels: Some(display.separate_labels),
            show_positions: display.show_positions,
            submit: false,
            modify: capabilities.modify,
            close: capabilities.close,
            reverse: display.reverse_button && capabilities.reverse,
            take_profit: capabilities.position_exits && capabilities.limit,
            stop_loss: capabilities.position_exits && capabilities.stop,
            order_exits: capabilities.brackets && capabilities.modify,
            menu: Some(capabilities),
            order_appearance: &[],
            position_appearance: &[],
            coverage: capabilities.brackets,
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
            formatter: self.formatter.as_deref(),
            tick_size: tick,
            tick_tiers: market.tick_tiers,
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
                let fills = &self.account_panel.fills;
                let marks: Vec<_> = fills
                    .executions
                    .iter()
                    .filter(|fill| fills.symbol(fill) == Some(symbol))
                    .map(super::ExecutionMark::new)
                    .collect();
                // `Trading::numbers` takes the ticket's decimals; prices here keep the market's.
                let numbers = Numbers {
                    symbol,
                    decimals,
                    formatter: self.formatter.as_deref(),
                };
                executions::show(ui, chart, &marks, display, numbers);
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
    ) -> Result<crate::widget::ChartImage, crate::widget::ExportError> {
        chart.export_layout_image(ctx, options, |ui, chart| self.draw_export(ui, chart))
    }

    /// Encodes [`Self::export_image`] as PNG bytes for host file, clipboard or sharing delivery.
    pub fn export_png(
        &self,
        chart: &Chart,
        ctx: &egui::Context,
        options: &crate::widget::ImageExportOptions,
    ) -> Result<Vec<u8>, crate::widget::ExportError> {
        chart.export_layout_png(ctx, options, |ui, chart| self.draw_export(ui, chart))
    }

    /// Writes [`Self::export_image`] as SVG, as [`Chart::export_svg`] does.
    pub fn export_svg(
        &self,
        chart: &Chart,
        ctx: &egui::Context,
        options: &crate::widget::ImageExportOptions,
    ) -> Result<String, crate::widget::ExportError> {
        chart.export_layout_svg(ctx, options, |ui, chart| self.draw_export(ui, chart))
    }

    /// Draws a frozen chart and its opted-in trading overlays inside an image layout callback.
    /// Use with [`Chart::export_layout_image`] and [`Chart::export_snapshot`] for several charts.
    pub fn draw_export(&self, ui: &mut egui::Ui, chart: &mut Chart) {
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
    /// [`render_executions`](super::render_executions) draws other fills, such as imported trades,
    /// with the same marks.
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
        if self.account_panel.editing.is_none() && !self.asks_close_quantity() {
            self.load_rules(symbol);
        }

        if let Some((quoted, side)) = self.quoted.take() {
            match self.applied_preset().cloned().filter(|_| quoted == symbol) {
                Some(preset) => self.trade_preset(ui.ctx(), chart, &colors, side, &preset),
                None => self.trade_now(quoted, side),
            }
        }

        if self.service.is_some() {
            let lines = self.draw_overlays(ui, chart);
            if let Some(on) = lines.separate_labels {
                self.display.separate_labels = on;
            }
            chart.trading_tags = lines.tags;
            chart.trading_outlined = lines.outlined;
            match lines.action {
                // Each dialog is the user's confirmation.
                Some(TradingLineAction::Edit(order, request)) => self.edit_order(order, request),
                Some(TradingLineAction::Protect(position)) => {
                    self.protecting = Some((position, None, None))
                }
                action if self.display.instant_orders => self.execute(action),
                Some(action) if self.pending.is_none() => self.ask(action, true),
                _ => {}
            }
        }
        if let Some(action) = &self.pending {
            // A row of the account panel may ask about another symbol than the chart's.
            let action_symbol = match action {
                TradingLineAction::Cancel(order)
                | TradingLineAction::Modify(order, _)
                | TradingLineAction::Edit(order, _) => &order.request.symbol,
                TradingLineAction::Close(position)
                | TradingLineAction::Reverse(position)
                | TradingLineAction::Exit(position, _)
                | TradingLineAction::Protect(position) => &position.symbol,
            };
            let numbers = self.numbers(action_symbol, symbol);
            let (title, message, confirm) = describe(ui.ctx(), action, &numbers);
            let mut open = true;
            let confirmed = if let (TradingLineAction::Close(position), Some(mut quantity)) =
                (action, self.close_quantity)
            {
                let rules = if self.rules_ready && self.rules.symbol == position.symbol {
                    Ok(&self.rules)
                } else if self.rules_failed {
                    Err("The symbol's trading rules are unavailable")
                } else {
                    Err("Waiting for the symbol's trading rules")
                };
                let confirmed = close_dialog(
                    ui.ctx(),
                    &message,
                    position.quantity.abs(),
                    &mut quantity,
                    rules,
                    &numbers,
                    &mut open,
                )
                .map(Some);
                self.close_quantity = Some(quantity);
                confirmed
            } else {
                ConfirmDialog::new("trading_confirm", title, message)
                    .confirm_text(confirm)
                    .cancel_text("Discard")
                    .show(ui.ctx(), &mut open)
                    .is_confirmed()
                    .then_some(None)
            };
            if let Some(part) = confirmed {
                let action = self.pending.take();
                self.close_quantity = None;
                self.execute_with(action, part);
            } else if !open {
                self.pending = None;
                self.close_quantity = None;
            }
        }

        self.show_order_editor(ui.ctx(), symbol);
        self.show_protect(ui.ctx(), symbol);

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
                    ui.ctx(),
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

        if let Some(choice) = chart.preset_chosen.take()
            && let Some(saved) = self.presets.get_mut(&self.account_id)
        {
            let selected =
                choice.and_then(|name| saved.iter().position(|preset| preset.name == name));
            for (index, preset) in saved.iter_mut().enumerate() {
                preset.applied = selected == Some(index);
            }
        }
        let currency = self
            .account
            .as_ref()
            .map_or("", |account| account.currency.as_str());
        chart.order_presets = self
            .service
            .as_ref()
            .and_then(|_| self.presets.get(&self.account_id))
            .filter(|saved| !saved.is_empty())
            .map(|saved| crate::chart::rendering::QuotePresets {
                applied: saved.iter().position(|preset| preset.applied),
                presets: saved
                    .iter()
                    .map(|preset| (preset.name.clone(), preset.summary(ui.ctx(), currency)))
                    .collect(),
            });

        // The legend shows the order quantity under the spread; clicking it toggles the calculator.
        chart.order_quantity = self.service.is_some().then_some(self.quantity);
        chart.order_quantity_text = self
            .service
            .as_ref()
            .and(self.formatter.as_deref())
            .and_then(|formatter| formatter.quantity(chart.symbol(), self.quantity));
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
        if self.ticket.editing.is_none()
            && let Ok(request) = self.ticket.request(ui.ctx(), &market)
            && request.order_type != OrderType::Market
        {
            let _theme = chart
                .theme_override
                .as_ref()
                .map(|theme| crate::theme::ThemeScope::new(ui.ctx(), theme));
            let mapping = chart.get_chart_mapping();
            let options = TradingLineOptions {
                timezone: chart.display_timezone(),
                show_orders: true,
                extend_lines: self.display.extend_lines,
                line_width: 1.0,
                line_length: 0.0,
                label_alignment: self.display.label_alignment,
                separate_labels: None,
                show_positions: false,
                submit: true,
                modify: true,
                close: false,
                reverse: false,
                take_profit: false,
                stop_loss: false,
                order_exits: self.capabilities().brackets,
                menu: None,
                order_appearance: &[],
                position_appearance: &[],
                coverage: self.capabilities().brackets,
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
                formatter: self.formatter.as_deref(),
                tick_size: tick,
                tick_tiers: market.tick_tiers,
            };
            let mut order = Order::new(
                OrderId::new("projected"),
                request.clone(),
                OrderStatus::Working,
            );
            // Where a trailing stop's stop starts, as the broker would set it now.
            order.trailing_stop = last
                .and_then(|last| request.order_type.trail(request.side, None, last))
                .map(|(_, stop)| stop);
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

    /// Opens the Modify order dialog for `order`, starting from `request`.
    pub(super) fn edit_order(&mut self, order: Order, request: OrderRequest) {
        self.ticket.open = false;
        self.load_rules(&order.request.symbol);
        self.account_panel.editing = Some((order, request));
    }

    fn show_protect(&mut self, ctx: &egui::Context, chart_symbol: &str) {
        let Some((position, _, _)) = &self.protecting else {
            return;
        };
        let held = held_exits(position, &self.orders);
        let capabilities = self.capabilities();
        let exits = capabilities.position_exits;
        let offers = [
            exits && capabilities.limit && !held.0,
            exits && capabilities.stop && !held.1,
        ];
        if !offers[0] && !offers[1] {
            self.protecting = None;
            return;
        }
        let decimals = self.numbers(&position.symbol, chart_symbol).decimals;
        let Some((position, take_profit, stop_loss)) = &mut self.protecting else {
            return;
        };
        let mut open = true;
        let mut save = false;
        egui::Window::new(l10n::text(ctx, "Protect position"))
            .id(egui::Id::unique("trading_protect"))
            .collapsible(false)
            .resizable(false)
            .open(&mut open)
            .show(ctx, |ui| {
                egui::Grid::new("trading_protect_grid")
                    .num_columns(2)
                    .spacing([12.0, 8.0])
                    .show(ui, |ui| {
                        for (offered, label, bracket) in [
                            (offers[0], "Take profit", &mut *take_profit),
                            (offers[1], "Stop loss", &mut *stop_loss),
                        ] {
                            if !offered {
                                *bracket = None;
                                continue;
                            }
                            let mut on = bracket.is_some();
                            let toggle = ui.checkbox(&mut on, l10n::text(ctx, label));
                            if on {
                                let value = bracket.get_or_insert(position.average_price);
                                ui.add(
                                    egui::DragValue::new(value)
                                        .range(0.0..=f64::MAX)
                                        .max_decimals(decimals)
                                        .speed(10f64.powi(-(decimals as i32))),
                                )
                                .labelled_by(toggle.id);
                            } else {
                                *bracket = None;
                            }
                            ui.end_row();
                        }
                    });
                let valid = (take_profit.is_some() || stop_loss.is_some())
                    && take_profit.iter().chain(stop_loss.iter()).all(|price| {
                        price.is_finite() && *price > 0.0 && *price != position.average_price
                    });
                save = ui
                    .add_enabled(
                        valid,
                        egui::Button::new(l10n::text(ctx, "Protect position")),
                    )
                    .clicked();
            });
        if save {
            let (position, take_profit, stop_loss) = self.protecting.take().expect("protecting");
            for (bracket, price) in [
                (PositionBracket::TakeProfit, take_profit),
                (PositionBracket::StopLoss, stop_loss),
            ] {
                if let Some(price) = price {
                    self.execute(Some(TradingLineAction::Exit(
                        position.clone(),
                        exit_request(&position, bracket, price),
                    )));
                }
            }
        } else if !open {
            self.protecting = None;
        }
    }

    /// Sends a confirmed change to the broker, or reports it as refused while the broker is not
    /// live. Closing and reversing a position are market orders on the other side, for the
    /// position's size and for twice it.
    pub(super) fn execute(&mut self, action: Option<TradingLineAction>) {
        self.execute_with(action, None);
    }

    /// Sends a confirmed Close for the checked `part`, or the whole position without one.
    fn execute_with(&mut self, action: Option<TradingLineAction>, part: Option<f64>) {
        // With instant orders `show` passes the lines' action of every frame, mostly none.
        if action.is_none() {
            return;
        }
        if self.service.is_none() {
            return;
        }
        let reverse = matches!(action, Some(TradingLineAction::Reverse(_)));
        let (position, mut order) = match action {
            Some(TradingLineAction::Cancel(order)) => {
                return self.cancel_order(order.id, Some(order.request));
            }
            Some(TradingLineAction::Modify(order, request)) => {
                if let Some((service, key)) = self.live_service(Sent::Modify(order.request)) {
                    service.modify(order.id, request, self.inbox.reply(key));
                }
                return;
            }
            Some(TradingLineAction::Exit(position, exit)) => (position, exit),
            Some(TradingLineAction::Close(position) | TradingLineAction::Reverse(position))
                if position.quantity != 0.0
                    && if reverse {
                        self.capabilities().reverse
                    } else {
                        self.capabilities().close
                            && (part.is_none() || self.capabilities().partial_close)
                    } =>
            {
                let side = if position.quantity > 0.0 {
                    OrderSide::Sell
                } else {
                    OrderSide::Buy
                };
                let size = position.quantity.abs();
                let quantity = if reverse {
                    size * 2.0
                } else {
                    part.unwrap_or(size)
                };
                let order = OrderRequest::new(&position.symbol, side, quantity, OrderType::Market);
                (position, order)
            }
            _ => return,
        };
        if self.rules_ready && self.rules.symbol == order.symbol {
            order.time_in_force = self.rules.time_in_force[0];
            self.place(order);
        } else if !self.is_connected() {
            self.refuse(Sent::Place(order), NOT_LIVE);
        } else if let Some(service) = self.service.clone() {
            self.request_key += 2;
            service.rules(&order.symbol, self.inbox.reply(self.request_key));
            self.placing.push((self.request_key, position, order));
        }
    }
}

/// The open of the exchange session containing `time`, by the chart's trading hours and zone.
pub(super) fn session_open(
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

/// Asks how much to close, retaining the whole position while its rules are unavailable.
/// Dismissal sets `open` to false; confirmation returns the checked quantity.
fn close_dialog(
    ctx: &egui::Context,
    message: &str,
    size: f64,
    quantity: &mut f64,
    rules: Result<&TradingRules, &str>,
    numbers: &Numbers<'_>,
    open: &mut bool,
) -> Option<f64> {
    use super::widgets::{field, muted, number};
    let response = Modal::new("trading_close", "Close position")
        .width(DESIGN_TOKENS.sizing.dialog.confirm_width)
        .show(ctx, open, |ui| {
            ui.label(message);
            ui.add_space(ui.active_theme().spacing.sm);
            let label = ui.label(l10n::text(ui.ctx(), "Quantity to close"));
            let places = rules
                .map_or(0, |rules| decimals(rules.quantity_step))
                .max(decimals(size));
            if rules.is_err() {
                *quantity = size;
            }
            ui.add_enabled_ui(rules.is_ok(), |ui| {
                field(ui, |ui| {
                    number(ui, "close_quantity", quantity, places).labelled_by(label.id);
                });
            });
            let part = match rules {
                Ok(rules) => close_part(*quantity, size, rules)
                    .map_err(|limit| limit.message(ui.ctx(), numbers, places)),
                Err(reason) => {
                    muted(ui, reason);
                    Ok(size)
                }
            };
            if let Err(why) = &part {
                ui.colored_label(ui.active_theme().ui.text_muted, why);
            }
            let (confirm, cancel) = DialogFooter::new("Close position")
                .secondary("Discard")
                .primary_enabled(part.is_ok())
                .show(ui);
            (confirm.then_some(part.ok()).flatten(), cancel)
        })?;
    let (part, cancelled) = response.inner;
    if part.is_some() || cancelled {
        *open = false;
    }
    part
}

/// Full closes retain their existing exemption from symbol limits. A smaller close rounds
/// down to the symbol's step and respects its bounds, without exceeding the held position.
fn close_part(requested: f64, size: f64, rules: &TradingRules) -> Result<f64, QuantityLimit> {
    // The field writes the size at 15 significant digits, which can read back a few ulps off;
    // a part one step below the size stays a part down to steps of a trillionth of it.
    if (requested - size).abs() <= size * 1e-12 {
        return Ok(size);
    }
    if requested > size {
        return Err(QuantityLimit::Maximum(size));
    }
    order_quantity(requested, rules)
}

/// The title, question and confirm button text of the confirmation for `action`, with its
/// numbers written by `numbers` and quantities at their own decimals, as the account tables
/// write them.
fn describe(
    ctx: &egui::Context,
    action: &TradingLineAction,
    numbers: &Numbers<'_>,
) -> (&'static str, String, &'static str) {
    let price = |value| numbers.price(value);
    let quantity_text = |value: f64| numbers.quantity(value, quantity_decimals(value));
    let order = |request: &OrderRequest| {
        let quantity = quantity_text(request.quantity);
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
            (
                OrderSide::Buy,
                OrderType::TrailingStop {
                    activation: Some(_),
                    ..
                },
            ) => "BUY {quantity} {symbol} trailing stop {distance} from {price}",
            (
                OrderSide::Sell,
                OrderType::TrailingStop {
                    activation: Some(_),
                    ..
                },
            ) => "SELL {quantity} {symbol} trailing stop {distance} from {price}",
            (OrderSide::Buy, OrderType::TrailingStop { .. }) => {
                "BUY {quantity} {symbol} trailing stop {distance}"
            }
            (OrderSide::Sell, OrderType::TrailingStop { .. }) => {
                "SELL {quantity} {symbol} trailing stop {distance}"
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
            OrderType::TrailingStop {
                distance,
                activation,
            } => l10n::format_text(
                ctx,
                key,
                &[
                    values[0],
                    values[1],
                    ("distance", numbers.trail(ctx, distance).as_str()),
                    ("price", activation.map(price).unwrap_or_default().as_str()),
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
            let mut values = vec![
                ("price", price(level.price)),
                ("quantity", quantity_text(level.quantity)),
            ];
            let key = if let Some(stop) = level.stop_loss {
                values.push(("stop", price(stop)));
                ", take profit {price} for {quantity} with a stop loss at {stop}"
            } else {
                ", take profit {price} for {quantity}"
            };
            text.push_str(&l10n::format_text(ctx, key, &values));
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
                ("quantity", quantity_text(position.quantity.abs()).as_str()),
                ("symbol", position.symbol.as_str()),
            ],
        )
    };
    match action {
        TradingLineAction::Protect(_) => unreachable!("show opens a dialog for it"),
        TradingLineAction::Cancel(working) => (
            "Cancel order",
            l10n::format_text(
                ctx,
                "Cancel the order to {order}?",
                &[("order", order(&working.request).as_str())],
            ),
            "Cancel order",
        ),
        TradingLineAction::Modify(_, changed) | TradingLineAction::Edit(_, changed) => (
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

    /// Orders report expiry in the table's UTC columns and label it in the chart's zone.
    #[test]
    fn good_till_date_orders_show_their_expiry() {
        use crate::model::{TimeInForce, TimezoneId};
        use crate::scales::TimezoneMode;
        use egui_kittest::kittest::Queryable;
        let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
        chart.set_symbol("ES");
        chart.chart_options.time_scale.timezone =
            TimezoneMode::Timezone(TimezoneId::fixed_offset(-5 * 3600).unwrap());
        let mut request = OrderRequest::new("ES", OrderSide::Buy, 1.0, OrderType::Limit(95.0));
        request.time_in_force = TimeInForce::GoodTillDate;
        request.expires = Some("2026-12-01T21:00:00Z".parse().unwrap());
        let order = Order::new(OrderId::new("dated"), request.clone(), OrderStatus::Working);
        request.side = OrderSide::Sell;
        request.order_type = OrderType::Limit(105.0);
        request.position_bracket = Some(crate::model::PositionBracket::TakeProfit);
        let exit = Order::new(OrderId::new("dated-exit"), request, OrderStatus::Working);
        let mut trading = Trading::default();
        let colors = crate::theme::Theme::dark().chart;
        for account in ["stocks", "crypto"] {
            trading.set_service(Arc::new(Recorder {
                account,
                ..Recorder::default()
            }));
            assert_eq!(trading.market(&chart, &colors).account, account);
        }
        trading.disconnect();
        assert_eq!(trading.market(&chart, &colors).account, "");
        trading.set_service(Arc::new(Recorder {
            state: vec![
                AccountEvent::Order(order),
                AccountEvent::Order(exit),
                AccountEvent::Position(Position::new("ES", 1.0, 100.0)),
            ],
            ..Recorder::default()
        }));
        let ctx = egui::Context::default();
        trading.poll(&ctx, &mut Toasts::new());
        let mut texts = Vec::new();
        for _ in 0..3 {
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
            texts = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
        }
        assert!(
            texts
                .iter()
                .any(|text| text == "Limit until 2026-12-01 16:00"),
            "{texts:?}"
        );
        assert_eq!(
            texts
                .iter()
                .filter(|text| text.ends_with(" until 2026-12-01 16:00"))
                .count(),
            2,
            "a position exit keeps its expiry beside its bracket name or P&L: {texts:?}"
        );
        let mut harness = egui_kittest::Harness::builder()
            .allow_missing_glyphs()
            .with_size(egui::vec2(1440.0, 900.0))
            .with_context_setup(egui_extras::install_image_loaders)
            .build_ui_state(
                |ui, trading: &mut Trading| {
                    trading.show_account_panel(ui, "ES");
                },
                trading,
            );
        harness.run();
        harness.get_by_label("Orders 2").click();
        harness.run();
        assert!(
            harness
                .output()
                .shapes
                .iter()
                .any(|shape| matches!(&shape.shape,
            egui::Shape::Text(text) if text.galley.text() == "2026-12-01 21:00:00"))
        );
    }

    /// Chart menus open their dialogs without an account panel and send each confirmation once.
    #[test]
    fn chart_menus_open_the_order_and_position_dialogs() {
        use egui_kittest::{Harness, kittest::Queryable};
        for (protect, both, panel) in [
            (false, false, false),
            (true, false, false),
            (true, true, false),
            (false, false, true),
        ] {
            let order = Order::new(
                OrderId::new("7"),
                OrderRequest::new("SYM", OrderSide::Buy, 1.0, OrderType::Limit(95.0)),
                OrderStatus::Working,
            );
            let position = Position::new("SYM", 2.0, 105.0);
            let broker = Arc::new(Recorder {
                brackets: true,
                state: vec![
                    AccountEvent::Order(order.clone()),
                    AccountEvent::Position(position),
                ],
                ..Recorder::default()
            });
            let mut trading = Trading::default();
            trading.set_service(broker.clone());
            trading.poll(&egui::Context::default(), &mut Toasts::new());
            trading.display.label_alignment = crate::chart::TradingLabelAlignment::Left;
            let mut chart = crate::widget::test_frames::chart(&[90.0], &[115.0]);
            chart.symbol = "SYM".into();
            let mut harness = Harness::builder()
                .with_size(egui::vec2(1000.0, 800.0))
                .with_context_setup(egui_extras::install_image_loaders)
                .build_ui_state(
                    |ui, (trading, chart): &mut (Trading, Chart)| {
                        trading.poll(ui.ctx(), &mut Toasts::new());
                        chart.show(ui);
                        trading.show(ui, chart);
                        if panel {
                            trading.show_account_panel(ui, "SYM");
                        }
                    },
                    (trading, chart),
                );
            harness.run();
            broker
                .rules
                .lock()
                .unwrap()
                .pop()
                .expect("symbol rules")
                .1
                .send(Ok(TradingRules::new("SYM")));
            harness.run();
            let name = if protect { "2" } else { "Limit" };
            let point = harness
                .output()
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == name => {
                        Some(text.visual_bounding_rect().center())
                    }
                    _ => None,
                })
                .expect("chart label");
            harness.hover_at(point);
            harness.run_steps(1);
            for pressed in [true, false] {
                harness.event(egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            harness.run();
            let label = if protect {
                "Protect position"
            } else {
                "Modify order"
            };
            harness
                .get_by_role_and_label(egui::Role::Button, label)
                .click();
            harness.run();
            assert!(harness.state().0.pending.is_none());
            if protect {
                harness
                    .get_by_role_and_label(egui::Role::CheckBox, "Take profit")
                    .click();
                harness.run();
                harness
                    .get_by_role_and_label(egui::Role::SpinButton, "Take profit")
                    .set_value_accesskit(110.0);
            } else {
                harness
                    .get_by_role_and_label(egui::Role::SpinButton, "Limit price")
                    .set_value_accesskit(96.0);
            }
            harness.run();
            if both {
                harness
                    .get_by_role_and_label(egui::Role::CheckBox, "Stop loss")
                    .click();
                harness.run();
                harness
                    .get_by_role_and_label(egui::Role::SpinButton, "Stop loss")
                    .set_value_accesskit(100.0);
                harness.run();
            }
            harness
                .get_by_role_and_label(egui::Role::Button, label)
                .click();
            harness.run();
            assert!(harness.state().0.pending.is_none());
            if protect {
                let mut exit =
                    OrderRequest::new("SYM", OrderSide::Sell, 2.0, OrderType::Limit(110.0));
                exit.position_bracket = Some(PositionBracket::TakeProfit);
                let mut expected = vec![exit];
                if both {
                    let mut stop =
                        OrderRequest::new("SYM", OrderSide::Sell, 2.0, OrderType::Stop(100.0));
                    stop.position_bracket = Some(PositionBracket::StopLoss);
                    expected.push(stop);
                }
                assert_eq!(*broker.placed.lock().unwrap(), expected);
                assert!(
                    harness
                        .query_by_role_and_label(egui::Role::SpinButton, "Take profit")
                        .is_none()
                );
            } else {
                let mut changed = order.request;
                changed.order_type = OrderType::Limit(96.0);
                assert_eq!(
                    *broker.modified.lock().unwrap(),
                    [(OrderId::new("7"), changed)]
                );
                assert!(harness.state().0.account_panel.editing.is_none());
            }
        }
    }

    /// An account that takes limit and stop orders but cannot attach them to a position shows
    /// no exit buttons on the position line, disables Protect position with its reason and sends
    /// nothing.
    #[test]
    fn positions_offer_exits_only_with_the_position_exits_capability() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::{NodeT, Queryable};
        for without_position_exits in [false, true] {
            let broker = Arc::new(Recorder {
                brackets: true,
                without_position_exits,
                state: vec![AccountEvent::Position(Position::new("SYM", 2.0, 105.0))],
                ..Recorder::default()
            });
            let mut trading = Trading::default();
            trading.set_service(broker.clone());
            trading.poll(&egui::Context::default(), &mut Toasts::new());
            trading.display.label_alignment = crate::chart::TradingLabelAlignment::Left;
            let mut chart = crate::widget::test_frames::chart(&[90.0], &[115.0]);
            chart.symbol = "SYM".into();
            let mut harness = Harness::builder()
                .with_size(egui::vec2(1000.0, 800.0))
                .with_context_setup(egui_extras::install_image_loaders)
                .build_ui_state(
                    |ui, (trading, chart): &mut (Trading, Chart)| {
                        trading.poll(ui.ctx(), &mut Toasts::new());
                        chart.show(ui);
                        trading.show(ui, chart);
                    },
                    (trading, chart),
                );
            harness.run();
            broker
                .rules
                .lock()
                .unwrap()
                .pop()
                .expect("symbol rules")
                .1
                .send(Ok(TradingRules::new("SYM")));
            harness.run();
            let painted = |harness: &Harness<'_, (Trading, Chart)>, name: &str| {
                harness
                    .output()
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == name => {
                            Some(text.visual_bounding_rect().center())
                        }
                        _ => None,
                    })
            };
            let point = painted(&harness, "2").expect("position label");
            harness.hover_at(point);
            harness.run();
            for button in ["TP", "SL"] {
                assert_eq!(
                    painted(&harness, button).is_some(),
                    !without_position_exits,
                    "{button}"
                );
            }
            for pressed in [true, false] {
                harness.event(egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Secondary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                });
            }
            harness.run();
            let protect = harness.get_by_role_and_label(egui::Role::Button, "Protect position");
            if !without_position_exits {
                assert!(!protect.accesskit_node().is_disabled());
                continue;
            }
            assert!(protect.accesskit_node().is_disabled());
            protect.hover();
            harness.run();
            harness.get_by_label("The account does not support exits on positions");
            // The dialog, however it opens, closes at once and sends nothing.
            let position = Position::new("SYM", 2.0, 105.0);
            harness.state_mut().0.protecting = Some((position, Some(110.0), Some(100.0)));
            harness.run();
            assert!(harness.state().0.protecting.is_none());
            assert!(harness.state().0.pending.is_none());
            assert!(broker.placed.lock().unwrap().is_empty());
        }
    }

    /// Confirmation sentences use template keys and the symbol's quantity text for orders,
    /// levels and positions.
    #[test]
    fn confirmations_reach_the_translator() {
        let ctx = egui::Context::default();
        l10n::mark_keys(&ctx);
        let numbers = |formatter: Option<&'static dyn TradingFormatter>| Numbers {
            symbol: "1301",
            decimals: 2,
            formatter,
        };
        let request = |side, kind| OrderRequest::new("1301", side, 2.0, kind);
        let mut bracketed = request(OrderSide::Buy, OrderType::Limit(101.0));
        bracketed.take_profit = Some(110.0);
        bracketed.stop_loss = Some(95.0);
        let working = Order::new(OrderId::new("7"), bracketed, OrderStatus::Working);
        let mut stopped_levels = request(OrderSide::Buy, OrderType::Limit(101.0));
        stopped_levels.take_profit_levels = vec![
            TakeProfitLevel::new(110.0, 1.0),
            TakeProfitLevel::new(120.0, 1.0),
        ];
        stopped_levels.take_profit_levels[0].stop_loss = Some(96.0);
        stopped_levels.stop_loss = Some(95.0);
        let stopped_levels = Order::new(OrderId::new("8"), stopped_levels, OrderStatus::Working);
        let long = Position::new("1301", 2.0, 100.0);
        let mut levels = OrderRequest::new("1301", OrderSide::Buy, 3_000.0, OrderType::Market);
        levels.take_profit_levels = vec![
            TakeProfitLevel::new(110.0, 1_000.0),
            TakeProfitLevel::new(111.0, 2_000.0),
        ];
        let levels = Order::new(OrderId::new("levels"), levels, OrderStatus::Working);
        for (action, expected) in [
            (
                TradingLineAction::Cancel(stopped_levels),
                "~~2~1301~101.00~~110.00~1~96.00~~120.00~1~~95.00~~",
            ),
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
            (
                TradingLineAction::Cancel(levels.clone()),
                "~~3,000~1301~~110.00~1,000~~111.00~2,000~~",
            ),
            (TradingLineAction::Close(long.clone()), "~~2~1301~~"),
            (
                TradingLineAction::Close(Position::new("1301", 1e-8, 100.0)),
                "~~0.00000001~1301~~",
            ),
            (
                TradingLineAction::Close(Position::new("1301", 1.0000001, 100.0)),
                "~~1.0000001~1301~~",
            ),
            (
                TradingLineAction::Close(Position::new("1301", 0.1 + 0.2, 100.0)),
                "~~0.3~1301~~",
            ),
            // Quantities are grouped as the account tables write them.
            (
                TradingLineAction::Close(Position::new("1301", 2_000.5, 100.0)),
                "~~2,000.5~1301~~",
            ),
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
            let (_, message, _) = describe(&ctx, &action, &numbers(None));
            assert_eq!(message, expected, "{action:?}");
        }
        // The host's quantity text replaces them.
        struct Lots;
        impl TradingFormatter for Lots {
            fn quantity(&self, _: &str, quantity: f64) -> Option<String> {
                Some(format!("{quantity} lots"))
            }
        }
        let reverse = TradingLineAction::Reverse(Position::new("1301", -3.0, 100.0));
        let (_, message, _) = describe(&ctx, &reverse, &numbers(Some(&Lots)));
        assert_eq!(message, "~~3 lots~1301~~");
        let (_, message, _) = describe(
            &ctx,
            &TradingLineAction::Cancel(levels),
            &numbers(Some(&Lots)),
        );
        assert_eq!(
            message,
            "~~3000 lots~1301~~110.00~1000 lots~~111.00~2000 lots~~"
        );
    }

    /// Rows of differently formatted symbols keep their own price decimals, host text and pip
    /// value instead of the chart symbol's decimals or a fixed two. A regression writes every
    /// symbol with the chart's decimals, ignores the host's formatter in the tables or on the
    /// chart's order and position lines, loses the pip value or shows
    /// another symbol's logo. A row's cancellation is confirmed in the order's own decimals too.
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
        // The chart's instrument describes its symbol even under another name.
        chart.set_instrument(Some(
            Instrument::new("X:USDJPY")
                .with_pip_size(0.25)
                .with_logo("bytes://x.svg"),
        ));
        let market = trading.market(&chart, &colors);
        assert_eq!(
            (market.logo, market.pip_value),
            (Some("bytes://x.svg"), Some(250.0))
        );
        chart.set_instrument(None);
        trading.rules.pip_value = Some(0.5);
        assert_eq!(trading.market(&chart, &colors).pip_value, Some(0.5));
        chart.set_symbol("UNKNOWN");
        trading.rules = TradingRules::new("UNKNOWN");
        assert_eq!(trading.market(&chart, &colors).pip_value, None);

        // A row of the account panel asks to cancel an order of another symbol.
        let order = Order::new(
            OrderId::new("1"),
            OrderRequest::new("USDJPY", OrderSide::Buy, 1.0, OrderType::Limit(151.234)),
            OrderStatus::Working,
        );
        trading.pending = Some(TradingLineAction::Cancel(order));
        let ctx = egui::Context::default();
        let mut texts = Vec::new();
        // A new window is measured in its first frame and drawn in the next.
        for _ in 0..2 {
            let output = ctx.run_ui(egui::RawInput::default(), |ui| trading.show(ui, &mut chart));
            texts = (output.shapes.iter())
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
        }
        let asked = "Cancel the order to BUY 1 USDJPY limit 151.234?";
        assert!(texts.iter().any(|text| text == asked), "{texts:?}");

        trading.pending = None;
        chart.set_symbol("BTC");
        trading.orders = vec![Order::new(
            OrderId::new("btc"),
            OrderRequest::new("BTC", OrderSide::Buy, 2.0, OrderType::Limit(95.0)),
            OrderStatus::Working,
        )];
        trading.positions = vec![Position::new("BTC", 3.0, 105.0)];
        for _ in 0..2 {
            let output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1000.0, 800.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    chart.show(ui);
                    trading.draw_overlays(ui, &chart);
                },
            );
            texts = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
        }
        assert!(texts.iter().any(|text| text == "2 coins"), "{texts:?}");
        assert!(texts.iter().any(|text| text == "3 coins"), "{texts:?}");
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

    // One test per width: the cases take minutes, and separate tests share them across threads.
    #[test]
    fn line_changes_wait_for_confirmation_and_send_one_call_at_800_points() {
        line_changes_wait_for_confirmation_and_send_one_call(800.0);
    }

    #[test]
    fn line_changes_wait_for_confirmation_and_send_one_call_at_320_points() {
        line_changes_wait_for_confirmation_and_send_one_call(320.0);
    }

    #[test]
    fn line_changes_wait_for_confirmation_and_send_one_call_at_260_points() {
        line_changes_wait_for_confirmation_and_send_one_call(260.0);
    }

    /// Line gestures wait for the actual confirmation button and send one broker call.
    fn line_changes_wait_for_confirmation_and_send_one_call(width: f32) {
        use crate::chart::TradingLabelAlignment::{Center, Left, Right};
        use crate::theme::{Theme, apply_to_egui};
        for theme in [Theme::light(), Theme::dark()] {
            for alignment in [Left, Center, Right] {
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
                        ("Modify order", false, true, "level stop drag"),
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
                        if gesture.starts_with("levels") || gesture == "level stop drag" {
                            request.take_profit_levels = vec![
                                TakeProfitLevel::new(102.0, 0.5),
                                TakeProfitLevel::new(101.0, 0.5),
                            ];
                            request.take_profit_levels[1].stop_loss = Some(107.0);
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
                            // Close asks for its quantity and places the prefilled whole position.
                            partial_close: operation == "Close position",
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
                        let y = mapping.price_to_y(if gesture == "level stop drag" {
                            107.0
                        } else if matches!(gesture, "levels drag" | "levels remove") {
                            101.0
                        } else if operation.ends_with("order") {
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
                        } else if gesture == "level stop drag" {
                            egui::pos2(x, mapping.price_to_y(106.5))
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
                            if operation == "Close position" {
                                assert!(
                                    find(
                                        frame(&mut trading, &mut chart, vec![]),
                                        "Quantity to close"
                                    )
                                    .is_some(),
                                    "a line's Close asks for the quantity at {width}"
                                );
                            }
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
                                        expected.take_profit_levels[1].stop_loss = Some(105.07);
                                        expected.stop_loss = Some(stop_loss);
                                    }
                                    "level stop drag" => {
                                        expected.take_profit_levels[1].stop_loss = Some(106.5)
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
        assert!(
            !defaults.show_execution_prices,
            "older settings keep fill prices off"
        );
        assert!(!TradingSettings::default().show_execution_prices);
        assert!(TradingSettings::default().show_execution_labels);
        assert!(
            !defaults.compact_ticket,
            "older settings keep the full ticket"
        );
        assert!(
            !defaults.separate_labels,
            "older settings keep labels at their prices"
        );
        assert!(
            !defaults.undocked_ticket,
            "older settings keep the ticket docked"
        );
        assert!(!TradingSettings::default().undocked_ticket);
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
                    trading.display.separate_labels = extended;
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
        struct Lots;
        impl TradingFormatter for Lots {
            fn quantity(&self, _: &str, quantity: f64) -> Option<String> {
                Some(format!("{} lots", quantity / 100.0))
            }
        }
        trading.set_formatter(Some(Arc::new(Lots)));
        frame(&mut trading, &mut chart, vec![]);
        let texts = frame(&mut trading, &mut chart, vec![]);
        assert!(find(&texts, "0.25 lots").is_some());
    }

    /// A broker can trade from a last-price feed; a valid bid and ask still take precedence.
    #[test]
    fn buy_and_sell_show_the_last_price_without_a_two_sided_quote() {
        use crate::model::Level1Quote;
        use crate::widget::QuoteSide;
        for panel in [false, true] {
            for (bid_ask, broker, expected) in [
                (None, true, Some("110.00")),
                (Some((101.0, 99.0)), true, Some("110.00")),
                (Some((f64::NAN, 101.0)), true, Some("110.00")),
                (Some((99.0, f64::INFINITY)), true, Some("110.00")),
                (Some((99.0, 101.0)), true, Some("101.00")),
                (None, false, None),
            ] {
                let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
                chart.symbol = "SYM".into();
                chart.config_mut().quote_panel = panel;
                chart.set_quote(bid_ask.map(|(bid, ask)| Level1Quote {
                    symbol: "SYM".into(),
                    ts: chart.bars(SeriesId::MAIN)[99].time,
                    bid,
                    ask,
                    bid_size: 1.0,
                    ask_size: 1.0,
                }));
                let mut trading = Trading::default();
                if broker {
                    trading.set_service(Arc::new(Recorder::default()));
                }
                let ctx = egui::Context::default();
                let frame = |trading: &mut Trading, chart: &mut Chart, events| {
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
                                text.pos + text.galley.size() / 2.0,
                            )),
                            _ => None,
                        })
                        .collect();
                    output.drop_without_applying_deltas();
                    texts
                };
                frame(&mut trading, &mut chart, vec![]);
                // Trading supplies the fallback on the next frame; the panel then measures it.
                frame(&mut trading, &mut chart, vec![]);
                let texts = frame(&mut trading, &mut chart, vec![]);
                let buy = texts
                    .iter()
                    .find(|(text, _)| text == "BUY")
                    .map(|(_, at)| *at);
                let price = buy
                    .and_then(|buy| {
                        texts.iter().find(|(_, at)| {
                            (at.x - buy.x).abs() < 1.0 && at.y < buy.y && buy.y - at.y < 34.0
                        })
                    })
                    .map(|(text, _)| text.as_str());
                assert_eq!(price, expected, "quote {bid_ask:?}, broker {broker}");
                if bid_ask.is_none() && broker {
                    let at = buy.unwrap();
                    for events in [
                        vec![egui::Event::PointerMoved(at)],
                        vec![egui::Event::PointerButton {
                            pos: at,
                            button: egui::PointerButton::Primary,
                            pressed: true,
                            modifiers: Default::default(),
                        }],
                        vec![egui::Event::PointerButton {
                            pos: at,
                            button: egui::PointerButton::Primary,
                            pressed: false,
                            modifiers: Default::default(),
                        }],
                        vec![],
                    ] {
                        frame(&mut trading, &mut chart, events);
                    }
                    assert_eq!(chart.take_quote_action(), Some(QuoteSide::Buy));
                }
            }
        }
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

    /// Guards the quantity a partial close sends. With `Capabilities::partial_close` the account
    /// panel's Close asks how much to close, prefilled with the whole position, and sends the
    /// opposite market order for the typed quantity rounded down to the quantity step; below the
    /// minimum or above the position it sends nothing and says why. The whole position closes
    /// even above the symbol's maximum and before its rules arrive. Without the capability the
    /// confirmation asks no quantity and closes in full. A regression sends the whole position
    /// for a part, an unrounded or too small quantity or more than the position, or blocks a
    /// full close while the rules are missing.
    #[test]
    fn a_partial_close_sends_the_typed_quantity_in_the_symbols_steps() {
        use crate::model::TimeInForce;
        use egui_kittest::{Harness, kittest::Queryable};
        struct Lots;
        impl TradingFormatter for Lots {
            fn quantity(&self, symbol: &str, quantity: f64) -> Option<String> {
                (symbol == "SYM").then(|| format!("{quantity} lots"))
            }
        }
        // (partial close, rules answered, host text, typed quantity, message shown, quantity placed)
        type Row = (
            bool,
            bool,
            bool,
            Option<&'static str>,
            Option<&'static str>,
            Option<f64>,
        );
        let below = "The quantity is below the minimum of 1.0";
        let rows: [Row; 10] = [
            (true, true, false, None, None, Some(3.0)),
            (true, true, false, Some("2.7"), None, Some(2.5)),
            (true, true, false, Some("1"), None, Some(1.0)),
            (true, true, false, Some("0.9"), Some(below), None),
            (
                true,
                true,
                false,
                Some("3.5"),
                Some("The quantity is above the maximum of 3.0"),
                None,
            ),
            (
                true,
                false,
                false,
                None,
                Some("Waiting for the symbol's trading rules"),
                Some(3.0),
            ),
            (false, true, false, None, None, Some(3.0)),
            (true, true, true, Some("2.7"), None, Some(2.5)),
            (
                true,
                true,
                true,
                Some("0.9"),
                Some("The quantity is below the minimum of 1 lots"),
                None,
            ),
            (
                true,
                true,
                true,
                Some("3.5"),
                Some("The quantity is above the maximum of 3 lots"),
                None,
            ),
        ];
        let mut rules = TradingRules::new("SYM");
        rules.quantity_step = 0.5;
        rules.min_quantity = 1.0;
        rules.max_quantity = Some(2.5);
        rules.time_in_force = vec![TimeInForce::GoodTillCancelled];
        for (partial_close, ready, host, typed, message, placed) in rows {
            let broker = Arc::new(Recorder {
                state: vec![AccountEvent::Position(Position::new("SYM", 3.0, 100.0))],
                partial_close,
                ..Recorder::default()
            });
            let answer = || {
                for (_, reply) in std::mem::take(&mut *broker.rules.lock().unwrap()) {
                    reply.send(Ok(rules.clone()));
                }
            };
            let mut trading = Trading::default();
            trading.set_service(broker.clone());
            if host {
                trading.set_formatter(Some(Arc::new(Lots)));
            }
            trading.poll(&egui::Context::default(), &mut Toasts::new());
            trading.account_panel.broker = "Broker".into();
            let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
            chart.symbol = "SYM".into();
            let mut harness = Harness::builder()
                .with_size(egui::vec2(1000.0, 600.0))
                .with_context_setup(egui_extras::install_image_loaders)
                .build_ui_state(
                    |ui, (trading, chart, panel): &mut (Trading, Chart, bool)| {
                        trading.poll(ui.ctx(), &mut Toasts::new());
                        if *panel {
                            trading.show_account_panel(ui, "SYM");
                        }
                        trading.show(ui, chart);
                    },
                    (trading, chart, true),
                );
            harness.run();
            if ready {
                answer();
                harness.run();
            }
            // The row's button: the only one so named until the dialog opens.
            harness.get_by_label("Close position").click();
            harness.run();
            // The row's button would share its name with the dialog's.
            harness.state_mut().2 = false;
            harness.run();
            let field = (egui::Role::TextInput, "Quantity to close");
            let asks = harness.query_by_role_and_label(field.0, field.1).is_some();
            assert_eq!(asks, partial_close, "{typed:?}");
            if let Some(text) = typed {
                harness.get_by_role_and_label(field.0, field.1).focus();
                harness.run();
                harness.key_press_modifiers(egui::Modifiers::COMMAND, egui::Key::A);
                harness
                    .get_by_role_and_label(field.0, field.1)
                    .type_text(text);
                harness.run();
            }
            if let Some(message) = message {
                harness.get_by_label(message);
            }
            harness
                .get_by_role_and_label(egui::Role::Button, "Close position")
                .click();
            harness.run();
            if !ready {
                answer();
                harness.run();
            }
            let expected: Vec<_> = (placed.into_iter())
                .map(|quantity| {
                    let mut order =
                        OrderRequest::new("SYM", OrderSide::Sell, quantity, OrderType::Market);
                    order.time_in_force = TimeInForce::GoodTillCancelled;
                    order
                })
                .collect();
            assert_eq!(*broker.placed.lock().unwrap(), expected, "{typed:?}");
            assert_eq!(
                harness.state().0.pending.is_some(),
                placed.is_none(),
                "a refused quantity keeps the dialog open: {typed:?}"
            );
        }
    }

    /// Guards the exact quantity of a partial close at fine steps: a part one step below a large
    /// position stays that part, and the position's size read back from the field's text closes
    /// all of it. A regression closes the whole position for a part, or leaves a speck of it.
    #[test]
    fn a_part_one_fine_step_below_the_position_closes_only_that_part() {
        let mut rules = TradingRules::new("SYM");
        for (step, size, requested, closed) in [
            (1e-8, 100.0, 99.999_999_99, 99.999_999_99),
            (1.0, 1e9, 999_999_999.0, 999_999_999.0),
            (0.1, 0.1 + 0.2, 0.3, 0.1 + 0.2),
        ] {
            rules.quantity_step = step;
            rules.min_quantity = step;
            assert_eq!(
                close_part(requested, size, &rules),
                Ok(closed),
                "{requested}"
            );
        }
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

    /// Buy and Sell take the selected preset, including exits, and keep a non-market or partly
    /// applicable preset in the ticket for review even when instant orders are enabled.
    #[test]
    fn buy_and_sell_take_the_applied_preset_or_open_it_in_the_ticket() {
        use crate::ui::trading::{Bracket, BracketMode, OrderKind, OrderPreset};
        for side in [OrderSide::Buy, OrderSide::Sell] {
            for (instant, brackets, limit) in [
                (true, true, false),
                (false, true, false),
                (true, true, true),
                (true, false, false),
            ] {
                let ctx = egui::Context::default();
                let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
                chart.symbol = "SYM".into();
                let last = crate::model::last_with_value(chart.bars(SeriesId::MAIN))
                    .unwrap()
                    .close;
                let broker = Arc::new(Recorder {
                    brackets,
                    ..Recorder::default()
                });
                let mut trading = Trading::default();
                trading.set_service(broker.clone());
                trading.poll(&ctx, &mut Toasts::new());
                let frame = |trading: &mut Trading, chart: &mut Chart| {
                    ctx.run_ui(egui::RawInput::default(), |ui| {
                        chart.show(ui);
                        trading.show(ui, chart);
                    })
                    .drop_without_applying_deltas();
                };
                frame(&mut trading, &mut chart);
                broker
                    .rules
                    .lock()
                    .unwrap()
                    .pop()
                    .unwrap()
                    .1
                    .send(Ok(TradingRules::new("SYM")));
                trading.poll(&ctx, &mut Toasts::new());
                trading.display.instant_orders = instant;
                trading.quantity = 1.0;
                let mut preset = OrderPreset::new("Swing", 3.0);
                preset.take_profit = Some(Bracket::new(BracketMode::Ticks, 20.0));
                preset.stop_loss = Some(Bracket::new(BracketMode::Ticks, 10.0));
                preset.applied = true;
                if limit {
                    preset.order_type = Some(OrderKind::Limit);
                    preset.price_ticks = Some(-5.0);
                }
                trading.presets.insert(
                    trading.account_id.clone(),
                    vec![OrderPreset::new("Scalp", 2.0), preset],
                );
                trading.trade("SYM".into(), side);
                frame(&mut trading, &mut chart);
                let placed = broker.placed.lock().unwrap();
                let sends = instant && brackets && !limit;
                assert_eq!(
                    placed.len(),
                    usize::from(sends),
                    "{side:?}, {instant}, {brackets}, {limit}"
                );
                assert_eq!(trading.ticket.open, !sends);
                if !brackets {
                    continue;
                }
                let colors = crate::theme::Theme::dark().chart;
                let market = trading.market(&chart, &colors);
                let request = if sends {
                    placed[0].clone()
                } else {
                    trading.ticket.request(&ctx, &market).unwrap()
                };
                assert_eq!(request.quantity, 3.0);
                assert_eq!(request.side, side);
                let sign = if side == OrderSide::Buy { 1.0 } else { -1.0 };
                let entry = if limit { last - sign * 0.05 } else { last };
                match request.order_type {
                    OrderType::Market => assert!(!limit),
                    OrderType::Limit(price) => assert!((price - entry).abs() < 1e-9),
                    kind => panic!("unexpected type {kind:?}"),
                }
                // 20 and 10 ticks at 0.01 are 0.20 and 0.10 from the entry, mirrored for sells.
                assert!((request.take_profit.unwrap() - (entry + sign * 0.20)).abs() < 1e-9);
                assert!((request.stop_loss.unwrap() - (entry - sign * 0.10)).abs() < 1e-9);
            }
        }
    }

    /// The window gives the chart its width back, sends the panel's request unchanged and
    /// restores its saved choice. The header can dock it again without losing the ticket.
    #[test]
    fn an_undocked_ticket_leaves_the_chart_its_width_and_places_the_same_order() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::{NodeT, Queryable};

        let host = |trading, chart| {
            Harness::builder()
                .with_accessibility_check(false)
                .with_size(egui::vec2(1000.0, 700.0))
                .with_context_setup(egui_extras::install_image_loaders)
                .build_ui_state(
                    |ui, (trading, chart, left): &mut (Trading, Chart, egui::Rect)| {
                        trading.poll(ui.ctx(), &mut Toasts::new());
                        trading.show_ticket(ui, chart);
                        *left = ui.available_rect_before_wrap();
                    },
                    (trading, chart, egui::Rect::NOTHING),
                )
        };
        for undock in [false, true] {
            let broker = Arc::new(Recorder::default());
            let mut trading = Trading::default();
            trading.set_service(broker.clone());
            trading.poll(&egui::Context::default(), &mut Toasts::new());
            let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
            chart.symbol = "SYM".into();
            let request = OrderRequest::new("SYM", OrderSide::Buy, 3.0, OrderType::Limit(100.0));
            let mut h = host(trading, chart);
            h.run();
            let full = h.state().2;
            h.state_mut().0.open_ticket(request.clone());
            h.run();
            broker
                .rules
                .lock()
                .unwrap()
                .pop()
                .expect("symbol rules")
                .1
                .send(Ok(TradingRules::new("SYM")));
            h.run();
            if undock {
                h.get_by_label("Undock ticket").click();
                h.run();
                assert!(h.state().0.display.undocked_ticket);
                assert!(
                    (h.state().2.right() - full.right()).abs() < 0.5,
                    "the window leaves the chart its width"
                );
            } else {
                assert!((full.right() - h.state().2.right() - 340.0).abs() < 0.5);
            }
            h.query_all_by_role(egui::Role::Button)
                .find(|node| {
                    node.accesskit_node()
                        .label()
                        .is_some_and(|label| label.starts_with("Buy "))
                })
                .expect("send button")
                .click();
            h.run();
            assert_eq!(*broker.placed.lock().unwrap(), [request.clone()]);
            if undock {
                let saved = serde_json::to_string(&h.state().0.display).unwrap();
                let mut restored = Trading::default();
                restored.set_service(broker.clone());
                restored.poll(&egui::Context::default(), &mut Toasts::new());
                restored.display = serde_json::from_str(&saved).unwrap();
                restored.open_ticket(request);
                let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
                chart.symbol = "SYM".into();
                let mut h = host(restored, chart);
                h.run();
                broker
                    .rules
                    .lock()
                    .unwrap()
                    .pop()
                    .expect("restored rules")
                    .1
                    .send(Ok(TradingRules::new("SYM")));
                h.run();
                assert!((h.state().2.right() - full.right()).abs() < 0.5);
                assert!(h.output().shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text() == "SYM")
                }));
                h.get_by_label("Dock ticket").click();
                h.run();
                assert!(!h.state().0.display.undocked_ticket);
                assert!((full.right() - h.state().2.right() - 340.0).abs() < 0.5);
            }
        }
    }

    /// The More menu keeps the compact choice in host settings. Both forms explain an unusable
    /// preset without showing a non-finite exit price; compact orders keep their visible exits.
    #[test]
    fn the_more_menu_switches_to_a_compact_ticket_and_keeps_the_choice() {
        use crate::ui::trading::{Bracket, BracketMode, OrderPreset};
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
            chart.symbol = "SYM".into();
            let broker = Arc::new(Recorder {
                brackets: true,
                state: vec![AccountEvent::Account(Account::new(
                    "USD",
                    0.0,
                    Capabilities {
                        market: true,
                        limit: true,
                        brackets: true,
                        take_profit_levels: true,
                        take_profit_level_stops: true,
                        preview: true,
                        ..Capabilities::default()
                    },
                ))],
                ..Recorder::default()
            });
            let mut trading = Trading::default();
            trading.set_service(broker.clone());
            trading.poll(&ctx, &mut Toasts::new());
            trading.open_ticket(OrderRequest::new(
                "SYM",
                OrderSide::Buy,
                1.0,
                OrderType::Limit(100.0),
            ));
            let (_, reply) = broker.rules.lock().unwrap().pop().unwrap();
            reply.send(Ok(TradingRules::new("SYM")));
            trading.poll(&ctx, &mut Toasts::new());
            let mut preset = OrderPreset::new("Balance stop", 1.0);
            preset.stop_loss = Some(Bracket::new(BracketMode::BalancePercent, 1.0));
            trading
                .presets
                .entry(trading.account_id.clone())
                .or_default()
                .push(preset);
            let frame = |trading: &mut Trading, events| {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(340.0, 1200.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| trading.show_ticket(ui, &chart),
                );
                let more = output.platform_output.events.iter().any(|event| {
                    matches!(event, egui::output::OutputEvent::FocusGained(info) if info.label.as_deref() == Some("More"))
                });
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
                (texts, more)
            };
            frame(&mut trading, vec![]);
            frame(&mut trading, vec![]);
            let key = |key| {
                vec![egui::Event::Key {
                    key,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Default::default(),
                }]
            };
            let mut focused = false;
            for _ in 0..30 {
                if frame(&mut trading, key(egui::Key::Tab)).1 {
                    focused = true;
                    break;
                }
            }
            assert!(focused, "More is reachable from the keyboard");
            frame(&mut trading, key(egui::Key::Enter));
            let click = |trading: &mut Trading, wanted: &str| {
                let texts = frame(trading, vec![]).0;
                let at = texts
                    .iter()
                    .find(|(text, _)| text == wanted)
                    .unwrap_or_else(|| panic!("{wanted}: {texts:?}"))
                    .1;
                for events in [
                    vec![egui::Event::PointerMoved(at)],
                    vec![egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: Default::default(),
                    }],
                    vec![egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Primary,
                        pressed: false,
                        modifiers: Default::default(),
                    }],
                    vec![],
                ] {
                    frame(trading, events);
                }
            };
            click(&mut trading, "Balance stop");
            let reason = "An exit in % of the balance needs the account's balance";
            let texts = frame(&mut trading, vec![]).0;
            for wanted in [
                "Limit",
                "Price",
                "Units",
                "Exits",
                "Risk, % balance",
                "Stop loss, % balance",
                "Extra settings",
                "Order info",
                reason,
            ] {
                assert!(
                    texts.iter().any(|(text, _)| text == wanted),
                    "{wanted}: {texts:?}"
                );
            }
            assert!(!texts.iter().any(|(text, _)| text.contains("NaN")));
            click(&mut trading, "Compact form");
            assert!(trading.display.compact_ticket);
            frame(&mut trading, key(egui::Key::Escape));
            let texts = frame(&mut trading, vec![]).0;
            for wanted in ["Limit", "Price", "Units", reason] {
                assert!(
                    texts.iter().any(|(text, _)| text == wanted),
                    "{wanted}: {texts:?}"
                );
            }
            for hidden in [
                "Exits",
                "Risk, % balance",
                "Stop loss, % balance",
                "Extra settings",
                "Order info",
            ] {
                assert!(
                    !texts.iter().any(|(text, _)| text == hidden),
                    "{hidden}: {texts:?}"
                );
            }
            assert!(!texts.iter().any(|(text, _)| text.contains("NaN")));
            // A valid order lists its exits, including each partial target, in the compact form.
            let mut order = OrderRequest::new("SYM", OrderSide::Buy, 2.0, OrderType::Limit(100.0));
            order.take_profit_levels = vec![
                TakeProfitLevel::new(110.0, 1.0),
                TakeProfitLevel::new(120.0, 1.0),
            ];
            order.take_profit_levels[1].stop_loss = Some(95.0);
            order.stop_loss = Some(90.0);
            trading.open_ticket(order.clone());
            frame(&mut trading, vec![]);
            let texts = frame(&mut trading, vec![]).0;
            for wanted in [
                "Take profit 110.00 for 1",
                "Take profit 120.00 for 1 with a stop loss at 95.00",
                "Stop loss 90.00",
                "Buy",
                "Preview order",
            ] {
                assert!(
                    texts.iter().any(|(text, _)| text == wanted),
                    "{wanted}: {texts:?}"
                );
            }
            let market = trading.market(&chart, &theme.chart);
            assert_eq!(trading.ticket.request(&ctx, &market), Ok(order));
        }
    }

    /// The order info sets the margin against the available funds the broker reports, giving
    /// back the margin of the order being modified, or against the balance without them, and
    /// shows the margin in the error colour once it exceeds that figure. A regression compares
    /// with the balance while positions hold most of it, or warns on every change to a working
    /// order.
    #[test]
    fn the_ticket_sets_margin_against_available_funds() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            // 500 at the limit of 110 are worth 55,000, so 5,500 of margin at 10:1.
            for (funds, editing, expected, short) in [
                (Some(2_000.0), false, "5,500.00 / 2,000.00", true),
                (None, false, "5,500.00 / 100,000.00", false),
                // The working order holds the same 5,500, which modifying it frees.
                (Some(2_000.0), true, "5,500.00 / 7,500.00", false),
            ] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut chart = crate::widget::test_frames::chart(&[90.0], &[110.0]);
                chart.symbol = "SYM".into();
                let request =
                    OrderRequest::new("SYM", OrderSide::Buy, 500.0, OrderType::Limit(110.0));
                let order = Order::new(OrderId::new("1"), request.clone(), OrderStatus::Working);
                let mut account = Account::new(
                    "USD",
                    100_000.0,
                    Capabilities {
                        market: true,
                        limit: true,
                        modify: true,
                        ..Capabilities::default()
                    },
                );
                account.available_funds = funds;
                let broker = Arc::new(Recorder {
                    state: vec![
                        AccountEvent::Account(account),
                        AccountEvent::Order(order.clone()),
                    ],
                    ..Recorder::default()
                });
                let mut trading = Trading::default();
                trading.set_service(broker.clone());
                trading.poll(&ctx, &mut Toasts::new());
                trading.open_ticket(request);
                let (_, reply) = broker.rules.lock().unwrap().pop().unwrap();
                let mut rules = TradingRules::new("SYM");
                rules.leverage = Some(10.0);
                reply.send(Ok(rules));
                trading.poll(&ctx, &mut Toasts::new());
                if editing {
                    trading.ticket.edit(&order);
                }
                let mut color = None;
                for _ in 0..2 {
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                egui::vec2(340.0, 1200.0),
                            )),
                            ..Default::default()
                        },
                        |ui| trading.show_ticket(ui, &chart),
                    );
                    color = output.shapes.iter().find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.text() == expected => {
                            Some(text.galley.job.sections[0].format.color)
                        }
                        _ => None,
                    });
                    output.drop_without_applying_deltas();
                }
                assert_eq!(
                    color,
                    Some(if short { theme.ui.error } else { theme.ui.text }),
                    "{expected} with funds {funds:?}, editing {editing}, dark {}",
                    theme.dark
                );
            }
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
        assert_eq!(
            messages,
            [
                "The broker disconnected: orders are not sent until it is back",
                "Order refused: Sell 2 OTHER at market: the broker is not connected",
                "Order refused: Buy 1 SYM at market: the broker is not connected",
                "Cancel refused: Buy 1 SYM limit 95.00: the broker is not connected",
                "Change refused: Buy 1 SYM limit 95.00: the broker is not connected",
            ]
        );

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
        assert_eq!(toasts.toasts().len(), 6, "the broker accepted them");
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
