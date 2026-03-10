//! Button Group Component
//!
//! Groups toolbar buttons with optional separators between groups.

use egui::{Align, Layout, Ui};

use crate::tokens::DESIGN_TOKENS;

/// A group of toolbar buttons with optional separator
///
/// # Example
/// ```ignore
/// ButtonGroup::new()
///     .separator_after()
///     .show(ui, |ui| {
///         ui.button("Action 1");
///         ui.button("Action 2");
///     });
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

    /// Show the button group
    ///
    /// Returns the result of the content closure.
    pub fn show<R>(self, ui: &mut Ui, content: impl FnOnce(&mut Ui) -> R) -> R {
        if self.separator_before {
            render_toolbar_separator(ui);
        }

        let result = content(ui);

        if self.separator_after {
            render_toolbar_separator(ui);
        }

        result
    }
}

/// Helper to render a toolbar separator
fn render_toolbar_separator(ui: &mut Ui) {
    ui.add_space(DESIGN_TOKENS.spacing.sm);
    let stroke = ui.style().visuals.widgets.noninteractive.bg_stroke;
    let available = ui.available_rect_before_wrap();
    let x = available.left();
    ui.painter().vline(x, available.y_range(), stroke);
    ui.add_space(DESIGN_TOKENS.spacing.sm);
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
    /// ```ignore
    /// ToolbarLayout::left_right(ui,
    ///     |ui| {
    ///         ui.button("Left 1");
    ///         ui.button("Left 2");
    ///     },
    ///     |ui| {
    ///         ui.button("Right 1");
    ///     },
    /// );
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
