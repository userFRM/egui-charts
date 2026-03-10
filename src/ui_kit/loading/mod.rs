//! Loading indicator component — spinner with optional message.
//!
//! ```ignore
//! LoadingIndicator::new().message("Loading templates...").show(ui);
//! ```

use egui::{RichText, Ui};

/// A loading spinner with optional message text.
///
/// Renders a horizontal layout with spinner + label, vertically padded.
pub struct LoadingIndicator<'a> {
    message: Option<&'a str>,
}

impl<'a> LoadingIndicator<'a> {
    pub fn new() -> Self {
        Self { message: None }
    }

    /// Set the loading message (default: "Loading...").
    #[must_use]
    pub fn message(mut self, msg: &'a str) -> Self {
        self.message = Some(msg);
        self
    }

    /// Show the loading indicator.
    pub fn show(self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            ui.spinner();
            ui.label(
                RichText::new(self.message.unwrap_or("Loading..."))
                    .color(ui.visuals().weak_text_color()),
            );
        });
    }
}

impl Default for LoadingIndicator<'_> {
    fn default() -> Self {
        Self::new()
    }
}
