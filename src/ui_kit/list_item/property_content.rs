//! PropertyContent - Key-value pair display for list items

use crate::localization as l10n;
use egui::{Color32, Response, Role, Ui, WidgetInfo};

use super::content::{ContentContext, ListItemContent};

/// Key-value pair content for displaying properties
///
/// # Example
///
/// ```
/// # use egui::Color32;
/// # use egui_charts::ui_kit::list_item::{ListItem, PropertyContent};
/// # egui::__run_test_ui(|ui| {
/// ListItem::new()
///     .show(ui, PropertyContent::new("Price", "$1,234.56")
///         .value_color(Color32::GREEN));
/// # });
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

        // Label is always subdued (noninteractive)
        let label_color = egui_visuals.weak_text_color();

        // Value color (use ListVisuals text_color or custom)
        let value_color = self
            .value_color
            .unwrap_or_else(|| context.visuals.text_color(&egui_visuals));

        let content_rect = context.rect;

        // Calculate label width (reserved for future use when we implement two-column layout)
        let _label_width = self.label_width.unwrap_or_else(|| {
            // Default to ~40% of available width
            content_rect.width() * 0.4
        });

        let label = l10n::text(ui.ctx(), &self.label);
        let value = l10n::text(ui.ctx(), &self.value);
        let translated_label = format!("{label}: {value}");
        context
            .response
            .widget_info(|| WidgetInfo::labeled(Role::Label, ui.is_enabled(), &translated_label));

        if ui.is_rect_visible(content_rect) {
            let y_center = content_rect.center().y;

            // Draw label
            ui.painter().text(
                egui::pos2(content_rect.min.x, y_center),
                egui::Align2::LEFT_CENTER,
                &label,
                egui::TextStyle::Body.resolve(ui.style()),
                label_color,
            );

            // Draw value (right-aligned)
            ui.painter().text(
                egui::pos2(content_rect.max.x, y_center),
                egui::Align2::RIGHT_CENTER,
                &value,
                egui::TextStyle::Body.resolve(ui.style()),
                value_color,
            );
        }

        context.response.clone()
    }
}
