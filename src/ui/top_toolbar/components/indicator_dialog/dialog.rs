//! Main indicator dialog implementation

use super::{
    actions::IndicatorDialogAction,
    config::IndicatorDialogConfig,
    data::{ConfiguredIndicator, IndicatorInfo, IndicatorParams},
    types::{IndicatorCategory, IndicatorTab, IndicatorType},
};
use crate::icons::icons;
use crate::studies::*;
use crate::styles::focus::draw_focus_ring;
use crate::theme::Theme;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::dialog::DialogFooter;
use crate::ui_kit::filter::FilterWidget;
use crate::ui_kit::modal::Modal;
use crate::ui_kit::sidebar_layout::{SidebarLayout, SidebarTab};
use crate::ui_kit::tab_bar::TabBar;
use crate::ui_kit::{EmptyState, FormGrid};
use egui::text::{LayoutJob, TextWrapping};
use egui::{
    Align2, Color32, FontId, Key, Modifiers, Pos2, Rect, Response, RichText, Role, Sense, Stroke,
    StrokeKind, Ui, Vec2, WidgetInfo,
};
use std::cell::Cell;
use std::collections::HashSet;

/// Width of the category sidebar.
const SIDEBAR_WIDTH: f32 = 140.0;

/// Indicator Dialog
///
/// Modal dialog for searching, selecting, and configuring indicators.
pub struct IndicatorDialog {
    /// Is dialog open
    pub is_open: bool,
    /// Search field and query
    search: FilterWidget,
    /// Focus the search field on the next frame
    focus_search: bool,
    /// Move the focus from the search field to the first row this frame
    focus_first_row: bool,
    /// Active tab
    active_tab: IndicatorTab,
    /// Selected category
    sel_category: IndicatorCategory,
    /// Selected indicator for configuration
    sel_indicator: Option<IndicatorInfo>,
    /// Favorite indicator IDs
    pub favorites: HashSet<String>,
    /// Available indicators
    indicators: Vec<IndicatorInfo>,
    /// Configuration
    config: IndicatorDialogConfig,
    /// Indicator params for configuration
    params: IndicatorParams,
    /// Show configuration panel
    show_config: bool,
}

impl Default for IndicatorDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl IndicatorDialog {
    /// Create a new indicator dialog with built-in indicators
    pub fn new() -> Self {
        Self {
            is_open: false,
            search: FilterWidget::new().placeholder("Search indicators"),
            focus_search: false,
            focus_first_row: false,
            active_tab: IndicatorTab::Indicators,
            sel_category: IndicatorCategory::BuiltIns,
            sel_indicator: None,
            favorites: HashSet::new(),
            indicators: IndicatorInfo::builtin_indicators(),
            config: IndicatorDialogConfig::default(),
            params: IndicatorParams::default(),
            show_config: false,
        }
    }

    /// Set the available indicator list
    pub fn with_indicators(mut self, indicators: Vec<IndicatorInfo>) -> Self {
        self.indicators = indicators;
        self
    }

    /// Set a custom dialog configuration
    pub fn with_config(mut self, config: IndicatorDialogConfig) -> Self {
        self.config = config;
        self
    }

    /// Open the dialog with the search field focused
    pub fn open(&mut self) {
        self.is_open = true;
        self.search.clear();
        self.focus_search = true;
        self.sel_indicator = None;
        self.show_config = false;
    }

    /// Close the dialog
    pub fn close(&mut self) {
        self.is_open = false;
        self.sel_indicator = None;
        self.show_config = false;
    }

    /// Check if dialog is open
    pub fn is_open(&self) -> bool {
        self.is_open
    }

    /// Sync dialog open/close state from an external flag.
    pub fn sync_open_state(&mut self, should_be_open: bool) {
        if should_be_open && !self.is_open {
            self.open();
        } else if !should_be_open && self.is_open {
            self.close();
        }
    }

    /// Show the dialog with live theme colors
    pub fn show(&mut self, ctx: &egui::Context, theme: &Theme) -> IndicatorDialogAction {
        if !self.is_open {
            return IndicatorDialogAction::None;
        }

        // Update config from theme each frame for live color updates
        self.config = IndicatorDialogConfig::from_theme(theme);
        let config = self.config.clone();

        // Escape clears a search before it closes the dialog.
        if !self.search.is_empty() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.search.clear();
        }

        let mut is_open = true;
        let action = Modal::new("indicator_dialog", "Indicators")
            .width(config.width)
            .height(config.height)
            .show(ctx, &mut is_open, |ui| {
                self.draw_content(ui, &config, theme)
            })
            .map_or(IndicatorDialogAction::None, |shown| shown.inner);

        if !is_open {
            self.close();
            return IndicatorDialogAction::Close;
        }
        action
    }

    fn draw_content(
        &mut self,
        ui: &mut Ui,
        config: &IndicatorDialogConfig,
        theme: &Theme,
    ) -> IndicatorDialogAction {
        let search = self.search.show(ui).response;
        if std::mem::take(&mut self.focus_search) {
            search.request_focus();
        }
        self.focus_first_row = search.has_focus() && ui.input(|i| i.key_pressed(Key::ArrowDown));
        ui.add_space(theme.spacing.lg);

        TabBar::new(IndicatorTab::all(), &mut self.active_tab).show(ui);

        // Profiles and patterns have no categories.
        if self.active_tab != IndicatorTab::Indicators {
            self.draw_empty_state(ui);
            return IndicatorDialogAction::None;
        }

        let mut action = IndicatorDialogAction::None;
        // The sidebar runs before the list, so the list shows a category clicked this frame.
        let category = Cell::new(self.sel_category);
        SidebarLayout::new(SIDEBAR_WIDTH).show(
            ui,
            |ui| {
                for item in IndicatorCategory::all() {
                    if SidebarTab::new(item.name(), category.get() == *item)
                        .icon(item.icon())
                        .show(ui)
                    {
                        category.set(*item);
                    }
                }
            },
            |ui| {
                self.sel_category = category.get();
                action = self.draw_results(ui, config, theme);
            },
        );
        action
    }

    /// The indicator list, with the configuration panel beside it once a row is selected.
    fn draw_results(
        &mut self,
        ui: &mut Ui,
        config: &IndicatorDialogConfig,
        theme: &Theme,
    ) -> IndicatorDialogAction {
        // The list keeps its place in the layout when the panel opens, so it keeps its scroll
        // offset and the focused row. A click on a row opens the panel from the next frame,
        // so the list and the panel are never wider than the dialog.
        let show_config = self.show_config;
        let mut action = IndicatorDialogAction::None;
        ui.horizontal_top(|ui| {
            ui.vertical(|ui| {
                if show_config {
                    ui.set_width(config.width - 420.0);
                }
                action = self.draw_indicator_list(ui, config, theme);
            });
            if show_config {
                ui.separator();
                ui.vertical(|ui| {
                    if let Some(a) = self.draw_config_panel(ui, config, theme) {
                        action = a;
                    }
                });
            }
        });
        action
    }

    /// Whether `ind` belongs to the active tab and category and matches the search.
    fn matches(&self, ind: &IndicatorInfo) -> bool {
        let searched = [&ind.name, &ind.desc, &ind.id]
            .iter()
            .any(|text| self.search.matches(text));
        if !searched {
            return false;
        }

        match (self.active_tab, self.sel_category) {
            (IndicatorTab::Indicators, IndicatorCategory::Favorites) => {
                self.favorites.contains(&ind.id)
            }
            (IndicatorTab::Indicators, IndicatorCategory::BuiltIns) => !ind.is_premium,
            (IndicatorTab::Indicators, IndicatorCategory::Premium) => ind.is_premium,
            // No source supplies community indicators, saved profiles or patterns yet.
            (IndicatorTab::Indicators, IndicatorCategory::Community)
            | (IndicatorTab::Profiles | IndicatorTab::Patterns, _) => false,
        }
    }

    fn draw_indicator_list(
        &mut self,
        ui: &mut Ui,
        config: &IndicatorDialogConfig,
        theme: &Theme,
    ) -> IndicatorDialogAction {
        let filtered: Vec<IndicatorInfo> = self
            .indicators
            .iter()
            .filter(|ind| self.matches(ind))
            .cloned()
            .collect();
        if filtered.is_empty() {
            self.draw_empty_state(ui);
            return IndicatorDialogAction::None;
        }

        let mut action = IndicatorDialogAction::None;
        // The scroll bar takes its own gutter, so it never covers the badges.
        ui.spacing_mut().scroll.floating = false;
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                // Room above the first row and below the last for the focus ring.
                let ring = DESIGN_TOKENS.spacing.xs;
                ui.add_space(ring);
                for (i, indicator) in filtered.iter().enumerate() {
                    if let Some(a) = self.draw_indicator_item(ui, config, theme, indicator, i == 0)
                    {
                        action = a;
                    }
                }
                ui.add_space(ring);
            });
        action
    }

    /// The empty state for the active search, tab or category.
    fn draw_empty_state(&self, ui: &mut Ui) {
        let query = self.search.query();
        let no_match = format!("No indicators match \"{query}\"");
        let state = match (self.active_tab, self.sel_category) {
            (IndicatorTab::Profiles, _) => EmptyState::new("No saved profiles"),
            (IndicatorTab::Patterns, _) => EmptyState::new("Nothing here yet"),
            _ if !query.is_empty() => EmptyState::new(&no_match)
                .description("Check the spelling or try another category."),
            (IndicatorTab::Indicators, IndicatorCategory::Favorites) => {
                EmptyState::new("No favorites yet")
                    .icon(&icons::STAR_EMPTY)
                    .description("Star an indicator to keep it here.")
            }
            _ => EmptyState::new("Nothing here yet"),
        };
        state.fill_height().show(ui);
    }

    fn draw_indicator_item(
        &mut self,
        ui: &mut Ui,
        config: &IndicatorDialogConfig,
        theme: &Theme,
        indicator: &IndicatorInfo,
        is_first: bool,
    ) -> Option<IndicatorDialogAction> {
        let desired_size = Vec2::new(ui.available_width(), DESIGN_TOKENS.sizing.button_lg);
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

        let is_favorite = self.favorites.contains(&indicator.id);
        let is_sel = self
            .sel_indicator
            .as_ref()
            .is_some_and(|s| s.id == indicator.id);
        response.widget_info(|| {
            WidgetInfo::selected(Role::Button, ui.is_enabled(), is_sel, &indicator.name)
        });
        if is_first && self.focus_first_row {
            response.request_focus();
        }
        if response.gained_focus() {
            response.scroll_to_me(None);
        }

        let fill = if is_sel {
            config.sel_color
        } else if response.hovered() {
            config.hover_color
        } else {
            Color32::TRANSPARENT
        };
        ui.painter().rect_filled(rect, theme.radius.md, fill);

        // Selected rows take the selection text colour, as egui's selected labels do.
        let (text, secondary) = if is_sel {
            let selected = ui.visuals().selection.stroke.color;
            (selected, selected)
        } else {
            (config.text_color, config.muted_color)
        };
        let star_res = Self::draw_favorite_star(ui, config, rect, is_favorite);
        let text_right = Self::draw_item_badges(ui, theme, rect, indicator, secondary);
        Self::draw_item_text(ui, theme, rect, text_right, indicator, text, secondary);
        draw_focus_ring(ui, &response);

        let enter = ui.input(|i| i.key_pressed(Key::Enter));
        self.handle_item_clicks(indicator, is_favorite, enter, &response, &star_res, rect)
    }

    /// The favourite toggle at the left of a row.
    fn star_rect(row: Rect) -> Rect {
        Rect::from_min_size(
            Pos2::new(row.min.x + DESIGN_TOKENS.spacing.sm, row.min.y),
            Vec2::new(DESIGN_TOKENS.sizing.button_sm, row.height()),
        )
    }

    fn draw_favorite_star(
        ui: &mut Ui,
        config: &IndicatorDialogConfig,
        rect: Rect,
        is_favorite: bool,
    ) -> Response {
        let star_rect = Self::star_rect(rect);
        let star_res = ui.allocate_rect(star_rect, Sense::click());
        let tooltip = if is_favorite {
            "Remove from favorites"
        } else {
            "Add to favorites"
        };
        star_res.widget_info(|| WidgetInfo::labeled(Role::Button, ui.is_enabled(), tooltip));

        let (icon, tint) = if is_favorite {
            (&icons::STAR_FILLED, config.favorite_color)
        } else if star_res.hovered() {
            (&icons::STAR_EMPTY, ui.visuals().text_color())
        } else {
            (&icons::STAR_EMPTY, ui.visuals().weak_text_color())
        };
        let size = Vec2::splat(DESIGN_TOKENS.sizing.icon_sm);
        icon.as_image_tinted(size, tint)
            .paint_at(ui, Rect::from_center_size(star_rect.center(), size));
        draw_focus_ring(ui, &star_res);
        star_res.on_hover_text(tooltip)
    }

    /// The name over the description, centred as a block on the row and cut off before `right`.
    fn draw_item_text(
        ui: &Ui,
        theme: &Theme,
        rect: Rect,
        right: f32,
        indicator: &IndicatorInfo,
        text: Color32,
        secondary: Color32,
    ) {
        let painter = ui.painter();
        let x = rect.min.x + DESIGN_TOKENS.sizing.button_md;
        let line = |s: &str, size: f32, color: Color32| {
            let mut job =
                LayoutJob::simple_singleline(s.to_owned(), FontId::proportional(size), color);
            job.wrap = TextWrapping::truncate_at_width(right - x);
            painter.layout_job(job)
        };
        let name = line(&indicator.name, theme.text.body, text);
        let desc = line(&indicator.desc, theme.text.small, secondary);
        let gap = theme.spacing.xs;
        let top = rect.center().y - (name.size().y + gap + desc.size().y) / 2.0;
        let desc_top = top + name.size().y + gap;
        painter.galley(Pos2::new(x, top), name, text);
        painter.galley(Pos2::new(x, desc_top), desc, secondary);
    }

    /// Outlined badges, right-aligned inside the row padding. Returns where the row text must end.
    fn draw_item_badges(
        ui: &Ui,
        theme: &Theme,
        rect: Rect,
        indicator: &IndicatorInfo,
        color: Color32,
    ) -> f32 {
        let painter = ui.painter();
        let pad = Vec2::new(theme.spacing.sm, theme.spacing.xs);
        let mut right = rect.right() - theme.spacing.lg;
        for (shown, label) in [
            (indicator.is_premium, "PREMIUM"),
            (indicator.is_overlay, "OVERLAY"),
        ] {
            if !shown {
                continue;
            }
            let galley = painter.layout_no_wrap(
                label.to_owned(),
                FontId::proportional(theme.text.small),
                color,
            );
            let badge = Align2::RIGHT_CENTER.align_size_within_rect(
                galley.size() + 2.0 * pad,
                Rect::from_x_y_ranges(rect.left()..=right, rect.y_range()),
            );
            painter.rect_stroke(
                badge,
                theme.radius.sm,
                Stroke::new(DESIGN_TOKENS.stroke.hairline, theme.ui.border),
                StrokeKind::Inside,
            );
            painter.galley(badge.min + pad, galley, color);
            right = badge.left() - theme.spacing.sm;
        }
        right
    }

    fn handle_item_clicks(
        &mut self,
        indicator: &IndicatorInfo,
        is_favorite: bool,
        enter: bool,
        response: &Response,
        star_res: &Response,
        rect: Rect,
    ) -> Option<IndicatorDialogAction> {
        if star_res.clicked() {
            if is_favorite {
                self.favorites.remove(&indicator.id);
            } else {
                self.favorites.insert(indicator.id.clone());
            }
            return Some(IndicatorDialogAction::ToggleFavorite(indicator.id.clone()));
        }

        let on_star = Self::star_rect(rect).contains(response.hover_pos().unwrap_or_default());
        if response.clicked() && !on_star {
            let is_sel = self
                .sel_indicator
                .as_ref()
                .is_some_and(|s| s.id == indicator.id);
            // Enter on the selected row adds it; a click on another row selects it with fresh
            // parameters, and any other click keeps the parameters being edited.
            if is_sel && enter {
                return self.add_selected();
            }
            if !is_sel {
                self.sel_indicator = Some(indicator.clone());
                self.show_config = true;
                self.params = IndicatorParams::default();
            }
        }
        None
    }

    /// Add the selected indicator with its parameters and close the dialog.
    fn add_selected(&mut self) -> Option<IndicatorDialogAction> {
        let indicator_type = self.sel_indicator.as_ref()?.indicator_type?;
        let params = self.params.clone();
        self.close();
        Some(IndicatorDialogAction::AddConfiguredIndicator(
            ConfiguredIndicator {
                indicator_type,
                params,
            },
        ))
    }

    fn draw_config_panel(
        &mut self,
        ui: &mut Ui,
        config: &IndicatorDialogConfig,
        theme: &Theme,
    ) -> Option<IndicatorDialogAction> {
        let indicator = self.sel_indicator.as_ref()?;
        ui.label(
            RichText::new(&indicator.name)
                .size(theme.text.title)
                .strong(),
        );
        let indicator_type = indicator.indicator_type;
        ui.add_space(theme.spacing.lg);

        FormGrid::new("indicator_params").show(ui, |ui| {
            self.draw_indicator_params(ui, config, theme, indicator_type);
        });

        let (add, back) = DialogFooter::new("Add")
            .secondary("Back")
            .primary_enabled(indicator_type.is_some())
            .show(ui);
        if back {
            self.sel_indicator = None;
            self.show_config = false;
        }
        if add {
            return self.add_selected();
        }
        None
    }

    fn draw_indicator_params(
        &mut self,
        ui: &mut Ui,
        config: &IndicatorDialogConfig,
        theme: &Theme,
        indicator_type: Option<IndicatorType>,
    ) {
        let p = &mut self.params;
        match indicator_type {
            Some(
                IndicatorType::SMA
                | IndicatorType::EMA
                | IndicatorType::WMA
                | IndicatorType::VWMA
                | IndicatorType::KeltnerChannels
                | IndicatorType::DonchianChannels
                | IndicatorType::Aroon,
            ) => {
                Self::draw_int_param(ui, "Period", &mut p.period, 1..=500);
            }
            Some(IndicatorType::BollingerBands) => {
                Self::draw_int_param(ui, "Period", &mut p.period, 1..=500);
                Self::draw_float_param(ui, "Std Dev", &mut p.bb_std_dev, 0.1..=5.0, 0.1);
            }
            Some(
                IndicatorType::RSI
                | IndicatorType::CCI
                | IndicatorType::ATR
                | IndicatorType::ADX
                | IndicatorType::MFI
                | IndicatorType::WilliamsR
                | IndicatorType::ROC
                | IndicatorType::Momentum
                | IndicatorType::ChaikinMoneyFlow,
            ) => {
                Self::draw_int_param(ui, "Period", &mut p.period, 2..=500);
            }
            Some(IndicatorType::MACD) => {
                Self::draw_int_param(ui, "Fast", &mut p.macd_fast, 1..=500);
                Self::draw_int_param(ui, "Slow", &mut p.macd_slow, 1..=500);
                Self::draw_int_param(ui, "Signal", &mut p.macd_signal, 1..=500);
            }
            Some(IndicatorType::Stochastic) => {
                Self::draw_int_param(ui, "%K Period", &mut p.stoch_k, 1..=500);
                Self::draw_int_param(ui, "%D Period", &mut p.stoch_d, 1..=500);
            }
            Some(IndicatorType::SuperTrend) => {
                Self::draw_int_param(ui, "Period", &mut p.period, 1..=500);
                Self::draw_float_param(
                    ui,
                    "Multiplier",
                    &mut p.supertrend_multiplier,
                    0.1..=10.0,
                    0.1,
                );
            }
            Some(IndicatorType::IchimokuCloud) => {
                Self::draw_int_param(ui, "Tenkan", &mut p.ichimoku_tenkan, 1..=500);
                Self::draw_int_param(ui, "Kijun", &mut p.ichimoku_kijun, 1..=500);
                Self::draw_int_param(ui, "Senkou", &mut p.ichimoku_senkou, 1..=500);
            }
            None | Some(IndicatorType::OBV | IndicatorType::VWAP | IndicatorType::ParabolicSAR) => {
                ui.label(
                    RichText::new("This indicator has no settings.")
                        .size(theme.text.small)
                        .color(config.muted_color),
                );
            }
        }
    }

    fn draw_int_param(
        ui: &mut Ui,
        label: &str,
        value: &mut usize,
        range: std::ops::RangeInclusive<usize>,
    ) {
        ui.label(label);
        ui.add(egui::DragValue::new(value).range(range));
        ui.end_row();
    }

    fn draw_float_param(
        ui: &mut Ui,
        label: &str,
        value: &mut f64,
        range: std::ops::RangeInclusive<f64>,
        speed: f64,
    ) {
        ui.label(label);
        ui.add(egui::DragValue::new(value).range(range).speed(speed));
        ui.end_row();
    }

    /// Create an indicator instance from current params
    pub fn create_indicator(&self, indicator_type: IndicatorType) -> Box<dyn Indicator> {
        match indicator_type {
            IndicatorType::SMA => Box::new(SMA::new(self.params.period)),
            IndicatorType::EMA => Box::new(EMA::new(self.params.period)),
            IndicatorType::WMA => Box::new(WMA::new(self.params.period)),
            IndicatorType::BollingerBands => Box::new(BollingerBands::new(
                self.params.period,
                self.params.bb_std_dev,
            )),
            IndicatorType::RSI => Box::new(RSI::new(self.params.period)),
            IndicatorType::MACD => Box::new(MACD::new(
                self.params.macd_fast,
                self.params.macd_slow,
                self.params.macd_signal,
            )),
            IndicatorType::Stochastic => {
                Box::new(Stochastic::new(self.params.stoch_k, 3, self.params.stoch_d))
            }
            IndicatorType::CCI => Box::new(CCI::new(self.params.period)),
            IndicatorType::ATR => Box::new(ATR::new(self.params.period)),
            IndicatorType::ADX => Box::new(ADX::new(self.params.period)),
            IndicatorType::OBV => Box::new(OnBalanceVolume::new()),
            IndicatorType::MFI => Box::new(MoneyFlowIndex::new(self.params.period)),
            IndicatorType::VWAP => Box::new(VolumeWeightedAvgPrice::new()),
            IndicatorType::IchimokuCloud => Box::new(IchimokuCloud::new(
                self.params.ichimoku_tenkan,
                self.params.ichimoku_kijun,
                self.params.ichimoku_senkou,
            )),
            IndicatorType::SuperTrend => Box::new(SuperTrend::new(
                self.params.period,
                self.params.supertrend_multiplier,
            )),
            IndicatorType::ParabolicSAR => Box::new(ParabolicSAR::default_params()),
            IndicatorType::KeltnerChannels => {
                Box::new(KeltnerChannels::new(self.params.period, 10, 2.0))
            }
            IndicatorType::DonchianChannels => Box::new(DonchianChannels::new(self.params.period)),
            IndicatorType::WilliamsR => Box::new(WilliamsR::new(self.params.period)),
            IndicatorType::ROC => Box::new(RateOfChange::new(self.params.period)),
            IndicatorType::Momentum => Box::new(Momentum::new(self.params.period)),
            IndicatorType::Aroon => Box::new(Aroon::new(self.params.period)),
            IndicatorType::ChaikinMoneyFlow => Box::new(ChaikinMoneyFlow::new(self.params.period)),
            IndicatorType::VWMA => Box::new(VWMA::new(self.params.period)),
        }
    }

    /// Get selected indicator if any
    pub fn selected(&self) -> Option<&IndicatorInfo> {
        self.sel_indicator.as_ref()
    }

    /// Get current params
    pub fn params(&self) -> &IndicatorParams {
        &self.params
    }

    /// Get mutable params
    pub fn params_mut(&mut self) -> &mut IndicatorParams {
        &mut self.params
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_tab_and_category_lists_only_its_own_indicators() {
        use IndicatorCategory::*;
        use IndicatorTab::*;
        // (tab, category, whether every built-in is listed); the other rows list none.
        let cases = [
            (Indicators, BuiltIns, true),
            (Indicators, Community, false),
            (Profiles, BuiltIns, false),
            (Patterns, BuiltIns, false),
        ];
        for (tab, category, all) in cases {
            let mut dialog = IndicatorDialog::new();
            dialog.active_tab = tab;
            dialog.sel_category = category;
            let listed = dialog
                .indicators
                .iter()
                .filter(|i| dialog.matches(i))
                .count();
            let expected = if all { dialog.indicators.len() } else { 0 };
            assert_eq!(listed, expected, "{tab:?} / {category:?}");
        }
    }

    #[test]
    fn dialog_follows_the_window_when_it_shrinks() {
        let ctx = egui::Context::default();
        let theme = Theme::dark();
        let mut dialog = IndicatorDialog::new();
        dialog.open();
        let mut frame = |height: f32| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, height))),
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                dialog.show(ui.ctx(), &theme);
            })
            .drop_without_applying_deltas();
            ctx.memory(|m| m.area_rect(egui::Id::unique("indicator_dialog")))
                .expect("the dialog is shown")
        };
        for _ in 0..3 {
            frame(900.0);
        }
        let mut shown = frame(500.0);
        for _ in 0..3 {
            shown = frame(500.0);
        }
        assert!(
            shown.top() >= 0.0 && shown.bottom() <= 500.0,
            "the dialog spans {shown:?} in a 500 px window"
        );
    }

    #[test]
    fn momentum_entry_creates_the_momentum_study() {
        let indicator = IndicatorDialog::new().create_indicator(IndicatorType::Momentum);
        assert_eq!(indicator.name(), "Momentum");
    }
}
