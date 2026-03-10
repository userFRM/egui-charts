//! WidgetExt - Extension traits for egui widgets
//!
//! Provides convenience methods for common widget variations following rerun patterns.
//!
//! # Usage
//!
//! ```ignore
//! use egui_charts::ui_kit::ext::WidgetExt;
//!
//! let button = egui::Button::new("Save")
//!     .fill_width()
//!     .min_height(36.0);
//! ```

use egui::{Button, TextEdit, Vec2};

/// Extension trait for `egui::Button`
pub trait ButtonWidgetExt<'a>: Sized {
    /// Make the button fill the available width
    fn fill_width(self) -> Self;

    /// Set a minimum width for the button
    fn min_width(self, width: f32) -> Self;

    /// Set a minimum height for the button
    fn min_height(self, height: f32) -> Self;

    /// Set exact dimensions
    fn exact_size(self, size: Vec2) -> Self;
}

impl<'a> ButtonWidgetExt<'a> for Button<'a> {
    fn fill_width(self) -> Self {
        self.min_size(Vec2::new(f32::INFINITY, 0.0))
    }

    fn min_width(self, width: f32) -> Self {
        self.min_size(Vec2::new(width, 0.0))
    }

    fn min_height(self, height: f32) -> Self {
        self.min_size(Vec2::new(0.0, height))
    }

    fn exact_size(self, size: Vec2) -> Self {
        self.min_size(size)
    }
}

/// Extension trait for `egui::TextEdit`
pub trait TextEditWidgetExt<'a>: Sized {
    /// Make the text edit fill available width
    fn fill_width(self) -> Self;

    /// Set a fixed width
    fn fixed_width(self, width: f32) -> Self;

    /// Add a character limit
    fn char_limit(self, limit: usize) -> Self;
}

impl<'a> TextEditWidgetExt<'a> for TextEdit<'a> {
    fn fill_width(self) -> Self {
        self.desired_width(f32::INFINITY)
    }

    fn fixed_width(self, width: f32) -> Self {
        self.desired_width(width)
    }

    fn char_limit(self, limit: usize) -> Self {
        self.char_limit(limit)
    }
}

/// Generic extension trait for widgets that support rounding
pub trait RoundingExt: Sized {
    /// Apply standard button rounding
    fn button_rounding(self) -> Self;

    /// Apply panel rounding
    fn panel_rounding(self) -> Self;

    /// Apply no rounding
    fn no_rounding(self) -> Self;

    /// Apply pill rounding (fully rounded)
    fn pill_rounding(self) -> Self;
}

// Note: Implementation for specific widget types that support corner_radius
// would be added here, but egui's Button::corner_radius returns Self,
// so we can chain these if needed.
