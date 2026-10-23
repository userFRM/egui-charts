//! Drawing Toolbar
//!
//! This is the main coordinator for the left sidebar toolbar.
//! It delegates rendering to:
//! - `components/` for reusable UI elements
//! - `categories/` for tool category submenus
//! - `utilities/` for bottom toolbar btns

use super::SubmenuBuilder;
use super::categories::cursors::CursorType;
use crate::drawings::DrawingToolType;
use crate::ext::UiExt;
use crate::icons::{Icon, icons as embedded_icons};
use crate::styles::responsive::LayoutContext;
use crate::styles::{icons as icon_sizes, typography};
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::Control;
use crate::ui_kit::buttons::IconButton;
use egui::{Color32, Id, Key, Modifiers, Popup, Pos2, Rect, RichText, Sense, Stroke, Ui, Vec2};
use std::collections::HashSet;

/// The icons the Icons/Emojis menu offers: the glyph a drawing shows, and its name. They are
/// drawn with the app's fonts, so the app needs emoji fonts, such as egui's
/// `monochrome_emoji_fonts` feature, as the demo has.
const ICONS: [(&str, &str); 10] = [
    ("👍", "Thumbs up"),
    ("👎", "Thumbs down"),
    ("🔥", "Fire"),
    ("🚀", "Rocket"),
    ("⭐", "Star"),
    ("⚠", "Warning"),
    ("✅", "Check mark"),
    ("❌", "Cross mark"),
    ("📌", "Pin"),
    ("💰", "Money bag"),
];

/// Data struct for passing cursor state to submenu via context
#[derive(Clone, Copy, Debug)]
pub struct CursorStateData {
    pub current_cursor: CursorType,
    pub eraser_mode: bool,
    pub values_tooltip: bool,
}

impl CursorStateData {
    /// Where the toolbar stores the state for the cursor menu.
    pub(crate) fn id() -> Id {
        Id::unique("drawing_toolbar_cursor_state")
    }

    /// Whether `cursor` is the mode in force: the eraser, or the cursor while the eraser is off.
    pub(crate) fn is_active(&self, cursor: CursorType) -> bool {
        match cursor {
            CursorType::Eraser => self.eraser_mode,
            _ => !self.eraser_mode && self.current_cursor == cursor,
        }
    }
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
    DrawingToolShortcuts, DrawingToolbarAction, DrawingToolbarConfig,
    categories::{
        AnnotationCategory, CursorsCategory, FibonacciCategory, PatternsCategory,
        ProjectionCategory, ShapesCategory, ToolCategory, TrendLinesCategory,
    },
    components::{draw_separator_styled, draw_tool_button, render_svg_at_rect_themed},
    data, icons,
    state::ToolbarState,
    submenu_builder::{flyout_id, section_header, show_flyout},
    utilities::{
        HideMenu, LockButton, MagnetButton, MeasureButton, RemoveMenu, StayInDrawingButton,
        TemplatesButton, ZoomInButton, ZoomOutButton,
    },
};

/// Drawing toolbar - main UI component
#[derive(Clone, Default)]
pub struct DrawingToolbar {
    /// Toolbar state (selected tool, favorites, recent, etc.)
    pub state: ToolbarState,
    /// Configuration (width, spacing, colors, etc.)
    pub config: DrawingToolbarConfig,
    /// Pos of the expanded category button (for submenu alignment)
    expanded_category_rect: Option<Rect>,
    /// Controls the host hid.
    hidden: HashSet<Control>,
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

    /// Shows or hides `control`: Trading (the hide positions and orders rows) and Templates
    /// (the template rows in the Hide flyout and their dialog). The toolbar ignores the other controls.
    pub fn set_control_visible(&mut self, control: Control, visible: bool) {
        crate::ui::app_state::set_control_visible(&mut self.hidden, control, visible);
        if control == Control::Templates && !visible {
            self.state.show_template_name_input = false;
        }
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
        let mut action = DrawingToolbarAction::None;

        // A tool's shortcut selects it, unless the user is typing.
        if !crate::widget::keyboard::shortcuts_suppressed(ui.ctx())
            && !ui.ctx().text_edit_focused()
            && let Some(tool) = DrawingToolShortcuts::default().check(ui.ctx())
        {
            self.select_tool(tool);
            action = DrawingToolbarAction::SelectTool(tool);
        }

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
            // Buttons and separators carry their own token gaps.
            child_ui.spacing_mut().item_spacing = Vec2::ZERO;
            // The scroll bar floats over the 52 px column, so it never narrows the buttons.
            child_ui.spacing_mut().scroll.floating = true;

            let content = egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                .show(&mut child_ui, |ui| self.render_toolbar_content(ui))
                .inner;
            if content != DrawingToolbarAction::None {
                action = content;
            }
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
            && let Some(template_action) = TemplatesButton::show_name_dialog(
                ui,
                sidebar_rect,
                &mut self.state.template_name_input,
            )
        {
            self.state.show_template_name_input = false;
            self.state.template_name_input.clear();
            if template_action != DrawingToolbarAction::None {
                action = template_action;
            }
        }

        // === TOOL SEARCH ===
        if self.state.search_open {
            self.show_search(ui, sidebar_rect);
        }

        // === SUBMENUS (outside scroll area) ===
        if let Some(ref category) = self.state.expanded_category.clone()
            && let Some(tool_action) = self.draw_submenu(ui, category)
        {
            action = tool_action;
        }

        ui.allocate_rect(sidebar_rect, Sense::hover());
        action
    }

    /// Render the main toolbar content
    fn render_toolbar_content(&mut self, ui: &mut Ui) -> DrawingToolbarAction {
        let mut action = DrawingToolbarAction::None;

        // Add top padding
        ui.add_space(self.config.padding);

        // "/" opens the tool search, unless a text field has the keyboard.
        if !self.state.search_open
            && !crate::widget::keyboard::shortcuts_suppressed(ui.ctx())
            && !ui.ctx().text_edit_focused()
            && ui.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Slash))
        {
            self.state.search_open = true;
            self.state.search_query.clear();
            ui.memory_mut(|m| m.request_focus(Self::search_id()));
        }

        // === CURSORS BUTTON ===
        action = self.render_cursors_btn(ui, action);

        // === TOOL CATEGORIES ===
        action = self.render_tool_categories(ui, action);

        // Separator
        draw_separator_styled(ui, self.config.width);

        // === BOTTOM TOOLS (using utilities modules) ===
        self.render_bottom_tools(ui, action)
    }

    fn search_id() -> Id {
        Id::unique("drawing_toolbar_search")
    }

    /// The tool search field, in a flyout beside the top of the bar. It closes with its button
    /// or Escape.
    fn show_search(&mut self, ui: &Ui, sidebar_rect: Rect) {
        let anchor = Rect::from_min_size(
            sidebar_rect.min,
            Vec2::new(
                self.config.width,
                DESIGN_TOKENS.sizing.toolbar.button_height,
            ),
        );
        let theme = ui.active_theme();
        let mut open = true;
        let mut close_clicked = false;
        let query = &mut self.state.search_query;
        Popup::new(
            Id::unique("drawing_toolbar_search_flyout"),
            ui.ctx().clone(),
            anchor,
            ui.layer_id(),
        )
        .open_bool(&mut open)
        .align(egui::RectAlign::RIGHT_START)
        .gap(theme.spacing.sm)
        .frame(egui::Frame::menu(ui.style()))
        .close_behavior(egui::PopupCloseBehavior::IgnoreClicks)
        .show(|ui| {
            ui.horizontal(|ui| {
                egui::TextEdit::singleline(query)
                    .id(Self::search_id())
                    .hint_text("Search tools")
                    .desired_width(DESIGN_TOKENS.sizing.drawing_toolbar_ext.submenu_width_lg)
                    .show(ui);
                close_clicked = IconButton::new(&embedded_icons::CLOSE)
                    .tooltip("Close search")
                    .show(ui)
                    .clicked();
            });
        });
        self.state.search_open = open && !close_clicked;
        if !self.state.search_open {
            self.state.search_query.clear();
        }
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

        // Zoom is a mode of its own, so it takes the highlight from the cursor.
        let is_selected = self.state.sel_tool.is_none() && !self.state.zoom_mode_active;

        // Simple button without arrow
        let response = draw_tool_button(ui, cursor_icon, cursor_tooltip, is_selected);
        self.menu_button(ui, "Cursors", &response);

        action
    }

    /// A click on `response` opens the flyout `category`, or closes it when it is open; the
    /// flyout opens beside the button.
    fn menu_button(&mut self, ui: &Ui, category: &str, response: &egui::Response) {
        if response.clicked() {
            self.toggle_category(ui.ctx(), category, response.rect);
        }
        if self.state.expanded_category.as_deref() == Some(category) {
            self.expanded_category_rect = Some(response.rect);
        }
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
            self.menu_button(ui, category, &response);
        }

        action
    }

    /// Render bottom tools section
    fn render_bottom_tools(
        &mut self,
        ui: &mut Ui,
        mut action: DrawingToolbarAction,
    ) -> DrawingToolbarAction {
        let width = self.config.width;

        // Measure tool
        let measure_active = self.state.sel_tool == Some(DrawingToolType::Measure);
        if let Some(a) = MeasureButton::show(ui, measure_active) {
            self.select_tool(DrawingToolType::Measure);
            action = a;
        }

        // Zoom In
        if let Some(a) = ZoomInButton::show(ui, self.state.zoom_mode_active) {
            self.state.zoom_mode_active = !self.state.zoom_mode_active;
            action = a;
        }

        // Zoom Out (only if history)
        if self.state.has_zoom_history
            && let Some(a) = ZoomOutButton::show(ui)
        {
            action = a;
        }

        // Separator
        draw_separator_styled(ui, width);

        // Magnet button
        let response = MagnetButton::show(ui, self.state.magnet_mode);
        self.menu_button(ui, "Magnet", &response);

        // Keep Drawing
        if let Some(a) = StayInDrawingButton::show(ui, self.state.stay_in_drawing_mode) {
            self.state.stay_in_drawing_mode = !self.state.stay_in_drawing_mode;
            action = a;
        }

        // Lock All Drawings
        if let Some(a) = LockButton::show(ui, self.state.lock_all) {
            self.state.lock_all = !self.state.lock_all;
            action = a;
        }

        // Hide dropdown
        let response = HideMenu::show(ui);
        self.menu_button(ui, "Hide", &response);

        // Separator
        draw_separator_styled(ui, width);

        // Remove dropdown
        let response = RemoveMenu::show(ui);
        self.menu_button(ui, "Remove", &response);

        action
    }

    /// Open the flyout `category` beside `rect`, or close it when it is the open one.
    fn toggle_category(&mut self, ctx: &egui::Context, category: &str, rect: Rect) {
        if self.state.expanded_category.as_deref() == Some(category) {
            self.close_flyout(ctx);
        } else {
            self.state.expanded_category = Some(category.to_string());
            self.expanded_category_rect = Some(rect);
            Popup::open_id(ctx, flyout_id(category));
        }
    }

    fn close_flyout(&mut self, ctx: &egui::Context) {
        if let Some(category) = self.state.expanded_category.take() {
            Popup::close_id(ctx, flyout_id(&category));
        }
        self.expanded_category_rect = None;
    }

    /// Show the flyout of `category`, apply what the user picked in it to the toolbar state and
    /// return the action for the host.
    fn draw_submenu(&mut self, ui: &mut Ui, category: &str) -> Option<DrawingToolbarAction> {
        let anchor = self.expanded_category_rect?;
        let ctx = ui.ctx().clone();
        ctx.data_mut(|d| {
            d.insert_temp(
                CursorStateData::id(),
                CursorStateData {
                    current_cursor: self.state.current_cursor_type,
                    eraser_mode: self.state.eraser_mode_enabled,
                    values_tooltip: self.state.values_tooltip_on_long_press,
                },
            );
        });

        let trading = !self.hidden.contains(&Control::Trading);
        let templates = !self.hidden.contains(&Control::Templates);
        let sel_tool = self.state.sel_tool;
        let state = &mut self.state;
        let action = show_flyout(ui, category, anchor, |ui| match category {
            "Magnet" => MagnetButton::show_submenu(ui, state.magnet_mode, state.magnet_type),
            "Hide" => {
                let mut action = HideMenu::show_submenu(ui, trading, state);
                if templates {
                    ui.separator();
                    section_header(ui, "DRAWING TEMPLATES", ui.min_rect().width());
                    action = action.or(TemplatesButton::show_submenu(ui, &state.templates));
                }
                action
            }
            "Remove" => RemoveMenu::show_submenu(ui),
            "Icons/Emojis" => {
                let mut builder = SubmenuBuilder::new(ui);
                for (icon_name, name) in ICONS {
                    builder = builder.add_text_item_with_action(
                        format!("{icon_name}  {name}"),
                        format!("Place a {} icon", name.to_lowercase()),
                        move || Some(DrawingToolbarAction::InsertEmoji { icon_name }),
                    );
                }
                builder.show()
            }
            _ => {
                let favorites = &mut state.favorites;
                let action = match category {
                    "Cursors" => CursorsCategory.render_submenu(ui, sel_tool, favorites),
                    "Lines" => TrendLinesCategory.render_submenu(ui, sel_tool, favorites),
                    "Fibonacci" => FibonacciCategory.render_submenu(ui, sel_tool, favorites),
                    "Patterns" => PatternsCategory.render_submenu(ui, sel_tool, favorites),
                    "Projection" => ProjectionCategory.render_submenu(ui, sel_tool, favorites),
                    "Brushes/Shapes" => ShapesCategory.render_submenu(ui, sel_tool, favorites),
                    "Text/Annotations" => {
                        AnnotationCategory.render_submenu(ui, sel_tool, favorites)
                    }
                    _ => DrawingToolbarAction::None,
                };
                (action != DrawingToolbarAction::None).then_some(action)
            }
        })
        .flatten();

        // Escape and a click outside close the flyout through egui's popup state.
        if !Popup::is_id_open(&ctx, flyout_id(category)) {
            self.state.expanded_category = None;
            self.expanded_category_rect = None;
        }
        let action = self.apply_pick(action?);
        // A tool picked while keeping the drawing mode on leaves the flyout open for the next one.
        let keeps_open = matches!(action, Some(DrawingToolbarAction::SelectTool(_)))
            && self.state.stay_in_drawing_mode;
        if !keeps_open {
            self.close_flyout(&ctx);
        }
        action
    }

    /// Mirror a pick from a flyout in the toolbar state. Returns the action for the host, or
    /// `None` when the pick has no effect to report.
    fn apply_pick(&mut self, action: DrawingToolbarAction) -> Option<DrawingToolbarAction> {
        use DrawingToolbarAction as A;
        match &action {
            A::SelectTool(tool) => self.select_tool(*tool),
            A::SetCursorType(cursor) => {
                self.state.current_cursor_type = *cursor;
                self.state.eraser_mode_enabled = false;
                self.state.zoom_mode_active = false;
            }
            A::ToggleEraserMode => {
                self.state.eraser_mode_enabled = !self.state.eraser_mode_enabled;
                self.state.zoom_mode_active = false;
            }
            A::ToggleValuesTooltip => {
                self.state.values_tooltip_on_long_press = !self.state.values_tooltip_on_long_press;
            }
            A::SetMagnetType(kind) => {
                self.state.magnet_type = *kind;
                self.state.magnet_mode = true;
            }
            // "Off" while the magnet is already off changes nothing, and the host's toggle would
            // switch it on.
            A::ToggleMagnet if !self.state.magnet_mode => return None,
            A::ToggleMagnet => self.state.magnet_mode = false,
            A::HideAllDrawings => self.state.hide_drawings = !self.state.hide_drawings,
            A::HideAllIndicators => self.state.hide_indicators = !self.state.hide_indicators,
            A::HideAllPoss => self.state.hide_positions = !self.state.hide_positions,
            A::HideAllOrders => self.state.hide_orders = !self.state.hide_orders,
            A::InsertEmoji { .. } => self.select_tool(DrawingToolType::FontIcon),
            A::OpenTemplateMenu => {
                // "Save as template" closes the menu and opens the name dialog.
                self.state.show_template_name_input = true;
                self.state.template_name_input.clear();
                return None;
            }
            _ => {}
        }
        Some(action)
    }

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

        let area_res = egui::Area::new(egui::Id::unique("color_picker_popup"))
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
            ui.painter().rect_stroke(
                rect,
                DESIGN_TOKENS.rounding.sm,
                Stroke::new(DESIGN_TOKENS.stroke.thick, ui.active_theme().ui.accent),
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

        egui::Area::new(egui::Id::unique("floating_drawing_toolbar"))
            .fixed_pos(position)
            .show(ui.ctx(), |ui| {
                egui::Frame::new()
                    .fill(ui.visuals().panel_fill)
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
                                    ui.active_theme().ui.accent
                                } else if response.hovered() {
                                    ui.visuals().widgets.hovered.bg_fill
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
                                ui.active_theme().ui.accent
                            } else if response.hovered() {
                                ui.visuals().widgets.hovered.bg_fill
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

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, Event, Key, Modifiers, RawInput};

    /// The bar keeps its grid and utility order, with distinct selection and magnet fills.
    #[test]
    fn drawing_bar_icons_keep_their_grid_pitch_and_state_colours() {
        use egui::{Color32, Pos2, Shape, vec2};

        let mut custom = crate::theme::Theme::dark();
        custom.ui.text = Color32::GREEN;
        custom.ui.accent_text = Color32::YELLOW;
        for (theme, text) in [
            (crate::theme::Theme::dark(), Color32::from_gray(219)),
            (crate::theme::Theme::light(), Color32::from_gray(26)),
            (custom, Color32::GREEN),
        ] {
            for (selected, magnet) in [
                (None, false),
                (None, true),
                (Some(DrawingToolType::TrendLine), false),
            ] {
                let ctx = Context::default();
                egui_extras::install_image_loaders(&ctx);
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut toolbar = DrawingToolbar::new();
                toolbar.state.sel_tool = selected;
                toolbar.state.magnet_mode = magnet;
                let mut origin = Pos2::ZERO;
                let mut shapes = Vec::new();
                for _ in 0..2 {
                    let mut output = ctx.run_ui(
                        RawInput {
                            events: vec![Event::PointerMoved(egui::pos2(26.0, 61.0))],
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(52.0, 1200.0))),
                            ..Default::default()
                        },
                        |ui| {
                            ui.spacing_mut().item_spacing = Vec2::ZERO;
                            origin = ui.cursor().min;
                            toolbar.render_toolbar_content(ui);
                        },
                    );
                    shapes = std::mem::take(&mut output.shapes);
                    output.drop_without_applying_deltas();
                }
                let icons: Vec<_> = shapes
                    .iter()
                    .filter_map(|s| match &s.shape {
                        Shape::Rect(r) if r.brush.is_some() => Some((r.rect, r.fill)),
                        _ => None,
                    })
                    .collect();
                assert_eq!(icons.len(), 15);
                for (i, y) in [
                    23.0, 61.0, 99.0, 137.0, 175.0, 213.0, 251.0, 289.0, 344.0, 382.0, 437.0,
                    475.0, 513.0, 551.0, 606.0,
                ]
                .into_iter()
                .enumerate()
                {
                    let colour = if (i == 0 && selected.is_none()) || (i == 1 && selected.is_some())
                    {
                        text
                    } else if i == 10 && magnet {
                        if theme.dark {
                            theme.ui.panel_bg
                        } else {
                            Color32::WHITE
                        }
                    } else {
                        text
                    };
                    assert_eq!(
                        icons[i],
                        (
                            Rect::from_center_size(origin + vec2(26.0, y), vec2(24.0, 24.0)),
                            colour
                        ),
                        "icon {i}, selected {selected:?}"
                    );
                }
                let selected_y = if selected.is_none() { 5.5 } else { 43.5 };
                let selected_fill = if theme.dark {
                    theme.ui.btn_bg_active
                } else {
                    Color32::from_gray(235)
                };
                let hovered_fill = if selected.is_none() {
                    theme.ui.btn_bg_hover
                } else {
                    selected_fill
                };
                assert!(shapes.iter().any(|s| matches!(
                    &s.shape,
                    Shape::Rect(r) if r.fill == hovered_fill
                        && r.rect == Rect::from_min_size(origin + vec2(8.5, 43.5), vec2(35.0, 35.0))
                )));
                assert!(shapes.iter().any(|s| matches!(
                    &s.shape,
                    Shape::Rect(r) if r.fill == selected_fill
                        && r.rect == Rect::from_min_size(origin + vec2(8.5, selected_y), vec2(35.0, 35.0))
                )));
                if magnet {
                    let fill = if theme.dark {
                        theme.ui.text
                    } else {
                        Color32::from_gray(26)
                    };
                    assert!(shapes.iter().any(|s| matches!(
                        &s.shape,
                        Shape::Rect(r) if r.fill == fill
                            && r.rect == Rect::from_center_size(origin + vec2(26.0, 437.0), vec2(35.0, 35.0))
                    )));
                }
                let separator = if theme.dark {
                    Color32::from_gray(74)
                } else {
                    Color32::from_gray(235)
                };
                assert!(shapes.iter().any(|s| matches!(
                    &s.shape,
                    Shape::Rect(r) if r.fill == separator
                        && r.rect == Rect::from_min_size(origin + vec2(8.0, 316.0), vec2(36.0, 1.0))
                )));
            }
        }
    }

    /// The visibility flyout saves and loads templates unless the host hides them. An empty
    /// save dialog stays open until cancelled.
    #[test]
    fn the_visibility_flyout_keeps_template_actions() {
        use crate::ui::drawing_toolbar::DrawingTemplate;
        use egui::{PointerButton, Pos2, Rect, Shape, vec2};

        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for (visible, label) in [
                (true, "Save as template…"),
                (true, "Saved line"),
                (false, "Saved line"),
            ] {
                let ctx = Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut toolbar = DrawingToolbar::new();
                toolbar.state.templates.push(DrawingTemplate {
                    id: "line".into(),
                    name: "Saved line".into(),
                    drawing_type: "TrendLine".into(),
                    is_default: false,
                });
                toolbar.set_control_visible(Control::Templates, visible);
                toolbar.toggle_category(
                    &ctx,
                    "Hide",
                    Rect::from_min_size(Pos2::new(0.0, 100.0), vec2(52.0, 38.0)),
                );
                let frame = |toolbar: &mut DrawingToolbar, events| {
                    let mut action = DrawingToolbarAction::None;
                    let mut output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(640.0, 450.0))),
                            events,
                            ..Default::default()
                        },
                        |ui| action = toolbar.show_with_action(ui),
                    );
                    let shapes = std::mem::take(&mut output.shapes);
                    output.drop_without_applying_deltas();
                    (action, shapes)
                };
                frame(&mut toolbar, vec![]);
                let (_, shapes) = frame(&mut toolbar, vec![]);
                let row = shapes.iter().find_map(|s| match &s.shape {
                    Shape::Text(t) if t.galley.text() == label => {
                        Some(t.pos + t.galley.size() / 2.0)
                    }
                    _ => None,
                });
                assert_eq!(row.is_some(), visible);
                if let Some(pos) = row {
                    frame(&mut toolbar, vec![Event::PointerMoved(pos)]);
                    for pressed in [true, false] {
                        let (action, _) = frame(
                            &mut toolbar,
                            vec![Event::PointerButton {
                                pos,
                                button: PointerButton::Primary,
                                pressed,
                                modifiers: Modifiers::NONE,
                            }],
                        );
                        if !pressed && label == "Saved line" {
                            assert_eq!(
                                action,
                                DrawingToolbarAction::LoadTemplate {
                                    template_id: "line".into(),
                                }
                            );
                        }
                    }
                    let saving = label == "Save as template…";
                    assert_eq!(toolbar.state.show_template_name_input, saving);
                    assert!(toolbar.state.expanded_category.is_none());
                    frame(&mut toolbar, vec![]);
                    assert_eq!(toolbar.state.show_template_name_input, saving);
                    frame(
                        &mut toolbar,
                        vec![Event::Key {
                            key: Key::Escape,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: Modifiers::NONE,
                        }],
                    );
                    assert!(!toolbar.state.show_template_name_input);
                }
            }
        }
    }

    /// Alt+T selects the trend line tool, unless a text field has the keyboard, where it must not
    /// switch tools while the user types.
    #[test]
    fn a_tool_shortcut_selects_the_tool_unless_a_text_field_has_focus() {
        let alt_t = Event::Key {
            key: Key::T,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers: Modifiers::ALT,
        };
        let press_alt_t = |text_focused: bool| {
            let ctx = Context::default();
            let mut toolbar = DrawingToolbar::new();
            let mut text = String::new();
            let mut frame = |events: Vec<Event>| {
                let mut action = DrawingToolbarAction::None;
                ctx.run_ui(
                    RawInput {
                        events,
                        ..Default::default()
                    },
                    |ui| {
                        if text_focused {
                            ui.text_edit_singleline(&mut text).request_focus();
                        }
                        action = toolbar.show_with_action(ui);
                    },
                )
                .drop_without_applying_deltas();
                action
            };
            frame(vec![]);
            frame(vec![alt_t.clone()])
        };
        assert_eq!(
            press_alt_t(false),
            DrawingToolbarAction::SelectTool(DrawingToolType::TrendLine)
        );
        assert_eq!(press_alt_t(true), DrawingToolbarAction::None);
    }
}
