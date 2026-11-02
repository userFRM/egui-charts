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
use crate::data::{Datafeed, FeedError, MetricSeries};
use crate::icons::icons;
use crate::localization as l10n;
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

/// Indicator Dialog
///
/// Modal dialog for searching and adding indicators. It lists every study its
/// [`IndicatorFactory`] registers; a click or Enter on a row adds that study with its
/// default inputs and leaves the dialog open for the next one. Edit the inputs of an added
/// study with [`IndicatorSettingsDialog`](super::IndicatorSettingsDialog).
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
    metric_source: Option<(Arc<dyn Datafeed>, String)>,
    metric_catalog: Option<Arc<Inbox>>,
    metric_names: Vec<String>,
    metric_error: Option<FeedError>,
    source: Option<StudySource>,
    source_error: Option<&'static str>,
    full: bool,
    /// Available indicators, one per factory name
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
                .icon_size(DESIGN_TOKENS.sizing.icon_md),
            focus_search: false,
            focus_first_row: false,
            active_tab: IndicatorTab::Indicators,
            sel_category: IndicatorCategory::BuiltIns,
            favorites: HashSet::new(),
            indicators: IndicatorInfo::from_factory(&factory),
            factory,
            metric_source: None,
            metric_catalog: None,
            metric_names: Vec::new(),
            metric_error: None,
            source: None,
            source_error: None,
            full: false,
            config: IndicatorDialogConfig::default(),
        }
    }

    /// List and create the studies of `factory`, such as the built-ins and the host's own
    /// added with [`IndicatorFactory::register`]
    pub fn with_factory(mut self, factory: IndicatorFactory) -> Self {
        self.indicators = IndicatorInfo::from_factory(&factory);
        self.factory = factory;
        self
    }

    /// List the metrics a feed offers for this instrument alongside the technical studies.
    /// Replacing the feed or symbol discards pending replies from the previous source.
    pub fn set_metric_source(&mut self, feed: Option<Arc<dyn Datafeed>>, symbol: &str) {
        if match (&self.metric_source, &feed) {
            (Some((previous, key)), Some(feed)) => key == symbol && Arc::ptr_eq(previous, feed),
            (None, None) => true,
            _ => false,
        } {
            return;
        }
        self.metric_source = feed.map(|feed| (feed, symbol.to_owned()));
        self.metric_catalog = None;
        self.metric_error = None;
        for name in self.metric_names.drain(..) {
            self.factory.unregister(&name);
        }
        self.indicators = IndicatorInfo::from_factory(&self.factory);
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

    /// The studies the dialog lists, sorted by display name, including the feed's metrics.
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
        if let Some(error) = self.source_error {
            ui.colored_label(theme.ui.error, l10n::text(ui.ctx(), error));
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
                                    .color(theme.ui.text_muted),
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
                .any(|ind| Self::destination(ind).0 == category && self.accepts_source(ind))
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
                        && self.accepts_source(ind)
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
            TabBar::new(&tabs, &mut self.active_tab).pills().show(ui);
            ui.add_space(theme.spacing.xl);
        }
    }

    fn accepts_source(&self, ind: &IndicatorInfo) -> bool {
        self.source.is_none()
            || self
                .factory
                .create(&ind.id)
                .is_some_and(|study| study.source().is_some())
    }

    /// Whether `ind` belongs to the active category, and page for the built-ins, and matches
    /// the search.
    fn matches(&self, ctx: &egui::Context, ind: &IndicatorInfo) -> bool {
        if !self.accepts_source(ind) {
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
        let label = ui.painter().layout_no_wrap(
            l10n::text(ui.ctx(), "NAME"),
            FontId::proportional(theme.text.small),
            theme.ui.text_muted,
        );
        let label_pos = Pos2::new(
            header.min.x + DESIGN_TOKENS.sizing.button_md,
            header.center().y - label.size().y / 2.0,
        );
        ui.painter().galley(label_pos, label, theme.ui.text_muted);

        let mut action = IndicatorDialogAction::None;
        // The scroll bar takes its own column, so it never covers the badges.
        crate::styles::menu_scroll_bar(ui);
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
        assert!(dialog.source_error.is_some());
        assert!(dialog.is_open());
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
