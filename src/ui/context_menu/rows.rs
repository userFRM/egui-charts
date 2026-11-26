//! Rows a host can rearrange with [`ChartMenus::set_menu_rows`](super::ChartMenus::set_menu_rows).

use super::{HostMenuContext, MenuEntry, MenuItem, trim_separators};
use crate::icons::Icon;
use crate::widget::PaneId;

/// One row received and returned by [`ChartMenus::set_menu_rows`](super::ChartMenus::set_menu_rows).
#[derive(Clone, Debug)]
#[non_exhaustive]
pub enum MenuRow {
    /// A built-in row, retaining its original action.
    BuiltIn(MenuRowId),
    /// A host row whose action becomes the id of [`ChartContextAction::Host`](super::ChartContextAction::Host).
    Host(MenuItem<String>),
    /// A row that opens its children beside the menu.
    Submenu {
        /// The built-in submenu this is; `None` for a host submenu.
        id: Option<MenuRowId>,
        /// Text displayed by the row.
        label: String,
        /// Glyph displayed by the row.
        icon: Option<&'static Icon>,
        /// The rows of the submenu.
        rows: Vec<MenuRow>,
    },
    /// A line between groups of rows.
    Separator,
}

impl MenuRow {
    /// The built-in row or submenu id; `None` for host rows, host submenus and separators.
    pub fn id(&self) -> Option<MenuRowId> {
        match self {
            Self::BuiltIn(id) => Some(*id),
            Self::Submenu { id, .. } => *id,
            Self::Host(_) | Self::Separator => None,
        }
    }
}

/// Identifies a built-in row or submenu of the chart menus. An id stays the same when the
/// row's label, position or language changes; rows with the same meaning in several menus share
/// one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum MenuRowId {
    /// "Reset chart view" in the chart menu.
    ResetChartView,
    /// "Copy price" in the chart and series menus.
    CopyPrice,
    /// "Add alert" in the chart, series, study and drawing menus.
    AddAlert,
    /// "Draw horizontal line" in the chart menu.
    DrawHorizontalLine,
    /// The "Add to watchlist" submenu of the chart menu.
    AddToWatchlist,
    /// The row of the list at this index of [`ChartMenus::set_watchlists`](super::ChartMenus::set_watchlists).
    Watchlist(usize),
    /// "Add note" in the chart and series menus.
    AddNote,
    /// The limit order row of the chart menu.
    LimitOrder,
    /// The stop order row of the chart menu.
    StopOrder,
    /// The stop-limit order row of the chart menu.
    StopLimitOrder,
    /// "Object tree" in the chart and study menus.
    ObjectTree,
    /// "Remove indicators" in the chart menu.
    RemoveIndicators,
    /// "Apply indicators to all charts" in the chart menu.
    ApplyIndicatorsToLayout,
    /// "View Data Table" in the chart menu.
    ViewDataTable,
    /// "Export Data to CSV" in the chart menu.
    ExportData,
    /// "Series Settings" in the chart menu and "Settings" in the series menu.
    SeriesSettings,
    /// "Chart Settings" in the chart menu.
    ChartSettings,
    /// "Merge with previous profile" for a TPO profile.
    TpoMergePrevious,
    /// "Merge with next profile" for a TPO profile.
    TpoMergeNext,
    /// "Split profile" for a TPO profile.
    TpoSplit,
    /// "Merge with previous profile" for an execution profile.
    TradeProfileMergePrevious,
    /// "Merge with next profile" for an execution profile.
    TradeProfileMergeNext,
    /// "Split profile at selected bar" for an execution profile.
    TradeProfileSplit,
    /// The "Pane axes" submenu of the chart menu.
    PaneAxes,
    /// "Axes on left" in the pane axes rows of the chart and price axis menus.
    AxesOnLeft,
    /// "Axes on right", as above.
    AxesOnRight,
    /// The "Join price scales" submenu, as above.
    JoinPriceScales,
    /// "At left edge" in "Join price scales".
    JoinAtLeftEdge,
    /// "At right edge" in "Join price scales".
    JoinAtRightEdge,
    /// "Hide unused axes", as above.
    HideUnusedAxes,
    /// "Add indicator" in the series and study menus.
    AddIndicator,
    /// "Add this indicator to favorites" in the study menu.
    AddToFavorites,
    /// "Add to all charts in layout" in the study menu.
    AddToLayout,
    /// The "Visual order" submenu of the study menu.
    VisualOrder,
    /// "Bring to front" in the study and drawing menus.
    BringToFront,
    /// "Bring forward", as above.
    BringForward,
    /// "Send backward", as above.
    SendBackward,
    /// "Send to back", as above.
    SendToBack,
    /// The interval visibility submenu of the study and drawing menus.
    IntervalVisibility,
    /// Show on the chart's interval and longer ones.
    IntervalsAndAbove,
    /// Show on the chart's interval and shorter ones.
    IntervalsAndBelow,
    /// Show on the chart's interval only.
    CurrentIntervalOnly,
    /// Show on every interval.
    AllIntervals,
    /// The "Move to" submenu of the study and drawing menus.
    MoveTo,
    /// "New pane above" in the study menu.
    MoveToNewPaneAbove,
    /// "New pane below" in the study menu.
    MoveToNewPaneBelow,
    /// The row of this pane in "Move to".
    MoveToPane(PaneId),
    /// "Main pane" in the study menu's "Move to".
    MoveToMainPane,
    /// The "Pin to scale" submenu of the study menu.
    PinToScale,
    /// "Pin to right scale".
    PinToRightScale,
    /// "Pin to left scale".
    PinToLeftScale,
    /// "Pin to new scale".
    PinToNewScale,
    /// "Pin to scale" with this key.
    PinToSharedScale(u64),
    /// "No scale".
    NoScale,
    /// "Copy" in the study and drawing menus.
    Copy,
    /// "Hide" or "Show" in the study and drawing menus.
    ToggleVisibility,
    /// "Remove" in the study menu.
    Remove,
    /// "Settings" in the study menu.
    StudySettings,
    /// "Study help" in the study menu.
    StudyHelp,
    /// "Edit" in the drawing menu.
    Edit,
    /// "Reverse position" in the drawing menu.
    ReversePosition,
    /// "Lock" or "Unlock" in the drawing menu.
    Lock,
    /// "Clone" in the drawing menu.
    Clone,
    /// "Delete" in the drawing menu.
    Delete,
    /// "Auto scale" in the price axis menu.
    AutoScale,
    /// "Scale price chart only".
    ScalePriceChartOnly,
    /// "Regular".
    RegularScale,
    /// "Percent".
    PercentScale,
    /// "Indexed to 100".
    IndexedTo100Scale,
    /// "Logarithmic".
    LogarithmicScale,
    /// "Invert scale".
    InvertScale,
    /// "Lock price to bar ratio".
    LockPriceToBarRatio,
    /// "Place scale on left".
    ScaleOnLeft,
    /// "Place scale on right".
    ScaleOnRight,
    /// "Last price label".
    LastPriceLabel,
    /// "Previous close label".
    PreviousCloseLabel,
    /// "Indicator values".
    IndicatorValues,
    /// "Countdown to bar close".
    Countdown,
    /// "Bid and ask labels".
    BidAndAsk,
    /// "Bid and ask lines".
    BidAndAskLines,
    /// "High and low labels".
    HighAndLow,
    /// "High and low lines".
    HighAndLowLines,
    /// "Scale settings" in the price axis menu.
    ScaleSettings,
    /// "UTC" in the time axis menu.
    Utc,
    /// "Local time".
    LocalTime,
    /// "Exchange time".
    ExchangeTime,
    /// "Extended hours".
    ExtendedHours,
    /// "Overnight hours".
    OvernightHours,
    /// "Session breaks".
    SessionBreaks,
    /// "Time zone and session settings" in the time axis menu.
    TimeSettings,
    /// Restore all session boundaries of a TPO or execution profile chart.
    ResetProfileEdits,
}

type MenuRowsHook = dyn Fn(&HostMenuContext, Vec<MenuRow>) -> Vec<MenuRow>;

/// The host's hook of [`ChartMenus::set_menu_rows`](super::ChartMenus::set_menu_rows).
pub(super) struct MenuRows(pub(super) Box<MenuRowsHook>);

impl std::fmt::Debug for MenuRows {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("MenuRows")
    }
}

/// Expose rows while retaining each built-in item and its current action under its id.
pub(super) fn split<A, B>(
    entries: Vec<MenuEntry<A>>,
    id: &impl Fn(&A) -> Option<MenuRowId>,
    tag: &impl Fn(A) -> B,
    store: &mut Vec<(MenuRowId, MenuItem<B>)>,
) -> Vec<MenuRow> {
    entries
        .into_iter()
        .filter_map(|entry| match entry {
            MenuEntry::Item(item) => {
                let key = id(&item.action)?;
                store.push((key, item.map(tag)));
                Some(MenuRow::BuiltIn(key))
            }
            MenuEntry::Submenu {
                id: key,
                label,
                icon,
                entries,
            } => Some(MenuRow::Submenu {
                id: key,
                label,
                icon,
                rows: split(entries, id, tag, store),
            }),
            MenuEntry::Separator => Some(MenuRow::Separator),
        })
        .collect()
}

/// Resolve built-ins once, tag host actions, then drop empty submenus and surplus separators.
pub(super) fn join<B>(
    rows: Vec<MenuRow>,
    store: &mut Vec<(MenuRowId, MenuItem<B>)>,
    host: &impl Fn(String) -> B,
) -> Vec<MenuEntry<B>> {
    let mut entries = rows
        .into_iter()
        .filter_map(|row| match row {
            MenuRow::BuiltIn(id) => {
                let at = store.iter().position(|(key, _)| *key == id)?;
                Some(MenuEntry::Item(store.remove(at).1))
            }
            MenuRow::Host(item) => Some(MenuEntry::Item(item.map(host))),
            MenuRow::Submenu {
                id,
                label,
                icon,
                rows,
            } => {
                let entries = join(rows, store, host);
                (!entries.is_empty()).then_some(MenuEntry::Submenu {
                    id,
                    label,
                    icon,
                    entries,
                })
            }
            MenuRow::Separator => Some(MenuEntry::Separator),
        })
        .collect();
    trim_separators(&mut entries);
    entries
}
