//! Modal Widget
//!
//! A modal dialog that displays content centered on the screen with a dimmed backdrop.

use egui::{Color32, Context, Id, Ui, Vec2};

use crate::icons::icons;
use crate::tokens::DESIGN_TOKENS;

/// A modal dialog widget
pub struct Modal {
    id: Id,
    title: String,
    width: Option<f32>,
    min_height: Option<f32>,
    closable: bool,
    dim_background: bool,
}

impl Modal {
    /// Create a new modal with the given title
    pub fn new(id: impl std::hash::Hash, title: impl Into<String>) -> Self {
        Self {
            id: Id::new(id),
            title: title.into(),
            width: None,
            min_height: None,
            closable: true,
            dim_background: true,
        }
    }

    /// Set the width of the modal
    pub fn width(mut self, width: f32) -> Self {
        self.width = Some(width);
        self
    }

    /// Set the minimum height of the modal
    pub fn min_height(mut self, height: f32) -> Self {
        self.min_height = Some(height);
        self
    }

    /// Set whether the modal can be closed with the X button
    pub fn closable(mut self, closable: bool) -> Self {
        self.closable = closable;
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

        let mut close_requested = false;
        let mut inner_response = None;

        egui::Area::new(self.id)
            .fixed_pos(egui::pos2(0.0, 0.0))
            .order(egui::Order::Foreground)
            .show(ctx, |ui| {
                let screen_rect = ctx.content_rect();

                // Dim background
                if self.dim_background {
                    let dim_color = Color32::from_black_alpha(180);
                    ui.painter().rect_filled(screen_rect, 0.0, dim_color);
                }

                // Backdrop click to close
                let backdrop_response = ui.allocate_rect(screen_rect, egui::Sense::click());
                if backdrop_response.clicked() && self.closable {
                    close_requested = true;
                }

                // Handle Escape key
                if ui.input(|i| i.key_pressed(egui::Key::Escape)) && self.closable {
                    close_requested = true;
                }

                // Modal dimensions
                let modal_width = self
                    .width
                    .unwrap_or(DESIGN_TOKENS.sizing.dialog.default_width);
                let modal_min_height = self
                    .min_height
                    .unwrap_or(DESIGN_TOKENS.sizing.dialog.default_min_height);

                // Center the modal
                let modal_pos =
                    screen_rect.center() - egui::vec2(modal_width / 2.0, modal_min_height / 2.0);

                // Modal frame
                egui::Window::new(&self.title)
                    .id(self.id.with("_window"))
                    .fixed_pos(modal_pos)
                    .fixed_size(egui::vec2(modal_width, 0.0)) // Height auto
                    .collapsible(false)
                    .resizable(false)
                    .title_bar(true)
                    .show(ctx, |ui| {
                        // Add close button in title bar area if closable
                        if self.closable {
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                                let close_btn = ui.add(icons::CLOSE.as_image_tinted(
                                    Vec2::splat(DESIGN_TOKENS.sizing.icon_sm),
                                    ui.style().visuals.widgets.noninteractive.fg_stroke.color,
                                ));
                                if close_btn.clicked() {
                                    close_requested = true;
                                }
                            });
                        }

                        ui.add_space(DESIGN_TOKENS.spacing.sm);

                        // Content
                        inner_response = Some(add_contents(ui));

                        ui.add_space(DESIGN_TOKENS.spacing.md);
                    });
            });

        if close_requested {
            *open = false;
        }

        inner_response.map(|inner| ModalResponse {
            inner,
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
    pub fn new(id: impl std::hash::Hash, title: impl Into<String>) -> Self {
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
    id: impl std::hash::Hash,
    title: impl Into<String>,
    open: &mut bool,
    add_contents: impl FnOnce(&mut Ui) -> R,
) -> Option<ModalResponse<R>> {
    Modal::new(id, title).show(ctx, open, add_contents)
}
