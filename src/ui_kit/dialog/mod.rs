//! Dialog chrome shared by every dialog in the crate.
//!
//! - `DialogFrame`: centred, non-modal window in the dialog frame
//! - `dialog_header`: title row with a close button
//! - `DialogFooter`: a rule above right-aligned action buttons, with optional left content
//!
//! The kit `Modal`, `ConfirmDialog` and `PromptDialog` use the same frame, header and footer
//! over a scrim.

use egui::{
    Align, Align2, Context, Frame, Layout, RichText, Style, Ui, UiBuilder, UiStackInfo, Vec2,
};

use crate::icons::icons;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::buttons::{Button, ButtonSize, IconButton};

/// Marks the content Ui of a fixed-height [`DialogFrame`].
const FIXED_HEIGHT_TAG: &str = "egui_charts::dialog_frame::fixed_height";

/// The dialog surface: the theme's panel fill, window border, radius and shadow, padded 16 px.
pub(crate) fn dialog_frame(style: &Style) -> Frame {
    Frame::window(style)
        .fill(style.visuals.panel_fill)
        .inner_margin(DESIGN_TOKENS.spacing.xxl)
}

/// A centered dialog window with standard styling.
///
/// The requested size is the preferred size of the content inside the 16 px padding: the content
/// area is at least that large, grows with its content, and is never sized beyond the application
/// window.
///
/// # Example
/// ```
/// use egui::Vec2;
/// use egui_charts::ui_kit::dialog::{DialogFrame, dialog_header};
///
/// # egui::__run_test_ctx(|ctx| {
/// # let mut open = true;
/// DialogFrame::new("Series Settings", Vec2::new(600.0, 500.0))
///     .show(ctx, |ui| {
///         if dialog_header(ui, "Series Settings") {
///             open = false;
///         }
///         // content...
///     });
/// # });
/// ```
pub struct DialogFrame {
    id: String,
    size: Vec2,
    auto_height: bool,
    rounding: Option<f32>,
}

impl DialogFrame {
    /// Create a centered dialog with a preferred size.
    pub fn new(id: impl Into<String>, size: Vec2) -> Self {
        Self {
            id: id.into(),
            size,
            auto_height: false,
            rounding: None,
        }
    }

    /// Create a centered dialog with a preferred width and auto-height.
    pub fn with_width(id: impl Into<String>, width: f32) -> Self {
        Self {
            id: id.into(),
            size: Vec2::new(width, 0.0),
            auto_height: true,
            rounding: None,
        }
    }

    /// Override the corner radius (default: the theme's window radius)
    pub fn rounding(mut self, r: f32) -> Self {
        self.rounding = Some(r);
        self
    }

    /// Show the dialog. The closure receives `&mut Ui` for the dialog content.
    pub fn show(self, ctx: &Context, add_contents: impl FnOnce(&mut Ui)) {
        let mut frame = dialog_frame(&ctx.global_style());
        if let Some(r) = self.rounding {
            frame = frame.corner_radius(r);
        }
        let max = ctx.content_rect().shrink(DESIGN_TOKENS.spacing.xxxl).size()
            - frame.total_margin().sum();
        let inner = self.size.min(max).max(Vec2::ZERO);

        let window = egui::Window::new(&self.id)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .frame(frame);
        let window = if self.auto_height {
            window
                .default_width(inner.x)
                .default_height((max.y + frame.total_margin().sum().y).max(0.0))
                .max_height((max.y + frame.total_margin().sum().y).max(0.0))
        } else {
            window.fixed_size(inner)
        };

        let auto_height = self.auto_height;
        window.show(ctx, |ui| {
            // A non-resizable window shrinks to its content, so hold the preferred size here.
            ui.set_min_size(inner);
            if auto_height {
                ui.set_max_height(max.y.max(0.0));
                add_contents(ui);
            } else {
                // The tag lets a `DialogFooter` drawn directly in this Ui sit at the bottom.
                let info = UiStackInfo::default().with_tag(FIXED_HEIGHT_TAG);
                ui.scope_builder(UiBuilder::new().ui_stack_info(info), add_contents);
            }
        });
    }
}

/// Standard dialog title row: left-aligned title and a close button centred on it.
///
/// Returns `true` if the close button was clicked.
pub fn dialog_header(ui: &mut Ui, title: &str) -> bool {
    title_row(ui, title, true)
}

/// Title row shared with the modal dialogs, which hide the close button when not closable.
pub(crate) fn title_row(ui: &mut Ui, title: &str, closable: bool) -> bool {
    // Fix the row height before placing the title so the title centres on the button.
    let row = Vec2::new(ui.available_width(), ButtonSize::MD.pixels());
    let close = ui
        .allocate_ui_with_layout(row, Layout::left_to_right(Align::Center), |ui| {
            ui.label(RichText::new(title).strong().size(20.0));
            closable
                && ui
                    .with_layout(Layout::right_to_left(Align::Center), |ui| {
                        IconButton::new(&icons::CLOSE)
                            .tooltip("Close")
                            .show(ui)
                            .clicked()
                    })
                    .inner
        })
        .inner;
    ui.add_space(DESIGN_TOKENS.spacing.sm);
    close
}

/// Standard dialog footer with right-aligned action buttons.
///
/// Renders a rule, then a row with optional left content and, on the right, the secondary
/// (outlined) button followed by the primary (filled) button. Shown directly in a
/// [`DialogFrame`] content Ui, the footer sits at the bottom of the frame.
///
/// # Example
/// ```
/// # use egui_charts::ui_kit::dialog::DialogFooter;
/// # egui::__run_test_ui(|ui| {
/// let (ok, cancel) = DialogFooter::new("Ok")
///     .secondary("Cancel")
///     .show(ui);
/// if ok { /* apply */ }
/// if cancel { /* close */ }
/// # });
/// ```
pub struct DialogFooter {
    primary_label: String,
    secondary_label: Option<String>,
    primary_enabled: bool,
    danger: bool,
}

impl DialogFooter {
    pub fn new(primary_label: impl Into<String>) -> Self {
        Self {
            primary_label: primary_label.into(),
            secondary_label: None,
            primary_enabled: true,
            danger: false,
        }
    }

    /// Add a secondary (cancel) button.
    pub fn secondary(mut self, label: impl Into<String>) -> Self {
        self.secondary_label = Some(label.into());
        self
    }

    /// Enable/disable the primary button.
    pub fn primary_enabled(mut self, enabled: bool) -> Self {
        self.primary_enabled = enabled;
        self
    }

    /// Mark the primary action as destructive: it is filled with the danger colour.
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    /// Show the footer. Returns `(primary_clicked, secondary_clicked)`.
    pub fn show(self, ui: &mut Ui) -> (bool, bool) {
        self.show_with_left(ui, |_| ())
    }

    /// Show the footer with left-side content. Returns `(primary_clicked, secondary_clicked)`.
    pub fn show_with_left<R>(
        self,
        ui: &mut Ui,
        left_content: impl FnOnce(&mut Ui) -> R,
    ) -> (bool, bool) {
        // In a fixed-height `DialogFrame`, the free space goes between the content and the rule.
        // The footer height is measured on the previous frame; the window's sizing pass measures
        // it unpinned, so the first visible frame already has the window at its final size.
        let pinned = ui.stack().tags().contains(FIXED_HEIGHT_TAG);
        let height_id = ui.scope_id().with("dialog_footer_height");
        if pinned && !ui.is_sizing_pass() {
            let height = ui.data(|d| d.get_temp::<f32>(height_id)).unwrap_or(0.0);
            ui.add_space((ui.available_height() - height).max(0.0));
        }
        let top = ui.cursor().top();

        ui.add_space(DESIGN_TOKENS.spacing.lg);
        ui.separator();
        ui.add_space(DESIGN_TOKENS.spacing.lg);

        let row = ui.horizontal(|ui| {
            left_content(ui);

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.spacing_mut().item_spacing.x = DESIGN_TOKENS.spacing.lg;

                let primary = Button::new(&self.primary_label).enabled(self.primary_enabled);
                let primary = if self.danger {
                    primary.danger()
                } else {
                    primary.primary()
                };
                let primary_clicked = primary.show(ui).clicked();

                let secondary_clicked = self
                    .secondary_label
                    .as_ref()
                    .is_some_and(|label| Button::new(label).outlined().show(ui).clicked());

                (primary_clicked, secondary_clicked)
            })
            .inner
        });

        if pinned {
            let height = row.response.rect.bottom() - top;
            ui.data_mut(|d| d.insert_temp(height_id, height));
        }
        row.inner
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn footer_sits_at_the_bottom_of_a_fixed_frame_from_the_first_visible_frame() {
        let ctx = Context::default();
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1000.0, 800.0));
        let frame = || {
            let mut row = egui::Rect::NOTHING;
            let mut window = egui::Id::NULL;
            let input = egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                DialogFrame::new("d", Vec2::new(420.0, 240.0)).show(ui.ctx(), |ui| {
                    window = ui.layer_id().id;
                    ui.label("Body");
                    DialogFooter::new("Apply")
                        .secondary("Cancel")
                        .show_with_left(ui, |ui| row = ui.max_rect());
                });
            })
            .drop_without_applying_deltas();
            (ctx.memory(|m| m.area_rect(window)).unwrap(), row)
        };
        frame(); // sizing pass
        let first = frame();
        let second = frame();
        assert_eq!(
            first.0, second.0,
            "the window moved after its first visible frame"
        );
        let inner_bottom = second.0.bottom() - DESIGN_TOKENS.spacing.xxl;
        assert!((second.1.top() + ButtonSize::MD.pixels() - inner_bottom).abs() <= 1.5);
    }

    #[test]
    fn auto_height_scroll_body_reaches_the_screen_cap() {
        let ctx = Context::default();
        for height in [900.0, 450.0] {
            let mut window = egui::Id::NULL;
            for _ in 0..3 {
                ctx.run_ui(
                    egui::RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            Vec2::new(1440.0, height),
                        )),
                        ..Default::default()
                    },
                    |ui| {
                        DialogFrame::with_width("scroll", 500.0).show(ui.ctx(), |ui| {
                            window = ui.layer_id().id;
                            dialog_header(ui, "Scrolling body");
                            egui::ScrollArea::vertical()
                                .max_height(ui.available_height())
                                .show(ui, |ui| {
                                    for i in 0..100 {
                                        ui.label(format!("Row {i}"));
                                    }
                                });
                        });
                    },
                )
                .drop_without_applying_deltas();
            }
            let rect = ctx.memory(|m| m.area_rect(window)).unwrap();
            assert!((rect.height() - (height - 48.0)).abs() <= 2.0, "{rect:?}");
            assert!(rect.top() >= 23.0 && rect.bottom() <= height - 23.0);
        }
    }
}
