//! Alert Component
//!
//! A semantic message box for displaying status information to users.
//!
//! # Usage
//!
//! ```ignore
//! use open_trading_charts::ui_kit::alert::{Alert, AlertKind};
//!
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
//! ```

use egui::{Color32, Response, RichText, Ui, Vec2};

use crate::icons::{Icon, icons};
use crate::tokens::DESIGN_TOKENS;

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
    /// Get the background color for this alert kind
    pub fn bg_color(&self) -> Color32 {
        let tokens = &DESIGN_TOKENS.semantic.alert;
        match self {
            AlertKind::Info => tokens.info_bg,
            AlertKind::Success => tokens.success_bg,
            AlertKind::Warning => tokens.warning_bg,
            AlertKind::Error => tokens.error_bg,
        }
    }

    /// Get the border color for this alert kind
    pub fn border_color(&self) -> Color32 {
        let tokens = &DESIGN_TOKENS.semantic.alert;
        match self {
            AlertKind::Info => tokens.info_border,
            AlertKind::Success => tokens.success_border,
            AlertKind::Warning => tokens.warning_border,
            AlertKind::Error => tokens.error_border,
        }
    }

    /// Get the icon color for this alert kind
    pub fn icon_color(&self) -> Color32 {
        let tokens = &DESIGN_TOKENS.semantic.alert;
        match self {
            AlertKind::Info => tokens.info_icon,
            AlertKind::Success => tokens.success_icon,
            AlertKind::Warning => tokens.warning_icon,
            AlertKind::Error => tokens.error_icon,
        }
    }

    /// Get the icon for this alert kind
    pub fn icon(&self) -> &'static Icon {
        match self {
            AlertKind::Info => &icons::INFO,
            AlertKind::Success => &icons::SETTINGS_STATUS_LINE,
            AlertKind::Warning => &icons::STATUS_ERROR,
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
        let bg_color = self.kind.bg_color();
        let border_color = self.kind.border_color();
        let icon_color = self.kind.icon_color();

        let frame = egui::Frame::new()
            .fill(bg_color)
            .stroke(egui::Stroke::new(
                DESIGN_TOKENS.stroke.hairline,
                border_color,
            ))
            .corner_radius(DESIGN_TOKENS.rounding.md)
            .inner_margin(egui::Margin::same(DESIGN_TOKENS.spacing.lg as i8));

        let mut dismissed = false;

        let inner_response = frame.show(ui, |ui| {
            ui.horizontal(|ui| {
                // Icon
                if self.show_icon {
                    let icon = self.kind.icon();
                    let icon_size = DESIGN_TOKENS.sizing.icon_md;
                    ui.add(icon.as_image_tinted(Vec2::splat(icon_size), icon_color));
                    ui.add_space(DESIGN_TOKENS.spacing.lg);
                }

                // Content
                ui.vertical(|ui| {
                    // Title
                    if let Some(title) = &self.title {
                        ui.label(RichText::new(title).strong().color(icon_color));
                        ui.add_space(DESIGN_TOKENS.spacing.xs);
                    }

                    // Message
                    ui.label(&self.message);
                });

                // Dismiss button
                if self.dismissible {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                        let close_icon = &icons::CLOSE;
                        let close_btn = ui.add(close_icon.as_image_tinted(
                            Vec2::splat(DESIGN_TOKENS.sizing.icon_sm),
                            ui.style().visuals.widgets.noninteractive.fg_stroke.color,
                        ));
                        if close_btn.clicked() {
                            dismissed = true;
                        }
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
