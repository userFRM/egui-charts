//! The order ticket's Order tab: the quote, the order kinds, the quantity and price fields, and the
//! exits.

use super::{
    Bracket, BracketMode, Kind, Market, OrderTicket, PresetLevel, Sizing, TrailUnit, exit_lines,
    levels_accepted, reward, risk,
};
use crate::chart::{COVERAGE_CELL, coverage_text, entry_price, exit_coverage, paint_coverage};
use crate::icons::icons;
use crate::localization as l10n;
use crate::model::{OrderRequest, OrderSide, OrderType, Position, PositionBracket, TimeInForce};
use crate::scales::decimals;
use crate::theme::ThemeExt;
use crate::ui::top_toolbar::buttons::rule_color;
use crate::ui::trading::widgets::{
    FIELD_HEIGHT, SECTION_TITLE_SIZE, chevron, date_time, dropdown, field, grouped, hit, info,
    muted, number, side_color, swap, text, time_in_force_menu, title, toggle, track,
};
use crate::ui_kit::IconButton;
use crate::ui_kit::buttons::{Button, ButtonSize};
use egui::{Align, Align2, FontId, Layout, Rect, RichText, Sense, Ui, pos2, vec2};

/// Quote prices sit between the heading and dialog-title text roles.
const QUOTE_PRICE_SIZE: f32 = 17.0;

impl OrderTicket {
    /// A trailing stop's distance with its unit, and its optional activation price, which
    /// starts at `touch` when it is switched on.
    fn trailing_fields(&mut self, ui: &mut Ui, market: &Market<'_>, touch: Option<f64>) {
        const UNITS: [(TrailUnit, &str); 3] = [
            (TrailUnit::Price, "Price"),
            (TrailUnit::Ticks, "Ticks"),
            (TrailUnit::Percent, "%"),
        ];
        muted(ui, "Trailing distance");
        let places = match self.trail.0 {
            TrailUnit::Price => market.decimals,
            TrailUnit::Ticks => 0,
            TrailUnit::Percent => 2,
        };
        field(ui, |ui| {
            number(ui, "Trailing distance", &mut self.trail.1, places);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                let (_, caption) = UNITS[UNITS
                    .iter()
                    .position(|(unit, _)| *unit == self.trail.0)
                    .unwrap_or_default()];
                let response = dropdown(ui, caption.to_owned());
                egui::Popup::menu(&response).show(|ui| {
                    for (unit, label) in UNITS {
                        ui.selectable_value(&mut self.trail.0, unit, l10n::text(ui.ctx(), label));
                    }
                });
            });
        });
        let (on, price) = &mut self.activation;
        ui.horizontal(|ui| {
            toggle(ui, on, "Activation price");
            muted(ui, "Activation price");
        });
        if *on {
            if *price == 0.0 {
                *price = touch.unwrap_or_default();
            }
            field(ui, |ui| {
                number(ui, "Activation price", price, market.decimals);
            });
        }
    }

    /// The Order tab above the send button.
    pub(super) fn form(&mut self, ui: &mut Ui, market: &Market<'_>, compact: bool) {
        let theme = ui.active_theme();
        self.quote(ui, market);
        self.kinds(ui, market);
        if let Some(entry) = self.entry(market) {
            for (bracket, profit) in [(&mut self.take_profit, true), (&mut self.stop_loss, false)] {
                if !bracket.0 && bracket.1.mode == BracketMode::Price && bracket.1.value == 0.0 {
                    bracket.1.value = market
                        .grid()
                        .offset(entry, Bracket::sign(self.side, profit) * 100.0);
                }
            }
        }
        let request = self.request(ui.ctx(), market);
        let entry = self.entry(market).map(|price| market.grid().snap(price));
        let rules = market.rules;
        let quantity = request
            .as_ref()
            .map_or(self.quantity, |request| request.quantity);
        let per_point = quantity * rules.point_value;
        let currency = market.currency;
        let numbers = market.numbers();

        // Prices of limit, stop and stop-limit orders.
        let prices: &[(&str, bool)] = match self.kind {
            Kind::Market => &[],
            Kind::Limit | Kind::Stop => &[("Price", false)],
            Kind::StopLimit => &[("Stop price", false), ("Limit price", true)],
            Kind::TrailingStop => &[],
        };
        let touch = self.reference(market);
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
                        let mut ticks = market.grid().steps_between(touch, *price);
                        if number(ui, (label, "ticks"), &mut ticks, 0).changed() {
                            *price = market.grid().offset(touch, ticks);
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
                            numbers.price(*price)
                        } else {
                            let reference = match (self.side, market.ask, market.bid) {
                                (OrderSide::Buy, Some(_), _) => "Ask",
                                (OrderSide::Sell, _, Some(_)) => "Bid",
                                _ => "Last",
                            };
                            let ticks = market.grid().steps_between(touch, *price);
                            let key = match (reference, ticks < 0.0) {
                                ("Ask", false) => "Ask + {ticks}",
                                ("Ask", true) => "Ask - {ticks}",
                                ("Bid", false) => "Bid + {ticks}",
                                ("Bid", true) => "Bid - {ticks}",
                                (_, false) => "Last + {ticks}",
                                (_, true) => "Last - {ticks}",
                            };
                            match ticks {
                                0.0 => reference.to_owned(),
                                ticks => l10n::format_text(
                                    ui.ctx(),
                                    key,
                                    &[("ticks", ticks.abs().to_string())],
                                ),
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
        if self.kind == Kind::TrailingStop {
            self.trailing_fields(ui, market, touch);
        }

        // The quantity, and the risk at the stop loss.
        let balance = market.positive_balance();
        let risk_money = request
            .as_ref()
            .ok()
            .and_then(|request| Some(risk(request, entry?)? * rules.point_value));
        let units = |ticket: &mut Self, ui: &mut Ui| {
            let caption = match ticket.sizing {
                Sizing::Value => {
                    l10n::format_text(ui.ctx(), "Amount, {currency}", &[("currency", currency)])
                }
                Sizing::Balance => "Amount, % balance".to_owned(),
                Sizing::Units | Sizing::Risk => "Units".to_owned(),
            };
            let response = dropdown(ui, caption);
            let mut sizing = ticket.sizing;
            egui::Popup::menu(&response).show(|ui| {
                ui.selectable_value(&mut sizing, Sizing::Units, l10n::text(ui.ctx(), "Units"));
                ui.selectable_value(
                    &mut sizing,
                    Sizing::Value,
                    l10n::format_text(
                        ui.ctx(),
                        "Amount, {currency}",
                        &[("currency", currency.to_string())],
                    ),
                );
                let share = ui.add_enabled(
                    balance.is_some(),
                    egui::Button::selectable(
                        sizing == Sizing::Balance,
                        l10n::text(ui.ctx(), "Amount, % balance"),
                    ),
                );
                if share.clicked() {
                    sizing = Sizing::Balance;
                }
                share.on_disabled_hover_text(l10n::text(ui.ctx(), "Needs the account's balance"));
            });
            if sizing != ticket.sizing {
                ticket.resize(ui.ctx(), market, sizing, ticket.risk_in_money);
            }
        };
        let quantity_field = |ticket: &mut Self, ui: &mut Ui, suffix: bool| {
            let mut swapped = false;
            field(ui, |ui| {
                if matches!(ticket.sizing, Sizing::Value | Sizing::Balance) {
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
                            &if matches!(ticket.sizing, Sizing::Value | Sizing::Balance) {
                                l10n::format_text(
                                    ui.ctx(),
                                    "{quantity} units",
                                    &[(
                                        "quantity",
                                        numbers
                                            .quantity(quantity, decimals(rules.quantity_step))
                                            .as_str(),
                                    )],
                                )
                            } else {
                                format!("{} {currency}", grouped(entry * per_point, 2, false))
                            },
                        );
                    }
                });
            });
            if swapped {
                // The order keeps its size in the new unit.
                let sizing = if matches!(ticket.sizing, Sizing::Value | Sizing::Balance) {
                    Sizing::Units
                } else {
                    Sizing::Value
                };
                ticket.resize(ui.ctx(), market, sizing, ticket.risk_in_money);
            }
        };
        if self.brackets(market) && !compact {
            ui.columns(2, |columns| {
                units(self, &mut columns[0]);
                quantity_field(self, &mut columns[0], false);
                let ui = &mut columns[1];
                let money = l10n::text(ui.ctx(), BracketMode::Money.name(currency));
                let in_money = l10n::format_text(
                    ui.ctx(),
                    "Risk, {currency}",
                    &[("currency", money.as_str())],
                );
                let caption = if self.risk_in_money {
                    in_money.clone()
                } else {
                    "Risk, % balance".to_owned()
                };
                let response = dropdown(ui, caption);
                let mut risk_in_money = self.risk_in_money;
                egui::Popup::menu(&response).show(|ui| {
                    let share = ui.add_enabled(
                        balance.is_some(),
                        egui::Button::selectable(
                            !risk_in_money,
                            l10n::text(ui.ctx(), "Risk, % balance"),
                        ),
                    );
                    if share.clicked() {
                        risk_in_money = false;
                    }
                    share.on_disabled_hover_text(l10n::text(
                        ui.ctx(),
                        "Needs the account's balance",
                    ));
                    ui.selectable_value(&mut risk_in_money, true, in_money);
                });
                if risk_in_money != self.risk_in_money {
                    // Only a risk-sized order has an entered risk amount to convert.
                    if self.sizing == Sizing::Risk {
                        self.resize(ui.ctx(), market, Sizing::Risk, risk_in_money);
                    } else {
                        self.risk_in_money = risk_in_money;
                    }
                }
                let risk_enabled = self.stop_loss.0 && (self.risk_in_money || balance.is_some());
                ui.add_enabled_ui(risk_enabled, |ui| {
                    field(ui, |ui| {
                        let mut risk = if self.sizing == Sizing::Risk {
                            self.amount
                        } else if self.risk_in_money {
                            risk_money.unwrap_or(0.0)
                        } else {
                            (risk_money.zip(balance))
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

        if compact {
            // Hidden controls still describe the order; list every exit that will be sent.
            if let Ok(request) = &request {
                for line in exit_lines(ui.ctx(), request, &numbers, decimals(rules.quantity_step)) {
                    ui.label(
                        RichText::new(line)
                            .color(theme.ui.text_muted)
                            .size(theme.text.body),
                    );
                }
            }
            return;
        }

        if self.brackets(market) {
            self.exits(ui, market, request.as_ref().ok(), entry, per_point);
        }

        ui.add_space(8.0);
        title(ui, "Extra settings");
        muted(ui, "Time in force");
        time_in_force_menu(ui, "order_ticket_tif", &mut self.time_in_force, rules);
        if self.time_in_force == TimeInForce::GoodTillDate {
            muted(ui, "Expiry");
            field(ui, |ui| {
                date_time(
                    ui,
                    "order_ticket_expiry",
                    &mut self.expires,
                    &market.timezone,
                );
            });
        }

        ui.add_space(8.0);
        if let Some(position) = market
            .positions
            .iter()
            .find(|position| position.symbol == self.symbol && position.quantity != 0.0)
        {
            position_info(ui, market, position);
            return;
        }
        title(ui, "Order info");
        let Some((request, entry)) = request.as_ref().ok().zip(entry) else {
            return;
        };
        let per_point = request.quantity * rules.point_value;
        let value = entry * per_point;
        if let Some(leverage) = rules.leverage {
            let margin = value / leverage;
            // Reported funds exclude the modified order's own margin, which the change frees.
            let held = self.editing.as_ref().map_or(0.0, |order| {
                let price = match order.request.order_type {
                    OrderType::Limit(price) | OrderType::Stop(price) => price,
                    OrderType::StopLimit { limit, .. } => limit,
                    _ => market.last.unwrap_or_default(),
                };
                order.request.quantity * price.abs() * rules.point_value / leverage
            });
            let available = (market.available_funds)
                .map_or(market.balance.unwrap_or_default(), |funds| funds + held);
            let short = margin > available;
            let row = info(
                ui,
                "Est. margin",
                &format!(
                    "{} / {}",
                    grouped(margin, 2, false),
                    grouped(available, 2, false)
                ),
                "",
                if short { theme.ui.error } else { theme.ui.text },
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
                if market.available_funds.is_some() {
                    "Estimated margin: trade value divided by leverage, compared with the available \
                     funds the broker reports, which leave out the margin of open positions and \
                     working orders. The margin of an order being modified counts as available."
                } else {
                    "Estimated margin: trade value divided by leverage, compared with account \
                     balance. This does not include margin already used by other positions or \
                     working orders."
                },
            ));
            let (bar, _) = ui.allocate_exact_size(vec2(ui.available_width(), 4.0), Sense::hover());
            ui.painter().rect_filled(bar, 2.0, theme.ui.border);
            let used = if available > 0.0 {
                (margin / available).clamp(0.0, 1.0) as f32
            } else {
                1.0
            };
            let color = if short {
                theme.ui.error
            } else {
                theme.ui.accent
            };
            let fill = Rect::from_min_size(bar.min, vec2(bar.width() * used, bar.height()));
            ui.painter().rect_filled(fill, 2.0, color);
            info(ui, "Leverage", &format!("{leverage}:1"), "", theme.ui.text);
        }
        let tick_value = market.grid().tick_at(entry) * per_point;
        let places = market.decimals.max(2);
        info(
            ui,
            "Tick value",
            &grouped(tick_value, places, false),
            currency,
            theme.ui.text,
        );
        if let Some(pip_value) = market.pip_value {
            info(
                ui,
                "Pip value",
                &numbers.pip_value(pip_value * request.quantity),
                currency,
                theme.ui.text,
            );
        }
        info(
            ui,
            "Trade value",
            &grouped(value, 2, false),
            currency,
            theme.ui.text,
        );
        if let Some(reward) = reward(request, entry) {
            let reward = grouped(reward * rules.point_value, 2, false);
            info(ui, "Reward", &reward, currency, theme.ui.take_profit);
        }
        if let Some(risk) = risk(request, entry) {
            let risk = grouped(risk * rules.point_value, 2, false);
            info(ui, "Risk", &risk, currency, theme.ui.stop_loss);
        }
    }

    /// The Sell and Buy halves with the bid and ask, the chosen side filled, and the spread.
    fn quote(&mut self, ui: &mut Ui, market: &Market<'_>) {
        let theme = ui.active_theme();
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::hover());
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
                && self.editing.is_none()
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
                ui.painter().rect_filled(
                    half,
                    corners,
                    theme.ui.panel_bg.lerp_to_gamma(color, 0.45),
                );
            }
            let text_color = if selected { color } else { theme.ui.text_muted };
            let inner = half.shrink2(vec2(12.0, 8.0));
            let anchor = align.pos_in_rect(&inner);
            text(ui, anchor, align, label, theme.text.body, text_color);
            let price = price
                .or(market.last)
                .map_or("-".to_owned(), |price| market.numbers().price(price));
            let below = anchor + vec2(0.0, 18.0);
            text(ui, below, align, &price, QUOTE_PRICE_SIZE, text_color);
        }
        if let (Some(bid), Some(ask)) = (market.bid, market.ask) {
            let spread = market.numbers().spread(ask - bid);
            let galley = ui.painter().layout_no_wrap(
                l10n::text(ui.ctx(), spread),
                FontId::proportional(theme.text.small),
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
            .filter(|(kind, _)| self.editing.is_none() || *kind == self.kind)
            .collect();
        if kinds.is_empty() {
            return;
        }
        let (rect, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), FIELD_HEIGHT), Sense::hover());
        let rail = Rect::from_min_max(rect.left_bottom() - vec2(0.0, 3.5), rect.right_bottom());
        ui.painter().rect_filled(rail, 0.0, rule_color(&theme));
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
                self.select_kind(kind, market);
            }
            let color = if selected || response.hovered() {
                theme.ui.text
            } else {
                theme.ui.text_secondary
            };
            text(
                ui,
                cell.center(),
                Align2::CENTER_CENTER,
                label,
                theme.text.title,
                color,
            );
            if selected {
                let line = Rect::from_min_max(
                    pos2(cell.left() + 4.0, cell.bottom() - rail.height()),
                    pos2(cell.right() - 4.0, cell.bottom()),
                );
                ui.painter().rect_filled(line, 1.5, theme.ui.text);
            }
        }
    }

    /// The collapsible exits: a take profit and a stop loss, each with a toggle and the way its
    /// distance is entered, then for an account that accepts them the further take-profit levels,
    /// each with its share of the quantity.
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
            FontId::proportional(SECTION_TITLE_SIZE),
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
            let reward = reward(request, entry)?;
            let risk = risk(request, entry)?;
            (risk > 0.0).then(|| reward / risk)
        });
        if let Some(ratio) = ratio {
            let ratio = l10n::format_text(
                ui.ctx(),
                "  ·  Risk/Reward {ratio}",
                &[("ratio", format!("{ratio:.2}").as_str())],
            );
            let at = row.left_center() + vec2(width, 0.0);
            text(
                ui,
                at,
                Align2::LEFT_CENTER,
                &ratio,
                theme.text.body,
                theme.ui.text_muted,
            );
        }
        if let Some(request) = request {
            let coverage = exit_coverage(request);
            let center = pos2(row.right() - 16.0 - COVERAGE_CELL / 2.0, row.center().y);
            paint_coverage(ui.painter(), &theme, center, coverage);
            ui.interact(
                Rect::from_center_size(center, vec2(COVERAGE_CELL, row.height())),
                ui.make_persistent_id("order_ticket_coverage"),
                Sense::hover(),
            )
            .on_hover_text(coverage_text(ui.ctx(), coverage));
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
        let grid = market.grid();
        let balance = market.positive_balance();
        // An exit's title, with `control` at its right, and its field. `control` says whether the
        // field is enabled.
        let exit_fields = |ui: &mut Ui,
                           per_point: f64,
                           profit: bool,
                           title: &str,
                           index: usize,
                           bracket: &mut Bracket,
                           control: &mut dyn FnMut(&mut Ui) -> bool| {
            // Money and balance shares need a quantity above 0 before they have a price.
            let priced = |mode| {
                (!matches!(mode, BracketMode::Money | BracketMode::BalancePercent)
                    || per_point > 0.0)
                    && (mode != BracketMode::BalancePercent || balance.is_some())
            };
            let exit = entry.filter(|_| priced(bracket.mode)).and_then(|entry| {
                let price = bracket.price(side, entry, grid, per_point, balance, profit);
                price.is_finite().then(|| (entry, grid.snap(price)))
            });
            let mut mode = bracket.mode;
            let enabled = ui
                .horizontal(|ui| {
                    let mode_name = l10n::text(ui.ctx(), bracket.mode.name(market.currency));
                    let caption = l10n::format_text(
                        ui.ctx(),
                        "{exit}, {mode}",
                        &[("exit", title), ("mode", mode_name.as_str())],
                    );
                    let response = dropdown(ui, caption);
                    egui::Popup::menu(&response).show(|ui| {
                        for option in BracketMode::ALL {
                            let usable = option != BracketMode::BalancePercent || balance.is_some();
                            let item = ui.add_enabled(
                                usable,
                                egui::Button::selectable(
                                    mode == option,
                                    l10n::text(ui.ctx(), option.name(market.currency)),
                                ),
                            );
                            if item.clicked() {
                                mode = option;
                            }
                            item.on_disabled_hover_text(l10n::text(
                                ui.ctx(),
                                "Needs the account's balance",
                            ));
                        }
                    });
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| control(ui))
                        .inner
                })
                .inner;
            let mut swapped = false;
            ui.add_enabled_ui(enabled, |ui| {
                field(ui, |ui| {
                    let places = match bracket.mode {
                        BracketMode::Price => market.decimals,
                        BracketMode::Ticks => 0,
                        BracketMode::Percent | BracketMode::Money | BracketMode::BalancePercent => {
                            2
                        }
                    };
                    number(ui, (profit, index), &mut bracket.value, places);
                    let Some((entry, price)) = exit else {
                        return;
                    };
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        swapped = swap(ui, "Enter the price or its distance in ticks");
                        muted(
                            ui,
                            &if bracket.mode == BracketMode::Price {
                                l10n::format_text(
                                    ui.ctx(),
                                    "{0} ticks",
                                    &[(
                                        "0",
                                        grid.steps_between(entry, price).abs().to_string().as_str(),
                                    )],
                                )
                            } else {
                                market.numbers().price(price)
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
            if mode != bracket.mode {
                // The exit stays where it is: its distance is written in the new way.
                *bracket = match exit {
                    Some((entry, price)) if priced(mode) => {
                        Bracket::at(mode, price, side, entry, grid, per_point, balance, profit)
                    }
                    _ => Bracket { mode, ..*bracket },
                };
            }
        };
        let level_quantity =
            |index: usize| request.and_then(|request| request.take_profit_levels.get(index));
        // What a level earns per price unit, from its quantity in the order while there is one.
        let level_per_point = |index: usize, share: f64| {
            level_quantity(index).map_or(per_point * share / 100.0, |level| {
                level.quantity * market.rules.point_value
            })
        };
        // A level's share in percent, with the quantity it comes to at the right.
        let share_field = |ui: &mut Ui, index: usize, share: &mut f64| {
            field(ui, |ui| {
                number(ui, ("take_profit_share", index), share, 2);
                let Some(level) = level_quantity(index) else {
                    return;
                };
                let quantity = market
                    .numbers()
                    .quantity(level.quantity, decimals(market.rules.quantity_step));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let text =
                        l10n::format_text(ui.ctx(), "% of the quantity: {0}", &[("0", quantity)]);
                    muted(ui, &text);
                });
            });
        };

        // An own stop starts at the order's stop until the user changes it.
        let start = self.stop_loss.1;
        let stop_fields =
            |ui: &mut Ui, level: usize, per_point: f64, stop: &mut Option<Bracket>| {
                let mut on = stop.is_some();
                let mut bracket = stop.unwrap_or(start);
                let title =
                    l10n::format_text(ui.ctx(), "Stop loss {0}", &[("0", level.to_string())]);
                exit_fields(
                    ui,
                    per_point,
                    false,
                    &title,
                    level,
                    &mut bracket,
                    &mut |ui: &mut Ui| {
                        toggle(ui, &mut on, "Stop loss");
                        on
                    },
                );
                *stop = on.then_some(bracket);
            };

        let offered = levels_accepted(&market.capabilities) && self.take_profit.0;
        let several = offered && !self.levels.is_empty();
        let level_stops = several && market.capabilities.take_profit_level_stops;
        let mut rest = 100.0 - self.levels.iter().map(|level| level.share).sum::<f64>();
        let (title, first_per_point) = if several {
            let title = l10n::format_text(ui.ctx(), "Take profit {0}", &[("0", "1".to_owned())]);
            (title, level_per_point(0, rest))
        } else {
            (l10n::text(ui.ctx(), "Take profit"), per_point)
        };
        let (on, bracket) = &mut self.take_profit;
        exit_fields(
            ui,
            first_per_point,
            true,
            &title,
            0,
            bracket,
            &mut |ui: &mut Ui| {
                toggle(ui, on, "Take profit");
                *on
            },
        );
        if several {
            ui.add_enabled_ui(false, |ui| share_field(ui, 0, &mut rest));
        }
        if level_stops {
            stop_fields(ui, 1, first_per_point, &mut self.first_level_stop_loss);
        }
        let stop_loss = l10n::text(ui.ctx(), "Stop loss");
        let (on, bracket) = &mut self.stop_loss;
        exit_fields(
            ui,
            per_point,
            false,
            &stop_loss,
            0,
            bracket,
            &mut |ui: &mut Ui| {
                toggle(ui, on, "Stop loss");
                *on
            },
        );
        if !offered {
            return;
        }

        let mut removed = None;
        for (index, saved) in self.levels.iter_mut().enumerate() {
            let level = index + 1;
            let title = l10n::format_text(
                ui.ctx(),
                "Take profit {0}",
                &[("0", (level + 1).to_string())],
            );
            exit_fields(
                ui,
                level_per_point(level, saved.share),
                true,
                &title,
                level,
                &mut saved.take_profit,
                &mut |ui: &mut Ui| {
                    if IconButton::new(&icons::TRASH)
                        .size(ButtonSize::SM)
                        .tooltip(l10n::text(ui.ctx(), "Remove level"))
                        .show(ui)
                        .clicked()
                    {
                        removed = Some(index);
                    }
                    true
                },
            );
            share_field(ui, level, &mut saved.share);
            if level_stops {
                stop_fields(
                    ui,
                    level + 1,
                    level_per_point(level, saved.share),
                    &mut saved.stop_loss,
                );
            }
        }
        if let Some(index) = removed {
            self.levels.remove(index);
        }
        if market
            .capabilities
            .max_take_profit_levels
            .is_some_and(|max| self.levels.len() + 1 >= max)
        {
            return;
        }
        ui.add_space(4.0);
        if Button::new("Add level").secondary().show(ui).clicked() {
            let last = self
                .levels
                .last()
                .map_or(self.take_profit.1, |level| level.take_profit);
            let share = (rest / 2.0 * 100.0).round() / 100.0;
            let price = request.and_then(|request| {
                let last = request.take_profit_levels.last();
                last.map(|level| level.price).or(request.take_profit)
            });
            // As far beyond the last take profit as the form puts the first from the entry.
            let bracket = match price.zip(entry) {
                Some((price, entry)) => Bracket::at(
                    last.mode,
                    grid.offset(price, Bracket::sign(side, true) * 100.0),
                    side,
                    entry,
                    grid,
                    per_point * share / 100.0,
                    balance,
                    true,
                ),
                None => last,
            };
            self.levels.push(PresetLevel::new(bracket, share));
        }
    }
}

/// The held position replaces the pending order's information. Reported broker figures take
/// precedence; exit results use the entry price and remaining quantity of each linked exit.
fn position_info(ui: &mut Ui, market: &Market<'_>, position: &Position) {
    let theme = ui.active_theme();
    let rules = market.rules;
    let numbers = market.numbers();
    let currency = market.currency;
    let size = position.quantity.abs();
    let sign = position.quantity.signum();
    let per_point = size * rules.point_value;
    let finite = |value: &f64| value.is_finite();
    let (side, label) = if position.quantity > 0.0 {
        (OrderSide::Buy, "Long")
    } else {
        (OrderSide::Sell, "Short")
    };
    title(ui, "Position info");
    info(ui, "Side", label, "", side_color(&theme, side));
    info(
        ui,
        "Quantity",
        &numbers.quantity(size, decimals(rules.quantity_step)),
        "",
        theme.ui.text,
    );
    info(
        ui,
        "Average price",
        &numbers.price(position.average_price),
        "",
        theme.ui.text,
    );
    let value = position
        .trade_value
        .filter(finite)
        .unwrap_or(position.average_price * per_point);
    info(
        ui,
        "Trade value",
        &grouped(value, 2, false),
        currency,
        theme.ui.text,
    );
    if let Some(leverage) = position
        .leverage
        .filter(|leverage| leverage.is_finite() && *leverage > 0.0)
        .or(rules.leverage)
    {
        info(ui, "Leverage", &format!("{leverage}:1"), "", theme.ui.text);
    }
    let last = position.last_price.or(market.last);
    let pnl = position
        .unrealized_pnl
        .filter(finite)
        .or_else(|| last.map(|last| (last - position.average_price) * sign * per_point));
    if let Some(pnl) = pnl {
        let color = if pnl > 0.0 {
            theme.ui.take_profit
        } else if pnl < 0.0 {
            theme.ui.error
        } else {
            theme.ui.text
        };
        info(
            ui,
            "Unrealized P&L",
            &grouped(pnl, 2, true),
            currency,
            color,
        );
    }
    let at_exits = |bracket| {
        market
            .orders
            .iter()
            .filter(|order| {
                order.request.symbol == position.symbol
                    && (order.request.side == OrderSide::Buy) != (position.quantity > 0.0)
                    && order.request.position_bracket == Some(bracket)
            })
            .filter_map(|order| {
                let price = entry_price(&order.request.order_type)?;
                let open = order.request.quantity - order.filled;
                Some((price - position.average_price) * sign * open * rules.point_value)
            })
            .reduce(|sum, pnl| sum + pnl)
    };
    if let Some(reward) = at_exits(PositionBracket::TakeProfit) {
        info(
            ui,
            "Reward",
            &grouped(reward, 2, false),
            currency,
            theme.ui.take_profit,
        );
    }
    if let Some(loss) = at_exits(PositionBracket::StopLoss) {
        info(
            ui,
            "Risk",
            &grouped(-loss, 2, false),
            currency,
            theme.ui.stop_loss,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Order, OrderId, OrderStatus, Position, PositionBracket, TradingRules};

    /// The information follows the held symbol, uses broker figures before estimates and prices
    /// a short position's exits for their remaining size. A flat position restores Order info.
    #[test]
    fn the_order_info_becomes_position_info_while_the_symbol_has_a_position() {
        let mut rules = TradingRules::new("ES");
        rules.leverage = Some(10.0);
        let mut long = Position::new("ES", 500.0, 100.0);
        long.unrealized_pnl = Some(1234.0);
        let mut short = Position::new("ES", -200.0, 105.0);
        short.trade_value = Some(21_500.0);
        short.leverage = Some(20.0);
        let other = Position::new("NQ", 3.0, 50.0);
        let exit = |id, side, quantity, kind, bracket, filled| {
            let mut request = OrderRequest::new("ES", side, quantity, kind);
            request.position_bracket = bracket;
            let mut order = Order::new(OrderId::new(id), request, OrderStatus::Working);
            order.filled = filled;
            order
        };
        let order_info = &[("Est. margin", "5,000.00 / 10,000.00")][..];
        let ctx = egui::Context::default();
        let mut ticket = OrderTicket {
            symbol: "ES".into(),
            quantity: 500.0,
            ..OrderTicket::default()
        };
        for (positions, orders, expected) in [
            (vec![other.clone()], vec![], order_info),
            (
                vec![other, long],
                vec![
                    exit(
                        "1",
                        OrderSide::Sell,
                        500.0,
                        OrderType::Limit(110.0),
                        Some(PositionBracket::TakeProfit),
                        0.0,
                    ),
                    exit(
                        "2",
                        OrderSide::Sell,
                        500.0,
                        OrderType::Stop(95.0),
                        Some(PositionBracket::StopLoss),
                        0.0,
                    ),
                    exit(
                        "3",
                        OrderSide::Buy,
                        100.0,
                        OrderType::Limit(90.0),
                        None,
                        0.0,
                    ),
                    exit(
                        "4",
                        OrderSide::Sell,
                        100.0,
                        OrderType::Limit(120.0),
                        None,
                        0.0,
                    ),
                ],
                &[
                    ("Side", "Long"),
                    ("Quantity", "500"),
                    ("Average price", "100.00"),
                    ("Trade value", "50,000.00"),
                    ("Leverage", "10:1"),
                    ("Unrealized P&L", "+1,234.00"),
                    ("Reward", "5,000.00"),
                    ("Risk", "2,500.00"),
                ][..],
            ),
            (
                vec![short],
                vec![
                    exit(
                        "5",
                        OrderSide::Buy,
                        200.0,
                        OrderType::Limit(95.0),
                        Some(PositionBracket::TakeProfit),
                        50.0,
                    ),
                    exit(
                        "6",
                        OrderSide::Buy,
                        200.0,
                        OrderType::Stop(108.0),
                        Some(PositionBracket::StopLoss),
                        0.0,
                    ),
                ],
                &[
                    ("Side", "Short"),
                    ("Quantity", "200"),
                    ("Average price", "105.00"),
                    ("Trade value", "21,500.00"),
                    ("Leverage", "20:1"),
                    ("Unrealized P&L", "+1,000.00"),
                    ("Reward", "1,500.00"),
                    ("Risk", "600.00"),
                ][..],
            ),
            (vec![Position::new("ES", 0.0, 100.0)], vec![], order_info),
            (vec![], vec![], order_info),
        ] {
            let market = Market {
                positions: &positions,
                orders: &orders,
                ..super::super::tests::market(&rules)
            };
            let held = positions
                .iter()
                .any(|p| p.symbol == "ES" && p.quantity != 0.0);
            let mut labels = Vec::new();
            for _ in 0..2 {
                let output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(Rect::from_min_size(
                            egui::Pos2::ZERO,
                            vec2(340.0, 1600.0),
                        )),
                        ..Default::default()
                    },
                    |ui| ticket.form(ui, &market, false),
                );
                labels = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) => Some((
                            text.galley.text().to_owned(),
                            text.visual_bounding_rect().center().y,
                        )),
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                output.drop_without_applying_deltas();
            }
            for (name, shown) in [
                ("Position info", held),
                ("Order info", !held),
                ("Est. margin", !held),
                ("Average price", held),
                ("Unrealized P&L", held),
            ] {
                assert_eq!(
                    labels.iter().any(|(label, _)| label == name),
                    shown,
                    "{name}: {positions:?}"
                );
            }
            for &(label, value) in expected {
                assert!(
                    labels.iter().any(|(name, y)| name == label
                        && labels
                            .iter()
                            .any(|(text, value_y)| text == value && (y - value_y).abs() < 2.0)),
                    "row {label}: {value}: {labels:?}"
                );
            }
        }
    }
}
