//! ContextExt - Extension trait for egui::Context
//!
//! Provides convenience methods for accessing global state and configuration.

use egui::Context;

use crate::theme::context::ThemeContextExt;

/// Extension trait for `egui::Context` providing convenience methods
pub trait ContextExt {
    /// Check if the UI is in dark mode
    fn is_dark_mode(&self) -> bool;

    /// Check if the chart area is dark
    fn is_dark_chart(&self) -> bool;

    /// Check if we're on a mobile device (phone or tablet)
    fn is_mobile(&self) -> bool;

    /// Check if we're on a phone
    fn is_phone(&self) -> bool;

    /// Check if we're on a tablet
    fn is_tablet(&self) -> bool;

    /// Check if we're on desktop
    fn is_desktop(&self) -> bool;

    /// Check if touch input is currently active
    fn is_touch_active(&self) -> bool;
}

impl ContextExt for Context {
    fn is_dark_mode(&self) -> bool {
        // Use the existing ThemeContextExt
        ThemeContextExt::is_dark_ui(self)
    }

    fn is_dark_chart(&self) -> bool {
        ThemeContextExt::is_dark_chart(self)
    }

    fn is_mobile(&self) -> bool {
        false // Desktop-only
    }

    fn is_phone(&self) -> bool {
        false // Desktop-only
    }

    fn is_tablet(&self) -> bool {
        false // Desktop-only
    }

    fn is_desktop(&self) -> bool {
        true // Desktop-only
    }

    fn is_touch_active(&self) -> bool {
        self.input(|i| i.any_touches())
    }
}
