//! Form layout components.
//!
//! Provides a two-column layout for label + widget form rows: a fixed-width
//! label column and a right-aligned widget column.
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
use crate::theme::context::ThemeContextExt;
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
        ui.horizontal(|ui| {
            ui.allocate_ui_with_layout(
                vec2(self.label_width, ui.spacing().interact_size.y),
                Layout::left_to_right(Align::Center),
                |ui| {
                    ui.set_width(self.label_width);
                    ui.label(self.label);
                },
            );
            ui.with_layout(Layout::right_to_left(Align::Center), add_widget);
        });
        if let Some(desc) = self.description {
            ui.label(
                RichText::new(desc)
                    .size(typography::SM)
                    .color(ui.ctx().theme_data().ui_text_muted),
            );
        }
        ui.add_space(DESIGN_TOKENS.spacing.sm);
    }
}
