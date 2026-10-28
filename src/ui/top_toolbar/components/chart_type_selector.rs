//! Chart Type Selector
//!
//! A dropdown menu for selecting different chart visualization types.
//! Uses [`crate::model::ChartType`] for the data definition.

use crate::icons::{Icon, icons as embedded_icons};
use crate::localization as l10n;
use crate::model::ChartType;
use crate::styles::icons as icon_sizes;
use crate::theme::{Theme, ThemeExt};
use crate::tokens::DESIGN_TOKENS;
use crate::ui::top_toolbar::buttons::Trigger;
use crate::ui::top_toolbar::components::timeframe_selector::{
    STAR_SLOT_W, favorite_star, favorite_strip, menu_list_height, menu_row_focus,
    retain_non_favorite,
};
use crate::ui_kit::list_item::{ContentContext, ListItem, ListItemContent};
use egui::{Color32, Rect, Role, Ui, WidgetInfo, pos2, vec2};
use std::collections::HashSet;

/// Height of a chart type row.
const ROW_H: f32 = 40.0;
/// Space between two groups of rows, with the separator line in its middle.
const GROUP_GAP: f32 = 13.0;

/// Extension trait for ChartType to provide UI-specific functionality
pub trait ChartTypeUiExt {
    /// Get the icon for this chart type
    fn icon(&self) -> &'static Icon;
}

impl ChartTypeUiExt for ChartType {
    fn icon(&self) -> &'static Icon {
        match self {
            ChartType::Bars => &embedded_icons::CHART_BARS,
            ChartType::Candles => &embedded_icons::CHART_CANDLES,
            ChartType::HollowCandles => &embedded_icons::CHART_HOLLOW_CANDLES,
            ChartType::VolumeCandles => &embedded_icons::CHART_VOLUME_CANDLES,
            ChartType::Line => &embedded_icons::CHART_LINE,
            ChartType::LineWithMarkers => &embedded_icons::CHART_LINE_MARKERS,
            ChartType::StepLine => &embedded_icons::CHART_STEP_LINE,
            ChartType::Area => &embedded_icons::CHART_AREA,
            ChartType::HlcArea => &embedded_icons::CHART_HLC_AREA,
            ChartType::Baseline => &embedded_icons::CHART_BASELINE,
            ChartType::Columns => &embedded_icons::CHART_COLUMNS,
            ChartType::HighLow => &embedded_icons::CHART_HIGH_LOW,
            ChartType::TimePriceOpportunity => &embedded_icons::CHART_TPO,
            ChartType::VolumeFootprint => &embedded_icons::CHART_VOLUME_FOOTPRINT,
            ChartType::SessionVolumeProfile => &embedded_icons::CHART_SESSION_VOLUME_PROFILE,
            ChartType::LineBreak => &embedded_icons::CHART_LINE_BREAK,
            ChartType::Kagi => &embedded_icons::CHART_KAGI,
            ChartType::Range => &embedded_icons::CHART_RANGE,
            ChartType::PointAndFigure => &embedded_icons::CHART_POINT_FIGURE,
            ChartType::Renko => &embedded_icons::CHART_RENKO,
            ChartType::Heikin => &embedded_icons::CHART_HEIKIN_ASHI,
        }
    }
}

/// Configuration for chart type selector
#[derive(Debug, Clone)]
pub struct ChartTypeSelectorConfig {
    /// Minimum width of each chart type button; it is otherwise sized to its icon.
    pub btn_width: f32,
    /// Minimum width of the menu.
    pub dropdown_width: f32,
    /// Not read: rows are laid out by the menu.
    pub item_height: f32,
    /// Not read: the menu takes its colours from the applied theme.
    pub bg_color: Color32,
    /// Not read: the menu takes its colours from the applied theme.
    pub hover_color: Color32,
    /// Not read: the menu takes its colours from the applied theme.
    pub sel_color: Color32,
    /// Not read: the menu takes its colours from the applied theme.
    pub text_color: Color32,
    /// Not read: the menu takes its colours from the applied theme.
    pub muted_color: Color32,
}

impl Default for ChartTypeSelectorConfig {
    fn default() -> Self {
        // Colors are TRANSPARENT to signal that colors should be fetched from
        // ui.style().visuals at render time for proper theme support.
        Self {
            btn_width: 0.0,
            dropdown_width: DESIGN_TOKENS.sizing.dialog.submenu_width
                + crate::theme::Theme::default().spacing.xxl,
            item_height: DESIGN_TOKENS.sizing.button_lg,
            bg_color: Color32::TRANSPARENT,
            hover_color: Color32::TRANSPARENT,
            sel_color: Color32::TRANSPARENT,
            text_color: Color32::TRANSPARENT,
            muted_color: Color32::TRANSPARENT,
        }
    }
}

/// Action from chart type selector
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum ChartTypeAction {
    /// No action requested.
    None,
    /// Select the supplied value.
    Select(ChartType),
    /// Toggle the supplied chart type's favorite state.
    ToggleFavorite(ChartType),
}

/// Host-owned chart type settings. Save with the user's settings and restore with
/// [`ChartTypeSelector::set_preferences`]. The selector owns no storage.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ChartTypePreferences {
    /// Pinned chart types in menu order.
    #[serde(default)]
    pub favorites: Vec<ChartType>,
}

/// Chart type selector widget
pub struct ChartTypeSelector {
    /// Current chart type
    pub current: ChartType,
    /// Is dropdown open
    is_open: bool,
    /// Favorite chart types
    pub favorites: HashSet<ChartType>,
    last_non_favorite: Option<ChartType>,
    /// Configuration
    pub config: ChartTypeSelectorConfig,
}

impl Default for ChartTypeSelector {
    fn default() -> Self {
        Self::new(ChartType::Candles)
    }
}

impl ChartTypeSelector {
    /// Create a selector with `initial` selected and no favorites.
    pub fn new(initial: ChartType) -> Self {
        Self {
            current: initial,
            is_open: false,
            favorites: HashSet::new(),
            last_non_favorite: None,
            config: ChartTypeSelectorConfig::default(),
        }
    }

    /// Use the supplied display and layout options.
    pub fn with_config(mut self, config: ChartTypeSelectorConfig) -> Self {
        self.config = config;
        self
    }

    /// Export the favorites for the host's persisted user settings.
    pub fn preferences(&self) -> ChartTypePreferences {
        ChartTypePreferences {
            favorites: self.favorites_in_menu_order(),
        }
    }

    /// Restore saved favorites.
    pub fn set_preferences(&mut self, preferences: ChartTypePreferences) {
        self.favorites = preferences.favorites.into_iter().collect();
    }

    fn favorites_in_menu_order(&self) -> Vec<ChartType> {
        ChartType::all()
            .iter()
            .copied()
            .filter(|chart_type| self.favorites.contains(chart_type))
            .collect()
    }

    /// Show favorite chart type buttons and the chart type menu trigger.
    pub fn show(&mut self, ui: &mut Ui) -> ChartTypeAction {
        let extra = retain_non_favorite(&mut self.last_non_favorite, self.current, &self.favorites);
        let (btn_res, clicked) = favorite_strip(
            ui,
            self.current,
            self.favorites_in_menu_order(),
            extra,
            self.is_open,
            "Chart type",
            |_, chart_type| Self::button(chart_type, self.config.btn_width),
        );
        let mut selected = ChartTypeAction::None;
        if let Some(chart_type) = clicked {
            self.current = chart_type;
            selected = ChartTypeAction::Select(chart_type);
        }

        let theme = ui.active_theme();
        let id = ui.make_persistent_id("chart_type_menu");
        let max_height = menu_list_height(ui, btn_res.rect, ROW_H);
        let action = egui::Popup::menu(&btn_res)
            .id(id)
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .align_alternatives(&[])
            .frame(egui::Frame::popup(ui.style()).fill(theme.ui.panel_bg))
            .gap(theme.spacing.xs)
            .show(|ui| self.menu_ui(ui, &theme, max_height))
            .map_or(ChartTypeAction::None, |menu| menu.inner);
        self.is_open = egui::Popup::is_id_open(ui.ctx(), id);
        if action == ChartTypeAction::None {
            selected
        } else {
            action
        }
    }

    /// An icon-only toolbar button for `chart_type`, named by the type's name.
    fn button(chart_type: ChartType, min_width: f32) -> Trigger<'static> {
        Trigger {
            icon: Some(chart_type.icon()),
            label: String::new(),
            name: chart_type.name().to_owned(),
            min_width: min_width.max(40.0),
            filled: false,
        }
    }

    fn menu_ui(&mut self, ui: &mut Ui, theme: &Theme, max_height: f32) -> ChartTypeAction {
        let mut action = ChartTypeAction::None;
        // Rows sit edge to edge, and the scroll bar is visible whenever the list overflows.
        let spacing = ui.spacing_mut();
        spacing.item_spacing.y = 0.0;
        spacing.scroll.floating = false;
        spacing.scroll.bar_width = 3.0;
        spacing.scroll.fade.strength = 0.0;

        ui.set_width(256.0_f32.max(self.config.dropdown_width));
        ui.visuals_mut().selection.bg_fill = theme.ui.text;
        ui.visuals_mut().selection.stroke.color = theme.ui.panel_bg;
        // A popup is first measured inside a 400 px high area; this lets the list use the room below.
        ui.set_max_height(max_height);

        let current = self.current;
        egui::ScrollArea::vertical()
            .max_height(max_height)
            .auto_shrink([false, true])
            .show(ui, |ui| {
                for chart_type in ChartType::all() {
                    if matches!(
                        chart_type,
                        ChartType::Line
                            | ChartType::Area
                            | ChartType::Columns
                            | ChartType::VolumeFootprint
                            | ChartType::Heikin
                    ) {
                        ui.add(egui::Separator::default().spacing(GROUP_GAP));
                    }
                    let mut toggle_favorite = false;
                    let content = ChartTypeRow {
                        chart_type: *chart_type,
                        favorite: self.favorites.contains(chart_type),
                        theme,
                        toggle_favorite: &mut toggle_favorite,
                    };
                    let row = ListItem::new()
                        .selected(current == *chart_type)
                        .height(ROW_H)
                        .show(ui, content);
                    if toggle_favorite {
                        action = ChartTypeAction::ToggleFavorite(*chart_type);
                    } else if row.clicked() {
                        action = ChartTypeAction::Select(*chart_type);
                    }
                }
            });

        if let ChartTypeAction::ToggleFavorite(chart_type) = action
            && !self.favorites.remove(&chart_type)
        {
            self.favorites.insert(chart_type);
        }
        if let ChartTypeAction::Select(chart_type) = action {
            self.current = chart_type;
            ui.close();
        }
        action
    }
}

/// One chart type row: icon, name, a check on the current type, and a favourite star.
struct ChartTypeRow<'a> {
    chart_type: ChartType,
    favorite: bool,
    theme: &'a Theme,
    toggle_favorite: &'a mut bool,
}

impl ListItemContent for ChartTypeRow<'_> {
    fn ui(self, ui: &mut Ui, context: &ContentContext<'_>) -> egui::Response {
        let theme = self.theme;
        let rect = context.rect;
        let color = if context.selected {
            theme.ui.panel_bg
        } else {
            theme.ui.text
        };
        let translated_label = l10n::text(ui.ctx(), self.chart_type.name());
        context.response.widget_info(|| {
            WidgetInfo::selected(
                Role::MenuItemRadio,
                context.response.enabled(),
                context.selected,
                &translated_label,
            )
        });

        let icon_rect = Rect::from_min_size(
            pos2(rect.min.x, rect.center().y - icon_sizes::MD / 2.0),
            vec2(icon_sizes::MD, icon_sizes::MD),
        );
        self.chart_type
            .icon()
            .as_image_tinted(icon_rect.size(), color)
            .paint_at(ui, icon_rect);

        ui.painter().text(
            pos2(icon_rect.max.x + theme.spacing.lg, rect.center().y),
            egui::Align2::LEFT_CENTER,
            l10n::text(ui.ctx(), self.chart_type.name()),
            egui::FontId::proportional(14.0),
            color,
        );

        *self.toggle_favorite = favorite_star(ui, context, self.favorite, true, color);

        if context.selected {
            // Left of the star's slot.
            let check = Rect::from_min_size(
                pos2(
                    rect.max.x - STAR_SLOT_W - icon_sizes::SM,
                    rect.center().y - icon_sizes::SM / 2.0,
                ),
                vec2(icon_sizes::SM, icon_sizes::SM),
            );
            embedded_icons::CHECK
                .as_image_tinted(check.size(), color)
                .paint_at(ui, check);
        }
        menu_row_focus(ui, context.response);
        context.response.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::{Harness, kittest::Queryable};

    /// Starred chart types become toolbar buttons in menu order, followed by the latest chosen
    /// type that is not starred and a menu chevron, and unstarring them restores one button.
    #[test]
    fn favorites_show_as_buttons_with_the_latest_other_type() {
        let mut harness = Harness::builder()
            .with_context_setup(egui_extras::install_image_loaders)
            .with_size(vec2(800.0, 1400.0))
            .build_ui_state(
                |ui, selector: &mut ChartTypeSelector| {
                    ui.horizontal(|ui| selector.show(ui));
                },
                ChartTypeSelector::new(ChartType::Candles),
            );
        harness.run();
        // Buttons left to right, by name.
        let strip = |harness: &Harness<'_, ChartTypeSelector>| {
            let mut buttons: Vec<_> = [
                "Chart type",
                "Bars",
                "Line",
                "Area",
                "Candles",
                "Heikin Ashi",
            ]
            .into_iter()
            .filter_map(|name| Some((harness.query_by_label(name)?.rect().min.x, name)))
            .collect();
            buttons.sort_by(|a, b| a.0.total_cmp(&b.0));
            buttons
                .into_iter()
                .map(|(_, name)| name)
                .collect::<Vec<_>>()
        };
        assert_eq!(strip(&harness), ["Candles"]);

        harness.get_by_label("Candles").click();
        harness.run();
        // Stars sit on every row in menu order and starring turns one into "Remove from favorites", so
        // star from the last row up to keep the earlier indices.
        for index in [7, 4, 0] {
            harness
                .query_all_by_label("Add to favorites")
                .nth(index)
                .unwrap()
                .click();
            harness.run();
        }
        harness.key_press(egui::Key::Escape);
        harness.run();
        assert_eq!(
            strip(&harness),
            ["Bars", "Line", "Area", "Candles", "Chart type"]
        );

        harness.get_by_label("Line").click();
        harness.run();
        assert_eq!(harness.state().current, ChartType::Line);
        assert_eq!(
            strip(&harness),
            ["Bars", "Line", "Area", "Candles", "Chart type"]
        );

        harness.get_by_label("Chart type").click();
        harness.run();
        harness.get_by_label("Heikin Ashi").click();
        harness.run();
        assert_eq!(harness.state().current, ChartType::Heikin);
        assert_eq!(
            strip(&harness),
            ["Bars", "Line", "Area", "Heikin Ashi", "Chart type"]
        );

        let saved = harness.state().preferences();
        assert_eq!(
            saved.favorites,
            [ChartType::Bars, ChartType::Line, ChartType::Area]
        );
        let mut restored = ChartTypeSelector::new(ChartType::Candles);
        restored.set_preferences(saved);
        assert_eq!(restored.favorites, harness.state().favorites);

        harness.get_by_label("Chart type").click();
        harness.run();
        for _ in 0..3 {
            harness
                .query_all_by_label("Remove from favorites")
                .next()
                .unwrap()
                .click();
            harness.run();
        }
        harness.key_press(egui::Key::Escape);
        harness.run();
        assert_eq!(strip(&harness), ["Heikin Ashi"]);
    }
}
