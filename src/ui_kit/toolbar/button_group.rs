//! Button Group Component
//!
//! Groups toolbar buttons with optional separators between groups.

use egui::{Align, Layout, Ui};

use crate::theme::ThemeExt;

/// A group of toolbar buttons with optional separator
///
/// # Example
/// ```
/// # use egui_charts::ui_kit::toolbar::ButtonGroup;
/// # egui::__run_test_ui(|ui| {
/// ButtonGroup::new()
///     .separator_after()
///     .show(ui, |ui| {
///         ui.button("Action 1");
///         ui.button("Action 2");
///     });
/// # });
/// ```
pub struct ButtonGroup {
    separator_after: bool,
    separator_before: bool,
}

impl Default for ButtonGroup {
    fn default() -> Self {
        Self::new()
    }
}

impl ButtonGroup {
    /// Create a new button group
    pub fn new() -> Self {
        Self {
            separator_after: false,
            separator_before: false,
        }
    }

    /// Add a separator after this group
    #[must_use]
    pub fn separator_after(mut self) -> Self {
        self.separator_after = true;
        self
    }

    /// Add a separator before this group
    #[must_use]
    pub fn separator_before(mut self) -> Self {
        self.separator_before = true;
        self
    }

    /// Show the button group along the parent's layout direction
    ///
    /// The buttons sit side by side in a horizontal parent and stacked in a vertical one. Each
    /// separator is a 1 px line across the group, with a small gap on either side. Returns the
    /// result of the content closure.
    pub fn show<R>(self, ui: &mut Ui, content: impl FnOnce(&mut Ui) -> R) -> R {
        let gap = ui.active_theme().spacing.sm;
        let stroke = ui.visuals().widgets.noninteractive.bg_stroke;
        let slot = 2.0 * gap + stroke.width;
        let horizontal = ui.layout().main_dir().is_horizontal();
        let body = |ui: &mut Ui| {
            if self.separator_before {
                ui.add_space(slot);
            }
            let result = content(ui);
            let group = ui.min_rect();
            // Half a stroke in, so the line covers whole pixels.
            let offset = gap + stroke.width / 2.0;
            if horizontal {
                if self.separator_before {
                    ui.painter()
                        .vline(group.left() + offset, group.y_range(), stroke);
                }
                if self.separator_after {
                    ui.painter()
                        .vline(group.right() + offset, group.y_range(), stroke);
                    ui.expand_to_include_x(group.right() + slot);
                }
            } else {
                if self.separator_before {
                    ui.painter()
                        .hline(group.x_range(), group.top() + offset, stroke);
                }
                if self.separator_after {
                    ui.painter()
                        .hline(group.x_range(), group.bottom() + offset, stroke);
                    ui.expand_to_include_y(group.bottom() + slot);
                }
            }
            result
        };
        if horizontal {
            ui.horizontal(body).inner
        } else {
            ui.vertical(body).inner
        }
    }
}

/// Helper for common toolbar layout patterns
pub struct ToolbarLayout;

impl ToolbarLayout {
    /// Create a left-right split layout
    ///
    /// Renders left content normally (left-to-right), then renders right content
    /// in right-to-left layout at the end.
    ///
    /// # Example
    /// ```
    /// # use egui_charts::ui_kit::toolbar::ToolbarLayout;
    /// # egui::__run_test_ui(|ui| {
    /// ToolbarLayout::left_right(ui,
    ///     |ui| {
    ///         ui.button("Left 1");
    ///         ui.button("Left 2");
    ///     },
    ///     |ui| {
    ///         ui.button("Right 1");
    ///     },
    /// );
    /// # });
    /// ```
    pub fn left_right<L, R>(
        ui: &mut Ui,
        left_content: impl FnOnce(&mut Ui) -> L,
        right_content: impl FnOnce(&mut Ui) -> R,
    ) -> (L, R) {
        let left = left_content(ui);

        let right = ui
            .with_layout(Layout::right_to_left(Align::Center), |ui| right_content(ui))
            .inner;

        (left, right)
    }

    /// Create a centered layout with optional left/right sections
    ///
    /// Useful for toolbars where the main content is centered.
    pub fn centered<C, L, R>(
        ui: &mut Ui,
        left_content: impl FnOnce(&mut Ui) -> L,
        center_content: impl FnOnce(&mut Ui) -> C,
        right_content: impl FnOnce(&mut Ui) -> R,
    ) -> (L, C, R) {
        let left = left_content(ui);

        let center = ui
            .with_layout(
                Layout::centered_and_justified(ui.layout().main_dir()),
                |ui| center_content(ui),
            )
            .inner;

        let right = ui
            .with_layout(Layout::right_to_left(Align::Center), |ui| right_content(ui))
            .inner;

        (left, center, right)
    }
}
