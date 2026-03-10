//! Main indicator dialog implementation

use super::{
    actions::IndicatorDialogAction,
    config::IndicatorDialogConfig,
    data::{IndicatorInfo, IndicatorParams},
    types::{IndicatorCategory, IndicatorTab, IndicatorType},
};
use crate::ext::UiExt;
use crate::studies::*;
use crate::styles::typography;
use crate::theme::Theme;
use crate::tokens::DESIGN_TOKENS;
use egui::{Color32, Pos2, Rect, Response, RichText, Sense, Ui, Vec2, Window};
use std::collections::HashSet;

/// Indicator Dialog
///
/// Modal dialog for searching, selecting, and configuring indicators.
pub struct IndicatorDialog {
    /// Is dialog open
    pub is_open: bool,
    /// Search query
    search_query: String,
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
            search_query: String::new(),
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

    /// Open the dialog
    pub fn open(&mut self) {
        self.is_open = true;
        self.search_query.clear();
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
        let mut action = IndicatorDialogAction::None;

        if !self.is_open {
            return action;
        }

        // Update config from theme each frame for live color updates
        self.config = IndicatorDialogConfig::from_theme(theme);

        let mut is_open = self.is_open;

        // fixed dialog size
        let (dialog_width, dialog_height) = (self.config.width, self.config.height);

        // Show modal overlay to block interactions behind the dialog
        egui::Area::new(egui::Id::new("indicator_dialog_overlay"))
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                // Use content_rect for overlay (respects safe areas on iOS)
                let screen = ui.ctx().content_rect();
                ui.painter()
                    .rect_filled(screen, 0.0, DESIGN_TOKENS.semantic.modal.overlay_bg);
            });

        // Corner radius - desktop style
        let corner_radius = DESIGN_TOKENS.rounding.lg;

        Window::new("Indicators, metrics, and strategies")
            .open(&mut is_open)
            .resizable(false)
            .collapsible(false)
            .order(egui::Order::Foreground)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .fixed_size(Vec2::new(dialog_width, dialog_height))
            .frame(
                egui::Frame::window(&ctx.style())
                    .fill(self.config.bg_color)
                    .corner_radius(corner_radius as u8),
            )
            .show(ctx, |ui| {
                action = self.draw_content(ui);
            });

        self.is_open = is_open;
        if !is_open {
            action = IndicatorDialogAction::Close;
        }

        action
    }
    fn draw_content(&mut self, ui: &mut Ui) -> IndicatorDialogAction {
        let mut action = IndicatorDialogAction::None;

        // Search bar
        ui.horizontal(|ui| {
            ui.space_lg();
            ui.label("Search:");
            let _res = ui.add(
                egui::TextEdit::singleline(&mut self.search_query)
                    .hint_text("Search indicators...")
                    .desired_width(self.config.width - 50.0),
            );
        });

        ui.space_lg();

        // Tabs
        ui.horizontal(|ui| {
            for tab in IndicatorTab::all() {
                let is_active = self.active_tab == *tab;
                let response = ui.selectable_label(is_active, tab.name());
                if response.clicked() {
                    self.active_tab = *tab;
                }
            }
        });

        ui.space_sm();
        ui.separator();

        // Main content
        ui.horizontal(|ui| {
            // Left sidebar - categories
            ui.vertical(|ui| {
                ui.set_width(140.0);
                ui.space_lg();

                for category in IndicatorCategory::all() {
                    let is_active = self.sel_category == *category;
                    let response = self.draw_category_item(ui, *category, is_active);
                    if response.clicked() {
                        self.sel_category = *category;
                    }
                }
            });

            ui.separator();

            // Middle - indicator list
            ui.vertical(|ui| {
                let list_width = if self.show_config {
                    self.config.width - 420.0 // More space for config panel
                } else {
                    self.config.width - 160.0
                };
                ui.set_width(list_width);
                action = self.draw_indicator_list(ui);
            });

            // Right - configuration panel (if showing)
            if self.show_config {
                ui.separator();
                ui.vertical(|ui| {
                    ui.set_width(250.0); // Wider config panel
                    if let Some(a) = self.draw_config_panel(ui) {
                        action = a;
                    }
                });
            }
        });

        action
    }

    fn draw_category_item(
        &self,
        ui: &mut Ui,
        category: IndicatorCategory,
        is_active: bool,
    ) -> Response {
        let desired_size = Vec2::new(
            DESIGN_TOKENS.sizing.dialog.indicator_item_width,
            DESIGN_TOKENS.sizing.button_md,
        );
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

        let bg_color = if is_active {
            self.config.sel_color
        } else if response.hovered() {
            self.config.hover_color
        } else {
            Color32::TRANSPARENT
        };

        ui.painter()
            .rect_filled(rect, DESIGN_TOKENS.rounding.md, bg_color);

        // Icon (render SVG)
        let icon = category.icon();
        let icon_rect = egui::Rect::from_center_size(
            Pos2::new(
                rect.min.x + DESIGN_TOKENS.spacing.xl + DESIGN_TOKENS.spacing.md,
                rect.center().y,
            ),
            egui::Vec2::splat(DESIGN_TOKENS.sizing.icon_14),
        );
        icon.as_image(egui::Vec2::splat(DESIGN_TOKENS.sizing.icon_14))
            .paint_at(ui, icon_rect);

        // Name
        ui.painter().text(
            Pos2::new(rect.min.x + 32.0, rect.center().y),
            egui::Align2::LEFT_CENTER,
            category.name(),
            egui::FontId::proportional(typography::MD),
            if is_active {
                DESIGN_TOKENS.semantic.ui.text_light
            } else {
                self.config.text_color
            },
        );

        response
    }

    fn draw_indicator_list(&mut self, ui: &mut Ui) -> IndicatorDialogAction {
        let mut action = IndicatorDialogAction::None;

        // Pre-filter indicators and clone to avoid borrow conflicts
        let search_query = self.search_query.clone();
        let sel_category = self.sel_category;
        let favorites = self.favorites.clone();

        let filtered: Vec<IndicatorInfo> = self
            .indicators
            .iter()
            .filter(|ind| {
                // Filter by search
                if !search_query.is_empty() {
                    let query = search_query.to_lowercase();
                    if !ind.name.to_lowercase().contains(&query)
                        && !ind.desc.to_lowercase().contains(&query)
                        && !ind.id.to_lowercase().contains(&query)
                    {
                        return false;
                    }
                }

                // Filter by category
                match sel_category {
                    IndicatorCategory::Favorites => favorites.contains(&ind.id),
                    IndicatorCategory::Premium => ind.is_premium,
                    IndicatorCategory::BuiltIns => !ind.is_premium,
                    _ => true,
                }
            })
            .cloned()
            .collect();

        let is_empty = filtered.is_empty();

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.space_lg();

            for indicator in &filtered {
                if let Some(a) = self.draw_indicator_item(ui, indicator) {
                    action = a;
                }
            }

            if is_empty {
                ui.vertical_centered(|ui| {
                    ui.add_space(DESIGN_TOKENS.spacing.section_lg);
                    ui.label(RichText::new("No indicators found").color(self.config.muted_color));
                });
            }
        });

        action
    }

    fn draw_indicator_item(
        &mut self,
        ui: &mut Ui,
        indicator: &IndicatorInfo,
    ) -> Option<IndicatorDialogAction> {
        let desired_size = Vec2::new(
            ui.available_width() - DESIGN_TOKENS.spacing.xxl,
            DESIGN_TOKENS.sizing.button_lg,
        );
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

        let is_favorite = self.favorites.contains(&indicator.id);
        let is_sel = self
            .sel_indicator
            .as_ref()
            .is_some_and(|s| s.id == indicator.id);

        self.draw_item_background(ui, rect, &response, is_sel);
        let star_res = self.draw_favorite_star(ui, rect, is_favorite);
        self.draw_item_text(ui, rect, indicator);
        Self::draw_item_badges(ui, rect, indicator);

        self.handle_item_clicks(indicator, is_favorite, &response, &star_res, rect)
    }

    fn draw_item_background(&self, ui: &mut Ui, rect: Rect, response: &Response, is_sel: bool) {
        if response.hovered() || is_sel {
            let bg = if is_sel {
                self.config.sel_color.gamma_multiply(0.5)
            } else {
                self.config.hover_color
            };
            ui.painter()
                .rect_filled(rect, DESIGN_TOKENS.rounding.md, bg);
        }
    }

    fn draw_favorite_star(&self, ui: &mut Ui, rect: Rect, is_favorite: bool) -> Response {
        let star_rect = Rect::from_min_size(
            Pos2::new(rect.min.x + DESIGN_TOKENS.spacing.sm, rect.min.y),
            Vec2::new(DESIGN_TOKENS.sizing.button_sm, rect.height()),
        );
        let star_res = ui.allocate_rect(star_rect, Sense::click());

        let star_color = if is_favorite {
            self.config.favorite_color
        } else if star_res.hovered() {
            ui.visuals().text_color()
        } else {
            ui.visuals().weak_text_color()
        };

        ui.painter().text(
            star_rect.center(),
            egui::Align2::CENTER_CENTER,
            if is_favorite { "*" } else { "o" },
            egui::FontId::proportional(typography::MD),
            star_color,
        );
        star_res
    }

    fn draw_item_text(&self, ui: &mut Ui, rect: Rect, indicator: &IndicatorInfo) {
        ui.painter().text(
            Pos2::new(
                rect.min.x + DESIGN_TOKENS.sizing.button_md,
                rect.min.y + DESIGN_TOKENS.spacing.xl,
            ),
            egui::Align2::LEFT_CENTER,
            &indicator.name,
            egui::FontId::proportional(typography::MD),
            self.config.text_color,
        );
        ui.painter().text(
            Pos2::new(
                rect.min.x + DESIGN_TOKENS.sizing.button_md,
                rect.min.y + DESIGN_TOKENS.sizing.button_sm,
            ),
            egui::Align2::LEFT_CENTER,
            &indicator.desc,
            egui::FontId::proportional(typography::XS),
            self.config.muted_color,
        );
    }

    fn draw_item_badges(ui: &mut Ui, rect: Rect, indicator: &IndicatorInfo) {
        let mut badge_x = rect.right() - DESIGN_TOKENS.spacing.lg;
        if indicator.is_premium {
            badge_x -= DESIGN_TOKENS.sizing.button_lg;
            ui.painter().text(
                Pos2::new(badge_x, rect.center().y),
                egui::Align2::LEFT_CENTER,
                "PREMIUM",
                egui::FontId::proportional(typography::XS),
                DESIGN_TOKENS.semantic.extended.favorite_gold,
            );
        }
        if indicator.is_overlay {
            badge_x -= DESIGN_TOKENS.sizing.button_lg;
            ui.painter().text(
                Pos2::new(badge_x, rect.center().y),
                egui::Align2::LEFT_CENTER,
                "OVERLAY",
                egui::FontId::proportional(typography::XS),
                DESIGN_TOKENS.semantic.extended.info_light,
            );
        }
    }

    fn handle_item_clicks(
        &mut self,
        indicator: &IndicatorInfo,
        is_favorite: bool,
        response: &Response,
        star_res: &Response,
        rect: Rect,
    ) -> Option<IndicatorDialogAction> {
        let star_rect = Rect::from_min_size(
            Pos2::new(rect.min.x + DESIGN_TOKENS.spacing.sm, rect.min.y),
            Vec2::new(DESIGN_TOKENS.sizing.button_sm, rect.height()),
        );

        if star_res.clicked() {
            if is_favorite {
                self.favorites.remove(&indicator.id);
            } else {
                self.favorites.insert(indicator.id.clone());
            }
            return Some(IndicatorDialogAction::ToggleFavorite(indicator.id.clone()));
        }

        if response.clicked() && !star_rect.contains(response.hover_pos().unwrap_or_default()) {
            self.sel_indicator = Some(indicator.clone());
            self.show_config = true;
            self.params = IndicatorParams::default();
        }
        None
    }

    fn draw_config_panel(&mut self, ui: &mut Ui) -> Option<IndicatorDialogAction> {
        let indicator = self.sel_indicator.as_ref()?;
        let indicator_name = indicator.name.clone();
        let indicator_type = indicator.indicator_type;

        self.draw_config_header(ui, &indicator_name);
        self.draw_indicator_params(ui, indicator_type);
        self.draw_config_buttons(ui, indicator_type)
    }

    fn draw_config_header(&self, ui: &mut Ui, name: &str) {
        ui.space_lg();
        ui.strong_label(name);
        ui.space_sm();
        ui.separator();
        ui.space_lg();
    }

    fn draw_indicator_params(&mut self, ui: &mut Ui, indicator_type: Option<IndicatorType>) {
        let muted = self.config.muted_color;
        let Some(ind_type) = indicator_type else {
            ui.label(RichText::new("No params").color(muted));
            return;
        };

        match ind_type {
            IndicatorType::SMA | IndicatorType::EMA | IndicatorType::WMA | IndicatorType::VWMA => {
                Self::draw_int_param(ui, "Period:", &mut self.params.period, 1..=500);
            }
            IndicatorType::BollingerBands => {
                Self::draw_int_param(ui, "Period:", &mut self.params.period, 1..=500);
                Self::draw_float_param(ui, "Std Dev:", &mut self.params.bb_std_dev, 0.1..=5.0, 0.1);
            }
            IndicatorType::RSI
            | IndicatorType::CCI
            | IndicatorType::ATR
            | IndicatorType::ADX
            | IndicatorType::MFI
            | IndicatorType::WilliamsR
            | IndicatorType::ROC
            | IndicatorType::Momentum
            | IndicatorType::ChaikinMoneyFlow => {
                Self::draw_int_param(ui, "Period:", &mut self.params.period, 2..=500);
            }
            IndicatorType::MACD => {
                Self::draw_int_param(ui, "Fast:", &mut self.params.macd_fast, 1..=500);
                Self::draw_int_param(ui, "Slow:", &mut self.params.macd_slow, 1..=500);
                Self::draw_int_param(ui, "Signal:", &mut self.params.macd_signal, 1..=500);
            }
            IndicatorType::Stochastic => {
                Self::draw_int_param(ui, "%K Period:", &mut self.params.stoch_k, 1..=500);
                Self::draw_int_param(ui, "%D Period:", &mut self.params.stoch_d, 1..=500);
            }
            IndicatorType::SuperTrend => {
                Self::draw_int_param(ui, "Period:", &mut self.params.period, 1..=500);
                Self::draw_float_param(
                    ui,
                    "Multiplier:",
                    &mut self.params.supertrend_multiplier,
                    0.1..=10.0,
                    0.1,
                );
            }
            IndicatorType::KeltnerChannels
            | IndicatorType::DonchianChannels
            | IndicatorType::Aroon => {
                Self::draw_int_param(ui, "Period:", &mut self.params.period, 1..=500);
            }
            IndicatorType::IchimokuCloud => {
                Self::draw_int_param(ui, "Tenkan:", &mut self.params.ichimoku_tenkan, 1..=500);
                Self::draw_int_param(ui, "Kijun:", &mut self.params.ichimoku_kijun, 1..=500);
                Self::draw_int_param(ui, "Senkou:", &mut self.params.ichimoku_senkou, 1..=500);
            }
            _ => {
                ui.label(RichText::new("No params").color(muted));
            }
        }
    }

    fn draw_int_param(
        ui: &mut Ui,
        label: &str,
        value: &mut usize,
        range: std::ops::RangeInclusive<usize>,
    ) {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.add(egui::DragValue::new(value).range(range));
        });
    }

    fn draw_float_param(
        ui: &mut Ui,
        label: &str,
        value: &mut f64,
        range: std::ops::RangeInclusive<f64>,
        speed: f64,
    ) {
        ui.horizontal(|ui| {
            ui.label(label);
            ui.add(egui::DragValue::new(value).range(range).speed(speed));
        });
    }

    fn draw_config_buttons(
        &mut self,
        ui: &mut Ui,
        indicator_type: Option<IndicatorType>,
    ) -> Option<IndicatorDialogAction> {
        ui.space_xxl();
        ui.separator();
        ui.space_lg();

        let mut result = None;
        ui.horizontal(|ui| {
            if ui.button("Add").clicked() {
                // Return configured indicator with type and params
                if let Some(ind_type) = indicator_type {
                    result = Some(IndicatorDialogAction::AddConfiguredIndicator(
                        super::data::ConfiguredIndicator {
                            indicator_type: ind_type,
                            params: self.params.clone(),
                        },
                    ));
                }
            }
            if ui.button("Cancel").clicked() {
                result = Some(IndicatorDialogAction::None);
            }
        });

        if result.is_some() {
            self.is_open = false;
            self.sel_indicator = None;
            self.show_config = false;
        }
        result
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
            IndicatorType::Momentum => Box::new(RateOfChange::new(self.params.period)), // Momentum is similar to ROC
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
