//! Layout Menu
//!
//! A dropdown menu for saving, loading, and managing chart layouts.
use crate::styles::typography;
use crate::tokens::DESIGN_TOKENS;
use chrono::{DateTime, Utc};
use egui::{Color32, Pos2, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2};

/// Theme colors extracted from visuals for menu rendering
struct MenuColors {
    bg: Color32,
    border: Color32,
    hover: Color32,
    accent: Color32,
    text: Color32,
    muted: Color32,
    strong: Color32,
    faint_bg: Color32,
}

impl MenuColors {
    fn from_visuals(visuals: &egui::Visuals) -> Self {
        Self {
            bg: visuals.window_fill,
            border: visuals.widgets.noninteractive.bg_stroke.color,
            hover: visuals.widgets.hovered.bg_fill,
            accent: visuals.selection.bg_fill,
            text: visuals.text_color(),
            muted: visuals.widgets.noninteractive.fg_stroke.color,
            strong: visuals.strong_text_color(),
            faint_bg: visuals.faint_bg_color,
        }
    }
}

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

/// Layout menu
pub struct LayoutMenu {
    /// Is menu open
    is_open: bool,
    /// Current layout name
    pub curr_layout: Option<String>,
    /// Available layouts
    pub layouts: Vec<LayoutInfo>,
    /// Configuration
    pub config: LayoutMenuConfig,
    /// Show save as dialog
    show_save_as: bool,
    /// New layout name input
    new_name: String,
    /// Show rename dialog
    show_rename: Option<String>,
}

impl Default for LayoutMenu {
    fn default() -> Self {
        Self::new()
    }
}

impl LayoutMenu {
    pub fn new() -> Self {
        Self {
            is_open: false,
            curr_layout: Some("Default".to_string()),
            layouts: vec![LayoutInfo::default_layout()],
            config: LayoutMenuConfig::default(),
            show_save_as: false,
            new_name: String::new(),
            show_rename: None,
        }
    }

    /// Add a layout
    pub fn add_layout(&mut self, layout: LayoutInfo) {
        self.layouts.push(layout);
    }

    /// Show the layout menu
    pub fn show(&mut self, ui: &mut Ui) -> LayoutAction {
        let mut action = LayoutAction::None;

        // Main button
        let btn_res = self.draw_btn(ui);
        if btn_res.clicked() {
            self.is_open = !self.is_open;
            self.show_save_as = false;
            self.show_rename = None;
        }

        // Dropdown menu
        if self.is_open {
            let btn_rect = btn_res.rect;
            action = self.draw_menu(ui, btn_rect);

            // Close on click outside
            if ui.input(|i| i.pointer.any_click())
                && let Some(pos) = ui.input(|i| i.pointer.hover_pos())
            {
                let menu_height = self.calculate_menu_height();
                let menu_rect = Rect::from_min_size(
                    Pos2::new(btn_rect.min.x, btn_rect.max.y + 2.0),
                    Vec2::new(self.config.menu_width, menu_height),
                );
                if !btn_rect.contains(pos) && !menu_rect.contains(pos) {
                    self.is_open = false;
                }
            }
        }

        action
    }

    fn draw_btn(&self, ui: &mut Ui) -> Response {
        // Get colors from theme
        let visuals = ui.style().visuals.clone();
        let hover_color = visuals.widgets.hovered.bg_fill;
        let border_color = visuals.widgets.noninteractive.bg_stroke.color;
        let text_color = visuals.text_color();
        let muted_color = visuals.widgets.noninteractive.fg_stroke.color;

        let layout_name = self.curr_layout.as_deref().unwrap_or("Untitled");
        // Approximate text width (roughly 7 pixels per character for 12pt font)
        let text_width = layout_name.len() as f32 * 7.0;
        let desired_size = Vec2::new(
            text_width.max(DESIGN_TOKENS.sizing.dialog.button_min_width_xs)
                + DESIGN_TOKENS.spacing.section_lg,
            DESIGN_TOKENS.sizing.button_md,
        );
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

        let bg_color = if self.is_open || response.hovered() {
            hover_color
        } else {
            Color32::TRANSPARENT
        };

        ui.painter()
            .rect_filled(rect, DESIGN_TOKENS.rounding.md, bg_color);
        ui.painter().rect_stroke(
            rect,
            DESIGN_TOKENS.rounding.md,
            Stroke::new(DESIGN_TOKENS.stroke.hairline, border_color),
            StrokeKind::Outside,
        );

        // Layout name
        ui.painter().text(
            rect.center() - Vec2::new(8.0, 0.0),
            egui::Align2::CENTER_CENTER,
            layout_name,
            egui::FontId::proportional(typography::SM_MD),
            text_color,
        );

        // Dropdown arrow
        ui.painter().text(
            Pos2::new(rect.right() - 12.0, rect.center().y),
            egui::Align2::CENTER_CENTER,
            "▾",
            egui::FontId::proportional(typography::XS),
            muted_color,
        );

        response
    }

    fn calculate_menu_height(&self) -> f32 {
        let base_items = 4; // Save, Download, separator, Create new
        let layout_items = self.layouts.len();
        let total = base_items + layout_items + 2; // +2 for header and separator
        (total as f32 * self.config.item_height).min(400.0)
    }

    fn draw_menu(&mut self, ui: &mut Ui, btn_rect: Rect) -> LayoutAction {
        let mut action = LayoutAction::None;
        let colors = MenuColors::from_visuals(&ui.style().visuals);

        let menu_pos = Pos2::new(btn_rect.min.x, btn_rect.max.y + 2.0);
        let menu_height = self.calculate_menu_height();
        let menu_rect =
            Rect::from_min_size(menu_pos, Vec2::new(self.config.menu_width, menu_height));

        // Draw menu background with shadow
        self.draw_menu_background(ui, menu_rect, &colors);

        let inner_rect = menu_rect.shrink(4.0);
        let mut y = inner_rect.min.y;

        // Action menu items
        action = self.draw_action_items(ui, &mut y, inner_rect, &colors, action);

        // Save as dialog (if open)
        if self.show_save_as {
            if let Some(a) = self.draw_save_as_dialog(ui, &mut y, inner_rect, &colors) {
                action = a;
            }
        }

        // Layouts section
        action = self.draw_layouts_section(ui, &mut y, inner_rect, &colors, action);

        action
    }

    /// Draw menu background with shadow
    fn draw_menu_background(&self, ui: &mut Ui, menu_rect: Rect, colors: &MenuColors) {
        let shadow_rect = menu_rect.translate(Vec2::splat(DESIGN_TOKENS.shadow.offset_sm));
        ui.painter().rect_filled(
            shadow_rect,
            DESIGN_TOKENS.rounding.md,
            Color32::from_black_alpha(60),
        );
        ui.painter()
            .rect_filled(menu_rect, DESIGN_TOKENS.rounding.md, colors.bg);
        ui.painter().rect_stroke(
            menu_rect,
            DESIGN_TOKENS.rounding.md,
            Stroke::new(DESIGN_TOKENS.stroke.hairline, colors.border),
            StrokeKind::Outside,
        );
    }

    /// Draw action menu items (Save, Download, Create new)
    fn draw_action_items(
        &mut self,
        ui: &mut Ui,
        y: &mut f32,
        inner_rect: Rect,
        colors: &MenuColors,
        mut action: LayoutAction,
    ) -> LayoutAction {
        // Save layout
        if let Some(a) = self.draw_menu_item(
            ui,
            ">",
            "Save layout",
            *y,
            inner_rect.min.x,
            inner_rect.width(),
        ) && a.clicked()
        {
            action = LayoutAction::Save;
            self.is_open = false;
        }
        *y += self.config.item_height;

        // Download chart image
        if let Some(a) = self.draw_menu_item(
            ui,
            ">",
            "Download chart image",
            *y,
            inner_rect.min.x,
            inner_rect.width(),
        ) && a.clicked()
        {
            action = LayoutAction::DownloadImage;
            self.is_open = false;
        }
        *y += self.config.item_height;

        // Separator
        *y += 4.0;
        self.draw_separator(ui, *y, inner_rect.min.x, inner_rect.width(), colors.border);
        *y += 8.0;

        // Create new layout
        if let Some(a) = self.draw_menu_item(
            ui,
            "+",
            "Create new layout",
            *y,
            inner_rect.min.x,
            inner_rect.width(),
        ) && a.clicked()
        {
            self.show_save_as = true;
            self.new_name.clear();
        }
        *y += self.config.item_height;

        action
    }

    /// Draw the layouts section (header + list)
    fn draw_layouts_section(
        &mut self,
        ui: &mut Ui,
        y: &mut f32,
        inner_rect: Rect,
        colors: &MenuColors,
        mut action: LayoutAction,
    ) -> LayoutAction {
        // Separator before layouts
        *y += 4.0;
        self.draw_separator(ui, *y, inner_rect.min.x, inner_rect.width(), colors.border);
        *y += 8.0;

        // Layouts header
        ui.painter().text(
            Pos2::new(inner_rect.min.x + 8.0, *y + 8.0),
            egui::Align2::LEFT_CENTER,
            "Layouts",
            egui::FontId::proportional(typography::SM),
            colors.muted,
        );
        *y += 20.0;

        // Layout items - collect data to avoid borrow issues
        let layouts: Vec<_> = self.layouts.to_vec();
        for layout in &layouts {
            if *y + self.config.item_height > inner_rect.max.y {
                break;
            }

            let is_current = self.curr_layout.as_ref() == Some(&layout.name);
            let item_rect = Rect::from_min_size(
                Pos2::new(inner_rect.min.x, *y),
                Vec2::new(inner_rect.width(), self.config.item_height),
            );

            if let Some(a) = self.draw_layout_item(ui, layout, item_rect, is_current, colors) {
                action = a;
                self.curr_layout = Some(layout.name.clone());
                self.is_open = false;
            }
            *y += self.config.item_height;
        }

        action
    }

    fn draw_menu_item(
        &self,
        ui: &mut Ui,
        icon: &str,
        label: &str,
        y: f32,
        x: f32,
        width: f32,
    ) -> Option<Response> {
        // Get colors from theme
        let visuals = ui.style().visuals.clone();
        let hover_color = visuals.widgets.hovered.bg_fill;
        let text_color = visuals.text_color();
        let muted_color = visuals.widgets.noninteractive.fg_stroke.color;

        let item_rect =
            Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, self.config.item_height));

        let response = ui.allocate_rect(item_rect, Sense::click());

        if response.hovered() {
            ui.painter()
                .rect_filled(item_rect, DESIGN_TOKENS.rounding.sm, hover_color);
        }

        // Icon
        ui.painter().text(
            Pos2::new(item_rect.min.x + 12.0, item_rect.center().y),
            egui::Align2::LEFT_CENTER,
            icon,
            egui::FontId::proportional(typography::LG),
            muted_color,
        );

        // Label
        ui.painter().text(
            Pos2::new(item_rect.min.x + 36.0, item_rect.center().y),
            egui::Align2::LEFT_CENTER,
            label,
            egui::FontId::proportional(typography::MD),
            text_color,
        );

        Some(response)
    }

    /// Draw a horizontal separator line
    fn draw_separator(&self, ui: &mut Ui, y: f32, x: f32, width: f32, color: Color32) {
        let sep_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(width, 1.0));
        ui.painter()
            .rect_filled(sep_rect, DESIGN_TOKENS.rounding.none, color);
    }

    /// Draw the save-as dialog section
    fn draw_save_as_dialog(
        &mut self,
        ui: &mut Ui,
        y: &mut f32,
        inner_rect: Rect,
        colors: &MenuColors,
    ) -> Option<LayoutAction> {
        *y += DESIGN_TOKENS.spacing.sm;

        let enter_pressed = self.draw_name_input(ui, y, inner_rect, colors);
        if enter_pressed && !self.new_name.is_empty() {
            return self.complete_save_as();
        }

        *y += DESIGN_TOKENS.sizing.button_md;

        let btn_clicked = self.draw_create_button(ui, y, inner_rect, colors);
        if btn_clicked && !self.new_name.is_empty() {
            return self.complete_save_as();
        }

        *y += DESIGN_TOKENS.sizing.button_md;
        None
    }

    fn draw_name_input(
        &mut self,
        ui: &mut Ui,
        y: &f32,
        inner_rect: Rect,
        colors: &MenuColors,
    ) -> bool {
        let input_rect = Rect::from_min_size(
            Pos2::new(inner_rect.min.x + DESIGN_TOKENS.spacing.sm, *y),
            Vec2::new(
                inner_rect.width() - DESIGN_TOKENS.spacing.lg,
                DESIGN_TOKENS.sizing.button_sm,
            ),
        );

        ui.painter()
            .rect_filled(input_rect, DESIGN_TOKENS.rounding.md, colors.faint_bg);
        ui.painter().rect_stroke(
            input_rect,
            DESIGN_TOKENS.rounding.sm,
            Stroke::new(DESIGN_TOKENS.stroke.hairline, colors.accent),
            StrokeKind::Outside,
        );

        let text_rect = input_rect.shrink(DESIGN_TOKENS.spacing.sm);
        let mut text_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(text_rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        let response = text_ui.add(
            egui::TextEdit::singleline(&mut self.new_name)
                .hint_text("Layout name")
                .frame(false),
        );

        response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))
    }

    fn draw_create_button(
        &self,
        ui: &mut Ui,
        y: &f32,
        inner_rect: Rect,
        colors: &MenuColors,
    ) -> bool {
        let btn_rect = Rect::from_min_size(
            Pos2::new(inner_rect.min.x + DESIGN_TOKENS.spacing.sm, *y),
            Vec2::new(
                inner_rect.width() - DESIGN_TOKENS.spacing.lg,
                DESIGN_TOKENS.sizing.button_sm,
            ),
        );
        let btn_res = ui.allocate_rect(btn_rect, Sense::click());

        let btn_bg = if btn_res.hovered() {
            colors.accent
        } else {
            colors.hover
        };
        ui.painter()
            .rect_filled(btn_rect, DESIGN_TOKENS.rounding.md, btn_bg);

        ui.painter().text(
            btn_rect.center(),
            egui::Align2::CENTER_CENTER,
            "Create",
            egui::FontId::proportional(typography::SM),
            colors.strong,
        );

        btn_res.clicked()
    }

    fn complete_save_as(&mut self) -> Option<LayoutAction> {
        let action = LayoutAction::SaveAs(self.new_name.clone());
        self.layouts.push(LayoutInfo::new(&self.new_name));
        self.curr_layout = Some(self.new_name.clone());
        self.show_save_as = false;
        self.is_open = false;
        Some(action)
    }

    /// Draw a single layout item
    fn draw_layout_item(
        &self,
        ui: &mut Ui,
        layout: &LayoutInfo,
        item_rect: Rect,
        is_current: bool,
        colors: &MenuColors,
    ) -> Option<LayoutAction> {
        let item_res = ui.allocate_rect(item_rect, Sense::click());

        // Background
        if is_current || item_res.hovered() {
            let bg = if is_current {
                colors.accent
            } else {
                colors.hover
            };
            ui.painter()
                .rect_filled(item_rect, DESIGN_TOKENS.rounding.sm, bg);
        }

        // Layout icon
        let icon = if layout.is_default { "*" } else { "-" };
        let icon_color = if is_current {
            colors.strong
        } else {
            colors.muted
        };
        ui.painter().text(
            Pos2::new(item_rect.min.x + 12.0, item_rect.center().y),
            egui::Align2::LEFT_CENTER,
            icon,
            egui::FontId::proportional(typography::SM_MD),
            icon_color,
        );

        // Layout name
        let name_color = if is_current {
            colors.strong
        } else {
            colors.text
        };
        ui.painter().text(
            Pos2::new(item_rect.min.x + 32.0, item_rect.center().y),
            egui::Align2::LEFT_CENTER,
            &layout.name,
            egui::FontId::proportional(typography::MD),
            name_color,
        );

        // Cloud sync indicator
        if layout.is_cloud_synced {
            let cloud_color = if is_current {
                colors.strong
            } else {
                colors.muted
            };
            ui.painter().text(
                Pos2::new(item_rect.right() - 12.0, item_rect.center().y),
                egui::Align2::CENTER_CENTER,
                "c",
                egui::FontId::proportional(typography::SM_MD),
                cloud_color,
            );
        }

        if item_res.clicked() {
            return Some(LayoutAction::Open(layout.name.clone()));
        }
        None
    }
}
