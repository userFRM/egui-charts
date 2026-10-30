//! The order ticket's Order tab: the quote, the order kinds, the quantity and price fields, and the
//! exits.

use super::{Bracket, BracketMode, Kind, Market, OrderTicket, Sizing};
use crate::chart::{decimals, to_step};
use crate::icons::icons;
use crate::localization as l10n;
use crate::model::{OrderRequest, OrderSide};
use crate::theme::ThemeExt;
use crate::ui::trading::widgets::{
    FIELD_HEIGHT, SECTION_TITLE_SIZE, chevron, dropdown, field, grouped, hit, info, muted, number,
    side_color, swap, text, time_in_force_menu, title, toggle, track,
};
use egui::{Align, Align2, FontId, Layout, Rect, Sense, Ui, pos2, vec2};

/// Quote prices sit between the heading and dialog-title text roles.
const QUOTE_PRICE_SIZE: f32 = 17.0;

impl OrderTicket {
    /// The Order tab above the send button.
    pub(super) fn form(&mut self, ui: &mut Ui, market: &Market<'_>) {
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
        let numbers = market.numbers();

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
                            numbers.price(*price)
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
                                    numbers.quantity(quantity, decimals(rules.quantity_step))
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
            .collect();
        if kinds.is_empty() {
            return;
        }
        let (rect, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), FIELD_HEIGHT), Sense::hover());
        let rail = Rect::from_min_max(rect.left_bottom() - vec2(0.0, 3.5), rect.right_bottom());
        ui.painter().rect_filled(
            rail,
            0.0,
            crate::ui::top_toolbar::buttons::rule_color(&theme),
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
                theme.text.body,
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
}
