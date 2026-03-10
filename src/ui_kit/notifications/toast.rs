//! Toast Struct
//!
//! A non-blocking notification that appears and optionally auto-dismisses.

use std::sync::atomic::{AtomicU64, Ordering};

use egui::Color32;

use crate::icons::{Icon, icons};
use crate::tokens::DESIGN_TOKENS;

/// Counter for generating unique toast IDs
static TOAST_ID_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a unique toast ID
fn next_toast_id() -> u64 {
    TOAST_ID_COUNTER.fetch_add(1, Ordering::Relaxed)
}

/// Toast severity/type
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ToastKind {
    /// Informational message (blue)
    #[default]
    Info,
    /// Success message (green)
    Success,
    /// Warning message (amber)
    Warning,
    /// Error message (red)
    Error,
}

impl ToastKind {
    /// Get the background color for this toast kind
    pub fn bg_color(&self) -> Color32 {
        match self {
            ToastKind::Info => DESIGN_TOKENS.semantic.extended.info.linear_multiply(0.9),
            ToastKind::Success => DESIGN_TOKENS.semantic.extended.success.linear_multiply(0.9),
            ToastKind::Warning => DESIGN_TOKENS.semantic.extended.warning.linear_multiply(0.9),
            ToastKind::Error => DESIGN_TOKENS.semantic.extended.error.linear_multiply(0.9),
        }
    }

    /// Get the text color for this toast kind
    pub fn text_color(&self) -> Color32 {
        DESIGN_TOKENS.semantic.ui.text_light
    }

    /// Get the icon for this toast kind
    pub fn icon(&self) -> &'static Icon {
        match self {
            ToastKind::Info => &icons::INFO,
            ToastKind::Success => &icons::SETTINGS_STATUS_LINE,
            ToastKind::Warning => &icons::STATUS_ERROR,
            ToastKind::Error => &icons::STATUS_ERROR,
        }
    }
}

/// A toast notification
#[derive(Clone, Debug)]
pub struct Toast {
    /// Unique identifier
    pub id: u64,
    /// Toast kind/severity
    pub kind: ToastKind,
    /// Message to display
    pub message: String,
    /// Optional title
    pub title: Option<String>,
    /// Duration in seconds (0 = permanent until dismissed)
    pub duration: f32,
    /// Timestamp when the toast was created (in seconds since app start)
    pub created_at: f64,
    /// Whether the toast can be dismissed by clicking
    pub dismissible: bool,
}

impl Toast {
    /// Create a new toast
    pub fn new(kind: ToastKind, message: impl Into<String>) -> Self {
        Self {
            id: next_toast_id(),
            kind,
            message: message.into(),
            title: None,
            duration: 5.0, // Default 5 seconds
            created_at: 0.0,
            dismissible: true,
        }
    }

    /// Create an info toast
    pub fn info(message: impl Into<String>) -> Self {
        Self::new(ToastKind::Info, message)
    }

    /// Create a success toast
    pub fn success(message: impl Into<String>) -> Self {
        Self::new(ToastKind::Success, message)
    }

    /// Create a warning toast
    pub fn warning(message: impl Into<String>) -> Self {
        Self::new(ToastKind::Warning, message)
    }

    /// Create an error toast
    pub fn error(message: impl Into<String>) -> Self {
        Self::new(ToastKind::Error, message)
    }

    /// Set the title
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    /// Set the duration in seconds (0 = permanent)
    pub fn duration(mut self, seconds: f32) -> Self {
        self.duration = seconds;
        self
    }

    /// Make the toast permanent (won't auto-dismiss)
    pub fn permanent(mut self) -> Self {
        self.duration = 0.0;
        self
    }

    /// Set whether the toast can be dismissed by clicking
    pub fn dismissible(mut self, dismissible: bool) -> Self {
        self.dismissible = dismissible;
        self
    }

    /// Check if the toast has expired
    pub fn is_expired(&self, current_time: f64) -> bool {
        if self.duration <= 0.0 {
            return false; // Permanent toast
        }
        current_time - self.created_at >= self.duration as f64
    }

    /// Get the remaining time as a fraction (1.0 = full, 0.0 = expired)
    pub fn remaining_fraction(&self, current_time: f64) -> f32 {
        if self.duration <= 0.0 {
            return 1.0; // Permanent toast
        }
        let elapsed = current_time - self.created_at;
        (1.0 - (elapsed / self.duration as f64) as f32).clamp(0.0, 1.0)
    }
}
