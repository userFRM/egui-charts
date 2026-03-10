//! Section Component
//!
//! A collapsible section with header and animated expand/collapse.
//!
//! # Usage
//!
//! ```ignore
//! use open_trading_charts::ui_kit::section::Section;
//!
//! Section::new("settings_section", "Settings")
//!     .default_open(true)
//!     .show(ui, |ui| {
//!         ui.label("Section content here");
//!     });
//! ```

use egui::{CollapsingResponse, Id, RichText, Ui, Vec2};

use crate::icons::Icon;
use crate::tokens::DESIGN_TOKENS;

/// A collapsible section with header
pub struct Section {
    id: Id,
    title: String,
    default_open: bool,
    icon: Option<&'static Icon>,
    strong_header: bool,
}

impl Section {
    /// Create a new section
    pub fn new(id: impl std::hash::Hash, title: impl Into<String>) -> Self {
        Self {
            id: Id::new(id),
            title: title.into(),
            default_open: true,
            icon: None,
            strong_header: true,
        }
    }

    /// Set whether the section is open by default
    pub fn default_open(mut self, open: bool) -> Self {
        self.default_open = open;
        self
    }

    /// Set an icon for the section header
    pub fn icon(mut self, icon: &'static Icon) -> Self {
        self.icon = Some(icon);
        self
    }

    /// Set whether the header text should be bold
    pub fn strong_header(mut self, strong: bool) -> Self {
        self.strong_header = strong;
        self
    }

    /// Show the section
    pub fn show<R>(
        self,
        ui: &mut Ui,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> CollapsingResponse<R> {
        let header_text = if self.strong_header {
            RichText::new(&self.title).strong()
        } else {
            RichText::new(&self.title)
        };

        let mut header = egui::CollapsingHeader::new(header_text)
            .id_salt(self.id)
            .default_open(self.default_open);

        // If we have an icon, we need to customize the header
        if let Some(icon) = self.icon {
            header = header.icon(move |ui, openness, response| {
                // Draw the icon
                let icon_size = DESIGN_TOKENS.sizing.icon_sm;
                let tint = ui.style().visuals.widgets.noninteractive.fg_stroke.color;

                // Position icon to the left of the text
                let icon_rect = egui::Rect::from_min_size(
                    response.rect.left_center()
                        - Vec2::new(icon_size + DESIGN_TOKENS.spacing.sm, icon_size / 2.0),
                    Vec2::splat(icon_size),
                );

                icon.as_image_tinted(Vec2::splat(icon_size), tint)
                    .paint_at(ui, icon_rect);

                // Draw default collapse arrow
                egui::collapsing_header::paint_default_icon(ui, openness, response);
            });
        }

        header.show(ui, add_contents)
    }
}

/// Extension trait for `egui::Ui` to easily create sections
pub trait SectionExt {
    /// Create a simple section with just a title
    fn section<R>(
        &mut self,
        title: impl Into<String>,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> CollapsingResponse<R>;

    /// Create a section that's closed by default
    fn collapsed_section<R>(
        &mut self,
        title: impl Into<String>,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> CollapsingResponse<R>;
}

impl SectionExt for Ui {
    fn section<R>(
        &mut self,
        title: impl Into<String>,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> CollapsingResponse<R> {
        let title = title.into();
        Section::new(&title, &title)
            .default_open(true)
            .show(self, add_contents)
    }

    fn collapsed_section<R>(
        &mut self,
        title: impl Into<String>,
        add_contents: impl FnOnce(&mut Ui) -> R,
    ) -> CollapsingResponse<R> {
        let title = title.into();
        Section::new(&title, &title)
            .default_open(false)
            .show(self, add_contents)
    }
}
