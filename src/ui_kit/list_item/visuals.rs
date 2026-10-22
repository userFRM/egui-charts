//! ListVisuals - Color computation methods for list items (rerun-inspired)
//!
//! Instead of pre-computing colors, this struct provides methods that compute
//! colors based on the current state, following rerun's pattern.

use egui::{Color32, Visuals};

/// Visual state information for list items with color computation methods
///
/// This struct follows rerun's pattern of computing colors via methods
/// rather than storing pre-computed values. This allows for more flexible
/// color computation based on complex state combinations.
#[derive(Clone, Debug)]
pub struct ListVisuals {
    /// Whether the item is hovered
    pub hovered: bool,
    /// Whether the item is selected
    pub selected: bool,
    /// Whether the item is in active state (e.g., expanded)
    pub active: bool,
    /// Whether the item is interactive (clickable)
    pub interactive: bool,
    /// Whether the text should be rendered with strong emphasis
    pub strong: bool,
    /// For collapsible items, the current openness (0.0 = closed, 1.0 = open)
    pub openness: Option<f32>,
}

impl Default for ListVisuals {
    fn default() -> Self {
        Self {
            hovered: false,
            selected: false,
            active: false,
            interactive: true,
            strong: false,
            openness: None,
        }
    }
}

impl ListVisuals {
    /// Create a new ListVisuals with default state
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the hovered state
    #[must_use]
    pub fn with_hovered(mut self, hovered: bool) -> Self {
        self.hovered = hovered;
        self
    }

    /// Set the selected state
    #[must_use]
    pub fn with_selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Set the active state
    #[must_use]
    pub fn with_active(mut self, active: bool) -> Self {
        self.active = active;
        self
    }

    /// Set the interactive state
    #[must_use]
    pub fn with_interactive(mut self, interactive: bool) -> Self {
        self.interactive = interactive;
        self
    }

    /// Set the strong emphasis state
    #[must_use]
    pub fn with_strong(mut self, strong: bool) -> Self {
        self.strong = strong;
        self
    }

    /// Set the openness for collapsible items
    #[must_use]
    pub fn with_openness(mut self, openness: f32) -> Self {
        self.openness = Some(openness);
        self
    }

    /// Compute background color based on state
    ///
    /// Returns None if no background should be drawn (transparent).
    pub fn bg_color(&self, visuals: &Visuals) -> Option<Color32> {
        if self.selected {
            Some(visuals.selection.bg_fill)
        } else if self.active {
            Some(visuals.widgets.active.bg_fill)
        } else if self.hovered && self.interactive {
            Some(visuals.widgets.hovered.bg_fill)
        } else {
            None
        }
    }

    /// Compute text color based on state
    pub fn text_color(&self, visuals: &Visuals) -> Color32 {
        if self.selected {
            visuals.selection.stroke.color
        } else if !self.interactive {
            visuals.widgets.noninteractive.fg_stroke.color
        } else if self.hovered || self.active || self.strong {
            visuals.widgets.hovered.fg_stroke.color
        } else {
            visuals.widgets.inactive.fg_stroke.color
        }
    }

    /// Compute icon tint color based on state
    pub fn icon_tint(&self, visuals: &Visuals) -> Color32 {
        if self.selected {
            visuals.selection.stroke.color
        } else if (self.hovered && self.interactive) || self.active {
            visuals.widgets.hovered.fg_stroke.color
        } else {
            visuals.widgets.noninteractive.fg_stroke.color
        }
    }

    /// Compute icon tint for interactive icons within list items
    ///
    /// Use this for icons that have their own hover state (e.g., action buttons)
    pub fn interactive_icon_tint(&self, icon_hovered: bool, visuals: &Visuals) -> Color32 {
        if icon_hovered && !self.selected {
            visuals.widgets.hovered.fg_stroke.color
        } else {
            self.icon_tint(visuals)
        }
    }

    /// Check if this item is collapsible
    pub fn is_collapsible(&self) -> bool {
        self.openness.is_some()
    }

    /// Check if this item is collapsed (for collapsible items)
    pub fn is_collapsed(&self) -> bool {
        self.openness.is_none_or(|o| o <= 0.0)
    }

    /// Get the current openness value (0.0 if not collapsible)
    pub fn openness_value(&self) -> f32 {
        self.openness.unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collapsible() {
        let non_collapsible = ListVisuals::new();
        assert!(!non_collapsible.is_collapsible());
        assert!(non_collapsible.is_collapsed());

        let collapsed = ListVisuals::new().with_openness(0.0);
        assert!(collapsed.is_collapsible());
        assert!(collapsed.is_collapsed());

        let expanded = ListVisuals::new().with_openness(1.0);
        assert!(expanded.is_collapsible());
        assert!(!expanded.is_collapsed());
    }
}
