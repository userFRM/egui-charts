//! The account panel's view: [`Trading::show_account_panel`] with its tabs, tables and dialogs.

use crate::localization as l10n;

use egui::{Align, Align2, Color32, FontId, Layout, Rect, RichText, Sense, Stroke, Ui, vec2};

use super::{
    ACTIONS_WIDTH, AccountPanelAction, Cell, Column, HEADER_HEIGHT, ROW_HEIGHT, Row, RowAction,
    TAB_HEIGHT, Tab, Trading, csv, kind, pnl_color, quantity_decimals, status, time,
};
use crate::chart::TradingLineAction;
use crate::icons::icons;
use crate::model::{Order, OrderSide, OrderType};
use crate::theme::ThemeExt;
use crate::ui::connection_status::{show_connection_status, show_connection_status_with_label};
use crate::ui::top_toolbar::buttons::rule_color;
use crate::ui::trading::widgets::{grouped, semibold, side_color};
use crate::ui_kit::buttons::ButtonSize;
use crate::ui_kit::{EmptyState, IconButton};

impl Trading {
    /// Shows the account panel in `ui`, such as a bottom panel of the terminal, for a chart
    /// showing `chart_symbol`. Returns what the user asked of the host; cancelling, changing and
    /// closing go to the broker after the user confirms them in [`show`](Self::show).
    pub fn show_account_panel(
        &mut self,
        ui: &mut Ui,
        chart_symbol: &str,
    ) -> Option<AccountPanelAction> {
        let mut action = None;
        let theme = ui.active_theme();
        let currency = self
            .account
            .as_ref()
            .map_or(String::new(), |account| account.currency.clone());

        // On the tab, which every panel height shows, until the broker reports it is live.
        let mark = self.offline();

        ui.painter().rect_filled(
            Rect::from_min_size(ui.cursor().min, vec2(ui.available_width(), TAB_HEIGHT)),
            0.0,
            theme.ui.btn_bg_hover,
        );
        // Tab strip: the broker's tab with a Trade button, then minimize and maximize.
        ui.horizontal(|ui| {
            ui.set_height(TAB_HEIGHT);
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if IconButton::new(&icons::MAXIMIZE)
                    .icon_scale(0.9)
                    .tooltip("Maximize panel")
                    .show(ui)
                    .clicked()
                {
                    action = Some(AccountPanelAction::Maximize);
                }
                if IconButton::new(&icons::MINUS)
                    .icon_scale(0.9)
                    .tooltip("Minimize panel")
                    .show(ui)
                    .clicked()
                {
                    action = Some(AccountPanelAction::Minimize);
                }
                ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                    let tab = egui::Frame::new()
                        .fill(theme.ui.panel_bg)
                        .corner_radius(ui.active_theme().radius.sm)
                        .inner_margin(egui::Margin::symmetric(10, 4));
                    tab.show(ui, |ui| {
                        ui.add(icons::CANDLESTICK.as_image_tinted(vec2(20.0, 20.0), theme.ui.text));
                        let (label, radius) =
                            (l10n::text(ui.ctx(), "Trade"), ui.active_theme().radius.sm);
                        let trade = || {
                            egui::Button::new(label.clone())
                                .min_size(vec2(63.0, 25.0))
                                .corner_radius(radius)
                        };
                        let name = RichText::new(l10n::text(ui.ctx(), &self.account_panel.broker))
                            .color(theme.ui.text);
                        let width = |ui: &mut Ui, show: &dyn Fn(&mut Ui) -> egui::Response| {
                            show(&mut ui.new_child(egui::UiBuilder::new().invisible()))
                                .rect
                                .width()
                        };
                        // The mark's label, then the broker's name, give way, so that Trade stays
                        // clear of the panel's buttons; the mark's icon stays. The tab may take
                        // the gap before the buttons, as its margin keeps Trade off them.
                        let spacing = ui.spacing().item_spacing.x;
                        let room =
                            ui.available_width() + spacing - width(ui, &|ui| ui.add(trade()));
                        let labelled = mark.as_ref().is_some_and(|mark| {
                            width(ui, &|ui| ui.label(name.clone()))
                                + width(ui, &|ui| show_connection_status_with_label(ui, mark))
                                + 2.0 * spacing
                                <= room
                        });
                        let status = mark.as_ref().map_or(0.0, |mark| match labelled {
                            true => width(ui, &|ui| show_connection_status_with_label(ui, mark)),
                            false => width(ui, &|ui| show_connection_status(ui, mark)),
                        } + spacing);
                        let name_width = (room - status - spacing).max(0.0);
                        ui.allocate_ui(vec2(name_width, ui.available_height()), |ui| {
                            ui.add(egui::Label::new(name).truncate());
                        });
                        if let Some(mark) = &mark {
                            match labelled {
                                true => show_connection_status_with_label(ui, mark),
                                false => show_connection_status(ui, mark),
                            };
                        }
                        if ui
                            .add(trade())
                            .on_hover_text(l10n::text(ui.ctx(), "Open the order ticket"))
                            .clicked()
                        {
                            self.toggle_ticket(chart_symbol.to_owned());
                        }
                    });
                });
            });
        });

        if ui.available_height() < HEADER_HEIGHT {
            return action;
        }
        // Keep selectors and every summary figure reachable in narrow panels.
        egui::ScrollArea::horizontal()
            .id_salt("account_summary")
            .auto_shrink([false, true])
            .show(ui, |ui| {
                // Account selectors, then the summary from the right edge as far as there is room.
                let (mut row, _) = ui
                    .allocate_exact_size(vec2(ui.available_width(), HEADER_HEIGHT), Sense::hover());
                let mut selectors = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(row.with_min_x(row.left() + 8.0))
                        .layout(Layout::left_to_right(Align::Center)),
                );
                let panel = &mut self.account_panel;
                let chevron = || icons::CHEVRON_DOWN.as_image_square(16.0);
                let broker = selectors.add(
                    egui::Button::new((
                        semibold(&panel.broker, theme.text.body, theme.ui.text),
                        chevron(),
                    ))
                    .frame_when_inactive(false)
                    .image_tint_follows_text_color(true),
                );
                egui::Popup::menu(&broker).show(|ui| {
                    if ui.button(l10n::text(ui.ctx(), "Disconnect")).clicked() {
                        action = Some(AccountPanelAction::Disconnect);
                    }
                });
                let name = panel.accounts.get(panel.active).map_or("", String::as_str);
                let account = selectors.add(
                    egui::Button::new((format!("{name} {currency}").trim().to_owned(), chevron()))
                        .frame_when_inactive(false)
                        .image_tint_follows_text_color(true),
                );
                egui::Popup::menu(&account).show(|ui| {
                    for (index, name) in panel.accounts.iter().enumerate() {
                        if ui
                            .selectable_label(index == panel.active, l10n::text(ui.ctx(), name))
                            .clicked()
                        {
                            action = Some(AccountPanelAction::SelectAccount(index));
                        }
                    }
                });
                if IconButton::new(&icons::SETTINGS_NUT)
                    .tooltip("Account settings")
                    .show(&mut selectors)
                    .clicked()
                {
                    action = Some(AccountPanelAction::Settings);
                }
                if let Some(account) = &self.account {
                    let unrealized = account.equity.map(|equity| equity - account.balance);
                    let figures = [
                        ("Account balance", Some(account.balance), false, None),
                        ("Equity", account.equity, false, None),
                        ("Realized P&L", account.realized_pnl, true, None),
                        ("Unrealized P&L", unrealized, true, None),
                        (
                            "Account margin",
                            account.margin,
                            false,
                            Some("The margin the open positions hold"),
                        ),
                        (
                            "Available funds",
                            account.available_funds,
                            false,
                            Some("Equity less the margin of the positions and the working orders"),
                        ),
                        (
                            "Orders margin",
                            account.orders_margin,
                            false,
                            Some("The margin the working orders hold once filled"),
                        ),
                    ];
                    let painter = ui.painter();
                    let value_font = FontId::proportional(theme.text.title);
                    let figures: Vec<_> = figures
                        .into_iter()
                        .filter_map(|(label, value, signed, info)| {
                            let value = value?;
                            let text = grouped(value, 2, signed);
                            let color = if signed {
                                pnl_color(&theme, value, &text)
                            } else {
                                theme.ui.text_secondary
                            };
                            let label = painter.layout_job(semibold(
                                &l10n::text(ui.ctx(), label),
                                theme.text.small,
                                theme.ui.text,
                            ));
                            let value = painter.layout_no_wrap(
                                l10n::text(ui.ctx(), text),
                                value_font.clone(),
                                color,
                            );
                            let info_width = if info.is_some() { 18.0 } else { 0.0 };
                            let width = (label.size().x + info_width).max(value.size().x);
                            Some((label, value, color, info, width))
                        })
                        .collect();
                    let summary_width: f32 = figures.iter().map(|figure| figure.4 + 28.0).sum();
                    row.max.x = row
                        .right()
                        .max(selectors.min_rect().right() + 24.0 + summary_width);
                    ui.expand_to_include_rect(row);
                    let mut right = row.right() - 16.0;
                    for (label, value, color, info, width) in figures.into_iter().rev() {
                        let left = right - width;
                        let top = row.center().y - (label.size().y + value.size().y + 2.0) / 2.0;
                        if let Some(info) = info {
                            let icon = Rect::from_min_size(
                                egui::pos2(
                                    left + label.size().x + 4.0,
                                    top + label.size().y / 2.0 - 7.0,
                                ),
                                vec2(14.0, 14.0),
                            );
                            icons::INFO
                                .as_image_tinted(icon.size(), theme.ui.text_muted)
                                .paint_at(ui, icon);
                            ui.interact(icon, ui.scope_id().with(info), Sense::hover())
                                .on_hover_text(l10n::text(ui.ctx(), info));
                        }
                        let painter = ui.painter();
                        painter.galley(egui::pos2(left, top + label.size().y + 2.0), value, color);
                        painter.galley(egui::pos2(left, top), label, theme.ui.text);
                        right = left - 28.0;
                    }
                }
            });

        // Tabs, with the number of positions and working orders, over a track that carries the
        // active tab's underline.
        egui::ScrollArea::horizontal()
            .id_salt("account_tabs")
            .show(ui, |ui| {
                let mut underline = None;
                let tabs = ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.spacing_mut().item_spacing.x = 20.0;
                    for tab in Tab::ALL {
                        let count = match tab {
                            Tab::Positions => self.positions.len(),
                            Tab::Orders => self.orders.len(),
                            _ => 0,
                        };
                        let mut title = RichText::new(l10n::text(ui.ctx(), tab.title()));
                        let active = self.account_panel.tab == tab;
                        title = title.color(if active {
                            theme.ui.text
                        } else {
                            theme.ui.text_secondary
                        });
                        let label = if count > 0 {
                            egui::Atoms::new((
                                title,
                                RichText::new(l10n::text(ui.ctx(), count.to_string()))
                                    .color(theme.ui.text_secondary),
                            ))
                        } else {
                            egui::Atoms::new(title)
                        };
                        let response = ui.add(egui::Button::new(label).frame(false));
                        if active {
                            underline = Some(response.rect.x_range());
                        }
                        if response.clicked() {
                            self.account_panel.tab = tab;
                        }
                    }
                });
                let (track, _) = ui.allocate_exact_size(
                    vec2(ui.available_width().max(tabs.response.rect.width()), 4.0),
                    Sense::hover(),
                );
                ui.painter().rect_filled(track, 0.0, rule_color(&theme));
                if let Some(x_range) = underline {
                    ui.painter().rect_filled(
                        Rect::from_x_y_ranges(x_range, track.y_range()),
                        0.0,
                        theme.ui.text,
                    );
                }
            });
        if self.account_panel.tab == Tab::History {
            ui.horizontal(|ui| {
                ui.selectable_value(
                    &mut self.account_panel.show_executions,
                    false,
                    l10n::text(ui.ctx(), "Orders"),
                );
                ui.selectable_value(
                    &mut self.account_panel.show_executions,
                    true,
                    l10n::text(ui.ctx(), "Executions"),
                );
            });
        }

        let tab = if self.account_panel.tab == Tab::History && self.account_panel.show_executions {
            Tab::Executions
        } else {
            self.account_panel.tab
        };
        let capabilities = self.capabilities();
        let price = |symbol: &str, value: f64| self.numbers(symbol, chart_symbol).price(value);
        let quantity = |symbol: &str, value: f64| {
            self.numbers(symbol, chart_symbol)
                .quantity(value, quantity_decimals(value))
        };
        let money = |value: Option<f64>, signed: bool| {
            let Some(value) = value else {
                return (String::new(), theme.ui.text, String::new());
            };
            let text = grouped(value, 2, signed);
            let color = if signed {
                pnl_color(&theme, value, &text)
            } else {
                theme.ui.text
            };
            (text, color, currency.clone())
        };
        let plain = |text: String| Cell {
            text,
            color: theme.ui.text,
            unit: String::new(),
            symbol: false,
        };
        let symbol = |symbol: &str| Cell {
            text: symbol.to_owned(),
            color: theme.ui.text,
            unit: String::new(),
            symbol: true,
        };
        let side = |side: OrderSide, text: &str| Cell {
            text: text.to_owned(),
            color: side_color(&theme, side),
            unit: String::new(),
            symbol: false,
        };
        let colored = |(text, color, unit): (String, Color32, String)| Cell {
            text,
            color,
            unit,
            symbol: false,
        };
        let optional =
            |symbol: &str, value: Option<f64>| value.map_or(String::new(), |v| price(symbol, v));
        let order_cells = |order: &Order, finished: bool| {
            let request = &order.request;
            let (limit, stop) = match request.order_type {
                OrderType::Limit(limit) => (Some(limit), None),
                OrderType::Stop(stop) => (None, Some(stop)),
                OrderType::StopLimit { stop, limit } => (Some(limit), Some(stop)),
                _ => (None, None),
            };
            let mut cells = vec![
                symbol(&request.symbol),
                side(
                    request.side,
                    if request.side == OrderSide::Buy {
                        "Buy"
                    } else {
                        "Sell"
                    },
                ),
                plain(kind(&request.order_type).to_owned()),
                plain(quantity(&request.symbol, request.quantity)),
                plain(optional(&request.symbol, limit)),
                plain(optional(&request.symbol, stop)),
            ];
            if finished {
                cells.push(plain(optional(&request.symbol, order.fill_price)));
            } else {
                cells.push(plain(if request.take_profit_levels.is_empty() {
                    optional(&request.symbol, request.take_profit)
                } else {
                    (request.take_profit_levels.iter())
                        .map(|level| price(&request.symbol, level.price))
                        .collect::<Vec<_>>()
                        .join(", ")
                }));
                cells.push(plain(optional(&request.symbol, request.stop_loss)));
            }
            cells.push(plain(status(&order.status)));
            cells.push(plain(time(order.placed)));
            if finished {
                cells.push(plain(time(order.closed)));
            }
            cells.push(plain(order.id.to_string()));
            cells
        };
        let rows: Vec<Row> = match tab {
            Tab::Positions => {
                self.positions
                    .iter()
                    .map(|position| {
                        let s = position.symbol.as_str();
                        let percent = position
                            .unrealized_pnl
                            .zip(position.trade_value)
                            .filter(|(_, value)| *value != 0.0)
                            .map(|(pnl, value)| pnl / value * 100.0);
                        let (long, label) = if position.quantity > 0.0 {
                            (OrderSide::Buy, "Long")
                        } else {
                            (OrderSide::Sell, "Short")
                        };
                        let size = position.quantity.abs();
                        Row {
                            cells: vec![
                                symbol(s),
                                side(long, label),
                                plain(quantity(s, size)),
                                plain(price(s, position.average_price)),
                                plain(optional(s, position.last_price)),
                                colored(money(position.unrealized_pnl, true)),
                                colored({
                                    let (text, color, _) = money(percent, true);
                                    let text = if text.is_empty() { text } else { text + "%" };
                                    (text, color, String::new())
                                }),
                                colored(money(position.trade_value, false)),
                                colored(money(position.market_value, false)),
                                plain(
                                    position
                                        .leverage
                                        .map_or(String::new(), |leverage| format!("{leverage}:1")),
                                ),
                                colored(money(position.margin, false)),
                                plain(position.expiration.map_or(String::new(), |date| {
                                    date.format("%Y-%m-%d").to_string()
                                })),
                            ],
                            symbol: Some(position.symbol.clone()),
                            edit: None,
                            close: capabilities
                                .close
                                .then(|| RowAction::Position(position.clone())),
                        }
                    })
                    .collect()
            }
            Tab::Orders => self
                .orders
                .iter()
                .map(|order| Row {
                    cells: order_cells(order, false),
                    symbol: Some(order.request.symbol.clone()),
                    edit: capabilities.modify.then(|| RowAction::Order(order.clone())),
                    close: Some(RowAction::Order(order.clone())),
                })
                .collect(),
            Tab::History => self
                .account_panel
                .history
                .iter()
                .rev()
                .map(|order| Row {
                    cells: order_cells(order, true),
                    symbol: Some(order.request.symbol.clone()),
                    edit: None,
                    close: None,
                })
                .collect(),
            Tab::Executions => self
                .account_panel
                .fills
                .executions
                .iter()
                .rev()
                .map(|fill| {
                    let order = self.account_panel.fills.orders.get(&fill.order);
                    let name = order.map(|order| order.symbol.as_str()).unwrap_or("");
                    Row {
                        cells: vec![
                            symbol(name),
                            side(fill.side, fill.side.label()),
                            plain(quantity(name, fill.quantity)),
                            plain(price(name, fill.price)),
                            plain(time(Some(fill.time))),
                            plain(fill.order.to_string()),
                        ],
                        symbol: order.map(|order| order.symbol.clone()),
                        edit: None,
                        close: None,
                    }
                })
                .collect(),
            Tab::Balance => self
                .account_panel
                .balance
                .iter()
                .rev()
                .map(|change| Row {
                    cells: vec![
                        plain(time(Some(change.time))),
                        plain(grouped(change.balance - change.amount, 2, false)),
                        plain(grouped(change.balance, 2, false)),
                        colored(money(Some(change.amount), true)),
                        plain(change.action.clone()),
                    ],
                    symbol: None,
                    edit: None,
                    close: None,
                })
                .collect(),
            Tab::Journal => self
                .account_panel
                .journal
                .iter()
                .rev()
                .map(|entry| Row {
                    cells: vec![plain(time(Some(entry.time))), plain(entry.text.clone())],
                    symbol: None,
                    edit: None,
                    close: None,
                })
                .collect(),
        };

        let columns: Vec<(usize, &Column)> = tab
            .columns()
            .iter()
            .enumerate()
            .filter(|(_, column)| !self.account_panel.hidden.contains(&(tab, column.title)))
            .collect();
        let font = FontId::proportional(theme.text.body);
        let heading = FontId::proportional(theme.text.body);
        let unit_font = FontId::proportional(theme.text.tiny);
        // Columns take their smallest width, at least their title's, and share what is left; a
        // narrow panel scrolls. Several take-profit levels share a cell, whose prices must not
        // lose digits.
        let widths: Vec<f32> = columns
            .iter()
            .map(|&(index, column)| {
                let title = ui.painter().layout_no_wrap(
                    l10n::text(ui.ctx(), column.title),
                    heading.clone(),
                    theme.ui.text,
                );
                let cells = rows
                    .iter()
                    .filter(|_| column.title == "Take Profit")
                    .map(|row| {
                        let text = row.cells[index].text.clone();
                        let cell = ui
                            .painter()
                            .layout_no_wrap(text, font.clone(), theme.ui.text);
                        cell.size().x + 16.0
                    })
                    .fold(0.0, f32::max);
                column.width.max(title.size().x + 16.0).max(cells)
            })
            .collect();
        let least: f32 = widths.iter().sum();
        let width = ui.available_width().max(least + ACTIONS_WIDTH + 16.0);
        let spare = (width - least - ACTIONS_WIDTH - 16.0) / columns.len().max(1) as f32;
        let edges = |rect: Rect| {
            let mut left = rect.left() + 8.0;
            widths
                .iter()
                .map(|width| {
                    let cell = Rect::from_min_max(
                        egui::pos2(left, rect.top()),
                        egui::pos2(left + width + spare - 12.0, rect.bottom()),
                    );
                    left += width + spare;
                    cell
                })
                .collect::<Vec<_>>()
        };

        let mut row_action = None;
        let mut at_end = false;
        egui::ScrollArea::horizontal()
            .id_salt("account_panel_columns")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                let (header, _) = ui.allocate_exact_size(vec2(width, 32.0), Sense::hover());
                let chooser = Rect::from_min_size(
                    egui::pos2(
                        header.right().min(ui.clip_rect().right()) - 36.0,
                        header.top() + 4.0,
                    ),
                    vec2(24.0, 24.0),
                );
                let mut header_clip = ui.clip_rect();
                header_clip.max.x = chooser.left() - 4.0;
                for ((_, column), cell) in columns.iter().zip(edges(header)) {
                    let (x, align) = if column.right {
                        (cell.right(), Align2::RIGHT_CENTER)
                    } else {
                        (cell.left(), Align2::LEFT_CENTER)
                    };
                    ui.painter()
                        .with_clip_rect(cell.intersect(header_clip))
                        .text(
                            egui::pos2(x, cell.center().y),
                            align,
                            l10n::text(ui.ctx(), column.title),
                            heading.clone(),
                            theme.ui.text_muted,
                        );
                }
                let chooser = ui.put(chooser, |ui: &mut Ui| {
                    IconButton::new(&icons::COLUMNS)
                        .size(ButtonSize::SM)
                        .tooltip("Columns and export")
                        .show(ui)
                });
                egui::Popup::menu(&chooser)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .show(|ui| {
                        for column in tab.columns() {
                            let key = (tab, column.title);
                            let mut shown = !self.account_panel.hidden.contains(&key);
                            if ui
                                .checkbox(&mut shown, l10n::text(ui.ctx(), column.title))
                                .changed()
                            {
                                if shown {
                                    self.account_panel.hidden.remove(&key);
                                } else {
                                    self.account_panel.hidden.insert(key);
                                }
                            }
                        }
                        ui.separator();
                        if ui.button(l10n::text(ui.ctx(), "Export as CSV")).clicked() {
                            action = Some(AccountPanelAction::Export {
                                name: tab.title().to_owned(),
                                csv: csv(&columns, &rows),
                            });
                            ui.close();
                        }
                    });

                if rows.is_empty() {
                    let mut empty =
                        ui.new_child(egui::UiBuilder::new().max_rect(Rect::from_min_size(
                            egui::pos2(ui.clip_rect().left(), ui.cursor().top()),
                            vec2(ui.clip_rect().width(), ui.available_height()),
                        )));
                    EmptyState::new(match tab {
                        Tab::Positions => "No open positions",
                        Tab::Orders => "No working orders",
                        Tab::History => "No finished orders",
                        Tab::Balance => "No balance changes",
                        Tab::Journal => "No journal entries",
                        Tab::Executions => "No executions",
                    })
                    .fill_height()
                    .show(&mut empty);
                    at_end = true;
                    return;
                }
                egui::ScrollArea::vertical()
                    .id_salt(("account_panel_rows", tab))
                    .auto_shrink([false, false])
                    .show_rows(ui, ROW_HEIGHT, rows.len(), |ui, range| {
                        at_end |= range.end == rows.len();
                        for row in &rows[range] {
                            let (rect, _) =
                                ui.allocate_exact_size(vec2(width, ROW_HEIGHT), Sense::hover());
                            let hovered = ui.rect_contains_pointer(rect);
                            if hovered {
                                ui.painter().rect_filled(rect, 0.0, theme.ui.btn_bg_hover);
                            }
                            ui.painter().hline(
                                rect.x_range(),
                                rect.bottom() - 0.5,
                                Stroke::new(1.0, theme.ui.border),
                            );
                            let has_actions = row.edit.is_some() || row.close.is_some();
                            let visible = rect.right().min(ui.clip_rect().right());
                            // Cells stop short of the actions, which stay in view at the end of a
                            // row wider than a narrow panel.
                            let mut cells_clip = ui.clip_rect();
                            if has_actions {
                                cells_clip.max.x = visible - ACTIONS_WIDTH;
                            }
                            let cells = ui.painter().with_clip_rect(cells_clip);
                            for (&(index, column), cell_rect) in columns.iter().zip(edges(rect)) {
                                let cell = &row.cells[index];
                                if cell.symbol && !cell.text.is_empty() {
                                    let galley = ui.painter().layout_job(semibold(
                                        &cell.text,
                                        theme.text.small,
                                        theme.ui.text,
                                    ));
                                    let chip = Rect::from_min_size(
                                        egui::pos2(cell_rect.left(), rect.center().y - 15.0),
                                        vec2((galley.size().x + 16.0).min(cell_rect.width()), 30.0),
                                    );
                                    let current = cell.text == chart_symbol;
                                    let (fill, text) = if current {
                                        (theme.ui.accent, theme.ui.accent_text)
                                    } else {
                                        (theme.ui.panel_bg_secondary, theme.ui.text)
                                    };
                                    let response = ui
                                        .interact(
                                            chip,
                                            ui.scope_id().with((
                                                "chip",
                                                &cell.text,
                                                rect.top() as i32,
                                            )),
                                            Sense::click(),
                                        )
                                        .on_hover_cursor(egui::CursorIcon::PointingHand);
                                    cells.rect_filled(chip, 4.0, fill);
                                    let galley = ui.painter().layout_job(semibold(
                                        &cell.text,
                                        theme.text.small,
                                        text,
                                    ));
                                    cells.with_clip_rect(chip.shrink(4.0)).galley(
                                        chip.left_center() + vec2(8.0, -galley.size().y / 2.0),
                                        galley,
                                        text,
                                    );
                                    if response.clicked() {
                                        action = row
                                            .symbol
                                            .clone()
                                            .map(AccountPanelAction::SelectSymbol);
                                    }
                                    continue;
                                }
                                let painter = cells.with_clip_rect(cell_rect);
                                let y = rect.center().y;
                                let mut right = cell_rect.right();
                                if !cell.unit.is_empty() && !cell.text.is_empty() {
                                    let unit = painter.text(
                                        egui::pos2(right, y + 1.0),
                                        Align2::RIGHT_CENTER,
                                        l10n::text(ui.ctx(), &cell.unit),
                                        unit_font.clone(),
                                        theme.ui.text_muted,
                                    );
                                    right = unit.left() - 3.0;
                                }
                                let (x, align) = if column.right {
                                    (right, Align2::RIGHT_CENTER)
                                } else {
                                    (cell_rect.left(), Align2::LEFT_CENTER)
                                };
                                painter.text(
                                    egui::pos2(x, y),
                                    align,
                                    l10n::text(ui.ctx(), &cell.text),
                                    font.clone(),
                                    cell.color,
                                );
                            }
                            if has_actions {
                                let buttons = Rect::from_min_max(
                                    egui::pos2(visible - ACTIONS_WIDTH, rect.top()),
                                    egui::pos2(visible - 12.0, rect.bottom()),
                                );
                                let mut child = ui.new_child(
                                    egui::UiBuilder::new()
                                        .max_rect(buttons)
                                        .layout(Layout::right_to_left(Align::Center)),
                                );
                                child.visuals_mut().widgets.inactive.fg_stroke.color =
                                    theme.ui.text_muted;
                                let (close, edit) = match row.close.as_ref().or(row.edit.as_ref()) {
                                    Some(RowAction::Position(_)) => ("Close position", ""),
                                    _ => ("Cancel order", "Modify order"),
                                };
                                if row.close.is_some()
                                    && IconButton::new(&icons::CLOSE)
                                        .size(ButtonSize::SM)
                                        .tooltip(close)
                                        .show(&mut child)
                                        .clicked()
                                {
                                    row_action = row.close.as_ref().map(|a| (a, false));
                                }
                                if row.edit.is_some()
                                    && IconButton::new(&icons::PENCIL)
                                        .size(ButtonSize::SM)
                                        .tooltip(edit)
                                        .show(&mut child)
                                        .clicked()
                                {
                                    row_action = row.edit.as_ref().map(|a| (a, true));
                                }
                            }
                        }
                    });
            });

        // A history shown to its end asks the broker for the page before it.
        if at_end && let Some(kind) = tab.history() {
            self.load_older(kind);
        }
        match row_action {
            Some((RowAction::Position(position), _)) => {
                self.pending = Some(TradingLineAction::Close(position.clone()));
            }
            Some((RowAction::Order(order), false)) => {
                self.pending = Some(TradingLineAction::Cancel(order.clone()));
            }
            Some((RowAction::Order(order), true)) => {
                self.ticket.open = false;
                self.load_rules(&order.request.symbol);
                self.account_panel.editing = Some((order.clone(), order.request.clone()));
            }
            None => {}
        }
        self.show_order_editor(ui.ctx(), chart_symbol);
        action
    }

    /// The edit dialog of a working order: its quantity, prices and brackets. Saving sends the
    /// change to the broker; the dialog is the user's confirmation.
    fn show_order_editor(&mut self, ctx: &egui::Context, chart_symbol: &str) {
        let Some(symbol) =
            (self.account_panel.editing.as_ref()).map(|(order, _)| order.request.symbol.clone())
        else {
            return;
        };
        let decimals = self.numbers(&symbol, chart_symbol).decimals;
        let brackets = self.capabilities().brackets;
        let Some((order, request)) = &mut self.account_panel.editing else {
            return;
        };
        let mut open = true;
        let mut save = false;
        egui::Window::new(l10n::format_text(
            ctx,
            "Modify order {0}",
            &[("0", order.id.to_string())],
        ))
        .id(egui::Id::unique("account_panel_modify"))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            egui::Grid::new("account_panel_modify_grid")
                .num_columns(2)
                .spacing([12.0, 8.0])
                .show(ui, |ui| {
                    let price = |ui: &mut Ui, label: &str, value: &mut f64| {
                        ui.label(l10n::text(ui.ctx(), label));
                        ui.add(
                            egui::DragValue::new(value)
                                .range(0.0..=f64::MAX)
                                .max_decimals(decimals)
                                .speed(10f64.powi(-(decimals as i32))),
                        );
                        ui.end_row();
                    };
                    ui.label(l10n::text(ui.ctx(), "Quantity"));
                    // The levels' quantities add up to it; they are changed on the chart.
                    ui.add_enabled(
                        request.take_profit_levels.is_empty(),
                        egui::DragValue::new(&mut request.quantity).range(0.0..=f64::MAX),
                    );
                    ui.end_row();
                    match &mut request.order_type {
                        OrderType::Limit(limit) => price(ui, "Limit price", limit),
                        OrderType::Stop(stop) => price(ui, "Stop price", stop),
                        OrderType::StopLimit { stop, limit } => {
                            price(ui, "Stop price", stop);
                            price(ui, "Limit price", limit);
                        }
                        _ => {}
                    }
                    if brackets {
                        let reference = match request.order_type {
                            OrderType::Limit(p) | OrderType::Stop(p) => p,
                            OrderType::StopLimit { limit, .. } => limit,
                            _ => 0.0,
                        };
                        for (index, level) in request.take_profit_levels.iter_mut().enumerate() {
                            let label = l10n::format_text(
                                ui.ctx(),
                                "Take profit {0}",
                                &[("0", (index + 1).to_string())],
                            );
                            price(ui, &label, &mut level.price);
                        }
                        let single = request
                            .take_profit_levels
                            .is_empty()
                            .then_some(("Take profit", &mut request.take_profit));
                        for (label, bracket) in single
                            .into_iter()
                            .chain([("Stop loss", &mut request.stop_loss)])
                        {
                            let mut on = bracket.is_some();
                            ui.checkbox(&mut on, label);
                            if on {
                                let value = bracket.get_or_insert(reference);
                                ui.add(
                                    egui::DragValue::new(value)
                                        .range(0.0..=f64::MAX)
                                        .max_decimals(decimals)
                                        .speed(10f64.powi(-(decimals as i32))),
                                );
                            } else {
                                *bracket = None;
                            }
                            ui.end_row();
                        }
                    }
                });
            ui.add_space(8.0);
            let valid = self.rules_ready
                && self.rules.symbol == request.symbol
                && crate::ui::trading::ticket::order_quantity(request.quantity, &self.rules)
                    .is_ok_and(|quantity| quantity == request.quantity)
                && request != &order.request;
            save = ui
                .add_enabled(
                    valid,
                    egui::Button::new(l10n::text(ui.ctx(), "Modify order")),
                )
                .clicked();
        });
        if save {
            let (order, request) = self.account_panel.editing.take().expect("editing");
            self.execute(Some(TradingLineAction::Modify(order, request)));
        } else if !open {
            self.account_panel.editing = None;
        }
    }
}
