//! Modal Handler
//!
//! Manages modal open/close state with optional payload.

use egui::{Context, Id, Ui};

use crate::theme::ThemeExt;
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
            id: Id::unique(id),
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
            .frame(dialog_frame(ctx))
            .backdrop_color(ctx.active_theme().ui.modal_overlay)
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
    fn modal_backdrops_follow_the_active_theme() {
        use crate::theme::{Theme, apply_to_egui};
        use crate::ui_kit::{command_palette::CommandPalette, modal::Modal};
        use egui::{Color32, Pos2, RawInput, Rect, Shape, Vec2};
        let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0));
        for (theme, alpha) in [
            (Theme::dark(), 180),
            (Theme::light(), 96),
            (Theme::high_contrast(), 0),
        ] {
            for surface in 0..3 {
                let ctx = Context::default();
                apply_to_egui(&ctx, &theme);
                ctx.all_styles_mut(|style| style.animation_time = 0.0);
                let mut handler = ModalHandler::new(("backdrop", surface));
                handler.open();
                let mut palette = CommandPalette::new(Vec::new());
                for frame in 0..3 {
                    let output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(screen),
                            ..Default::default()
                        },
                        |ui| match surface {
                            0 => {
                                Modal::new("backdrop", "Modal")
                                    .show(ui.ctx(), &mut true, |ui| ui.label("Content"));
                            }
                            1 => {
                                handler.show_simple(ui.ctx(), |ui| ui.label("Content"));
                            }
                            _ => {
                                palette.show(ui.ctx(), &mut true);
                            }
                        },
                    );
                    let fills: Vec<_> = output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            Shape::Rect(rect)
                                if rect.rect == screen
                                    && rect.fill.a() > 0
                                    && (alpha == 0 || rect.fill.a() < 255) =>
                            {
                                Some(rect.fill)
                            }
                            _ => None,
                        })
                        .collect();
                    output.drop_without_applying_deltas();
                    let expected = if alpha == 0 {
                        vec![]
                    } else {
                        vec![Color32::from_black_alpha(alpha)]
                    };
                    if frame == 2 {
                        assert_eq!(fills, expected, "dark: {}, surface: {surface}", theme.dark);
                    }
                }
            }
        }
    }

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
