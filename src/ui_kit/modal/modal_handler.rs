//! Modal Handler
//!
//! Manages modal open/close state with optional payload.

use egui::{Context, Id, Ui};

/// Manages modal open/close state
pub struct ModalHandler<T = ()> {
    id: Id,
    open: bool,
    payload: Option<T>,
}

impl<T> Default for ModalHandler<T> {
    fn default() -> Self {
        Self {
            id: Id::NULL,
            open: false,
            payload: None,
        }
    }
}

impl<T> ModalHandler<T> {
    /// Create a new modal handler with the given ID
    pub fn new(id: impl std::hash::Hash) -> Self {
        Self {
            id: Id::new(id),
            open: false,
            payload: None,
        }
    }

    /// Get the modal ID
    pub fn id(&self) -> Id {
        self.id
    }

    /// Open the modal without a payload
    pub fn open(&mut self) {
        self.open = true;
    }

    /// Open the modal with a payload
    pub fn open_with(&mut self, payload: T) {
        self.open = true;
        self.payload = Some(payload);
    }

    /// Close the modal and clear the payload
    pub fn close(&mut self) {
        self.open = false;
        self.payload = None;
    }

    /// Check if the modal is open
    pub fn is_open(&self) -> bool {
        self.open
    }

    /// Get a reference to the payload
    pub fn payload(&self) -> Option<&T> {
        self.payload.as_ref()
    }

    /// Get a mutable reference to the payload
    pub fn payload_mut(&mut self) -> Option<&mut T> {
        self.payload.as_mut()
    }

    /// Take the payload out of the handler
    pub fn take_payload(&mut self) -> Option<T> {
        self.payload.take()
    }

    /// Show the modal if open
    ///
    /// Returns the result of the content function if the modal was shown.
    pub fn show<R>(
        &mut self,
        ctx: &Context,
        add_contents: impl FnOnce(&mut Ui, Option<&T>) -> R,
    ) -> Option<R> {
        if !self.open {
            return None;
        }

        let mut result = None;

        egui::Area::new(self.id)
            .fixed_pos(egui::pos2(0.0, 0.0))
            .show(ctx, |ui| {
                // Dim background
                let screen_rect = ctx.content_rect();
                let dim_color = egui::Color32::from_black_alpha(180);
                ui.painter().rect_filled(screen_rect, 0.0, dim_color);

                // Capture clicks on backdrop to close
                let backdrop_response = ui.allocate_rect(screen_rect, egui::Sense::click());
                if backdrop_response.clicked() {
                    self.close();
                    return;
                }

                // Center the modal content
                let modal_rect = egui::Rect::from_center_size(
                    screen_rect.center(),
                    egui::vec2(
                        crate::tokens::DESIGN_TOKENS.sizing.dialog.default_width,
                        crate::tokens::DESIGN_TOKENS.sizing.dialog.default_height,
                    ),
                );

                // Create a child UI for the modal content
                let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(modal_rect));
                result = Some(add_contents(&mut child_ui, self.payload.as_ref()));
            });

        result
    }

    /// Show the modal with mutable payload access
    pub fn show_mut<R>(
        &mut self,
        ctx: &Context,
        add_contents: impl FnOnce(&mut Ui, Option<&mut T>) -> R,
    ) -> Option<R> {
        if !self.open {
            return None;
        }

        let mut result = None;

        egui::Area::new(self.id)
            .fixed_pos(egui::pos2(0.0, 0.0))
            .show(ctx, |ui| {
                let screen_rect = ctx.content_rect();
                let dim_color = egui::Color32::from_black_alpha(180);
                ui.painter().rect_filled(screen_rect, 0.0, dim_color);

                let backdrop_response = ui.allocate_rect(screen_rect, egui::Sense::click());
                if backdrop_response.clicked() {
                    self.close();
                    return;
                }

                let modal_rect = egui::Rect::from_center_size(
                    screen_rect.center(),
                    egui::vec2(
                        crate::tokens::DESIGN_TOKENS.sizing.dialog.default_width,
                        crate::tokens::DESIGN_TOKENS.sizing.dialog.default_height,
                    ),
                );

                let mut child_ui = ui.new_child(egui::UiBuilder::new().max_rect(modal_rect));
                result = Some(add_contents(&mut child_ui, self.payload.as_mut()));
            });

        result
    }
}

impl ModalHandler<()> {
    /// Show the modal if open (simplified for no payload)
    pub fn show_simple<R>(
        &mut self,
        ctx: &Context,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> Option<R> {
        self.show(ctx, |ui, _| add_contents(ui))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_modal_handler_lifecycle() {
        let mut handler: ModalHandler<String> = ModalHandler::new("test_modal");

        assert!(!handler.is_open());
        assert!(handler.payload().is_none());

        handler.open_with("test_payload".to_string());
        assert!(handler.is_open());
        assert_eq!(handler.payload(), Some(&"test_payload".to_string()));

        handler.close();
        assert!(!handler.is_open());
        assert!(handler.payload().is_none());
    }
}
