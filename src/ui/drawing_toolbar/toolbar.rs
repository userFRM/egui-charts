//! Drawing Toolbar
//!
//! This is the main coordinator for the left sidebar toolbar.
//! It delegates rendering to:
//! - `components/` for reusable UI elements
//! - `categories/` for tool category submenus
//! - `utilities/` for bottom toolbar btns

use super::categories::cursors::CursorType;
use crate::drawings::DrawingToolType;
use crate::icons::{Icon, icons as embedded_icons};
use crate::styles::responsive::LayoutContext;
use crate::styles::{icons as icon_sizes, typography};
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};

/// Data struct for passing cursor state to submenu via context
#[derive(Clone, Copy, Debug)]
pub struct CursorStateData {
    pub current_cursor: CursorType,
    pub eraser_mode: bool,
}

/// Domain state that should be synced with AppState
///
/// This represents the drawing-related state that is managed centrally
/// in AppState.drawing, not UI-specific state like expanded menus.
#[derive(Clone, Debug)]
pub struct DrawingToolbarDomainState {
    pub sel_tool: Option<DrawingToolType>,
    pub magnet_mode: bool,
    pub stay_in_drawing_mode: bool,
    pub drawing_color: [u8; 4],
}

// Import from modular components
use super::{
    DrawingToolbarAction, DrawingToolbarConfig,
    categories::{
        AnnotationCategory, CursorsCategory, FibonacciCategory, PatternsCategory,
        ProjectionCategory, ShapesCategory, ToolCategory, TrendLinesCategory,
    },
    components::{draw_separator_styled, draw_tool_button, render_svg_at_rect_themed},
    data, icons,
    state::ToolbarState,
    utilities::{
        FavoritesButton, HideMenu, LockButton, MagnetButton, MeasureButton, RemoveMenu,
        StayInDrawingButton, TemplatesButton, ZoomInButton, ZoomOutButton,
    },
};
use crate::ext::UiExt;
use crate::theming;

/// Drawing toolbar - main UI component
#[derive(Clone, Default)]
pub struct DrawingToolbar {
    /// Toolbar state (selected tool, favorites, recent, etc.)
    pub state: ToolbarState,
    /// Configuration (width, spacing, colors, etc.)
    pub config: DrawingToolbarConfig,
    /// Pos of the expanded category button (for submenu alignment)
    expanded_category_rect: Option<Rect>,
}

impl DrawingToolbar {
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with custom configuration
    pub fn with_config(mut self, config: DrawingToolbarConfig) -> Self {
        self.config = config;
        self
    }

    // === State Sync Methods ===

    /// Sync toolbar state FROM a domain state snapshot.
    ///
    /// Call this BEFORE `show_with_action()` to ensure the toolbar
    /// reflects the current application state. This makes AppState
    /// the single source of truth for domain state.
    pub fn sync_from_domain_state(&mut self, domain: &DrawingToolbarDomainState) {
        self.state.sel_tool = domain.sel_tool;
        self.state.magnet_mode = domain.magnet_mode;
        self.state.stay_in_drawing_mode = domain.stay_in_drawing_mode;
        self.state.drawing_color = domain.drawing_color;
    }

    /// Get the current domain state for syncing TO AppState
    ///
    /// This can be used to initialize AppState from component state
    /// during migration.
    pub fn get_domain_state(&self) -> DrawingToolbarDomainState {
        DrawingToolbarDomainState {
            sel_tool: self.state.sel_tool,
            magnet_mode: self.state.magnet_mode,
            stay_in_drawing_mode: self.state.stay_in_drawing_mode,
            drawing_color: self.state.drawing_color,
        }
    }

    // === State management methods ===

    pub fn select_tool(&mut self, tool: DrawingToolType) {
        self.state.select_tool(tool);
    }

    pub fn add_to_recent(&mut self, tool: DrawingToolType) {
        self.state.add_to_recent(tool);
    }

    pub fn clear_selection(&mut self) {
        self.state.clear_selection();
    }

    pub fn toggle_favorite(&mut self, tool: DrawingToolType) {
        self.state.toggle_favorite(tool);
    }

    pub fn is_favorite(&self, tool: &DrawingToolType) -> bool {
        self.state.is_favorite(tool)
    }

    pub fn toggle_magnet(&mut self) {
        self.state.toggle_magnet();
    }

    pub fn toggle_stay_in_drawing(&mut self) {
        self.state.toggle_stay_in_drawing();
    }

    /// Render the toolbar as a side panel and return the selected tool if changed
    pub fn show(&mut self, ui: &mut Ui) -> Option<DrawingToolType> {
        let action = self.show_with_action(ui);
        match action {
            DrawingToolbarAction::SelectTool(tool) => Some(tool),
            _ => None,
        }
    }

    /// Render the toolbar and return the full action
    pub fn show_with_action(&mut self, ui: &mut Ui) -> DrawingToolbarAction {
        let mut action;

        // Config uses fixed desktop values
        let _layout_ctx = LayoutContext::from_egui(ui.ctx());

        let sidebar_rect = Rect::from_min_size(
            ui.available_rect_before_wrap().min,
            Vec2::new(self.config.width, ui.available_height()),
        );

        // Background is handled by parent Frame in platform.rs

        // Desktop layout: direct vertical rendering
        {
            let mut child_ui = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(sidebar_rect)
                    .layout(egui::Layout::top_down(egui::Align::Center)),
            );

            action = self.render_toolbar_content(&mut child_ui, sidebar_rect);
        }

        // === COLOR PICKER POPUP (outside scroll area) ===
        if self.state.show_color_picker
            && let Some(new_color) = self.draw_color_picker_popup(ui, sidebar_rect)
        {
            self.state.drawing_color = new_color;
            action = DrawingToolbarAction::ColorChanged(new_color);
        }

        // === TEMPLATE NAME DIALOG (outside scroll area) ===
        if self.state.show_template_name_input
            && self.state.expanded_category.as_deref() != Some("Templates")
        {
            if let Some(template_action) = TemplatesButton::show_name_dialog(
                ui,
                sidebar_rect,
                &mut self.state.template_name_input,
            ) {
                self.state.show_template_name_input = false;
                self.state.template_name_input.clear();
                action = template_action;
            }
            // Cancel: empty input means user pressed Cancel/Escape
            if self.state.template_name_input.is_empty() {
                self.state.show_template_name_input = false;
            }
        }

        // === SUBMENUS (outside scroll area) ===
        if let Some(ref category) = self.state.expanded_category.clone()
            && let Some(tool_action) = self.draw_submenu(ui, category, sidebar_rect)
        {
            action = tool_action;
        }

        ui.allocate_rect(sidebar_rect, Sense::hover());
        action
    }

    /// Render the main toolbar content
    fn render_toolbar_content(
        &mut self,
        child_ui: &mut Ui,
        sidebar_rect: Rect,
    ) -> DrawingToolbarAction {
        let mut action = DrawingToolbarAction::None;

        // Add top padding
        child_ui.add_space(self.config.padding);

        // === QUICK_SEARCH BAR ===
        if child_ui.input(|i| i.key_pressed(egui::Key::Slash)) && !self.state.search_open {
            self.state.search_open = true;
            self.state.search_query.clear();
        }

        if self.state.search_open {
            self.render_search_bar(child_ui);
        }

        // === CURSORS BUTTON ===
        action = self.render_cursors_btn(child_ui, action);

        // === TOOL CATEGORIES ===
        action = self.render_tool_categories(child_ui, action);

        // Separator
        draw_separator_styled(child_ui, self.config.width);

        // === BOTTOM TOOLS (using utilities modules) ===
        action = self.render_bottom_tools(child_ui, action, sidebar_rect);

        action
    }

    /// Render search bar
    fn render_search_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let response = ui.text_edit_singleline(&mut self.state.search_query);
            if response.has_focus() && ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.state.search_open = false;
                self.state.search_query.clear();
            }
            if ui.small_button("×").clicked() {
                self.state.search_open = false;
                self.state.search_query.clear();
            }
        });
        ui.space_lg();
    }

    /// Render cursors button
    fn render_cursors_btn(
        &mut self,
        ui: &mut Ui,
        action: DrawingToolbarAction,
    ) -> DrawingToolbarAction {
        // Determine the current cursor icon based on active mode
        let (cursor_icon, cursor_tooltip): (&Icon, &str) = if self.state.eraser_mode_enabled {
            (&embedded_icons::ERASER, "Eraser")
        } else {
            (
                self.state.current_cursor_type.icon(),
                self.state.current_cursor_type.name(),
            )
        };

        let is_selected = self.state.sel_tool.is_none();

        // Simple button without arrow
        let response = draw_tool_button(ui, cursor_icon, cursor_tooltip, is_selected);

        // Click opens dropdown menu
        if response.clicked() {
            let btn_rect = response.rect;
            self.toggle_category("Cursors", btn_rect);
        }

        if self.state.expanded_category.as_deref() == Some("Cursors") {
            self.expanded_category_rect = Some(response.rect);
        }

        ui.add_space(self.config.padding);
        action
    }

    /// Render tool category btns
    fn render_tool_categories(
        &mut self,
        ui: &mut Ui,
        action: DrawingToolbarAction,
    ) -> DrawingToolbarAction {
        let primary_categories = [
            "Lines",
            "Fibonacci",
            "Patterns",
            "Projection",
            "Brushes/Shapes",
            "Text/Annotations",
            "Icons/Emojis",
        ];

        for category in primary_categories {
            let sections = data::get_category_sections(category);
            if sections.is_empty() {
                continue;
            }

            let mut tools: Vec<DrawingToolType> = sections
                .iter()
                .flat_map(|(_, tools)| tools.iter())
                .copied()
                .collect();

            // Filter by search
            if !self.state.search_query.is_empty() {
                let query = self.state.search_query.to_lowercase();
                tools.retain(|tool| tool.as_str().to_lowercase().contains(&query));
            }

            let is_icons = category == "Icons/Emojis";
            if tools.is_empty() && !is_icons {
                continue;
            }

            // Determine display tool
            let display_tool = if is_icons {
                None
            } else if let Some(selected) = self.state.sel_tool {
                if tools.contains(&selected) {
                    Some(selected)
                } else {
                    self.state
                        .category_last_tool
                        .get(category)
                        .copied()
                        .or_else(|| tools.first().copied())
                }
            } else {
                self.state
                    .category_last_tool
                    .get(category)
                    .copied()
                    .or_else(|| tools.first().copied())
            };

            let tool_icon = display_tool
                .map(icons::get_icon)
                .unwrap_or(&embedded_icons::EMOJI_ICON);

            let has_sel = self.state.sel_tool.is_some_and(|t| tools.contains(&t));

            let tool_name = display_tool.map(|t| t.as_str().to_string());
            let tooltip = tool_name.as_deref().unwrap_or(category);

            // Simple button without arrow
            let response = draw_tool_button(ui, tool_icon, tooltip, has_sel);

            // Click opens dropdown menu
            if response.clicked() {
                self.toggle_category(category, response.rect);
            }

            if self.state.expanded_category.as_deref() == Some(category) {
                self.expanded_category_rect = Some(response.rect);
            }

            ui.add_space(self.config.padding);
        }

        action
    }

    /// Render bottom tools section
    fn render_bottom_tools(
        &mut self,
        ui: &mut Ui,
        mut action: DrawingToolbarAction,
        _sidebar_rect: Rect,
    ) -> DrawingToolbarAction {
        let width = self.config.width;

        // Add spacing before bottom tools
        ui.space_xl();

        // Measure tool
        let measure_active = self.state.sel_tool == Some(DrawingToolType::Measure);
        if let Some(a) = MeasureButton::show(ui, measure_active) {
            self.select_tool(DrawingToolType::Measure);
            action = a;
        }
        ui.space_md();

        // Zoom In
        if let Some(a) = ZoomInButton::show(ui, self.state.zoom_mode_active) {
            self.state.zoom_mode_active = !self.state.zoom_mode_active;
            action = a;
        }

        // Zoom Out (only if history)
        if self.state.has_zoom_history {
            ui.space_md();
            if let Some(a) = ZoomOutButton::show(ui) {
                action = a;
            }
            ui.space_md();
        }

        // Separator
        draw_separator_styled(ui, width);

        // Magnet button
        let is_magnet_expanded = self.state.expanded_category.as_deref() == Some("Magnet");
        let (mag_action, _icon_clicked, arrow_clicked, arrow_rect) =
            MagnetButton::show(ui, self.state.magnet_mode);

        if _icon_clicked {
            self.state.magnet_mode = !self.state.magnet_mode;
            action = DrawingToolbarAction::ToggleMagnet;
        }
        if arrow_clicked {
            self.toggle_category("Magnet", arrow_rect);
        }
        if is_magnet_expanded {
            self.expanded_category_rect = Some(arrow_rect);
        }
        if let Some(a) = mag_action {
            action = a;
        }

        ui.space_md();

        // Keep Drawing
        if let Some(a) = StayInDrawingButton::show(ui, self.state.stay_in_drawing_mode) {
            self.state.stay_in_drawing_mode = !self.state.stay_in_drawing_mode;
            action = a;
        }
        ui.space_md();

        // Lock All Drawings
        if let Some(a) = LockButton::show(ui, false) {
            action = a;
        }
        ui.space_md();

        // Templates
        let is_templates_expanded = self.state.expanded_category.as_deref() == Some("Templates");
        let (_templates_icon_clicked, templates_arrow_clicked, templates_rect) =
            TemplatesButton::show(ui);

        if templates_arrow_clicked {
            self.toggle_category("Templates", templates_rect);
        }
        if is_templates_expanded {
            self.expanded_category_rect = Some(templates_rect);
        }
        ui.space_md();

        // Hide dropdown
        let is_hide_expanded = self.state.expanded_category.as_deref() == Some("Hide");
        let (hide_icon_clicked, hide_arrow_clicked, hide_rect) = HideMenu::show(ui);

        if hide_icon_clicked {
            action = DrawingToolbarAction::HideAllDrawings;
        }
        if hide_arrow_clicked {
            self.toggle_category("Hide", hide_rect);
        }
        if is_hide_expanded {
            self.expanded_category_rect = Some(hide_rect);
        }

        // Separator
        draw_separator_styled(ui, width);

        // Remove dropdown
        let is_remove_expanded = self.state.expanded_category.as_deref() == Some("Remove");
        let (remove_icon_clicked, remove_arrow_clicked, remove_rect) = RemoveMenu::show(ui);

        if remove_icon_clicked {
            action = DrawingToolbarAction::ClearAllDrawings;
        }
        if remove_arrow_clicked {
            self.toggle_category("Remove", remove_rect);
        }
        if is_remove_expanded {
            self.expanded_category_rect = Some(remove_rect);
        }

        // Flexible spacer to push Favorites to bottom
        let remaining = ui.available_height();
        if remaining > DESIGN_TOKENS.sizing.button_xxl {
            ui.add_space(remaining - DESIGN_TOKENS.sizing.button_xxl);
        }

        // Separator before favorites
        draw_separator_styled(ui, width);

        // Favorites
        if let Some(a) = FavoritesButton::show(ui, self.state.show_favorites_toolbar) {
            self.state.show_favorites_toolbar = !self.state.show_favorites_toolbar;
            action = a;
        }

        action
    }

    /// Toggle category expansion
    fn toggle_category(&mut self, category: &str, rect: Rect) {
        if self.state.expanded_category.as_deref() == Some(category) {
            self.state.expanded_category = None;
            self.expanded_category_rect = None;
        } else {
            self.state.expanded_category = Some(category.to_string());
            self.expanded_category_rect = Some(rect);
        }
    }

    /// Draw submenu for a category - delegates to category modules
    fn draw_submenu(
        &mut self,
        ui: &mut Ui,
        category: &str,
        sidebar_rect: Rect,
    ) -> Option<DrawingToolbarAction> {
        // Utility submenus (bottom btns)
        match category {
            "Magnet" => {
                return MagnetButton::show_submenu(ui, sidebar_rect, self.expanded_category_rect);
            }
            "Hide" => return HideMenu::show_submenu(ui, sidebar_rect, self.expanded_category_rect),
            "Remove" => {
                return RemoveMenu::show_submenu(ui, sidebar_rect, self.expanded_category_rect);
            }
            "Templates" => {
                if self.state.show_template_name_input {
                    let result = TemplatesButton::show_name_dialog(
                        ui,
                        sidebar_rect,
                        &mut self.state.template_name_input,
                    );
                    if result.is_some() {
                        self.state.show_template_name_input = false;
                        self.state.template_name_input.clear();
                        self.state.expanded_category = None;
                    }
                    if self.state.template_name_input.is_empty() {
                        self.state.show_template_name_input = false;
                    }
                    return result;
                }

                let templates = self.state.templates.clone();
                let result = TemplatesButton::show_submenu(
                    ui,
                    sidebar_rect,
                    self.expanded_category_rect,
                    &templates,
                    &mut self.state.template_name_input,
                    self.state.show_template_name_input,
                );

                // If "Save as Template..." was clicked, show the name dialog
                if matches!(result, Some(DrawingToolbarAction::OpenTemplateMenu)) {
                    self.state.show_template_name_input = true;
                    self.state.template_name_input.clear();
                    return None;
                }

                if result.is_some() {
                    self.state.expanded_category = None;
                }

                return result;
            }
            _ => {}
        }

        // Tool category submenus - delegate to category modules
        let anchor_rect = self.expanded_category_rect.unwrap_or(sidebar_rect);
        let sel_tool = self.state.sel_tool;
        let favorites = self.state.favorites.clone();

        // Store cursor state in context for the cursor submenu to access
        ui.ctx().data_mut(|d| {
            d.insert_temp(
                egui::Id::new("cursor_state"),
                CursorStateData {
                    current_cursor: self.state.current_cursor_type,
                    eraser_mode: self.state.eraser_mode_enabled,
                },
            );
        });

        let action = match category {
            "Cursors" => CursorsCategory.render_submenu(ui, anchor_rect, sel_tool, &favorites),
            "Lines" => TrendLinesCategory.render_submenu(ui, anchor_rect, sel_tool, &favorites),
            "Fibonacci" => FibonacciCategory.render_submenu(ui, anchor_rect, sel_tool, &favorites),
            "Patterns" => PatternsCategory.render_submenu(ui, anchor_rect, sel_tool, &favorites),
            "Projection" => {
                ProjectionCategory.render_submenu(ui, anchor_rect, sel_tool, &favorites)
            }
            "Brushes/Shapes" => {
                ShapesCategory.render_submenu(ui, anchor_rect, sel_tool, &favorites)
            }
            "Text/Annotations" => {
                AnnotationCategory.render_submenu(ui, anchor_rect, sel_tool, &favorites)
            }
            _ => DrawingToolbarAction::None,
        };

        // Handle tool selection
        if let DrawingToolbarAction::SelectTool(tool) = action {
            self.select_tool(tool);
            if !self.state.stay_in_drawing_mode {
                self.state.expanded_category = None;
            }
            return Some(action);
        }

        // Handle cursor actions - also close submenu
        if matches!(
            action,
            DrawingToolbarAction::SetCursorType(_) | DrawingToolbarAction::ToggleEraserMode
        ) {
            self.state.expanded_category = None;
            return Some(action);
        }

        if action != DrawingToolbarAction::None {
            Some(action)
        } else {
            None
        }
    }

    // Submenu rendering delegated to category modules - see categories/*.rs

    /// Draw color picker popup
    fn draw_color_picker_popup(&mut self, ui: &mut Ui, sidebar_rect: Rect) -> Option<[u8; 4]> {
        let popup_pos = self.color_picker_position(sidebar_rect);
        let current_color = self.state.drawing_color;

        let (area_res, sel_color) = self.render_color_picker_area(ui, popup_pos, current_color);

        self.handle_color_picker_close(ui, &area_res, sidebar_rect, sel_color.is_some());

        sel_color
    }

    /// Calculate popup position for color picker
    fn color_picker_position(&self, sidebar_rect: Rect) -> Pos2 {
        let popup_height = DESIGN_TOKENS.sizing.dialog.color_picker_height;
        Pos2::new(
            sidebar_rect.right() + DESIGN_TOKENS.spacing.sm,
            sidebar_rect.center().y - popup_height / 2.0,
        )
    }

    /// Render the color picker area and return selected color if any
    fn render_color_picker_area(
        &self,
        ui: &mut Ui,
        popup_pos: Pos2,
        current_color: [u8; 4],
    ) -> (egui::InnerResponse<()>, Option<[u8; 4]>) {
        let mut sel_color = None;

        let area_res = egui::Area::new(egui::Id::new("color_picker_popup"))
            .fixed_pos(popup_pos)
            .order(egui::Order::Foreground)
            .constrain(true)
            .show(ui.ctx(), |ui| {
                sel_color = self.render_color_picker_frame(ui, current_color);
            });

        (area_res, sel_color)
    }

    /// Render the color picker frame content
    fn render_color_picker_frame(&self, ui: &mut Ui, current_color: [u8; 4]) -> Option<[u8; 4]> {
        let popup_width = DESIGN_TOKENS.sizing.dialog.color_picker_width;
        let popup_height = DESIGN_TOKENS.sizing.dialog.color_picker_height;
        let mut sel_color = None;

        egui::Frame::popup(ui.style())
            .fill(ui.style().visuals.window_fill())
            .stroke(ui.style().visuals.window_stroke)
            .corner_radius(DESIGN_TOKENS.rounding.lg)
            .show(ui, |ui| {
                ui.set_min_size(Vec2::new(popup_width, popup_height));
                ui.space_lg();
                ui.label(RichText::new("Drawing Color").size(typography::SM));
                ui.space_md();

                sel_color = self.render_color_swatches(ui, current_color);
            });

        sel_color
    }

    /// Render the color swatch grid and return selected color if clicked
    fn render_color_swatches(&self, ui: &mut Ui, current_color: [u8; 4]) -> Option<[u8; 4]> {
        let colors = Self::color_palette();
        let swatch_size = DESIGN_TOKENS.sizing.button_sm;
        let swatch_spacing = DESIGN_TOKENS.spacing.sm;
        let columns = 5;
        let mut sel_color = None;

        for row in 0..4 {
            ui.horizontal(|ui| {
                ui.space_lg();
                sel_color = sel_color.or(self.render_swatch_row(
                    ui,
                    &colors,
                    row,
                    columns,
                    swatch_size,
                    swatch_spacing,
                    current_color,
                ));
            });
            ui.add_space(swatch_spacing);
        }

        sel_color
    }

    /// Render a single row of color swatches
    fn render_swatch_row(
        &self,
        ui: &mut Ui,
        colors: &[([u8; 4], &str)],
        row: usize,
        columns: usize,
        swatch_size: f32,
        swatch_spacing: f32,
        current_color: [u8; 4],
    ) -> Option<[u8; 4]> {
        let mut sel_color = None;

        for col in 0..columns {
            let idx = row * columns + col;
            if idx >= colors.len() {
                break;
            }

            let (color_val, name) = &colors[idx];
            if let Some(clicked) =
                self.render_single_swatch(ui, *color_val, name, swatch_size, current_color)
            {
                sel_color = Some(clicked);
            }
            ui.add_space(swatch_spacing);
        }

        sel_color
    }

    /// Render a single color swatch and return its color if clicked
    fn render_single_swatch(
        &self,
        ui: &mut Ui,
        color_val: [u8; 4],
        name: &str,
        swatch_size: f32,
        current_color: [u8; 4],
    ) -> Option<[u8; 4]> {
        let (rect, response) = ui.allocate_exact_size(Vec2::splat(swatch_size), Sense::click());

        let c =
            Color32::from_rgba_unmultiplied(color_val[0], color_val[1], color_val[2], color_val[3]);
        ui.painter().rect_filled(rect, DESIGN_TOKENS.rounding.sm, c);

        self.render_swatch_border(ui, rect, &response, current_color == color_val);
        let clicked = response.clicked();
        response.on_hover_text(name);

        if clicked { Some(color_val) } else { None }
    }

    /// Render border for a color swatch based on selection/hover state
    fn render_swatch_border(
        &self,
        ui: &mut Ui,
        rect: Rect,
        response: &egui::Response,
        is_selected: bool,
    ) {
        if is_selected {
            let selection_color = ui.style().visuals.selection.stroke.color;
            ui.painter().rect_stroke(
                rect,
                DESIGN_TOKENS.rounding.sm,
                Stroke::new(DESIGN_TOKENS.stroke.thick, selection_color),
                egui::StrokeKind::Outside,
            );
        } else if response.hovered() {
            let hover_color = ui.style().visuals.widgets.hovered.fg_stroke.color;
            ui.painter().rect_stroke(
                rect,
                DESIGN_TOKENS.rounding.sm,
                Stroke::new(DESIGN_TOKENS.stroke.hairline, hover_color),
                egui::StrokeKind::Outside,
            );
        }
    }

    /// Handle closing the color picker on outside click or selection
    fn handle_color_picker_close(
        &mut self,
        ui: &mut Ui,
        area_res: &egui::InnerResponse<()>,
        sidebar_rect: Rect,
        color_selected: bool,
    ) {
        let clicked_outside = ui.input(|i| i.pointer.hover_pos()).is_some_and(|pos| {
            ui.input(|i| i.pointer.primary_clicked())
                && !area_res.response.rect.contains(pos)
                && !sidebar_rect.contains(pos)
        });

        if clicked_outside || color_selected {
            self.state.show_color_picker = false;
        }
    }

    /// Get the color palette for the color picker
    fn color_palette() -> Vec<([u8; 4], &'static str)> {
        vec![
            ([242, 54, 69, 255], "Red"),
            ([255, 82, 82, 255], "Light Red"),
            ([255, 109, 0, 255], "Orange"),
            ([255, 152, 0, 255], "Amber"),
            ([255, 235, 59, 255], "Yellow"),
            ([38, 166, 154, 255], "Teal"),
            ([76, 175, 80, 255], "Green"),
            ([139, 195, 74, 255], "Light Green"),
            ([205, 220, 57, 255], "Lime"),
            ([0, 150, 136, 255], "Cyan"),
            ([33, 150, 243, 255], "Blue"),
            ([41, 98, 255, 255], "Indigo"),
            ([103, 58, 183, 255], "Deep Purple"),
            ([156, 39, 176, 255], "Purple"),
            ([233, 30, 99, 255], "Pink"),
            ([255, 255, 255, 255], "White"),
            ([178, 181, 190, 255], "Gray"),
            ([120, 123, 134, 255], "Dark Gray"),
            ([66, 66, 66, 255], "Charcoal"),
            ([0, 0, 0, 255], "Black"),
        ]
    }

    /// Render compact floating toolbar
    pub fn show_floating(&mut self, ui: &mut Ui, position: Pos2) -> Option<DrawingToolType> {
        let mut new_selection = None;

        egui::Area::new(egui::Id::new("floating_drawing_toolbar"))
            .fixed_pos(position)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(theming::toolbar_bg(ui))
                    .stroke(ui.style().visuals.window_stroke)
                    .corner_radius(DESIGN_TOKENS.rounding.md)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let quick_tools = [
                                DrawingToolType::TrendLine,
                                DrawingToolType::HorizontalLine,
                                DrawingToolType::VerticalLine,
                                DrawingToolType::FibonacciRetracement,
                                DrawingToolType::Rect,
                                DrawingToolType::TextLabel,
                            ];

                            for tool in quick_tools {
                                let is_sel = self.state.sel_tool == Some(tool);
                                let (rect, response) = ui.allocate_exact_size(
                                    Vec2::splat(DESIGN_TOKENS.sizing.button_md),
                                    Sense::click(),
                                );

                                let bg = if is_sel {
                                    theming::sel_color(ui)
                                } else if response.hovered() {
                                    theming::hover_color(ui)
                                } else {
                                    Color32::TRANSPARENT
                                };
                                ui.painter()
                                    .rect_filled(rect, DESIGN_TOKENS.rounding.sm, bg);

                                let icon_rect = Rect::from_center_size(
                                    rect.center(),
                                    Vec2::splat(icon_sizes::MD),
                                );
                                let svg_icon = icons::get_icon(tool);
                                render_svg_at_rect_themed(
                                    ui,
                                    svg_icon,
                                    icon_rect,
                                    response.hovered(),
                                    is_sel,
                                );

                                if response.on_hover_text(tool.as_str()).clicked() {
                                    new_selection = Some(tool);
                                }
                            }

                            ui.separator();

                            // Magnet toggle
                            let (rect, response) = ui.allocate_exact_size(
                                Vec2::splat(DESIGN_TOKENS.sizing.button_md),
                                Sense::click(),
                            );
                            let bg = if self.state.magnet_mode {
                                theming::sel_color(ui)
                            } else if response.hovered() {
                                theming::hover_color(ui)
                            } else {
                                Color32::TRANSPARENT
                            };
                            ui.painter()
                                .rect_filled(rect, DESIGN_TOKENS.rounding.sm, bg);

                            let icon_rect =
                                Rect::from_center_size(rect.center(), Vec2::splat(icon_sizes::MD));
                            render_svg_at_rect_themed(
                                ui,
                                &embedded_icons::MAGNET,
                                icon_rect,
                                response.hovered(),
                                self.state.magnet_mode,
                            );

                            if response.on_hover_text("Magnet Mode").clicked() {
                                self.state.magnet_mode = !self.state.magnet_mode;
                            }

                            ui.separator();

                            ui.menu_button(" ", |ui| {
                                for category in DrawingToolType::categories() {
                                    ui.menu_button(*category, |ui| {
                                        for tool in DrawingToolType::by_category(category) {
                                            if ui.button(tool.as_str()).clicked() {
                                                new_selection = Some(tool);
                                                ui.close();
                                            }
                                        }
                                    });
                                }
                            });
                        });
                    });
            });

        if let Some(tool) = new_selection {
            self.select_tool(tool);
        }

        new_selection
    }
}
