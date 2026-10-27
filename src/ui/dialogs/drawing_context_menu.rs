//! Drawing context menu - right-click menu when drawing is selected.
//!
//! Provides actions for alerts, editing, layering, panes, locking, and deleting drawings.

use crate::config::KeyboardAction;
use crate::icons::icons as embedded_icons;
use crate::model::{Alert, AlertComparison, AlertCondition};
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
pub(crate) fn move_targets(chart: &Chart, drawing_id: usize) -> Vec<(PaneId, String)> {
    let current = chart.drawing_pane(drawing_id);
    let mut panes = crate::ui::dialogs::study_context_menu::other_panes(chart, current);
    if current.is_some_and(|pane| pane != PaneId::MAIN) {
        panes.push((PaneId::MAIN, "Main pane".into()));
    }
    panes
}

/// Actions from the drawing context menu
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum DrawingContextMenuAction {
    /// No action requested.
    None,
    /// Open the alert dialog on this alert, the price crossing the drawing's first level, for
    /// example with [`AlertDialog::edit`](crate::ui::dialogs::AlertDialog::edit),
    /// listing the levels of [`Drawing::alert_levels`](crate::drawings::Drawing::alert_levels)
    /// as its sources.
    AddAlert(Box<Alert>),
    /// Open drawing settings dialog
    Edit,
    /// Bring drawing to front of render order
    BringToFront,
    /// Send drawing to back of render order
    SendToBack,
    /// Move the drawing to this pane, for example with [`Chart::move_drawing_to_pane`].
    MoveTo(PaneId),
    /// Toggle drawing lock state
    Lock,
    /// Toggle drawing visibility
    Hide,
    /// Clone drawing with offset
    Clone,
    /// Delete drawing
    Delete,
}

/// Drawing context menu state
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
    pub(crate) fn open_on(&mut self, chart: &Chart, pos: Pos2, drawing_id: usize) {
        let drawing = chart
            .drawings()
            .drawings
            .iter()
            .find(|d| d.id == drawing_id);
        if let Some(drawing) = drawing {
            self.set_context(drawing.tool_type.as_str(), drawing.locked, drawing.visible);
        }
        self.alert = drawing
            .filter(|drawing| !chart.symbol.is_empty() && !drawing.alert_levels(0.0).is_empty())
            .map(|drawing| {
                Alert::new(
                    chart.symbol.clone(),
                    AlertCondition::Compare {
                        other: drawing.alert_source(0),
                        comparison: AlertComparison::Crossing,
                    },
                )
            });
        self.panes = move_targets(chart, drawing_id);
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

    /// Build menu items for the drawing context menu
    pub(crate) fn build_menu_items(
        &self,
        shortcuts: &crate::widget::ShortcutMap,
    ) -> Vec<MenuEntry<DrawingContextMenuAction>> {
        let alert = self.alert.iter().flat_map(|alert| {
            [
                MenuEntry::Item(
                    MenuItem::new(
                        format!("Add alert on {}…", self.drawing_type),
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
                    "Send to back",
                    DrawingContextMenuAction::SendToBack,
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
            .chain([
                MenuEntry::Separator,
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
        let entries = self.build_menu_items(shortcuts);
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
