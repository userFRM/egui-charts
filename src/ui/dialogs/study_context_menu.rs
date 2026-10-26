//! Study context menu - opened from a study's legend row or by a right-click on its plot.
//!
//! Offers an alert on the study's value, adding studies, the study's draw order, interval
//! visibility, pane and scale, copying, hiding and removal, the object tree and the settings.

use crate::chart::series::SeriesId;
use crate::config::KeyboardAction;
use crate::icons::icons as embedded_icons;
use crate::model::{Alert, AlertCondition, AlertSource, Timeframe};
use crate::studies::StudyScale;
use crate::ui::Control;
use crate::ui::context_menu::{MenuEntry, MenuItem, trim_separators};
use crate::widget::{Chart, PaneId, VisualOrder};

use std::collections::HashSet;

/// Actions from the study context menu. [`ChartMenus`](crate::ui::context_menu::ChartMenus)
/// applies the draw order, interval, pane, scale, visibility, removal and settings actions to
/// the chart as one undoable user action before it returns them; the host carries out the
/// others.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum StudyContextMenuAction {
    /// Save this alert, on the study's value at the menu's bar, through the host's alerts
    /// service, for example with
    /// [`AlertsPanel::save`](crate::ui::widget_bar::panels::AlertsPanel::save).
    AddAlert(Box<Alert>),
    /// Open the indicators dialog to add a study.
    AddIndicator,
    /// Add the study's catalogue name to the indicators dialog's favorites.
    AddToFavorites(String),
    /// Change the study's place in the draw order of its pane.
    VisualOrder(VisualOrder),
    /// Show the study only on the intervals of this range around the chart's interval.
    Intervals(IntervalRange),
    /// Move the study to another pane.
    MoveTo(PaneTarget),
    /// Pin the study to a scale.
    PinTo(StudyScale),
    /// Copy the study with [`Chart::copy_indicator`] to the clipboard.
    Copy,
    /// Hide the study, or show it again.
    ToggleVisibility,
    /// Remove the study.
    Remove,
    /// Open the object tree panel. The chart has selected the study.
    ObjectTree,
    /// Open the study's settings, through [`Chart::take_indicator_settings_request`].
    Settings,
}

/// Intervals on which a study is shown, relative to the chart's interval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IntervalRange {
    /// The chart's interval and every longer one.
    AndAbove,
    /// The chart's interval and every shorter one.
    AndBelow,
    /// The chart's interval alone.
    Only,
    /// Every interval.
    All,
}

impl IntervalRange {
    /// The preset intervals and `current` on which the range hides a study, by duration.
    // shortcut: custom intervals other than `current` stay visible, as hidden intervals are an
    // exact list; upgrade when study visibility takes duration ranges.
    pub fn hidden_intervals(self, current: Timeframe) -> Vec<Timeframe> {
        let mut intervals = Timeframe::all();
        if !intervals.contains(&current) {
            intervals.push(current);
        }
        intervals.retain(|interval| match self {
            Self::AndAbove => interval.duration_ms() < current.duration_ms(),
            Self::AndBelow => interval.duration_ms() > current.duration_ms(),
            Self::Only => *interval != current,
            Self::All => false,
        });
        intervals
    }
}

/// Where the study context menu moves a study.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneTarget {
    /// A new pane directly above the study's pane.
    NewPaneAbove,
    /// A new pane directly below the study's pane, or below the main pane.
    NewPaneBelow,
    /// An existing pane, addressed by its stable id.
    Pane(PaneId),
    /// The main pane.
    Main,
}

/// The study context menu of [`ChartMenus`](crate::ui::context_menu::ChartMenus), with the
/// study's state taken when it opened.
#[derive(Default)]
pub(crate) struct StudyContextMenu {
    is_open: bool,
    name: String,
    catalogue_name: String,
    alert: Option<(Alert, String)>,
    visible: bool,
    overlay: bool,
    volume: bool,
    scale: Option<StudyScale>,
    /// The side of the pane's price axis.
    axis_side: &'static str,
    /// Other panes by their stable ids, with their titles.
    panes: Vec<(PaneId, String)>,
    has_interval: bool,
    /// Controls the host hid, set through
    /// [`ChartMenus::set_control_visible`](crate::ui::context_menu::ChartMenus::set_control_visible).
    pub(crate) hidden: HashSet<Control>,
}

impl StudyContextMenu {
    /// Open the menu on study `id` of `chart` at the rendered bar `bar`.
    pub(crate) fn open(&mut self, chart: &Chart, id: SeriesId, bar: usize) {
        let Some((_, study)) = chart.indicators().find(|(study, _)| *study == id) else {
            self.is_open = false;
            return;
        };
        self.is_open = true;
        self.volume = study.is_volume();
        self.name = if self.volume { "Vol" } else { study.name() }.to_owned();
        self.catalogue_name = study.name().to_owned();
        // shortcut: on price-based chart types `bar` counts elements while this reads the
        // source-bar results; upgrade when alerts evaluate element studies.
        self.alert = crate::widget::menu_value(study, bar)
            .filter(|_| !chart.symbol.is_empty())
            .map(|(output, value)| {
                let mut alert = Alert::new(chart.symbol.clone(), AlertCondition::Crossing(value));
                alert.source = if self.volume {
                    AlertSource::Volume
                } else {
                    AlertSource::Study {
                        id: id.0.to_string(),
                        output,
                    }
                };
                (alert, study.format_value(value))
            });
        self.visible = study.is_visible();
        self.overlay = study.is_overlay();
        self.scale = (!(self.overlay && self.volume)).then(|| study.scale());
        self.axis_side = if chart.config().show_right_axis || !chart.config().show_left_axis {
            "right"
        } else {
            "left"
        };
        let pane = chart.series_pane(id);
        self.panes = chart
            .panes()
            .into_iter()
            .filter(|other| *other != PaneId::MAIN && Some(*other) != pane)
            .map(|other| {
                let title = chart.pane_series(other).first().map_or("empty", |member| {
                    chart.indicators().find(|(id, _)| id == member).map_or_else(
                        || {
                            chart
                                .series()
                                .find(|(id, _)| id == member)
                                .map_or("Series", |(_, options)| options.name.as_str())
                        },
                        |(_, study)| study.name(),
                    )
                });
                (other, format!("{title} pane"))
            })
            .collect();
        self.has_interval = chart.timeframe().is_some();
    }

    pub(crate) fn close(&mut self) {
        self.is_open = false;
    }

    pub(crate) fn is_open(&self) -> bool {
        self.is_open
    }

    pub(crate) fn build_menu_items(
        &self,
        shortcuts: &crate::widget::ShortcutMap,
    ) -> Vec<MenuEntry<StudyContextMenuAction>> {
        use StudyContextMenuAction as A;
        if !self.is_open {
            return Vec::new();
        }
        let name = &self.name;
        let item = |label: &str, action| MenuEntry::Item(MenuItem::new(label, action));
        let mut items = Vec::new();
        if let Some((alert, value)) = &self.alert {
            items.push(MenuEntry::Item(
                MenuItem::new(
                    format!("Add alert on {name} at {value}…"),
                    A::AddAlert(Box::new(alert.clone())),
                )
                .with_icon(&embedded_icons::ALERTS)
                .with_shortcut(shortcuts.shortcut(KeyboardAction::CreateAlert)),
            ));
        }
        items.extend([
            MenuEntry::Item(
                MenuItem::new(format!("Add indicator on {name}…"), A::AddIndicator)
                    .with_icon(&embedded_icons::INDICATORS),
            ),
            MenuEntry::Item(
                MenuItem::new(
                    "Add this indicator to favorites",
                    A::AddToFavorites(self.catalogue_name.clone()),
                )
                .with_icon(&embedded_icons::STAR_EMPTY),
            ),
            MenuEntry::Separator,
            MenuEntry::Submenu {
                label: "Visual order".into(),
                icon: Some(&embedded_icons::WIDGET_BAR_LAYERS),
                entries: [
                    ("Bring to front", VisualOrder::BringToFront),
                    ("Bring forward", VisualOrder::BringForward),
                    ("Send backward", VisualOrder::SendBackward),
                    ("Send to back", VisualOrder::SendToBack),
                ]
                .into_iter()
                .map(|(label, order)| item(label, A::VisualOrder(order)))
                .collect(),
            },
        ]);
        if self.has_interval {
            items.push(MenuEntry::Submenu {
                label: "Visibility on intervals".into(),
                icon: None,
                entries: [
                    ("Current interval and above", IntervalRange::AndAbove),
                    ("Current interval and below", IntervalRange::AndBelow),
                    ("Current interval only", IntervalRange::Only),
                    ("All intervals", IntervalRange::All),
                ]
                .into_iter()
                .map(|(label, range)| item(label, A::Intervals(range)))
                .collect(),
            });
        }
        let mut moves = Vec::new();
        moves.push(item("New pane above", A::MoveTo(PaneTarget::NewPaneAbove)));
        moves.push(item("New pane below", A::MoveTo(PaneTarget::NewPaneBelow)));
        for (pane, title) in &self.panes {
            moves.push(item(title, A::MoveTo(PaneTarget::Pane(*pane))));
        }
        if !self.overlay {
            moves.push(item("Main pane", A::MoveTo(PaneTarget::Main)));
        }
        items.push(MenuEntry::Submenu {
            label: "Move to".into(),
            icon: Some(&embedded_icons::MOVE_PANE),
            entries: moves,
        });
        let side = self.axis_side;
        items.push(match self.scale {
            Some(scale) => {
                let now = match scale {
                    StudyScale::Main => side,
                    StudyScale::Independent if self.overlay => "left",
                    StudyScale::Independent => side,
                    StudyScale::None => "no scale",
                };
                let mut entries = vec![item(
                    &format!("Pin to {side} scale"),
                    A::PinTo(if self.overlay {
                        StudyScale::Main
                    } else {
                        StudyScale::Independent
                    }),
                )];
                if self.overlay {
                    entries.push(item(
                        "Pin to new left scale",
                        A::PinTo(StudyScale::Independent),
                    ));
                }
                entries.push(item("No scale", A::PinTo(StudyScale::None)));
                MenuEntry::Submenu {
                    label: format!("Pin to scale (now {now})"),
                    icon: Some(&embedded_icons::PIN_TO_SCALE),
                    entries,
                }
            }
            // A volume overlay keeps its own band at the foot of the main pane.
            None => MenuEntry::Item(
                MenuItem::new("Pin to scale (now no scale)", A::PinTo(StudyScale::None))
                    .with_icon(&embedded_icons::PIN_TO_SCALE)
                    .disabled(),
            ),
        });
        items.extend([
            MenuEntry::Separator,
            MenuEntry::Item(
                MenuItem::new("Copy", A::Copy)
                    .with_shortcut(shortcuts.shortcut(KeyboardAction::CopyDrawing)),
            ),
            MenuEntry::Item(if self.visible {
                MenuItem::new("Hide", A::ToggleVisibility).with_icon(&embedded_icons::EYE_HIDE)
            } else {
                MenuItem::new("Show", A::ToggleVisibility).with_icon(&embedded_icons::EYE)
            }),
            MenuEntry::Item(
                MenuItem::new("Remove", A::Remove)
                    .with_icon(&embedded_icons::TRASH)
                    .with_shortcut(shortcuts.shortcut(KeyboardAction::DeleteSelectedDrawing)),
            ),
            MenuEntry::Separator,
            item("Object tree", A::ObjectTree),
            MenuEntry::Separator,
            MenuEntry::Item(
                MenuItem::new("Settings…", A::Settings).with_icon(&embedded_icons::SETTINGS_GEAR),
            ),
        ]);
        items.retain(|entry| {
            let control = match entry {
                MenuEntry::Item(item) => match item.action {
                    A::AddAlert(_) => Control::Alerts,
                    A::AddIndicator | A::AddToFavorites(_) => Control::Indicators,
                    A::ObjectTree => Control::ObjectTree,
                    A::Settings => Control::Settings,
                    _ => return true,
                },
                MenuEntry::Submenu { .. } | MenuEntry::Separator => return true,
            };
            !self.hidden.contains(&control)
        });
        trim_separators(&mut items);
        items
    }
}

/// Apply a chart-owned action to study `id`. The caller wraps it in a user action.
pub(crate) fn apply(chart: &mut Chart, id: SeriesId, action: &StudyContextMenuAction) {
    use StudyContextMenuAction as A;
    match action {
        A::VisualOrder(order) => {
            chart.reorder_indicator(id, *order);
        }
        A::Intervals(range) => {
            if let Some(current) = chart.timeframe()
                && let Some(study) = chart.indicator_mut(id)
            {
                let mut settings = study.settings();
                settings.hidden_intervals = range.hidden_intervals(current);
                // A study without editable presentation keeps showing on every interval.
                let _ = study.apply_settings(&[], settings);
            }
        }
        A::MoveTo(target) => {
            match *target {
                PaneTarget::NewPaneAbove => chart.move_indicator_to_new_pane(id, true),
                PaneTarget::NewPaneBelow => chart.move_indicator_to_new_pane(id, false),
                PaneTarget::Pane(pane) => chart.move_series_to_pane(id, pane),
                PaneTarget::Main => chart.move_series_to_pane(id, PaneId::MAIN),
            };
        }
        A::PinTo(scale) => {
            chart.set_indicator_scale(id, *scale);
        }
        A::ToggleVisibility => {
            let visible = chart
                .indicators()
                .find(|(study, _)| *study == id)
                .is_some_and(|(_, study)| study.is_visible());
            chart.set_indicator_visible(id, !visible);
        }
        A::Remove => {
            chart.remove_series(id);
        }
        A::Settings => chart.indicator_settings_request = Some(id),
        A::AddAlert(_) | A::AddIndicator | A::AddToFavorites(_) | A::Copy | A::ObjectTree => {}
    }
}
