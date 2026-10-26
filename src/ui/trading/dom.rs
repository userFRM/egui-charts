//! The depth ladder of the order ticket's DOM tab: the size resting at each price around the last
//! trade, the user's working orders at their prices, and orders placed, moved and cancelled from
//! the ladder.

use crate::localization as l10n;
use std::collections::{BTreeMap, HashMap, HashSet};

use egui::{Align2, Color32, Rect, Sense, Stroke, Ui, pos2, vec2};

use super::account_panel::{HISTORY_LIMIT, grouped};
use super::ticket::{
    Action, Market, chevron, field, muted, number, order_quantity, side_color, side_fill,
    side_name, text, time_in_force_menu,
};
use crate::chart::{TradingLineAction, decimals, to_step};
use crate::icons::icons;
use crate::model::{
    Execution, Order, OrderId, OrderRequest, OrderSide, OrderStatus, OrderType, Tick, TimeInForce,
};
use crate::theme::ThemeExt;

/// Height of a price row.
const ROW: f32 = 22.0;

/// Where the ladder stands between frames.
#[derive(Default)]
pub(super) struct Ladder {
    /// The tick of the middle row, until the ladder has a price to centre on.
    centre: Option<i64>,
    /// Keeps the ladder where the user scrolled it instead of re-centring on the market.
    locked: bool,
    /// Scrolled distance short of a whole row.
    scroll: f32,
    /// The Details section is open.
    details: bool,
    /// The previous observed sizes, and the last change at each level.
    sizes: [BTreeMap<i64, f64>; 2],
    changes: [BTreeMap<i64, f64>; 2],
    observed: bool,
    /// The order chosen from a price with more than one working order on the same side.
    selected: Option<OrderId>,
}

/// The account's orders and executions, shared by the ladder, chart and account history.
#[derive(Default)]
pub(super) struct Fills {
    pub(super) orders: HashMap<OrderId, OrderRequest>,
    pub(super) executions: Vec<Execution>,
    working: HashSet<OrderId>,
    filled: HashMap<OrderId, f64>,
    /// Latest fill for each symbol, independent of execution-history eviction.
    latest: HashMap<String, Execution>,
}

impl Fills {
    pub(super) fn order(&mut self, order: &Order) {
        self.orders.insert(order.id.clone(), order.request.clone());
        if order.status == OrderStatus::Working {
            self.working.insert(order.id.clone());
        } else {
            self.working.remove(&order.id);
        }
        if let Some(fill) = self
            .executions
            .iter()
            .filter(|fill| fill.order == order.id)
            .max_by_key(|fill| fill.time)
            .cloned()
        {
            self.remember(&order.request.symbol, &fill);
        }
    }

    pub(super) fn execution(&mut self, fill: &Execution) {
        *self.filled.entry(fill.order.clone()).or_default() += fill.quantity;
        if let Some(order) = self.orders.get(&fill.order) {
            let symbol = order.symbol.clone();
            self.remember(&symbol, fill);
        }
        self.executions.push(fill.clone());
        if self.executions.len() > HISTORY_LIMIT {
            let old = self.executions.remove(0);
            self.forget(&old.order);
        }
    }

    pub(super) fn forget(&mut self, id: &OrderId) {
        if !self.working.contains(id) && !self.executions.iter().any(|fill| &fill.order == id) {
            self.orders.remove(id);
            self.filled.remove(id);
        }
    }

    fn remember(&mut self, symbol: &str, fill: &Execution) {
        if self
            .latest
            .get(symbol)
            .is_none_or(|old| old.time <= fill.time)
        {
            self.latest.insert(symbol.to_owned(), fill.clone());
        }
    }

    pub(super) fn price(&self, symbol: &str) -> Option<f64> {
        self.latest.get(symbol).map(|fill| fill.price)
    }

    pub(super) fn filled_quantity(&self, id: &OrderId) -> f64 {
        self.filled.get(id).copied().unwrap_or_default()
    }
}

/// Sell aggressors trade at the bid, buys at the ask. Unclassified prints contribute only to
/// total volume. The profile covers the trades retained by the chart, not inferred bar volume.
fn traded(trades: &[&Tick], tick: f64, top: i64, rows: usize) -> Vec<[f64; 3]> {
    let mut volumes = vec![[0.0; 3]; rows];
    for trade in trades {
        let level = (trade.price / tick).round() as i64;
        if let Some(row) = top
            .checked_sub(level)
            .and_then(|row| usize::try_from(row).ok())
            && let Some(volume) = volumes.get_mut(row)
        {
            let side = match trade.side {
                Some(OrderSide::Sell) => 0,
                Some(OrderSide::Buy) => 1,
                None => 2,
            };
            volume[side] += trade.size;
        }
    }
    volumes
}

/// The column edges of the ladder: the margin of the buy orders, the bid sizes, the prices, the
/// ask sizes and the margin of the sell orders.
fn columns(rect: Rect) -> [Rect; 5] {
    let mut left = rect.left();
    [0.15, 0.21, 0.28, 0.21, 0.15].map(|share| {
        let right = left + rect.width() * share;
        let column = Rect::from_x_y_ranges(left..=right, rect.y_range());
        left = right;
        column
    })
}

/// The price an order waits for: its limit price, its stop price, or the stop of a stop-limit
/// order.
fn entry_price(order_type: &OrderType) -> Option<f64> {
    match *order_type {
        OrderType::Limit(price)
        | OrderType::Stop(price)
        | OrderType::StopLimit { stop: price, .. } => Some(price),
        _ => None,
    }
}

/// The tag of a working order in the ladder's margin.
fn tag(order_type: &OrderType) -> &'static str {
    match order_type {
        OrderType::Limit(_) => "LIMIT",
        OrderType::Stop(_) => "STOP",
        OrderType::StopLimit { .. } => "STP LMT",
        _ => "MKT",
    }
}

/// A size without trailing zeros, such as "0.2703" or "1,250".
fn format_size(value: f64) -> String {
    let text = grouped(value, 4, false);
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
}

impl Ladder {
    /// Draws the ladder and its footer into `ui`. Returns an order to place, orders to cancel, or
    /// a change for the user to confirm.
    pub(super) fn show(
        &mut self,
        ui: &mut Ui,
        symbol: &str,
        market: &Market<'_>,
        quantity: &mut f64,
        time_in_force: &mut TimeInForce,
    ) -> Option<Action> {
        let theme = ui.active_theme();
        let tick = market.tick;
        let ticks = |price: f64| (price / tick).round() as i64;
        let price = |tick_index: i64| to_step(tick_index as f64 * tick, tick, f64::round);
        let buy = side_color(&theme, OrderSide::Buy);
        let sell = side_color(&theme, OrderSide::Sell);
        let orders: Vec<&Order> = (market.orders.iter())
            .filter(|order| order.request.symbol == symbol)
            .collect();
        let position = (market.positions.iter()).find(|position| position.symbol == symbol);
        let quantity_decimals = decimals(market.rules.quantity_step);
        let wanted = *quantity;
        let order = |side, order_type| -> Result<OrderRequest, String> {
            if !market.rules_ready {
                return Err("Waiting for the symbol's trading rules".into());
            }
            if !market.capabilities.accepts(&order_type) {
                return Err("The account does not accept this order type".into());
            }
            if entry_price(&order_type).is_some_and(|price| !price.is_finite() || price <= 0.0) {
                return Err("Enter a positive price".into());
            }
            let quantity = order_quantity(wanted, market.rules)?;
            let mut request = OrderRequest::new(symbol, side, quantity, order_type);
            request.time_in_force = *time_in_force;
            Ok(request)
        };
        let mut action = None;

        // The header over the size columns.
        let (header, _) = ui.allocate_exact_size(vec2(ui.available_width(), 18.0), Sense::hover());
        let [_, bid_column, _, ask_column, _] = columns(header);
        for (column, label) in [(bid_column, "Bid size"), (ask_column, "Ask size")] {
            let at = column.center();
            text(
                ui,
                at,
                Align2::CENTER_CENTER,
                label,
                12.0,
                theme.ui.text_muted,
            );
        }

        let footer = 28.0 + 32.0 + 32.0 + 36.0 + 24.0 + if self.details { 64.0 } else { 0.0 };
        let spacing = ui.spacing().item_spacing.y;
        let height = ui.available_height() - footer - 6.0 * spacing;
        let rows = ((height / ROW).floor() as i64).max(3);
        let (rect, response) = ui.allocate_exact_size(
            vec2(ui.available_width(), rows as f32 * ROW),
            Sense::click(),
        );

        // Scroll by whole rows; without the lock the market stays in view.
        // Keep the price under a press or drag stable through its release frame.
        let interacting =
            ui.input(|input| input.pointer.any_down() || input.pointer.any_released());
        if response.hovered() && !interacting {
            self.scroll += ui.input(|input| input.smooth_scroll_delta.y);
            let moved = (self.scroll / ROW).trunc();
            self.scroll -= moved * ROW;
            if let Some(centre) = &mut self.centre {
                *centre = centre.saturating_add(moved as i64).max(0);
            }
        }
        let last = market.last.filter(|last| last.is_finite()).map(ticks);
        let middle = market
            .bid
            .zip(market.ask)
            .map(|(bid, ask)| ticks(bid / 2.0 + ask / 2.0))
            .or(last);
        let top = |centre: i64| centre.saturating_add(rows / 2);
        let visible = |centre: i64, tick_index: i64| {
            (top(centre).saturating_sub(rows - 1)..=top(centre)).contains(&tick_index)
        };
        match (self.centre, middle) {
            (None, Some(middle)) => self.centre = Some(middle),
            (Some(centre), Some(middle))
                if !self.locked && !interacting && !visible(centre, middle) =>
            {
                self.centre = Some(middle);
            }
            _ => {}
        }
        let Some(centre) = self.centre else {
            text(
                ui,
                rect.center(),
                Align2::CENTER_CENTER,
                "No price yet",
                13.0,
                theme.ui.text_muted,
            );
            return None;
        };
        let top = top(centre);
        let row_of =
            |tick_index: i64| (visible(centre, tick_index)).then(|| (top - tick_index) as usize);
        let row_rect = |row: usize, column: Rect| {
            Rect::from_x_y_ranges(
                column.x_range(),
                rect.top() + row as f32 * ROW..=rect.top() + (row + 1) as f32 * ROW,
            )
        };

        let volumes = traded(market.trades, tick, top, rows as usize);
        let max_volume = volumes
            .iter()
            .map(|v| v.iter().sum::<f64>())
            .fold(0.0, f64::max);
        // A missing snapshot is a feed gap, not a cancellation of every resting order.
        if market.book.is_none() {
            self.sizes = Default::default();
            self.changes = Default::default();
            self.observed = false;
        }
        // Size changes are between observed book states; unchanged frames retain the last change.
        for side in 0..2 {
            let current: BTreeMap<_, _> = market
                .book
                .into_iter()
                .flat_map(|book| if side == 0 { &book.bids } else { &book.asks })
                .map(|level| (ticks(level.price), level.size))
                .collect();
            for (&level, &size) in &current {
                let old = self.sizes[side].get(&level).copied().unwrap_or_default();
                if size != old && self.observed {
                    self.changes[side].insert(level, size - old);
                }
            }
            for (&level, &old) in &self.sizes[side] {
                if !current.contains_key(&level) {
                    self.changes[side].insert(level, -old);
                }
            }
            self.changes[side]
                .retain(|level, _| current.contains_key(level) || row_of(*level).is_some());
            self.sizes[side] = current;
        }
        self.observed = market.book.is_some();
        // Size and order count per visible row and side.
        let mut sizes = vec![[(0.0, None::<u32>); 2]; rows as usize];
        if let Some(book) = market.book {
            for (side, levels) in [(0, &book.bids), (1, &book.asks)] {
                for level in levels {
                    if let Some(row) = row_of(ticks(level.price)) {
                        let (size, count) = &mut sizes[row][side];
                        *size += level.size;
                        *count = match (*count, level.order_cnt) {
                            (Some(old), Some(new)) => Some(old.saturating_add(new)),
                            (None, value) => value,
                            (old, None) => old,
                        };
                    }
                }
            }
        }

        let [
            buy_margin,
            bid_column,
            price_column,
            ask_column,
            sell_margin,
        ] = columns(rect);
        let painter = ui.painter();
        let grid = Stroke::new(1.0, theme.ui.border);
        for (row, [bid, ask]) in sizes.iter().enumerate() {
            let tick_index = top - row as i64;
            for ((size, _), column, color) in [(bid, bid_column, buy), (ask, ask_column, sell)] {
                if *size > 0.0 {
                    let cell = row_rect(row, column);
                    painter.rect_filled(cell, 0.0, color.gamma_multiply(0.3));
                    let at = cell.center();
                    text(
                        ui,
                        at,
                        Align2::CENTER_CENTER,
                        &format_size(*size),
                        12.0,
                        theme.ui.text,
                    );
                }
            }
            let cell = row_rect(row, price_column);
            let volume: f64 = volumes[row].iter().sum();
            if max_volume > 0.0 && volume > 0.0 {
                let profile = Rect::from_min_size(
                    cell.min,
                    vec2(cell.width() * (volume / max_volume) as f32, cell.height()),
                );
                painter.rect_filled(profile, 0.0, theme.ui.accent.gamma_multiply(0.16));
            }
            let color = match last {
                Some(last) if tick_index > last => sell,
                Some(last) if tick_index < last => buy,
                Some(_) => {
                    // The price of the last trade.
                    painter.rect_filled(cell, 0.0, theme.ui.btn_bg_active);
                    theme.ui.text
                }
                None => theme.ui.text,
            };
            let label = grouped(price(tick_index), market.decimals, false);
            text(
                ui,
                cell.center(),
                Align2::CENTER_CENTER,
                &label,
                12.0,
                color,
            );
            if market
                .last_fill
                .is_some_and(|fill| ticks(fill) == tick_index)
            {
                painter.rect_stroke(
                    cell.shrink(1.0),
                    0.0,
                    Stroke::new(2.0, theme.ui.accent),
                    egui::StrokeKind::Inside,
                );
            }
        }
        let ladder = Rect::from_x_y_ranges(bid_column.left()..=ask_column.right(), rect.y_range());
        for row in 0..=rows {
            painter.hline(ladder.x_range(), rect.top() + row as f32 * ROW, grid);
        }
        for x in [
            bid_column.left(),
            price_column.left(),
            price_column.right(),
            ask_column.right(),
        ] {
            painter.vline(x, rect.y_range(), grid);
        }

        if let Some(pos) = response.hover_pos() {
            let row = ((pos.y - rect.top()) / ROW).floor() as usize;
            if let Some([bid, ask]) = sizes.get(row) {
                let counts =
                    |count: Option<u32>| count.map_or("unknown".to_owned(), |n| n.to_string());
                let delta = |side: usize| {
                    self.changes[side]
                        .get(&(top - row as i64))
                        .map_or("unknown".to_owned(), |n| format!("{n:+}"))
                };
                let [bid_traded, ask_traded, unknown] = volumes[row];
                response.clone().on_hover_text(l10n::format_text(ui.ctx(), "Bid orders: {0} · Ask orders: {1}\nSize change: bid {2}, ask {3}\nTraded at bid: {4} · at ask: {5}\nUnclassified: {6} · Total: {7}\nProfile: retained feed trades{8}", &[("0", counts(bid.1).to_string()), ("1", counts(ask.1).to_string()), ("2", delta(0).to_string()), ("3", delta(1).to_string()), ("4", format_size(bid_traded).to_string()), ("5", format_size(ask_traded).to_string()), ("6", format_size(unknown).to_string()), ("7", format_size(bid_traded + ask_traded + unknown).to_string()), ("8", (if market.last_fill.is_some_and(|fill| ticks(fill) == top - row as i64) {
                        "\nLast account fill"
                    } else { "" }).to_string())]));
            }
        }

        // Clicking a size cell places a limit order on the passive side of the market and a stop
        // order beyond it.
        let pointer_row = |pos: egui::Pos2| {
            ((pos.y - rect.top()) / ROW)
                .floor()
                .clamp(0.0, (rows - 1) as f32) as usize
        };
        let cell_order = |pos: egui::Pos2| {
            let (side, column) = if bid_column.contains(pos) {
                (OrderSide::Buy, bid_column)
            } else if ask_column.contains(pos) {
                (OrderSide::Sell, ask_column)
            } else {
                return None;
            };
            let row = pointer_row(pos);
            let at = price(top - row as i64);
            let touch = match side {
                OrderSide::Buy => market.ask,
                OrderSide::Sell => market.bid,
            }
            .or(market.last);
            let limit = touch.is_none_or(|touch| match side {
                OrderSide::Buy => at <= touch,
                OrderSide::Sell => at >= touch,
            });
            let order_type = if limit {
                market.capabilities.limit.then_some(OrderType::Limit(at))
            } else {
                market.capabilities.stop.then_some(OrderType::Stop(at))
            }?;
            Some((side, order_type, row_rect(row, column)))
        };
        if let Some(pos) = response.hover_pos()
            && let Some((side, order_type, cell)) = cell_order(pos)
        {
            match order(side, order_type) {
                Ok(request) => {
                    let color = side_color(&theme, side);
                    let outline = cell.shrink(1.0);
                    let corners = [
                        outline.left_top(),
                        outline.right_top(),
                        outline.right_bottom(),
                        outline.left_bottom(),
                        outline.left_top(),
                    ];
                    painter.extend(egui::Shape::dashed_line(
                        &corners,
                        Stroke::new(1.0, color),
                        3.0,
                        2.0,
                    ));
                    let margin = if side == OrderSide::Buy {
                        buy_margin
                    } else {
                        sell_margin
                    };
                    let at = row_rect(pointer_row(pos), margin).center();
                    text(
                        ui,
                        at,
                        Align2::CENTER_CENTER,
                        tag(&request.order_type),
                        10.0,
                        color,
                    );
                    if response.clicked() {
                        action = Some(Action::Place(request));
                    }
                }
                Err(reason) => {
                    response.clone().on_hover_text(l10n::text(ui.ctx(), reason));
                }
            }
        }

        // The working orders in the margin of their side: drag one to move it, right-click it to
        // cancel it.
        let pointer = ui.input(|input| input.pointer.interact_pos());
        let mut groups = BTreeMap::<_, Vec<_>>::new();
        for working in &orders {
            if let Some(at) = entry_price(&working.request.order_type) {
                groups
                    .entry((ticks(at), working.request.side == OrderSide::Buy))
                    .or_default()
                    .push(*working);
            }
        }
        for group in groups.values() {
            let working = group
                .iter()
                .find(|order| self.selected.as_ref() == Some(&order.id))
                .copied()
                .unwrap_or(group[0]);
            let at = entry_price(&working.request.order_type).expect("a priced order");
            let side = working.request.side;
            let margin = if side == OrderSide::Buy {
                buy_margin
            } else {
                sell_margin
            };
            let id = ui.make_persistent_id(("dom_order", &working.id.0));
            let dragging = ui.ctx().is_being_dragged(id);
            let row = match pointer.filter(|_| dragging) {
                Some(pos) => Some(pointer_row(pos)),
                None => row_of(ticks(at)),
            };
            let Some(row) = row else {
                continue;
            };
            let cell = row_rect(row, margin).shrink(2.0);
            let tag_response = ui.interact(cell, id, Sense::click_and_drag());
            let color = side_color(&theme, side);
            let painter = ui.painter();
            painter.rect_filled(cell, 2.0, color.gamma_multiply(0.25));
            painter.rect_stroke(cell, 2.0, Stroke::new(1.0, color), egui::StrokeKind::Inside);
            let label = tag(&working.request.order_type);
            let shown = if group.len() > 1 {
                format!("{label} +{}", group.len() - 1)
            } else {
                label.to_owned()
            };
            text(
                ui,
                cell.center(),
                Align2::CENTER_CENTER,
                &shown,
                10.0,
                color,
            );
            if group.len() > 1 {
                egui::Popup::menu(&tag_response).show(|ui| {
                    ui.label(l10n::text(ui.ctx(), "Choose an order to drag or cancel"));
                    for order in group {
                        let label = format!(
                            "{} {} · {}",
                            format_size(order.request.quantity - order.filled),
                            tag(&order.request.order_type),
                            order.id.0,
                        );
                        if ui
                            .selectable_label(order.id == working.id, l10n::text(ui.ctx(), label))
                            .clicked()
                        {
                            self.selected = Some(order.id.clone());
                        }
                    }
                });
            }
            let tag_response = tag_response.on_hover_text(l10n::format_text(ui.ctx(), "{0} {1} @ {2} {label} · {3}. Drag to move, right-click to cancel. {4} order(s) here; click to choose", &[("0", side.label().to_string()), ("1", grouped(working.request.quantity - working.filled, quantity_decimals, false).to_string()), ("2", grouped(at, market.decimals, false).to_string()), ("label", label.to_string()), ("3", working.id.0.to_string()), ("4", format!("{}", group.len()))]));
            if tag_response.secondary_clicked() {
                action = Some(Action::Confirm(TradingLineAction::Cancel(working.clone())));
            }
            if tag_response.drag_stopped()
                && market.capabilities.modify
                && let Some(pos) = pointer
            {
                let moved = price(top - pointer_row(pos) as i64);
                if moved > 0.0 && moved != at {
                    let shift = |other: f64| to_step(other + moved - at, tick, f64::round);
                    let mut changed = working.request.clone();
                    changed.order_type = match changed.order_type {
                        OrderType::Limit(_) => OrderType::Limit(moved),
                        OrderType::Stop(_) => OrderType::Stop(moved),
                        OrderType::StopLimit { limit, .. } => OrderType::StopLimit {
                            stop: moved,
                            limit: shift(limit),
                        },
                        other => other,
                    };
                    changed.take_profit = changed.take_profit.map(shift);
                    changed.stop_loss = changed.stop_loss.map(shift);
                    let valid = |price: f64| price.is_finite() && price > 0.0;
                    if changed.take_profit.is_none_or(valid)
                        && changed.stop_loss.is_none_or(valid)
                        && match changed.order_type {
                            OrderType::StopLimit { limit, .. } => valid(limit),
                            _ => true,
                        }
                    {
                        let modify = TradingLineAction::Modify(working.clone(), changed);
                        action = Some(Action::Confirm(modify));
                    }
                }
            }
        }

        // The sizes in view, the lock and the buttons that cancel every buy or every sell.
        let (totals, _) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::hover());
        let [
            buy_margin,
            bid_column,
            price_column,
            ask_column,
            sell_margin,
        ] = columns(totals);
        for (side, margin, column, index) in [
            (OrderSide::Buy, buy_margin, bid_column, 0),
            (OrderSide::Sell, sell_margin, ask_column, 1),
        ] {
            let color = side_color(&theme, side);
            let total: f64 = sizes.iter().map(|sizes| sizes[index].0).sum();
            let cell = column.shrink2(vec2(2.0, 2.0));
            ui.painter()
                .rect_filled(cell, 2.0, color.gamma_multiply(0.3));
            text(
                ui,
                cell.center(),
                Align2::CENTER_CENTER,
                &format_size(total),
                12.0,
                theme.ui.text,
            );
            let ids: Vec<_> = (orders.iter())
                .filter(|order| order.request.side == side)
                .map(|order| order.id.clone())
                .collect();
            let label = format!("Cancel all {}s", side_name(side).to_lowercase());
            let response = ui
                .interact(
                    margin,
                    ui.make_persistent_id(("dom_cancel", index)),
                    Sense::click(),
                )
                .on_hover_text(l10n::text(ui.ctx(), &label));
            let translated_label = l10n::text(ui.ctx(), &label);
            response.widget_info(|| {
                egui::WidgetInfo::labeled(egui::Role::Button, !ids.is_empty(), &translated_label)
            });
            let ink = if response.hovered() && !ids.is_empty() {
                theme.ui.text
            } else {
                theme.ui.text_muted
            };
            let (cross, count) = if index == 0 {
                (
                    margin.left_center() + vec2(8.0, 0.0),
                    margin.left_center() + vec2(20.0, 0.0),
                )
            } else {
                (
                    margin.right_center() - vec2(8.0, 0.0),
                    margin.right_center() - vec2(20.0, 0.0),
                )
            };
            let painter = ui.painter();
            let stroke = Stroke::new(1.3, ink);
            painter.line_segment([cross + vec2(-4.0, -4.0), cross + vec2(4.0, 4.0)], stroke);
            painter.line_segment([cross + vec2(-4.0, 4.0), cross + vec2(4.0, -4.0)], stroke);
            let align = if index == 0 {
                Align2::LEFT_CENTER
            } else {
                Align2::RIGHT_CENTER
            };
            text(ui, count, align, &ids.len().to_string(), 12.0, ink);
            if response.clicked() && !ids.is_empty() {
                action = Some(Action::Cancel(ids));
            }
        }
        let lock = Rect::from_center_size(price_column.center(), vec2(24.0, 24.0));
        let lock_response = ui
            .interact(lock, ui.make_persistent_id("dom_lock"), Sense::click())
            .on_hover_text(l10n::text(
                ui.ctx(),
                if self.locked {
                    "Re-centre the ladder on the market"
                } else {
                    "Keep the ladder from re-centring on the market"
                },
            ));
        let translated_label = l10n::text(ui.ctx(), "Lock the ladder");
        lock_response.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::Button, true, self.locked, &translated_label)
        });
        if lock_response.clicked() {
            self.locked = !self.locked;
            if !self.locked {
                self.centre = middle;
            }
        }
        let icon = if self.locked {
            &icons::LOCK
        } else {
            &icons::UNLOCK
        };
        let ink = if self.locked || lock_response.hovered() {
            theme.ui.text
        } else {
            theme.ui.text_muted
        };
        icon.as_image_tinted(vec2(16.0, 16.0), ink)
            .paint_at(ui, Rect::from_center_size(lock.center(), vec2(16.0, 16.0)));

        // The position and its profit or loss.
        ui.columns(2, |halves| {
            let (text_color, label) = match position {
                Some(position) => (
                    side_color(
                        &theme,
                        if position.quantity > 0.0 {
                            OrderSide::Buy
                        } else {
                            OrderSide::Sell
                        },
                    ),
                    format!(
                        "{} @ {}",
                        grouped(position.quantity, quantity_decimals, false),
                        grouped(position.average_price, market.decimals, false)
                    ),
                ),
                None => (theme.ui.text_muted, "Flat".to_owned()),
            };
            boxed(&mut halves[0], &label, text_color);
            let pnl = position
                .and_then(|position| {
                    position.unrealized_pnl.or_else(|| {
                        market.last.map(|last| {
                            (last - position.average_price)
                                * position.quantity
                                * market.rules.point_value
                        })
                    })
                })
                .unwrap_or_default();
            let color = match pnl {
                pnl if pnl > 0.0 => theme.ui.success,
                pnl if pnl < 0.0 => theme.ui.error,
                _ => theme.ui.text_muted,
            };
            let pnl = format!("{} {}", grouped(pnl, 2, false), market.currency);
            boxed(&mut halves[1], pnl.trim_end(), color);
        });

        // Flatten, cancel all and reverse.
        ui.columns(3, |thirds| {
            let close = market.capabilities.market && position.is_some();
            let outlined = |ui: &mut Ui, enabled: bool, label: &str| {
                let button = egui::Button::new(l10n::text(ui.ctx(), label))
                    .fill(Color32::TRANSPARENT)
                    .stroke(Stroke::new(1.0, theme.ui.border_control))
                    .corner_radius(6.0);
                let size = [ui.available_width(), 32.0];
                ui.add_enabled_ui(enabled, |ui| ui.add_sized(size, button))
                    .inner
                    .clicked()
            };
            if outlined(&mut thirds[0], close, "Flatten")
                && let Some(position) = position
            {
                action = Some(Action::Confirm(TradingLineAction::Close(position.clone())));
            }
            if outlined(&mut thirds[1], !orders.is_empty(), "CXL All") {
                action = Some(Action::Cancel(
                    orders.iter().map(|order| order.id.clone()).collect(),
                ));
            }
            if outlined(&mut thirds[2], close, "Reverse")
                && let Some(position) = position
            {
                action = Some(Action::Confirm(TradingLineAction::Reverse(
                    position.clone(),
                )));
            }
        });

        // Market orders of the quantity between them.
        ui.columns(3, |thirds| {
            for (index, side) in [(0, OrderSide::Buy), (2, OrderSide::Sell)] {
                let ui = &mut thirds[index];
                let label = format!("{} Mkt", side_name(side));
                let button = egui::Button::new(
                    egui::RichText::new(l10n::text(ui.ctx(), label)).color(Color32::WHITE),
                )
                .fill(side_fill(&theme, side))
                .corner_radius(6.0);
                let request = order(side, OrderType::Market);
                let enabled = market.capabilities.market && request.is_ok();
                let size = [ui.available_width(), 36.0];
                let response = ui
                    .add_enabled_ui(enabled, |ui| ui.add_sized(size, button))
                    .inner;
                let response = match &request {
                    Err(reason) => response.on_disabled_hover_text(l10n::text(ui.ctx(), reason)),
                    Ok(_) => response,
                };
                if response.clicked()
                    && let Ok(request) = request
                {
                    action = Some(Action::Place(request));
                }
            }
            field(&mut thirds[1], |ui| {
                number(ui, "dom_quantity", quantity, quantity_decimals);
            });
        });

        // The Details section with the time in force of the ladder's orders.
        let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
        let details = ui.interact(row, ui.make_persistent_id("dom_details"), Sense::click());
        let translated_label = l10n::text(ui.ctx(), "Details");
        details.widget_info(|| {
            egui::WidgetInfo::selected(egui::Role::Button, true, self.details, &translated_label)
        });
        if details.clicked() {
            self.details = !self.details;
        }
        let galley = ui.painter().layout_no_wrap(
            l10n::text(ui.ctx(), "DETAILS  "),
            egui::FontId::proportional(11.0),
            theme.ui.text_muted,
        );
        let width = galley.size().x;
        ui.painter().galley(
            row.left_center() - vec2(0.0, galley.size().y / 2.0),
            galley,
            theme.ui.text_muted,
        );
        let at = row.left_center() + vec2(width, 0.0);
        text(
            ui,
            at,
            Align2::LEFT_CENTER,
            time_in_force.label(),
            11.0,
            theme.ui.text_disabled,
        );
        chevron(
            ui,
            pos2(row.right() - 8.0, row.center().y),
            theme.ui.text,
            self.details,
        );
        if self.details {
            muted(ui, "Time in force");
            time_in_force_menu(ui, "dom_tif", time_in_force, market.rules);
        }
        action
    }
}

/// `label` centred in a bordered box the width of `ui`.
fn boxed(ui: &mut Ui, label: &str, color: Color32) {
    let theme = ui.active_theme();
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 32.0), Sense::hover());
    ui.painter().rect_stroke(
        rect,
        6.0,
        Stroke::new(1.0, theme.ui.border_control),
        egui::StrokeKind::Inside,
    );
    text(ui, rect.center(), Align2::CENTER_CENTER, label, 13.0, color);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Moving quotes must not move the price under a pressed cell or a dragged order.
    #[test]
    fn gestures_keep_their_price_while_the_book_moves() {
        use crate::model::{Capabilities, OrderBook, OrderBookLevel, OrderStatus, TradingRules};
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for width in [340.0, 240.0] {
                for (gesture, stacked) in [
                    ("place", false),
                    ("place-quote", false),
                    ("modify", false),
                    ("cancel", false),
                    ("modify", true),
                    ("cancel", true),
                ] {
                    let ctx = egui::Context::default();
                    crate::theme::apply_to_egui(&ctx, &theme);
                    let mut ladder = Ladder::default();
                    let rules = TradingRules::new("ES");
                    let mut request =
                        OrderRequest::new("ES", OrderSide::Buy, 2.0, OrderType::Limit(99.5));
                    request.take_profit = Some(102.0);
                    request.stop_loss = Some(98.0);
                    let mut working = vec![Order::new(
                        OrderId::new("bid"),
                        request.clone(),
                        OrderStatus::Working,
                    )];
                    if stacked {
                        let mut second = working[0].clone();
                        second.id = OrderId::new("second");
                        second.request.quantity = 3.0;
                        working.push(second);
                    }
                    let positions = [crate::model::Position::new("ES", 2.0, 99.0)];
                    let mut frame = |events, last: f64| {
                        let book = OrderBook {
                            ts: chrono::Utc::now(),
                            symbol: "ES".into(),
                            bids: vec![OrderBookLevel {
                                price: last - 0.25,
                                size: 3.0,
                                order_cnt: Some(2),
                            }],
                            asks: vec![OrderBookLevel {
                                price: last + 0.25,
                                size: 5.0,
                                order_cnt: Some(4),
                            }],
                        };
                        let market = Market {
                            last: Some(if gesture == "place-quote" {
                                100.0
                            } else {
                                last
                            }),
                            bid: Some(last - 0.25),
                            ask: Some(last + 0.25),
                            tick: 0.25,
                            decimals: 2,
                            currency: "USD",
                            balance: None,
                            capabilities: Capabilities {
                                market: true,
                                limit: true,
                                stop: true,
                                modify: true,
                                ..Default::default()
                            },
                            rules: &rules,
                            rules_ready: true,
                            book: Some(&book),
                            orders: &working,
                            positions: &positions,
                            trades: &[],
                            last_fill: None,
                        };
                        let mut action = None;
                        let output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(Rect::from_min_size(
                                    egui::Pos2::ZERO,
                                    vec2(width, 600.0),
                                )),
                                events,
                                ..Default::default()
                            },
                            |ui| {
                                action =
                                    ladder.show(ui, "ES", &market, &mut 1.0, &mut TimeInForce::Day)
                            },
                        );
                        let labels: BTreeMap<_, _> = output
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
                    let (_, labels) = frame(vec![], 100.0);
                    assert!(labels.contains_key("2.00 USD"), "mark an unpriced position");
                    let start = if gesture.starts_with("place") {
                        pos2(labels["Bid size"].x, labels["99.50"].y)
                    } else {
                        labels[if stacked { "LIMIT +1" } else { "LIMIT" }]
                    };
                    if stacked {
                        let click = |pos, pressed| egui::Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        };
                        frame(vec![egui::Event::PointerMoved(start)], 100.0);
                        frame(vec![click(start, true)], 100.0);
                        frame(vec![click(start, false)], 100.0);
                        let (_, menu) = frame(vec![], 100.0);
                        let choice = menu["3 LIMIT · second"];
                        frame(vec![egui::Event::PointerMoved(choice)], 100.0);
                        frame(vec![click(choice, true)], 100.0);
                        frame(vec![click(choice, false)], 100.0);
                        frame(vec![], 100.0);
                    }
                    let end = if gesture == "modify" {
                        pos2(start.x, labels["99.00"].y)
                    } else {
                        start
                    };
                    let button = if gesture == "cancel" {
                        egui::PointerButton::Secondary
                    } else {
                        egui::PointerButton::Primary
                    };
                    let click = |pos, pressed| egui::Event::PointerButton {
                        pos,
                        button,
                        pressed,
                        modifiers: Default::default(),
                    };
                    frame(vec![egui::Event::PointerMoved(start)], 100.0);
                    assert!(frame(vec![click(start, true)], 100.0).0.is_none());
                    assert!(
                        frame(vec![egui::Event::PointerMoved(end)], 110.0)
                            .0
                            .is_none()
                    );
                    let (action, labels) = frame(vec![click(end, false)], 110.0);
                    let pnl = if gesture == "place-quote" {
                        "2.00 USD"
                    } else {
                        "22.00 USD"
                    };
                    assert!(labels.contains_key(pnl), "mark the moving position");
                    let original = &working[usize::from(stacked)];
                    request.quantity = original.request.quantity;
                    let expected = match gesture {
                        "place" | "place-quote" => Action::Place(OrderRequest::new(
                            "ES",
                            OrderSide::Buy,
                            1.0,
                            OrderType::Limit(99.5),
                        )),
                        "modify" => {
                            request.order_type = OrderType::Limit(99.0);
                            request.take_profit = Some(101.5);
                            request.stop_loss = Some(97.5);
                            Action::Confirm(TradingLineAction::Modify(original.clone(), request))
                        }
                        _ => Action::Confirm(TradingLineAction::Cancel(original.clone())),
                    };
                    assert_eq!(action, Some(expected), "{gesture} at {width}");
                    let (_, labels) = frame(vec![], 110.0);
                    assert!(
                        labels.contains_key("110.00"),
                        "follow the book even without a trade"
                    );
                }
            }
        }
    }

    #[test]
    fn size_changes_distinguish_empty_sides_from_missing_snapshots() {
        use crate::model::{Capabilities, OrderBook, OrderBookLevel, TradingRules};
        let ctx = egui::Context::default();
        let mut ladder = Ladder::default();
        let rules = TradingRules::new("ES");
        for (size, expected) in [
            (Some(5.0), None),
            (Some(0.0), Some(-5.0)),
            (Some(3.0), Some(3.0)),
            (None, None),
            (Some(4.0), None),
            (Some(2.0), Some(-2.0)),
        ] {
            let book = size.map(|size| OrderBook {
                ts: chrono::Utc::now(),
                symbol: "ES".into(),
                bids: if size > 0.0 {
                    vec![OrderBookLevel {
                        price: 100.0,
                        size,
                        order_cnt: Some(1),
                    }]
                } else {
                    vec![]
                },
                asks: vec![],
            });
            let market = Market {
                last: Some(100.0),
                bid: None,
                ask: None,
                tick: 0.25,
                decimals: 2,
                currency: "USD",
                balance: None,
                capabilities: Capabilities::default(),
                rules: &rules,
                rules_ready: true,
                book: book.as_ref(),
                orders: &[],
                positions: &[],
                trades: &[],
                last_fill: None,
            };
            ctx.run_ui(egui::RawInput::default(), |ui| {
                ladder.show(ui, "ES", &market, &mut 1.0, &mut TimeInForce::Day);
            })
            .drop_without_applying_deltas();
            assert_eq!(
                ladder.changes[0].get(&400).copied(),
                expected,
                "size {size:?}"
            );
        }
    }

    #[test]
    fn profile_keeps_bid_ask_and_unknown_volume_at_their_ticks() {
        let now = chrono::Utc::now();
        let trades = [
            Tick::new(now, 100.0, 2.0, Some(OrderSide::Sell)),
            Tick::new(now, 100.0 + 1e-12, 3.0, Some(OrderSide::Sell)),
            Tick::new(now, 100.0, 7.0, Some(OrderSide::Buy)),
            Tick::new(now, 100.0, 11.0, None),
            Tick::new(now, 100.25, 13.0, Some(OrderSide::Buy)),
            Tick::new(now, 99.5, 17.0, Some(OrderSide::Sell)),
        ];
        // Rows descend by 0.25: 100.25, 100, 99.75. At 100 the bid has 2 + 3,
        // the ask 7 and unknown 11, so the profile is 23; 99.5 is outside the view.
        let refs: Vec<_> = trades.iter().collect();
        assert_eq!(
            traded(&refs, 0.25, 401, 3),
            vec![[0.0, 13.0, 0.0], [5.0, 7.0, 11.0], [0.0, 0.0, 0.0],]
        );
    }

    #[test]
    fn last_fill_belongs_to_its_order_symbol_and_newest_execution() {
        let mut fills = Fills::default();
        let order = Order::new(
            OrderId::new("a"),
            OrderRequest::new("ES", OrderSide::Buy, 2.0, OrderType::Market),
            crate::model::OrderStatus::Working,
        );
        fills.order(&order);
        let now = chrono::Utc::now();
        let mut fill = Execution::new(order.id.clone(), now, OrderSide::Buy, 100.0, 1.0);
        fills.execution(&fill);
        fill.time -= chrono::Duration::seconds(1);
        fill.price = 99.0;
        fills.execution(&fill);
        fill.order = OrderId::new("unknown");
        fills.execution(&fill);
        assert_eq!(fills.price("ES"), Some(100.0));
        assert_eq!(fills.price("NQ"), None);
        let mut delayed = order.clone();
        delayed.id = fill.order.clone();
        delayed.request.symbol = "NQ".into();
        fills.order(&delayed);
        assert_eq!(fills.price("NQ"), Some(99.0));
        assert_eq!(fills.price("ES"), Some(100.0));
    }
}
