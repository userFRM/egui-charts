//! Modal Handler
//!
//! Manages modal open/close state with optional payload.

use egui::{Context, Id, Ui};

use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::dialog::dialog_frame;

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
    pub fn new(id: impl std::hash::Hash + std::fmt::Debug) -> Self {
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
    /// The content is drawn in the dialog frame, centred above a dimmed backdrop. Escape or a
    /// click on the backdrop closes the modal.
    ///
    /// Returns the result of the content function if the modal was shown.
    pub fn show<R>(
        &mut self,
        ctx: &Context,
        add_contents: impl FnOnce(&mut Ui, Option<&T>) -> R,
    ) -> Option<R> {
        self.show_mut(ctx, |ui, payload| add_contents(ui, payload.map(|p| &*p)))
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

        let response = egui::Modal::new(self.id)
            .frame(dialog_frame(&ctx.global_style()))
            .backdrop_color(DESIGN_TOKENS.semantic.modal.overlay_bg)
            .show(ctx, |ui| add_contents(ui, self.payload.as_mut()));
        if response.should_close() {
            self.close();
        }
        Some(response.inner)
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
