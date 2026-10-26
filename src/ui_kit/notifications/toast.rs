//! Toast Struct
//!
//! A non-blocking notification that appears and optionally auto-dismisses.

use std::sync::atomic::{AtomicU64, Ordering};

use egui::Color32;

use crate::icons::{Icon, icons};
use crate::theme::UiColors;

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
    /// The theme's status colour for this kind, which tints the icon and the progress bar
    pub fn color(&self, ui: &UiColors) -> Color32 {
        match self {
            ToastKind::Info => ui.info,
            ToastKind::Success => ui.success,
            ToastKind::Warning => ui.warning,
            ToastKind::Error => ui.error,
        }
    }

    /// Get the icon for this toast kind
    pub fn icon(&self) -> &'static Icon {
        match self {
            ToastKind::Info => &icons::INFO,
            ToastKind::Success => &icons::SUCCESS,
            ToastKind::Warning => &icons::WARNING,
            ToastKind::Error => &icons::STATUS_ERROR,
        }
    }
}

/// Structured details of an order notification, rendered as symbol and side chips.
#[derive(Clone, Debug)]
pub struct TradingToast {
    /// Instrument identifier.
    pub symbol: String,
    /// Buy or sell.
    pub side: crate::model::OrderSide,
    /// Filled or placed quantity.
    pub quantity: f64,
    /// Price formatted at the instrument's precision.
    pub price: String,
    /// Executed orders have a check; placements have a side chevron.
    pub executed: bool,
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
    /// Optional structured order details.
    pub trading: Option<TradingToast>,
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
            trading: None,
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
