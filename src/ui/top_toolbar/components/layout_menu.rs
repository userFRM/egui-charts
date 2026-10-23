//! Host-owned named layout actions. No storage or account service is assumed.

use crate::theme::ThemeExt;
use chrono::{DateTime, Utc};
use egui::{Color32, Ui};

/// Layout information
#[derive(Debug, Clone)]
pub struct LayoutInfo {
    pub name: String,
    pub created_at: DateTime<Utc>,
    pub modified_at: DateTime<Utc>,
    pub is_default: bool,
    pub is_cloud_synced: bool,
}

impl LayoutInfo {
    pub fn new(name: impl Into<String>) -> Self {
        let now = Utc::now();
        Self {
            name: name.into(),
            created_at: now,
            modified_at: now,
            is_default: false,
            is_cloud_synced: false,
        }
    }

    pub fn default_layout() -> Self {
        Self {
            name: "Default".to_string(),
            created_at: Utc::now(),
            modified_at: Utc::now(),
            is_default: true,
            is_cloud_synced: false,
        }
    }
}

/// Configuration for layout menu
#[derive(Debug, Clone)]
pub struct LayoutMenuConfig {
    pub menu_width: f32,
    pub item_height: f32,
    pub bg_color: Color32,
    pub hover_color: Color32,
    pub text_color: Color32,
    pub muted_color: Color32,
    pub accent_color: Color32,
}

impl Default for LayoutMenuConfig {
    fn default() -> Self {
        // Colors are TRANSPARENT to signal that colors should be fetched from
        // ui.style().visuals at render time for proper theme support.
        Self {
            menu_width: 220.0,
            item_height: 32.0,
            bg_color: Color32::TRANSPARENT,
            hover_color: Color32::TRANSPARENT,
            text_color: Color32::TRANSPARENT,
            muted_color: Color32::TRANSPARENT,
            accent_color: Color32::TRANSPARENT,
        }
    }
}

/// Action from layout menu
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutAction {
    None,
    /// Select a tiled or tabbed chart arrangement.
    SelectGrid(crate::widget::ChartLayoutMode),
    /// Open the layout management menu
    OpenMenu,
    Save,
    SaveAs(String),
    DownloadImage,
    CreateNew,
    Open(String),
    Delete(String),
    Rename(String, String),
    SetDefault(String),
}

/// Named layout menu. The host supplies the available layouts and handles returned intents.
pub struct LayoutMenu {
    /// Name of the current layout.
    pub curr_layout: Option<String>,
    /// Host-owned layouts available to open.
    pub layouts: Vec<LayoutInfo>,
    /// Menu sizing and optional colour overrides.
    pub config: LayoutMenuConfig,
    editing: Option<bool>,
    new_name: String,
}

impl Default for LayoutMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutMenu {
    /// Creates an unnamed layout menu.
    pub fn new() -> Self {
        Self {
            curr_layout: Some("Unnamed".into()),
            layouts: Vec::new(),
            config: LayoutMenuConfig::default(),
            editing: None,
            new_name: String::new(),
        }
    }

    /// Adds or refreshes a host-owned layout.
    pub fn add_layout(&mut self, layout: LayoutInfo) {
        if let Some(old) = self.layouts.iter_mut().find(|old| old.name == layout.name) {
            *old = layout;
        } else {
            self.layouts.push(layout);
        }
    }

    /// Width of the named trigger, for toolbar overflow measurement.
    pub fn width(&self, ui: &Ui) -> f32 {
        ui.fonts_mut(|f| {
            f.layout_no_wrap(
                self.curr_layout.as_deref().unwrap_or("Unnamed").to_owned(),
                egui::FontId::proportional(14.0),
                ui.visuals().text_color(),
            )
            .size()
            .x
        }) + 32.0
    }

    /// Shows the current name and an anchored menu. Actions do not mutate the host's catalog.
    pub fn show(&mut self, ui: &mut Ui) -> LayoutAction {
        let (rect, button) =
            ui.allocate_exact_size(egui::vec2(self.width(ui), 28.0), egui::Sense::click());
        if button.hovered() {
            ui.painter()
                .rect_filled(rect, 2.0, ui.visuals().widgets.hovered.bg_fill);
        }
        ui.painter().text(
            rect.left_center() + egui::vec2(4.0, 0.0),
            egui::Align2::LEFT_CENTER,
            self.curr_layout.as_deref().unwrap_or("Unnamed"),
            egui::FontId::proportional(14.0),
            ui.visuals().text_color(),
        );
        crate::icons::icons::CHEVRON_DOWN
            .as_image_tinted(egui::Vec2::splat(12.0), ui.visuals().text_color())
            .paint_at(
                ui,
                egui::Rect::from_center_size(
                    rect.right_center() - egui::vec2(10.0, 0.0),
                    egui::Vec2::splat(12.0),
                ),
            );
        let mut action = LayoutAction::None;
        egui::Popup::menu(&button).show(|ui| {
            action = self.show_menu(ui);
        });
        action
    }

    /// Menu contents for use in a toolbar overflow menu.
    pub fn show_menu(&mut self, ui: &mut Ui) -> LayoutAction {
        ui.set_min_width(self.config.menu_width);
        let mut action = LayoutAction::None;
        for (label, value) in [
            ("Save layout", LayoutAction::Save),
            ("New layout", LayoutAction::CreateNew),
            ("Download chart image", LayoutAction::DownloadImage),
        ] {
            if ui.button(label).clicked() {
                action = value;
                ui.close();
            }
        }
        if ui.button("Save layout as…").clicked() {
            self.editing = Some(false);
            self.new_name.clear();
        }
        if ui.button("Rename layout…").clicked() {
            self.editing = Some(true);
            self.new_name = self.curr_layout.clone().unwrap_or_default();
        }
        if let Some(rename) = self.editing {
            let response = ui.text_edit_singleline(&mut self.new_name);
            let name = self.new_name.trim();
            let valid = !name.is_empty() && !self.layouts.iter().any(|l| l.name == name);
            if ui.add_enabled(valid, egui::Button::new("Apply")).clicked()
                || (valid && response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            {
                action = if rename {
                    LayoutAction::Rename(
                        self.curr_layout.clone().unwrap_or_default(),
                        name.to_owned(),
                    )
                } else {
                    LayoutAction::SaveAs(name.to_owned())
                };
                self.editing = None;
                ui.close();
            }
        }
        ui.separator();
        ui.colored_label(ui.active_theme().ui.text_secondary, "Layouts");
        egui::ScrollArea::vertical()
            .max_height(240.0)
            .show(ui, |ui| {
                for layout in &self.layouts {
                    ui.horizontal(|ui| {
                        if ui
                            .selectable_label(
                                self.curr_layout.as_deref() == Some(&layout.name),
                                &layout.name,
                            )
                            .clicked()
                        {
                            action = LayoutAction::Open(layout.name.clone());
                            ui.close();
                        }
                        if ui
                            .small_button("×")
                            .on_hover_text("Delete layout")
                            .clicked()
                        {
                            action = LayoutAction::Delete(layout.name.clone());
                            ui.close();
                        }
                    });
                }
            });
        action
    }
}
