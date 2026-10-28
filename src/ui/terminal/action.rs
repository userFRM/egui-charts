//! What the terminal's controls emit: each component's own action, unchanged.

use crate::model::Instrument;
use crate::ui::context_menu::ChartContextAction;
use crate::ui::dialogs::{AlertDialogAction, DrawingPropertiesAction};
use crate::ui::drawing_toolbar::DrawingToolbarAction;
use crate::ui::export::ExportResponse;
use crate::ui::floating_replay_control::ReplayControlAction;
use crate::ui::replay::ReplayStartAction;
use crate::ui::strategy_report::StrategyReportAction;
use crate::ui::timeframe_toolbar::{DatePickerAction, TimeframeToolbarAction};
use crate::ui::top_toolbar::components::indicator_dialog::IndicatorDialogAction;
use crate::ui::top_toolbar::components::settings_menu::SettingsAction;
use crate::ui::top_toolbar::{CompareAction, LayoutAction, TopToolbarAction};
use crate::ui::trading::{AccountPanelAction, BrokerPickerAction};
use crate::ui::widget_bar::panels::object_tree::ObjectTreeAction;
use crate::ui::widget_bar::{
    AlertPanelAction, CalendarAction, HotlistsAction, NewsAction, NotificationsAction,
    ProductsAction, ScreenerAction, WatchlistAction, WidgetBarAction,
};
use crate::widget::ShortcutRequest;

/// What a terminal control emitted: the component's own action, unchanged.
///
/// [`Terminal::show`](super::Terminal::show) returns the actions the host intercepted and those
/// the terminal does not perform; [`Terminal::run`](super::Terminal::run) performs one.
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum Action {
    /// The top toolbar.
    TopToolbar(TopToolbarAction),
    /// The drawing toolbar.
    DrawingToolbar(DrawingToolbarAction),
    /// The bottom bar: date ranges, time zone, session and adjustments.
    BottomBar(TimeframeToolbarAction),
    /// The right-hand widget bar.
    WidgetBar(WidgetBarAction),
    /// The object tree panel.
    ObjectTree(ObjectTreeAction),
    /// The chart, series, study, drawing and axis context menus.
    ChartMenu(ChartContextAction),
    /// A key the chart hands to its host, from `Chart::take_shortcut_requests`.
    Shortcut(ShortcutRequest),
    /// The layout menu.
    Layout(LayoutAction),
    /// The floating replay control.
    Replay(ReplayControlAction),
    /// The replay start picker.
    ReplayStart(ReplayStartAction),
    /// The symbol search dialog's pick.
    SymbolPicked(Box<Instrument>),
    /// The compare symbols dialog.
    Compare(CompareAction),
    /// The indicator dialog.
    Indicators(IndicatorDialogAction),
    /// The chart settings dialog.
    Settings(SettingsAction),
    /// The drawing properties dialog.
    DrawingProperties(DrawingPropertiesAction),
    /// The alert dialog.
    AlertDialog(AlertDialogAction),
    /// The alerts panel.
    Alerts(AlertPanelAction),
    /// The export dialog.
    Export(ExportResponse),
    /// The go-to-date picker.
    DatePicker(DatePickerAction),
    /// The watchlist panel.
    Watchlist(WatchlistAction),
    /// The news and highlights panels.
    News(NewsAction),
    /// The calendar panel.
    Calendar(CalendarAction),
    /// The screener panel.
    Screener(ScreenerAction),
    /// The hotlists panel.
    Hotlists(HotlistsAction),
    /// The notifications panel.
    Notifications(NotificationsAction),
    /// The products panel.
    Products(ProductsAction),
    /// The trading account panel.
    Account(AccountPanelAction),
    /// The broker picker.
    BrokerPicker(BrokerPickerAction),
    /// The strategy report panel.
    StrategyReport(StrategyReportAction),
}
