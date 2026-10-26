//! A named save form and catalog for study and drawing templates.

use super::TemplateStore;
use crate::localization as l10n;
use crate::services::StoreKind;
use crate::ui_kit::modal::Modal;
use serde::{Serialize, de::DeserializeOwned};

/// Template management opened by a host's study or drawing toolbar intents. The host supplies
/// the current value, handles the returned loaded value and supplies durable storage if desired.
pub struct TemplateDialog<T> {
    /// Host-readable catalog and storage connection.
    pub templates: TemplateStore<T>,
    /// Whether the dialog is visible.
    pub is_open: bool,
    current: Option<T>,
    name: String,
}

impl<T: Clone + Serialize + DeserializeOwned> TemplateDialog<T> {
    /// A closed dialog in the given template namespace.
    pub fn new(kind: StoreKind) -> Self {
        Self {
            templates: TemplateStore::new(kind),
            is_open: false,
            current: None,
            name: String::new(),
        }
    }

    /// Open the name form for a snapshot of the current host settings.
    pub fn open(&mut self, current: T) {
        self.current = Some(current);
        self.name.clear();
        self.is_open = true;
    }

    /// Open the catalog without a current value to save. Loading still returns a value for the host.
    pub fn open_catalog(&mut self) {
        self.current = None;
        self.name.clear();
        self.is_open = true;
    }

    /// Poll storage and show named Save/Replace, Load and Delete controls. A loaded value is
    /// returned without mutating the host chart; cancellation never applies the pending snapshot.
    pub fn show(&mut self, ctx: &egui::Context, title: &str) -> Option<T> {
        self.templates.poll(ctx);
        if !self.is_open {
            return None;
        }
        let mut loaded = None;
        let mut open = self.is_open;
        Modal::new(title, title)
            .width(440.0)
            .show(ctx, &mut open, |ui| {
                egui::ScrollArea::vertical()
                    .max_height(ui.available_height())
                    .show(ui, |ui| {
                        let ready = self.templates.ready();
                        if self.current.is_some() {
                            ui.label(l10n::text(ui.ctx(), "Template name"));
                            ui.add(
                                egui::TextEdit::singleline(&mut self.name)
                                    .desired_width(ui.available_width()),
                            );
                            let label = if self.templates.entries().contains_key(self.name.trim()) {
                                "Replace"
                            } else {
                                "Save"
                            };
                            if ui
                                .add_enabled(
                                    ready && !self.name.trim().is_empty(),
                                    egui::Button::new(l10n::text(ui.ctx(), label)),
                                )
                                .clicked()
                                && let Some(current) = &self.current
                                && let Err(error) = self.templates.save(&self.name, current.clone())
                            {
                                self.templates.error = Some(error);
                            }
                        }
                        ui.separator();
                        let names: Vec<_> = self.templates.entries().keys().cloned().collect();
                        if names.is_empty() && ready {
                            ui.label(l10n::text(ui.ctx(), "No saved templates"));
                        }
                        for name in names {
                            ui.add(egui::Label::new(l10n::text(ui.ctx(), &name)).truncate())
                                .on_hover_text(l10n::text(ui.ctx(), &name));
                            ui.horizontal(|ui| {
                                if ui
                                    .add_enabled(
                                        ready,
                                        egui::Button::new(l10n::text(ui.ctx(), "Load")),
                                    )
                                    .clicked()
                                {
                                    loaded = self.templates.entries().get(&name).cloned();
                                }
                                if ui
                                    .add_enabled(
                                        ready,
                                        egui::Button::new(l10n::text(ui.ctx(), "Delete")),
                                    )
                                    .clicked()
                                    && let Err(error) = self.templates.delete(&name)
                                {
                                    self.templates.error = Some(error);
                                }
                            });
                        }
                        if let Some(error) = &self.templates.error {
                            ui.colored_label(
                                ui.visuals().error_fg_color,
                                l10n::text(ui.ctx(), error),
                            );
                            if !self.templates.ready()
                                && ui
                                    .button(l10n::text(ui.ctx(), "Retry loading templates"))
                                    .clicked()
                            {
                                self.templates.reload();
                            }
                        } else if !ready {
                            ui.label(l10n::text(ui.ctx(), "Loading templates…"));
                        }
                    });
            });
        self.is_open = open && loaded.is_none();
        loaded
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn catalog_actions_remain_visible_with_long_names_in_short_and_narrow_windows() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for size in [egui::vec2(1440.0, 900.0), egui::vec2(420.0, 360.0)] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, size);
                let mut dialog = TemplateDialog::new(StoreKind::StudyTemplates);
                for index in 0..20 {
                    dialog
                        .templates
                        .save(&format!("{index:02} {}", "Long name ".repeat(20)), index)
                        .unwrap();
                }
                dialog.open(42);
                for frame in 0..3 {
                    let output = ctx.run_ui(
                        egui::RawInput {
                            screen_rect: Some(screen),
                            ..Default::default()
                        },
                        |ui| {
                            dialog.show(ui.ctx(), "Study templates");
                        },
                    );
                    let shapes = output.shapes.clone();
                    output.drop_without_applying_deltas();
                    if frame > 0 {
                        let area = ctx
                            .memory(|memory| memory.area_rect(egui::Id::unique("Study templates")))
                            .unwrap();
                        assert!(
                            screen.shrink(23.0).contains_rect(area),
                            "{size:?}: {area:?}"
                        );
                        for label in ["Save", "Load", "Delete"] {
                            let (clip, rect) = shapes
                                .iter()
                                .find_map(|shape| match &shape.shape {
                                    egui::Shape::Text(text) if text.galley.text() == label => {
                                        Some((
                                            shape.clip_rect,
                                            egui::Rect::from_min_size(text.pos, text.galley.size()),
                                        ))
                                    }
                                    _ => None,
                                })
                                .expect("catalog action is painted");
                            assert!(
                                screen.contains_rect(rect) && clip.contains_rect(rect),
                                "{label}: {rect:?}"
                            );
                        }
                    }
                }
            }
        }
    }
}
