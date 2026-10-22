//! Alert Component
//!
//! A semantic message box for displaying status information to users.
//!
//! # Usage
//!
//! ```
//! use egui_charts::ui_kit::alert::Alert;
//!
//! # egui::__run_test_ui(|ui| {
//! // Simple alert
//! Alert::info("This is an informational message").show(ui);
//!
//! // Alert with title
//! Alert::error("Failed to load data")
//!     .with_title("Connection Error")
//!     .show(ui);
//!
//! // Dismissible alert
//! Alert::warning("Your session will expire soon")
//!     .dismissible(true)
//!     .show(ui);
//! # });
//! ```

use egui::{Color32, Response, RichText, Ui, Vec2};

use crate::icons::{Icon, icons};
use crate::theme::{ThemeExt, UiColors};
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::buttons::{ButtonSize, IconButton};

/// Alert severity levels
#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub enum AlertKind {
    /// Informational message (blue)
    #[default]
    Info,
    /// Success message (green)
    Success,
    /// Warning message (orange/amber)
    Warning,
    /// Error message (red)
    Error,
}

impl AlertKind {
    /// The theme's status colour for this kind, which tints the icon, fill and border
    pub fn color(&self, ui: &UiColors) -> Color32 {
        match self {
            AlertKind::Info => ui.info,
            AlertKind::Success => ui.success,
            AlertKind::Warning => ui.warning,
            AlertKind::Error => ui.error,
        }
    }

    /// Get the icon for this alert kind
    pub fn icon(&self) -> &'static Icon {
        match self {
            AlertKind::Info => &icons::INFO,
            AlertKind::Success => &icons::SUCCESS,
            AlertKind::Warning => &icons::WARNING,
            AlertKind::Error => &icons::STATUS_ERROR,
        }
    }
}

/// Response from showing an Alert
pub struct AlertResponse {
    /// The underlying egui Response
    pub response: Response,
    /// Whether the dismiss button was clicked
    pub dismissed: bool,
}

/// A semantic message box for displaying status information
pub struct Alert {
    kind: AlertKind,
    title: Option<String>,
    message: String,
    dismissible: bool,
    show_icon: bool,
}

impl Alert {
    /// Create a new alert with the given kind and message
    pub fn new(kind: AlertKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            title: None,
            message: message.into(),
            dismissible: false,
            show_icon: true,
        }
    }

    /// Create an info alert
    pub fn info(message: impl Into<String>) -> Self {
        Self::new(AlertKind::Info, message)
    }

    /// Create a success alert
    pub fn success(message: impl Into<String>) -> Self {
        Self::new(AlertKind::Success, message)
    }

    /// Create a warning alert
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(AlertKind::Warning, message)
    }

    /// Create an error alert
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(AlertKind::Error, message)
    }

    /// Set the alert title
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Make the alert dismissible
    pub fn dismissible(mut self, dismissible: bool) -> Self {
        self.dismissible = dismissible;
        self
    }

    /// Show or hide the icon
    pub fn show_icon(mut self, show: bool) -> Self {
        self.show_icon = show;
        self
    }

    /// Show the alert
    pub fn show(self, ui: &mut Ui) -> AlertResponse {
        let theme = ui.active_theme();
        let color = self.kind.color(&theme.ui);

        let frame = egui::Frame::new()
            .fill(color.gamma_multiply(0.1))
            .stroke(egui::Stroke::new(
                DESIGN_TOKENS.stroke.hairline,
                color.gamma_multiply(0.4),
            ))
            .corner_radius(theme.radius.md)
            .inner_margin(egui::Margin::same(theme.spacing.lg as i8));

        let mut dismissed = false;

        let inner_response = frame.show(ui, |ui| {
            // Every alert fills the width, dismissible or not.
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                // Icon
                if self.show_icon {
                    let icon = self.kind.icon();
                    let icon_size = DESIGN_TOKENS.sizing.icon_md;
                    ui.add(icon.as_image_tinted(Vec2::splat(icon_size), color));
                }

                // Content
                ui.vertical(|ui| {
                    // Title, then the message, muted so the title stands out
                    let mut message_color = theme.ui.text;
                    if let Some(title) = &self.title {
                        ui.label(RichText::new(title).strong());
                        ui.add_space(DESIGN_TOKENS.spacing.xs);
                        message_color = theme.ui.text_secondary;
                    }
                    ui.label(RichText::new(&self.message).color(message_color));
                });

                // Dismiss button
                if self.dismissible {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                        dismissed = IconButton::new(&icons::CLOSE)
                            .size(ButtonSize::SM)
                            .tooltip("Dismiss")
                            .show(ui)
                            .clicked();
                    });
                }
            });
        });

        AlertResponse {
            response: inner_response.response,
            dismissed,
        }
    }
}
