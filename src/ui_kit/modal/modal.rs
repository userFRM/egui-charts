//! Modal Widget
//!
//! A modal dialog in the dialog chrome, centred on the screen above a dimmed backdrop.

use egui::{Color32, Context, Id, Ui};

use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::dialog::{dialog_frame, title_row};

/// A modal dialog widget
pub struct Modal {
    id: Id,
    title: String,
    width: Option<f32>,
    height: Option<f32>,
    min_height: Option<f32>,
    closable: bool,
    dim_background: bool,
    rounding: Option<f32>,
}

impl Modal {
    /// Create a new modal with the given title. An empty title leaves the header to the content.
    pub fn new(id: impl std::hash::Hash + std::fmt::Debug, title: impl Into<String>) -> Self {
        Self {
            id: Id::unique(id),
            title: title.into(),
            width: None,
            height: None,
            min_height: None,
            closable: true,
            dim_background: true,
            rounding: None,
        }
    }

    /// Set the outer width of the modal, including its 16 px padding
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Set the outer height of the modal, including its 16 px padding, for content that fills
    /// the modal, such as a list in a scroll area. The modal keeps 24 px free above and below it
    /// in the window and follows the window when it shrinks. Overrides [`Self::min_height`].
    pub fn height(mut self, height: f32) -> Self {
        self.height = Some(height);
        self
    }

    /// Set the minimum outer height of the modal, including its 16 px padding
    pub fn min_height(mut self, height: f32) -> Self {
        self.min_height = Some(height);
        self
    }

    /// Set whether the modal can be closed with the close button, Escape or a backdrop click
    pub fn closable(mut self, closable: bool) -> Self {
        self.closable = closable;
        self
    }

    /// Override the corner radius, leaving the theme's radius as the default.
    pub fn rounding(mut self, rounding: f32) -> Self {
        self.rounding = Some(rounding);
        self
    }

    /// Set whether to dim the background
    pub fn dim_background(mut self, dim: bool) -> Self {
        self.dim_background = dim;
        self
    }

    /// Show the modal
    ///
    /// Returns `Some(R)` if the modal was shown, where R is the result of the content function.
    /// The `open` parameter controls whether the modal is visible and can be set to `false`
    /// to close the modal.
    pub fn show<R>(
        self,
        ctx: &Context,
        open: &mut bool,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> Option<ModalResponse<R>> {
        if !*open {
            return None;
        }

        let mut frame = dialog_frame(&ctx.global_style());
        if let Some(rounding) = self.rounding {
            frame = frame.corner_radius(rounding);
        }
        let screen_height = ctx.content_rect().height() - 2.0 * DESIGN_TOKENS.spacing.xxxl;
        let height = self.height.map(|height| height.min(screen_height));
        let size = egui::vec2(
            self.width
                .unwrap_or(DESIGN_TOKENS.sizing.dialog.default_width),
            height
                .or(self.min_height)
                .unwrap_or(DESIGN_TOKENS.sizing.dialog.default_min_height),
        ) - frame.total_margin().sum();
        let backdrop = if self.dim_background {
            DESIGN_TOKENS.semantic.modal.overlay_bg
        } else {
            Color32::TRANSPARENT
        };

        let mut close_clicked = false;
        let response = egui::Modal::new(self.id)
            .frame(frame)
            .backdrop_color(backdrop)
            .show(ctx, |ui| {
                ui.set_width(size.x);
                ui.set_min_height(size.y);
                if height.is_some() {
                    // The area offers its last size, which filling content would otherwise keep.
                    ui.set_max_height(size.y);
                }
                if !self.title.is_empty() {
                    close_clicked = title_row(ui, &self.title, self.closable);
                }
                add_contents(ui)
            });

        // `should_close` covers Escape and a click on the backdrop.
        let close_requested = self.closable && (close_clicked || response.should_close());
        if close_requested {
            *open = false;
        }

        Some(ModalResponse {
            inner: response.inner,
            close_requested,
        })
    }
}

/// Response from showing a Modal
pub struct ModalResponse<R> {
    /// The result of the content function
    pub inner: R,
    /// Whether the modal was requested to close
    pub close_requested: bool,
}

/// A simple modal that manages its own open state
pub struct SimpleModal {
    modal: Modal,
    open: bool,
}

impl SimpleModal {
    /// Create a new simple modal
    pub fn new(id: impl std::hash::Hash + std::fmt::Debug, title: impl Into<String>) -> Self {
        Self {
            modal: Modal::new(id, title),
            open: false,
        }
    }

    /// Set the width
    pub fn width(mut self, width: f32) -> Self {
        self.modal = self.modal.width(width);
        self
    }

    /// Set closable
    pub fn closable(mut self, closable: bool) -> Self {
        self.modal = self.modal.closable(closable);
        self
    }

    /// Open the modal
    pub fn open(&mut self) {
        self.open = true;
    }

    /// Close the modal
    pub fn close(&mut self) {
        self.open = false;
    }

    /// Check if the modal is open
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Show the modal
    pub fn show<R>(
        &mut self,
        ctx: &Context,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> Option<ModalResponse<R>> {
        // Clone modal config to avoid borrowing issues
        let modal = Modal::new(self.modal.id, &self.modal.title)
            .width(
                self.modal
                    .width
                    .unwrap_or(DESIGN_TOKENS.sizing.dialog.default_width),
            )
            .closable(self.modal.closable)
            .dim_background(self.modal.dim_background);

        modal.show(ctx, &mut self.open, add_contents)
    }
}

/// Helper function to show a centered modal with standard styling
pub fn show_modal<R>(
    ctx: &Context,
    id: impl std::hash::Hash + std::fmt::Debug,
    title: impl Into<String>,
    open: &mut bool,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> Option<ModalResponse<R>> {
    Modal::new(id, title).show(ctx, open, add_contents)
}
