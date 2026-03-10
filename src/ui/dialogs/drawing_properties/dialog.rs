//! Drawing properties dialog - modal popup for editing drawing properties.

use egui::{Context, Ui, Vec2};

use super::actions::{DrawingId, DrawingPropertiesAction, DrawingProps};
use super::config::DrawingPropertiesConfig;
use super::tabs::{
    CoordinatesState, CoordinatesTab, PropertiesTab, StyleTab, TextTab, VisibilityTab,
};
use crate::ext::UiExt;
use crate::ui_kit::dialog::{DialogFrame, dialog_header};
use crate::ui_kit::tab_bar::TabBar;

/// Drawing properties dialog
pub struct DrawingPropertiesDialog {
    pub config: DrawingPropertiesConfig,
    pub is_open: bool,
    pub drawing_id: Option<DrawingId>,
    pub drawing_name: String,
    pub active_tab: PropertiesTab,
    pub props: DrawingProps,
    pub coordinates: CoordinatesState,
}

impl Default for DrawingPropertiesDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl DrawingPropertiesDialog {
    pub fn new() -> Self {
        Self {
            config: DrawingPropertiesConfig::default(),
            is_open: false,
            drawing_id: None,
            drawing_name: String::new(),
            active_tab: PropertiesTab::Style,
            props: DrawingProps::default(),
            coordinates: CoordinatesState::default(),
        }
    }

    /// Open the dialog for a specific drawing
    pub fn open(&mut self, id: DrawingId, name: String, props: DrawingProps) {
        self.is_open = true;
        self.drawing_id = Some(id);
        self.drawing_name = name;
        self.props = props;
        self.active_tab = PropertiesTab::Style;
    }

    /// Close the dialog
    pub fn close(&mut self) {
        self.is_open = false;
        self.drawing_id = None;
    }

    /// Sync dialog open/close state from an external flag.
    ///
    /// Opening requires drawing context, so only close is synced here.
    pub fn sync_open_state(&mut self, should_be_open: bool) {
        if !should_be_open && self.is_open {
            self.close();
        }
    }

    /// Show the dialog
    pub fn show(&mut self, ctx: &Context) -> DrawingPropertiesAction {
        if !self.is_open {
            return DrawingPropertiesAction::None;
        }

        let mut action = DrawingPropertiesAction::None;

        DialogFrame::new(
            "Drawing Properties",
            Vec2::new(self.config.width, self.config.height),
        )
        .show(ctx, |ui| {
            action = self.render_contents(ui);
        });

        action
    }

    fn render_contents(&mut self, ui: &mut Ui) -> DrawingPropertiesAction {
        let mut action = DrawingPropertiesAction::None;

        // Title bar
        if dialog_header(ui, &self.drawing_name) {
            action = DrawingPropertiesAction::Cancel;
            self.is_open = false;
        }
        ui.separator();

        // Tabs
        ui.space_sm();
        TabBar::new(&PropertiesTab::all(), &mut self.active_tab).show(ui);
        ui.space_sm();
        ui.separator();

        // Content area
        egui::ScrollArea::vertical()
            .max_height(self.config.height - 180.0)
            .show(ui, |ui| match self.active_tab {
                PropertiesTab::Style => StyleTab::show(ui, &mut self.props),
                PropertiesTab::Coordinates => CoordinatesTab::show(ui, &mut self.coordinates),
                PropertiesTab::Visibility => VisibilityTab::show(ui, &mut self.props),
                PropertiesTab::Text => TextTab::show(ui, &mut self.props),
            });

        // Footer with buttons
        ui.with_layout(egui::Layout::bottom_up(egui::Align::LEFT), |ui| {
            ui.space_lg();
            ui.horizontal(|ui| {
                ui.space_lg();

                // Delete button
                if ui.danger_button("Delete").clicked()
                    && let Some(id) = self.drawing_id
                {
                    action = DrawingPropertiesAction::Delete(id);
                    self.is_open = false;
                }

                // Clone button
                if ui.button("Clone").clicked()
                    && let Some(id) = self.drawing_id
                {
                    action = DrawingPropertiesAction::Clone(id);
                }

                ui.right_aligned(|ui| {
                    ui.space_lg();

                    // Apply button (primary)
                    if ui.primary_button("Apply").clicked()
                        && let Some(id) = self.drawing_id
                    {
                        action = DrawingPropertiesAction::Apply(id, self.props.clone());
                        self.is_open = false;
                    }

                    // Cancel button
                    if ui.button("Cancel").clicked() {
                        action = DrawingPropertiesAction::Cancel;
                        self.is_open = false;
                    }
                });
            });
            ui.separator();
        });

        action
    }
}
