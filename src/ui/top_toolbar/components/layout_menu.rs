//! Host-owned named layout actions. No storage or account service is assumed.

use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use egui::Ui;

/// Configuration for layout menu
#[derive(Debug, Clone)]
pub struct LayoutMenuConfig {
    pub menu_width: f32,
}

impl Default for LayoutMenuConfig {
    fn default() -> Self {
        Self { menu_width: 220.0 }
    }
}

/// Action from layout menu
#[derive(Debug, Clone, PartialEq)]
pub enum LayoutAction {
    None,
    /// Select a tiled or tabbed chart arrangement.
    SelectGrid(crate::widget::ChartLayoutMode),
    Save,
    SaveAs(String),
    DownloadImage,
    CreateNew,
    Open(String),
    Delete(String),
    Rename(String, String),
}

/// Named layout menu. The host supplies the available layouts and handles returned intents.
pub struct LayoutMenu {
    /// Name of the current layout.
    pub curr_layout: Option<String>,
    /// Host-owned layouts available to open.
    pub layouts: Vec<String>,
    /// Menu sizing.
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
            ui.painter().rect_filled(
                rect,
                DESIGN_TOKENS.rounding.button,
                ui.visuals().widgets.hovered.bg_fill,
            );
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
        button.widget_info(|| {
            egui::WidgetInfo::labeled(egui::Role::Button, ui.is_enabled(), "Named layouts")
        });
        crate::styles::focus::draw_focus_ring(ui, &button, DESIGN_TOKENS.rounding.button);
        egui::Popup::menu(&button)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
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
        let save_as = ui.button("Save layout as…").clicked();
        let rename = ui.button("Rename layout…").clicked();
        if save_as {
            self.editing = Some(false);
            self.new_name.clear();
        }
        if rename {
            self.editing = Some(true);
            self.new_name = self.curr_layout.clone().unwrap_or_default();
        }
        if let Some(is_rename) = self.editing {
            let response = ui.text_edit_singleline(&mut self.new_name);
            if save_as || rename {
                response.request_focus();
            }
            let name = self.new_name.trim();
            let valid = !name.is_empty() && !self.layouts.iter().any(|l| l == name);
            if ui.add_enabled(valid, egui::Button::new("Apply")).clicked()
                || (valid && response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)))
            {
                action = if is_rename {
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
                                self.curr_layout.as_deref() == Some(layout.as_str()),
                                layout,
                            )
                            .clicked()
                        {
                            action = LayoutAction::Open(layout.clone());
                            ui.close();
                        }
                        if ui
                            .small_button("×")
                            .on_hover_text("Delete layout")
                            .clicked()
                        {
                            action = LayoutAction::Delete(layout.clone());
                            ui.close();
                        }
                    });
                }
            });
        action
    }
}
