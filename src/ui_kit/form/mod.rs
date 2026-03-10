//! Form layout components using `egui_extras::StripBuilder`.
//!
//! Provides a proportional two-column layout for label + widget form rows,
//! replacing manual `set_width` + `right_to_left` patterns.
//!
//! # Example
//! ```ignore
//! FormRow::new("Theme")
//!     .description("Color scheme for the application")
//!     .show(ui, |ui| {
//!         ComboBox::from_id_salt("theme")
//!             .selected_text(current.display_name())
//!             .show_ui(ui, |ui| { ... });
//!     });
//! ```

pub mod grid;

use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;
use egui::{RichText, Ui};
use egui_extras::{Size, StripBuilder};

pub use grid::FormGrid;

/// A two-column form row: fixed-width label area + remainder widget area.
///
/// Uses `StripBuilder` for clean proportional column sizing.
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
    pub fn show(self, ui: &mut Ui, add_widget: impl FnOnce(&mut Ui)) {
        StripBuilder::new(ui)
            .size(Size::exact(self.label_width))
            .size(Size::remainder())
            .horizontal(|mut strip| {
                // Label column
                strip.cell(|ui| {
                    ui.vertical(|ui| {
                        ui.label(self.label);
                        if let Some(desc) = self.description {
                            ui.label(
                                RichText::new(desc)
                                    .size(typography::SM)
                                    .color(DESIGN_TOKENS.semantic.ui.text_muted_dark),
                            );
                        }
                    });
                });

                // Widget column (right-aligned)
                strip.cell(|ui| {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        add_widget(ui);
                    });
                });
            });
        ui.add_space(DESIGN_TOKENS.spacing.sm);
    }
}
