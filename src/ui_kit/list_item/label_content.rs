//! LabelContent - Simple text content with optional icon

use crate::localization as l10n;
use crate::theme::ThemeExt;
use egui::{Response, RichText, Role, Ui, Vec2, WidgetInfo};

use crate::icons::Icon;
use crate::styles::icons;

use super::content::{ContentContext, ListItemContent};

/// Simple text content with optional icon
///
/// # Example
///
/// ```
/// # use egui_charts::icons::icons;
/// # use egui_charts::ui_kit::list_item::{LabelContent, ListItem};
/// # egui::__run_test_ui(|ui| {
/// ListItem::new()
///     .show(ui, LabelContent::new("Hello World").with_icon(&icons::SETTINGS));
/// # });
/// ```
pub struct LabelContent<'a> {
    text: String,
    icon: Option<&'a Icon>,
    subdued: bool,
    italics: bool,
    strong: bool,
    icon_size: f32,
}

impl<'a> LabelContent<'a> {
    /// Create new label content with the given text
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            icon: None,
            subdued: false,
            italics: false,
            strong: false,
            icon_size: icons::SM,
        }
    }

    /// Add an icon to the label
    #[must_use]
    pub fn with_icon(mut self, icon: &'a Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Make the text subdued (secondary color)
    #[must_use]
    pub fn subdued(mut self, subdued: bool) -> Self {
        self.subdued = subdued;
        self
    }

    /// Make the text italic
    #[must_use]
    pub fn italics(mut self, italics: bool) -> Self {
        self.italics = italics;
        self
    }

    /// Make the text bold
    #[must_use]
    pub fn strong(mut self, strong: bool) -> Self {
        self.strong = strong;
        self
    }

    /// Set the icon size
    #[must_use]
    pub fn icon_size(mut self, size: f32) -> Self {
        self.icon_size = size;
        self
    }
}

impl<'a> ListItemContent for LabelContent<'a> {
    fn ui(self, ui: &mut Ui, context: &ContentContext<'_>) -> Response {
        let egui_visuals = ui.style().visuals.clone();

        let mut list_visuals = context.visuals.clone();
        list_visuals.strong = self.strong;

        // Determine text color using ListVisuals
        let text_color = if self.subdued {
            egui_visuals.weak_text_color()
        } else {
            list_visuals.text_color(&egui_visuals)
        };

        // Determine icon color using ListVisuals
        let icon_color = list_visuals.icon_tint(&egui_visuals);

        // Calculate layout
        let icon_width = if self.icon.is_some() {
            self.icon_size + ui.active_theme().spacing.sm
        } else {
            0.0
        };

        let content_rect = context.rect;
        let is_menu = ui.stack().contained_in(egui::UiKind::Menu);
        let translated_label = l10n::text(ui.ctx(), &self.text);
        context.response.widget_info(|| {
            WidgetInfo::labeled(
                if is_menu {
                    Role::MenuItem
                } else if context.response.sense.senses_click() {
                    Role::ListBoxOption
                } else {
                    Role::ListItem
                },
                context.response.enabled(),
                &translated_label,
            )
        });

        if !is_menu && context.response.sense.senses_click() {
            ui.ctx()
                .accesskit_node_builder(context.response.id, |node| {
                    node.set_selected(context.selected)
                });
        }

        if ui.is_rect_visible(content_rect) {
            // Draw icon if present
            if let Some(icon) = self.icon {
                let icon_rect = egui::Rect::from_min_size(
                    content_rect.min
                        + Vec2::new(0.0, (content_rect.height() - self.icon_size) / 2.0),
                    Vec2::splat(self.icon_size),
                );
                icon.as_image_tinted(Vec2::splat(self.icon_size), icon_color)
                    .paint_at(ui, icon_rect);
            }

            // Draw text
            let text_pos = egui::pos2(content_rect.min.x + icon_width, content_rect.center().y);

            let mut rich_text = RichText::new(l10n::text(ui.ctx(), &self.text)).color(text_color);
            if self.italics {
                rich_text = rich_text.italics();
            }
            if self.strong {
                rich_text = rich_text.strong();
            }

            ui.painter().text(
                text_pos,
                egui::Align2::LEFT_CENTER,
                rich_text.text(),
                egui::TextStyle::Body.resolve(ui.style()),
                text_color,
            );
        }

        context.response.clone()
    }
}
