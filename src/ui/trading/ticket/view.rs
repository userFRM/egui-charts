//! The order ticket's panel: its header, tabs and menu, and the button that sends the order.

use std::collections::BTreeMap;

use super::{
    Action, Market, OrderPreset, OrderTicket, Tab, confirmation, levels_accepted, order_quantity,
    summary,
};
use crate::icons::icons;
use crate::localization as l10n;
use crate::painting::symbol_mark::paint_symbol_mark;
use crate::scales::decimals;
use crate::theme::ThemeExt;
use crate::ui::connection_status::show_connection_status_with_label;
use crate::ui::trading::widgets::{SECTION_TITLE_SIZE, hit, side_fill, side_name, text, track};
use crate::ui_kit::buttons::{ButtonSize, IconButton};
use crate::ui_kit::modal::dialogs::ConfirmDialog;
use egui::{
    Align, Align2, ImageSource, Label, Layout, Rect, RichText, Role, Sense, Ui, UiBuilder,
    WidgetInfo, vec2,
};

impl OrderTicket {
    /// Draws the ticket's contents into `ui`, the panel the host gives it. Returns what the user
    /// asked for: an order submitted on the Order tab, or an order, change or cancellation from
    /// the depth ladder.
    pub fn show(
        &mut self,
        ui: &mut Ui,
        market: &Market<'_>,
        presets: &mut BTreeMap<String, Vec<OrderPreset>>,
        one_click: &mut bool,
        compact: &mut bool,
    ) -> Option<Action> {
        if let Some(editing) = &self.editing {
            self.editing = market
                .orders
                .iter()
                .find(|order| order.id == editing.id)
                .cloned();
            if self.editing.is_none() {
                self.tab = Tab::Dom;
            }
        }
        if !market.rules.time_in_force.contains(&self.time_in_force) {
            self.time_in_force = market.rules.time_in_force[0];
        }
        if self
            .preset_notice
            .as_ref()
            .is_some_and(|(account, _)| account != market.account)
        {
            self.preset_notice = None;
        }
        ui.spacing_mut().item_spacing.y = 8.0;
        self.header(ui, market, presets, one_click, compact);
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
                    .show(ui, |ui| {
                        if let Some((_, notice)) = &self.preset_notice {
                            for (template, name) in notice {
                                let name = name.map(|name| l10n::text(ui.ctx(), name));
                                ui.label(
                                    RichText::new(l10n::format_text(
                                        ui.ctx(),
                                        template,
                                        &[("name", name.unwrap_or_default())],
                                    ))
                                    .color(ui.active_theme().ui.warning)
                                    .size(ui.active_theme().text.label),
                                );
                            }
                        }
                        self.form(ui, market, *compact)
                    });
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
                        let bracketed = self.with_brackets(ui.ctx(), request, market);
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
                    Some(Action::Edit(order)) => {
                        self.edit(&order);
                        None
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
                || (request.stop_loss.is_some()
                    || request.take_profit.is_some()
                    || !request.take_profit_levels.is_empty())
                    && !market.capabilities.brackets
                || !request.take_profit_levels.is_empty() && !levels_accepted(&market.capabilities)
                || market
                    .capabilities
                    .max_take_profit_levels
                    .is_some_and(|max| request.take_profit_levels.len() > max)
                || request
                    .take_profit_levels
                    .iter()
                    .any(|level| level.stop_loss.is_some())
                    && !market.capabilities.take_profit_level_stops
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
                l10n::format_text(
                    ui.ctx(),
                    "Cancel {count} working orders for {symbol}?",
                    &[
                        ("count", ids.len().to_string().as_str()),
                        ("symbol", self.symbol.as_str()),
                    ],
                ),
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
                ui.ctx(),
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

    /// The symbol with the broker's status while it is not live, the menu of presets and
    /// one-click trading, and the close button.
    fn header(
        &mut self,
        ui: &mut Ui,
        market: &Market<'_>,
        presets: &mut BTreeMap<String, Vec<OrderPreset>>,
        one_click: &mut bool,
        compact: &mut bool,
    ) {
        ui.horizontal(|ui| {
            ui.set_min_height(32.0);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if IconButton::new(&icons::CLOSE)
                    .icon_scale(1.0)
                    .tooltip("Close")
                    .show(ui)
                    .clicked()
                {
                    self.open = false;
                    self.editing = None;
                }
                let more = IconButton::new(&icons::UI_MORE_HORIZONTAL)
                    .icon_scale(1.0)
                    .tooltip("More")
                    .show(ui);
                egui::Popup::menu(&more)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show(|ui| self.more(ui, market, presets, one_click, compact));
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    let (mark, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
                    paint_symbol_mark(
                        ui.painter(),
                        mark,
                        &self.symbol,
                        market.logo.map(ImageSource::from),
                    );
                    // A long symbol gives way, so that the status stays clear of the buttons.
                    let status = market.offline.as_ref().map_or(0.0, |state| {
                        let mut measure = ui.new_child(UiBuilder::new().invisible());
                        show_connection_status_with_label(&mut measure, state)
                            .rect
                            .width()
                            + ui.spacing().item_spacing.x
                    });
                    let width = (ui.available_width() - status).max(0.0);
                    ui.allocate_ui(vec2(width, ui.available_height()), |ui| {
                        ui.add(
                            Label::new(
                                RichText::new(&self.symbol)
                                    .strong()
                                    .size(SECTION_TITLE_SIZE),
                            )
                            .truncate(),
                        );
                    });
                    if let Some(state) = &market.offline {
                        show_connection_status_with_label(ui, state);
                    }
                });
            });
        });
    }

    fn more(
        &mut self,
        ui: &mut Ui,
        market: &Market<'_>,
        presets: &mut BTreeMap<String, Vec<OrderPreset>>,
        one_click: &mut bool,
        compact: &mut bool,
    ) {
        ui.checkbox(one_click, l10n::text(ui.ctx(), "One-click trading"))
            .on_hover_text(l10n::text(
                ui.ctx(),
                "Buy and Sell, and the depth ladder, place orders at once, without confirmation",
            ));
        ui.checkbox(compact, l10n::text(ui.ctx(), "Compact form"))
            .on_hover_text(l10n::text(
                ui.ctx(),
                "Show only the side, the order type, the price and the quantity above the send button",
            ));
        ui.separator();
        ui.weak(l10n::text(ui.ctx(), "Presets"));
        let mut saved = presets.remove(market.account).unwrap_or_default();
        if saved.is_empty() {
            ui.weak(l10n::text(ui.ctx(), "No presets saved"));
        }
        if self
            .renaming
            .as_ref()
            .is_some_and(|(old, _)| !saved.iter().any(|p| &p.name == old))
        {
            self.renaming = None;
        }
        let selected = saved.iter().position(|preset| preset.applied);
        let mut applied = None;
        let mut remove = None;
        let mut rename = None;
        let mut cancel_rename = false;
        for (index, preset) in saved.iter().enumerate() {
            ui.horizontal(|ui| {
                if let Some((old, name)) = &mut self.renaming
                    && old == &preset.name
                {
                    ui.add(
                        egui::TextEdit::singleline(name)
                            .hint_text(old.as_str())
                            .desired_width(120.0),
                    )
                    .accessible_name(l10n::text(ui.ctx(), "New name"));
                    let name = name.trim();
                    let allowed = !name.is_empty()
                        && name != old.as_str()
                        && !saved.iter().any(|p| p.name == name);
                    if ui
                        .add_enabled(allowed, egui::Button::new(l10n::text(ui.ctx(), "Rename")))
                        .clicked()
                    {
                        rename = Some((index, name.to_owned()));
                    }
                    cancel_rename = ui.button(l10n::text(ui.ctx(), "Cancel")).clicked();
                } else {
                    if ui
                        .selectable_label(selected == Some(index), preset.name.as_str())
                        .clicked()
                    {
                        self.apply(preset, market);
                        applied = Some(index);
                    }
                    if IconButton::new(&icons::PENCIL)
                        .size(ButtonSize::SM)
                        .tooltip("Rename preset")
                        .show(ui)
                        .accessible_name(l10n::text(ui.ctx(), "Rename preset"))
                        .clicked()
                    {
                        self.renaming = Some((preset.name.clone(), String::new()));
                    }
                    if IconButton::new(&icons::TRASH)
                        .size(ButtonSize::SM)
                        .tooltip("Delete preset")
                        .show(ui)
                        .accessible_name(l10n::text(ui.ctx(), "Delete preset"))
                        .clicked()
                    {
                        remove = Some(index);
                    }
                }
            });
        }
        if let Some(chosen) = applied {
            for (index, preset) in saved.iter_mut().enumerate() {
                preset.applied = index == chosen;
            }
        }
        if let Some((index, name)) = rename {
            saved[index].name = name;
            self.renaming = None;
        }
        if cancel_rename {
            self.renaming = None;
        }
        if let Some(index) = remove {
            saved.remove(index);
        }
        ui.horizontal_wrapped(|ui| {
            ui.weak(l10n::text(ui.ctx(), "Keep in preset"));
            for (value, label, name) in [
                (
                    &mut self.preset_fields.quantity,
                    "Quantity",
                    "Keep quantity",
                ),
                (&mut self.preset_fields.exits, "Exits", "Keep exits"),
                (
                    &mut self.preset_fields.order_type,
                    "Order type",
                    "Keep order type",
                ),
                (
                    &mut self.preset_fields.time_in_force,
                    "Time in force",
                    "Keep time in force",
                ),
            ] {
                ui.checkbox(value, l10n::text(ui.ctx(), label))
                    .accessible_name(l10n::text(ui.ctx(), name));
            }
        });
        ui.horizontal(|ui| {
            ui.add(
                egui::TextEdit::singleline(&mut self.preset_name)
                    .hint_text(l10n::text(ui.ctx(), "Preset name"))
                    .desired_width(120.0),
            )
            .accessible_name(l10n::text(ui.ctx(), "Preset name"));
            let name = self.preset_name.trim();
            let exists = saved.iter().any(|p| p.name == name);
            let fields = self.preset_fields;
            if ui
                .add_enabled(
                    !name.is_empty()
                        && (exists || saved.len() < 10)
                        && self.replacing.is_none()
                        && (fields.quantity
                            || fields.exits
                            || fields.order_type
                            || fields.time_in_force),
                    egui::Button::new(l10n::text(ui.ctx(), "Save")),
                )
                .clicked()
            {
                let preset = self.preset(ui.ctx(), name.to_owned(), market);
                if exists {
                    self.replacing = Some(preset);
                } else {
                    saved.push(preset);
                    self.preset_name.clear();
                }
            }
        });
        if saved.len() >= 10 {
            ui.weak(l10n::text(
                ui.ctx(),
                "You can save up to 10 presets per account",
            ));
        }
        if let Some(preset) = &self.replacing {
            ui.label(l10n::format_text(
                ui.ctx(),
                "Replace the preset {name}?",
                &[("name", &preset.name)],
            ));
            let exists = saved.iter().position(|p| p.name == preset.name);
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(
                        exists.is_some() || saved.len() < 10,
                        egui::Button::new(l10n::text(ui.ctx(), "Replace")),
                    )
                    .clicked()
                {
                    let mut preset = self.replacing.take().unwrap();
                    if let Some(index) = exists {
                        preset.applied = saved[index].applied;
                        saved[index] = preset;
                    } else {
                        saved.push(preset);
                    }
                    self.preset_name.clear();
                }
                if ui.button(l10n::text(ui.ctx(), "Cancel")).clicked() {
                    self.replacing = None;
                }
            });
        }
        if !saved.is_empty() {
            presets.insert(market.account.to_owned(), saved);
        }
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
        let editing = self.editing.clone();
        let request = self.request(ui.ctx(), market).and_then(|request| {
            let Some(order) = &editing else {
                return Ok(request);
            };
            let mut changed = order.request.clone();
            let exit = order.request.position_bracket.is_some();
            // A position exit is sized by its position, so only its price and timing change.
            if exit && request.quantity != order.request.quantity {
                return Err("A position exit keeps its quantity".into());
            }
            changed.quantity = request.quantity;
            changed.order_type = request.order_type;
            changed.time_in_force = request.time_in_force;
            changed.expires = request.expires;
            // An account without bracket support keeps the working order's own exits.
            if market.capabilities.brackets && !exit {
                changed.take_profit = request.take_profit;
                changed.take_profit_levels = request.take_profit_levels;
                changed.stop_loss = request.stop_loss;
            }
            if !market.capabilities.modify {
                return Err("The account does not support modifying orders".into());
            }
            if changed.quantity <= order.filled {
                return Err(l10n::format_text(
                    ui.ctx(),
                    "Enter more than the filled quantity of {quantity}",
                    &[(
                        "quantity",
                        market
                            .numbers()
                            .quantity(order.filled, decimals(market.rules.quantity_step)),
                    )],
                ));
            }
            if changed == order.request {
                return Err("Change the order to modify it".into());
            }
            Ok(changed)
        });
        if let Some(order) = &editing {
            ui.weak(l10n::format_text(
                ui.ctx(),
                "Modifying order {id}",
                &[("id", &order.id.0)],
            ));
            if ui
                .small_button(l10n::text(ui.ctx(), "Stop editing"))
                .clicked()
            {
                self.editing = None;
                return None;
            }
        }
        if let Err(reason) = &request {
            ui.label(
                RichText::new(l10n::text(ui.ctx(), reason))
                    .color(theme.ui.error)
                    .size(ui.active_theme().text.label),
            );
        }
        let mut preview = None;
        if market.capabilities.preview
            && editing.is_none()
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
        let name = if editing.is_some() {
            "Modify order"
        } else {
            side_name(self.side)
        };
        let detail = request.as_ref().map_or(String::new(), |request| {
            summary(
                ui.ctx(),
                request,
                &market.numbers(),
                decimals(market.rules.quantity_step),
            )
        });
        let translated_label = format!("{} {detail}", l10n::text(ui.ctx(), name));
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
            .map(|request| match editing {
                Some(order) => {
                    self.editing = None;
                    self.tab = Tab::Dom;
                    Action::Confirm(Box::new(crate::chart::TradingLineAction::Modify(
                        order, request,
                    )))
                }
                None => Action::Place(request),
            })
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
    use egui_kittest::kittest::{NodeT, Queryable};

    #[test]
    fn preset_names_are_shown_as_typed() {
        let rules = TradingRules::new("ES");
        let market = super::super::tests::market(&rules);
        let mut harness = egui_kittest::Harness::builder()
            .with_context_setup(egui_extras::install_image_loaders)
            .build_ui_state(
                |ui, (ticket, presets): &mut (OrderTicket, BTreeMap<String, Vec<OrderPreset>>)| {
                    ticket.more(ui, &market, presets, &mut false, &mut false);
                },
                (
                    OrderTicket::default(),
                    BTreeMap::from([("".into(), vec![OrderPreset::new("Market", 1.0)])]),
                ),
            );
        crate::localization::set_translator(&harness.ctx, |key| {
            (key == "Market").then(|| "Markt".to_owned())
        });
        harness.run();
        assert!(harness.query_by_label("Market").is_some());
        assert!(harness.query_by_label("Markt").is_none());
    }

    /// Presets follow the account, replacement waits and keeps its position, and rename changes
    /// only the name. Saving a new name at ten presets is disabled while replacement still works.
    #[test]
    fn presets_belong_to_their_account_ask_before_replacing_and_keep_fields_on_rename() {
        use super::super::{Bracket, BracketMode, Kind, PresetLevel, PresetSizing};
        let rules = TradingRules::new("ES");
        let mut harness = egui_kittest::Harness::builder()
            .with_context_setup(egui_extras::install_image_loaders)
            .with_size(vec2(700.0, 900.0))
            .build_ui_state(
                |ui,
                 (ticket, presets, account): &mut (
                    OrderTicket,
                    BTreeMap<String, Vec<OrderPreset>>,
                    &'static str,
                )| {
                    let mut market = super::super::tests::market(&rules);
                    market.account = account;
                    ticket.more(ui, &market, presets, &mut false, &mut false);
                },
                (
                    OrderTicket {
                        quantity: 2.0,
                        preset_name: "Scalp".into(),
                        ..OrderTicket::default()
                    },
                    BTreeMap::new(),
                    "stocks",
                ),
            );
        harness.run();
        harness.state_mut().0.preset_fields = super::super::PresetFields {
            quantity: false,
            exits: false,
            order_type: false,
            time_in_force: false,
        };
        harness.run();
        assert!(harness.get_by_label("Save").accesskit_node().is_disabled());
        harness.state_mut().0.preset_fields = super::super::PresetFields::default();
        harness.run();
        harness.get_by_label("Save").click();
        harness.run();
        assert_eq!(harness.state().1["stocks"][0].quantity, 2.0);
        harness.state_mut().2 = "crypto";
        harness.run();
        assert!(harness.query_by_label("Scalp").is_none());
        harness.get_by_label("No presets saved");
        assert!(!harness.state().1.contains_key("crypto"));
        harness.state_mut().2 = "stocks";
        harness
            .state_mut()
            .1
            .get_mut("stocks")
            .unwrap()
            .push(OrderPreset::new("Other", 1.0));
        harness.run();
        harness.get_by_label("Scalp").click();
        harness.run();
        assert_eq!(
            harness.state().1["stocks"]
                .iter()
                .map(|preset| preset.applied)
                .collect::<Vec<_>>(),
            [true, false]
        );
        assert_eq!(
            harness.get_by_label("Scalp").accesskit_node().toggled(),
            Some(egui::accesskit::Toggled::True)
        );
        harness.state_mut().0.quantity = 5.0;
        harness.state_mut().0.preset_name = "Scalp".into();
        harness.run();
        harness.get_by_label("Save").click();
        harness.run();
        assert_eq!(harness.state().1["stocks"][0].quantity, 2.0);
        harness.get_by_label("Cancel").click();
        harness.run();
        assert_eq!(harness.state().1["stocks"][0].quantity, 2.0);
        harness.get_by_label("Save").click();
        harness.run();
        harness.get_by_label("Replace").click();
        harness.run();
        assert_eq!(
            harness.state().1["stocks"]
                .iter()
                .map(|p| p.name.as_str())
                .collect::<Vec<_>>(),
            ["Scalp", "Other"]
        );
        assert_eq!(harness.state().1["stocks"][0].quantity, 5.0);
        assert!(harness.state().1["stocks"][0].applied);
        let mut kept = harness.state().1["stocks"][0].clone();
        kept.sizing = Some(PresetSizing::RiskPercent(1.0));
        kept.order_type = Some(Kind::Limit);
        kept.take_profit = Some(Bracket::new(BracketMode::Ticks, 8.0));
        kept.take_profit_levels = vec![PresetLevel::new(
            Bracket::new(BracketMode::Ticks, 16.0),
            50.0,
        )];
        kept.first_level_stop_loss = Some(Bracket::new(BracketMode::Ticks, 4.0));
        harness.state_mut().1.get_mut("stocks").unwrap()[0] = kept.clone();
        harness.state_mut().1.get_mut("stocks").unwrap().remove(1);
        harness.run();
        harness.get_by_label("Rename preset").click();
        harness.run();
        harness.get_by_label("New name").focus();
        harness.run();
        harness.get_by_label("New name").type_text("Swing");
        harness.run();
        harness.get_by_label("Rename").click();
        harness.run();
        kept.name = "Swing".into();
        assert_eq!(harness.state().1["stocks"], [kept.clone()]);
        harness
            .state_mut()
            .1
            .get_mut("stocks")
            .unwrap()
            .extend((1..10).map(|i| OrderPreset::new(format!("Other {i}"), 1.0)));
        harness.state_mut().0.preset_name = "Eleventh".into();
        harness.run();
        assert!(harness.get_by_label("Save").accesskit_node().is_disabled());
        harness.state_mut().0.preset_name = "Swing".into();
        harness.run();
        assert!(!harness.get_by_label("Save").accesskit_node().is_disabled());
        harness.state_mut().1.get_mut("stocks").unwrap().truncate(1);
        harness.run();
        harness.get_by_label("Rename preset").click();
        harness.run();
        harness
            .state_mut()
            .1
            .get_mut("stocks")
            .unwrap()
            .push(OrderPreset::new("Other", 1.0));
        harness.get_by_label("New name").focus();
        harness.run();
        harness.get_by_label("New name").type_text("Other");
        harness.run();
        assert!(
            harness
                .get_by_label("Rename")
                .accesskit_node()
                .is_disabled()
        );
        harness.get_by_label("Cancel").click();
        harness.run();
        harness.state_mut().1.get_mut("stocks").unwrap().truncate(1);
        harness.run();
        harness.get_by_label("Delete preset").click();
        harness.run();
        assert!(!harness.state().1.contains_key("stocks"));
    }

    /// A preset the account or the order cannot take in full is applied in part, and the ticket
    /// names each parameter it left out until a preset that fits or another account replaces
    /// the notice.
    #[test]
    fn presets_name_the_parameters_they_cannot_apply() {
        use super::super::{Bracket, BracketMode, Kind, PresetLevel, PresetSizing};
        let rules = TradingRules::new("ES");
        let base = super::super::tests::market(&rules).capabilities;
        let ticks = |value| Bracket::new(BracketMode::Ticks, value);
        let trailing = OrderPreset {
            order_type: Some(Kind::TrailingStop),
            time_in_force: Some(TimeInForce::FillOrKill),
            ..OrderPreset::new("Preset 1", 1.0)
        };
        let fits = OrderPreset {
            order_type: Some(Kind::Limit),
            time_in_force: Some(TimeInForce::Day),
            ..OrderPreset::new("Preset 2", 1.0)
        };
        let risk = OrderPreset {
            sizing: Some(PresetSizing::RiskPercent(1.0)),
            stop_loss: Some(ticks(8.0)),
            ..OrderPreset::new("Preset 3", 1.0)
        };
        let levels = |stop_loss| OrderPreset {
            take_profit: Some(ticks(8.0)),
            take_profit_levels: vec![PresetLevel {
                stop_loss,
                ..PresetLevel::new(ticks(16.0), 50.0)
            }],
            ..OrderPreset::new("Preset 4", 1.0)
        };
        let stop = OrderPreset {
            order_type: Some(Kind::Stop),
            ..OrderPreset::new("Preset 5", 1.0)
        };
        let working = Order::new(
            OrderId::new("1"),
            OrderRequest::new("ES", OrderSide::Buy, 1.0, OrderType::Limit(99.5)),
            crate::model::OrderStatus::Working,
        );
        for (row, (capabilities, editing, applied, switch, expected)) in [
            (
                base,
                false,
                vec![trailing.clone()],
                false,
                &[
                    "Order type Trailing stop is not available for this account",
                    "Time in force Fill or kill is not offered for this symbol",
                ][..],
            ),
            (base, false, vec![trailing.clone(), fits], false, &[]),
            (
                Capabilities {
                    brackets: false,
                    ..base
                },
                false,
                vec![risk],
                false,
                &[
                    "Risk sizing is not available for this order, so the quantity is in units",
                    "Take profit and stop loss are not available for this order",
                ],
            ),
            (
                base,
                false,
                vec![levels(None)],
                false,
                &[
                    "Take-profit levels are not available for this account, so the first take profit closes the whole order",
                ],
            ),
            (
                Capabilities {
                    take_profit_levels: true,
                    ..base
                },
                false,
                vec![levels(Some(ticks(4.0)))],
                false,
                &[
                    "A stop loss for each take-profit level is not available for this account, so the order's stop loss protects every level",
                ],
            ),
            (
                base,
                true,
                vec![stop],
                false,
                &["Order type Stop cannot be applied to a working order"],
            ),
            (base, false, vec![trailing.clone()], true, &[]),
        ]
        .into_iter()
        .enumerate()
        {
            let mut ticket = OrderTicket {
                open: true,
                symbol: "ES".into(),
                ..OrderTicket::default()
            };
            if editing {
                ticket.edit(&working);
            }
            let presets = BTreeMap::from([
                ("a".to_owned(), applied.clone()),
                ("b".to_owned(), applied.clone()),
            ]);
            let orders = vec![working.clone()];
            let mut harness = egui_kittest::Harness::builder()
                .with_context_setup(egui_extras::install_image_loaders)
                .with_size(vec2(700.0, 900.0))
                .with_accessibility_check(false)
                .build_ui_state(
                    |ui,
                     (ticket, presets, account): &mut (
                        OrderTicket,
                        BTreeMap<String, Vec<OrderPreset>>,
                        &'static str,
                    )| {
                        let mut market = super::super::tests::market(&rules);
                        market.capabilities = capabilities;
                        market.account = account;
                        market.orders = &orders;
                        ticket.show(ui, &market, presets, &mut false, &mut false);
                    },
                    (ticket, presets, "a"),
                );
            harness.run();
            harness.get_by_label("More").click();
            harness.run();
            for preset in &applied {
                harness.get_by_label(&preset.name).click();
                harness.run();
            }
            if switch {
                harness.state_mut().2 = "b";
                harness.run();
            }
            let notice: Vec<String> = harness
                .query_all_by_role(egui::accesskit::Role::Label)
                .filter_map(|node| node.value())
                .filter(|text| {
                    ["not available", "not offered", "cannot be applied"]
                        .iter()
                        .any(|part| text.contains(part))
                })
                .collect();
            assert_eq!(notice, expected, "row {row}");
            if row == 0 {
                assert_eq!(harness.state().0.kind, Kind::Market);
                assert_eq!(harness.state().0.time_in_force, TimeInForce::Day);
            }
        }
    }

    /// The broker rules control visibility, and a wall time at UTC-5 reaches the broker at UTC.
    #[test]
    fn good_till_date_expiry_is_entered_in_the_charts_zone() {
        let mut rules = TradingRules::new("ES");
        rules.time_in_force.push(TimeInForce::GoodTillDate);
        let mut harness = egui_kittest::Harness::builder()
            .allow_missing_glyphs()
            .with_size(vec2(340.0, 1200.0))
            .with_accessibility_check(false)
            .build_ui_state(
                |ui, (ticket, rules): &mut (OrderTicket, TradingRules)| {
                    let mut market = super::super::tests::market(rules);
                    market.timezone = crate::scales::TimezoneMode::Timezone(
                        crate::model::TimezoneId::fixed_offset(-5 * 3600).unwrap(),
                    );
                    ticket.show(ui, &market, &mut BTreeMap::new(), &mut false, &mut false);
                },
                (
                    OrderTicket {
                        symbol: "ES".into(),
                        time_in_force: TimeInForce::GoodTillDate,
                        ..OrderTicket::default()
                    },
                    rules,
                ),
            );
        harness.run();
        harness
            .get_by_role_and_label(egui::Role::TextInput, "Expiry")
            .focus();
        harness.run();
        harness
            .get_by_role_and_label(egui::Role::TextInput, "Expiry")
            .type_text("2026-12-01 16:00");
        harness.run();
        let (ticket, rules) = harness.state();
        let market = super::super::tests::market(rules);
        assert_eq!(
            ticket.request(&harness.ctx, &market).unwrap().expires,
            Some("2026-12-01T21:00:00Z".parse().unwrap())
        );
        harness.state_mut().1.time_in_force = vec![TimeInForce::Day];
        harness.run();
        assert!(harness.query_by_label("Expiry").is_none());
        assert_eq!(harness.state().0.time_in_force, TimeInForce::Day);
    }

    /// Without the account's capability the ticket shows no trailing stop; with it, the
    /// Trailing tab offers the distance and the activation price and sends a trailing stop.
    #[test]
    fn the_trailing_tab_needs_the_capability() {
        let rules = TradingRules::new("ES");
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for accepted in [false, true] {
                let mut harness = egui_kittest::Harness::builder()
                    .allow_missing_glyphs()
                    .with_size(vec2(340.0, 1200.0))
                    .with_accessibility_check(false)
                    .build_ui_state(
                        |ui, (ticket, accepted): &mut (OrderTicket, bool)| {
                            let mut market = super::super::tests::market(&rules);
                            market.capabilities.trailing_stop = *accepted;
                            ticket.show(ui, &market, &mut BTreeMap::new(), &mut false, &mut false);
                        },
                        (
                            OrderTicket {
                                symbol: "ES".into(),
                                ..OrderTicket::default()
                            },
                            accepted,
                        ),
                    );
                crate::theme::apply_to_egui(&harness.ctx, &theme);
                harness.run();
                if !accepted {
                    assert!(harness.query_by_label("Trailing").is_none());
                    continue;
                }
                harness.get_by_label("Trailing").click();
                harness.run();
                harness.get_by_label("Trailing distance");
                harness.get_by_role_and_label(egui::Role::Switch, "Activation price");
                harness.get_by_label("Buy 1 ES TRAIL 10 ticks");
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
                    symbol: "ES",
                    formatter: None,
                    pip_value: None,
                    last: Some(100.0),
                    bid: None,
                    ask: None,
                    tick: 0.25,
                    tick_tiers: &[],
                    decimals: 2,
                    currency: "USD",
                    balance: None,
                    available_funds: None,
                    rules: &rules,
                    rules_ready: true,
                    offline: None,
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
                    pnl_unit: crate::chart::PnlUnit::Money,
                    logo: None,
                    account: "",
                    timezone: crate::scales::TimezoneMode::Utc,
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
                            ticket.show(
                                ui,
                                &market,
                                &mut std::collections::BTreeMap::new(),
                                &mut false,
                                &mut false,
                            );
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
                        ticket.show(
                            ui,
                            &market,
                            &mut std::collections::BTreeMap::new(),
                            &mut false,
                            &mut false,
                        );
                    })
                    .drop_without_applying_deltas();
                    assert_eq!(
                        ticket.price, price,
                        "an invalid entered price must not become the market price"
                    );
                    assert!(ticket.request(&egui::Context::default(), &market).is_err());
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
            bid: Some(100.0),
            ask: Some(100.25),
            tick: 0.25,
            tick_tiers: &[],
            decimals: 2,
            currency: "USD",
            balance: None,
            available_funds: None,
            capabilities,
            rules: &rules,
            rules_ready: true,
            offline: None,
            ladder: None,
            ladder_unavailable: None,
            highlights: Vec::new(),
            orders: &working,
            positions: &[],
            trades: &[],
            session_open: None,
            last_fill: None,
            pnl_unit: crate::chart::PnlUnit::Money,
            logo: None,
            account: "",
            timezone: crate::scales::TimezoneMode::Utc,
        };
        let cases = [
            ("Bid size", "99.50", "ctrl", false, true, None),
            (
                "Bid size",
                "99.50",
                "click",
                false,
                true,
                Some((OrderSide::Buy, OrderType::Limit(99.5))),
            ),
            (
                "Bid size",
                "100.50",
                "click",
                false,
                true,
                Some((OrderSide::Buy, OrderType::Limit(100.5))),
            ),
            (
                "Bid size",
                "100.50",
                "ctrl",
                false,
                true,
                Some((OrderSide::Buy, OrderType::Stop(100.5))),
            ),
            ("Bid size", "100.25", "ctrl", false, true, None),
            (
                "Ask size",
                "99.50",
                "ctrl",
                false,
                true,
                Some((OrderSide::Sell, OrderType::Stop(99.5))),
            ),
            ("Ask size", "100.00", "ctrl", false, true, None),
            (
                "Ask size",
                "99.75",
                "click",
                false,
                true,
                Some((OrderSide::Sell, OrderType::Limit(99.75))),
            ),
            (
                "Bid size",
                "99.50",
                "Buy limit 99.50",
                false,
                true,
                Some((OrderSide::Buy, OrderType::Limit(99.5))),
            ),
            (
                "Bid size",
                "99.50",
                "Sell stop 99.50",
                false,
                true,
                Some((OrderSide::Sell, OrderType::Stop(99.5))),
            ),
            ("Bid size", "99.50", "Buy stop 99.50", false, true, None),
            (
                "View",
                "99.50",
                "Sell stop limit 99.50",
                true,
                true,
                Some((
                    OrderSide::Sell,
                    OrderType::StopLimit {
                        stop: 99.5,
                        limit: 99.5,
                    },
                )),
            ),
            (
                "Bid size",
                "99.50",
                "click",
                false,
                false,
                Some((OrderSide::Buy, OrderType::Limit(99.5))),
            ),
            ("CXL All", "", "click", false, true, None),
        ];
        for (column, row, gesture, stop_limit, instant, expected) in cases {
            let ctx = egui::Context::default();
            ctx.all_styles_mut(|style| {
                style.interaction.tooltip_delay = 0.0;
                style.interaction.show_tooltips_only_when_still = false;
            });
            let market = Market {
                capabilities: Capabilities {
                    stop_limit,
                    ..capabilities
                },
                offline: market.offline.clone(),
                ladder: market.ladder.clone(),
                highlights: market.highlights.clone(),
                timezone: market.timezone.clone(),
                ..market
            };
            let mut ticket = OrderTicket {
                open: true,
                symbol: "ES".into(),
                tab: Tab::Dom,
                ..OrderTicket::default()
            };
            let mut one_click = instant;
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
                    |ui| {
                        action = ticket.show(
                            ui,
                            &market,
                            &mut std::collections::BTreeMap::new(),
                            &mut one_click,
                            &mut false,
                        )
                    },
                );
                let labels: Vec<_> = output
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
                (action, labels)
            };
            let find = |labels: &[(String, egui::Pos2)], label: &str| {
                labels
                    .iter()
                    .find(|(text, _)| text == label)
                    .unwrap_or_else(|| panic!("missing {label}: {labels:?}"))
                    .1
            };
            frame(vec![]);
            let labels = frame(vec![]).1;
            let cell = if row.is_empty() {
                find(&labels, column)
            } else {
                pos2(find(&labels, column).x, find(&labels, row).y)
            };
            let modifiers = if gesture == "ctrl" {
                egui::Modifiers::COMMAND
            } else {
                egui::Modifiers::NONE
            };
            frame(vec![
                egui::Event::ModifiersChanged(modifiers),
                egui::Event::PointerMoved(cell),
            ]);
            frame(vec![]);
            if expected.is_none() && matches!(gesture, "click" | "ctrl") && !row.is_empty() {
                let labels = frame(vec![]).1;
                let reason = if column == "Bid size" {
                    "A buy stop must lie above the market"
                } else {
                    "A sell stop must lie below the market"
                };
                assert!(labels.iter().any(|(text, _)| text == reason), "{labels:?}");
            }
            let button = |pos, button, pressed| egui::Event::PointerButton {
                pos,
                button,
                pressed,
                modifiers,
            };
            let menu = !matches!(gesture, "click" | "ctrl");
            let kind = if menu {
                egui::PointerButton::Secondary
            } else {
                egui::PointerButton::Primary
            };
            frame(vec![button(cell, kind, true)]);
            let (mut action, _) = frame(vec![button(cell, kind, false)]);
            if menu {
                let labels = frame(vec![]).1;
                for label in [
                    "Buy limit 99.50",
                    "Sell limit 99.50",
                    "Buy stop 99.50",
                    "Sell stop 99.50",
                ] {
                    find(&labels, label);
                }
                assert_eq!(
                    labels
                        .iter()
                        .any(|(text, _)| text == "Sell stop limit 99.50"),
                    stop_limit
                );
                let choice = find(&labels, gesture);
                frame(vec![egui::Event::PointerMoved(choice)]);
                frame(vec![button(choice, egui::PointerButton::Primary, true)]);
                action = frame(vec![button(choice, egui::PointerButton::Primary, false)]).0;
            }
            let expected =
                expected.map(|(side, order_type)| OrderRequest::new("ES", side, 1.0, order_type));
            if row.is_empty() {
                assert_eq!(action, None, "bulk cancellation waits for confirmation");
                assert_eq!(ticket.cancel, Some(vec![OrderId::new("bid")]));
            } else if instant {
                assert_eq!(
                    action,
                    expected.map(Action::Place),
                    "{gesture}: {column} at {row}"
                );
                assert_eq!(ticket.confirm, None);
            } else {
                assert_eq!(action, None);
                assert_eq!(ticket.confirm, expected);
            }
        }
    }
    /// Tag gestures open one modification, keep order metadata and a position exit's quantity,
    /// and respect account support.
    #[test]
    fn ladder_tags_open_their_order_for_one_modification() {
        use crate::chart::TradingLineAction;
        use crate::model::{OrderStatus, PositionBracket};
        for (gesture, modify, expiry, exit) in [
            ("tab-double-click", true, false, false),
            ("double-click", true, false, false),
            ("menu", true, false, false),
            ("double-click", false, false, false),
            ("menu", false, false, false),
            ("double-click", true, true, false),
            ("double-click", true, false, true),
        ] {
            let ctx = egui::Context::default();
            ctx.all_styles_mut(|style| {
                style.interaction.tooltip_delay = 0.0;
                style.interaction.show_tooltips_only_when_still = false;
            });
            let mut rules = TradingRules::new("ES");
            rules.time_in_force.push(TimeInForce::GoodTillDate);
            let mut request = OrderRequest::new("ES", OrderSide::Buy, 1.0, OrderType::Limit(99.5));
            if exit {
                request.position_bracket = Some(PositionBracket::TakeProfit);
            } else {
                request.take_profit = Some(110.0);
            }
            if expiry {
                request.time_in_force = TimeInForce::GoodTillDate;
                request.expires = Some("2026-12-01T21:00:00Z".parse().unwrap());
            }
            let order = Order::new(OrderId::new("bid"), request, OrderStatus::Working);
            let orders = [order.clone()];
            let market = Market {
                symbol: "ES",
                formatter: None,
                pip_value: None,
                last: Some(100.0),
                bid: Some(100.0),
                ask: Some(100.25),
                tick: 0.25,
                tick_tiers: &[],
                decimals: 2,
                currency: "USD",
                balance: None,
                available_funds: None,
                capabilities: Capabilities {
                    market: true,
                    limit: true,
                    stop: true,
                    modify,
                    ..Default::default()
                },
                rules: &rules,
                rules_ready: true,
                offline: None,
                ladder: None,
                ladder_unavailable: None,
                highlights: Vec::new(),
                orders: &orders,
                positions: &[],
                trades: &[],
                session_open: None,
                last_fill: None,
                pnl_unit: crate::chart::PnlUnit::Money,
                logo: None,

                account: "",
                timezone: crate::scales::TimezoneMode::Utc,
            };
            let mut ticket = OrderTicket {
                open: true,
                symbol: "ES".into(),
                tab: if gesture == "tab-double-click" {
                    Tab::Order
                } else {
                    Tab::Dom
                },
                ..Default::default()
            };
            let frame = |ticket: &mut OrderTicket, market: &Market<'_>, events| {
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
                    |ui| {
                        action = ticket.show(
                            ui,
                            market,
                            &mut std::collections::BTreeMap::new(),
                            &mut true,
                            &mut false,
                        )
                    },
                );
                let labels: Vec<_> = output
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
                (action, labels)
            };
            let find = |labels: &[(String, egui::Pos2)], label: &str| {
                labels
                    .iter()
                    .find(|(text, _)| text == label)
                    .unwrap_or_else(|| panic!("missing {label}: {labels:?}"))
                    .1
            };
            let click = |pos, pressed| egui::Event::PointerButton {
                pos,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            frame(&mut ticket, &market, vec![]);
            let mut labels = frame(&mut ticket, &market, vec![]).1;
            if gesture == "tab-double-click" {
                let tab = find(&labels, "DOM");
                frame(&mut ticket, &market, vec![egui::Event::PointerMoved(tab)]);
                frame(&mut ticket, &market, vec![click(tab, true)]);
                frame(&mut ticket, &market, vec![click(tab, false)]);
                labels = frame(&mut ticket, &market, vec![]).1;
            }
            let tag = find(&labels, "LIMIT");
            frame(&mut ticket, &market, vec![egui::Event::PointerMoved(tag)]);
            let labels = frame(&mut ticket, &market, vec![]).1;
            if gesture != "tab-double-click" {
                let hint = labels
                    .iter()
                    .find(|(text, _)| text.contains("right-click to cancel"))
                    .expect("order hint");
                assert_eq!(hint.0.contains("Drag to move"), modify);
                assert_eq!(
                    hint.0
                        .contains("The account does not support modifying orders"),
                    !modify
                );
            }
            frame(&mut ticket, &market, vec![click(tag, true)]);
            frame(&mut ticket, &market, vec![click(tag, false)]);
            if gesture != "menu" {
                frame(&mut ticket, &market, vec![click(tag, true)]);
                frame(&mut ticket, &market, vec![click(tag, false)]);
            } else {
                let labels = frame(&mut ticket, &market, vec![]).1;
                let edit = find(&labels, "Edit order");
                frame(&mut ticket, &market, vec![egui::Event::PointerMoved(edit)]);
                frame(&mut ticket, &market, vec![click(edit, true)]);
                frame(&mut ticket, &market, vec![click(edit, false)]);
            }
            assert_eq!(
                ticket.tab,
                if modify { Tab::Order } else { Tab::Dom },
                "{gesture}"
            );
            assert_eq!(ticket.editing.as_ref(), modify.then_some(&order));
            if !modify || gesture == "menu" {
                continue;
            }
            frame(&mut ticket, &market, vec![]);
            let labels = frame(&mut ticket, &market, vec![]).1;
            assert!(
                labels
                    .iter()
                    .any(|(text, _)| text == "Change the order to modify it"),
                "{labels:?}"
            );
            let send = find(&labels, "Modify order");
            frame(&mut ticket, &market, vec![egui::Event::PointerMoved(send)]);
            frame(&mut ticket, &market, vec![click(send, true)]);
            assert_eq!(
                frame(&mut ticket, &market, vec![click(send, false)]).0,
                None
            );
            ticket.quantity = 2.0;
            if exit {
                // A position exit keeps its quantity; only its price can change.
                let labels = frame(&mut ticket, &market, vec![]).1;
                find(&labels, "A position exit keeps its quantity");
                let send = find(&labels, "Modify order");
                frame(&mut ticket, &market, vec![egui::Event::PointerMoved(send)]);
                frame(&mut ticket, &market, vec![click(send, true)]);
                assert_eq!(
                    frame(&mut ticket, &market, vec![click(send, false)]).0,
                    None
                );
                ticket.quantity = 1.0;
                ticket.price = 99.75;
                let labels = frame(&mut ticket, &market, vec![]).1;
                let send = find(&labels, "Modify order");
                frame(&mut ticket, &market, vec![egui::Event::PointerMoved(send)]);
                frame(&mut ticket, &market, vec![click(send, true)]);
                let mut changed = order.request.clone();
                changed.order_type = OrderType::Limit(99.75);
                assert_eq!(
                    frame(&mut ticket, &market, vec![click(send, false)]).0,
                    Some(Action::Confirm(Box::new(TradingLineAction::Modify(
                        order.clone(),
                        changed
                    ))))
                );
                continue;
            }
            if expiry {
                ticket.expires = Some("2026-12-01T22:00:00Z".parse().unwrap());
            }
            let labels = frame(&mut ticket, &market, vec![]).1;
            let send = find(&labels, "Modify order");
            frame(&mut ticket, &market, vec![egui::Event::PointerMoved(send)]);
            frame(&mut ticket, &market, vec![click(send, true)]);
            let action = frame(&mut ticket, &market, vec![click(send, false)]).0;
            let mut changed = order.request.clone();
            changed.quantity = 2.0;
            if expiry {
                changed.expires = Some("2026-12-01T22:00:00Z".parse().unwrap());
            }
            assert_eq!(
                action,
                Some(Action::Confirm(Box::new(TradingLineAction::Modify(
                    order.clone(),
                    changed
                ))))
            );
            assert_eq!(ticket.editing, None);
            assert_eq!(ticket.tab, Tab::Dom);

            ticket.edit(&order);
            let without_modify = Market {
                capabilities: Capabilities {
                    modify: false,
                    ..market.capabilities
                },
                ..market
            };
            let labels = frame(&mut ticket, &without_modify, vec![]).1;
            assert!(
                labels
                    .iter()
                    .any(|(text, _)| text == "The account does not support modifying orders")
            );
            let gone = Market {
                orders: &[],
                ..without_modify
            };
            assert_eq!(frame(&mut ticket, &gone, vec![]).0, None);
            assert_eq!(ticket.editing, None);
            assert_eq!(ticket.tab, Tab::Dom);
        }
    }
}
