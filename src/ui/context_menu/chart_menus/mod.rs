use super::{ChartContextMenu, ContextMenuAction, MenuEntry, MenuItem, POINTER_GAP, show_menu};
use crate::chart::selection::{ChartElementId, SeriesId};
use crate::icons::Icon;
use crate::model::Capabilities;
use crate::theme::ThemeExt;
use crate::ui::Control;
use crate::ui::app_state::set_control_visible;
use crate::ui::context_menu::drawing::DRAWING_MENU_OFFSET;
use crate::ui::context_menu::study::{self as study_context_menu, StudyContextMenu};
use crate::ui::context_menu::tpo::TpoMenu;
use crate::ui::dialogs::{DrawingContextMenu, SeriesContextMenu};
use crate::widget::{Chart, RightClickTarget};
use egui::{Context, Id};

/// The original click retained while a host menu action is being chosen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HostMenuContext {
    /// Background, series, study or drawing that was clicked, including its screen position.
    pub target: RightClickTarget,
    /// UTC time of the nearest displayed bar or element; `None` outside loaded data.
    pub time: Option<chrono::DateTime<chrono::Utc>>,
    /// Value at the click, using the clicked pane's scale for drawings.
    pub price: f64,
}

/// A host row appended to the chart, series and drawing menus in registration order.
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

impl HostMenuEntry {
    /// Create a row visible for every chart, series and drawing click.
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
/// The chart opens one of three menus depending on what the right-click landed
/// on (background, a series/indicator, or a drawing); each menu has its own
/// action enum. [`ChartMenus::show`] returns this unified value so a host can match a single
/// type.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum ChartContextAction {
    /// A registered host entry, with the original click context.
    Host {
        /// The registered entry id.
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

/// The chart, series and drawing context menus of one [`Chart`].
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
#[derive(Default)]
pub struct ChartMenus {
    chart: ChartContextMenu,
    series: SeriesContextMenu,
    study: StudyContextMenu,
    drawing: DrawingContextMenu,
    axes: crate::ui::context_menu::axis::AxisMenus,
    tpo: Option<TpoMenu>,
    /// Host rows in display order. Empty by default.
    pub host_entries: Vec<HostMenuEntry>,
    context: Option<HostMenuContext>,
    just_opened: bool,
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
    }

    /// Sets the quantity of the chart menu's order entries and the order types the account
    /// accepts, such as [`Trading::quantity`](crate::ui::trading::Trading::quantity) and
    /// [`Trading::capabilities`](crate::ui::trading::Trading::capabilities). By default the
    /// entries are for 1 unit and offer limit and stop orders.
    pub fn set_order_entries(&mut self, quantity: f64, capabilities: Capabilities) {
        self.chart.order_quantity = quantity;
        self.chart.order_types = capabilities;
    }

    /// Offers adding the chart's symbol to these watchlists, such as the names of
    /// [`WatchlistPanel::lists`](crate::ui::widget_bar::WatchlistPanel::lists), in a submenu.
    /// The choice returns as [`ContextMenuAction::AddToWatchlist`] with the list's index. Without
    /// names the menu has no such entry.
    pub fn set_watchlists(&mut self, names: impl IntoIterator<Item = impl Into<String>>) {
        self.chart.watchlists = names.into_iter().map(Into::into).collect();
    }

    /// Opens the menu for the chart's last right-click, shows the open one and returns the
    /// chosen action. While a menu is open the chart draws no crosshair or tooltip under it.
    pub fn show(&mut self, ctx: &Context, chart: &mut Chart) -> Option<ChartContextAction> {
        if self.chart.hidden.contains(&Control::ContextMenus) {
            self.chart.close();
            self.series.close();
            self.study.close();
            self.drawing.close();
            self.axes.close();
            self.context = None;
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
        }
        if let Some(target) = chart.take_right_click() {
            self.axes.close();
            let pos = match target {
                RightClickTarget::Element { pos, .. }
                | RightClickTarget::Drawing { pos, .. }
                | RightClickTarget::Background { pos, .. } => pos,
            };
            let mapping = chart.get_chart_mapping();
            let index = mapping.x_to_idx_f64(pos.x).round();
            let time = (index >= 0.0)
                .then(|| chart.state().shown().get(index as usize))
                .flatten()
                .map(|bar| bar.time);
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
            self.context = Some(HostMenuContext {
                target,
                time,
                price,
            });
            self.tpo = match target {
                RightClickTarget::Background { .. }
                | RightClickTarget::Element {
                    id: ChartElementId::Series(SeriesId::MAIN),
                    ..
                } => TpoMenu::open(chart, pos),
                _ => None,
            };
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
                } => self.study.open(chart, id, bar_idx),
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
                    self.drawing.open_on(chart, pos, drawing_id);
                }
                RightClickTarget::Background { pos, price } => {
                    self.chart.set_indicator_cnt(chart.indicators().count());
                    self.chart.set_symbol(chart.symbol.clone());
                    self.chart.market_price =
                        crate::model::last_with_value(chart.bars(SeriesId::MAIN)).map(|b| b.close);
                    self.chart.tick_size = chart
                        .instrument()
                        .and_then(|instrument| instrument.tick_size)
                        .filter(|tick| tick.is_finite() && *tick > 0.0);
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
            let entries = self.build_menu_items(chart.shortcuts(), context);
            if let Some(picked) = show_menu(
                ctx,
                Id::unique("chart_host_menu"),
                &theme,
                pos,
                gap,
                &entries,
                std::mem::take(&mut self.just_opened),
            ) {
                if let Some(ChartContextAction::Study { id, action }) = &picked {
                    chart.user_action(|chart| study_context_menu::apply(chart, *id, action));
                }
                if let Some(ChartContextAction::Chart(action)) = &picked
                    && let Some(tpo) = &self.tpo
                {
                    chart.user_action(|chart| tpo.apply(chart, action));
                }
                if let Some(ChartContextAction::Chart(ContextMenuAction::DrawHorizontalLine(price))) =
                    &picked
                    && let RightClickTarget::Background { pos, .. } = context.target
                {
                    chart.user_action(|chart| chart.draw_horizontal_line_at(pos, *price));
                }
                menu_action = picked;
                self.context = None;
                self.chart.close();
                self.series.close();
                self.study.close();
                self.drawing.close();
            }
        }
        let axis_action = chart.user_action(|chart| self.axes.show(ctx, chart));
        chart.menu_open = self.chart.is_open()
            || self.series.is_open()
            || self.study.is_open()
            || self.drawing.is_open()
            || self.axes.is_open();

        // At most one menu is open at a time, so at most one action resolves.
        axis_action.map(ChartContextAction::Chart).or(menu_action)
    }

    fn build_menu_items(
        &self,
        shortcuts: &crate::widget::ShortcutMap,
        context: HostMenuContext,
    ) -> Vec<MenuEntry<ChartContextAction>> {
        fn tagged<A>(
            entries: Vec<MenuEntry<A>>,
            tag: impl Fn(A) -> ChartContextAction,
        ) -> Vec<MenuEntry<ChartContextAction>> {
            entries.into_iter().map(|entry| entry.map(&tag)).collect()
        }
        let mut entries = match context.target {
            RightClickTarget::Background { .. } => tagged(
                self.chart.build_menu_items(shortcuts),
                ChartContextAction::Chart,
            ),
            RightClickTarget::Element {
                id: ChartElementId::Indicator(id),
                ..
            } => tagged(self.study.build_menu_items(shortcuts), |action| {
                ChartContextAction::Study { id, action }
            }),
            RightClickTarget::Element { .. } => tagged(
                self.series.build_menu_items(shortcuts),
                ChartContextAction::Series,
            ),
            RightClickTarget::Drawing { .. } => tagged(
                self.drawing.build_menu_items(shortcuts),
                ChartContextAction::Drawing,
            ),
        };
        if let Some(tpo) = &self.tpo {
            let mut profile = tagged(tpo.entries(), ChartContextAction::Chart);
            if !entries.is_empty() {
                profile.push(MenuEntry::Separator);
            }
            profile.extend(entries);
            entries = profile;
        }
        let mut separated = false;
        for entry in &self.host_entries {
            if !(entry.condition)(&context) {
                continue;
            }
            if !separated {
                if !entries.is_empty() {
                    entries.push(MenuEntry::Separator);
                }
                separated = true;
            }
            let mut item = MenuItem::new(
                &entry.label,
                ChartContextAction::Host {
                    id: entry.id.clone(),
                    context,
                },
            );
            item.icon = entry.icon;
            entries.push(MenuEntry::Item(item));
        }
        entries
    }
}

#[cfg(test)]
mod tests;
