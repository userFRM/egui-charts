//! Empty state component — centered message for lists/panels with no content.
//!
//! ```ignore
//! EmptyState::new("No scan results")
//!     .description("Add filters and click Scan to search")
//!     .show(ui);
//! ```

use egui::{RichText, Ui};

use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;

/// A centered empty-state placeholder.
///
/// Renders a vertically-centered heading with optional description,
/// replacing the repeated `vertical_centered + label + weak_text` boilerplate.
pub struct EmptyState<'a> {
    message: &'a str,
    description: Option<&'a str>,
}

impl<'a> EmptyState<'a> {
    /// Create an empty state with the given message.
    pub fn new(message: &'a str) -> Self {
        Self {
            message,
            description: None,
        }
    }

    /// Add a secondary description line.
    #[must_use]
    pub fn description(mut self, desc: &'a str) -> Self {
        self.description = Some(desc);
        self
    }

    /// Show the empty state, vertically centered.
    pub fn show(self, ui: &mut Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(DESIGN_TOKENS.spacing.xl);
            ui.label(
                RichText::new(self.message)
                    .size(typography::MD)
                    .color(ui.visuals().weak_text_color()),
            );
            if let Some(desc) = self.description {
                ui.add_space(DESIGN_TOKENS.spacing.sm);
                ui.label(
                    RichText::new(desc)
                        .size(typography::SM)
                        .color(ui.visuals().weak_text_color()),
                );
            }
            ui.add_space(DESIGN_TOKENS.spacing.xl);
        });
    }
}
