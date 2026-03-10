//! Toast Manager
//!
//! Global toast manager for managing and displaying toasts.

use std::sync::Mutex;

use once_cell::sync::Lazy;

use super::toast::Toast;

/// Global toast storage
static TOASTS: Lazy<Mutex<Vec<Toast>>> = Lazy::new(|| Mutex::new(Vec::new()));

/// Add a toast to the global toast list
pub fn add_toast(mut toast: Toast) {
    // Set the creation time
    toast.created_at = get_current_time();

    if let Ok(mut toasts) = TOASTS.lock() {
        toasts.push(toast);
    }
}

/// Add an info toast
pub fn toast_info(message: impl Into<String>) {
    add_toast(Toast::info(message));
}

/// Add a success toast
pub fn toast_success(message: impl Into<String>) {
    add_toast(Toast::success(message));
}

/// Add a warning toast
pub fn toast_warning(message: impl Into<String>) {
    add_toast(Toast::warning(message));
}

/// Add an error toast
pub fn toast_error(message: impl Into<String>) {
    add_toast(Toast::error(message));
}

/// Remove a toast by ID
pub fn remove_toast(id: u64) {
    if let Ok(mut toasts) = TOASTS.lock() {
        toasts.retain(|t| t.id != id);
    }
}

/// Clear all toasts
pub fn clear_toasts() {
    if let Ok(mut toasts) = TOASTS.lock() {
        toasts.clear();
    }
}

/// Get the current toasts (cloned for thread safety)
pub fn get_toasts() -> Vec<Toast> {
    if let Ok(toasts) = TOASTS.lock() {
        toasts.clone()
    } else {
        Vec::new()
    }
}

/// Remove expired toasts
pub fn cleanup_expired_toasts() {
    let current_time = get_current_time();
    if let Ok(mut toasts) = TOASTS.lock() {
        toasts.retain(|t| !t.is_expired(current_time));
    }
}

/// Get the number of active toasts
pub fn toast_count() -> usize {
    if let Ok(toasts) = TOASTS.lock() {
        toasts.len()
    } else {
        0
    }
}

/// Get current time in seconds (since app start)
fn get_current_time() -> f64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Toasts container for non-global usage
#[derive(Default)]
pub struct Toasts {
    toasts: Vec<Toast>,
}

impl Toasts {
    /// Create a new toasts container
    pub fn new() -> Self {
        Self { toasts: Vec::new() }
    }

    /// Add a toast
    pub fn add(&mut self, mut toast: Toast) {
        toast.created_at = get_current_time();
        self.toasts.push(toast);
    }

    /// Add an info toast
    pub fn info(&mut self, message: impl Into<String>) {
        self.add(Toast::info(message));
    }

    /// Add a success toast
    pub fn success(&mut self, message: impl Into<String>) {
        self.add(Toast::success(message));
    }

    /// Add a warning toast
    pub fn warning(&mut self, message: impl Into<String>) {
        self.add(Toast::warning(message));
    }

    /// Add an error toast
    pub fn error(&mut self, message: impl Into<String>) {
        self.add(Toast::error(message));
    }

    /// Remove a toast by ID
    pub fn remove(&mut self, id: u64) {
        self.toasts.retain(|t| t.id != id);
    }

    /// Clear all toasts
    pub fn clear(&mut self) {
        self.toasts.clear();
    }

    /// Get the toasts
    pub fn toasts(&self) -> &[Toast] {
        &self.toasts
    }

    /// Get the toasts mutably
    pub fn toasts_mut(&mut self) -> &mut Vec<Toast> {
        &mut self.toasts
    }

    /// Remove expired toasts
    pub fn cleanup_expired(&mut self) {
        let current_time = get_current_time();
        self.toasts.retain(|t| !t.is_expired(current_time));
    }

    /// Get the number of toasts
    pub fn len(&self) -> usize {
        self.toasts.len()
    }

    /// Check if there are no toasts
    pub fn is_empty(&self) -> bool {
        self.toasts.is_empty()
    }
}
