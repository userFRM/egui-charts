//! Drawing Toolbar
//!
//! This is the main coordinator for the left sidebar toolbar.
//! It delegates rendering to:
//! - `components/` for reusable UI elements
//! - `categories/` for tool category submenus
//! - `utilities/` for bottom toolbar btns

mod color_picker;
mod floating;

use super::categories::cursors::CursorType;
use crate::drawings::DrawingToolType;
use crate::icons::{Icon, icons as embedded_icons};
use crate::localization as l10n;
use crate::styles::responsive::LayoutContext;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::Control;
use crate::ui_kit::buttons::IconButton;
use egui::{Id, Key, Modifiers, Popup, Rect, Sense, Ui, Vec2};
use std::collections::HashSet;

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

/// Drawing and cursor choices shared by the host and drawing toolbar.
///
/// Copy host choices into the toolbar before showing it, then apply its returned
/// actions to the chart and host state.
#[derive(Clone, Debug)]
pub struct DrawingToolbarDomainState {
    /// The chart's active drawing tool, or `None` after construction finishes.
    pub sel_tool: Option<DrawingToolType>,
    /// Whether snapping is enabled.
    pub magnet_mode: bool,
    /// The snapping strength, retained while snapping is off.
    pub magnet_type: MagnetType,
    /// Whether magnet targets include visible study plots.
    pub snap_to_indicators: bool,
    /// The chart cursor used while the eraser is off.
    pub current_cursor_type: CursorType,
    /// Whether the chart is erasing drawings.
    pub eraser_mode_enabled: bool,
    /// Whether the active drawing tool remains selected after construction.
    pub stay_in_drawing_mode: bool,
    /// Default colour for new drawings, in straight RGBA.
    pub drawing_color: [u8; 4],
}

// Import from modular components
use super::{
    DrawingToolShortcuts, DrawingToolbarAction, DrawingToolbarConfig,
    categories::{CursorsCategory, ToolCategory, trend_lines::render_registered_tool_submenu},
    components::{draw_separator_styled, draw_tool_button},
    data, icons,
    state::{MagnetType, ToolbarState},
    submenu_builder::{flyout_id, show_flyout},
    utilities::{
        FavoritesButton, HideMenu, LockButton, MagnetButton, MeasureButton, RemoveMenu,
        StayInDrawingButton, TemplatesButton, ZoomInButton, ZoomOutButton,
    },
};

/// Drawing toolbar - main UI component
#[derive(Clone, Default)]
pub struct DrawingToolbar {
    /// Built-in availability for the bar, flyouts and shortcuts. Clone the chart manager's
    /// registry here after changing it; hosts use its custom catalogue for their own picker.
    pub registry: crate::drawings::DrawingToolRegistry,
    /// Toolbar state (selected tool, favorites, recent, etc.)
    pub state: ToolbarState,
    /// Configuration (width, spacing, colors, etc.)
    pub config: DrawingToolbarConfig,
    /// Pos of the expanded category button (for submenu alignment)
    expanded_category_rect: Option<Rect>,
    /// Controls the host hid.
    hidden: HashSet<Control>,
    /// The egui pass the favourites strip was last drawn in.
    strip_pass: Option<u64>,
}

impl DrawingToolbar {
    /// Create a [`DrawingToolbar`] with default settings.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create with custom configuration
    pub fn with_config(mut self, config: DrawingToolbarConfig) -> Self {
        self.config = config;
        self
    }

    /// Shows or hides `control`: Trading (the hide positions and orders rows) and Templates
    /// (the standalone template name dialog). The toolbar ignores the other controls.
    pub fn set_control_visible(&mut self, control: Control, visible: bool) {
        crate::ui::app_state::set_control_visible(&mut self.hidden, control, visible);
        if control == Control::Templates && !visible {
            self.state.show_template_name_input = false;
        }
    }

    // === State Sync Methods ===

    /// Copy the host's drawing and cursor choices before [`Self::show_with_action`].
    pub fn sync_from_domain_state(&mut self, domain: &DrawingToolbarDomainState) {
        if domain.sel_tool.is_some()
            || domain.eraser_mode_enabled
            || domain.current_cursor_type != self.state.current_cursor_type
        {
            self.state.zoom_mode_active = false;
        }
        self.state.sel_tool = domain.sel_tool;
        self.state.magnet_mode = domain.magnet_mode;
        self.state.magnet_type = domain.magnet_type;
        self.state.snap_to_indicators = domain.snap_to_indicators;
        self.state.current_cursor_type = domain.current_cursor_type;
        self.state.eraser_mode_enabled = domain.eraser_mode_enabled;
        self.state.stay_in_drawing_mode = domain.stay_in_drawing_mode;
        self.state.drawing_color = domain.drawing_color;
    }

    /// Read the toolbar's drawing and cursor choices for synchronization with the host.
    pub fn get_domain_state(&self) -> DrawingToolbarDomainState {
        DrawingToolbarDomainState {
            sel_tool: self.state.sel_tool,
            magnet_mode: self.state.magnet_mode,
            magnet_type: self.state.magnet_type,
            snap_to_indicators: self.state.snap_to_indicators,
            current_cursor_type: self.state.current_cursor_type,
            eraser_mode_enabled: self.state.eraser_mode_enabled,
            stay_in_drawing_mode: self.state.stay_in_drawing_mode,
            drawing_color: self.state.drawing_color,
        }
    }

    // === State management methods ===

    /// Select the tool if its registry entry is enabled.
    pub fn select_tool(&mut self, tool: DrawingToolType) {
        if self.registry.availability(tool).enabled {
            self.state.select_tool(tool);
        }
    }

    /// Record a tool in the recent selection list.
    pub fn add_to_recent(&mut self, tool: DrawingToolType) {
        self.state.add_to_recent(tool);
    }

    /// Clear the toolbar's selected tool.
    pub fn clear_selection(&mut self) {
        self.state.clear_selection();
    }

    /// Toggle a tool's membership in favorites.
    pub fn toggle_favorite(&mut self, tool: DrawingToolType) {
        self.state.toggle_favorite(tool);
    }

    /// Whether the tool is marked as a favorite.
    pub fn is_favorite(&self, tool: &DrawingToolType) -> bool {
        self.state.is_favorite(tool)
    }

    /// Toggle the toolbar's magnet setting.
    pub fn toggle_magnet(&mut self) {
        self.state.toggle_magnet();
    }

    /// Toggle whether the selected tool stays active after completing a drawing.
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
            && !ui.ctx().data(|data| {
                data.get_temp::<crate::widget::ShortcutMap>(Id::NULL.with("chart_shortcuts"))
                    .is_some()
            })
            && !Popup::is_any_open(ui.ctx())
            && !ui.ctx().text_edit_focused()
            && let Some(tool) = DrawingToolShortcuts::default().check(ui.ctx())
            && self.registry.availability(tool).enabled
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

        // === FAVOURITE TOOLS STRIP ===
        if let Some(tool) = self.show_floating(ui, sidebar_rect.right_top() + Vec2::splat(8.0)) {
            action = DrawingToolbarAction::SelectTool(tool);
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
            && !ui.ctx().data(|data| {
                data.get_temp::<crate::widget::ShortcutMap>(Id::NULL.with("chart_shortcuts"))
                    .is_some()
            })
            && !crate::widget::keyboard::shortcuts_suppressed(ui.ctx())
            && !Popup::is_any_open(ui.ctx())
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
                    .hint_text(l10n::text(ui.ctx(), "Search tools"))
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

            tools.retain(|tool| self.registry.availability(*tool).visible);

            // Filter by search
            if !self.state.search_query.is_empty() {
                let query = self.state.search_query.to_lowercase();
                tools.retain(|tool| {
                    tool.as_str().to_lowercase().contains(&query)
                        || l10n::text(ui.ctx(), tool.as_str())
                            .to_lowercase()
                            .contains(&query)
                });
            }

            let is_icons = category == "Icons/Emojis";
            if tools.is_empty() {
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
                        .filter(|tool| tools.contains(tool))
                        .or_else(|| tools.first().copied())
                }
            } else {
                self.state
                    .category_last_tool
                    .get(category)
                    .copied()
                    .filter(|tool| tools.contains(tool))
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
        let measure = self.registry.availability(DrawingToolType::Measure);
        if measure.visible
            && let Some(a) = ui
                .add_enabled_ui(measure.enabled, |ui| {
                    MeasureButton::show(ui, measure_active)
                })
                .inner
        {
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

        // Favourite tools strip
        let strip_shown = self.state.floating_visible && !self.state.favorites.is_empty();
        if FavoritesButton::show(ui, strip_shown) {
            self.state.floating_visible = !strip_shown;
        }

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
        let sel_tool = self.state.sel_tool;
        let starred = self.state.favorites.len();
        let state = &mut self.state;
        let registry = &self.registry;
        let action = show_flyout(ui, category, anchor, |ui| match category {
            "Magnet" => MagnetButton::show_submenu(
                ui,
                state.magnet_mode,
                state.magnet_type,
                state.snap_to_indicators,
            ),
            "Hide" => HideMenu::show_submenu(ui, trading, state),
            "Remove" => RemoveMenu::show_submenu(ui),
            "Icons/Emojis" => state.glyph_picker.show(ui, registry),
            _ => {
                let favorites = &mut state.favorites;
                let action = if category == "Cursors" {
                    CursorsCategory.render_submenu(ui, sel_tool, favorites)
                } else {
                    render_registered_tool_submenu(
                        ui,
                        data::get_category_sections(category),
                        sel_tool,
                        favorites,
                        registry,
                    )
                };
                (action != DrawingToolbarAction::None).then_some(action)
            }
        })
        .flatten();
        // Starring a tool shows the strip it joins.
        if self.state.favorites.len() > starred {
            self.state.floating_visible = true;
        }

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
        let tool = match &action {
            A::SelectTool(tool) => Some(*tool),
            A::InsertEmoji { .. } => Some(DrawingToolType::FontIcon),
            A::InsertGlyph { icon_name, .. } => Some(if icon_name.starts_with("sticker:") {
                DrawingToolType::Sticker
            } else {
                DrawingToolType::FontIcon
            }),
            _ => None,
        };
        if tool.is_some_and(|tool| !self.registry.availability(tool).enabled) {
            return None;
        }
        match &action {
            A::SelectTool(tool) => self.select_tool(*tool),
            A::SetCursorType(cursor) => {
                self.clear_selection();
                self.state.current_cursor_type = *cursor;
                self.state.eraser_mode_enabled = false;
            }
            A::ToggleEraserMode => {
                self.clear_selection();
                self.state.eraser_mode_enabled = !self.state.eraser_mode_enabled;
            }
            A::ToggleValuesTooltip => {
                self.state.values_tooltip_on_long_press = !self.state.values_tooltip_on_long_press;
            }
            A::SetSnapToIndicators(enabled) => self.state.snap_to_indicators = *enabled,
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
            A::InsertGlyph { icon_name, .. } => {
                self.select_tool(if icon_name.starts_with("sticker:") {
                    DrawingToolType::Sticker
                } else {
                    DrawingToolType::FontIcon
                })
            }
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Context, Event, Key, Modifiers, Pos2, RawInput};

    /// The floating strip lists the favorite tools as they change, stays where it was first put
    /// and where the host moves it, and hides until shown again.
    #[test]
    fn floating_strip_follows_favorites_and_keeps_position_and_visibility() {
        let ctx = Context::default();
        let id = Id::NULL.with("floating_drawing_toolbar");
        let mut toolbar = DrawingToolbar::new();
        toolbar.state.favorites = vec![
            DrawingToolType::TrendLine,
            DrawingToolType::HorizontalLine,
            DrawingToolType::FibonacciRetracement,
        ];
        let frame = |toolbar: &mut DrawingToolbar, at: Pos2| {
            ctx.run_ui(RawInput::default(), |ui| {
                toolbar.show_floating(ui, at);
            })
            .drop_without_applying_deltas();
            ctx.memory(|memory| memory.area_rect(id))
        };
        let three = frame(&mut toolbar, Pos2::new(20.0, 30.0)).unwrap();
        assert_eq!(three.min, Pos2::new(20.0, 30.0));
        toolbar.state.favorites.pop();
        let two = frame(&mut toolbar, Pos2::new(300.0, 300.0)).unwrap();
        assert!(
            two.width() < three.width() - 20.0,
            "{two:?} after unstarring from {three:?}"
        );
        assert_eq!(two.min, three.min, "the strip stays where it was put");
        assert_eq!(toolbar.state.floating_pos, Some(three.min));
        toolbar.state.floating_visible = false;
        let before = ctx.memory(|memory| memory.area_rect(id));
        frame(&mut toolbar, Pos2::ZERO);
        assert_eq!(ctx.memory(|memory| memory.area_rect(id)), before);
    }

    /// The flyout searches all families, emits the picked glyph and shows session recents first.
    #[test]
    fn glyph_flyout_searches_places_and_shows_recents() {
        fn frame(
            toolbar: &mut DrawingToolbar,
            ctx: &egui::Context,
            events: Vec<egui::Event>,
        ) -> (Option<DrawingToolbarAction>, Vec<egui::Shape>) {
            let mut action = None;
            let output = ctx.run_ui(
                egui::RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    action = toolbar.draw_submenu(ui, "Icons/Emojis");
                },
            );
            let shapes = output.shapes.iter().map(|s| s.shape.clone()).collect();
            output.drop_without_applying_deltas();
            (action, shapes)
        }
        fn click(
            toolbar: &mut DrawingToolbar,
            ctx: &egui::Context,
            at: Pos2,
        ) -> Option<DrawingToolbarAction> {
            frame(toolbar, ctx, vec![egui::Event::PointerMoved(at)]);
            frame(
                toolbar,
                ctx,
                [true, false]
                    .map(|pressed| egui::Event::PointerButton {
                        pos: at,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    })
                    .to_vec(),
            )
            .0
        }
        fn label(shapes: &[egui::Shape], value: &str) -> Option<Rect> {
            shapes.iter().find_map(|s| match s {
                egui::Shape::Text(t) if t.galley.job.text == value => {
                    Some(t.visual_bounding_rect())
                }
                _ => None,
            })
        }
        for dark in [false, true] {
            for (width, height) in [(240.0, 320.0), (320.0, 400.0), (1440.0, 900.0)] {
                let ctx = egui::Context::default();
                ctx.set_visuals(if dark {
                    egui::Visuals::dark()
                } else {
                    egui::Visuals::light()
                });
                l10n::set_translator(&ctx, |english| (english == "Star").then(|| "Etoile".into()));
                let screen = Rect::from_min_size(Pos2::ZERO, Vec2::new(width, height));
                ctx.run_ui(
                    RawInput {
                        screen_rect: Some(screen),
                        ..Default::default()
                    },
                    |_| {},
                )
                .drop_without_applying_deltas();
                let mut toolbar = DrawingToolbar::new();
                toolbar.state.glyph_picker.size = 48.0;
                let anchor = Rect::from_min_size(Pos2::new(10., 100.), Vec2::splat(40.));
                let mut previous = "Search icons, emoji and stickers";
                for (query, glyph) in [
                    ("star", "icon:star"),
                    ("etoile", "icon:star"),
                    ("grinning", "😀"),
                    ("idea", "sticker:idea"),
                    ("", "sticker:idea"),
                ] {
                    toolbar.toggle_category(&ctx, "Icons/Emojis", anchor);
                    frame(&mut toolbar, &ctx, vec![]);
                    let (_, shapes) = frame(&mut toolbar, &ctx, vec![]);
                    let input =
                        label(&shapes, previous).expect("the flyout must have a search field");
                    assert!(
                        screen.contains_rect(input),
                        "search field outside {screen:?}: {input:?}"
                    );
                    click(&mut toolbar, &ctx, input.center());
                    let modifiers = Modifiers {
                        ctrl: true,
                        command: true,
                        ..Modifiers::NONE
                    };
                    frame(
                        &mut toolbar,
                        &ctx,
                        [true, false]
                            .map(|pressed| egui::Event::Key {
                                key: Key::A,
                                physical_key: None,
                                pressed,
                                repeat: false,
                                modifiers,
                            })
                            .to_vec(),
                    );
                    frame(&mut toolbar, &ctx, vec![egui::Event::Text(query.into())]);
                    if query.is_empty() {
                        frame(
                            &mut toolbar,
                            &ctx,
                            [true, false]
                                .map(|pressed| egui::Event::Key {
                                    key: Key::Backspace,
                                    physical_key: None,
                                    pressed,
                                    repeat: false,
                                    modifiers: Modifiers::NONE,
                                })
                                .to_vec(),
                        );
                    }
                    let (_, shapes) = frame(&mut toolbar, &ctx, vec![]);
                    if query == "star" {
                        assert!(label(&shapes, "Icons: shapes").is_some());
                        assert!(label(&shapes, "Emoji: symbols").is_some());
                        assert!(label(&shapes, "Emoji: faces").is_none());
                    }
                    if query.is_empty() {
                        assert!(
                            label(&shapes, "Recent").unwrap().top()
                                < label(&shapes, "Icons: signals").unwrap().top()
                        );
                    }
                    let cell = shapes
                        .iter()
                        .find_map(|s| match s {
                            egui::Shape::Rect(r) if r.rect.size() == Vec2::splat(40.0) => {
                                Some(r.rect.center())
                            }
                            _ => None,
                        })
                        .unwrap();
                    assert!(
                        screen.contains(cell),
                        "first glyph outside {screen:?}: {cell:?}"
                    );
                    let action = click(&mut toolbar, &ctx, cell);
                    assert_eq!(
                        action,
                        Some(DrawingToolbarAction::InsertGlyph {
                            icon_name: glyph,
                            size: 48.0
                        })
                    );
                    previous = query;
                }
            }
        }
    }

    /// Host snapshots restore modes, clear conflicting zoom and retain menus and tool history.
    #[test]
    fn domain_snapshots_restore_magnet_cursor_and_eraser_modes() {
        for (magnet, cursor, eraser, tool, zoom) in [
            (
                MagnetType::Weak,
                CursorType::Arrow,
                false,
                Some(DrawingToolType::HorizontalLine),
                false,
            ),
            (MagnetType::Strong, CursorType::Dot, false, None, false),
            (MagnetType::OHLC, CursorType::Arrow, true, None, false),
            (MagnetType::Strong, CursorType::Arrow, false, None, true),
        ] {
            let mut source = DrawingToolbar::new();
            source.state.magnet_mode = true;
            source.state.magnet_type = magnet;
            source.state.snap_to_indicators = magnet != MagnetType::Weak;
            source.state.current_cursor_type = cursor;
            source.state.eraser_mode_enabled = eraser;
            source.state.sel_tool = tool;
            let mut target = DrawingToolbar::new();
            target.select_tool(DrawingToolType::TrendLine);
            target.clear_selection();
            target.state.zoom_mode_active = true;
            target.state.snap_to_indicators = !source.state.snap_to_indicators;
            target.state.expanded_category = Some("Magnet".into());
            target.sync_from_domain_state(&source.get_domain_state());
            assert_eq!(target.state.magnet_type, magnet);
            assert_eq!(
                target.state.snap_to_indicators,
                source.state.snap_to_indicators
            );
            assert_eq!(target.state.current_cursor_type, cursor);
            assert_eq!(target.state.eraser_mode_enabled, eraser);
            assert!(target.state.magnet_mode);
            assert_eq!(target.state.sel_tool, tool);
            assert_eq!(target.state.zoom_mode_active, zoom);
            assert_eq!(target.state.expanded_category.as_deref(), Some("Magnet"));
            assert_eq!(
                target.state.recent.front(),
                Some(&DrawingToolType::TrendLine)
            );
        }
    }

    /// Cursor, eraser and drawing picks leave the previous mode before a host synchronises.
    #[test]
    fn flyout_picks_leave_the_previous_drawing_mode() {
        use DrawingToolType::TrendLine;
        use DrawingToolbarAction::{SelectTool, SetCursorType, ToggleEraserMode};

        let mut toolbar = DrawingToolbar::new();
        toolbar.select_tool(TrendLine);
        for (action, tool, eraser) in [
            (SetCursorType(CursorType::Dot), None, false),
            (SelectTool(TrendLine), Some(TrendLine), false),
            (ToggleEraserMode, None, true),
            (SelectTool(TrendLine), Some(TrendLine), false),
            (ToggleEraserMode, None, true),
            (ToggleEraserMode, None, false),
        ] {
            assert_eq!(toolbar.apply_pick(action.clone()), Some(action));
            assert_eq!(toolbar.state.sel_tool, tool);
            assert_eq!(toolbar.state.eraser_mode_enabled, eraser);
            assert_eq!(toolbar.state.current_cursor_type, CursorType::Dot);
        }
        for enabled in [false, true] {
            let mode = toolbar.state.magnet_type;
            let on = toolbar.state.magnet_mode;
            let action = DrawingToolbarAction::SetSnapToIndicators(enabled);
            assert_eq!(toolbar.apply_pick(action.clone()), Some(action));
            assert_eq!(toolbar.state.snap_to_indicators, enabled);
            assert_eq!(toolbar.state.magnet_type, mode);
            assert_eq!(toolbar.state.magnet_mode, on);
        }
        toolbar.registry.set_availability(
            TrendLine,
            crate::drawings::ToolAvailability {
                visible: false,
                enabled: false,
            },
        );
        assert_eq!(toolbar.apply_pick(SelectTool(TrendLine)), None);
        let ctx = Context::default();
        let output = ctx.run_ui(RawInput::default(), |ui| {
            render_registered_tool_submenu(
                ui,
                data::get_category_sections("Lines"),
                None,
                &mut vec![],
                &toolbar.registry,
            );
        });
        let labels: Vec<_> = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                egui::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                _ => None,
            })
            .collect();
        assert!(!labels.contains(&TrendLine.as_str()));
        assert!(labels.contains(&DrawingToolType::HorizontalLine.as_str()));
        output.drop_without_applying_deltas();
    }

    /// The bar keeps its grid and utility order, with distinct selection and magnet fills.
    #[test]
    fn drawing_bar_icons_keep_their_grid_pitch_and_state_colours() {
        use egui::emath::GuiRounding;
        use egui::{Color32, Pos2, Shape, vec2};

        let mut custom = crate::theme::Theme::dark();
        custom.ui.text = Color32::GREEN;
        custom.ui.accent_text = Color32::YELLOW;
        for (theme, text) in [
            (crate::theme::Theme::dark(), Color32::from_gray(219)),
            (crate::theme::Theme::light(), Color32::BLACK),
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
                assert_eq!(icons.len(), 16);
                for (i, y) in [
                    23.0, 61.0, 99.0, 137.0, 175.0, 213.0, 251.0, 289.0, 344.0, 382.0, 437.0,
                    475.0, 513.0, 551.0, 589.0, 644.0,
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
                            Rect::from_center_size(
                                origin + vec2(26.0, y),
                                match i {
                                    1 => vec2(29.0, 29.0),
                                    2 => vec2(34.0, 27.0),
                                    8 => vec2(31.0, 31.0),
                                    _ => vec2(24.0, 24.0),
                                }
                            )
                            .round_to_pixels(1.0),
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

    /// Alt+T selects the trend line tool, unless a text field has the keyboard, where it must not
    /// switch tools while the user types.
    #[test]
    fn a_tool_shortcut_selects_the_tool_unless_a_text_field_has_focus() {
        let press_alt_t = |text_focused: bool, modifiers| {
            let alt_t = Event::Key {
                key: Key::T,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers,
            };
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
            press_alt_t(false, Modifiers::ALT),
            DrawingToolbarAction::SelectTool(DrawingToolType::TrendLine)
        );
        assert_eq!(
            press_alt_t(true, Modifiers::ALT),
            DrawingToolbarAction::None
        );
        assert_eq!(
            press_alt_t(false, Modifiers::ALT.plus(Modifiers::SHIFT)),
            DrawingToolbarAction::None
        );
        for enabled in [false, true] {
            for key in [Key::T, Key::Slash] {
                let ctx = Context::default();
                let mut chart = crate::widget::test_frames::chart(&[10.0], &[20.0]);
                chart.chart_options.keyboard.enabled = enabled;
                if enabled {
                    chart.shortcuts_mut().bindings.clear();
                }
                let mut toolbar = DrawingToolbar::new();
                ctx.run_ui(
                    RawInput {
                        events: vec![Event::Key {
                            key,
                            physical_key: None,
                            pressed: true,
                            repeat: false,
                            modifiers: if key == Key::T {
                                Modifiers::ALT
                            } else {
                                Modifiers::NONE
                            },
                        }],
                        ..Default::default()
                    },
                    |ui| {
                        chart.dispatch_shortcuts(ui.ctx());
                        assert_eq!(toolbar.show_with_action(ui), DrawingToolbarAction::None);
                        assert!(!toolbar.state.search_open);
                        assert!(ui.input(|input| input.key_pressed(key)));
                    },
                )
                .drop_without_applying_deltas();
            }
        }
    }
}
