use super::OrderFormatter;
use super::drawing::DrawingContextMenuAction;
use super::series::SeriesContextMenuAction;
use super::{
    ChartContextMenu, ContextMenuAction, MenuEntry, MenuItem, MenuRow, MenuRowId, POINTER_GAP,
    rows, show_menu,
};
use crate::chart::selection::{ChartElementId, SeriesId};
use crate::config::KeyboardAction;
use crate::icons::Icon;
use crate::model::{Capabilities, OrderSide, OrderType};
use crate::theme::ThemeExt;
use crate::ui::Control;
use crate::ui::app_state::set_control_visible;
use crate::ui::context_menu::drawing::DRAWING_MENU_OFFSET;
use crate::ui::context_menu::study::{self as study_context_menu, StudyContextMenu};
use crate::ui::context_menu::tpo::TpoMenu;
use crate::ui::context_menu::trade_profile::TradeProfileMenu;
use crate::ui::dialogs::{DrawingContextMenu, SeriesContextMenu};
use crate::ui::trading::TradingFormatter;
use crate::widget::{Chart, RightClickTarget};
use egui::{Context, Id};
use std::sync::Arc;

/// The chart menu a [`HostMenuContext`] belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChartMenuKind {
    /// The menu of the chart background.
    Chart,
    /// The menu of a bar series.
    Series,
    /// The menu of a study, opened from its plot or its legend row.
    Study,
    /// The menu of a drawing.
    Drawing,
    /// The menu of a price axis.
    PriceAxis {
        /// The pane of the axis.
        pane: crate::widget::PaneId,
        /// The axis within the pane, as [`Chart::pane_scales`] lists it.
        axis: crate::widget::PaneScale,
    },
    /// The menu of the time axis.
    TimeAxis,
}

/// The original click retained while a host menu action is being chosen.
#[derive(Debug, Clone, Copy, PartialEq)]
#[non_exhaustive]
pub struct HostMenuContext {
    /// Background, series, study or drawing that was clicked, including its screen position.
    /// A click on an axis reads as `Background` at the click.
    pub target: RightClickTarget,
    /// UTC time of the nearest displayed bar or element; `None` outside loaded data and on a
    /// price axis.
    pub time: Option<chrono::DateTime<chrono::Utc>>,
    /// Value at the click, using the clicked pane's scale for drawings. On a price axis, the value
    /// at the click height on the price scale of the pane's first series or study, which in the
    /// main series' pane is the main series' scale; `NaN` on the time axis and where the pane
    /// reads no value.
    pub price: f64,
    /// The menu the click opened.
    pub menu: ChartMenuKind,
}

impl HostMenuContext {
    /// A click on `target` at `price`, near the bar or element at `time` if there is one.
    /// `menu` is the menu a click on `target` opens.
    ///
    /// ```
    /// use egui_charts::ui::context_menu::{ChartMenuKind, HostMenuContext};
    /// use egui_charts::widget::RightClickTarget;
    ///
    /// let target = RightClickTarget::Background {
    ///     pos: egui::pos2(10.0, 20.0),
    ///     price: 100.0,
    /// };
    /// let context = HostMenuContext::new(target, None, 100.0);
    /// assert_eq!(context.price, 100.0);
    /// assert_eq!(context.menu, ChartMenuKind::Chart);
    /// ```
    pub fn new(
        target: RightClickTarget,
        time: Option<chrono::DateTime<chrono::Utc>>,
        price: f64,
    ) -> Self {
        Self {
            target,
            time,
            price,
            menu: match target {
                RightClickTarget::Element {
                    id: ChartElementId::Indicator(_),
                    ..
                } => ChartMenuKind::Study,
                RightClickTarget::Element { .. } => ChartMenuKind::Series,
                RightClickTarget::Drawing { .. } => ChartMenuKind::Drawing,
                RightClickTarget::Background { .. } => ChartMenuKind::Chart,
            },
        }
    }
}

/// A host row appended to the chart, series, study and drawing menus in registration order;
/// [`ChartMenus::set_menu_rows`] can move it.
///
/// Use [`Self::when`] to restrict a row to particular click targets. Host ids must be unique
/// within one [`ChartMenus`]. The host owns any services invoked by the returned action.
pub struct HostMenuEntry {
    /// Stable id returned in [`ChartContextAction::Host`].
    pub id: String,
    /// Text painted with the built-in menu style.
    pub label: String,
    /// Optional icon painted with the built-in tint and size.
    pub icon: Option<&'static Icon>,
    condition: Box<dyn Fn(&HostMenuContext) -> bool>,
}

impl std::fmt::Debug for HostMenuEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HostMenuEntry")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("icon", &self.icon)
            .finish_non_exhaustive()
    }
}

impl HostMenuEntry {
    /// Create a row visible for every chart, series, study and drawing click.
    pub fn new(id: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            icon: None,
            condition: Box::new(|_| true),
        }
    }

    /// Paint an icon beside the label.
    pub fn with_icon(mut self, icon: &'static Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Omit this row when the predicate is false for the original click.
    pub fn when(mut self, condition: impl Fn(&HostMenuContext) -> bool + 'static) -> Self {
        self.condition = Box::new(condition);
        self
    }
}

/// A context-menu action surfaced by the turnkey right-click path, tagged with
/// the menu it came from.
///
/// The chart opens the menu of the background, series, study, drawing or axis at the click.
/// [`ChartMenus::show`] returns the chosen action as this unified value so a host can match a
/// single type.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ChartContextAction {
    /// Joining the displayed scales of a pane, already applied with undo support.
    MergePaneAxes {
        /// The pane at the original click.
        pane: crate::widget::PaneId,
        /// The edge where its shared axis is displayed.
        side: crate::model::AxisSide,
    },
    /// A pane's axis layout, already applied with undo support by [`ChartMenus`].
    PaneAxes {
        /// The pane at the original click.
        pane: crate::widget::PaneId,
        /// Its new saved axis layout.
        options: crate::model::PaneAxisOptions,
    },
    /// A registered host entry or a host row of [`ChartMenus::set_menu_rows`], with the original
    /// click context.
    Host {
        /// The entry id, or the action of the host row's [`MenuItem`].
        id: String,
        /// The click that opened the menu, retained even if the pointer or chart moves.
        context: HostMenuContext,
    },
    /// An action from the chart-background menu.
    Chart(ContextMenuAction),
    /// An action from the series menu.
    Series(crate::ui::dialogs::SeriesContextMenuAction),
    /// An action from the menu of study `id`, opened from its legend row or plot.
    Study {
        /// The study the menu acts on.
        id: SeriesId,
        /// The chosen action, already applied to the chart when it is the chart's to apply.
        action: crate::ui::dialogs::StudyContextMenuAction,
    },
    /// An action from the drawing menu.
    Drawing(crate::ui::dialogs::DrawingContextMenuAction),
}

/// The chart, series, study, drawing and axis menus of one [`Chart`].
///
/// Keep one next to the chart and call [`show`](Self::show) after each
/// [`Chart::show`]: a right-click on the chart opens the menu for what it landed on, and the
/// item the user picks comes back as a [`ChartContextAction`]. The menus follow the theme
/// applied with [`apply_to_egui`](crate::theme::apply_to_egui).
///
/// ```
/// use egui_charts::ui::context_menu::{ChartContextAction, ChartMenus, HostMenuEntry};
/// use egui_charts::widget::RightClickTarget;
/// let mut menus = ChartMenus::default();
/// menus.host_entries.push(HostMenuEntry::new("ticket", "Send to ticket")
///     .when(|click| matches!(click.target, RightClickTarget::Background { .. })));
/// // After chart.show(ui), handle ChartContextAction::Host { id, context } from menus.show.
/// ```
#[derive(Debug, Default)]
pub struct ChartMenus {
    chart: ChartContextMenu,
    series: SeriesContextMenu,
    study: StudyContextMenu,
    drawing: DrawingContextMenu,
    axes: crate::ui::context_menu::axis::AxisMenus,
    tpo: Option<TpoMenu>,
    trade_profile: Option<TradeProfileMenu>,
    /// Host rows in display order, after the built-in rows of the chart, series, study and
    /// drawing menus. Empty by default.
    pub host_entries: Vec<HostMenuEntry>,
    menu_rows: Option<rows::MenuRows>,
    context: Option<HostMenuContext>,
    context_pane: Option<crate::widget::PaneId>,
    just_opened: bool,
    price_menu: bool,
    price_button: Option<egui::Pos2>,
    pane_axes: Option<(crate::widget::PaneId, crate::model::PaneAxisOptions)>,
}

impl ChartMenus {
    /// The price of the series menu's last right-click, which stays after a pick.
    pub fn series_click_price(&self) -> f64 {
        self.series.click_price
    }

    /// Shows or hides named menu entries: Alerts, Trading, Settings, ObjectTree, Indicators,
    /// ResetView, CopyPrice, ExportData, TimeZone and Session. `ContextMenus` disables all menus,
    /// including drawings and axes. Controls belonging only to toolbars are ignored.
    pub fn set_control_visible(&mut self, control: Control, visible: bool) {
        set_control_visible(&mut self.chart.hidden, control, visible);
        set_control_visible(&mut self.series.hidden, control, visible);
        set_control_visible(&mut self.study.hidden, control, visible);
        set_control_visible(&mut self.axes.hidden, control, visible);
        set_control_visible(&mut self.drawing.hidden, control, visible);
    }

    /// Sets the quantity of the chart menu's order entries and the order types the account
    /// accepts, such as [`Trading::quantity`](crate::ui::trading::Trading::quantity) and
    /// [`Trading::capabilities`](crate::ui::trading::Trading::capabilities). By default the
    /// entries are for 1 unit and offer limit and stop orders.
    pub fn set_order_entries(&mut self, quantity: f64, capabilities: Capabilities) {
        self.chart.order_quantity = quantity;
        self.chart.order_types = capabilities;
    }

    /// Writes the chart menu's order quantities with the host's text where it returns one.
    /// Give this and [`Trading::set_formatter`](crate::ui::trading::Trading::set_formatter)
    /// the same formatter. With `None`, the default, quantities use their own decimals, up to
    /// eight, with thousands separators, as the account tables write them.
    pub fn set_order_formatter(&mut self, formatter: Option<Arc<dyn TradingFormatter>>) {
        self.chart.order_formatter = formatter.map(OrderFormatter);
    }

    /// Offers copying a study, or the chart's whole study set, to the other charts of the layout,
    /// for a host showing several charts. The choices return as
    /// [`StudyContextMenuAction::AddToLayout`](crate::ui::dialogs::StudyContextMenuAction::AddToLayout)
    /// and [`ContextMenuAction::ApplyIndicatorsToLayout`] for
    /// [`ChartLayout::apply_copy`](crate::widget::ChartLayout::apply_copy). Off by default.
    pub fn set_layout_copies(&mut self, offered: bool) {
        self.chart.layout_copies = offered;
        self.study.layout_copies = offered;
    }

    /// Offers adding the chart's symbol to these watchlists, such as the names of
    /// [`WatchlistPanel::lists`](crate::ui::widget_bar::WatchlistPanel::lists), in a submenu.
    /// The choice returns as [`ContextMenuAction::AddToWatchlist`] with the list's index. Without
    /// names the menu has no such entry.
    pub fn set_watchlists(&mut self, names: impl IntoIterator<Item = impl Into<String>>) {
        self.chart.watchlists = names.into_iter().map(Into::into).collect();
    }

    /// Shows the binding of [`KeyboardAction::AddToWatchlist`] on the row at `list` in
    /// [`set_watchlists`](Self::set_watchlists), such as the panel's active list.
    pub fn set_active_watchlist(&mut self, list: Option<usize>) {
        self.chart.active_watchlist = list;
    }

    /// Lets the host place, reorder and leave out the rows of every chart menu: the chart, series,
    /// study and drawing menus and the price and time axis menus.
    ///
    /// On every frame a menu is open, `rows` receives the original click, whose
    /// [`menu`](HostMenuContext::menu) names the menu, and the menu's rows after
    /// [`set_control_visible`](Self::set_control_visible) and [`host_entries`](Self::host_entries),
    /// and returns the rows to show. A [`MenuRow::BuiltIn`] row does what it does without this hook;
    /// an id that is not a row of this menu, such as a submenu's id or a row of another menu, and a
    /// second copy of a row are left out. A [`MenuRow::Host`] row returns
    /// [`ChartContextAction::Host`]. Separators at either end or next to another separator and
    /// submenus without rows are left out, and a menu without rows does not open. The price actions
    /// menu of the pointer button keeps its rows. Return the same rows for the same click, so that an
    /// open submenu stays open.
    ///
    /// ```
    /// use egui_charts::ui::context_menu::{ChartMenuKind, ChartMenus, MenuItem, MenuRow, MenuRowId};
    /// let mut menus = ChartMenus::default();
    /// menus.set_menu_rows(|click, mut rows| {
    ///     if click.menu == ChartMenuKind::Chart {
    ///         rows.retain(|row| row.id() != Some(MenuRowId::ViewDataTable));
    ///         let settings = rows.iter().position(|row| row.id() == Some(MenuRowId::ChartSettings));
    ///         let ticket = MenuItem::new("Send to ticket", "ticket".to_owned());
    ///         rows.insert(settings.unwrap_or(rows.len()), MenuRow::Host(ticket));
    ///     }
    ///     rows
    /// });
    /// ```
    pub fn set_menu_rows(
        &mut self,
        rows: impl Fn(&HostMenuContext, Vec<MenuRow>) -> Vec<MenuRow> + 'static,
    ) {
        self.menu_rows = Some(rows::MenuRows(Box::new(rows)));
    }

    /// While Alt and the command modifier are held, shows a button at the pointer and returns the
    /// price target once it is clicked. The press position holds through the click.
    fn show_price_button(&mut self, ctx: &Context, chart: &Chart) -> Option<RightClickTarget> {
        if self.context.is_some()
            || !chart.pointer_commands_enabled(ctx)
            || !ctx.input(|input| {
                input.modifiers.alt && input.modifiers.command && !input.modifiers.shift
            })
        {
            self.price_button = None;
            return None;
        }
        let previous = self.price_button.take();
        let pos = ctx.input(|input| {
            if input.pointer.primary_down() || input.pointer.primary_released() {
                previous.or(input.pointer.hover_pos())
            } else {
                input.pointer.hover_pos()
            }
        })?;
        let (_, price) = chart.pointer_price(pos)?;
        self.price_button = Some(pos);
        let chart_response = ctx.read_response(chart.keyboard.chart_id?)?;
        if !pointer_on_chart(ctx, chart, pos) {
            self.price_button = None;
            return None;
        }
        let mut ui = egui::Ui::new(
            ctx.clone(),
            chart_response.id.with("pointer_price"),
            egui::UiBuilder::new()
                .layer_id(chart_response.layer_id)
                .max_rect(egui::Rect::from_center_size(pos, egui::vec2(20.0, 20.0))),
        );
        ui.spacing_mut().button_padding = egui::Vec2::ZERO;
        let response = ui
            .add_sized(
                egui::vec2(20.0, 20.0),
                egui::Button::new("+").corner_radius(10.0),
            )
            .on_hover_text(crate::localization::text(ctx, "Price actions"));
        response
            .clicked()
            .then_some(RightClickTarget::Background { pos, price })
    }

    /// The order of the chart menu's buy or sell limit row at the pointer, or `None` where the
    /// menu would show no such row there.
    fn limit_order_at_pointer(
        &mut self,
        ctx: &Context,
        chart: &Chart,
        key: KeyboardAction,
    ) -> Option<ChartContextAction> {
        let side = match key {
            KeyboardAction::BuyLimitAtPointer => OrderSide::Buy,
            KeyboardAction::SellLimitAtPointer => OrderSide::Sell,
            _ => return None,
        };
        if !chart.pointer_commands_enabled(ctx) {
            return None;
        }
        let pos = ctx.input(|input| input.pointer.hover_pos())?;
        let (pane, price) = chart.pointer_price(pos)?;
        if chart.series_pane(SeriesId::MAIN) != Some(pane) || !pointer_on_chart(ctx, chart, pos) {
            return None;
        }
        self.load_order_context(chart, pos, price);
        self.chart
            .order_requests(price)
            .into_iter()
            .find(|order| order.side == side && matches!(order.order_type, OrderType::Limit(_)))
            .map(|order| ChartContextAction::Chart(ContextMenuAction::Order(order)))
    }

    /// Reads the symbol, its last price and tick size, the decimals used for `price`, and whether
    /// `pos` reads prices of the chart symbol into the menu, for its alert and order rows.
    fn load_order_context(&mut self, chart: &Chart, pos: egui::Pos2, price: f64) {
        self.chart.price_decimals = chart.menu_price_decimals(SeriesId::MAIN, price);
        self.chart.set_symbol(chart.symbol.clone());
        self.chart.market_price =
            crate::model::last_with_value(chart.bars(SeriesId::MAIN)).map(|b| b.close);
        self.chart.tick_size = chart
            .instrument()
            .and_then(|instrument| instrument.tick_size)
            .filter(|tick| tick.is_finite() && *tick > 0.0);
        self.chart.symbol_price = chart
            .pane_at_position(pos)
            .is_none_or(|pane| Some(pane) == chart.series_pane(SeriesId::MAIN));
    }

    /// Opens the menu for the chart's last right-click, shows the open one and returns the
    /// chosen action. Alt plus the platform command modifier reveals a price button at the
    /// pointer, including when the price-axis button is hidden. Clicking it offers the line in
    /// the pointed pane, and alert and order actions in the pane of the main series, whose scale
    /// reads prices of the chart symbol. [`KeyboardAction::BuyLimitAtPointer`] and
    /// [`KeyboardAction::SellLimitAtPointer`], Alt+Shift+B and Alt+Shift+S unless the chart's
    /// [`ShortcutMap`](crate::widget::ShortcutMap) binds them otherwise, return the order of the
    /// chart menu's buy or sell limit row at the pointer, and nothing where the menu would show
    /// no such row. Editors and drawing drags retain their input.
    /// While a menu is open the chart draws no crosshair or tooltip under it.
    ///
    /// [`KeyboardAction::BuyLimitAtPointer`]: crate::config::KeyboardAction::BuyLimitAtPointer
    /// [`KeyboardAction::SellLimitAtPointer`]: crate::config::KeyboardAction::SellLimitAtPointer
    pub fn show(&mut self, ctx: &Context, chart: &mut Chart) -> Option<ChartContextAction> {
        let order_key = chart.keyboard.order_key.take();
        if self.chart.hidden.contains(&Control::ContextMenus) {
            self.chart.close();
            self.series.close();
            self.study.close();
            self.drawing.close();
            self.axes.close();
            self.context = None;
            self.context_pane = None;
            chart.take_right_click();
            chart.menu_open = false;
            return None;
        }
        if self.axes.open_from_pointer(ctx, chart) {
            self.chart.close();
            self.series.close();
            self.study.close();
            self.drawing.close();
            self.context = None;
            self.context_pane = None;
        }
        let key_order = order_key.and_then(|key| self.limit_order_at_pointer(ctx, chart, key));
        let quick_price = self.show_price_button(ctx, chart);
        if let Some(target) = quick_price.or_else(|| chart.take_right_click()) {
            self.price_menu = quick_price.is_some();
            self.axes.close();
            let pos = match target {
                RightClickTarget::Element { pos, .. }
                | RightClickTarget::Drawing { pos, .. }
                | RightClickTarget::Background { pos, .. } => pos,
            };
            self.context_pane = chart.pointer_price(pos).map(|(pane, _)| pane);
            let mapping = chart.get_chart_mapping();
            let time = bar_time_at(chart, pos.x);
            let price = match target {
                RightClickTarget::Element { price, .. }
                | RightClickTarget::Background { price, .. } => price,
                RightClickTarget::Drawing { .. } => chart
                    .get_rendered_indicator_panes()
                    .iter()
                    .find(|pane| pane.chart_rect.contains(pos))
                    .map_or_else(
                        || mapping.y_to_price(pos.y),
                        |pane| pane.mapping.y_to_price(pos.y),
                    ),
            };
            self.context = Some(HostMenuContext::new(target, time, price));
            self.tpo = match target {
                RightClickTarget::Background { .. }
                | RightClickTarget::Element {
                    id: ChartElementId::Series(SeriesId::MAIN),
                    ..
                } if !self.price_menu => TpoMenu::open(chart, pos),
                _ => None,
            };
            self.trade_profile = match target {
                RightClickTarget::Background { .. }
                | RightClickTarget::Element {
                    id: ChartElementId::Series(SeriesId::MAIN),
                    ..
                } if !self.price_menu => TradeProfileMenu::open(chart, pos, time),
                _ => None,
            };
            self.pane_axes = chart
                .pane_at_position(pos)
                .and_then(|pane| chart.pane_axis_options(pane).map(|options| (pane, options)));
            self.just_opened = true;
            // A new right-click supersedes any menu still open.
            self.chart.close();
            self.series.close();
            self.study.close();
            self.drawing.close();
            match target {
                RightClickTarget::Element {
                    id: ChartElementId::Indicator(id),
                    bar_idx,
                    ..
                } => self.study.open(ctx, chart, id, bar_idx),
                RightClickTarget::Element {
                    id: ChartElementId::Series(id),
                    pos,
                    price,
                    ..
                } => {
                    self.series.set_context(chart.symbol.clone(), price);
                    self.series.open(pos, id);
                }
                RightClickTarget::Drawing { drawing_id, pos } => {
                    self.drawing.open_on(ctx, chart, pos, drawing_id);
                }
                RightClickTarget::Background { pos, price } => {
                    self.chart.set_indicator_cnt(chart.indicators().count());
                    self.load_order_context(chart, pos, price);
                    let line = chart
                        .drawings()
                        .registry
                        .availability(crate::drawings::DrawingToolType::HorizontalLine);
                    self.chart.horizontal_line = line.visible && line.enabled;
                    self.chart.open(pos, price);
                }
            }
        }

        let theme = ctx.active_theme();
        let mut menu_action = None;
        if let Some(context) = self.context {
            let (pos, gap) = match context.target {
                RightClickTarget::Element { pos, .. }
                | RightClickTarget::Background { pos, .. } => (pos, POINTER_GAP),
                RightClickTarget::Drawing { pos, .. } => (pos, DRAWING_MENU_OFFSET),
            };
            let entries = self.build_menu_items(ctx, chart, context);
            if let Some(picked) = show_menu(
                ctx,
                Id::unique("chart_host_menu"),
                &theme,
                pos,
                gap,
                &entries,
                std::mem::take(&mut self.just_opened),
            ) {
                if let Some(ChartContextAction::MergePaneAxes { pane, side }) = &picked {
                    chart.user_action(|chart| chart.merge_pane_scales(*pane, *side));
                }
                if let Some(ChartContextAction::PaneAxes { pane, options }) = &picked {
                    chart.user_action(|chart| chart.set_pane_axis_options(*pane, *options));
                }
                if let Some(ChartContextAction::Drawing(action)) = &picked
                    && let RightClickTarget::Drawing { drawing_id, .. } = context.target
                {
                    if *action == crate::ui::dialogs::DrawingContextMenuAction::ReversePosition {
                        if chart.drawings().user_can_edit(drawing_id) {
                            let _ = chart.reverse_position_drawing(drawing_id);
                        }
                    } else {
                        action.apply_interval_visibility(chart, drawing_id);
                    }
                }
                if let Some(ChartContextAction::Study { id, action }) = &picked {
                    chart.user_action(|chart| study_context_menu::apply(chart, *id, action));
                }
                if let Some(ChartContextAction::Chart(action)) = &picked
                    && let Some(tpo) = &self.tpo
                {
                    chart.user_action(|chart| tpo.apply(chart, action));
                }
                if let Some(ChartContextAction::Chart(action)) = &picked
                    && let Some(profile) = &self.trade_profile
                {
                    chart.user_action(|chart| profile.apply(chart, action));
                }
                if let Some(ChartContextAction::Chart(ContextMenuAction::DrawHorizontalLine(price))) =
                    &picked
                    && let RightClickTarget::Background { pos, .. } = context.target
                {
                    let pane = self.context_pane;
                    chart.user_action(|chart| {
                        if pane.is_some_and(|pane| !chart.panes().contains(&pane)) {
                            return;
                        }
                        if let Some(id) = chart.draw_horizontal_line_at(pos, *price)
                            && let Some(pane) = pane
                            && chart.drawing_pane(id) != Some(pane)
                        {
                            chart.move_drawing_to_pane(id, pane);
                        }
                    });
                }
                menu_action = picked;
                self.context = None;
                self.context_pane = None;
                self.chart.close();
                self.series.close();
                self.study.close();
                self.drawing.close();
            }
        }
        let axis_action =
            chart.user_action(|chart| self.axes.show(ctx, chart, self.menu_rows.as_ref()));
        chart.menu_open = self.chart.is_open()
            || self.series.is_open()
            || self.study.is_open()
            || self.drawing.is_open()
            || self.axes.is_open();

        // At most one menu is open at a time, and keys wait while one is, so at most one action
        // resolves.
        axis_action.or(menu_action).or(key_order)
    }

    fn build_menu_items(
        &self,
        ctx: &Context,
        chart: &Chart,
        context: HostMenuContext,
    ) -> Vec<MenuEntry<ChartContextAction>> {
        let shortcuts = chart.shortcuts();
        let pos = match context.target {
            RightClickTarget::Element { pos, .. }
            | RightClickTarget::Drawing { pos, .. }
            | RightClickTarget::Background { pos, .. } => pos,
        };
        // The price as the chart's scale writes it at the click, as the host copies it.
        let price_text = |price| {
            chart
                .format_menu_price(pos, price)
                .unwrap_or_else(|| super::format_price(price))
        };
        let mut store = Vec::new();
        let mut rows = match context.target {
            RightClickTarget::Background { .. } => rows::split(
                self.chart.build_menu_items(ctx, shortcuts, &price_text),
                &ContextMenuAction::row_id,
                &ChartContextAction::Chart,
                &mut store,
            ),
            RightClickTarget::Element {
                id: ChartElementId::Indicator(id),
                ..
            } => rows::split(
                self.study.build_menu_items(ctx, shortcuts),
                &|action| self.study.row_id(action),
                &|action| ChartContextAction::Study { id, action },
                &mut store,
            ),
            RightClickTarget::Element { .. } => rows::split(
                self.series.build_menu_items(ctx, shortcuts, &price_text),
                &SeriesContextMenuAction::row_id,
                &ChartContextAction::Series,
                &mut store,
            ),
            RightClickTarget::Drawing { .. } => rows::split(
                self.drawing.build_menu_items(ctx, shortcuts),
                &DrawingContextMenuAction::row_id,
                &ChartContextAction::Drawing,
                &mut store,
            ),
        };
        let host = |id| ChartContextAction::Host { id, context };
        if self.price_menu {
            rows.retain(|row| {
                matches!(
                    row,
                    MenuRow::BuiltIn(
                        MenuRowId::AddAlert
                            | MenuRowId::DrawHorizontalLine
                            | MenuRowId::LimitOrder
                            | MenuRowId::StopOrder
                            | MenuRowId::StopLimitOrder
                    )
                )
            });
            return rows::join(rows, &mut store, &host);
        }
        if let Some((pane, options)) = self.pane_axes
            && matches!(context.target, RightClickTarget::Background { .. })
        {
            rows.push(MenuRow::Submenu {
                id: Some(MenuRowId::PaneAxes),
                label: "Pane axes".into(),
                icon: None,
                rows: rows::split(
                    super::axis::layout_entries(options),
                    &|action| Some(action.row_id()),
                    &|action| match action {
                        super::axis::LayoutAction::Side(_, options)
                        | super::axis::LayoutAction::HideUnused(options) => {
                            ChartContextAction::PaneAxes { pane, options }
                        }
                        super::axis::LayoutAction::Merge(side) => {
                            ChartContextAction::MergePaneAxes { pane, side }
                        }
                    },
                    &mut store,
                ),
            });
        }
        if let Some(tpo) = &self.tpo {
            let mut profile = rows::split(
                tpo.entries(),
                &ContextMenuAction::row_id,
                &ChartContextAction::Chart,
                &mut store,
            );
            if !rows.is_empty() {
                profile.push(MenuRow::Separator);
            }
            profile.extend(rows);
            rows = profile;
        }
        if let Some(profile) = &self.trade_profile {
            let mut profile = rows::split(
                profile.entries(),
                &ContextMenuAction::row_id,
                &ChartContextAction::Chart,
                &mut store,
            );
            if !rows.is_empty() {
                profile.push(MenuRow::Separator);
            }
            profile.extend(rows);
            rows = profile;
        }
        let mut separated = false;
        for entry in &self.host_entries {
            if !(entry.condition)(&context) {
                continue;
            }
            if !separated {
                if !rows.is_empty() {
                    rows.push(MenuRow::Separator);
                }
                separated = true;
            }
            let mut item = MenuItem::new(&entry.label, entry.id.clone());
            item.icon = entry.icon;
            rows.push(MenuRow::Host(item));
        }
        let rows = match &self.menu_rows {
            Some(hook) => (hook.0)(&context, rows),
            None => rows,
        };
        rows::join(rows, &mut store, &host)
    }
}

/// UTC time of the displayed bar nearest to screen `x`; `None` outside the loaded bars.
pub(super) fn bar_time_at(chart: &Chart, x: f32) -> Option<chrono::DateTime<chrono::Utc>> {
    let index = chart.get_chart_mapping().x_to_idx_f64(x).round();
    (index >= 0.0)
        .then(|| chart.state().shown().get(index as usize))
        .flatten()
        .map(|bar| bar.time)
}

/// Whether `pos` is on the enabled chart's own layer, not on a window or popup over it.
fn pointer_on_chart(ctx: &Context, chart: &Chart, pos: egui::Pos2) -> bool {
    chart
        .keyboard
        .chart_id
        .and_then(|id| ctx.read_response(id))
        .is_some_and(|response| {
            response.enabled() && ctx.layer_id_at(pos) == Some(response.layer_id)
        })
}

#[cfg(test)]
mod tests;
