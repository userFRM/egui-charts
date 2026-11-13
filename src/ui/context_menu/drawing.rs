//! Drawing context menu - right-click menu when drawing is selected.
//!
//! Provides actions for alerts, editing, layering, panes, locking, and deleting drawings.

use super::study::IntervalRange;
use crate::config::KeyboardAction;
use crate::drawings::TimeframeVisibility;
use crate::icons::icons as embedded_icons;
use crate::localization as l10n;
use crate::model::{Alert, AlertComparison, AlertCondition, Timeframe};
use crate::theme::Theme;
use crate::ui::context_menu::{MenuEntry, MenuItem, show_menu};
use crate::widget::{Chart, PaneId};

use egui::{Context, Id, Pos2};

/// Distance between the pointer and the menu corner, enough to clear the handle under the
/// pointer: half the 8 px handle plus 2 px.
// shortcut: assumes the default handle size, upgrade when the hit result carries the handle rect.
pub(crate) const DRAWING_MENU_OFFSET: f32 = 6.0;

/// The panes of `chart` other than the one drawing `drawing_id`, the price pane last, by their
/// stable ids and titles.
pub(crate) fn move_targets(
    ctx: Option<&Context>,
    chart: &Chart,
    drawing_id: usize,
) -> Vec<(PaneId, String)> {
    let current = chart.drawing_pane(drawing_id);
    let mut panes = super::study::other_panes(ctx, chart, current);
    if current.is_some_and(|pane| pane != PaneId::MAIN) && chart.panes().contains(&PaneId::MAIN) {
        panes.push((
            PaneId::MAIN,
            ctx.map_or_else(
                || "Main pane".to_owned(),
                |ctx| l10n::text(ctx, "Main pane"),
            ),
        ));
    }
    panes
}

/// Actions from the drawing context menu
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum DrawingContextMenuAction {
    /// No action requested.
    None,
    /// Open the alert dialog on this alert, the price crossing the drawing's first visible level, for
    /// example with [`AlertDialog::edit`](crate::ui::dialogs::AlertDialog::edit),
    /// listing the levels of [`Drawing::alert_levels_with_bars`](crate::drawings::Drawing::alert_levels_with_bars)
    /// as its sources.
    AddAlert(Box<Alert>),
    /// Open drawing settings dialog
    Edit,
    /// Bring drawing to front of render order
    BringToFront,
    /// Move drawing above its next neighbour in render order.
    BringForward,
    /// Send drawing to back of render order
    SendToBack,
    /// Move drawing below its previous neighbour in render order.
    SendBackward,
    /// Copy the selected drawings to the clipboard.
    Copy,
    /// Move the drawing to this pane, for example with [`Chart::move_drawing_to_pane`].
    MoveTo(PaneId),
    /// Set the drawing's interval filter. Applied by `ChartMenus` and the floating toolbar
    /// as one undoable chart action; standalone menus call [`Self::apply_interval_visibility`].
    Intervals(IntervalRange),
    /// Toggle drawing lock state
    Lock,
    /// Toggle drawing visibility
    Hide,
    /// Exchange a position's target and stop and switch between long and short.
    /// Applied by the mounted chart menu and floating toolbar through
    /// [`Chart::reverse_position_drawing`]. Standalone hosts call that method themselves.
    ReversePosition,
    /// Clone drawing with offset
    Clone,
    /// Delete drawing
    Delete,
}

/// Drawing context menu state
#[derive(Clone, Debug)]
pub struct DrawingContextMenu {
    is_open: bool,
    position: Pos2,
    /// Drawing ID that was right-clicked
    pub drawing_id: Option<usize>,
    /// Drawing type name for display
    pub drawing_type: String,
    /// Whether the drawing is currently locked
    pub is_locked: bool,
    /// Whether the drawing is currently visible
    pub is_visible: bool,
    /// The alert the Add alert entry offers; `None` leaves the entry out.
    pub alert: Option<Alert>,
    /// Panes the Move to submenu offers, by stable id and title; empty leaves it out.
    pub panes: Vec<(PaneId, String)>,
    /// Current chart interval; `None` disables relative interval commands.
    pub current_interval: Option<Timeframe>,
    /// Saved interval filter used to mark matching quick commands.
    pub timeframe_visibility: TimeframeVisibility,
    /// Last action taken
    pub last_action: DrawingContextMenuAction,
    /// Skip close detection on first frame
    just_opened: bool,
}

impl Default for DrawingContextMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl DrawingContextMenu {
    /// Create new context menu
    pub fn new() -> Self {
        Self {
            is_open: false,
            position: Pos2::ZERO,
            drawing_id: None,
            drawing_type: "Drawing".to_string(),
            is_locked: false,
            is_visible: true,
            alert: None,
            panes: Vec::new(),
            current_interval: None,
            timeframe_visibility: TimeframeVisibility::all(),
            last_action: DrawingContextMenuAction::None,
            just_opened: false,
        }
    }

    /// Open the context menu
    pub fn open(&mut self, pos: Pos2, drawing_id: usize) {
        self.is_open = true;
        self.position = pos;
        self.drawing_id = Some(drawing_id);
        self.last_action = DrawingContextMenuAction::None;
        self.just_opened = true;
    }

    /// Set drawing context for dynamic menu items
    pub fn set_context(&mut self, drawing_type: impl Into<String>, locked: bool, visible: bool) {
        self.drawing_type = drawing_type.into();
        self.is_locked = locked;
        self.is_visible = visible;
    }

    /// Open the menu on drawing `drawing_id` of `chart`, offering an alert when the chart has a
    /// symbol and the drawing has price levels, and the chart's other panes.
    pub(crate) fn open_on(&mut self, ctx: &Context, chart: &Chart, pos: Pos2, drawing_id: usize) {
        let drawing = chart
            .drawings()
            .drawings
            .iter()
            .find(|d| d.id == drawing_id);
        if let Some(drawing) = drawing {
            self.set_context(drawing.tool_type.as_str(), drawing.locked, drawing.visible);
        }
        self.current_interval = chart.timeframe();
        self.timeframe_visibility = drawing
            .map(|d| d.timeframe_visibility.clone())
            .unwrap_or_else(TimeframeVisibility::all);
        self.alert = drawing
            .filter(|_| !chart.symbol.is_empty())
            .and_then(|drawing| {
                let (level, _, _) = drawing
                    .alert_levels_with_bars(
                        chart
                            .bars(crate::chart::series::SeriesId::MAIN)
                            .len()
                            .saturating_sub(1) as f64,
                        chart.config().right_axis_scale_mode,
                        chart.bars(crate::chart::series::SeriesId::MAIN),
                    )
                    .into_iter()
                    .next()?;
                Some(Alert::new(
                    chart.symbol.clone(),
                    AlertCondition::Compare {
                        other: drawing.alert_source(level),
                        comparison: AlertComparison::Crossing,
                    },
                ))
            });
        self.panes = move_targets(Some(ctx), chart, drawing_id);
        self.open(pos, drawing_id);
    }

    /// Close the context menu
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Check if menu is open
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Take and consume the last action
    pub fn take_action(&mut self) -> DrawingContextMenuAction {
        std::mem::replace(&mut self.last_action, DrawingContextMenuAction::None)
    }

    fn interval_items(&self) -> Vec<MenuEntry<DrawingContextMenuAction>> {
        let mut checked = false;
        [
            ("Show on higher intervals", IntervalRange::AndAbove),
            ("Show on lower intervals", IntervalRange::AndBelow),
            ("Show on current interval", IntervalRange::Only),
            ("Show on all intervals", IntervalRange::All),
        ]
        .into_iter()
        .map(|(label, range)| {
            let mut item = MenuItem::new(label, DrawingContextMenuAction::Intervals(range));
            item.enabled = range == IntervalRange::All || self.current_interval.is_some();
            let filter = interval_filter(range, self.current_interval);
            if !checked
                && filter.as_ref().is_some_and(|filter| {
                    let saved = &self.timeframe_visibility;
                    // A single interval can be saved as an exact label or as a one-value range.
                    *filter == *saved
                        || range == IntervalRange::All && saved.show_on_all
                        || range == IntervalRange::Only
                            && !saved.show_on_all
                            && filter
                                .visible_timeframes
                                .iter()
                                .all(|label| saved.is_visible_on(label))
                            && saved
                                .visible_timeframes
                                .iter()
                                .all(|label| filter.is_visible_on(label))
                            && saved.ranges.iter().all(|(unit, range)| {
                                range.is_empty() || filter.range(*unit).as_ref() == Some(range)
                            })
                })
            {
                item.icon = Some(&embedded_icons::CHECK);
                checked = true;
            }
            MenuEntry::Item(item)
        })
        .collect()
    }

    pub(crate) fn position_direction_available(&self) -> bool {
        use crate::drawings::DrawingToolType as T;
        [T::LongPos, T::ShortPos]
            .iter()
            .any(|tool| tool.as_str() == self.drawing_type)
    }

    /// Build menu items for the drawing context menu
    pub(crate) fn build_menu_items(
        &self,
        ctx: &Context,
        shortcuts: &crate::widget::ShortcutMap,
    ) -> Vec<MenuEntry<DrawingContextMenuAction>> {
        let alert = self.alert.iter().flat_map(|alert| {
            [
                MenuEntry::Item(
                    MenuItem::new(
                        l10n::format_text(
                            ctx,
                            "Add alert on {name}…",
                            &[("name", l10n::text(ctx, &self.drawing_type).as_str())],
                        ),
                        DrawingContextMenuAction::AddAlert(Box::new(alert.clone())),
                    )
                    .with_icon(&embedded_icons::ALERTS),
                ),
                MenuEntry::Separator,
            ]
        });
        alert
            .chain([
                // Edit section
                MenuEntry::Item(
                    MenuItem::new("Edit…", DrawingContextMenuAction::Edit)
                        .with_icon(&embedded_icons::PENCIL),
                ),
                MenuEntry::Separator,
                // Layer ordering
                MenuEntry::Item(MenuItem::new(
                    "Bring to front",
                    DrawingContextMenuAction::BringToFront,
                )),
                MenuEntry::Item(MenuItem::new(
                    "Bring forward",
                    DrawingContextMenuAction::BringForward,
                )),
                MenuEntry::Item(MenuItem::new(
                    "Send to back",
                    DrawingContextMenuAction::SendToBack,
                )),
                MenuEntry::Item(MenuItem::new(
                    "Send backward",
                    DrawingContextMenuAction::SendBackward,
                )),
            ])
            .chain((!self.panes.is_empty()).then(|| {
                MenuEntry::Submenu {
                    label: "Move to".into(),
                    icon: Some(&embedded_icons::MOVE_PANE),
                    entries: self
                        .panes
                        .iter()
                        .map(|(pane, title)| {
                            MenuEntry::Item(MenuItem::new(
                                title,
                                DrawingContextMenuAction::MoveTo(*pane),
                            ))
                        })
                        .collect(),
                }
            }))
            .chain(self.position_direction_available().then(|| {
                let mut item = MenuItem::new(
                    "Reverse position",
                    DrawingContextMenuAction::ReversePosition,
                );
                item.enabled = !self.is_locked;
                MenuEntry::Item(item)
            }))
            .chain([
                MenuEntry::Separator,
                MenuEntry::Submenu {
                    label: "Intervals visibility".into(),
                    icon: None,
                    entries: self.interval_items(),
                },
                // State toggles
                MenuEntry::Item(
                    MenuItem::new(
                        if self.is_locked { "Unlock" } else { "Lock" },
                        DrawingContextMenuAction::Lock,
                    )
                    .with_icon(if self.is_locked {
                        &embedded_icons::UNLOCK
                    } else {
                        &embedded_icons::LOCK
                    }),
                ),
                MenuEntry::Item(
                    MenuItem::new(
                        if self.is_visible { "Hide" } else { "Show" },
                        DrawingContextMenuAction::Hide,
                    )
                    .with_icon(if self.is_visible {
                        &embedded_icons::EYE_HIDE
                    } else {
                        &embedded_icons::EYE
                    }),
                ),
                MenuEntry::Separator,
                MenuEntry::Item(
                    MenuItem::new("Copy", DrawingContextMenuAction::Copy)
                        .with_shortcut(shortcuts.shortcut(KeyboardAction::CopyDrawing)),
                ),
                MenuEntry::Item(MenuItem::new("Clone", DrawingContextMenuAction::Clone)),
                MenuEntry::Separator,
                // Delete (last item, potentially destructive)
                MenuEntry::Item(
                    MenuItem::new("Delete", DrawingContextMenuAction::Delete)
                        .with_icon(&embedded_icons::TRASH)
                        .with_shortcut(shortcuts.shortcut(KeyboardAction::DeleteSelectedDrawing))
                        .danger(),
                ),
            ])
            .collect()
    }

    /// Show the context menu
    pub fn show(&mut self, ctx: &Context, theme: &Theme) {
        self.show_with_shortcuts(ctx, theme, &crate::widget::ShortcutMap::default());
    }

    /// Show menu hints from the same bindings used by chart input.
    pub fn show_with_shortcuts(
        &mut self,
        ctx: &Context,
        theme: &Theme,
        shortcuts: &crate::widget::ShortcutMap,
    ) {
        if !self.is_open {
            return;
        }
        let entries = self.build_menu_items(ctx, shortcuts);
        let just_opened = std::mem::take(&mut self.just_opened);
        let id = Id::unique("drawing_context_menu");
        if let Some(action) = show_menu(
            ctx,
            id,
            theme,
            self.position,
            DRAWING_MENU_OFFSET,
            &entries,
            just_opened,
        ) {
            self.last_action = action.unwrap_or(DrawingContextMenuAction::None);
            self.close();
        }
    }
}

fn interval_filter(
    range: IntervalRange,
    current: Option<Timeframe>,
) -> Option<TimeframeVisibility> {
    Some(match range {
        IntervalRange::All => TimeframeVisibility::all(),
        IntervalRange::Only => TimeframeVisibility::current_interval(current?),
        IntervalRange::AndAbove => TimeframeVisibility::higher_intervals(current?),
        IntervalRange::AndBelow => TimeframeVisibility::lower_intervals(current?),
    })
}

impl DrawingContextMenuAction {
    /// Apply an interval command to `id`, recording undo and saved-state updates. Returns
    /// whether this is an interval command; other actions remain the caller's responsibility.
    /// With no chart interval, only the all-intervals command changes the filter.
    pub fn apply_interval_visibility(&self, chart: &mut Chart, id: usize) -> bool {
        let Self::Intervals(range) = self else {
            return false;
        };
        if let Some(filter) = interval_filter(*range, chart.timeframe())
            && chart
                .drawings()
                .drawings
                .iter()
                .any(|d| d.id == id && d.timeframe_visibility != filter)
        {
            chart.user_action(|chart| {
                chart
                    .drawings_mut()
                    .edit_drawing(id, |d| d.timeframe_visibility = filter)
            });
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::drawings::{ChartPoint, Drawing, DrawingManager, DrawingToolType, FibonacciConfig};
    use crate::model::{AlertSource, BarData};

    /// The initial alert follows the first visible level, including when level zero is hidden.
    #[test]
    fn alerts_start_on_a_visible_drawing_level() {
        let mut chart = Chart::new(BarData::default());
        chart.symbol = "SAMPLE".into();
        let mut drawing = Drawing::new(7, DrawingToolType::FibonacciRetracement);
        drawing.chart_points = vec![ChartPoint::new(0.0, 100.0), ChartPoint::new(10.0, 200.0)];
        drawing.stored_id = Some("fib".into());
        chart.drawings_mut().drawings.push(drawing);
        let mut menu = DrawingContextMenu::new();
        for visible in [vec![0, 1], vec![1], vec![]] {
            let mut config = FibonacciConfig::default();
            for (index, level) in config.levels.iter_mut().enumerate() {
                level.visible = visible.contains(&index);
            }
            chart.drawings_mut().drawings[0].fib_config = Some(config);
            menu.open_on(&Context::default(), &chart, Pos2::ZERO, 7);
            assert_eq!(
                menu.alert.as_ref().map(|alert| &alert.condition),
                visible
                    .first()
                    .map(|level| AlertCondition::Compare {
                        other: AlertSource::Drawing(if *level == 0 {
                            "fib".into()
                        } else {
                            "fib#1".into()
                        }),
                        comparison: AlertComparison::Crossing,
                    })
                    .as_ref()
            );
        }
    }

    #[test]
    fn quick_interval_menu_is_reachable() {
        let mut menu = DrawingContextMenu::new();
        let ctx = Context::default();
        let mut chart = Chart::new(crate::model::BarData::default());
        chart.set_timeframe(Timeframe::Hour1);
        let mut drawing = Drawing::new(7, DrawingToolType::TrendLine);
        drawing.chart_points = vec![
            crate::drawings::ChartPoint::new(0.0, 100.0),
            crate::drawings::ChartPoint::new(1.0, 110.0),
        ];
        drawing.completed = true;
        chart.drawings_mut().drawings.push(drawing);
        let mut mounted = crate::ui::context_menu::ChartMenus::default();
        let frame =
            |chart: &mut Chart, mounted: &mut crate::ui::context_menu::ChartMenus, events| {
                let mut output = ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            Pos2::ZERO,
                            egui::vec2(1440.0, 900.0),
                        )),
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        mounted.show(ui.ctx(), chart);
                    },
                );
                let shapes = std::mem::take(&mut output.shapes);
                output.drop_without_applying_deltas();
                shapes
            };
        let text_position = |shapes: &[egui::epaint::ClippedShape], label: &str| {
            shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) if text.galley.text() == label => {
                        Some(text.pos + text.galley.size() * 0.5)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("missing menu row: {label}"))
        };
        for (range, shown) in [
            (IntervalRange::Only, [false, true, false]),
            (IntervalRange::AndAbove, [false, true, true]),
            (IntervalRange::AndBelow, [true, true, false]),
            (IntervalRange::All, [true, true, true]),
        ] {
            let before = chart.drawings().drawings[0].timeframe_visibility.clone();
            chart.right_click = Some(crate::widget::RightClickTarget::Drawing {
                drawing_id: 7,
                pos: Pos2::new(100.0, 100.0),
            });
            frame(&mut chart, &mut mounted, vec![]);
            let shapes = frame(&mut chart, &mut mounted, vec![]);
            let pos = text_position(&shapes, "Intervals visibility");
            frame(
                &mut chart,
                &mut mounted,
                vec![egui::Event::PointerMoved(pos)],
            );
            let shapes = frame(&mut chart, &mut mounted, vec![]);
            let label = match range {
                IntervalRange::Only => "Show on current interval",
                IntervalRange::AndAbove => "Show on higher intervals",
                IntervalRange::AndBelow => "Show on lower intervals",
                IntervalRange::All => "Show on all intervals",
            };
            let pos = text_position(&shapes, label);
            frame(
                &mut chart,
                &mut mounted,
                vec![egui::Event::PointerMoved(pos)],
            );
            for pressed in [true, false] {
                frame(
                    &mut chart,
                    &mut mounted,
                    vec![egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    }],
                );
            }
            let saved = chart.drawings().drawings[0].clone();
            assert!(chart.undo());
            assert_eq!(chart.drawings().drawings[0].timeframe_visibility, before);
            assert!(chart.redo());
            assert_eq!(
                chart.drawings().drawings[0].timeframe_visibility,
                saved.timeframe_visibility
            );
            let template = crate::drawings::DrawingTemplate::from_drawing(&saved);
            let template: crate::drawings::DrawingTemplate =
                serde_json::from_str(&serde_json::to_string(&template).unwrap()).unwrap();
            let mut restored = Drawing::new(8, DrawingToolType::TrendLine);
            template.apply_to(&mut restored).unwrap();
            assert_eq!(restored.timeframe_visibility, saved.timeframe_visibility);
            if range == IntervalRange::Only {
                // Editing the tab back to the same single interval keeps the current checkmark.
                chart.drawings_mut().drawings[0]
                    .timeframe_visibility
                    .set_range(crate::model::TimeframeUnit::Hour, Some(1..=1));
            }
            for (interval, shown) in [Timeframe::Min30, Timeframe::Hour1, Timeframe::Hour4]
                .into_iter()
                .zip(shown)
            {
                chart.set_timeframe(interval);
                assert_eq!(saved.is_visible_on_timeframe(&interval.as_str()), shown);
                menu.open_on(&ctx, &chart, Pos2::ZERO, 7);
                for entry in menu.interval_items() {
                    if let MenuEntry::Item(item) = entry {
                        let checked = item.icon.is_some();
                        assert_eq!(
                            checked,
                            item.action == DrawingContextMenuAction::Intervals(range)
                                && (interval == Timeframe::Hour1 || range == IntervalRange::All)
                        );
                    }
                }
            }
            chart.set_timeframe(Timeframe::Hour1);
        }
        // At a unit's boundary, a relative preset can cover the current interval alone.
        for (count, range) in [
            (1, IntervalRange::AndBelow),
            (u64::MAX, IntervalRange::AndAbove),
        ] {
            let current = Timeframe::Ticks(std::num::NonZeroU64::new(count).unwrap());
            chart.set_timeframe(current);
            chart.drawings_mut().drawings[0].timeframe_visibility =
                interval_filter(range, Some(current)).unwrap();
            menu.open_on(&ctx, &chart, Pos2::ZERO, 7);
            let checked: Vec<_> = menu
                .interval_items()
                .into_iter()
                .filter_map(|entry| match entry {
                    MenuEntry::Item(item) if item.icon.is_some() => Some(item.action),
                    _ => None,
                })
                .collect();
            assert_eq!(checked, vec![DrawingContextMenuAction::Intervals(range)]);
        }
    }

    #[test]
    fn locked_interval_edits_keep_geometry_and_undo() {
        for bulk in [false, true] {
            let mut manager = DrawingManager::new();
            let mut drawing = Drawing::new(7, DrawingToolType::TrendLine);
            drawing.locked = true;
            drawing.chart_points = vec![
                crate::drawings::ChartPoint::new(0.0, 100.0),
                crate::drawings::ChartPoint::new(1.0, 110.0),
            ];
            manager.drawings.push(drawing.clone());
            let filter = TimeframeVisibility::specific(&["4h"]);
            let edit = |d: &mut Drawing| {
                d.timeframe_visibility = filter.clone();
                d.stroke_width = 9.0;
                d.chart_points[0].price = 999.0;
            };
            if bulk {
                manager.edit_drawings(&[7], edit);
            } else {
                manager.edit_drawing(7, edit);
            }
            assert_eq!(manager.drawings[0].timeframe_visibility, filter);
            assert_eq!(manager.drawings[0].stroke_width, drawing.stroke_width);
            assert_eq!(manager.drawings[0].chart_points, drawing.chart_points);
            assert!(manager.undo());
            assert_eq!(
                manager.drawings[0].timeframe_visibility,
                drawing.timeframe_visibility
            );
            assert!(manager.redo());
            assert_eq!(manager.drawings[0].timeframe_visibility, filter);
        }
    }
}
