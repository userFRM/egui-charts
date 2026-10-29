//! Compare symbols dialog.

use super::row::{SEARCH_RADIUS, paint_symbol_mark, search_backdrop};
use super::service::Search;
use crate::icons::icons;
use crate::localization as l10n;
use crate::model::Instrument;
use crate::services::{Storage, SymbolSearch};
use crate::theme::ThemeExt;
use crate::tokens::DESIGN_TOKENS;
use crate::ui_kit::EmptyState;
use crate::ui_kit::buttons::IconButton;
use crate::ui_kit::ext::UiExt;
use crate::ui_kit::modal::Modal;
use egui::{Color32, Pos2, RichText, Sense, Ui, Vec2};
use std::sync::Arc;

// =============================================================================
// Compare Symbols Dialog
// =============================================================================

/// Instrument information for comparison
#[derive(Debug, Clone)]
pub struct CompareSymbol {
    /// Instrument ticker
    pub symbol: String,
    /// Full name/desc
    pub name: String,
    /// Exchange name
    pub exchange: String,
    /// Color for this symbol on chart
    pub color: Color32,
    /// Whether this symbol is visible on chart
    pub visible: bool,
}

impl CompareSymbol {
    /// Create a new compare symbol
    pub fn new(
        symbol: impl Into<String>,
        name: impl Into<String>,
        exchange: impl Into<String>,
        color: Color32,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            name: name.into(),
            exchange: exchange.into(),
            color,
            visible: true,
        }
    }
}

/// Configuration for compare symbols dialog
#[derive(Debug, Clone)]
pub struct CompareSymbolsConfig {
    /// Dialog width
    pub width: f32,
    /// Dialog height
    pub height: f32,
    /// Background color
    pub bg_color: Color32,
    /// Text color
    pub text_color: Color32,
    /// Muted/secondary text color
    pub muted_color: Color32,
    /// Hover background color
    pub hover_color: Color32,
}

impl Default for CompareSymbolsConfig {
    fn default() -> Self {
        // Colors are TRANSPARENT to signal that colors should be fetched from
        // ui.style().visuals at render time for proper theme support.
        Self {
            width: 840.0,
            height: 560.0,
            bg_color: Color32::TRANSPARENT,
            text_color: Color32::TRANSPARENT,
            muted_color: Color32::TRANSPARENT,
            hover_color: Color32::TRANSPARENT,
        }
    }
}

impl CompareSymbolsConfig {
    /// The configured colors, each `TRANSPARENT` one taken from `visuals`.
    pub(super) fn resolved(&self, visuals: &egui::Visuals) -> Self {
        let or = |color: Color32, theme: Color32| {
            if color == Color32::TRANSPARENT {
                theme
            } else {
                color
            }
        };
        Self {
            bg_color: or(self.bg_color, visuals.panel_fill),
            text_color: or(self.text_color, visuals.text_color()),
            muted_color: or(self.muted_color, visuals.weak_text_color()),
            hover_color: or(self.hover_color, visuals.widgets.hovered.weak_bg_fill),
            ..self.clone()
        }
    }
}

/// Where a symbol added from the compare dialog goes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ComparePlacement {
    /// On the main price scale, which then shows every series as a percentage change.
    #[default]
    SamePercentScale,
    /// On a price scale of its own, beside the main one.
    NewPriceScale,
    /// In a pane of its own.
    NewPane,
}

impl ComparePlacement {
    /// Every placement, in the order the dialog offers them.
    const ALL: [Self; 3] = [Self::SamePercentScale, Self::NewPriceScale, Self::NewPane];

    /// The label of the placement's button.
    fn label(self) -> &'static str {
        match self {
            Self::SamePercentScale => "Same % scale",
            Self::NewPriceScale => "New price scale",
            Self::NewPane => "New pane",
        }
    }
}

/// Action from compare symbols dialog
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum CompareAction {
    /// No action
    None,
    /// Add the symbol to the comparison, placed as the user chose. A click on the row itself
    /// chooses [`ComparePlacement::SamePercentScale`].
    AddSymbol(String, ComparePlacement),
    /// Remove symbol from comparison
    RemoveSymbol(String),
    /// Toggle symbol visibility
    ToggleVisibility(String),
    /// Dialog closed
    Close,
}

/// Compare symbols dialog
///
/// Allows comparing multiple symbols on the same chart with:
/// - Search for symbols to add
/// - Suggestions from the symbols the host passes in, with an empty state when there are none
/// - Color-coded symbols
/// - Visibility toggles
///
/// # Example
///
/// ```no_run
/// use egui_charts::model::Instrument;
/// use egui_charts::ui::top_toolbar::{CompareAction, CompareSymbolsDialog};
///
/// # fn update(ctx: &egui::Context) {
/// let mut compare = CompareSymbolsDialog::new();
/// compare.open();
///
/// let symbols = vec![Instrument::new("ETHUSDT").with_description("Ethereum / Tether")];
/// match compare.show(ctx, &symbols) {
///     CompareAction::AddSymbol(sym, placement) => {
///         // Add the symbol to the chart where `placement` says
///     }
///     CompareAction::RemoveSymbol(sym) => {
///         // Remove symbol from chart
///     }
///     _ => {}
/// }
/// # }
/// ```
pub struct CompareSymbolsDialog {
    /// Is dialog open
    pub is_open: bool,
    /// Search query
    pub(super) search_query: String,
    /// Current comparison series on the host's chart. Update after accepting an intent,
    /// including changes made outside this dialog.
    pub compared_symbols: Vec<CompareSymbol>,
    /// Configuration
    pub(super) config: CompareSymbolsConfig,
    search: Search,
}

impl Default for CompareSymbolsDialog {
    fn default() -> Self {
        Self::new()
    }
}

impl CompareSymbolsDialog {
    /// Create a new compare symbols dialog
    pub fn new() -> Self {
        Self {
            is_open: false,
            search_query: String::new(),
            compared_symbols: Vec::new(),
            config: CompareSymbolsConfig::default(),
            search: Search::default(),
        }
    }

    /// Connects the same host service used by symbol search.
    pub fn set_search_service(&mut self, service: Option<Arc<dyn SymbolSearch>>) {
        self.search.set_service(service);
    }

    /// Lists the instruments symbol search recently opened, from the same host storage, while
    /// the query is empty.
    pub fn set_storage(&mut self, storage: Arc<dyn Storage>) {
        self.search.recents.set_storage(storage);
    }

    /// Create with custom configuration
    pub fn with_config(mut self, config: CompareSymbolsConfig) -> Self {
        self.config = config;
        self
    }

    /// Open the dialog
    pub fn open(&mut self) {
        self.is_open = true;
        self.search_query.clear();
        self.search.reset();
        // Symbol search writes the recent list through its own store.
        self.search.recents.reload();
    }

    /// Close the dialog
    pub fn close(&mut self) {
        self.is_open = false;
    }

    /// Show the dialog, suggesting `symbols`: the symbols the host's data source offers
    pub fn show(&mut self, ctx: &egui::Context, symbols: &[Instrument]) -> CompareAction {
        let mut action = CompareAction::None;
        self.search.poll(ctx);

        if !self.is_open {
            return action;
        }

        let mut is_open = self.is_open;
        let config = self.config.resolved(&ctx.global_style().visuals);
        Modal::new("compare_symbols", "")
            .width(config.width)
            .height(config.height)
            .fill(config.bg_color)
            .rounding(SEARCH_RADIUS)
            .backdrop(search_backdrop(ctx))
            .show(ctx, &mut is_open, |ui| {
                let translated_label = l10n::text(ui.ctx(), "Compare symbols");
                ui.response().widget_info(|| {
                    egui::WidgetInfo::labeled(egui::Role::Dialog, true, &translated_label)
                });
                ui.ctx()
                    .accesskit_node_builder(ui.response().id, |node| node.set_modal());
                action = self.draw_content(ui, symbols, &config);
            });

        self.is_open = is_open && action != CompareAction::Close;
        if !self.is_open {
            action = CompareAction::Close;
        }

        action
    }

    pub(super) fn draw_content(
        &mut self,
        ui: &mut Ui,
        symbols: &[Instrument],
        config: &CompareSymbolsConfig,
    ) -> CompareAction {
        let mut action = CompareAction::None;

        ui.horizontal(|ui| {
            ui.add(icons::QUICK_SEARCH.as_image_tinted(Vec2::splat(24.0), config.muted_color));
            ui.add(
                egui::TextEdit::singleline(&mut self.search_query)
                    .hint_text(RichText::new(l10n::text(ui.ctx(), "Search symbols")).size(16.0))
                    .font(egui::FontId::proportional(16.0))
                    .frame(egui::Frame::NONE)
                    .desired_width((ui.available_width() - 40.0).max(0.0)),
            )
            .accessible_name(l10n::text(ui.ctx(), "Search symbols"));
            if IconButton::new(&icons::CLOSE)
                .tooltip("Close")
                .show(ui)
                .clicked()
            {
                action = CompareAction::Close;
            }
        });
        ui.add_space(24.0);

        // Currently compared symbols
        if !self.compared_symbols.is_empty() {
            ui.label(
                RichText::new(l10n::text(ui.ctx(), "Currently comparing:"))
                    .color(config.muted_color),
            );
            ui.space_sm();

            egui::ScrollArea::vertical()
                .id_salt("current_comparisons")
                .max_height(ui.available_height() * 0.4)
                .show(ui, |ui| {
                    for symbol in &self.compared_symbols {
                        ui.horizontal(|ui| {
                            // Color indicator
                            let (rect, _) = ui.allocate_exact_size(
                                Vec2::splat(DESIGN_TOKENS.sizing.icon_xs),
                                Sense::hover(),
                            );
                            ui.painter().circle_filled(rect.center(), 5.0, symbol.color);

                            ui.label(l10n::text(ui.ctx(), &symbol.symbol));
                            ui.label(
                                RichText::new(&symbol.name)
                                    .color(config.muted_color)
                                    .size(ui.active_theme().text.small),
                            );

                            ui.right_aligned(|ui| {
                                if ui.small_button(l10n::text(ui.ctx(), "X")).clicked() {
                                    action = CompareAction::RemoveSymbol(symbol.symbol.clone());
                                }

                                let eye_label = if symbol.visible { "Hide" } else { "Show" };
                                if ui.small_button(l10n::text(ui.ctx(), eye_label)).clicked() {
                                    action = CompareAction::ToggleVisibility(symbol.symbol.clone());
                                }
                            });
                        });
                    }
                });
            ui.separator_with_margin(ui.active_theme().spacing.lg);
        }

        self.search.query(ui.ctx(), &self.search_query, None, 20);
        let recent = self.search_query.is_empty() && self.search.has_recents();
        let found = if recent {
            self.search.recent_results()
        } else {
            self.search.results()
        };
        let symbols = if self.search.connected() || recent {
            found.as_slice()
        } else {
            symbols
        };
        self.search.status(ui);
        let mut next_page = false;
        // Suggestions
        let heading = if recent { "Recent symbols" } else { "Symbols" };
        ui.label(RichText::new(l10n::text(ui.ctx(), heading)).color(config.muted_color));
        ui.space_lg();

        if symbols.is_empty() {
            EmptyState::new("No symbols to compare").show(ui);
        }

        egui::ScrollArea::vertical()
            .id_salt("comparison_suggestions")
            .max_height(ui.available_height())
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                let query = self.search_query.to_lowercase();
                for symbol in symbols.iter().filter(|symbol| {
                    self.search.connected()
                        || symbol.symbol.to_lowercase().contains(&query)
                        || symbol
                            .description
                            .as_deref()
                            .unwrap_or(&symbol.symbol)
                            .to_lowercase()
                            .contains(&query)
                }) {
                    next_page |= ui.clip_rect().contains(ui.cursor().min)
                        && symbols
                            .last()
                            .is_some_and(|last| last.symbol == symbol.symbol);
                    if let Some(a) = self.draw_symbol_item(ui, symbol, config) {
                        action = a;
                    }
                }
            });

        if next_page {
            self.search.next_page();
        }
        action
    }

    fn draw_symbol_item(
        &self,
        ui: &mut Ui,
        symbol: &Instrument,
        config: &CompareSymbolsConfig,
    ) -> Option<CompareAction> {
        let theme = ui.active_theme();
        let is_compared = self
            .compared_symbols
            .iter()
            .any(|s| s.symbol == symbol.symbol);

        let desired_size = Vec2::new(ui.available_width(), 56.0);
        let (rect, response) = ui.allocate_exact_size(desired_size, Sense::click());
        response.widget_info(|| {
            egui::WidgetInfo::labeled(
                egui::Role::ListBoxOption,
                response.enabled() && !is_compared,
                &symbol.symbol,
            )
        });
        ui.ctx()
            .accesskit_node_builder(response.id, |node| node.set_selected(is_compared));
        crate::styles::focus::draw_focus_ring(ui, &response, theme.radius.md);

        // The placement buttons follow the pointer or the keyboard focus into the row.
        let placement_ids = ComparePlacement::ALL.map(|placement| response.id.with(placement));
        let menu_id = response.id.with("placements");
        let engaged = response.contains_pointer()
            || response.has_focus()
            || egui::Popup::is_id_open(ui.ctx(), menu_id.with("popup"))
            || ui.memory(|m| m.has_focus(menu_id))
            || ui.memory(|m| placement_ids.iter().any(|id| m.has_focus(*id)));
        if engaged && !is_compared {
            ui.painter()
                .rect_filled(rect, theme.radius.md, config.hover_color);
        }

        // Every row reserves the mark, so text lines up whatever the host supplies.
        let mark = egui::Rect::from_center_size(
            Pos2::new(rect.min.x + 4.0 + 18.0, rect.center().y),
            Vec2::splat(36.0),
        );
        paint_symbol_mark(ui, symbol, mark);
        let left = mark.right() + 8.0;

        // Right-hand column, from the edge: country, then the comparison check.
        let mut right = rect.max.x - 12.0;
        if let Some(country) = &symbol.country {
            let galley = ui.painter().layout_no_wrap(
                country.to_uppercase(),
                egui::FontId::proportional(theme.text.small),
                config.muted_color,
            );
            let pos = Pos2::new(
                right - galley.size().x,
                rect.center().y - galley.size().y / 2.0,
            );
            right = pos.x - 12.0;
            ui.painter().galley(pos, galley, config.muted_color);
        }
        if is_compared {
            let center = Pos2::new(right - 8.0, rect.center().y);
            ui.painter().circle_filled(center, 8.0, theme.ui.success);
            ui.painter().text(
                center,
                egui::Align2::CENTER_CENTER,
                l10n::text(ui.ctx(), "v"),
                egui::FontId::proportional(theme.text.micro),
                theme.ui.panel_bg,
            );
            right = center.x - 20.0;
        }

        let mut action = None;
        if engaged && !is_compared {
            let placements = ComparePlacement::ALL.map(|placement| {
                let label = ui.painter().layout_no_wrap(
                    l10n::text(ui.ctx(), placement.label()),
                    egui::FontId::proportional(theme.text.small),
                    config.text_color,
                );
                (placement, label)
            });
            let buttons_width: f32 = placements
                .iter()
                .map(|(_, label)| label.size().x + 20.0)
                .sum();
            if right - left < buttons_width + 80.0 {
                let button = egui::Rect::from_center_size(
                    Pos2::new(right - 20.0, rect.center().y),
                    Vec2::new(40.0, 24.0),
                );
                right = button.left() - 4.0;
                let menu = ui.interact(button, menu_id, Sense::click());
                let label = l10n::text(ui.ctx(), "Add");
                menu.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
                let visuals = ui.style().interact(&menu);
                ui.painter().rect(
                    button,
                    12.0,
                    visuals.weak_bg_fill,
                    visuals.bg_stroke,
                    egui::StrokeKind::Inside,
                );
                ui.painter().text(
                    button.center(),
                    egui::Align2::CENTER_CENTER,
                    label,
                    egui::FontId::proportional(theme.text.small),
                    config.text_color,
                );
                crate::styles::focus::draw_focus_ring(ui, &menu, 12.0);
                egui::Popup::menu(&menu).show(|ui| {
                    for placement in ComparePlacement::ALL {
                        if ui.button(l10n::text(ui.ctx(), placement.label())).clicked() {
                            action =
                                Some(CompareAction::AddSymbol(symbol.symbol.clone(), placement));
                            ui.close();
                        }
                    }
                });
            } else {
                for ((placement, galley), id) in placements.into_iter().zip(placement_ids).rev() {
                    let button = egui::Rect::from_min_max(
                        Pos2::new(right - galley.size().x - 16.0, rect.center().y - 12.0),
                        Pos2::new(right, rect.center().y + 12.0),
                    );
                    right = button.left() - 4.0;
                    let clicked = ui.interact(button, id, Sense::click());
                    clicked.widget_info(|| {
                        egui::WidgetInfo::labeled(egui::Role::Button, true, galley.text())
                    });
                    let fill = if clicked.hovered() {
                        theme.ui.btn_bg_hover
                    } else {
                        config.bg_color
                    };
                    ui.painter().rect(
                        button,
                        button.height() / 2.0,
                        fill,
                        ui.visuals().widgets.inactive.bg_stroke,
                        egui::StrokeKind::Inside,
                    );
                    crate::styles::focus::draw_focus_ring(ui, &clicked, button.height() / 2.0);
                    ui.painter().galley(
                        button.center() - galley.size() / 2.0,
                        galley,
                        config.text_color,
                    );
                    if clicked.clicked() {
                        action = Some(CompareAction::AddSymbol(symbol.symbol.clone(), placement));
                    }
                }
            }
        }

        // Instrument, then its description and type badge
        let width = (right - 12.0 - left).max(0.0);
        let line = |text: &str, size: f32, color: Color32, width: f32| {
            egui::WidgetText::from(RichText::new(text).size(size).color(color)).into_galley(
                ui,
                Some(egui::TextWrapMode::Truncate),
                width,
                egui::TextStyle::Body,
            )
        };
        let name = line(&symbol.symbol, 16.0, config.text_color, width);
        ui.painter().galley(
            Pos2::new(left, rect.center().y - 1.0 - name.size().y),
            name,
            config.text_color,
        );
        let kind = symbol
            .symbol_type
            .map(|kind| l10n::text(ui.ctx(), kind.to_string()).to_uppercase());
        let badge = kind.map(|kind| {
            ui.painter().layout_no_wrap(
                kind,
                egui::FontId::proportional(theme.text.micro),
                config.muted_color,
            )
        });
        let badge_width = badge.as_ref().map_or(0.0, |badge| badge.size().x + 14.0);
        let description = line(
            symbol.description.as_deref().unwrap_or(&symbol.symbol),
            12.0,
            config.muted_color,
            (width - badge_width).max(0.0),
        );
        let top = rect.center().y + 3.0;
        let badge_left = left + description.size().x + 6.0;
        let description_height = description.size().y;
        ui.painter()
            .galley(Pos2::new(left, top), description, config.muted_color);
        if let Some(badge) = badge {
            let frame = egui::Rect::from_min_size(
                Pos2::new(
                    badge_left,
                    top + (description_height - badge.size().y - 4.0) / 2.0,
                ),
                badge.size() + Vec2::new(8.0, 4.0),
            );
            ui.painter()
                .rect_filled(frame, theme.radius.xs, theme.ui.btn_bg_hover);
            ui.painter()
                .galley(frame.min + Vec2::new(4.0, 2.0), badge, config.muted_color);
        }

        if action.is_none() && response.clicked() && !is_compared {
            action = Some(CompareAction::AddSymbol(
                symbol.symbol.clone(),
                ComparePlacement::SamePercentScale,
            ));
        }
        action
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::dialogs::symbol_search::tests::create_test_symbols;
    use egui::Context;

    #[test]
    fn compare_recents_do_not_require_a_search_service() {
        let symbols = create_test_symbols();
        for connected in [false, true] {
            let ctx = Context::default();
            let mut dialog = CompareSymbolsDialog::new();
            if connected {
                dialog.set_search_service(Some(Arc::new(symbols.clone())));
            }
            dialog.search.record(&symbols[0]);
            dialog.search.poll(&ctx);
            let config = dialog.config.resolved(&ctx.global_style().visuals);
            for (query, heading, listed, absent) in [
                ("", "Recent symbols", "BTCUSDT", "TSLA"),
                ("tsla", "Symbols", "TSLA", "BTCUSDT"),
            ] {
                dialog.search_query = query.into();
                for frame in 0..2 {
                    let output = ctx.run_ui(Default::default(), |ui| {
                        dialog.draw_content(ui, &symbols, &config);
                    });
                    let texts: Vec<_> = output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) => Some(text.galley.text()),
                            _ => None,
                        })
                        .collect();
                    if frame > 0 {
                        assert!(texts.contains(&heading), "connected {connected}: {texts:?}");
                        assert!(texts.contains(&listed));
                        assert!(!texts.contains(&absent));
                    }
                    output.drop_without_applying_deltas();
                }
            }
        }
    }

    #[test]
    fn comparison_actions_wait_for_host_acceptance() {
        let mut dialog = CompareSymbolsDialog::new();
        assert!(!dialog.is_open);

        dialog.open();
        assert!(dialog.is_open);

        dialog.close();
        assert!(!dialog.is_open);
        let symbols = create_test_symbols();
        let add = |placement| CompareAction::AddSymbol("AAPL".into(), placement);
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for width in [1440.0, 360.0] {
                for (hover, label, expected, populated) in [
                    (None, "AAPL", add(ComparePlacement::SamePercentScale), false),
                    (
                        Some("AAPL"),
                        "Same % scale",
                        add(ComparePlacement::SamePercentScale),
                        false,
                    ),
                    (
                        Some("AAPL"),
                        "New price scale",
                        add(ComparePlacement::NewPriceScale),
                        false,
                    ),
                    (
                        Some("AAPL"),
                        "New pane",
                        add(ComparePlacement::NewPane),
                        false,
                    ),
                    (None, "X", CompareAction::RemoveSymbol("AAPL".into()), true),
                    (
                        None,
                        "Hide",
                        CompareAction::ToggleVisibility("AAPL".into()),
                        true,
                    ),
                ] {
                    let ctx = Context::default();
                    crate::theme::apply_to_egui(&ctx, &theme);
                    let mut dialog = CompareSymbolsDialog::new();
                    if populated {
                        dialog.compared_symbols.push(CompareSymbol::new(
                            "AAPL",
                            "Apple",
                            "Test",
                            Color32::GOLD,
                        ));
                    }
                    let config = dialog.config.resolved(&ctx.global_style().visuals);
                    let mut frame = |events, label| {
                        let mut action = CompareAction::None;
                        let output = ctx.run_ui(
                            egui::RawInput {
                                screen_rect: Some(egui::Rect::from_min_size(
                                    Pos2::ZERO,
                                    Vec2::new(width, 640.0),
                                )),
                                events,
                                ..Default::default()
                            },
                            |ui| {
                                action = dialog.draw_content(ui, &symbols, &config);
                            },
                        );
                        let pos = output.shapes.iter().find_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) if text.galley.text() == label => {
                                let rect = egui::Rect::from_min_size(text.pos, text.galley.size());
                                assert!(!text.galley.elided, "{label} is unreadable at {width}");
                                assert!(
                                    shape.clip_rect.contains_rect(rect),
                                    "{label} is clipped at {width}"
                                );
                                Some(rect.center())
                            }
                            _ => None,
                        });
                        output.drop_without_applying_deltas();
                        (action, pos)
                    };
                    frame(vec![], label);
                    if let Some(row) = hover {
                        let pos = frame(vec![], row).1.unwrap();
                        frame(vec![egui::Event::PointerMoved(pos)], row);
                    }
                    if hover.is_some()
                        && let Some(pos) = frame(vec![], "Add").1
                    {
                        frame(vec![egui::Event::PointerMoved(pos)], "Add");
                        for pressed in [true, false] {
                            frame(
                                vec![egui::Event::PointerButton {
                                    pos,
                                    button: egui::PointerButton::Primary,
                                    pressed,
                                    modifiers: Default::default(),
                                }],
                                "Add",
                            );
                        }
                    }
                    let pos = frame(vec![], label).1.unwrap();
                    frame(vec![egui::Event::PointerMoved(pos)], label);
                    let button = |pressed| egui::Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    };
                    frame(vec![button(true)], label);
                    assert_eq!(frame(vec![button(false)], label).0, expected);
                    assert_eq!(
                        dialog.compared_symbols.len(),
                        usize::from(populated),
                        "only the host updates chart membership"
                    );
                    if populated {
                        assert!(
                            dialog.compared_symbols[0].visible,
                            "only the host updates chart visibility"
                        );
                    }
                }
            }
        }
    }
}
