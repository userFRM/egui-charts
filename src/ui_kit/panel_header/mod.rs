//! Panel header component — heading + right-aligned actions + separator.
//!
//! Extracts the universal panel header boilerplate:
//! ```ignore
//! PanelHeader::new("Watchlist").show(ui, |ui| {
//!     if ui.button("+").on_hover_text("Add Symbol").clicked() { /* ... */ }
//! });
//! ```

use egui::{Align, Layout, Response, RichText, Ui};

use crate::styles::typography;

/// Panel header with title and optional right-aligned actions.
pub struct PanelHeader<'a> {
    title: &'a str,
    use_heading: bool,
    separator: bool,
}

impl<'a> PanelHeader<'a> {
    /// Create a panel header with the given title.
    ///
    /// Defaults to `ui.heading()` style with a separator below.
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            use_heading: true,
            separator: true,
        }
    }

    /// Use `ui.strong()` style instead of `ui.heading()`.
    #[must_use]
    pub fn strong(mut self) -> Self {
        self.use_heading = false;
        self
    }

    /// Disable the separator below the header.
    #[must_use]
    pub fn no_separator(mut self) -> Self {
        self.separator = false;
        self
    }

    /// Show the panel header. The closure receives a right-to-left layout
    /// for action buttons.
    pub fn show(self, ui: &mut Ui, actions: impl FnOnce(&mut Ui)) -> Response {
        let response = ui
            .horizontal(|ui| {
                if self.use_heading {
                    ui.heading(self.title);
                } else {
                    ui.label(RichText::new(self.title).strong().size(typography::LG));
                }

                ui.with_layout(Layout::right_to_left(Align::Center), actions);
            })
            .response;

        if self.separator {
            ui.separator();
        }

        response
    }

    /// Show the panel header with no action buttons.
    pub fn show_plain(self, ui: &mut Ui) -> Response {
        self.show(ui, |_| {})
    }
}
