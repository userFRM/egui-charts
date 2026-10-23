//! Main indicator dialog implementation

use super::{
    actions::IndicatorDialogAction,
    config::IndicatorDialogConfig,
    data::{IndicatorInfo, IndicatorParams, IndicatorPreferences},
    types::{IndicatorCategory, IndicatorTab},
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
const SIDEBAR_WIDTH: f32 = 228.0;

/// Indicator Dialog
///
/// Modal dialog for searching, selecting, and configuring indicators. It lists every study
/// its [`IndicatorFactory`] registers and creates the one the user adds through it.
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
    /// Creates the studies the dialog lists
    factory: IndicatorFactory,
    /// Available indicators, one per factory name
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
    /// Settings to save in the host's user settings document. The dialog owns no storage.
    pub fn preferences(&self) -> IndicatorPreferences {
        IndicatorPreferences {
            favorites: self.favorites.iter().cloned().collect(),
        }
    }

    /// Restore the user's saved browser settings, including unavailable host studies.
    pub fn set_preferences(&mut self, preferences: IndicatorPreferences) {
        self.favorites = preferences.favorites.into_iter().collect();
    }

    /// Create a new indicator dialog with the built-in studies
    pub fn new() -> Self {
        let factory = IndicatorFactory::new();
        Self {
            is_open: false,
            search: FilterWidget::new().placeholder("Search indicators"),
            focus_search: false,
            focus_first_row: false,
            active_tab: IndicatorTab::Indicators,
            sel_category: IndicatorCategory::BuiltIns,
            sel_indicator: None,
            favorites: HashSet::new(),
            indicators: IndicatorInfo::from_factory(&factory),
            factory,
            config: IndicatorDialogConfig::default(),
            params: IndicatorParams::default(),
            show_config: false,
        }
    }

    /// List and create the studies of `factory`, such as the built-ins and the host's own
    /// added with [`IndicatorFactory::register`]
    pub fn with_factory(mut self, factory: IndicatorFactory) -> Self {
        self.indicators = IndicatorInfo::from_factory(&factory);
        self.factory = factory;
        self
    }

    /// Set a custom dialog configuration; its `TRANSPARENT` colours follow the theme
    pub fn with_config(mut self, config: IndicatorDialogConfig) -> Self {
        self.config = config;
        self
    }

    /// Open the dialog on the Indicators tab with the search field focused
    pub fn open(&mut self) {
        self.open_tab(IndicatorTab::Indicators);
    }

    /// Open the dialog on `tab` with the search field focused
    pub fn open_tab(&mut self, tab: IndicatorTab) {
        self.active_tab = tab;
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

        let config = self.config.resolved(theme);

        // Escape clears a search before it closes the dialog.
        if !self.search.is_empty() && ctx.input_mut(|i| i.consume_key(Modifiers::NONE, Key::Escape))
        {
            self.search.clear();
        }

        let mut is_open = true;
        let action = Modal::new("indicator_dialog", "Indicators")
            .width(config.width)
            .height(config.height)
            .rounding(16.0)
            .dim_background(false)
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
        let search = ui
            .scope(|ui| {
                ui.spacing_mut().interact_size.y = 40.0;
                ui.style_mut().override_font_id = Some(FontId::proportional(16.0));
                self.search.show(ui).response
            })
            .inner;
        if std::mem::take(&mut self.focus_search) {
            search.request_focus();
        }
        self.focus_first_row = search.has_focus() && ui.input(|i| i.key_pressed(Key::ArrowDown));
        ui.add_space(theme.spacing.lg);

        // Profiles and patterns have no categories.
        if self.active_tab != IndicatorTab::Indicators {
            TabBar::new(IndicatorTab::all(), &mut self.active_tab)
                .pills()
                .show(ui);
            self.draw_empty_state(ui);
            return IndicatorDialogAction::None;
        }

        let mut action = IndicatorDialogAction::None;
        // The sidebar runs before the list, so the list shows a category clicked this frame.
        let category = Cell::new(self.sel_category);
        SidebarLayout::new(SIDEBAR_WIDTH).show(
            ui,
            |ui| {
                ui.visuals_mut().selection.bg_fill = theme.ui.btn_bg_hover;
                ui.visuals_mut().selection.stroke.color = theme.ui.text;
                for item in [IndicatorCategory::Favorites, IndicatorCategory::BuiltIns] {
                    ui.add_space(16.0);
                    ui.label(
                        RichText::new(if item == IndicatorCategory::Favorites {
                            "PERSONAL"
                        } else {
                            "BUILT-IN"
                        })
                        .size(11.0)
                        .color(theme.ui.text_secondary),
                    );
                    ui.add_space(8.0);
                    if SidebarTab::new(item.name(), category.get() == item)
                        .icon(item.icon())
                        .show(ui)
                    {
                        category.set(item);
                    }
                }
            },
            |ui| {
                self.sel_category = category.get();
                TabBar::new(IndicatorTab::all(), &mut self.active_tab)
                    .pills()
                    .show(ui);
                ui.add_space(12.0);
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
        let searched = [&ind.name, &ind.desc, &ind.id, &ind.category]
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
                ui.spacing_mut().item_spacing.y = 0.0;
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
        let desired_size = Vec2::new(ui.available_width(), 32.0);
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
        Self::draw_item_text(ui, rect, text_right, indicator, text);
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

    /// The name centred on the row and cut off before `right`.
    fn draw_item_text(ui: &Ui, rect: Rect, right: f32, indicator: &IndicatorInfo, text: Color32) {
        let x = rect.min.x + DESIGN_TOKENS.sizing.button_md;
        let mut job =
            LayoutJob::simple_singleline(indicator.name.clone(), FontId::proportional(14.0), text);
        job.wrap = TextWrapping::truncate_at_width(right - x);
        let name = ui.painter().layout_job(job);
        let top = rect.center().y - name.size().y / 2.0;
        ui.painter().galley(Pos2::new(x, top), name, text);
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
                self.select(indicator);
            }
        }
        None
    }

    /// Select `indicator` and open its configuration panel with the study's own parameters.
    fn select(&mut self, indicator: &IndicatorInfo) {
        self.params = Self::defaults(self.edited(&indicator.id));
        self.sel_indicator = Some(indicator.clone());
        self.show_config = true;
    }

    /// Input caption for a study just created by this dialog; unknown host schemas stay empty.
    pub fn input_summary(&self, name: &str) -> String {
        let p = &self.params;
        match self.edited(name) {
            "SMA" | "EMA" | "WMA" | "VWMA" | "Donchian" | "Aroon" | "RSI" | "CCI" | "ATR"
            | "ADX" | "MFI" | "Williams %R" | "ROC" | "Momentum" | "CMF" => p.period.to_string(),
            "BB" => format!("{} {}", p.period, p.bb_std_dev),
            "Keltner" => format!("{} {} {}", p.period, p.keltner_atr, p.keltner_multiplier),
            "MACD" => format!("{} {} {}", p.macd_fast, p.macd_slow, p.macd_signal),
            "Stochastic" => format!("{} {} {}", p.stoch_k, p.stoch_smooth, p.stoch_d),
            "SuperTrend" => format!("{} {}", p.period, p.supertrend_multiplier),
            "Ichimoku" => format!(
                "{} {} {}",
                p.ichimoku_tenkan, p.ichimoku_kijun, p.ichimoku_senkou
            ),
            _ => String::new(),
        }
    }

    /// Add the selected indicator with its parameters and close the dialog.
    fn add_selected(&mut self) -> Option<IndicatorDialogAction> {
        let study = self.create(&self.sel_indicator.as_ref()?.id)?;
        self.close();
        Some(IndicatorDialogAction::AddIndicator(study))
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
        let name = indicator.id.clone();
        ui.add_space(theme.spacing.lg);

        FormGrid::new("indicator_params").show(ui, |ui| {
            self.draw_indicator_params(ui, config, theme, &name);
        });

        let (add, back) = DialogFooter::new("Add").secondary("Back").show(ui);
        if back {
            self.sel_indicator = None;
            self.show_config = false;
        }
        if add {
            return self.add_selected();
        }
        None
    }

    /// The parameters the dialog edits for the study registered as `name`.
    fn draw_indicator_params(
        &mut self,
        ui: &mut Ui,
        config: &IndicatorDialogConfig,
        theme: &Theme,
        name: &str,
    ) {
        let edited = self.edited(name);
        let p = &mut self.params;
        match edited {
            "SMA" | "EMA" | "WMA" | "VWMA" | "Donchian" | "Aroon" => {
                Self::draw_int_param(ui, "Period", &mut p.period, 1..=500);
            }
            "Keltner" => {
                Self::draw_int_param(ui, "Period", &mut p.period, 1..=500);
                Self::draw_int_param(ui, "ATR Period", &mut p.keltner_atr, 1..=500);
                Self::draw_float_param(
                    ui,
                    "Multiplier",
                    &mut p.keltner_multiplier,
                    0.1..=10.0,
                    0.1,
                );
            }
            "BB" => {
                Self::draw_int_param(ui, "Period", &mut p.period, 1..=500);
                Self::draw_float_param(ui, "Std Dev", &mut p.bb_std_dev, 0.1..=5.0, 0.1);
            }
            "RSI" | "CCI" | "ATR" | "ADX" | "MFI" | "Williams %R" | "ROC" | "Momentum" | "CMF" => {
                Self::draw_int_param(ui, "Period", &mut p.period, 2..=500);
            }
            "MACD" => {
                Self::draw_int_param(ui, "Fast", &mut p.macd_fast, 1..=500);
                Self::draw_int_param(ui, "Slow", &mut p.macd_slow, 1..=500);
                Self::draw_int_param(ui, "Signal", &mut p.macd_signal, 1..=500);
            }
            "Stochastic" => {
                Self::draw_int_param(ui, "%K Period", &mut p.stoch_k, 1..=500);
                Self::draw_int_param(ui, "%K Smoothing", &mut p.stoch_smooth, 1..=500);
                Self::draw_int_param(ui, "%D Period", &mut p.stoch_d, 1..=500);
            }
            "SuperTrend" => {
                Self::draw_int_param(ui, "Period", &mut p.period, 1..=500);
                Self::draw_float_param(
                    ui,
                    "Multiplier",
                    &mut p.supertrend_multiplier,
                    0.1..=10.0,
                    0.1,
                );
            }
            "Ichimoku" => {
                Self::draw_int_param(ui, "Tenkan", &mut p.ichimoku_tenkan, 1..=500);
                Self::draw_int_param(ui, "Kijun", &mut p.ichimoku_kijun, 1..=500);
                Self::draw_int_param(ui, "Senkou", &mut p.ichimoku_senkou, 1..=500);
            }
            _ => {
                ui.label(
                    RichText::new("Added with its default settings.")
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

    /// The parameters of the study registered as `name` as the factory creates it; the
    /// fields the study does not read keep their `IndicatorParams::default()` values.
    fn defaults(name: &str) -> IndicatorParams {
        let period = match name {
            "SMA" | "WMA" | "VWMA" | "BB" | "CCI" | "Keltner" | "Donchian" | "CMF" => 20,
            "EMA" => 12,
            "Momentum" | "SuperTrend" => 10,
            _ => 14,
        };
        IndicatorParams {
            period,
            ..IndicatorParams::default()
        }
    }

    /// `name` if the dialog edits the parameters of the study registered under it, which is
    /// a built-in, else `""`: a host's study, even under a built-in's name, is created as
    /// the host registered it.
    fn edited<'a>(&self, name: &'a str) -> &'a str {
        if self.factory.category(name) == Some(IndicatorFactory::CUSTOM) {
            ""
        } else {
            name
        }
    }

    /// The study registered as `name`, with the parameters the dialog edits for it.
    fn create(&self, name: &str) -> Option<Box<dyn Indicator>> {
        let p = &self.params;
        Some(match self.edited(name) {
            "SMA" => Box::new(SMA::new(p.period)),
            "EMA" => Box::new(EMA::new(p.period)),
            "WMA" => Box::new(WMA::new(p.period)),
            "VWMA" => Box::new(VWMA::new(p.period)),
            "BB" => Box::new(BollingerBands::new(p.period, p.bb_std_dev)),
            "RSI" => Box::new(RSI::new(p.period)),
            "MACD" => Box::new(MACD::new(p.macd_fast, p.macd_slow, p.macd_signal)),
            "Stochastic" => Box::new(Stochastic::new(p.stoch_k, p.stoch_smooth, p.stoch_d)),
            "CCI" => Box::new(CCI::new(p.period)),
            "ATR" => Box::new(ATR::new(p.period)),
            "ADX" => Box::new(ADX::new(p.period)),
            "MFI" => Box::new(MoneyFlowIndex::new(p.period)),
            "Ichimoku" => Box::new(IchimokuCloud::new(
                p.ichimoku_tenkan,
                p.ichimoku_kijun,
                p.ichimoku_senkou,
            )),
            "SuperTrend" => Box::new(SuperTrend::new(p.period, p.supertrend_multiplier)),
            "Keltner" => Box::new(KeltnerChannels::new(
                p.period,
                p.keltner_atr,
                p.keltner_multiplier,
            )),
            "Donchian" => Box::new(DonchianChannels::new(p.period)),
            "Williams %R" => Box::new(WilliamsR::new(p.period)),
            "ROC" => Box::new(RateOfChange::new(p.period)),
            "Momentum" => Box::new(Momentum::new(p.period)),
            "Aroon" => Box::new(Aroon::new(p.period)),
            "CMF" => Box::new(ChaikinMoneyFlow::new(p.period)),
            _ => return self.factory.create(name),
        })
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
    fn saved_favorites_restore_the_favorites_list() {
        let mut dialog = IndicatorDialog::new();
        dialog
            .favorites
            .extend(["BB".to_owned(), "Host study".to_owned()]);
        let saved = serde_json::to_string(&dialog.preferences()).unwrap();
        let mut restored = IndicatorDialog::new();
        restored.set_preferences(serde_json::from_str(&saved).unwrap());
        restored.sel_category = IndicatorCategory::Favorites;
        let listed: Vec<_> = restored
            .indicators
            .iter()
            .filter(|i| restored.matches(i))
            .collect();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "BB");
        assert!(restored.favorites.contains("Host study"));
        assert_eq!(
            saved,
            serde_json::to_string(&restored.preferences()).unwrap()
        );
    }

    #[test]
    fn each_tab_and_category_lists_only_its_own_indicators() {
        use IndicatorCategory::*;
        use IndicatorTab::*;
        // (tab, category, whether every study of the factory is listed); the other rows list none.
        let cases = [
            (Indicators, BuiltIns, true),
            (Indicators, Community, false),
            (Profiles, BuiltIns, false),
            (Patterns, BuiltIns, false),
        ];
        let factory = || {
            let mut factory = IndicatorFactory::new();
            factory.register("Host SMA", || Box::new(SMA::new(42)));
            factory
        };
        for (tab, category, all) in cases {
            let mut dialog = IndicatorDialog::new().with_factory(factory());
            dialog.active_tab = tab;
            dialog.sel_category = category;
            let mut listed: Vec<String> = dialog
                .indicators
                .iter()
                .filter(|i| dialog.matches(i))
                .map(|i| i.id.clone())
                .collect();
            listed.sort();
            let mut expected = if all { factory().list() } else { Vec::new() };
            expected.sort();
            assert_eq!(listed, expected, "{tab:?} / {category:?}");
        }

        // A study the host registers, under a new name or a built-in's, is added through the
        // factory.
        for name in ["Host SMA", "RSI"] {
            let mut factory = factory();
            factory.register(name, || Box::new(SMA::new(42)));
            let mut dialog = IndicatorDialog::new().with_factory(factory);
            let host = dialog.indicators.iter().find(|i| i.id == name).cloned();
            dialog.select(&host.expect("the host's study is listed"));
            let Some(IndicatorDialogAction::AddIndicator(study)) = dialog.add_selected() else {
                panic!("the host's {name} is added");
            };
            assert_eq!(study.name(), "SMA", "{name}");
        }
    }

    /// Each study is added with the parameters the factory creates it with until the user
    /// changes them.
    #[test]
    fn each_study_is_added_with_its_own_defaults() {
        use crate::model::Bar;
        use chrono::{TimeZone, Utc};
        let bars: Vec<Bar> = (0..300)
            .map(|i: i64| {
                let x = i as f64;
                let close = 100.0 + (x * 0.13).sin() * 8.0 + (x * 0.029).cos() * 5.0;
                let open = close - (x * 0.7).sin() * 1.5;
                Bar {
                    time: Utc.timestamp_opt(i * 3_600, 0).unwrap(),
                    open,
                    high: close.max(open) + 0.5 + (x * 0.3).sin().abs(),
                    low: close.min(open) - 0.5 - (x * 0.5).cos().abs(),
                    close,
                    volume: 1_000.0 + ((i * 37) % 211) as f64 * 9.0,
                }
            })
            .collect();
        let factory = IndicatorFactory::new();
        let mut dialog = IndicatorDialog::new();
        let mut differ = Vec::new();
        for info in dialog.indicators.clone() {
            dialog.select(&info);
            let Some(IndicatorDialogAction::AddIndicator(mut added)) = dialog.add_selected() else {
                panic!("{} is added", info.id);
            };
            let mut default = factory.create(&info.id).unwrap();
            added.calculate(&bars);
            default.calculate(&bars);
            if format!("{:?}", added.values()) != format!("{:?}", default.values()) {
                differ.push(info.id);
            }
        }
        assert!(differ.is_empty(), "added with other parameters: {differ:?}");
    }

    #[test]
    fn dialog_follows_the_window_when_it_shrinks() {
        let ctx = egui::Context::default();
        let theme = Theme::dark();
        let mut dialog = IndicatorDialog::new();
        dialog.open();
        let mut frame = |size: Vec2| {
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, size)),
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
            frame(Vec2::new(1440.0, 900.0));
        }
        let mut shown = frame(Vec2::new(600.0, 500.0));
        for _ in 0..3 {
            shown = frame(Vec2::new(600.0, 500.0));
        }
        assert!(
            Rect::from_min_size(Pos2::ZERO, Vec2::new(600.0, 500.0)).contains_rect(shown),
            "the dialog spans {shown:?} in a 500 px window"
        );
    }

    #[test]
    fn show_keeps_the_configuration_a_caller_sets() {
        let custom = IndicatorDialogConfig {
            width: 900.0,
            text_color: Color32::RED,
            ..IndicatorDialogConfig::default()
        };
        let hover = custom.hover_color;
        let mut dialog = IndicatorDialog::new().with_config(custom);
        dialog.open();
        egui::Context::default()
            .run_ui(egui::RawInput::default(), |ui| {
                dialog.show(ui.ctx(), &Theme::light());
            })
            .drop_without_applying_deltas();
        assert_eq!(dialog.config.width, 900.0);
        assert_eq!(dialog.config.text_color, Color32::RED);
        assert_eq!(dialog.config.hover_color, hover);
    }
}
