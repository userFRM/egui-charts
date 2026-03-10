//! Top Toolbar Widget
//!
//! Complete toolbar implementation with:
//! - Symbol search pill
//! - Compare/add symbol
//! - Intervals dropdown
//! - Chart style dropdown
//! - Indicators button
//! - Indicator templates
//! - Alert button
//! - Replay button
//! - Undo/Redo btns
//! - Screenshot button
//! - Settings dropdown
//! - Save button
//! - Publish button
//! - Fullscreen button

use crate::icons::icons as embedded_icons;
use crate::styles::responsive::LayoutContext;
use crate::styles::{icons, typography};
use egui::{Pos2, Rect, Response, Sense, Ui, Vec2};

use crate::ext::UiExt;

use super::{
    actions::TopToolbarAction,
    buttons::{
        IconTextButton, PillButtonFilled, PillButtonOutlined, TextIconButton, ToolbarIconButton,
    },
    components::{
        chart_grid_selector::ChartGridSelector,
        chart_type_selector::{ChartTypeAction, ChartTypeSelector},
        layout_menu::{LayoutAction, LayoutMenu},
        sync_button::SyncButton,
        timeframe_selector::{Timeframe, TimeframeAction, TimeframeSelector, TimeframeUnit},
    },
    config::TopToolbarConfig,
    state::TopToolbarState,
};
use crate::model::ChartType;
use crate::theming;
use crate::tokens::DESIGN_TOKENS;

/// Top toolbar widget
pub struct TopToolbar {
    pub config: TopToolbarConfig,
    pub state: TopToolbarState,
    /// Timeframe selector dropdown
    pub timeframe_selector: TimeframeSelector,
    /// Chart type selector dropdown
    pub chart_type_selector: ChartTypeSelector,
    /// Layout selector dropdown
    pub layout_menu: LayoutMenu,
    /// Chart grid layout selector
    pub chart_grid_selector: ChartGridSelector,
    /// Multi-chart sync options button
    pub sync_button: SyncButton,
}

impl Default for TopToolbar {
    fn default() -> Self {
        Self {
            config: TopToolbarConfig::default(),
            state: TopToolbarState::default(),
            timeframe_selector: TimeframeSelector::new(Timeframe::new(1, TimeframeUnit::Day)),
            chart_type_selector: ChartTypeSelector::new(ChartType::Candles),
            layout_menu: LayoutMenu::new(),
            chart_grid_selector: ChartGridSelector::default(),
            sync_button: SyncButton::new(),
        }
    }
}

impl TopToolbar {
    /// Create a new top toolbar with default configuration
    pub fn new() -> Self {
        Self::default()
    }

    /// Set a custom toolbar configuration
    pub fn with_config(mut self, config: TopToolbarConfig) -> Self {
        self.config = config;
        self
    }

    /// Show the full toolbar, reading the active symbol from app state.
    pub fn show_with_state(
        &mut self,
        ui: &mut Ui,
        app_state: &dyn crate::ui::app_state::ChartAppState,
    ) -> TopToolbarAction {
        let _layout_ctx = LayoutContext::from_egui(ui.ctx());

        ui.horizontal(|ui| {
            ui.set_height(self.config.height);
            self.show_contents_desktop(ui, app_state.active_symbol())
        })
        .inner
    }

    /// Show the toolbar inline (same layout, no outer frame).
    pub fn show_inline(
        &mut self,
        ui: &mut Ui,
        app_state: &dyn crate::ui::app_state::ChartAppState,
    ) -> TopToolbarAction {
        ui.horizontal(|ui| {
            ui.set_height(self.config.height);
            self.show_contents_desktop(ui, app_state.active_symbol())
        })
        .inner
    }

    /// Desktop layout: horizontal toolbar contents.
    fn show_contents_desktop(&mut self, ui: &mut Ui, symbol: &str) -> TopToolbarAction {
        let mut action = TopToolbarAction::None;

        // Menu button (burger) - leftmost
        self.show_menu_button(ui, &mut action);
        self.toolbar_separator(ui);

        // Left section
        self.show_symbol_controls(ui, &mut action, symbol);
        self.show_chart_controls(ui, &mut action);
        self.show_action_controls(ui, &mut action);

        // Right section (right-aligned)
        ui.right_aligned(|ui| {
            self.show_right_controls(ui, &mut action);
        });

        action
    }

    /// Helper to draw separator
    fn toolbar_separator(&self, ui: &mut Ui) {
        ui.toolbar_separator();
    }

    /// Symbol search pill, compare button, and timeframe selector.
    fn show_symbol_controls(&mut self, ui: &mut Ui, action: &mut TopToolbarAction, symbol: &str) {
        if self.symbol_search_pill(ui, symbol).clicked() {
            *action = TopToolbarAction::OpenSymbolSearch;
        }
        ui.space_sm();

        if ToolbarIconButton::new(&embedded_icons::PLUS, "Compare or Add Symbol", &self.config)
            .show(ui)
            .clicked()
        {
            *action = TopToolbarAction::OpenCompare;
        }
        self.toolbar_separator(ui);

        let tf_action = self.timeframe_selector.show(ui);
        if let TimeframeAction::Select(tf) = tf_action {
            let display = tf.display();
            *action = TopToolbarAction::IntervalSelected(display);
        }
        self.toolbar_separator(ui);
    }

    /// Chart type and indicators controls
    fn show_chart_controls(&mut self, ui: &mut Ui, action: &mut TopToolbarAction) {
        let ct_action = self.chart_type_selector.show(ui);
        if let ChartTypeAction::Select(ct) = ct_action {
            let name = ct.name().to_string();
            // Note: AppState is updated via dispatch, no need to set local state
            *action = TopToolbarAction::ChartStyleSelected(name);
        }
        self.toolbar_separator(ui);

        if IconTextButton::new(&embedded_icons::INDICATORS, "Indicators", &self.config)
            .show(ui)
            .clicked()
        {
            *action = TopToolbarAction::OpenIndicators;
        }
        ui.space_xs();

        if ToolbarIconButton::new(
            &embedded_icons::LAYOUT_GRID,
            "Indicator templates",
            &self.config,
        )
        .show(ui)
        .clicked()
        {
            *action = TopToolbarAction::OpenIndicatorTemplates;
        }
        self.toolbar_separator(ui);
    }

    /// Alert, replay, undo/redo controls (Trade moved to right side)
    fn show_action_controls(&mut self, ui: &mut Ui, action: &mut TopToolbarAction) {
        if IconTextButton::new(&embedded_icons::BELL, "Alert", &self.config)
            .show(ui)
            .clicked()
        {
            *action = TopToolbarAction::CreateAlert;
        }
        ui.space_sm();

        if IconTextButton::new(&embedded_icons::REPLAY, "Replay", &self.config)
            .show(ui)
            .clicked()
        {
            self.state.toggle_replay();
            *action = TopToolbarAction::ToggleReplay;
        }
        self.toolbar_separator(ui);

        if ToolbarIconButton::new(&embedded_icons::UNDO, "Undo", &self.config)
            .show(ui)
            .clicked()
        {
            *action = TopToolbarAction::Undo;
        }
        ui.space_xs();

        if ToolbarIconButton::new(&embedded_icons::REDO, "Redo", &self.config)
            .show(ui)
            .clicked()
        {
            *action = TopToolbarAction::Redo;
        }
    }

    /// Right-aligned controls (RTL)
    /// Order: Publish | Trade | Camera | Fullscreen | Settings | QuickSearch | sep | Save▼ | LayoutSetup
    fn show_right_controls(&mut self, ui: &mut Ui, action: &mut TopToolbarAction) {
        ui.space_lg();

        // Publish button (outlined pill) - rightmost
        if PillButtonOutlined::new("Publish", "Publish Idea")
            .show(ui)
            .clicked()
        {
            *action = TopToolbarAction::Publish;
        }
        ui.space_lg();

        // Trade button (blue filled pill)
        if PillButtonFilled::new("Trade", "Open Trading Panel")
            .show(ui)
            .clicked()
        {
            *action = TopToolbarAction::OpenTradingPanel;
        }
        ui.space_sm();

        // Camera/Screenshot
        if ToolbarIconButton::new(&embedded_icons::CAMERA, "Take Screenshot", &self.config)
            .show(ui)
            .clicked()
        {
            *action = TopToolbarAction::TakeScreenshot;
        }
        ui.space_xs();

        // Fullscreen
        if ToolbarIconButton::new(&embedded_icons::FULLSCREEN, "Fullscreen", &self.config)
            .show(ui)
            .clicked()
        {
            *action = TopToolbarAction::ToggleFullscreen;
        }
        ui.space_xs();

        // Settings
        if ToolbarIconButton::new(&embedded_icons::SETTINGS_GEAR, "Settings", &self.config)
            .show(ui)
            .clicked()
        {
            *action = TopToolbarAction::OpenSettings;
        }
        ui.space_xs();

        // Quick Search
        if ToolbarIconButton::new(&embedded_icons::QUICK_SEARCH, "Quick Search", &self.config)
            .show(ui)
            .clicked()
        {
            *action = TopToolbarAction::OpenSymbolSearch;
        }

        // Separator
        ui.toolbar_separator();

        // Save dropdown (text + chevron)
        if TextIconButton::new(
            "Save",
            &embedded_icons::CHEVRON_DOWN,
            "Save Chart",
            &self.config,
        )
        .show(ui)
        .clicked()
        {
            *action = TopToolbarAction::Save;
        }
        ui.space_sm();

        // Layout Setup icon (21x19)
        if ToolbarIconButton::new(
            &embedded_icons::LAYOUT_SETUP,
            "Manage Layouts",
            &self.config,
        )
        .show(ui)
        .clicked()
        {
            *action = TopToolbarAction::Layout(LayoutAction::OpenMenu);
        }
        ui.space_sm();
    }

    /// Menu button (burger icon)
    /// Click area centered in ~52px width, with optional notification badge
    fn show_menu_button(&self, ui: &mut Ui, action: &mut TopToolbarAction) {
        let btn_size = DESIGN_TOKENS.sizing.button_dialog;
        let icon_size = DESIGN_TOKENS.sizing.icon_lg;

        let (rect, response) = ui.allocate_exact_size(Vec2::splat(btn_size), Sense::click());

        if ui.is_rect_visible(rect) {
            // Hover background
            if response.hovered() {
                ui.painter().rect_filled(
                    rect,
                    DESIGN_TOKENS.rounding.button,
                    theming::hover_color(ui),
                );
            }

            // Menu icon
            let icon_rect = Rect::from_center_size(rect.center(), Vec2::splat(icon_size));
            let icon_color = if response.hovered() {
                theming::active_icon_color(ui)
            } else {
                theming::icon_color(ui)
            };
            embedded_icons::MENU
                .as_image_tinted(Vec2::splat(icon_size), icon_color)
                .paint_at(ui, icon_rect);

            // Optional: Notification badge
            // Uncomment and customize if needed
            // let badge_size = Vec2::new(16.0, 14.0);
            // let badge_pos = Pos2::new(rect.right() - 4.0, rect.top() - 2.0);
            // let badge_rect = Rect::from_min_size(badge_pos, badge_size);
            // ui.painter().rect_filled(badge_rect, 7.0, theming::publish_button_bg(ui));
            // ui.painter().text(
            //     badge_rect.center(),
            //     egui::Align2::CENTER_CENTER,
            //     "10",
            //     egui::FontId::proportional(9.0),
            //     egui::Color32::WHITE,
            // );
        }

        if response.clicked() {
            *action = TopToolbarAction::OpenMenu;
        }

        response.on_hover_text("Menu");
    }

    /// Symbol search pill (with internal separator and diamond)
    ///
    /// Layout: [Search icon | Symbol text | separator | Diamond | Chevron]
    ///
    /// `symbol` is read from AppState (single source of truth).
    fn symbol_search_pill(&self, ui: &mut Ui, symbol: &str) -> Response {
        let icon_size = icons::SM_MD;
        let small_icon_size = icons::XS;
        let padding = DESIGN_TOKENS.spacing.lg;
        // Use provided symbol or fallback to default display
        let display_symbol = if symbol.is_empty() { "AAPL" } else { symbol };
        let text_width = display_symbol.len() as f32 * 7.5; // Slightly more space for text
        let separator_width = DESIGN_TOKENS.stroke.hairline;
        let separator_padding = DESIGN_TOKENS.spacing.sm;

        // Total width: search + padding + text + padding + separator + padding + diamond + padding + chevron + padding
        let total_width = icon_size
            + padding
            + text_width
            + separator_padding
            + separator_width
            + separator_padding
            + small_icon_size
            + DESIGN_TOKENS.spacing.xs
            + small_icon_size
            + padding;
        let height = DESIGN_TOKENS.sizing.button_md;

        let (rect, response) =
            ui.allocate_exact_size(Vec2::new(total_width, height), Sense::click());

        if ui.is_rect_visible(rect) {
            // Background (use theme-aware pill color)
            let bg_color = if response.is_pointer_button_down_on() {
                theming::btn_bg_pressed(ui)
            } else if response.hovered() {
                theming::btn_bg_hover(ui)
            } else {
                theming::pill_bg(ui)
            };

            ui.painter()
                .rect_filled(rect, DESIGN_TOKENS.rounding.pill, bg_color);

            let mut x = rect.min.x + padding;

            // Search icon
            let icon_pos = Pos2::new(x, rect.center().y - icon_size / 2.0);
            let icon_rect = Rect::from_min_size(icon_pos, Vec2::splat(icon_size));
            let icon_color = if response.hovered() {
                theming::icon_hover_color(ui)
            } else {
                theming::icon_normal(ui)
            };
            embedded_icons::QUICK_SEARCH
                .as_image_tinted(Vec2::splat(icon_size), icon_color)
                .paint_at(ui, icon_rect);
            x += icon_size + padding;

            // Symbol text
            ui.painter().text(
                Pos2::new(x, rect.center().y),
                egui::Align2::LEFT_CENTER,
                display_symbol,
                egui::FontId::proportional(typography::MD),
                theming::text_color(ui),
            );
            x += text_width + separator_padding;

            // Vertical separator inside pill
            let sep_height = height * 0.5;
            let sep_y = rect.center().y - sep_height / 2.0;
            ui.painter().rect_filled(
                Rect::from_min_size(Pos2::new(x, sep_y), Vec2::new(separator_width, sep_height)),
                DESIGN_TOKENS.rounding.none,
                theming::separator_color(ui),
            );
            x += separator_width + separator_padding;

            // Diamond icon
            let diamond_pos = Pos2::new(x, rect.center().y - small_icon_size / 2.0);
            let diamond_rect = Rect::from_min_size(diamond_pos, Vec2::splat(small_icon_size));
            embedded_icons::CANDLESTICK
                .as_image_tinted(Vec2::splat(small_icon_size), icon_color)
                .paint_at(ui, diamond_rect);
            x += small_icon_size + DESIGN_TOKENS.spacing.xs;

            // Chevron down
            let chevron_pos = Pos2::new(x, rect.center().y - small_icon_size / 2.0);
            let chevron_rect = Rect::from_min_size(chevron_pos, Vec2::splat(small_icon_size));
            embedded_icons::CHEVRON_DOWN
                .as_image_tinted(Vec2::splat(small_icon_size), icon_color)
                .paint_at(ui, chevron_rect);
        }

        response
    }
}

impl TopToolbar {
    /// Sync toolbar state from application state.
    pub fn sync_from_app_state(&mut self, app_state: &dyn crate::ui::app_state::ChartAppState) {
        self.state.sync_from_app_state(app_state);
    }
}
