//! Remove objects dropdown menu.
//!
//! Provides options to permanently remove chart elements:
//! - All Drawings
//! - All Indicators
//! - All drawings and indicators

use crate::icons::icons;
use crate::localization as l10n;
use crate::ui::drawing_toolbar::components::draw_tool_button;
use crate::ui::drawing_toolbar::submenu_builder::SubmenuBuilder;
use crate::ui::drawing_toolbar::{DrawingToolbarAction, ToolbarState};
use crate::ui_kit::buttons::Button;
use crate::ui_kit::dialog::DialogFooter;
use crate::ui_kit::modal::Modal;
use egui::{Response, Ui};

/// Remove objects dropdown menu component.
pub struct RemoveMenu;

impl RemoveMenu {
    /// Render the remove button with trash icon. A click opens the dropdown.
    pub fn show(ui: &mut Ui) -> Response {
        draw_tool_button(ui, &icons::TRASH, "Remove objects", false)
    }

    /// Render removal scopes and the saved locked-drawing choice.
    pub fn show_submenu(ui: &mut Ui, state: &ToolbarState) -> Option<DrawingToolbarAction> {
        let include = state.preferences.remove_locked == Some(true);
        SubmenuBuilder::new(ui)
            .add_text_item_with_action(
                "Remove all drawings",
                "Permanently delete all drawing tools from chart",
                || Some(DrawingToolbarAction::RemoveAllDrawings),
            )
            .danger(&icons::TRASH)
            .add_text_item_with_action(
                "Remove all indicators",
                "Remove all indicators from chart",
                || Some(DrawingToolbarAction::RemoveAllIndicators),
            )
            .danger(&icons::TRASH)
            .add_text_item_with_action(
                "Remove drawings and indicators",
                "Permanently delete all drawing tools and remove all indicators from chart",
                || Some(DrawingToolbarAction::RemoveDrawingsAndIndicators),
            )
            .danger(&icons::TRASH)
            .add_text_item_with_action(
                "Include locked drawings",
                "Choose whether bulk removal also deletes locked drawings",
                move || Some(DrawingToolbarAction::SetLockedDrawingRemoval(!include)),
            )
            .checked(include)
            .show()
    }

    /// Resolve a bulk drawing removal against the saved preference. Call this with the live
    /// lock state before changing either drawings or studies. A pending request changes neither.
    pub fn request(
        action: DrawingToolbarAction,
        has_locked: bool,
        state: &mut ToolbarState,
    ) -> DrawingToolbarAction {
        let include_indicators = match action {
            DrawingToolbarAction::ClearAllDrawings | DrawingToolbarAction::RemoveAllDrawings => {
                false
            }
            DrawingToolbarAction::RemoveDrawingsAndIndicators => true,
            _ => return action,
        };
        if has_locked && state.preferences.remove_locked.is_none() {
            state.pending_removal = Some(action);
            state.remember_removal = false;
            DrawingToolbarAction::None
        } else {
            state.pending_removal = None;
            DrawingToolbarAction::RemoveDrawings {
                include_locked: state.preferences.remove_locked.unwrap_or(false),
                include_indicators,
            }
        }
    }

    /// Show the pending bulk removal choice. Cancel, Escape and closing the dialog discard
    /// the request without editing the chart or the saved preference.
    pub fn show_confirmation(
        ctx: &egui::Context,
        state: &mut ToolbarState,
    ) -> DrawingToolbarAction {
        let Some(request) = state.pending_removal.clone() else {
            return DrawingToolbarAction::None;
        };
        let mut open = true;
        let mut choice = None;
        let modal = Modal::new("locked_drawing_removal", "Remove locked drawings too?")
            .width(480.0)
            .show(ctx, &mut open, |ui| {
                ui.label(l10n::text(
                    ui.ctx(),
                    "Some drawings are locked. Choose which drawings to remove.",
                ));
                ui.checkbox(
                    &mut state.remember_removal,
                    l10n::text(ui.ctx(), "Remember my choice"),
                );
                DialogFooter::new("Keep locked")
                    .secondary("Cancel")
                    .show_with_left(ui, |ui| {
                        if Button::new("Remove including locked")
                            .danger()
                            .show(ui)
                            .clicked()
                        {
                            choice = Some(true);
                        }
                    })
            });
        if let Some(modal) = modal {
            if modal.inner.0 {
                choice = Some(false);
            }
            open &= !modal.inner.1;
        }
        if let Some(include_locked) = choice {
            if state.remember_removal {
                state.preferences.remove_locked = Some(include_locked);
            }
            state.pending_removal = None;
            return DrawingToolbarAction::RemoveDrawings {
                include_locked,
                include_indicators: request == DrawingToolbarAction::RemoveDrawingsAndIndicators,
            };
        }
        if !open {
            state.pending_removal = None;
        }
        DrawingToolbarAction::None
    }
}
