//! Dialog components — reusable building blocks for modal dialogs.
//!
//! Provides composable pieces for constructing dialogs with consistent styling:
//! - `DialogFrame`: Centered frameless window with standard rounding
//! - `dialog_header`: Title bar with close button
//! - `DialogFooter`: Right-aligned action buttons with optional left content

use egui::{Align2, Context, RichText, Ui, Vec2};

use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::buttons::dialog_close_button;

/// A centered, frameless dialog window with standard styling.
///
/// Replaces the repeated `Window::new(...).anchor(...).fixed_size(...).title_bar(false).frame(...)`
/// boilerplate found across all modal dialogs.
///
/// # Example
/// ```ignore
/// DialogFrame::new("Series Settings", Vec2::new(600.0, 500.0))
///     .show(ctx, |ui| {
///         if dialog_header(ui, "Series Settings") { self.close(); }
///         // content...
///     });
/// ```
pub struct DialogFrame {
    id: String,
    size: Vec2,
    auto_height: bool,
    rounding: Option<f32>,
}

impl DialogFrame {
    /// Create a centered dialog with a fixed size.
    pub fn new(id: impl Into<String>, size: Vec2) -> Self {
        Self {
            id: id.into(),
            size,
            auto_height: false,
            rounding: None,
        }
    }

    /// Create a centered dialog with a fixed width and auto-height.
    pub fn with_width(id: impl Into<String>, width: f32) -> Self {
        Self {
            id: id.into(),
            size: Vec2::new(width, 0.0),
            auto_height: true,
            rounding: None,
        }
    }

    /// Override the corner radius (default: `DESIGN_TOKENS.sizing.settings_dialog.rounding`)
    pub fn rounding(mut self, r: f32) -> Self {
        self.rounding = Some(r);
        self
    }

    /// Show the dialog. The closure receives `&mut Ui` for the dialog content.
    pub fn show(self, ctx: &Context, add_contents: impl FnOnce(&mut Ui)) {
        let rounding = self
            .rounding
            .unwrap_or(DESIGN_TOKENS.sizing.settings_dialog.rounding);

        let mut window = egui::Window::new(&self.id)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .title_bar(false)
            .collapsible(false)
            .resizable(false)
            .frame(egui::Frame::window(&ctx.style()).corner_radius(rounding));

        if self.auto_height {
            window = window.default_width(self.size.x);
        } else {
            window = window.fixed_size(self.size);
        }

        window.show(ctx, add_contents);
    }
}

/// Standard dialog title bar: left-padded heading + right-aligned close button.
///
/// Returns `true` if the close button was clicked.
///
/// # Example
/// ```ignore
/// if dialog_header(ui, "Notification Settings") {
///     self.close();
/// }
/// ```
pub fn dialog_header(ui: &mut Ui, title: &str) -> bool {
    let mut close_clicked = false;
    ui.horizontal(|ui| {
        ui.add_space(DESIGN_TOKENS.spacing.lg);
        ui.heading(title);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add_space(DESIGN_TOKENS.spacing.sm);
            if dialog_close_button(ui).clicked() {
                close_clicked = true;
            }
        });
    });
    ui.add_space(DESIGN_TOKENS.spacing.sm);
    close_clicked
}

/// Standard dialog footer with right-aligned action buttons.
///
/// Renders a separator, then a horizontal row with:
/// - Optional left-side content (via `left_content`)
/// - Right-aligned primary button (filled with selection color)
/// - Optional secondary button
///
/// # Example
/// ```ignore
/// let (ok, cancel) = DialogFooter::new("Ok")
///     .secondary("Cancel")
///     .show(ui);
/// if ok { /* apply */ }
/// if cancel { /* close */ }
/// ```
pub struct DialogFooter {
    primary_label: String,
    secondary_label: Option<String>,
    primary_enabled: bool,
}

impl DialogFooter {
    pub fn new(primary_label: impl Into<String>) -> Self {
        Self {
            primary_label: primary_label.into(),
            secondary_label: None,
            primary_enabled: true,
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

    /// Show the footer. Returns `(primary_clicked, secondary_clicked)`.
    pub fn show(self, ui: &mut Ui) -> (bool, bool) {
        let mut primary_clicked = false;
        let mut secondary_clicked = false;

        ui.add_space(DESIGN_TOKENS.spacing.sm);
        ui.separator();
        ui.add_space(DESIGN_TOKENS.spacing.sm);

        ui.horizontal(|ui| {
            ui.add_space(DESIGN_TOKENS.spacing.lg);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(DESIGN_TOKENS.spacing.lg);

                // Primary button (rightmost)
                let btn = egui::Button::new(RichText::new(&self.primary_label))
                    .fill(ui.style().visuals.selection.bg_fill)
                    .min_size(Vec2::new(
                        DESIGN_TOKENS.sizing.button_min_width_xs,
                        DESIGN_TOKENS.sizing.button_md,
                    ));
                if ui.add_enabled(self.primary_enabled, btn).clicked() {
                    primary_clicked = true;
                }

                // Secondary button
                if let Some(ref label) = self.secondary_label {
                    let btn = egui::Button::new(label.as_str()).min_size(Vec2::new(
                        DESIGN_TOKENS.sizing.button_min_width_xs,
                        DESIGN_TOKENS.sizing.button_md,
                    ));
                    if ui.add(btn).clicked() {
                        secondary_clicked = true;
                    }
                }
            });
        });
        ui.add_space(DESIGN_TOKENS.spacing.sm);

        (primary_clicked, secondary_clicked)
    }

    /// Show the footer with left-side content. Returns `(primary_clicked, secondary_clicked)`.
    pub fn show_with_left<R>(
        self,
        ui: &mut Ui,
        left_content: impl FnOnce(&mut Ui) -> R,
    ) -> (bool, bool) {
        let mut primary_clicked = false;
        let mut secondary_clicked = false;

        ui.add_space(DESIGN_TOKENS.spacing.sm);
        ui.separator();
        ui.add_space(DESIGN_TOKENS.spacing.sm);

        ui.horizontal(|ui| {
            ui.add_space(DESIGN_TOKENS.spacing.lg);

            // Left content
            left_content(ui);

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(DESIGN_TOKENS.spacing.lg);

                // Primary button (rightmost)
                let btn = egui::Button::new(RichText::new(&self.primary_label))
                    .fill(ui.style().visuals.selection.bg_fill)
                    .min_size(Vec2::new(
                        DESIGN_TOKENS.sizing.button_min_width_xs,
                        DESIGN_TOKENS.sizing.button_md,
                    ));
                if ui.add_enabled(self.primary_enabled, btn).clicked() {
                    primary_clicked = true;
                }

                // Secondary button
                if let Some(ref label) = self.secondary_label {
                    let btn = egui::Button::new(label.as_str()).min_size(Vec2::new(
                        DESIGN_TOKENS.sizing.button_min_width_xs,
                        DESIGN_TOKENS.sizing.button_md,
                    ));
                    if ui.add(btn).clicked() {
                        secondary_clicked = true;
                    }
                }
            });
        });
        ui.add_space(DESIGN_TOKENS.spacing.sm);

        (primary_clicked, secondary_clicked)
    }
}
