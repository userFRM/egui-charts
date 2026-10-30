//! The order ticket's panel: its header, tabs and menu, and the button that sends the order.

use super::{Action, Market, OrderPreset, OrderTicket, Tab, confirmation, order_quantity, summary};
use crate::chart::decimals;
use crate::icons::icons;
use crate::localization as l10n;
use crate::theme::ThemeExt;
use crate::ui::trading::widgets::{SECTION_TITLE_SIZE, hit, side_fill, side_name, text, track};
use crate::ui_kit::buttons::IconButton;
use crate::ui_kit::modal::dialogs::ConfirmDialog;
use egui::{Align, Align2, Layout, Rect, RichText, Role, Sense, Ui, WidgetInfo, vec2};

impl OrderTicket {
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
                            .size(ui.active_theme().text.label),
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
                &market.numbers(),
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
            ui.label(
                RichText::new(&self.symbol)
                    .strong()
                    .size(SECTION_TITLE_SIZE),
            );
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if IconButton::new(&icons::CLOSE)
                    .icon_scale(1.0)
                    .tooltip("Close")
                    .show(ui)
                    .clicked()
                {
                    self.open = false;
                }
                let more = IconButton::new(&icons::UI_MORE_HORIZONTAL)
                    .icon_scale(1.0)
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
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 34.0), Sense::hover());
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
                theme.text.title,
                if selected {
                    theme.ui.text
                } else {
                    theme.ui.text_secondary
                },
            );
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
                    .size(ui.active_theme().text.label),
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
            ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::click());
        let name = side_name(self.side);
        let detail = request.as_ref().map_or(String::new(), |request| {
            summary(
                request,
                &market.numbers(),
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
            fill = match self.side {
                crate::model::OrderSide::Buy => theme.ui.accent_hover,
                crate::model::OrderSide::Sell => theme.ui.danger_bg_hover,
            };
        }
        ui.painter().rect_filled(rect, 8.0, fill);
        let ink = match self.side {
            crate::model::OrderSide::Buy => theme.ui.accent_text,
            crate::model::OrderSide::Sell => theme.ui.danger_text,
        };
        let top = rect.center_top() + vec2(0.0, 8.0);
        text(ui, top, Align2::CENTER_TOP, name, theme.text.heading, ink);
        let bottom = rect.center_bottom() - vec2(0.0, 8.0);
        text(
            ui,
            bottom,
            Align2::CENTER_BOTTOM,
            &detail,
            theme.text.label,
            ink,
        );
        (response.clicked())
            .then_some(request.ok())
            .flatten()
            .map(Action::Place)
            .or(preview)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        Capabilities, Order, OrderId, OrderRequest, OrderSide, OrderType, TimeInForce, TradingRules,
    };
    use egui::pos2;

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
                    symbol: "ES",
                    formatter: None,
                    pip_value: None,
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
                    ladder: None,
                    ladder_unavailable: None,
                    highlights: Vec::new(),
                    orders: &[],
                    positions: &[],
                    trades: &[],
                    session_open: None,
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
            symbol: "ES",
            formatter: None,
            pip_value: None,
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
            ladder: None,
            ladder_unavailable: None,
            highlights: Vec::new(),
            orders: &working,
            positions: &[],
            trades: &[],
            session_open: None,
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
}
