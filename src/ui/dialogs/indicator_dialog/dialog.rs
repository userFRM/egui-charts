//! Study browser dialog backed by the configured factory.
//!
//! Use [`IndicatorDialog`] for search, category and favorite filtering and add-study
//! actions. Host-provided metric series can be offered beside the factory catalogue.

use super::{
    actions::IndicatorDialogAction,
    config::IndicatorDialogConfig,
    data::{IndicatorInfo, IndicatorPreferences},
    types::{IndicatorCategory, IndicatorTab},
};
use crate::data::inbox::Inbox;
use crate::data::{
    Datafeed, FeedCapabilities, FeedError, MetricSeries, OptionChain, OptionChainQuery,
    OptionExposureHandle, OptionExposureKind, OptionExposureQuery,
};
use crate::icons::icons;
use crate::localization as l10n;
use crate::model::{FeedService, Instrument};
use crate::studies::*;
use crate::styles::focus::draw_focus_ring;
use crate::theme::Theme;
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::EmptyState;
use crate::ui_kit::filter::FilterWidget;
use crate::ui_kit::modal::Modal;
use crate::ui_kit::sidebar_layout::{SidebarLayout, SidebarTab};
use crate::ui_kit::tab_bar::TabBar;
use egui::text::{LayoutJob, TextWrapping};
use egui::{
    Align2, Color32, FontId, Key, Modifiers, Pos2, Rect, Response, RichText, Role, Sense, Stroke,
    StrokeKind, Ui, Vec2, WidgetInfo,
};
use std::cell::Cell;
use std::collections::HashSet;
use std::sync::Arc;

/// Width of the category sidebar.
const SIDEBAR_WIDTH: f32 = 200.0;

/// Whether `theme` is the stock light preset, the only one with chooser-specific inks.
fn is_default_light(theme: &Theme) -> bool {
    !theme.dark && theme.ui == Theme::light().ui
}

/// Indicator Dialog
///
/// Modal dialog for searching and adding indicators. It lists studies from its
/// [`IndicatorFactory`], filtered by the supplied source and feed capabilities; a click or
/// Enter on a row adds that study with its default inputs and leaves the dialog open for the
/// next one. Edit the inputs of an added study with
/// [`IndicatorSettingsDialog`](super::IndicatorSettingsDialog).
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
    /// Favorite indicator IDs
    pub favorites: HashSet<String>,
    /// Creates the studies the dialog lists
    factory: IndicatorFactory,
    feed_capabilities: Option<(Option<FeedCapabilities>, Option<Instrument>)>,
    unavailable_order_flow: HashSet<String>,
    metric_source: Option<(Arc<dyn Datafeed>, String)>,
    metric_catalog: Option<Arc<Inbox>>,
    metric_names: Vec<String>,
    metric_error: Option<FeedError>,
    option_catalog: Option<Arc<Inbox>>,
    option_profile_registered: bool,
    option_error: Option<FeedError>,
    source: Option<StudySource>,
    source_error: Option<StudyError>,
    full: bool,
    /// Catalogue entries, one per factory name
    indicators: Vec<IndicatorInfo>,
    /// Configuration
    config: IndicatorDialogConfig,
}

impl Default for IndicatorDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl IndicatorDialog {
    /// Restrict studies in the factory's Order flow category to services explicitly declared
    /// for the resolved instrument. Pass [`Chart::feed_capabilities`](crate::widget::Chart::feed_capabilities)
    /// and [`Chart::instrument`](crate::widget::Chart::instrument) when they change. Missing
    /// metadata declares no services. The options profile is offered on Profiles only when the
    /// instrument declares the option-chain service. Other categories and saved favorites are
    /// unchanged.
    /// Until the first call every study is offered. Requirements are cached between updates.
    pub fn set_feed_capabilities(
        &mut self,
        capabilities: Option<&FeedCapabilities>,
        instrument: Option<&Instrument>,
    ) {
        if self
            .feed_capabilities
            .as_ref()
            .is_some_and(|(previous, symbol)| {
                previous.as_ref() == capabilities && symbol.as_ref() == instrument
            })
        {
            return;
        }
        self.feed_capabilities = Some((capabilities.cloned(), instrument.cloned()));
        self.refresh_order_flow_availability();
        self.clear_options();
    }

    fn refresh_order_flow_availability(&mut self) {
        self.unavailable_order_flow.clear();
        let Some((capabilities, instrument)) = &self.feed_capabilities else {
            return;
        };
        for ind in &self.indicators {
            if ind.category != IndicatorFactory::ORDER_FLOW {
                continue;
            }
            let available = self.factory.create(&ind.id).is_some_and(|study| {
                study.order_flow_inputs().iter().all(|input| {
                    input.services().iter().all(|service| {
                        capabilities.as_ref().zip(instrument.as_ref()).is_some_and(
                            |(capabilities, instrument)| {
                                capabilities.declares(*service, instrument)
                            },
                        )
                    })
                })
            });
            if !available {
                self.unavailable_order_flow.insert(ind.id.clone());
            }
        }
    }

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
            search: FilterWidget::new()
                .placeholder("Search indicators")
                .icon_size(DESIGN_TOKENS.sizing.icon_lg),
            focus_search: false,
            focus_first_row: false,
            active_tab: IndicatorTab::Indicators,
            sel_category: IndicatorCategory::BuiltIns,
            favorites: HashSet::new(),
            indicators: IndicatorInfo::from_factory(&factory),
            factory,
            feed_capabilities: None,
            unavailable_order_flow: HashSet::new(),
            metric_source: None,
            metric_catalog: None,
            metric_names: Vec::new(),
            metric_error: None,
            option_catalog: None,
            option_profile_registered: false,
            option_error: None,
            source: None,
            source_error: None,
            full: false,
            config: IndicatorDialogConfig::default(),
        }
    }

    /// List and create the studies of `factory`, such as the built-ins and the host's own
    /// added with [`IndicatorFactory::register`]
    pub fn with_factory(mut self, factory: IndicatorFactory) -> Self {
        self.option_catalog = None;
        self.option_profile_registered = false;
        self.indicators = IndicatorInfo::from_factory(&factory);
        self.factory = factory;
        self.refresh_order_flow_availability();
        self
    }

    /// List the metrics and, when declared by `set_feed_capabilities`, the option-chain
    /// profile a feed offers for this instrument alongside the technical studies.
    /// Replacing the feed or symbol discards pending replies from the previous source.
    pub fn set_metric_source(&mut self, feed: Option<Arc<dyn Datafeed>>, symbol: &str) {
        if match (&self.metric_source, &feed) {
            (Some((previous, key)), Some(feed)) => key == symbol && Arc::ptr_eq(previous, feed),
            (None, None) => true,
            _ => false,
        } {
            return;
        }
        self.clear_options();
        self.metric_source = feed.map(|feed| (feed, symbol.to_owned()));
        self.metric_catalog = None;
        self.metric_error = None;
        for name in self.metric_names.drain(..) {
            self.factory.unregister(&name);
        }
        self.indicators = IndicatorInfo::from_factory(&self.factory);
        self.refresh_order_flow_availability();
    }

    fn clear_options(&mut self) {
        self.option_catalog = None;
        self.option_error = None;
        if std::mem::take(&mut self.option_profile_registered) {
            self.factory.unregister("Options Volume Profile");
            self.indicators = IndicatorInfo::from_factory(&self.factory);
        }
    }

    fn poll_options(&mut self, ctx: &egui::Context) {
        let Some((feed, symbol)) = &self.metric_source else {
            return;
        };
        let Some((Some(capabilities), Some(instrument))) = &self.feed_capabilities else {
            return;
        };
        if symbol.is_empty()
            || instrument.symbol != *symbol
            || !capabilities.declares(FeedService::OptionChain, instrument)
        {
            return;
        }
        if self.option_catalog.is_none() {
            let inbox = Arc::new(Inbox::default());
            let ctx = ctx.clone();
            inbox.set_waker(move || ctx.request_repaint());
            feed.option_expiries(symbol, inbox.reply(0));
            self.option_catalog = Some(inbox);
        }
        for (_, answer) in self.option_catalog.as_ref().unwrap().drain().answers {
            let result = answer
                .downcast::<Result<Vec<chrono::NaiveDate>, FeedError>>()
                .map(|result| *result)
                .unwrap_or(Err(FeedError::Unanswered));
            match result {
                Ok(mut dates) => {
                    dates.sort_unstable();
                    dates.dedup();
                    let Some((&first, &last)) = dates.first().zip(dates.last()) else {
                        continue;
                    };
                    if self.factory.has("Options Volume Profile") {
                        continue;
                    }
                    let exposure = capabilities
                        .declares(FeedService::OptionExposure, instrument)
                        .then(|| {
                            let ctx = ctx.clone();
                            OptionExposureHandle::new(
                                &**feed,
                                OptionExposureQuery::new(
                                    symbol,
                                    first..=last,
                                    OptionExposureKind::Gamma,
                                ),
                                move || ctx.request_repaint(),
                            )
                            .expect("validated options exposure query")
                        });
                    let ctx = ctx.clone();
                    let chain = OptionChain::new(
                        &**feed,
                        OptionChainQuery::new(symbol, first..=last),
                        move || ctx.request_repaint(),
                    )
                    .expect("nonempty symbol and listed expiry range");
                    let symbol = symbol.clone();
                    self.factory.register_in(
                        IndicatorFactory::PROFILES,
                        "Options Volume Profile",
                        move || {
                            let mut profile = OptionsVolumeProfile::from_chain(
                                chain.clone(),
                                &symbol,
                                dates.clone(),
                            )
                            .expect("validated options profile inputs");
                            if let Some(exposure) = &exposure {
                                profile = profile
                                    .with_exposure(exposure.clone())
                                    .expect("matching gamma subscription");
                            }
                            Box::new(profile)
                        },
                    );
                    self.option_profile_registered = true;
                    self.indicators = IndicatorInfo::from_factory(&self.factory);
                }
                Err(FeedError::NotSupported) => {}
                Err(error) => self.option_error = Some(error),
            }
        }
    }

    fn poll_metrics(&mut self, ctx: &egui::Context) {
        let Some((feed, symbol)) = &self.metric_source else {
            return;
        };
        if self.metric_catalog.is_none() {
            let inbox = Arc::new(Inbox::default());
            let ctx = ctx.clone();
            inbox.set_waker(move || ctx.request_repaint());
            feed.metrics(symbol, inbox.reply(0));
            self.metric_catalog = Some(inbox);
        }
        for (_, answer) in self.metric_catalog.as_ref().unwrap().drain().answers {
            let result = answer
                .downcast::<Result<Vec<MetricSeries>, FeedError>>()
                .map(|result| *result)
                .unwrap_or(Err(FeedError::Unanswered));
            match result {
                Ok(metrics) => {
                    for metric in metrics {
                        if metric.id.is_empty() {
                            continue;
                        }
                        let key = format!("Metric: {}", metric.id);
                        if self.factory.has(&key) {
                            continue;
                        }
                        let mut info = IndicatorInfo::new(&key, &metric.name, &metric.unit);
                        info.category = "Metrics".into();
                        self.factory.register(key.clone(), move || {
                            Box::new(MetricStudy::new(metric.clone()))
                        });
                        self.metric_names.push(key);
                        self.indicators.push(info);
                    }
                    self.indicators.sort_by_key(|info| info.name.to_lowercase());
                }
                Err(FeedError::NotSupported) => {}
                Err(error) => self.metric_error = Some(error),
            }
        }
    }

    /// The catalogue sorted by display name, including the feed's metrics. Entries hidden by
    /// source or feed capability filters remain in this list.
    pub fn indicators(&self) -> &[IndicatorInfo] {
        &self.indicators
    }

    /// The factory that creates the listed studies, for a host that adds one by its id
    /// outside the dialog.
    pub fn factory(&self) -> &IndicatorFactory {
        &self.factory
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

    /// Open for `chart`. `source` preselects an input for an Add indicator action: the
    /// dialog then lists only the studies that accept a source, and each one added carries it,
    /// through [`ConfiguredStudy`] where needed. A study output `chart` does not have leaves
    /// the dialog closed.
    pub fn open_for_chart(
        &mut self,
        chart: &crate::widget::Chart,
        source: Option<StudySource>,
    ) -> bool {
        if let Some(StudySource::Study { id, plot }) = source
            && !chart
                .indicators()
                .any(|(study, indicator)| study.0 == id && plot < indicator.line_names().len())
        {
            self.close();
            return false;
        }
        self.open();
        self.source = source;
        true
    }

    /// Open the dialog on `tab` with the search field focused. Dedicated pages open
    /// among the built-ins; an unavailable page opens on the first populated page.
    pub fn open_tab(&mut self, tab: IndicatorTab) {
        if tab != IndicatorTab::Indicators {
            self.sel_category = IndicatorCategory::BuiltIns;
        }
        self.source = None;
        self.source_error = None;
        self.active_tab = tab;
        self.is_open = true;
        self.search.clear();
        self.focus_search = true;
    }

    /// Whether the chart takes no more studies. The dialog then says so and adds none; call it
    /// before each [`show`](Self::show), as the cap can be reached while the dialog is open.
    pub fn set_full(&mut self, full: bool) {
        self.full = full;
    }

    /// Close the dialog
    pub fn close(&mut self) {
        self.source = None;
        self.is_open = false;
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

        self.poll_metrics(ctx);
        self.poll_options(ctx);
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
        ui.add_space(theme.spacing.xxl);
        if let Some(error) = &self.metric_error {
            ui.label(l10n::format_text(
                ui.ctx(),
                "Metrics: {error}",
                &[("error", format!("{}", error))],
            ));
        }
        if let Some(error) = &self.option_error {
            ui.colored_label(
                theme.ui.error,
                l10n::format_text(
                    ui.ctx(),
                    "Options: {error}",
                    &[("error", error.to_string())],
                ),
            );
        }
        if let Some(error) = &self.source_error {
            ui.colored_label(theme.ui.error, l10n::text(ui.ctx(), error.to_string()));
        }
        if self.full {
            ui.colored_label(
                theme.ui.error,
                l10n::text(ui.ctx(), "The chart has reached its study limit"),
            );
        }

        let search = ui
            .scope(|ui| {
                ui.spacing_mut().interact_size.y = 40.0;
                if is_default_light(theme) {
                    ui.visuals_mut().weak_text_color = Some(Color32::from_gray(112));
                }
                ui.style_mut().override_font_id = Some(FontId::proportional(theme.text.heading));
                self.search.show(ui).response
            })
            .inner;
        if std::mem::take(&mut self.focus_search) {
            search.request_focus();
        }
        self.focus_first_row = search.has_focus() && ui.input(|i| i.key_pressed(Key::ArrowDown));
        ui.add_space(theme.spacing.lg);

        // Only destinations with studies are offered.
        let offered: Vec<_> = IndicatorCategory::all()
            .iter()
            .copied()
            .filter(|category| self.offers(*category))
            .collect();
        if !offered.contains(&self.sel_category) {
            self.sel_category = offered
                .iter()
                .copied()
                .find(|category| *category != IndicatorCategory::Favorites)
                .unwrap_or(IndicatorCategory::Favorites);
        }
        let mut action = IndicatorDialogAction::None;
        if ui.available_width() < 600.0 {
            egui::ComboBox::from_label(l10n::text(ui.ctx(), "Category"))
                .selected_text(l10n::text(ui.ctx(), self.sel_category.name()))
                .show_ui(ui, |ui| {
                    for &category in &offered {
                        ui.selectable_value(
                            &mut self.sel_category,
                            category,
                            l10n::text(ui.ctx(), category.name()),
                        );
                    }
                });
            self.draw_tabs(ui, theme);
            return self.draw_indicator_list(ui, config, theme);
        }
        // The sidebar runs before the list, so the list shows a category clicked this frame.
        let category = Cell::new(self.sel_category);
        let section_color = if is_default_light(theme) {
            Color32::from_gray(116)
        } else {
            theme.ui.text_muted
        };
        SidebarLayout::new(SIDEBAR_WIDTH).show(
            ui,
            |ui| {
                use IndicatorCategory as C;
                for (heading, items) in [
                    ("PERSONAL", &[C::Favorites, C::MyScripts][..]),
                    ("BUILT-IN", &[C::BuiltIns]),
                    ("COMMUNITY", &[C::Community]),
                ] {
                    let items: Vec<_> = items.iter().filter(|c| offered.contains(c)).collect();
                    if items.is_empty() {
                        continue;
                    }
                    ui.add_space(theme.spacing.xxl);
                    egui::Frame::new()
                        .inner_margin(egui::Margin {
                            left: theme.spacing.xl as i8,
                            ..Default::default()
                        })
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(l10n::text(ui.ctx(), heading))
                                    .size(theme.text.small)
                                    .color(section_color),
                            )
                        });
                    ui.add_space(theme.spacing.lg);
                    for &item in items {
                        if SidebarTab::new(item.name(), category.get() == item)
                            .icon(item.icon())
                            .fill(theme.ui.panel_bg_secondary)
                            .show(ui)
                        {
                            category.set(item);
                        }
                    }
                }
            },
            |ui| {
                self.sel_category = category.get();
                self.draw_tabs(ui, theme);
                action = self.draw_indicator_list(ui, config, theme);
            },
        );
        action
    }

    /// The sidebar category and built-in page that list `ind`, from its factory category.
    fn destination(ind: &IndicatorInfo) -> (IndicatorCategory, IndicatorTab) {
        use {IndicatorCategory as C, IndicatorTab as T};
        match ind.category.as_str() {
            IndicatorFactory::PERSONAL => (C::MyScripts, T::Indicators),
            IndicatorFactory::COMMUNITY => (C::Community, T::Indicators),
            IndicatorFactory::PROFILES => (C::BuiltIns, T::Profiles),
            IndicatorFactory::PATTERNS => (C::BuiltIns, T::Patterns),
            IndicatorFactory::ORDER_FLOW => (C::BuiltIns, T::OrderFlow),
            _ => (C::BuiltIns, T::Indicators),
        }
    }

    /// Favourites remain available when empty; other categories need registered studies.
    fn offers(&self, category: IndicatorCategory) -> bool {
        category == IndicatorCategory::Favorites
            || self
                .indicators
                .iter()
                .any(|ind| Self::destination(ind).0 == category && self.accepts(ind))
    }

    /// The built-in pages that have studies, above the built-in results. A search covers
    /// every page, so they are hidden while one is typed.
    fn draw_tabs(&mut self, ui: &mut Ui, theme: &Theme) {
        let tabs: Vec<_> = IndicatorTab::all()
            .iter()
            .copied()
            .filter(|tab| {
                self.indicators.iter().any(|ind| {
                    Self::destination(ind) == (IndicatorCategory::BuiltIns, *tab)
                        && self.accepts(ind)
                })
            })
            .collect();
        if !tabs.contains(&self.active_tab) {
            self.active_tab = tabs.first().copied().unwrap_or(IndicatorTab::Indicators);
        }
        if self.sel_category == IndicatorCategory::BuiltIns
            && tabs.len() > 1
            && self.search.is_empty()
        {
            let mut pill_theme = theme.clone();
            if is_default_light(theme) {
                pill_theme.ui.text = Color32::from_gray(26);
                // The tab bar blends this surface with white at 60%, giving #f2f2f2.
                pill_theme.ui.panel_bg_secondary = Color32::from_gray(233);
            }
            let _palette = crate::theme::ThemeScope::new(ui.ctx(), &pill_theme);
            TabBar::new(&tabs, &mut self.active_tab).pills().show(ui);
            ui.add_space(theme.spacing.xl);
        }
    }

    fn accepts(&self, ind: &IndicatorInfo) -> bool {
        !self.unavailable_order_flow.contains(&ind.id)
            && (self.source.is_none()
                || self
                    .factory
                    .create(&ind.id)
                    .is_some_and(|study| study.source().is_some()))
    }

    /// Whether `ind` belongs to the active category, and page for the built-ins, and matches
    /// the search.
    fn matches(&self, ctx: &egui::Context, ind: &IndicatorInfo) -> bool {
        if !self.accepts(ind) {
            return false;
        }
        let searched = [&ind.name, &ind.desc, &ind.id, &ind.category]
            .iter()
            .any(|text| self.search.matches(text) || self.search.matches(&l10n::text(ctx, text)));
        if !searched {
            return false;
        }

        let (category, tab) = Self::destination(ind);
        match self.sel_category {
            IndicatorCategory::Favorites => self.favorites.contains(&ind.id),
            IndicatorCategory::Premium => ind.is_premium,
            IndicatorCategory::BuiltIns => {
                category == IndicatorCategory::BuiltIns
                    && !ind.is_premium
                    && (!self.search.is_empty() || tab == self.active_tab)
            }
            selected => category == selected,
        }
    }

    fn draw_indicator_list(
        &mut self,
        ui: &mut Ui,
        config: &IndicatorDialogConfig,
        theme: &Theme,
    ) -> IndicatorDialogAction {
        let mut filtered: Vec<IndicatorInfo> = self
            .indicators
            .iter()
            .filter(|ind| self.matches(ui.ctx(), ind))
            .cloned()
            .collect();
        filtered.sort_by_key(|indicator| !self.favorites.contains(&indicator.id));
        if filtered.is_empty() {
            self.draw_empty_state(ui);
            return IndicatorDialogAction::None;
        }

        // The column header stays in place above the scrolled rows.
        let (_, header) = ui.allocate_space(Vec2::new(
            ui.available_width(),
            DESIGN_TOKENS.sizing.button_sm,
        ));
        let header_color = if is_default_light(theme) {
            Color32::from_gray(114)
        } else {
            theme.ui.text_muted
        };
        let label = ui.painter().layout_no_wrap(
            l10n::text(ui.ctx(), "NAME"),
            FontId::proportional(theme.text.small),
            header_color,
        );
        let label_pos = Pos2::new(
            header.min.x + DESIGN_TOKENS.sizing.button_md,
            header.center().y - label.size().y / 2.0,
        );
        ui.painter().galley(label_pos, label, header_color);

        let mut action = IndicatorDialogAction::None;
        // The scroll bar takes its own column, so it never covers the badges.
        crate::styles::menu_scroll_bar(ui);
        if is_default_light(theme) {
            let scroll = &mut ui.spacing_mut().scroll;
            scroll.dormant_handle_opacity = 99.0 / 255.0;
            scroll.active_handle_opacity = 99.0 / 255.0;
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                // Room above the first row and below the last for the focus ring.
                let ring = theme.spacing.xs;
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
        let no_match = l10n::format_text(
            ui.ctx(),
            "No indicators match \"{query}\"",
            &[("query", query)],
        );
        let state = match self.sel_category {
            _ if !query.is_empty() => EmptyState::new(&no_match)
                .description("Check the spelling or try another category."),
            IndicatorCategory::Favorites => EmptyState::new("No favorites yet")
                .icon(&icons::STAR_EMPTY)
                .description("Star an indicator to keep it here."),
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
        let desired_size = Vec2::new(
            ui.available_width(),
            32.0_f32.max(theme.text.title + 2.0 * theme.spacing.sm),
        );
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());

        let is_favorite = self.favorites.contains(&indicator.id);
        let translated_label = l10n::text(ui.ctx(), &indicator.name);
        response.widget_info(|| {
            WidgetInfo::labeled(Role::ListBoxOption, response.enabled(), &translated_label)
        });
        if is_first && self.focus_first_row {
            response.request_focus();
        }
        if response.gained_focus() {
            response.scroll_to_me(None);
        }

        if response.hovered() {
            ui.painter()
                .rect_filled(rect, theme.radius.md, config.hover_color);
        }
        let star_res = Self::draw_favorite_star(
            ui,
            config,
            rect,
            is_favorite,
            response.hovered() || response.has_focus(),
        );
        let text_right = Self::draw_item_badges(ui, theme, rect, indicator, config.muted_color);
        Self::draw_item_text(ui, rect, text_right, indicator, config.text_color);
        draw_focus_ring(ui, &response, theme.radius.md);

        self.handle_item_clicks(indicator, is_favorite, &response, &star_res)
    }

    /// The favourite toggle at the left of a row.
    fn star_rect(ui: &Ui, row: Rect) -> Rect {
        Rect::from_min_size(
            Pos2::new(row.min.x + ui.active_theme().spacing.sm, row.min.y),
            Vec2::new(DESIGN_TOKENS.sizing.button_sm, row.height()),
        )
    }

    /// The star shows on a favourite and on the row under the pointer or with the focus; the
    /// toggle stays clickable and focusable while hidden.
    fn draw_favorite_star(
        ui: &mut Ui,
        config: &IndicatorDialogConfig,
        rect: Rect,
        is_favorite: bool,
        row_active: bool,
    ) -> Response {
        let star_rect = Self::star_rect(ui, rect);
        let star_res = ui.allocate_rect(star_rect, Sense::click());
        let tooltip = if is_favorite {
            "Remove from favorites"
        } else {
            "Add to favorites"
        };
        let translated_label = l10n::text(ui.ctx(), tooltip);
        star_res.widget_info(|| {
            WidgetInfo::selected(
                Role::Button,
                star_res.enabled(),
                is_favorite,
                &translated_label,
            )
        });

        let (icon, tint) = if is_favorite {
            (&icons::STAR_FILLED, config.favorite_color)
        } else if star_res.hovered() {
            (&icons::STAR_EMPTY, ui.visuals().text_color())
        } else {
            (&icons::STAR_EMPTY, ui.visuals().weak_text_color())
        };
        if is_favorite || row_active || star_res.hovered() || star_res.has_focus() {
            let size = Vec2::splat(DESIGN_TOKENS.sizing.icon_sm);
            icon.as_image_tinted(size, tint)
                .paint_at(ui, Rect::from_center_size(star_rect.center(), size));
        }
        draw_focus_ring(ui, &star_res, 0.0);
        star_res.on_hover_text(l10n::text(ui.ctx(), tooltip))
    }

    /// The name centred on the row and cut off before `right`.
    fn draw_item_text(ui: &Ui, rect: Rect, right: f32, indicator: &IndicatorInfo, text: Color32) {
        let x = rect.min.x + DESIGN_TOKENS.sizing.button_md;
        let mut job = LayoutJob::simple_singleline(
            l10n::text(ui.ctx(), indicator.name.clone()),
            FontId::proportional(ui.active_theme().text.title),
            text,
        );
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
                l10n::text(ui.ctx(), label),
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
        response: &Response,
        star_res: &Response,
    ) -> Option<IndicatorDialogAction> {
        if star_res.clicked() {
            if is_favorite {
                self.favorites.remove(&indicator.id);
            } else {
                self.favorites.insert(indicator.id.clone());
            }
            return Some(IndicatorDialogAction::ToggleFavorite(indicator.id.clone()));
        }

        let on_star = star_res
            .rect
            .contains(response.hover_pos().unwrap_or_default());
        // A click, or Enter or Space on the focused row, adds the study.
        if response.clicked() && !on_star {
            return self.add(&indicator.id);
        }
        None
    }

    /// The study registered as `id` with its default inputs and the requested source. A
    /// source the study rejects is reported in the dialog and adds nothing.
    fn add(&mut self, id: &str) -> Option<IndicatorDialogAction> {
        self.source_error = None;
        if self.full {
            return None;
        }
        let mut study = self.factory.create(id)?;
        if let Some(source) = self.source {
            if !study
                .inputs()
                .iter()
                .any(|input| input.id == "source" && matches!(input.kind, InputKind::Source))
            {
                study = Box::new(ConfiguredStudy::new(study));
            }
            if let Err(error) = study.set_input("source", InputValue::Source(source)) {
                self.source_error = Some(error);
                return None;
            }
        }
        Some(IndicatorDialogAction::AddIndicator(study))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scales::StudyFormat;

    #[test]
    fn metric_catalog_ignores_old_symbols_and_creates_host_studies() {
        use crate::data::{HistoryPage, HistoryRequest, Reply};
        use std::sync::Mutex;
        #[derive(Default)]
        struct Feed(Mutex<Vec<Reply<Vec<MetricSeries>>>>);
        impl Datafeed for Feed {
            fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
                reply.send(Err(FeedError::NotSupported));
            }
            fn metrics(&self, _: &str, reply: Reply<Vec<MetricSeries>>) {
                self.0.lock().unwrap().push(reply);
            }
        }
        let feed = Arc::new(Feed::default());
        let ctx = egui::Context::default();
        let mut dialog = IndicatorDialog::new();
        dialog.set_metric_source(Some(feed.clone()), "OLD");
        dialog.poll_metrics(&ctx);
        dialog.set_metric_source(Some(feed.clone()), "CURRENT");
        dialog.poll_metrics(&ctx);
        let mut replies = feed.0.lock().unwrap();
        for (reply, name) in replies.drain(..).zip(["Stale metric", "Funding rate"]) {
            reply.send(Ok(vec![MetricSeries {
                id: "funding".into(),
                name: name.into(),
                unit: "%".into(),
                format: StudyFormat::Percent,
            }]));
        }
        dialog.poll_metrics(&ctx);
        assert!(
            dialog
                .indicators
                .iter()
                .any(|info| info.name == "Funding rate")
        );
        assert!(
            !dialog
                .indicators
                .iter()
                .any(|info| info.name == "Stale metric")
        );
        let Some(IndicatorDialogAction::AddIndicator(study)) = dialog.add("Metric: funding") else {
            panic!("the metric is added");
        };
        assert_eq!(study.name(), "Funding rate");
        assert!(!study.is_overlay());
        assert_eq!(study.value_format(), StudyFormat::Percent);
        assert_eq!(study.data_requests()[0].metric.as_deref(), Some("funding"));
        use egui_kittest::{Harness, kittest::Queryable};
        for theme in [Theme::light(), Theme::dark()] {
            for width in [640.0, 800.0, 1440.0] {
                dialog.open();
                *dialog.search.query_mut() = "Funding rate".into();
                let style = theme.clone();
                let mut added = false;
                let mut harness = Harness::builder()
                    .with_size(Vec2::new(width, 640.0))
                    .with_context_setup(move |ctx| {
                        egui_extras::install_image_loaders(ctx);
                        crate::theme::apply_to_egui(ctx, &style);
                    })
                    .build_ui(|ui| {
                        if let IndicatorDialogAction::AddIndicator(study) =
                            dialog.show(ui.ctx(), &theme)
                        {
                            assert_eq!(study.name(), "Funding rate");
                            added = true;
                        }
                    });
                harness.run();
                let row = harness.get_by_role_and_label(Role::ListBoxOption, "Funding rate");
                assert!(
                    harness.ctx.content_rect().contains_rect(row.rect()),
                    "width {width}"
                );
                row.click();
                harness.run();
                drop(harness);
                assert!(added, "width {width}");
            }
        }
        dialog.set_metric_source(None, "CURRENT");
        assert!(dialog.add("Metric: funding").is_none());
        assert!(dialog.add("SMA").is_some());
    }

    #[test]
    fn source_chooser_lists_favorites_first_and_adds_with_the_requested_source() {
        use egui_kittest::{Harness, kittest::Queryable};
        let mut chart = crate::widget::ChartBuilder::new().build();
        let parent = chart.add_indicator(Box::new(SMA::new(3)));
        let source = StudySource::Study {
            id: parent.0,
            plot: 0,
        };
        for theme in [Theme::light(), Theme::dark()] {
            for width in [320.0, 640.0] {
                let mut dialog = IndicatorDialog::new();
                let name = dialog
                    .indicators
                    .iter()
                    .find(|info| info.id == "SMA")
                    .unwrap()
                    .name
                    .clone();
                dialog.favorites.insert("SMA".into());
                assert!(dialog.open_for_chart(&chart, Some(source)));
                let mut added = None;
                let style = theme.clone();
                let mut harness = Harness::builder()
                    .with_size(Vec2::new(width, 640.0))
                    .with_context_setup(move |ctx| {
                        egui_extras::install_image_loaders(ctx);
                        crate::theme::apply_to_egui(ctx, &style);
                    })
                    .build_ui(|ui| {
                        if let IndicatorDialogAction::AddIndicator(study) =
                            dialog.show(ui.ctx(), &theme)
                        {
                            added = study.source();
                        }
                    });
                harness.run();
                let row = harness.get_by_role_and_label(Role::ListBoxOption, &name);
                assert!(
                    harness
                        .query_all_by_role(Role::ListBoxOption)
                        .all(|other| other.rect().top() >= row.rect().top())
                );
                row.click();
                harness.run();
                drop(harness);
                assert_eq!(added, Some(source), "width {width}");
                assert!(dialog.is_open(), "the dialog stays open for the next study");
                dialog.open();
                assert!(
                    dialog.source.is_none(),
                    "opening normally clears the previous source"
                );
            }
        }
    }

    /// A chart at its study cap shows the message and adds no study, whichever way a row is chosen.
    #[test]
    fn dialog_at_the_study_limit_says_so_and_adds_nothing() {
        use egui_kittest::{Harness, kittest::Queryable};
        let theme = Theme::light();
        let mut dialog = IndicatorDialog::new();
        dialog.open();
        dialog.set_full(true);
        let style = theme.clone();
        let mut harness = Harness::builder()
            .with_size(Vec2::new(640.0, 640.0))
            .with_context_setup(move |ctx| {
                egui_extras::install_image_loaders(ctx);
                crate::theme::apply_to_egui(ctx, &style);
            })
            .build_ui(|ui| {
                dialog.show(ui.ctx(), &theme);
            });
        harness.run();
        harness.get_by_label("The chart has reached its study limit");
        drop(harness);
        assert!(dialog.add("SMA").is_none());
        dialog.set_full(false);
        assert!(dialog.add("SMA").is_some());
    }

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
        let ctx = egui::Context::default();
        let listed: Vec<_> = restored
            .indicators
            .iter()
            .filter(|i| restored.matches(&ctx, i))
            .collect();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "BB");
        assert!(restored.favorites.contains("Host study"));
        assert_eq!(
            saved,
            serde_json::to_string(&restored.preferences()).unwrap()
        );
        for category in [IndicatorCategory::BuiltIns, IndicatorCategory::Favorites] {
            restored.sel_category = category;
            let output = ctx.run_ui(Default::default(), |ui| {
                restored.draw_indicator_list(ui, &restored.config.clone(), &Theme::dark());
            });
            let first = output.shapes.iter().find_map(|shape| match &shape.shape {
                egui::Shape::Text(text)
                    if restored
                        .indicators
                        .iter()
                        .any(|info| info.name == text.galley.job.text) =>
                {
                    Some(text.galley.job.text.clone())
                }
                _ => None,
            });
            output.drop_without_applying_deltas();
            let favorite = restored
                .indicators
                .iter()
                .find(|info| info.id == "BB")
                .unwrap();
            assert_eq!(
                first.as_deref(),
                Some(favorite.name.as_str()),
                "{category:?}"
            );
        }
    }

    #[test]
    fn added_study_keeps_the_selected_source_and_reopening_clears_it() {
        use crate::model::PriceSource;
        let mut chart = crate::widget::ChartBuilder::new().build();
        let id = chart.add_indicator(Box::new(SMA::new(3)));
        let mut dialog = IndicatorDialog::new();
        for source in [
            StudySource::Study { id: id.0, plot: 0 },
            StudySource::Price(PriceSource::Open),
        ] {
            assert!(dialog.open_for_chart(&chart, Some(source)));
            let Some(IndicatorDialogAction::AddIndicator(study)) = dialog.add("SMA") else {
                panic!("a source-capable study is added");
            };
            assert_eq!(study.source(), Some(source));
        }
        assert!(dialog.open_for_chart(&chart, Some(StudySource::Study { id: id.0, plot: 0 })));
        assert!(
            dialog.add("ATR").is_none(),
            "OHLC studies cannot silently ignore the source"
        );
        dialog.open();
        assert!(matches!(
            dialog.add("ATR"),
            Some(IndicatorDialogAction::AddIndicator(_))
        ));
        assert!(!dialog.open_for_chart(&chart, Some(StudySource::Study { id: id.0, plot: 99 })));
        assert!(!dialog.is_open());

        let mut factory = IndicatorFactory::new();
        factory.register("Host source", || {
            let mut study = ConfiguredStudy::new(Box::new(SMA::new(3)));
            study
                .set_input(
                    "calculation_timeframe",
                    InputValue::Timeframe(Some(crate::model::Timeframe::Hour1)),
                )
                .unwrap();
            Box::new(study)
        });
        let mut dialog = IndicatorDialog::new().with_factory(factory);
        assert!(dialog.open_for_chart(&chart, Some(StudySource::Study { id: id.0, plot: 0 })));
        assert!(
            dialog.add("Host source").is_none(),
            "the host's source validation must be retained"
        );
        assert!(
            matches!(dialog.source_error, Some(StudyError::InvalidInput { ref id, .. }) if id == "source")
        );
        assert!(dialog.is_open());
        let message = dialog.source_error.as_ref().unwrap().to_string();
        for theme in [Theme::light(), Theme::dark()] {
            for width in [320.0, 640.0] {
                use egui_kittest::{Harness, kittest::Queryable};
                let style = theme.clone();
                let mut harness = Harness::builder()
                    .with_size(Vec2::new(width, 640.0))
                    .with_context_setup(move |ctx| {
                        egui_extras::install_image_loaders(ctx);
                        crate::theme::apply_to_egui(ctx, &style);
                    })
                    .build_ui(|ui| {
                        dialog.show(ui.ctx(), &theme);
                    });
                harness.run();
                let error = harness.get_by_label(&message);
                assert!(
                    harness.ctx.content_rect().contains_rect(error.rect()),
                    "source error at width {width}"
                );
            }
        }
        assert!(dialog.open_for_chart(&chart, Some(StudySource::Price(PriceSource::Open))));
        let Some(IndicatorDialogAction::AddIndicator(study)) = dialog.add("Host source") else {
            panic!("a compatible price source is accepted");
        };
        assert!(dialog.source_error.is_none());
        assert_eq!(
            study
                .inputs()
                .iter()
                .find(|input| input.id == "calculation_timeframe")
                .unwrap()
                .value,
            InputValue::Timeframe(Some(crate::model::Timeframe::Hour1))
        );
    }

    #[test]
    fn each_tab_and_category_lists_only_its_own_indicators() {
        use IndicatorCategory::*;
        use IndicatorTab::*;
        // (tab, category, search, studies listed, studies not listed). A search covers every
        // built-in page.
        let cases: [(_, _, _, &[&str], &[&str]); 9] = [
            (
                Indicators,
                BuiltIns,
                "",
                &["RSI", "Host SMA"],
                &[
                    "Vol Profile VR",
                    "All Candlestick Patterns",
                    "Execution Delta",
                    "Cumulative Execution Delta",
                    "Host flow",
                    "My SMA",
                    "Shared SMA",
                ],
            ),
            (
                Profiles,
                BuiltIns,
                "",
                &["Vol Profile VR"],
                &["RSI", "All Candlestick Patterns"],
            ),
            (
                Patterns,
                BuiltIns,
                "",
                &["All Candlestick Patterns"],
                &["RSI", "Vol Profile VR"],
            ),
            (
                OrderFlow,
                BuiltIns,
                "",
                &["Execution Delta", "Cumulative Execution Delta", "Host flow"],
                &["RSI", "Vol Profile VR", "All Candlestick Patterns"],
            ),
            (
                Indicators,
                BuiltIns,
                "Vol Profile VR",
                &["Vol Profile VR"],
                &["RSI"],
            ),
            (
                OrderFlow,
                BuiltIns,
                "delta",
                &[
                    "Execution Delta",
                    "Cumulative Execution Delta",
                    "Multi-Range Delta",
                ],
                &["RSI", "Quoted Spread"],
            ),
            (
                OrderFlow,
                Favorites,
                "",
                &["Execution Delta", "Host flow"],
                &["Cumulative Execution Delta", "RSI"],
            ),
            (
                Indicators,
                MyScripts,
                "",
                &["My SMA"],
                &["Host SMA", "Shared SMA", "RSI"],
            ),
            (
                Indicators,
                Community,
                "",
                &["Shared SMA"],
                &["My SMA", "RSI"],
            ),
        ];
        let factory = || {
            let mut factory = IndicatorFactory::new();
            factory.register("Host SMA", || Box::new(SMA::new(42)));
            factory.register_in(IndicatorFactory::ORDER_FLOW, "Host flow", || {
                Box::new(SMA::new(42))
            });
            factory.register_in(IndicatorFactory::PERSONAL, "My SMA", || {
                Box::new(SMA::new(42))
            });
            factory.register_in(IndicatorFactory::COMMUNITY, "Shared SMA", || {
                Box::new(SMA::new(42))
            });
            factory
        };
        // The user's scripts and community studies are offered only while a host supplies some.
        let mut builtin = IndicatorDialog::new();
        let hosted = IndicatorDialog::new().with_factory(factory());
        for category in [MyScripts, Community] {
            assert!(
                !builtin.offers(category) && hosted.offers(category),
                "{category:?}"
            );
        }
        let mut searched = IndicatorDialog::new().with_factory(factory());
        let study = searched
            .indicators
            .iter()
            .find(|ind| ind.id == "Host SMA")
            .unwrap()
            .clone();
        let name = study.name.clone();
        let ctx = egui::Context::default();
        for tab in [Indicators, OrderFlow] {
            builtin.active_tab = tab;
            for ind in &builtin.indicators {
                if builtin.factory.category(&ind.id) == Some(IndicatorFactory::ORDER_FLOW) {
                    assert_eq!(builtin.matches(&ctx, ind), tab == OrderFlow, "{}", ind.id);
                }
            }
        }
        l10n::set_translator(&ctx, move |english| {
            (english == name).then(|| "Moyenne locale".into())
        });
        for query in ["Moyenne", "Host SMA"] {
            *searched.search.query_mut() = query.into();
            assert!(searched.matches(&ctx, &study), "{query}");
        }
        for (tab, category, query, listed, unlisted) in cases {
            let mut dialog = IndicatorDialog::new().with_factory(factory());
            dialog.active_tab = tab;
            dialog.sel_category = category;
            dialog
                .favorites
                .extend(["Execution Delta".into(), "Host flow".into()]);
            *dialog.search.query_mut() = query.into();
            let lists = |id: &str| {
                let ind = dialog.indicators.iter().find(|i| i.id == id).unwrap();
                dialog.matches(&ctx, ind)
            };
            for id in listed {
                assert_eq!(
                    dialog
                        .indicators
                        .iter()
                        .filter(|ind| ind.id == *id && dialog.matches(&ctx, ind))
                        .count(),
                    1,
                    "{tab:?} / {category:?} / {query:?} lists {id} once"
                );
            }
            for id in unlisted {
                assert!(!lists(id), "{tab:?} / {category:?} / {query:?} omits {id}");
            }
        }

        // A page needs a registered study the dialog accepts; search still spans all pages.
        for has_flow in [false, true] {
            for source in [
                None,
                Some(StudySource::Price(crate::model::PriceSource::Open)),
            ] {
                let mut factory = IndicatorFactory::new();
                if !has_flow {
                    for id in factory.list() {
                        if factory.category(&id) == Some(IndicatorFactory::ORDER_FLOW) {
                            factory.unregister(&id);
                        }
                    }
                }
                let mut dialog = IndicatorDialog::new().with_factory(factory);
                dialog.open_tab(OrderFlow);
                dialog.source = source;
                let output = ctx.run_ui(Default::default(), |ui| {
                    dialog.draw_tabs(ui, &Theme::dark());
                });
                let offered = output.shapes.iter().any(|shape| {
                    matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == "Order flow")
                });
                let accepted = has_flow && source.is_none();
                assert_eq!(offered, accepted, "{has_flow} / {source:?}");
                assert_eq!(
                    dialog.active_tab,
                    if accepted { OrderFlow } else { Indicators }
                );
                output.drop_without_applying_deltas();
                dialog.source = None;
                *dialog.search.query_mut() = "delta".into();
                for ind in dialog
                    .indicators
                    .iter()
                    .filter(|ind| ind.name.to_lowercase().contains("delta"))
                {
                    assert!(dialog.matches(&ctx, ind), "{} / {has_flow}", ind.id);
                }
                let ids: Vec<_> = dialog
                    .indicators
                    .iter()
                    .filter(|ind| dialog.matches(&ctx, ind))
                    .map(|ind| &ind.id)
                    .collect();
                assert_eq!(ids.len(), ids.iter().collect::<HashSet<_>>().len());
            }
        }

        // A host may replace the catalogue with just one destination.
        for (category, selected, tab) in [
            (IndicatorFactory::PROFILES, BuiltIns, Profiles),
            (IndicatorFactory::PATTERNS, BuiltIns, Patterns),
            (IndicatorFactory::ORDER_FLOW, BuiltIns, OrderFlow),
            (IndicatorFactory::PERSONAL, MyScripts, Indicators),
            (IndicatorFactory::COMMUNITY, Community, Indicators),
        ] {
            for source in [
                None,
                Some(StudySource::Price(crate::model::PriceSource::Open)),
            ] {
                let mut factory = IndicatorFactory::new();
                for name in factory.list() {
                    factory.unregister(&name);
                }
                factory.register_in(category, "Host study", || Box::new(ATR::new(14)));
                let mut dialog = IndicatorDialog::new().with_factory(factory);
                assert!(dialog.open_for_chart(&crate::widget::ChartBuilder::new().build(), source));
                ctx.run_ui(Default::default(), |ui| {
                    dialog.draw_content(ui, &dialog.config.clone(), &Theme::dark());
                })
                .drop_without_applying_deltas();
                let compatible = source.is_none();
                assert_eq!(
                    dialog.sel_category,
                    if compatible { selected } else { Favorites },
                    "{category} / {source:?}"
                );
                assert_eq!(
                    dialog.active_tab,
                    if compatible { tab } else { Indicators },
                    "{category} / {source:?}"
                );
                assert_eq!(
                    dialog.offers(BuiltIns),
                    compatible && selected == BuiltIns,
                    "{category} / {source:?}"
                );
                assert_eq!(
                    dialog.matches(&ctx, &dialog.indicators[0]),
                    compatible,
                    "{category} / {source:?}"
                );
            }
        }

        use crate::data::{FeedCapabilities, OrderFlowKind};
        use crate::model::{FeedService::*, Instrument};
        let instrument = Instrument::new("A");
        let none = FeedCapabilities::new().with_services([]);
        let live = FeedCapabilities::new().with_services([LiveTrades]);
        let sided = FeedCapabilities::new().with_services([LiveTrades, TradeSides]);
        let all =
            FeedCapabilities::new().with_services([LiveTrades, TradeSides, BestBidAsk, Depth]);
        let unknown = FeedCapabilities::new();
        let narrowed = instrument.clone().with_services([]);
        let declared = instrument.clone().with_services([LiveTrades, TradeSides]);
        for (updated, capabilities, symbol, expected) in [
            (false, None, None, [true; 4]),
            (true, None, None, [false; 4]),
            (true, Some(&sided), None, [false; 4]),
            (true, None, Some(&declared), [false; 4]),
            (true, Some(&unknown), Some(&instrument), [false; 4]),
            (true, Some(&none), Some(&instrument), [false; 4]),
            (true, Some(&all), Some(&narrowed), [false; 4]),
            (true, Some(&none), Some(&declared), [false; 4]),
            (
                true,
                Some(&unknown),
                Some(&declared),
                [true, true, false, false],
            ),
            (
                true,
                Some(&live),
                Some(&instrument),
                [true, false, false, false],
            ),
            (
                true,
                Some(&sided),
                Some(&instrument),
                [true, true, false, false],
            ),
            (true, Some(&all), Some(&instrument), [true; 4]),
        ] {
            let mut dialog = IndicatorDialog::new();
            if updated {
                dialog.set_feed_capabilities(capabilities, symbol);
            }
            dialog.favorites.insert("Quoted Spread".into());
            dialog.open_tab(OrderFlow);
            for query in ["", "Order flow"] {
                *dialog.search.query_mut() = query.into();
                for (id, accepted) in [
                    "Trade Rate",
                    "Rolling Volume and Delta",
                    "Quoted Spread",
                    "Book Size",
                ]
                .into_iter()
                .zip(expected)
                {
                    let ind = dialog.indicators.iter().find(|ind| ind.id == id).unwrap();
                    assert_eq!(
                        dialog.matches(&ctx, ind),
                        accepted,
                        "{id} / {query:?} / {capabilities:?}"
                    );
                }
            }
            dialog.search.clear();
            let output = ctx.run_ui(Default::default(), |ui| {
                dialog.draw_tabs(ui, &Theme::dark())
            });
            assert_eq!(output.shapes.iter().any(|shape| {
                matches!(&shape.shape, egui::Shape::Text(text) if text.galley.job.text == "Order flow")
            }), expected.iter().any(|accepted| *accepted));
            output.drop_without_applying_deltas();
            dialog.sel_category = Favorites;
            let spread = dialog
                .indicators
                .iter()
                .find(|ind| ind.id == "Quoted Spread")
                .unwrap();
            assert_eq!(dialog.matches(&ctx, spread), expected[2]);
            assert!(dialog.preferences().favorites.contains("Quoted Spread"));
            // Profiles keep their bar-based fallback even though they also expose a footprint.
            dialog.sel_category = BuiltIns;
            dialog.active_tab = Profiles;
            let profile = dialog
                .indicators
                .iter()
                .find(|ind| ind.id == "Vol Profile VR")
                .unwrap();
            assert!(dialog.matches(&ctx, profile));
            dialog.set_feed_capabilities(Some(&all), Some(&instrument));
            dialog.sel_category = Favorites;
            let spread = dialog
                .indicators
                .iter()
                .find(|ind| ind.id == "Quoted Spread")
                .unwrap();
            assert!(dialog.matches(&ctx, spread));
        }

        // Repeated capability updates and frames reuse the requirements; replacing the factory
        // reevaluates them, including a host study registered in the Order flow category.
        use std::sync::atomic::{AtomicUsize, Ordering};
        let builds = Arc::new(AtomicUsize::new(0));
        let counted = |kind| {
            let builds = builds.clone();
            let mut factory = IndicatorFactory::new();
            for id in factory.list() {
                factory.unregister(&id);
            }
            factory.register_in(IndicatorFactory::ORDER_FLOW, "Host book", move || {
                builds.fetch_add(1, Ordering::Relaxed);
                Box::new(OrderFlowStudy::new(kind))
            });
            factory
        };
        let mut dialog = IndicatorDialog::new().with_factory(counted(OrderFlowKind::BookSize {
            levels: 5,
            distance: 0.0,
        }));
        dialog.set_feed_capabilities(Some(&live), Some(&instrument));
        assert!(!dialog.offers(BuiltIns));
        let built = builds.load(Ordering::Relaxed);
        for _ in 0..3 {
            dialog.set_feed_capabilities(Some(&live), Some(&instrument));
            ctx.run_ui(Default::default(), |ui| {
                dialog.draw_content(ui, &dialog.config.clone(), &Theme::dark());
            })
            .drop_without_applying_deltas();
        }
        assert_eq!(builds.load(Ordering::Relaxed), built);
        dialog = dialog.with_factory(counted(OrderFlowKind::TradeRate {
            volume: false,
            seconds: 60,
            instant: 3,
        }));
        assert!(dialog.offers(BuiltIns));
        dialog.set_feed_capabilities(Some(&none), Some(&instrument));
        assert!(!dialog.offers(BuiltIns));
        dialog.set_feed_capabilities(Some(&all), Some(&instrument));
        assert!(dialog.offers(BuiltIns));

        // A study the host registers, under a new name or a built-in's, is added through the
        // factory.
        for name in ["Host SMA", "RSI"] {
            let mut factory = factory();
            factory.register(name, || Box::new(SMA::new(42)));
            let mut dialog = IndicatorDialog::new().with_factory(factory);
            assert!(dialog.indicators.iter().any(|i| i.id == name));
            let Some(IndicatorDialogAction::AddIndicator(study)) = dialog.add(name) else {
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
                    colors: None,
                    payload: None,
                }
            })
            .collect();
        let factory = IndicatorFactory::new();
        let mut dialog = IndicatorDialog::new();
        let mut differ = Vec::new();
        for info in dialog.indicators.clone() {
            let Some(IndicatorDialogAction::AddIndicator(mut added)) = dialog.add(&info.id) else {
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

    /// The light chooser has distinct hint inks; other palettes keep their supplied roles.
    #[test]
    fn chooser_chrome_uses_light_inks_and_preserves_other_palettes() {
        let mut custom = Theme::light();
        custom.ui.text_muted = Color32::RED;
        for (theme, section, header, hint, selected, pill) in [
            (
                Theme::light(),
                Color32::from_gray(116),
                Color32::from_gray(114),
                Color32::from_gray(112),
                Color32::from_gray(26),
                Color32::from_gray(242),
            ),
            (
                Theme::dark(),
                Theme::dark().ui.text_muted,
                Theme::dark().ui.text_muted,
                Theme::dark().ui.text_muted,
                Theme::dark().ui.text,
                Theme::dark()
                    .ui
                    .panel_bg
                    .lerp_to_gamma(Theme::dark().ui.panel_bg_secondary, 0.6),
            ),
            (
                custom.clone(),
                Color32::RED,
                Color32::RED,
                Color32::RED,
                custom.ui.text,
                custom
                    .ui
                    .panel_bg
                    .lerp_to_gamma(custom.ui.panel_bg_secondary, 0.6),
            ),
        ] {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            ctx.all_styles_mut(|style| style.animation_time = 0.0);
            egui_extras::install_image_loaders(&ctx);
            let mut dialog = IndicatorDialog::new();
            dialog.open();
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(1440.0, 900.0))),
                ..Default::default()
            };
            for _ in 0..2 {
                ctx.run_ui(input.clone(), |ui| {
                    dialog.show(ui.ctx(), &theme);
                })
                .drop_without_applying_deltas();
            }
            let output = ctx.run_ui(input, |ui| {
                dialog.show(ui.ctx(), &theme);
            });
            for (label, color) in [
                ("PERSONAL", section),
                ("NAME", header),
                ("Search indicators", hint),
            ] {
                let text = output
                    .shapes
                    .iter()
                    .find_map(|shape| match &shape.shape {
                        egui::Shape::Text(text) if text.galley.job.text == label => Some(text),
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("missing {label}"));
                assert_eq!(text.galley.job.sections[0].format.color, color, "{label}");
            }
            assert!(
                output.shapes.iter().any(|shape| matches!(
                    &shape.shape, egui::Shape::Rect(rect)
                    if rect.brush.is_some() && rect.rect.size() == Vec2::splat(24.0)
                        && rect.fill == hint
                )),
                "search glyph canvas"
            );
            for color in [selected, pill] {
                assert!(
                    output.shapes.iter().any(|shape| matches!(
                        &shape.shape, egui::Shape::Rect(rect)
                        if rect.brush.is_none() && rect.fill == color && rect.rect.height() == 28.0
                    )),
                    "pill {color:?}"
                );
            }
            if theme.ui == Theme::light().ui {
                assert!(
                    output.shapes.iter().any(|shape| matches!(
                        &shape.shape, egui::Shape::Rect(rect)
                        if rect.rect.width() == 3.0 && rect.fill == Color32::from_black_alpha(99)
                    )),
                    "light scroll handle"
                );
            }
            output.drop_without_applying_deltas();
            assert_eq!(ctx.active_theme(), theme, "chooser palette stays local");
        }
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

#[cfg(test)]
mod options_profile_tests {
    use super::*;
    use crate::data::{
        HistoryPage, HistoryRequest, OptionChainQuery, OptionChainSink, OptionKind, OptionQuote,
        ProfileDisplay, Reply,
    };
    use crate::model::{Bar, FeedService};
    use chrono::{NaiveDate, TimeDelta, Utc};
    use std::sync::Mutex;

    #[derive(Default)]
    struct Feed {
        dates: Vec<NaiveDate>,
        rows: Vec<OptionQuote>,
        sink: Mutex<Option<OptionChainSink>>,
    }
    impl Datafeed for Feed {
        fn history(&self, _: HistoryRequest, reply: Reply<HistoryPage>) {
            reply.send(Ok(HistoryPage::default()));
        }
        fn option_expiries(&self, _: &str, reply: Reply<Vec<NaiveDate>>) {
            reply.send(Ok(self.dates.clone()));
        }
        fn option_chain(&self, _: OptionChainQuery, sink: OptionChainSink) {
            sink.push(self.rows.clone()).unwrap();
            *self.sink.lock().unwrap() = Some(sink);
        }
    }
    fn quote(
        expiry: NaiveDate,
        strike: f64,
        kind: OptionKind,
        volume: f64,
        interest: f64,
    ) -> OptionQuote {
        let mut row = OptionQuote::new(
            format!("{expiry}-{strike}-{kind:?}"),
            expiry,
            strike,
            kind,
            Utc::now(),
        );
        row.volume = Some(volume);
        row.open_interest = Some(interest);
        row.multiplier = Some(100.0);
        row
    }
    fn show(dialog: &mut IndicatorDialog, ctx: &egui::Context) {
        dialog.open_tab(IndicatorTab::Profiles);
        ctx.run_ui(Default::default(), |ui| {
            dialog.show(ui.ctx(), &Theme::dark());
        })
        .drop_without_applying_deltas();
    }
    fn profile(feed: Arc<Feed>) -> Box<dyn Indicator> {
        let mut dialog = IndicatorDialog::new();
        dialog.set_metric_source(Some(feed), "SAMPLE");
        dialog.set_feed_capabilities(
            Some(&FeedCapabilities::new().with_services([FeedService::OptionChain])),
            Some(&Instrument::new("SAMPLE").with_tick_size(0.01)),
        );
        show(&mut dialog, &egui::Context::default());
        let mut study = dialog
            .factory()
            .create("Options Volume Profile")
            .expect("declared option chains offer a profile");
        let time = Utc::now();
        study.calculate(
            &(0..3)
                .map(|i| Bar::new(time + TimeDelta::minutes(i), 105.0, 112.0, 98.0, 105.0, 1.0))
                .collect::<Vec<_>>(),
        );
        study
    }

    #[test]
    fn options_profile_expiries_keep_measures_separate_and_missing_fields_hidden() {
        let today = Utc::now().date_naive();
        let tomorrow = today.succ_opt().unwrap();
        let later = today + TimeDelta::days(3);
        let mut rows = Vec::new();
        for (day, calls, puts, volume) in [
            (today, 10.0, 6.0, 20.0),
            (tomorrow, 30.0, 20.0, 100.0),
            (later, 90.0, 80.0, 500.0),
        ] {
            rows.push(quote(day, 100.0, OptionKind::Call, volume, calls));
            rows.push(quote(day, 100.0, OptionKind::Put, volume / 2.0, puts));
        }
        let feed = Arc::new(Feed {
            dates: vec![later, today, tomorrow, today],
            rows: rows.clone(),
            ..Default::default()
        });
        let mut study = profile(feed.clone());
        assert_eq!(study.volume_profiles(0..3)[0].profile.up(), [20.0]);
        assert_eq!(study.plot_value(0, 0), Some(0.5));
        study.set_input("measure", InputValue::Choice(1)).unwrap();
        study.set_input("sides", InputValue::Choice(1)).unwrap();
        assert_eq!(study.plot_value(0, 0), Some(0.6));
        for (expiry, calls, puts) in [(0, 10.0, 6.0), (2, 30.0, 20.0), (4, 130.0, 106.0)] {
            study
                .set_input("expiry", InputValue::Choice(expiry))
                .unwrap();
            let plot = study.volume_profiles(0..3).remove(0);
            assert_eq!(plot.config.display, ProfileDisplay::Delta);
            assert_eq!(plot.profile.up(), [calls]);
            assert_eq!(plot.profile.down(), [puts]);
        }
        study
            .set_input(
                "through_date",
                InputValue::Text((today + TimeDelta::days(2)).to_string()),
            )
            .unwrap();
        assert_eq!(study.volume_profiles(0..3)[0].profile.up(), [40.0]);
        assert!(
            study
                .set_input("through_date", InputValue::Text("2026-02-30".into()))
                .is_err()
        );
        study.set_input("expiry", InputValue::Choice(99)).unwrap();
        study.set_input("expiry", InputValue::Choice(0)).unwrap();
        let sink = feed.sink.lock().unwrap().clone().unwrap();
        rows[0].volume = Some(0.0);
        rows[0].open_interest = None;
        sink.push(rows.clone()).unwrap();
        let measure = study
            .inputs()
            .into_iter()
            .find(|input| input.id == "measure")
            .unwrap();
        assert!(matches!(measure.kind, InputKind::Choice(choices) if choices == ["Volume"]));
        assert!(!study.inputs().iter().any(|input| input.id == "max_pain"));
        assert_eq!(
            study.volume_profiles(0..3)[0].profile.down(),
            [10.0],
            "missing interest falls back to the offered volume"
        );
        study.set_input("measure", InputValue::Choice(0)).unwrap();
        assert_eq!(study.plot_value(0, 0), None, "zero calls have no ratio");
        rows[0].volume = None;
        sink.push(rows).unwrap();
        assert!(
            study.volume_profiles(0..3).is_empty(),
            "unknown volume is not zero"
        );
        let mut interest_only = feed.rows[..2].to_vec();
        for row in &mut interest_only {
            row.volume = None;
        }
        sink.push(interest_only).unwrap();
        assert_eq!(study.volume_profiles(0..3)[0].profile.up(), [10.0]);
        assert_eq!(study.plot_value(0, 0), Some(0.6));
        sink.push(Vec::new()).unwrap();
        assert_eq!(study.plot_value(0, 0), None);
    }

    #[test]
    fn options_profile_rows_follow_strikes_and_max_pain_updates_without_bars() {
        let today = Utc::now().date_naive();
        let rows = vec![
            quote(today, 100.0, OptionKind::Call, 20.0, 1.0),
            quote(today, 110.0, OptionKind::Put, 10.0, 2.0),
        ];
        let feed = Arc::new(Feed {
            dates: vec![today],
            rows: rows.clone(),
            ..Default::default()
        });
        let mut study = profile(feed.clone());
        let plot = study.volume_profiles(0..3).remove(0);
        assert_eq!(
            (plot.profile.up(), plot.profile.down()),
            (&[20.0, 0.0][..], &[0.0, 10.0][..]),
            "one row per listed strike, not per price tick"
        );
        let sink = feed.sink.lock().unwrap().clone().unwrap();
        let mut offset = rows.clone();
        offset[0].strike = 102.5;
        offset[1].strike = 107.5;
        sink.push(offset).unwrap();
        let profile = study.volume_profiles(0..3).remove(0).profile;
        let half = profile.row_height() / 2.0;
        assert_eq!(
            (profile.low() + half, profile.high() - half),
            (102.5, 107.5),
            "rows centre on strikes off a multiple of their spacing"
        );
        assert_eq!(study.plot_value(1, 0), None, "max pain starts off");
        let legend =
            |study: &dyn Indicator| ConfiguredStudy::new(study.clone_box()).legend_visible(1);
        assert!(!legend(&*study), "no max pain readout while it is off");
        study
            .set_input("max_pain", InputValue::Toggle(true))
            .unwrap();
        assert!(legend(&*study));
        // Payout at 100 and 110: 2,000 and 1,000; equal interest ties; a 300 call
        // multiplier makes 110 cost 3,000; no interest ties at zero.
        for (calls, puts, call_multiplier, expected) in [
            (1.0, 2.0, 100.0, 110.0),
            (2.0, 2.0, 100.0, 100.0),
            (1.0, 2.0, 300.0, 100.0),
            (0.0, 0.0, 100.0, 100.0),
        ] {
            let mut rows = rows.clone();
            rows[0].open_interest = Some(calls);
            rows[0].multiplier = Some(call_multiplier);
            rows[1].open_interest = Some(puts);
            sink.push(rows).unwrap();
            assert_eq!(study.plot_value(1, 0), Some(expected));
            assert_eq!(
                study.plot_value(0, 0),
                Some(0.5),
                "volume ratio ignores interest"
            );
        }

        use crate::chart::rendering::{ChartMapping, IndicatorRenderer, RenderContext};
        use crate::scales::{PriceMap, PriceRange, PriceTransform};
        let rect = Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 300.0));
        let price = PriceMap::new(
            PriceTransform {
                mode: crate::scales::PriceScaleMode::Normal,
                base: 100.0,
            },
            PriceRange::new(90.0, 120.0),
            0.0,
            300.0,
            false,
        );
        let coords = ChartMapping::new(rect, 20.0, 0, 2, 3.0, price);
        let ctx = egui::Context::default();
        let output = ctx.run_ui(Default::default(), |ui| {
            IndicatorRenderer::render(
                &RenderContext::new(ui.painter(), rect),
                &[study.clone_box()],
                &[],
                &price,
                &coords,
            );
        });
        assert!(
            output.shapes.iter().any(|clipped| matches!(&clipped.shape, egui::Shape::Path(path)
                if path.points.len() >= 2
                    && path.points.iter().all(|point| (point.y - price.price_to_y(100.0)).abs() < 0.001))),
            "live max pain level is painted"
        );
        output.drop_without_applying_deltas();
    }

    #[test]
    fn options_profile_browser_requires_matching_declared_chain_and_releases_subscriptions() {
        let today = Utc::now().date_naive();
        let feed = Arc::new(Feed {
            dates: vec![today],
            ..Default::default()
        });
        let mut dialog = IndicatorDialog::new();
        dialog.set_metric_source(Some(feed.clone()), "SAMPLE");
        let ctx = egui::Context::default();
        let caps = FeedCapabilities::new().with_services([FeedService::OptionChain]);
        for (capabilities, instrument, offered) in [
            (None, None, false),
            (Some(&caps), Some(Instrument::new("OTHER")), false),
            (
                Some(&caps),
                Some(Instrument::new("SAMPLE").with_services([])),
                false,
            ),
            (Some(&caps), Some(Instrument::new("SAMPLE")), true),
            (None, None, false),
        ] {
            dialog.set_feed_capabilities(capabilities, instrument.as_ref());
            show(&mut dialog, &ctx);
            assert_eq!(dialog.factory().has("Options Volume Profile"), offered);
            if offered {
                let info = dialog
                    .indicators()
                    .iter()
                    .find(|info| info.id == "Options Volume Profile")
                    .unwrap();
                assert_eq!(
                    IndicatorDialog::destination(info),
                    (IndicatorCategory::BuiltIns, IndicatorTab::Profiles)
                );
            }
        }
        dialog.set_feed_capabilities(Some(&caps), Some(&Instrument::new("SAMPLE")));
        show(&mut dialog, &ctx);
        assert!(dialog.factory().has("Options Volume Profile"));
        dialog.set_metric_source(None, "SAMPLE");
        assert!(!dialog.factory().has("Options Volume Profile"));
        assert!(!feed.sink.lock().unwrap().as_ref().unwrap().is_active());
    }
}
