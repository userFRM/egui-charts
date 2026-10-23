//! Section Component
//!
//! A collapsible section with header and animated expand/collapse.
//!
//! # Usage
//!
//! ```
//! use egui_charts::ui_kit::section::Section;
//!
//! # egui::__run_test_ui(|ui| {
//! Section::new("settings_section", "Settings")
//!     .default_open(true)
//!     .show(ui, |ui| {
//!         ui.label("Section content here");
//!     });
//! # });
//! ```

use egui::text::LayoutJob;
use egui::{Align, CollapsingResponse, Id, RichText, TextStyle, Ui, Vec2, WidgetText};

use crate::icons::{Icon, icons};
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::emath::GuiRounding;

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
    pub fn new(id: impl std::hash::Hash + std::fmt::Debug, title: impl Into<String>) -> Self {
        Self {
            id: Id::unique(id),
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
        let theme = ui.active_theme();
        let mut text = RichText::new(&self.title).size(theme.text.small);
        if self.strong_header {
            text = text.strong();
        }
        let icon_size = DESIGN_TOKENS.sizing.icon_sm;

        let header_text: WidgetText = if self.icon.is_some() {
            // Reserve the icon and its gap before the title, inside the header's own text.
            let mut job = LayoutJob::default();
            text.append_to(
                &mut job,
                ui.style(),
                TextStyle::Button.into(),
                Align::Center,
            );
            job.sections[0].leading_space = icon_size + theme.spacing.lg;
            job.into()
        } else {
            text.into()
        };

        let header = egui::CollapsingHeader::new(header_text)
            .id_salt(self.id)
            .default_open(self.default_open);

        let header = header.icon(move |ui, openness, response| {
            let tint = ui.visuals().widgets.noninteractive.fg_stroke.color;
            let chevron = if openness > 0.5 {
                &icons::CHEVRON_DOWN
            } else {
                &icons::CHEVRON_RIGHT
            };
            let rect = egui::Rect::from_center_size(response.rect.center(), Vec2::splat(icon_size))
                .round_to_pixels(ui.pixels_per_point());
            chevron
                .as_image_tinted(rect.size(), tint)
                .paint_at(ui, rect);
            if let Some(icon) = self.icon {
                // The title reserves this icon slot after the disclosure control.
                let left = response.rect.center().x + ui.spacing().indent / 2.0;
                let rect = egui::Rect::from_min_size(
                    egui::pos2(left, response.rect.center().y - icon_size / 2.0),
                    Vec2::splat(icon_size),
                )
                .round_to_pixels(ui.pixels_per_point());
                icon.as_image_tinted(rect.size(), tint).paint_at(ui, rect);
            }
        });

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
