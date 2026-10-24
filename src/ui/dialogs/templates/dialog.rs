//! A named save form and catalog for study and drawing templates.

use super::TemplateStore;
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
                let ready = self.templates.ready();
                if self.current.is_some() {
                    ui.label("Template name");
                    ui.text_edit_singleline(&mut self.name);
                    let label = if self.templates.entries().contains_key(self.name.trim()) {
                        "Replace"
                    } else {
                        "Save"
                    };
                    if ui
                        .add_enabled(
                            ready && !self.name.trim().is_empty(),
                            egui::Button::new(label),
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
                    ui.label("No saved templates");
                }
                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        for name in names {
                            ui.horizontal(|ui| {
                                ui.label(&name);
                                if ui.add_enabled(ready, egui::Button::new("Load")).clicked() {
                                    loaded = self.templates.entries().get(&name).cloned();
                                }
                                if ui.add_enabled(ready, egui::Button::new("Delete")).clicked()
                                    && let Err(error) = self.templates.delete(&name)
                                {
                                    self.templates.error = Some(error);
                                }
                            });
                        }
                    });
                if let Some(error) = &self.templates.error {
                    ui.colored_label(ui.visuals().error_fg_color, error);
                    if !self.templates.ready() && ui.button("Retry loading templates").clicked() {
                        self.templates.reload();
                    }
                } else if !ready {
                    ui.label("Loading templates…");
                }
            });
        self.is_open = open && loaded.is_none();
        loaded
    }
}
