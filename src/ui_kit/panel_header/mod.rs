//! Panel header component: title, right-aligned actions and a separator.
//!
//! Extracts the universal panel header boilerplate:
//! ```
//! # use egui_charts::ui_kit::PanelHeader;
//! # egui::__run_test_ui(|ui| {
//! PanelHeader::new("Watchlist").show(ui, |ui| {
//!     if ui.button("+").on_hover_text("Add Symbol").clicked() { /* ... */ }
//! });
//! # });
//! ```

use egui::{Align, Layout, Response, RichText, Ui, vec2};

use crate::theme::ThemeExt;

/// Height of the header row, which fits a medium `IconButton`.
const ROW_HEIGHT: f32 = 28.0;

/// Panel header with title and optional right-aligned actions.
///
/// The title is in the theme's title size and strong text colour; actions sit 4 px apart.
pub struct PanelHeader<'a> {
    title: &'a str,
    separator: bool,
}

impl<'a> PanelHeader<'a> {
    /// Create a panel header with the given title and a separator below.
    pub fn new(title: &'a str) -> Self {
        Self {
            title,
            separator: true,
        }
    }

    /// Has no effect: every header uses the same title style. Kept for source compatibility.
    #[must_use]
    pub fn strong(self) -> Self {
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
        let theme = ui.active_theme();
        let size = vec2(ui.available_width(), ROW_HEIGHT + 2.0 * theme.spacing.sm);
        let response = ui
            .allocate_ui_with_layout(size, Layout::left_to_right(Align::Center), |ui| {
                ui.set_min_height(size.y);
                ui.label(RichText::new(self.title).strong().size(theme.text.title));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = theme.spacing.sm;
                    actions(ui);
                });
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
