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
use crate::ui_kit::list_item::ListItem;
use egui::Ui;

use super::{
    actions::TopToolbarAction,
    buttons::{IconTextButton, PillButton, TextIconButton, ToolbarIconButton, Trigger},
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
fn gap() -> f32 {
    DESIGN_TOKENS.spacing.sm
}

/// Width of the group rule with its padding.
fn separator() -> f32 {
    2.0 * gap() + DESIGN_TOKENS.sizing.toolbar.separator_width
}

/// Neutral group rule, centred in the strip.
fn show_separator(ui: &mut Ui) {
    ui.add_space(gap());
    let (rect, _) = ui.allocate_exact_size(egui::vec2(1.0, 22.0), egui::Sense::hover());
    let color = super::buttons::rule_color(&ui.active_theme());
    ui.painter().rect_filled(rect, 0.0, color);
    ui.add_space(gap());
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
                                        ui.add_space(gap());
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

        ui.add_space(DESIGN_TOKENS.spacing.lg);
        self.show_groups(ui, LEFT, symbol, &mut action);
        let right = self.groups_width(ui, RIGHT) + DESIGN_TOKENS.spacing.lg;
        ui.add_space((ui.available_width() - right).max(separator()));
        self.show_groups(ui, RIGHT, symbol, &mut action);
        ui.add_space(DESIGN_TOKENS.spacing.lg);
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
                    S::Save => Self::save_button(&self.config).width(ui),
                    S::Trade => Self::trade_button().width(ui),
                    _ => self.config.btn_size,
                };
                count += 1;
            }
            if count > 0 {
                width += gap() * (count - 1) as f32;
                shown += 1;
            }
        }
        width + separator() * shown.saturating_sub(1) as f32
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
                    ui.add_space(gap());
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
                    .align(egui::RectAlign::BOTTOM_START)
                    .align_alternatives(&[])
                    .frame(egui::Frame::popup(ui.style()).fill(ui.active_theme().ui.panel_bg))
                    .show(|ui| self.show_indicator_templates(ui, action));
                return;
            }
            S::Save => {
                let response = Self::save_button(&self.config).show(ui);
                let arrow = ui.interact(
                    egui::Rect::from_min_max(
                        egui::pos2(response.rect.right() - 16.0, response.rect.top()),
                        response.rect.max,
                    ),
                    response.id.with("save_menu"),
                    egui::Sense::click(),
                );
                let translated_label = l10n::text(ui.ctx(), "Layout options");
                arrow.widget_info(|| {
                    egui::WidgetInfo::labeled(
                        egui::Role::Button,
                        arrow.enabled(),
                        &translated_label,
                    )
                });
                crate::styles::focus::draw_focus_ring(ui, &arrow, self.config.rounding);
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
                        ui.set_width(240.0);
                        ui.style_mut().override_font_id = Some(egui::FontId::proportional(14.0));
                        ui.label(
                            egui::RichText::new(l10n::text(ui.ctx(), "SNAPSHOT"))
                                .size(11.0)
                                .color(ui.active_theme().ui.text_secondary),
                        );
                        if ListItem::new()
                            .height(32.0)
                            .show_with_icon(ui, "Download image", &embedded_icons::IMAGE)
                            .clicked()
                        {
                            *action = TopToolbarAction::TakeScreenshot;
                            ui.close();
                        }
                    });
                return;
            }
            S::Undo | S::Redo => {
                let enabled = if control == S::Undo {
                    self.state.can_undo
                } else {
                    self.state.can_redo
                };
                ui.add_enabled(enabled, |ui: &mut Ui| {
                    ToolbarIconButton::new(icon, label, &self.config).show(ui)
                })
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

    fn show_indicator_templates(&self, ui: &mut Ui, action: &mut TopToolbarAction) {
        ui.set_width(296.0);
        ui.style_mut().override_font_id = Some(egui::FontId::proportional(14.0));
        if ListItem::new()
            .height(40.0)
            .show_with_icon(ui, "Save indicator template", &embedded_icons::PLUS)
            .clicked()
        {
            *action = TopToolbarAction::SaveIndicatorTemplate;
            ui.close();
        }
        if !self.indicator_templates.is_empty() {
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    for (id, name) in &self.indicator_templates {
                        if ListItem::new()
                            .height(32.0)
                            .show(ui, crate::ui_kit::list_item::LabelContent::new(name))
                            .clicked()
                        {
                            *action = TopToolbarAction::ApplyIndicatorTemplate(id.clone());
                            ui.close();
                        }
                    }
                });
        }
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

    fn save_button(config: &TopToolbarConfig) -> TextIconButton<'static> {
        TextIconButton::new("Save", &embedded_icons::CHEVRON_DOWN, "Save chart", config)
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
                (
                    S::Save,
                    "Save layout",
                    TopToolbarAction::Layout(LayoutAction::Save),
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
                toolbar.state.can_undo = expected != TopToolbarAction::None;
                toolbar.state.can_redo = expected != TopToolbarAction::None;
                toolbar
                    .indicator_templates
                    .push(("custom-id".into(), "My studies".into()));
                let mut frame = |events| {
                    let mut action = TopToolbarAction::None;
                    let mut rect = Rect::NOTHING;
                    let output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(1440.0, 900.0))),
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
                let (_, _, labels) = frame(vec![]);
                let pos = labels
                    .iter()
                    .find(|(text, _)| text == label)
                    .expect("menu choice")
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
        ];
        let ctx = egui::Context::default();
        let broker = || {
            let mut toolbar = TopToolbar::default();
            toolbar.state.can_trade = true;
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
