//! Empty state component: a centred message for lists and panels with no content.
//!
//! ```
//! # use egui_charts::ui_kit::EmptyState;
//! # egui::__run_test_ui(|ui| {
//! let create = EmptyState::new("No alerts yet")
//!     .icon(&egui_charts::icons::icons::ALERTS)
//!     .description("Create one from the toolbar or the chart menu.")
//!     .action("Create alert")
//!     .fill_height()
//!     .show(ui);
//! # let _ = create;
//! # });
//! ```

use egui::{Frame, Margin, RichText, Ui, Vec2};

use crate::icons::Icon;
use crate::theme::ThemeExt;
use crate::ui_kit::buttons::Button;

/// Size of the optional icon above the message.
const ICON_SIZE: f32 = 32.0;

/// A horizontally centred empty-state placeholder: optional icon, message, optional
/// description and optional primary action.
pub struct EmptyState<'a> {
    message: &'a str,
    description: Option<&'a str>,
    icon: Option<&'static Icon>,
    action: Option<&'a str>,
    fill_height: bool,
}

impl<'a> EmptyState<'a> {
    /// Create an empty state with the given message.
    pub fn new(message: &'a str) -> Self {
        Self {
            message,
            description: None,
            icon: None,
            action: None,
            fill_height: false,
        }
    }

    /// Add a secondary description line.
    #[must_use]
    pub fn description(mut self, desc: &'a str) -> Self {
        self.description = Some(desc);
        self
    }

    /// Show an icon above the message.
    #[must_use]
    pub fn icon(mut self, icon: &'static Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Add a primary button with this label below the text; [`Self::show`] reports its clicks.
    #[must_use]
    pub fn action(mut self, label: &'a str) -> Self {
        self.action = Some(label);
        self
    }

    /// Centre the empty state vertically in the rest of the `Ui`, for a panel or list that
    /// shows nothing else. Without it, the empty state takes only its own height.
    #[must_use]
    pub fn fill_height(mut self) -> Self {
        self.fill_height = true;
        self
    }

    /// Show the empty state. Returns `true` when the action button was clicked.
    pub fn show(self, ui: &mut Ui) -> bool {
        let theme = ui.active_theme();
        // The height of the last pass, to centre this one.
        let height_id = ui.next_auto_id().with("empty_state_height");
        let last_height = if self.fill_height {
            ui.data(|d| d.get_temp::<f32>(height_id))
        } else {
            None
        };
        if let Some(height) = last_height {
            ui.add_space(((ui.available_height() - height) / 2.0).max(0.0));
        }

        let shown = Frame::new()
            .inner_margin(Margin::symmetric(0, theme.spacing.xl as i8))
            .show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    ui.spacing_mut().item_spacing.y = theme.spacing.lg;
                    let muted = ui.visuals().weak_text_color();
                    if let Some(icon) = self.icon {
                        ui.add(icon.as_image_tinted(Vec2::splat(ICON_SIZE), muted));
                    }
                    ui.label(RichText::new(self.message).size(theme.text.body));
                    if let Some(desc) = self.description {
                        ui.label(RichText::new(desc).size(theme.text.small).color(muted));
                    }
                    self.action
                        .is_some_and(|label| Button::new(label).primary().show(ui).clicked())
                })
                .inner
            });

        let height = shown.response.rect.height();
        if self.fill_height && last_height != Some(height) {
            ui.data_mut(|d| d.insert_temp(height_id, height));
            ui.ctx().request_discard("EmptyState measured its height");
        }
        shown.inner
    }
}
