//! Top Toolbar Widget
//!
//! Symbol search, interval, chart type, indicators, alert, replay, undo and redo
//! on the left; the chart layout, layout name, save, quick search, settings,
//! fullscreen, screenshot, publish and trade on the right, then the host's call to
//! action, in a horizontally scrolling strip after the fixed main menu.

use crate::icons::{Icon, icons as embedded_icons};
use crate::localization as l10n;
use crate::model::Timeframe;
use crate::theme::ThemeExt;
use crate::ui_kit::buttons::IconButton;
use crate::ui_kit::list_item::{LabelContent, ListItem};
use egui::{Role, Ui, WidgetInfo};

use super::{
    actions::TopToolbarAction,
    buttons::{IconTextButton, PillButton, TextButton, TextIconButton, ToolbarIconButton, Trigger},
    components::{
        chart_grid_selector::ChartGridSelector,
        chart_type_selector::{ChartTypeAction, ChartTypeSelector},
        layout_menu::{LayoutAction, LayoutMenu},
        timeframe_selector::{TimeframeAction, TimeframeSelector},
    },
    config::TopToolbarConfig,
    state::TopToolbarState,
};
use crate::model::ChartType;
use crate::tokens::DESIGN_TOKENS;
use crate::ui::Control;
use crate::ui::dialogs::keyboard_shortcuts::shortcut_key_labels;
use std::collections::HashSet;

/// A place in the strip, holding one control.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Slot {
    Symbol,
    Compare,
    Interval,
    ChartType,
    Indicators,
    Templates,
    Alert,
    Replay,
    Undo,
    Redo,
    Theme,
    Layouts,
    LayoutName,
    Save,
    QuickSearch,
    Settings,
    Fullscreen,
    Screenshot,
    Publish,
    Trade,
}

use Slot as S;

/// Glyph of the snapshot menu's download entry.
const DOWNLOAD_ICON: Icon = Icon::new("top-toolbar-download.svg", include_bytes!("download.svg"));
/// Glyph of the snapshot menu's new tab entry.
const OPEN_NEW_ICON: Icon = Icon::new("top-toolbar-open-new.svg", include_bytes!("open-new.svg"));
/// Glyph of the snapshot menu's link entry.
const LINK_ICON: Icon = Icon::new("top-toolbar-link.svg", include_bytes!("link.svg"));

/// Left part of the strip, in groups divided by separators.
const LEFT: &[&[Slot]] = &[
    &[S::Symbol, S::Compare],
    &[S::Interval],
    &[S::ChartType],
    &[S::Indicators, S::Templates],
    &[S::Alert, S::Replay],
    &[S::Undo, S::Redo],
];

/// Right-aligned part of the strip, in groups divided by separators.
const RIGHT: &[&[Slot]] = &[
    &[S::Layouts, S::LayoutName, S::Save],
    &[
        S::QuickSearch,
        S::Settings,
        S::Fullscreen,
        S::Screenshot,
        S::Publish,
        S::Trade,
    ],
];

/// Space between the controls of a group.
fn gap(ui: &Ui) -> f32 {
    ui.active_theme().spacing.sm
}

/// Width of the group rule with its padding.
fn separator(ui: &Ui) -> f32 {
    2.0 * gap(ui) + DESIGN_TOKENS.sizing.toolbar.separator_width
}

/// Neutral group rule, centred in the strip.
fn show_separator(ui: &mut Ui) {
    ui.add_space(gap(ui));
    let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, 22.0), egui::Sense::hover());
    let color = super::buttons::rule_color(&ui.active_theme());
    ui.painter().rect_filled(rect, 0.0, color);
    ui.add_space(gap(ui));
}

/// Keys of the snapshot shortcut as the snapshot menu shows them.
fn snapshot_shortcut_hint(ctx: &egui::Context, font: &egui::FontId) -> Option<String> {
    let shortcut = ctx
        .data(|data| {
            data.get_temp::<crate::widget::ShortcutMap>(egui::Id::NULL.with("chart_shortcuts"))
        })
        .unwrap_or_default()
        .shortcut(crate::config::KeyboardAction::Snapshot)?;
    let keys: Vec<_> = shortcut_key_labels(ctx, font, &shortcut)
        .iter()
        .map(|key| l10n::text(ctx, key))
        .collect();
    Some(keys.join(" + "))
}

impl Slot {
    /// Icon, name and action of a control that acts on a click; `None` for the dropdowns.
    fn entry(self) -> Option<(&'static Icon, &'static str, TopToolbarAction)> {
        use TopToolbarAction as A;
        Some(match self {
            S::Symbol => (
                &embedded_icons::QUICK_SEARCH,
                "Symbol search",
                A::OpenSymbolSearch,
            ),
            S::Compare => (
                &embedded_icons::COMPARE_ADD,
                "Compare symbols",
                A::OpenCompare,
            ),
            S::Indicators => (&embedded_icons::INDICATORS, "Indicators", A::OpenIndicators),
            S::Templates => (&embedded_icons::LAYOUT_GRID, "Indicator templates", A::None),
            S::Alert => (&embedded_icons::BELL, "Alert", A::CreateAlert),
            S::Replay => (&embedded_icons::REPLAY, "Replay", A::ToggleReplay),
            S::Undo => (&embedded_icons::UNDO, "Undo", A::Undo),
            S::Redo => (&embedded_icons::REDO, "Redo", A::Redo),
            S::Theme => (
                &embedded_icons::THEME_TOGGLE,
                "Toggle Theme",
                A::ThemeToggle,
            ),
            S::Save => (&embedded_icons::DATABASE, "Save chart", A::Save),
            S::QuickSearch => (
                &embedded_icons::COMMAND_SEARCH,
                "Quick Search",
                A::OpenQuickSearch,
            ),
            S::Settings => (&embedded_icons::SETTINGS_NUT, "Settings", A::OpenSettings),
            S::Fullscreen => (
                &embedded_icons::FULLSCREEN,
                "Fullscreen",
                A::ToggleFullscreen,
            ),
            S::Screenshot => (&embedded_icons::CAMERA, "Snapshot", A::TakeScreenshot),
            S::Publish => (&embedded_icons::PUBLISH, "Publish", A::Publish),
            S::Interval | S::ChartType | S::Layouts | S::LayoutName | S::Trade => return None,
        })
    }

    /// The host control that hides this slot.
    fn host_control(self) -> Control {
        match self {
            S::Compare => Control::Compare,
            S::Templates => Control::Templates,
            S::Alert => Control::Alerts,
            S::Replay => Control::Replay,
            S::Save | S::Layouts | S::LayoutName => Control::Layouts,
            S::Symbol => Control::SymbolSearch,
            S::Interval => Control::Interval,
            S::ChartType => Control::ChartType,
            S::Indicators => Control::Indicators,
            S::Undo => Control::Undo,
            S::Redo => Control::Redo,
            S::Theme => Control::Theme,
            S::QuickSearch => Control::QuickSearch,
            S::Settings => Control::Settings,
            S::Fullscreen => Control::Fullscreen,
            S::Screenshot => Control::Snapshot,
            S::Publish => Control::Publish,
            S::Trade => Control::Trading,
        }
    }
}

/// A page of the indicator templates menu.
#[derive(Clone, Copy, PartialEq, Default)]
enum TemplatesPage {
    #[default]
    Indicators,
    Templates,
}

impl TemplatesPage {
    fn label(self) -> &'static str {
        match self {
            Self::Indicators => "Indicators",
            Self::Templates => "Templates",
        }
    }
}

/// Top toolbar widget
pub struct TopToolbar {
    /// Display and layout options for this component.
    pub config: TopToolbarConfig,
    /// Mutable interaction state of this component.
    pub state: TopToolbarState,
    /// Timeframe selector dropdown
    pub timeframe_selector: TimeframeSelector,
    /// Chart type selector dropdown
    pub chart_type_selector: ChartTypeSelector,
    /// Chart preset and synchronization menu.
    pub chart_grid_selector: ChartGridSelector,
    /// Layout selector dropdown
    pub layout_menu: LayoutMenu,
    /// Controls the host hid.
    host_hidden: HashSet<Control>,
    /// Host-supplied saved indicator sets as (id, name), in display order.
    pub indicator_templates: Vec<(String, String)>,
    /// Host-supplied favourite studies as (factory name, display name), in display order,
    /// such as the starred studies of an
    /// [`IndicatorDialog`](super::components::IndicatorDialog).
    pub favorite_indicators: Vec<(String, String)>,
    templates_page: TemplatesPage,
    /// Host buttons and dropdowns in registration order, placed before the host action slot.
    pub host_entries: Vec<super::ToolbarEntry>,
    /// Host call to action as (id, label), drawn as a filled pill at the right end after the host
    /// action slot. Choosing it emits [`TopToolbarAction::Host`] with the id; `None` shows nothing.
    pub call_to_action: Option<(String, String)>,
}

impl Default for TopToolbar {
    fn default() -> Self {
        Self {
            config: TopToolbarConfig::default(),
            state: TopToolbarState::default(),
            timeframe_selector: TimeframeSelector::new(Timeframe::Day1),
            chart_type_selector: ChartTypeSelector::new(ChartType::Candles),
            layout_menu: LayoutMenu::new(),
            chart_grid_selector: ChartGridSelector::default(),
            host_hidden: HashSet::new(),
            indicator_templates: Vec::new(),
            favorite_indicators: Vec::new(),
            templates_page: TemplatesPage::default(),
            host_entries: Vec::new(),
            call_to_action: None,
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

    /// Shows or hides the named top-toolbar control, including its popup.
    /// Controls for other components are ignored. This controls UI visibility; configure chart
    /// behavior and shortcut bindings separately through `ChartOptions` and `ShortcutMap`.
    pub fn set_control_visible(&mut self, control: Control, visible: bool) {
        crate::ui::app_state::set_control_visible(&mut self.host_hidden, control, visible);
    }

    /// Whether the host left `slot` visible. Trade also needs a broker.
    fn is_shown(&self, slot: Slot) -> bool {
        !self.host_hidden.contains(&slot.host_control())
            && (slot != S::Trade || self.state.can_trade)
            && (slot != S::Publish || self.state.can_publish_image)
    }

    /// Show the full toolbar, reading the active symbol from app state.
    pub fn show_with_state(
        &mut self,
        ui: &mut Ui,
        app_state: &dyn crate::ui::app_state::ChartAppState,
    ) -> TopToolbarAction {
        let mut menu_action = TopToolbarAction::None;
        let menu_visible = [S::QuickSearch, S::Theme].map(|slot| self.is_shown(slot));
        let action = self.show_with_slots(
            ui,
            app_state,
            |ui| Self::show_menu(ui, &mut menu_action, menu_visible),
            |_| {},
        );
        if menu_action != TopToolbarAction::None {
            menu_action
        } else {
            action
        }
    }

    /// Show the toolbar inline (same layout, no outer frame).
    pub fn show_inline(
        &mut self,
        ui: &mut Ui,
        app_state: &dyn crate::ui::app_state::ChartAppState,
    ) -> TopToolbarAction {
        self.show_with_state(ui, app_state)
    }

    /// Show host menu content before the symbol and host actions at the far right.
    ///
    /// The menu slot is 48 px wide. Keep host actions compact; their measured width is
    /// reserved alongside the built-in controls. The closures
    /// handle their own responses and may open menus or return actions to the host.
    /// The strip after the menu slot scrolls horizontally when its controls cannot fit, without a
    /// visible scroll bar; the menu slot stays at the left edge.
    pub fn show_with_slots(
        &mut self,
        ui: &mut Ui,
        app_state: &dyn crate::ui::app_state::ChartAppState,
        menu: impl FnOnce(&mut Ui),
        actions: impl FnOnce(&mut Ui),
    ) -> TopToolbarAction {
        ui.horizontal_top(|ui| {
            if !self.host_hidden.contains(&Control::Menu) {
                // The strip follows the slot directly but keeps the default spacing, which it
                // uses as the margin when scrolling a focused control into view.
                let spacing = std::mem::take(&mut ui.spacing_mut().item_spacing.x);
                ui.allocate_ui_with_layout(
                    egui::vec2(48.0, self.config.height),
                    egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                    menu,
                );
                ui.spacing_mut().item_spacing.x = spacing;
            }
            let width = ui.available_width();
            egui::ScrollArea::horizontal()
                .id_salt("top_toolbar")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .scroll_source(egui::scroll_area::ScrollSource::ALL)
                .auto_shrink([false, true])
                .show(ui, |ui| {
                    ui.set_width(width);
                    let row = ui.horizontal(|ui| {
                        ui.set_height(self.config.height);
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let width_id = ui.scope_id().with("host_actions_width");
                        let actions_width = ui.data(|d| d.get_temp::<f32>(width_id)).unwrap_or(0.0);
                        let action = ui
                            .allocate_ui_with_layout(
                                egui::vec2(
                                    (ui.available_width() - actions_width).max(0.0),
                                    self.config.height,
                                ),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| self.show_contents_desktop(ui, app_state.active_symbol()),
                            )
                            .inner;
                        ui.add_space((ui.available_width() - actions_width).max(0.0));
                        let mut host_action = TopToolbarAction::None;
                        let width = ui
                            .allocate_ui_with_layout(
                                egui::vec2(ui.available_width(), self.config.height),
                                egui::Layout::left_to_right(egui::Align::Center),
                                |ui| {
                                    for entry in &self.host_entries {
                                        entry.show(ui, &mut host_action);
                                    }
                                    actions(ui);
                                    if let Some((id, label)) = &self.call_to_action {
                                        let response = PillButton {
                                            label,
                                            tooltip: None,
                                            filled: true,
                                        }
                                        .show(ui);
                                        if response.clicked() {
                                            host_action = TopToolbarAction::Host(id.clone());
                                        }
                                        ui.add_space(gap(ui));
                                    }
                                },
                            )
                            .response
                            .rect
                            .width();
                        if (width - actions_width).abs() > 0.5 {
                            ui.data_mut(|d| d.insert_temp(width_id, width));
                            ui.ctx().request_discard("Host actions changed width");
                        }
                        if host_action != TopToolbarAction::None {
                            host_action
                        } else {
                            action
                        }
                    });
                    if let Some(response) = ui
                        .ctx()
                        .memory(|m| m.focused())
                        .and_then(|id| ui.ctx().read_response(id))
                        && response.gained_focus()
                        && row.response.rect.contains_rect(response.rect)
                    {
                        response.scroll_to_me(None);
                    }
                    row.inner
                })
                .inner
        })
        .inner
    }

    fn show_menu(ui: &mut Ui, action: &mut TopToolbarAction, visible: [bool; 2]) {
        if !visible.into_iter().any(|shown| shown) {
            return;
        }
        let response = IconButton::new(&super::buttons::MENU_ICON)
            .icon_scale(24.0 / 28.0)
            .tooltip("Menu")
            .show(ui);
        egui::Popup::menu(&response).show(|ui| {
            ui.set_width(DESIGN_TOKENS.sizing.dialog.submenu_width);
            for (control, visible) in [S::QuickSearch, S::Theme].into_iter().zip(visible) {
                if !visible {
                    continue;
                }
                if let Some((icon, label, clicked_action)) = control.entry()
                    && ListItem::new().show_with_icon(ui, label, icon).clicked()
                {
                    *action = clicked_action;
                    ui.close();
                }
            }
        });
    }

    /// Desktop layout: horizontal toolbar contents.
    fn show_contents_desktop(&mut self, ui: &mut Ui, symbol: &str) -> TopToolbarAction {
        let mut action = TopToolbarAction::None;
        // Controls carry their own token gaps (`gap`, separators).
        ui.spacing_mut().item_spacing.x = 0.0;
        // Kit buttons label in egui's button style; every label of the strip has the title size.
        let font = egui::FontId::proportional(ui.active_theme().text.title);
        ui.style_mut()
            .text_styles
            .insert(egui::TextStyle::Button, font);

        ui.add_space(ui.active_theme().spacing.lg);
        self.show_groups(ui, LEFT, symbol, &mut action);
        let right = self.groups_width(ui, RIGHT) + ui.active_theme().spacing.lg;
        ui.add_space((ui.available_width() - right).max(separator(ui)));
        self.show_groups(ui, RIGHT, symbol, &mut action);
        ui.add_space(ui.active_theme().spacing.lg);
        action
    }

    /// Width of the right-hand controls with their gaps and separators.
    fn groups_width(&self, ui: &Ui, groups: &[&[Slot]]) -> f32 {
        let mut width = 0.0;
        let mut shown = 0_usize;
        for group in groups {
            let mut count = 0;
            for &control in group.iter().filter(|&&c| self.is_shown(c)) {
                width += match control {
                    S::LayoutName => self.layout_menu.width(ui),
                    S::Save => {
                        Self::save_button(&self.config).width(ui)
                            + Self::save_menu_button(&self.config).width(ui)
                    }
                    S::Trade => Self::trade_button().width(ui),
                    _ => self.config.btn_size,
                };
                count += 1;
            }
            if count > 0 {
                width += gap(ui) * (count - 1) as f32;
                shown += 1;
            }
        }
        width + separator(ui) * shown.saturating_sub(1) as f32
    }

    /// Draw the visible controls of `groups`, with separators between groups.
    fn show_groups(
        &mut self,
        ui: &mut Ui,
        groups: &[&[Slot]],
        symbol: &str,
        action: &mut TopToolbarAction,
    ) {
        let mut first_group = true;
        for group in groups {
            let visible: Vec<Slot> = group
                .iter()
                .copied()
                .filter(|&c| self.is_shown(c))
                .collect();
            if visible.is_empty() {
                continue;
            }
            if !first_group {
                show_separator(ui);
            }
            first_group = false;
            for (i, control) in visible.into_iter().enumerate() {
                if i > 0 {
                    ui.add_space(gap(ui));
                }
                self.show_control(ui, control, symbol, action);
            }
        }
    }

    fn show_control(
        &mut self,
        ui: &mut Ui,
        control: Slot,
        symbol: &str,
        action: &mut TopToolbarAction,
    ) {
        match control {
            S::Layouts => {
                if let Some(mode) = self.chart_grid_selector.show(ui) {
                    *action = TopToolbarAction::Layout(LayoutAction::SelectGrid(mode));
                }
                return;
            }
            S::LayoutName => {
                let layout_action = self.layout_menu.show(ui);
                if layout_action != LayoutAction::None {
                    *action = TopToolbarAction::Layout(layout_action);
                }
                return;
            }
            S::Interval => {
                if let TimeframeAction::Select(tf) = self.timeframe_selector.show(ui) {
                    *action = TopToolbarAction::IntervalSelected(tf);
                }
                return;
            }
            S::ChartType => {
                if let ChartTypeAction::Select(ct) = self.chart_type_selector.show(ui) {
                    *action = TopToolbarAction::ChartStyleSelected(ct);
                }
                return;
            }
            S::Trade => {
                if Self::trade_button().show(ui).clicked() {
                    *action = TopToolbarAction::ToggleTrading;
                }
                return;
            }
            _ => {}
        }
        let Some((icon, label, clicked_action)) = control.entry() else {
            return;
        };
        let response = match control {
            S::Symbol => Self::symbol_trigger(symbol).show(ui, false),
            S::Compare => ToolbarIconButton::new(icon, label, &self.config).show(ui),
            S::Templates => {
                let response = ToolbarIconButton::new(icon, label, &self.config).show(ui);
                egui::Popup::menu(&response)
                    // Choosing a page keeps the menu open; its rows close it themselves.
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .align(egui::RectAlign::BOTTOM_START)
                    .align_alternatives(&[])
                    .frame(egui::Frame::popup(ui.style()).fill(ui.active_theme().ui.panel_bg))
                    .show(|ui| self.show_indicator_templates(ui, action));
                return;
            }
            S::Save => {
                let response = Self::save_button(&self.config).show(ui);
                let arrow = Self::save_menu_button(&self.config).show(ui);
                egui::Popup::menu(&arrow)
                    .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
                    .align(egui::RectAlign::BOTTOM_START)
                    .align_alternatives(&[])
                    .frame(egui::Frame::popup(ui.style()).fill(ui.active_theme().ui.panel_bg))
                    .show(|ui| {
                        let layout_action = self.layout_menu.show_menu(ui);
                        if layout_action != LayoutAction::None {
                            *action = TopToolbarAction::Layout(layout_action);
                        }
                    });
                if arrow.clicked() {
                    return;
                }
                response
            }

            S::Screenshot => {
                let response = ToolbarIconButton::new(icon, label, &self.config).show(ui);
                egui::Popup::menu(&response)
                    .align(egui::RectAlign::BOTTOM_START)
                    .align_alternatives(&[])
                    .frame(egui::Frame::popup(ui.style()).fill(ui.active_theme().ui.panel_bg))
                    .show(|ui| {
                        ui.set_width((ui.ctx().content_rect().width() - 24.0).clamp(0.0, 240.0));
                        let font = egui::FontId::proportional(14.0);
                        ui.style_mut().override_font_id = Some(font.clone());
                        let theme = ui.active_theme();
                        let hint = snapshot_shortcut_hint(ui.ctx(), &font);
                        let height =
                            (ui.ctx().content_rect().bottom() - ui.next_widget_position().y - 16.0)
                                .max(0.0);
                        egui::ScrollArea::vertical()
                            .max_height(height)
                            .show(ui, |ui| {
                                ui.label(
                                    egui::RichText::new(l10n::text(ui.ctx(), "SNAPSHOT"))
                                        .size(11.0)
                                        .color(theme.ui.text_muted),
                                );
                                for (label, icon, value, available) in [
                                    (
                                        "Download image",
                                        &DOWNLOAD_ICON,
                                        TopToolbarAction::TakeScreenshot,
                                        true,
                                    ),
                                    (
                                        "Copy image",
                                        &embedded_icons::DUPLICATE,
                                        TopToolbarAction::CopyImage,
                                        true,
                                    ),
                                    (
                                        "Open in new tab",
                                        &OPEN_NEW_ICON,
                                        TopToolbarAction::OpenImage,
                                        true,
                                    ),
                                    (
                                        "Copy link",
                                        &LINK_ICON,
                                        TopToolbarAction::CopyImageLink,
                                        self.state.can_publish_image,
                                    ),
                                    (
                                        "Post image",
                                        &embedded_icons::PUBLISH,
                                        TopToolbarAction::Publish,
                                        self.state.can_publish_image,
                                    ),
                                ] {
                                    if !available {
                                        continue;
                                    }
                                    let item = ListItem::new().height(32.0).show(
                                        ui,
                                        LabelContent::new(label).with_icon(icon).icon_size(17.0),
                                    );
                                    // Hosts bind the snapshot shortcut to this entry's action.
                                    if value == TopToolbarAction::TakeScreenshot
                                        && let Some(hint) = &hint
                                    {
                                        ui.painter().text(
                                            item.rect.right_center()
                                                - egui::vec2(theme.spacing.lg, 0.0),
                                            egui::Align2::RIGHT_CENTER,
                                            hint,
                                            font.clone(),
                                            theme.ui.text_muted,
                                        );
                                    }
                                    if item.clicked() {
                                        *action = value;
                                        ui.close();
                                    }
                                }
                            });
                    });
                return;
            }
            S::Undo | S::Redo => {
                let enabled = if control == S::Undo {
                    self.state.can_undo
                } else {
                    self.state.can_redo
                };
                ui.scope(|ui| {
                    // Disabled glyphs carry their own theme ink instead of egui's alpha fade.
                    ui.visuals_mut().disabled_alpha = 1.0;
                    ui.add_enabled(enabled, |ui: &mut Ui| {
                        ToolbarIconButton::new(icon, label, &self.config).show(ui)
                    })
                })
                .inner
                .on_disabled_hover_text(l10n::text(
                    ui.ctx(),
                    if control == S::Undo {
                        "Nothing to undo"
                    } else {
                        "Nothing to redo"
                    },
                ))
            }
            S::Indicators | S::Alert | S::Replay => {
                IconTextButton::new(icon, label, &self.config).show(ui)
            }
            _ => ToolbarIconButton::new(icon, label, &self.config).show(ui),
        };
        if response.clicked() {
            if control == S::Replay {
                self.state.toggle_replay();
            }
            *action = clicked_action;
        }
    }

    /// The templates menu's pages as equal halves of one track, the selected one a panel tile.
    fn show_templates_tabs(&mut self, ui: &mut Ui) {
        let theme = ui.active_theme();
        let size = egui::vec2(ui.available_width(), 34.0);
        let (track, _) = ui.allocate_exact_size(size, egui::Sense::hover());
        ui.painter().rect_filled(
            track,
            theme.radius.md,
            theme
                .ui
                .panel_bg
                .lerp_to_gamma(theme.ui.panel_bg_secondary, 0.65),
        );
        for (index, page) in [TemplatesPage::Indicators, TemplatesPage::Templates]
            .into_iter()
            .enumerate()
        {
            let half = egui::Rect::from_min_size(
                track.min + egui::vec2(index as f32 * size.x / 2.0, 0.0),
                egui::vec2(size.x / 2.0, size.y),
            );
            let id = ui.scope_id().with(page.label());
            let response = ui.interact(half, id, egui::Sense::click());
            if response.clicked() {
                self.templates_page = page;
            }
            let selected = self.templates_page == page;
            let label = l10n::text(ui.ctx(), page.label());
            response.widget_info(|| WidgetInfo::labeled(Role::Tab, ui.is_enabled(), &label));
            ui.ctx()
                .accesskit_node_builder(response.id, |node| node.set_selected(selected));
            if selected {
                ui.painter().rect_filled(
                    half.shrink(theme.spacing.xs),
                    theme.radius.sm,
                    theme.ui.panel_bg,
                );
            }
            ui.painter().text(
                half.center(),
                egui::Align2::CENTER_CENTER,
                label,
                egui::FontId::proportional(theme.text.title),
                if selected || response.hovered() {
                    theme.ui.text
                } else {
                    theme.ui.text_muted
                },
            );
            crate::styles::focus::draw_focus_ring(ui, &response, theme.radius.md);
        }
    }

    /// Favourite studies to add and saved templates to apply, on two pages.
    fn show_indicator_templates(&mut self, ui: &mut Ui, action: &mut TopToolbarAction) {
        use crate::ui_kit::{EmptyState, list_item::LabelContent};
        ui.set_width(292.0);
        self.show_templates_tabs(ui);
        ui.style_mut().override_font_id = Some(egui::FontId::proportional(14.0));
        let (entries, empty) = match self.templates_page {
            TemplatesPage::Indicators => (
                &self.favorite_indicators,
                "No favorite indicators added yet",
            ),
            TemplatesPage::Templates => {
                if ListItem::new()
                    .height(40.0)
                    .show_with_icon(ui, "Save indicator template", &embedded_icons::PLUS)
                    .clicked()
                {
                    *action = TopToolbarAction::SaveIndicatorTemplate;
                    ui.close();
                }
                (&self.indicator_templates, "No templates saved yet")
            }
        };
        if entries.is_empty() {
            // Keep room for a few rows, with the sentence centred in it.
            let height = (203.0 - (ui.cursor().top() - ui.min_rect().top())).max(0.0);
            ui.allocate_ui(egui::vec2(ui.available_width(), height), |ui| {
                ui.set_min_height(height);
                EmptyState::new(empty).fill_height().show(ui);
            });
            return;
        }
        egui::ScrollArea::vertical()
            .max_height(320.0)
            .show(ui, |ui| {
                for (id, name) in entries {
                    if ListItem::new()
                        .height(32.0)
                        .show(ui, LabelContent::new(name))
                        .clicked()
                    {
                        *action = match self.templates_page {
                            TemplatesPage::Indicators => TopToolbarAction::AddIndicator(id.clone()),
                            TemplatesPage::Templates => {
                                TopToolbarAction::ApplyIndicatorTemplate(id.clone())
                            }
                        };
                        ui.close();
                    }
                }
            });
    }

    fn symbol_trigger(symbol: &str) -> Trigger<'static> {
        Trigger {
            icon: Some(&embedded_icons::QUICK_SEARCH),
            label: symbol.to_owned(),
            name: "Symbol search".to_owned(),
            min_width: 128.0,
            filled: true,
        }
    }

    fn trade_button() -> PillButton<'static> {
        PillButton {
            label: "Trade",
            tooltip: Some("Order ticket"),
            filled: false,
        }
    }

    fn save_button(config: &TopToolbarConfig) -> TextButton {
        TextButton::new("Save".to_owned(), config).with_tooltip("Save chart")
    }

    fn save_menu_button(config: &TopToolbarConfig) -> TextIconButton<'static> {
        TextIconButton::new("", &embedded_icons::CHEVRON_DOWN, "Layout options", config)
    }
}

impl TopToolbar {
    /// Sync toolbar state from application state.
    pub fn sync_from_app_state(&mut self, app_state: &dyn crate::ui::app_state::ChartAppState) {
        self.state.sync_from_app_state(app_state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, PointerButton, Pos2, RawInput, Rect, pos2, vec2};

    struct State;
    impl crate::ui::app_state::ChartAppState for State {
        fn active_symbol(&self) -> &str {
            "SYNTH:LONG_SYMBOL"
        }
        fn active_timeframe(&self) -> &crate::model::Timeframe {
            &crate::model::Timeframe::Hour1
        }
        fn chart_type(&self) -> ChartType {
            ChartType::Candles
        }
        fn is_connected(&self) -> bool {
            true
        }
        fn is_replay_active(&self) -> bool {
            false
        }
    }

    /// Actions from clicking the strip every 4 px along its middle, one click at a time.
    fn click_along(ctx: &egui::Context, toolbar: &mut TopToolbar) -> Vec<TopToolbarAction> {
        let mut actions = Vec::new();
        let screen = Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 32.0));
        for x in (0..1440).step_by(4) {
            let pos = pos2(x as f32, 16.0);
            for pressed in [true, false] {
                let input = RawInput {
                    screen_rect: Some(screen),
                    events: vec![
                        Event::PointerMoved(pos),
                        Event::PointerButton {
                            pos,
                            button: PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        },
                    ],
                    ..Default::default()
                };
                ctx.run_ui(input, |ui| {
                    let action = ui
                        .horizontal(|ui| {
                            ui.set_height(toolbar.config.height);
                            toolbar.show_contents_desktop(ui, "BTCUSD")
                        })
                        .inner;
                    actions.push(action);
                })
                .drop_without_applying_deltas();
            }
        }
        actions
    }

    #[test]
    fn disabled_history_icons_use_the_theme_ink_without_fading() {
        for (_, mut theme) in crate::theme::presets() {
            theme.ui.text_disabled = egui::Color32::from_rgb(123, 100, 90);
            let ctx = egui::Context::default();
            egui_extras::install_image_loaders(&ctx);
            crate::theme::apply_to_egui(&ctx, &theme);
            let mut toolbar = TopToolbar::new();
            toolbar.state.can_undo = false;
            toolbar.state.can_redo = false;
            for _ in 0..2 {
                let mut output = ctx.run_ui(Default::default(), |ui| {
                    for slot in [S::Undo, S::Redo] {
                        toolbar.show_control(ui, slot, "SAMPLE", &mut TopToolbarAction::None);
                    }
                });
                output.textures_delta.clear();
                let icons: Vec<_> = output
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Rect(rect) if rect.brush.is_some() => Some(rect.fill),
                        _ => None,
                    })
                    .collect();
                assert_eq!(icons, vec![theme.ui.text_disabled; 2]);
                output.drop_without_applying_deltas();
            }
        }
    }

    #[test]
    fn templates_tabs_switch_the_page_and_keep_the_menu_open() {
        use egui::accesskit::Role;
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        let mut harness = Harness::builder()
            .with_size(vec2(400.0, 400.0))
            .with_context_setup(egui_extras::install_image_loaders)
            .build_ui_state(
                |ui, toolbar: &mut TopToolbar| {
                    toolbar.show_control(ui, S::Templates, "SAMPLE", &mut TopToolbarAction::None);
                },
                TopToolbar::new(),
            );
        harness.run();
        harness
            .get_by_role_and_label(Role::Button, "Indicator templates")
            .click();
        harness.run();
        harness
            .get_by_role_and_label(Role::Tab, "Templates")
            .click();
        harness.run();
        harness.get_by_label("Save indicator template");
        for (tab, selected) in [("Indicators", false), ("Templates", true)] {
            let node = harness.get_by_role_and_label(Role::Tab, tab);
            assert_eq!(node.accesskit_node().is_selected(), Some(selected), "{tab}");
        }
    }

    #[test]
    fn hover_labels_name_the_control_and_show_keys_as_symbols() {
        use egui_kittest::{Harness, kittest::Queryable};
        for (slot, button, label, keys) in [
            (S::Indicators, "Indicators", "Indicators", &["/"][..]),
            (S::Alert, "Alert", "Create alert", &["Alt", "A"]),
            (S::Save, "Layout options", "Layout options", &[]),
        ] {
            let mut harness = Harness::builder()
                .with_size(vec2(400.0, 200.0))
                .with_context_setup(egui_extras::install_image_loaders)
                .build_ui_state(
                    move |ui, toolbar: &mut TopToolbar| {
                        ui.horizontal(|ui| {
                            toolbar.show_control(ui, slot, "SAMPLE", &mut TopToolbarAction::None);
                        });
                    },
                    TopToolbar::new(),
                );
            harness.run();
            harness
                .get_by_role_and_label(egui::accesskit::Role::Button, button)
                .hover();
            harness.run_steps(4);
            harness.get_by_role_and_label(egui::accesskit::Role::Label, label);
            for key in keys {
                let chip = harness.get_by_role_and_label(egui::accesskit::Role::Label, key);
                assert_eq!(chip.rect().height(), 18.0, "the {key} chip of {label}");
            }
        }
    }

    #[test]
    fn save_and_layout_options_have_separate_hover_and_focus_targets() {
        use egui_kittest::{Harness, kittest::Queryable};
        for (_, theme) in crate::theme::presets() {
            let context_theme = theme.clone();
            let mut harness = Harness::builder()
                .with_size(vec2(300.0, 100.0))
                .with_context_setup(move |ctx| {
                    egui_extras::install_image_loaders(ctx);
                    crate::theme::apply_to_egui(ctx, &context_theme);
                    l10n::set_translator(ctx, |english| {
                        assert!(
                            !english.is_empty(),
                            "icon-only arrows have no text to translate"
                        );
                        None
                    });
                })
                .build_ui_state(
                    |ui, toolbar: &mut TopToolbar| {
                        ui.horizontal(|ui| {
                            ui.spacing_mut().item_spacing.x = 0.0;
                            toolbar.show_control(
                                ui,
                                S::Save,
                                "SAMPLE",
                                &mut TopToolbarAction::None,
                            );
                        });
                    },
                    TopToolbar::new(),
                );
            harness.run();
            let save = harness
                .get_by_role_and_label(egui::accesskit::Role::Button, "Save chart")
                .rect();
            let options = harness
                .get_by_role_and_label(egui::accesskit::Role::Button, "Layout options")
                .rect();
            assert!(save.right() <= options.left());
            for (label, rect) in [("Save chart", save), ("Layout options", options)] {
                harness
                    .get_by_role_and_label(egui::accesskit::Role::Button, label)
                    .hover();
                harness.run();
                let tiles: Vec<_> = harness
                    .output()
                    .shapes
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        egui::Shape::Rect(tile) if tile.fill == theme.ui.btn_bg_hover => {
                            Some(tile.rect)
                        }
                        _ => None,
                    })
                    .collect();
                assert_eq!(tiles, [rect], "{label} must hover independently");
                harness
                    .get_by_role_and_label(egui::accesskit::Role::Button, label)
                    .focus();
                harness.run();
                assert!(
                    harness
                        .get_by_role_and_label(egui::accesskit::Role::Button, label)
                        .is_focused()
                );
            }
        }
    }

    #[test]
    fn menu_triggers_fit_without_input_outlines_or_chart_type_text() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for (width, host_label) in [
                (320.0, "Host action"),
                (393.0, "Host action"),
                (640.0, "Host action"),
                (1440.0, "Host action"),
                (600.0, "Search   Trade   Publish"),
            ] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut toolbar = TopToolbar::new();
                toolbar.timeframe_selector.current = Timeframe::Hour1;
                toolbar.state.can_undo = true;
                toolbar.state.can_redo = true;
                toolbar.state.can_publish_image = true;
                ctx.global_style_mut(|style| {
                    style.scroll_animation = egui::style::ScrollAnimation::none();
                    style.spacing.scroll.kinetic.decay_time = 0.0;
                });
                let mut menu_rect = None;
                let mut frame = |events| {
                    let mut host_rect = Rect::NOTHING;
                    let mut action = TopToolbarAction::None;
                    let mut strip = Rect::NOTHING;
                    let output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 50.0))),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            action = toolbar.show_with_slots(
                                ui,
                                &State,
                                |ui| {
                                    let rect = IconButton::new(&embedded_icons::MENU)
                                        .tooltip("Menu")
                                        .show(ui)
                                        .rect;
                                    assert_eq!(
                                        *menu_rect.get_or_insert(rect),
                                        rect,
                                        "menu must not scroll with the strip"
                                    );
                                },
                                |ui| {
                                    host_rect = crate::ui_kit::buttons::Button::new(host_label)
                                        .ghost()
                                        .show(ui)
                                        .rect;
                                },
                            );
                            strip = ui.min_rect();
                            if !ui.ctx().will_discard() {
                                assert!(
                                    ui.min_rect().right() <= width,
                                    "controls exceed the strip"
                                );
                            }
                        },
                    );
                    (output, host_rect, action, strip)
                };
                let (mut output, host_rect, _, strip) = frame(vec![]);
                output.textures_delta.clear();
                assert_eq!(strip.height(), 38.0, "scrollbar must take no space");
                if width < 1000.0 {
                    assert!(host_rect.left() > width, "controls must stay in the row");
                }
                let mut interval = false;
                for shape in &output.shapes {
                    match &shape.shape {
                        egui::Shape::Text(text) => {
                            assert_ne!(text.galley.text(), "Candles");
                            interval |= text.galley.text() == "1h";
                            assert!(
                                text.galley
                                    .rect
                                    .translate(text.pos.to_vec2())
                                    .intersect(shape.clip_rect)
                                    .right()
                                    <= width,
                                "{} outside toolbar at {} with width {}",
                                text.galley.text(),
                                text.pos.x,
                                text.galley.size().x
                            );
                        }
                        egui::Shape::Rect(rect)
                            if rect.rect.height() >= 28.0
                                && (rect.fill != egui::Color32::TRANSPARENT
                                    || rect.stroke.width > 0.0
                                    || rect.brush.is_some()) =>
                        {
                            assert!(
                                rect.rect.right() <= host_rect.left() || rect.rect == host_rect,
                                "control {:?} overlaps host {:?}",
                                rect.rect,
                                host_rect
                            );
                            assert_eq!(
                                rect.stroke.width, 0.0,
                                "triggers must have no input outline"
                            );
                        }
                        _ => {}
                    }
                }
                output.drop_without_applying_deltas();
                assert!(interval);
                if width < 1000.0 {
                    let pointer = |pos, pressed| Event::PointerButton {
                        pos,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    };
                    let touch = |pos, phase| Event::Touch {
                        device_id: egui::TouchDeviceId(0),
                        id: egui::TouchId(0),
                        pos,
                        phase,
                        force: None,
                    };
                    // Both ends are over the symbol button: movement must cancel its click.
                    let start = pos2(200.0, 19.0);
                    let end = pos2(100.0, 19.0);
                    frame(vec![Event::PointerMoved(start)])
                        .0
                        .drop_without_applying_deltas();
                    frame(vec![
                        touch(start, egui::TouchPhase::Start),
                        pointer(start, true),
                    ])
                    .0
                    .drop_without_applying_deltas();
                    frame(vec![
                        touch(pos2(180.0, 19.0), egui::TouchPhase::Move),
                        Event::PointerMoved(pos2(180.0, 19.0)),
                    ])
                    .0
                    .drop_without_applying_deltas();
                    frame(vec![
                        touch(end, egui::TouchPhase::Move),
                        Event::PointerMoved(end),
                    ])
                    .0
                    .drop_without_applying_deltas();
                    let (out, moved, action, _) = frame(vec![]);
                    assert_eq!(action, TopToolbarAction::None);
                    assert!(
                        host_rect.left() - moved.left() > 50.0,
                        "button drag must scroll: {host_rect:?} -> {moved:?}, dragged {:?}",
                        ctx.dragged_id()
                    );
                    out.drop_without_applying_deltas();
                    let (out, _, action, _) =
                        frame(vec![touch(end, egui::TouchPhase::End), pointer(end, false)]);
                    assert_eq!(action, TopToolbarAction::None, "swipe must not activate");
                    out.drop_without_applying_deltas();
                    for source in [
                        egui::MouseWheelSource::Wheel,
                        egui::MouseWheelSource::Trackpad,
                    ] {
                        for delta in [-2000.0, 2000.0] {
                            let (out, before, _, _) =
                                frame(vec![Event::PointerMoved(pos2(150.0, 19.0))]);
                            out.drop_without_applying_deltas();
                            frame(vec![Event::MouseWheel {
                                unit: egui::MouseWheelUnit::Point,
                                delta: vec2(delta, 0.0),
                                phase: egui::TouchPhase::Move,
                                source,
                                modifiers: Default::default(),
                            }])
                            .0
                            .drop_without_applying_deltas();
                            for _ in 0..60 {
                                frame(vec![]).0.drop_without_applying_deltas();
                            }
                            let (out, after, _, _) = frame(vec![]);
                            out.drop_without_applying_deltas();
                            assert!(
                                (after.left() - before.left()) * delta > 0.0,
                                "horizontal wheel must scroll"
                            );
                            if delta < 0.0 {
                                assert!(after.right() <= width, "last control must be reachable");
                            }
                        }
                    }
                    frame(vec![Event::PointerMoved(start)])
                        .0
                        .drop_without_applying_deltas();
                    frame(vec![
                        touch(start, egui::TouchPhase::Start),
                        pointer(start, true),
                    ])
                    .0
                    .drop_without_applying_deltas();
                    let (out, _, action, _) = frame(vec![
                        touch(start, egui::TouchPhase::End),
                        pointer(start, false),
                    ]);
                    assert_eq!(
                        action,
                        TopToolbarAction::OpenSymbolSearch,
                        "plain tap must activate"
                    );
                    out.drop_without_applying_deltas();
                    ctx.memory_mut(|m| {
                        if let Some(id) = m.focused() {
                            m.surrender_focus(id);
                        }
                    });
                }
                let mut labels = Vec::new();
                let mut previous = None;
                let mut reached_host = false;
                for index in 0..30 {
                    let (output, _, _, _) = frame(vec![Event::Key {
                        key: egui::Key::Tab,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }]);
                    for event in &output.platform_output.events {
                        if let egui::output::OutputEvent::FocusGained(info) = event
                            && let Some(label) = &info.label
                        {
                            labels.push(label.clone());
                        }
                    }
                    output.drop_without_applying_deltas();
                    for _ in 0..3 {
                        frame(vec![]).0.drop_without_applying_deltas();
                    }
                    let (output, host_rect, _, _) = frame(vec![]);
                    let focused = ctx
                        .memory(|m| m.focused())
                        .and_then(|id| ctx.read_response(id))
                        .unwrap()
                        .rect;
                    assert!(
                        focused.left() >= 0.0 && focused.right() <= width,
                        "focus must scroll into view: {focused:?}"
                    );
                    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                        egui::Shape::Rect(r) if (r.rect.min - focused.expand(2.0).min).length() < 1.5 && (r.rect.max - focused.expand(2.0).max).length() < 1.5 && r.stroke.width == 2.0)), "focused control has no ring: {focused:?}");
                    output.drop_without_applying_deltas();
                    let content_x = focused.center().x - host_rect.right();
                    if let Some(center) = previous {
                        assert!(content_x >= center, "Tab moved backwards: {focused:?}");
                    }
                    previous = Some(content_x);
                    if focused == host_rect {
                        assert!(index > 1, "host actions preceded built-in controls");
                        reached_host = true;
                        break;
                    }
                }
                assert!(reached_host);
                assert_eq!(
                    labels,
                    [
                        "Menu",
                        "Symbol search",
                        "Compare symbols",
                        "1 hour",
                        "Candles",
                        "Indicators",
                        "Indicator templates",
                        "Alert",
                        "Replay",
                        "Undo",
                        "Redo",
                        "Chart layout",
                        "Named layouts",
                        "Save chart",
                        "Layout options",
                        "Quick Search",
                        "Settings",
                        "Fullscreen",
                        "Snapshot",
                        "Publish",
                        host_label,
                    ],
                    "accessible controls must keep their order at every width"
                );
            }
        }
    }

    #[test]
    fn toolbar_buttons_and_menu_choices_emit_distinct_actions() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            ctx.enable_accesskit();
            for (slot, label, expected) in [
                (
                    S::QuickSearch,
                    "Quick Search",
                    TopToolbarAction::OpenQuickSearch,
                ),
                (S::Symbol, "Symbol", TopToolbarAction::OpenSymbolSearch),
                (
                    S::Screenshot,
                    "Download image",
                    TopToolbarAction::TakeScreenshot,
                ),
                (S::Screenshot, "Copy image", TopToolbarAction::CopyImage),
                (
                    S::Screenshot,
                    "Open in new tab",
                    TopToolbarAction::OpenImage,
                ),
                (S::Screenshot, "Copy link", TopToolbarAction::CopyImageLink),
                (S::Screenshot, "Post image", TopToolbarAction::Publish),
                (
                    S::Save,
                    "Save layout",
                    TopToolbarAction::Layout(LayoutAction::Save),
                ),
                (
                    S::Save,
                    "Download chart data",
                    TopToolbarAction::Layout(LayoutAction::DownloadData),
                ),
                (
                    S::Save,
                    "Create new layout",
                    TopToolbarAction::Layout(LayoutAction::CreateNew),
                ),
                (
                    S::Save,
                    "Open layout",
                    TopToolbarAction::Layout(LayoutAction::OpenLayout),
                ),
                (
                    S::Layouts,
                    "2x2 Grid",
                    TopToolbarAction::Layout(LayoutAction::SelectGrid(
                        crate::widget::ChartLayoutMode::Four,
                    )),
                ),
                (
                    S::LayoutName,
                    "Save layout",
                    TopToolbarAction::Layout(LayoutAction::Save),
                ),
                (
                    S::LayoutName,
                    "Rename…",
                    TopToolbarAction::Layout(LayoutAction::Rename("Unnamed".into(), "Desk".into())),
                ),
                (
                    S::LayoutName,
                    "Save layout as…",
                    TopToolbarAction::Layout(LayoutAction::SaveAs("Desk".into())),
                ),
                (
                    S::Templates,
                    "Simple Moving Average",
                    TopToolbarAction::AddIndicator("SMA".into()),
                ),
                (
                    S::Templates,
                    "Save indicator template",
                    TopToolbarAction::SaveIndicatorTemplate,
                ),
                (
                    S::Templates,
                    "My studies",
                    TopToolbarAction::ApplyIndicatorTemplate("custom-id".into()),
                ),
                (S::Undo, "Undo", TopToolbarAction::Undo),
                (S::Undo, "Undo", TopToolbarAction::None),
                (S::Redo, "Redo", TopToolbarAction::Redo),
                (S::Redo, "Redo", TopToolbarAction::None),
                (S::Replay, "Replay", TopToolbarAction::ToggleReplay),
                (S::Publish, "Publish", TopToolbarAction::Publish),
                (S::Trade, "Trade", TopToolbarAction::ToggleTrading),
            ] {
                let direct = matches!(slot, S::Undo | S::Redo | S::Replay | S::Publish | S::Trade);
                let mut toolbar = TopToolbar::new();
                let publishing = label != "Copy image";
                toolbar.state.can_publish_image = publishing;
                toolbar.state.can_undo = expected != TopToolbarAction::None;
                toolbar.state.can_redo = expected != TopToolbarAction::None;
                toolbar
                    .indicator_templates
                    .push(("custom-id".into(), "My studies".into()));
                toolbar
                    .favorite_indicators
                    .push(("SMA".into(), "Simple Moving Average".into()));
                if matches!(
                    expected,
                    TopToolbarAction::SaveIndicatorTemplate
                        | TopToolbarAction::ApplyIndicatorTemplate(_)
                ) {
                    toolbar.templates_page = TemplatesPage::Templates;
                }
                let height = if slot == S::Screenshot { 120.0 } else { 900.0 };
                let mut frame = |events| {
                    let mut action = TopToolbarAction::None;
                    let mut rect = Rect::NOTHING;
                    let output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(320.0, height))),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            rect = ui
                                .horizontal(|ui| {
                                    toolbar.show_control(ui, slot, "SAMPLE", &mut action);
                                })
                                .response
                                .rect;
                        },
                    );
                    let labels: Vec<_> = output
                        .shapes
                        .iter()
                        .filter_map(|shape| match &shape.shape {
                            egui::Shape::Text(text) => Some((
                                text.galley.text().to_owned(),
                                text.pos + text.galley.size() / 2.0,
                            )),
                            _ => None,
                        })
                        // Icon-only choices, such as the layout diagrams, by their accessible names.
                        .chain(
                            output
                                .platform_output
                                .accesskit_update
                                .iter()
                                .flat_map(|update| &update.nodes)
                                .filter_map(|(_, node)| {
                                    let bounds = node.bounds()?;
                                    Some((
                                        node.label()?.to_owned(),
                                        pos2(
                                            ((bounds.x0 + bounds.x1) / 2.0) as f32,
                                            ((bounds.y0 + bounds.y1) / 2.0) as f32,
                                        ),
                                    ))
                                }),
                        )
                        .collect();
                    output.drop_without_applying_deltas();
                    (action, rect, labels)
                };
                let (_, rect, _) = frame(vec![]);
                let pos = if slot == S::Save {
                    pos2(rect.right() - 8.0, rect.center().y)
                } else {
                    rect.center()
                };
                frame(vec![Event::PointerMoved(pos)]);
                let click = |pos, pressed| Event::PointerButton {
                    pos,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                };
                frame(vec![click(pos, true)]);
                let (action, _, _) = frame(vec![click(pos, false)]);
                if matches!(slot, S::QuickSearch | S::Symbol) || direct {
                    assert_eq!(action, expected);
                    if slot == S::Replay {
                        assert!(toolbar.state.replay_active);
                    }
                    continue;
                }
                assert_eq!(
                    action,
                    TopToolbarAction::None,
                    "opening the menu must not capture an image"
                );
                frame(vec![]);
                let (_, _, mut labels) = frame(vec![]);
                if slot == S::Screenshot && !publishing {
                    assert!(!labels.iter().any(|(text, _)| text == "Copy link"));
                    assert!(!labels.iter().any(|(text, _)| text == "Post image"));
                }
                for _ in 0..32 {
                    if labels
                        .iter()
                        .any(|(text, at)| text == label && (5.0..height - 16.0).contains(&at.y))
                    {
                        break;
                    }
                    labels = frame(vec![
                        Event::PointerMoved(pos2(150.0, 100.0)),
                        Event::MouseWheel {
                            unit: egui::MouseWheelUnit::Point,
                            delta: vec2(0.0, -6.0),
                            phase: egui::TouchPhase::Move,
                            source: egui::MouseWheelSource::Unknown,
                            modifiers: Default::default(),
                        },
                    ])
                    .2;
                }
                let pos = labels
                    .iter()
                    .find(|(text, at)| text == label && (5.0..height - 16.0).contains(&at.y))
                    .unwrap_or_else(|| panic!("reachable {label}: {labels:?}"))
                    .1;
                frame(vec![Event::PointerMoved(pos)]);
                frame(vec![click(pos, true)]);
                let (mut action, _, _) = frame(vec![click(pos, false)]);
                if matches!(
                    expected,
                    TopToolbarAction::Layout(LayoutAction::Rename(_, _) | LayoutAction::SaveAs(_))
                ) {
                    let (_, _, labels) = frame(vec![]);
                    assert!(
                        labels.iter().any(|(text, _)| text == "Apply"),
                        "editing must keep the popup open"
                    );
                    let key = |key, modifiers| Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers,
                    };
                    frame(vec![key(egui::Key::A, egui::Modifiers::COMMAND)]);
                    frame(vec![Event::Text("Desk".into())]);
                    action = frame(vec![key(egui::Key::Enter, egui::Modifiers::NONE)]).0;
                }
                assert_eq!(action, expected);
            }
        }
    }

    #[test]
    fn controls_that_are_off_emit_nothing() {
        type Off = fn(&mut TopToolbar);
        let rows: &[(Off, &[TopToolbarAction])] = &[
            (|t| t.state.can_undo = false, &[TopToolbarAction::Undo]),
            (|t| t.state.can_redo = false, &[TopToolbarAction::Redo]),
            (
                |t| t.state.can_publish_image = false,
                &[TopToolbarAction::Publish],
            ),
            (
                |t| t.set_control_visible(Control::SymbolSearch, false),
                &[TopToolbarAction::OpenSymbolSearch],
            ),
            (
                |t| t.set_control_visible(Control::Indicators, false),
                &[TopToolbarAction::OpenIndicators],
            ),
            (
                |t| t.set_control_visible(Control::Undo, false),
                &[TopToolbarAction::Undo],
            ),
            (
                |t| t.set_control_visible(Control::Redo, false),
                &[TopToolbarAction::Redo],
            ),
            (
                |t| t.set_control_visible(Control::QuickSearch, false),
                &[TopToolbarAction::OpenQuickSearch],
            ),
            (
                |t| t.set_control_visible(Control::Settings, false),
                &[TopToolbarAction::OpenSettings],
            ),
            (
                |t| t.set_control_visible(Control::Fullscreen, false),
                &[TopToolbarAction::ToggleFullscreen],
            ),
            (
                |t| t.set_control_visible(Control::Compare, false),
                &[TopToolbarAction::OpenCompare],
            ),
            (
                |t| t.set_control_visible(Control::Layouts, false),
                &[TopToolbarAction::Save],
            ),
            (
                |t| t.set_control_visible(Control::Publish, false),
                &[TopToolbarAction::Publish],
            ),
            (
                |t| t.set_control_visible(Control::Trading, false),
                &[TopToolbarAction::ToggleTrading],
            ),
            (
                |t| t.state.can_trade = false,
                &[TopToolbarAction::ToggleTrading],
            ),
            (
                |t| {
                    t.timeframe_selector.favorites.remove(&Timeframe::Min5);
                },
                &[TopToolbarAction::IntervalSelected(Timeframe::Min5)],
            ),
        ];
        let ctx = egui::Context::default();
        let broker = || {
            let mut toolbar = TopToolbar::default();
            toolbar.state.can_trade = true;
            toolbar.state.can_publish_image = true;
            toolbar.timeframe_selector.favorites.insert(Timeframe::Min5);
            toolbar
        };
        let on = click_along(&ctx, &mut broker());
        for (turn_off, gone) in rows {
            for action in *gone {
                assert!(on.contains(action), "{action:?} missing while on");
            }
            let mut toolbar = broker();
            turn_off(&mut toolbar);
            let off = click_along(&ctx, &mut toolbar);
            for action in *gone {
                assert!(!off.contains(action), "{action:?} emitted while off");
            }
            if !gone.contains(&TopToolbarAction::OpenIndicators) {
                assert!(off.contains(&TopToolbarAction::OpenIndicators));
            }
        }
        // The same flags apply to the default menu, independently of the strip.
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for width in [320.0, 1440.0] {
                for (control, hidden, retained) in [
                    (Control::QuickSearch, "Quick Search", "Toggle Theme"),
                    (Control::Theme, "Toggle Theme", "Quick Search"),
                ] {
                    let ctx = egui::Context::default();
                    crate::theme::apply_to_egui(&ctx, &theme);
                    let mut toolbar = TopToolbar::new();
                    toolbar.set_control_visible(control, false);
                    let frame = |toolbar: &mut TopToolbar, events| {
                        let mut output = ctx.run_ui(
                            RawInput {
                                screen_rect: Some(Rect::from_min_size(
                                    Pos2::ZERO,
                                    vec2(width, 900.0),
                                )),
                                events,
                                ..Default::default()
                            },
                            |ui| {
                                toolbar.show_with_state(ui, &State);
                            },
                        );
                        let labels = output
                            .shapes
                            .iter()
                            .filter_map(|shape| match &shape.shape {
                                egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                                _ => None,
                            })
                            .collect::<Vec<_>>();
                        output.textures_delta.clear();
                        output.drop_without_applying_deltas();
                        labels
                    };
                    frame(&mut toolbar, vec![]);
                    let pos = pos2(32.0, 27.0);
                    for pressed in [true, false] {
                        frame(
                            &mut toolbar,
                            vec![
                                Event::PointerMoved(pos),
                                Event::PointerButton {
                                    pos,
                                    button: PointerButton::Primary,
                                    pressed,
                                    modifiers: Default::default(),
                                },
                            ],
                        );
                    }
                    let labels = frame(&mut toolbar, vec![]);
                    assert!(
                        !labels.iter().any(|label| label == hidden),
                        "hidden menu row: {labels:?}"
                    );
                    assert!(
                        labels.iter().any(|label| label == retained),
                        "missing retained menu row: {labels:?}"
                    );
                    toolbar.set_control_visible(control, true);
                    let labels = frame(&mut toolbar, vec![]);
                    assert!(labels.iter().any(|label| label == hidden));
                }
            }
        }
    }
}
