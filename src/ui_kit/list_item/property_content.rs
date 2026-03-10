//! PropertyContent - Key-value pair display for list items

use egui::{Color32, Response, Ui, Vec2};

use super::content::{ContentContext, ListItemContent};
use crate::tokens::DESIGN_TOKENS;

/// Key-value pair content for displaying properties
///
/// # Example
///
/// ```ignore
/// ListItem::new()
///     .show(ui, PropertyContent::new("Price", "$1,234.56")
///         .value_color(Color32::GREEN));
/// ```
pub struct PropertyContent {
    label: String,
    value: String,
    value_color: Option<Color32>,
    label_width: Option<f32>,
}

impl PropertyContent {
    /// Create new property content with label and value
    pub fn new(label: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            value_color: None,
            label_width: None,
        }
    }

    /// Set a custom color for the value
    #[must_use]
    pub fn value_color(mut self, color: Color32) -> Self {
        self.value_color = Some(color);
        self
    }

    /// Set a fixed width for the label column
    #[must_use]
    pub fn label_width(mut self, width: f32) -> Self {
        self.label_width = Some(width);
        self
    }
}

impl ListItemContent for PropertyContent {
    fn ui(self, ui: &mut Ui, context: &ContentContext<'_>) -> Response {
        let egui_visuals = ui.style().visuals.clone();

        // Use ListVisuals for color computation (rerun-inspired)
        // Label is always subdued (noninteractive)
        let label_color = egui_visuals.widgets.noninteractive.fg_stroke.color;

        // Value color (use ListVisuals text_color or custom)
        let value_color = self
            .value_color
            .unwrap_or_else(|| context.visuals.text_color(&egui_visuals));

        let content_rect = context
            .rect
            .shrink2(Vec2::new(DESIGN_TOKENS.spacing.sm, 0.0));

        // Calculate label width (reserved for future use when we implement two-column layout)
        let _label_width = self.label_width.unwrap_or_else(|| {
            // Default to ~40% of available width
            content_rect.width() * 0.4
        });

        // Allocate response
        let sense = if context.list_item.interactive {
            egui::Sense::click()
        } else {
            egui::Sense::hover()
        };

        let (rect, response) = ui.allocate_exact_size(context.rect.size(), sense);

        if ui.is_rect_visible(rect) {
            let y_center = content_rect.center().y;

            // Draw label
            ui.painter().text(
                egui::pos2(content_rect.min.x, y_center),
                egui::Align2::LEFT_CENTER,
                &self.label,
                egui::TextStyle::Body.resolve(ui.style()),
                label_color,
            );

            // Draw value (right-aligned)
            ui.painter().text(
                egui::pos2(content_rect.max.x, y_center),
                egui::Align2::RIGHT_CENTER,
                &self.value,
                egui::TextStyle::Body.resolve(ui.style()),
                value_color,
            );
        }

        response
    }
}
