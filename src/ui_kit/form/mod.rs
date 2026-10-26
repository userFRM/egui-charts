//! Form layout components.
//!
//! Provides a two-column layout for label + widget form rows: a fixed-width
//! label column and a left-aligned widget column.
//!
//! # Example
//! ```
//! # use egui_charts::ui_kit::form::FormRow;
//! # egui::__run_test_ui(|ui| {
//! # let mut current = "dark";
//! FormRow::new("Theme")
//!     .description("Color scheme for the application")
//!     .show(ui, |ui| {
//!         egui::ComboBox::from_id_salt("theme")
//!             .selected_text(current)
//!             .show_ui(ui, |ui| {
//!                 for (name, _) in egui_charts::theme::presets() {
//!                     ui.selectable_value(&mut current, name, name);
//!                 }
//!             });
//!     });
//! # });
//! ```

pub mod grid;

use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;
use egui::{Align, Layout, RichText, Ui, vec2};

pub use grid::FormGrid;

/// A two-column form row: fixed-width label area + remainder widget area.
pub struct FormRow<'a> {
    label: &'a str,
    description: Option<&'a str>,
    label_width: f32,
}

impl<'a> FormRow<'a> {
    pub fn new(label: &'a str) -> Self {
        Self {
            label,
            description: None,
            label_width: DESIGN_TOKENS.sizing.settings_dialog.label_width,
        }
    }

    pub fn description(mut self, desc: &'a str) -> Self {
        self.description = Some(desc);
        self
    }

    pub fn label_width(mut self, width: f32) -> Self {
        self.label_width = width;
        self
    }

    /// Show the form row with the given widget content.
    ///
    /// The row is as tall as its content, so consecutive rows stack. The label and the
    /// widget are centred on one line; the description follows below them.
    pub fn show(self, ui: &mut Ui, add_widget: impl FnOnce(&mut Ui)) {
        let spacing = ui.spacing().item_spacing;
        ui.spacing_mut().item_spacing.y = DESIGN_TOKENS.spacing.md;
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = DESIGN_TOKENS.spacing.lg;
            let label = ui.allocate_ui_with_layout(
                vec2(self.label_width, ui.spacing().interact_size.y),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.set_width(self.label_width);
                    ui.label(self.label)
                },
            );
            ui.with_layout(Layout::left_to_right(Align::Center), |ui| {
                ui.label_inputs_by(label.inner.id, add_widget);
            });
        });
        if let Some(desc) = self.description {
            ui.label(
                RichText::new(desc)
                    .size(typography::SM)
                    .color(ui.visuals().weak_text_color()),
            );
        }
        ui.spacing_mut().item_spacing = spacing;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_share_the_grid_control_edge_and_pitch() {
        let ctx = egui::Context::default();
        for frame in 0..3 {
            ctx.run_ui(Default::default(), |ui| {
                let width = 120.0;
                let mut rows = Vec::new();
                for control_width in [40.0, 100.0, 40.0] {
                    FormRow::new("Label").label_width(width).show(ui, |ui| {
                        rows.push(
                            ui.allocate_exact_size(vec2(control_width, 24.0), egui::Sense::hover())
                                .0,
                        );
                    });
                }
                let mut grid = Vec::new();
                FormGrid::new("comparison").show(ui, |ui| {
                    for _ in 0..2 {
                        ui.add_sized(vec2(width, 24.0), egui::Label::new("Label"));
                        grid.push(
                            ui.allocate_exact_size(vec2(40.0, 24.0), egui::Sense::hover())
                                .0,
                        );
                        ui.end_row();
                    }
                });
                if frame < 2 {
                    return;
                }
                assert_eq!(rows[0].left(), rows[1].left());
                assert_eq!(rows[0].left(), grid[0].left());
                assert_eq!(rows[1].top() - rows[0].top(), grid[1].top() - grid[0].top());
            })
            .drop_without_applying_deltas();
        }
    }
}
