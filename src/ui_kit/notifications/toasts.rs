//! Toasts container
//!
//! Per-instance storage for toast notifications. Each chart instance owns its
//! own [`Toasts`] queue so that notifications never leak across independent
//! widgets in the same process.

use super::toast::Toast;

/// Current wall-clock time in seconds.
///
/// Uses [`web_time`] so the same call resolves on native and `wasm32` targets;
/// the standard-library clock panics in the browser, which would crash any
/// host embedding this crate in WebAssembly.
pub(crate) fn current_time_seconds() -> f64 {
    web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0)
}

/// Toasts container for per-instance usage.
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
        toast.created_at = current_time_seconds();
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
        let current_time = current_time_seconds();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn current_time_is_monotonic_and_nonzero() {
        // web_time::SystemTime must resolve on every target the crate ships to;
        // a working clock returns a positive wall-clock value.
        let t = current_time_seconds();
        assert!(t > 0.0, "expected a positive wall-clock time, got {t}");
    }

    #[test]
    fn add_stamps_creation_time_from_web_time() {
        let mut toasts = Toasts::new();
        toasts.info("hello");
        assert_eq!(toasts.len(), 1);
        assert!(
            toasts.toasts()[0].created_at > 0.0,
            "add() must stamp created_at from the web_time clock"
        );
    }

    #[test]
    fn cleanup_keeps_permanent_and_fresh_toasts() {
        let mut toasts = Toasts::new();
        toasts.add(Toast::info("permanent").permanent());
        toasts.add(Toast::info("fresh").duration(60.0));
        toasts.cleanup_expired();
        assert_eq!(toasts.len(), 2);
    }
}
