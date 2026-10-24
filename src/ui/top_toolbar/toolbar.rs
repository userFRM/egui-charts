//! Top Toolbar Widget
//!
//! Symbol search, interval, chart type, indicators, undo and redo on the left;
//! save, quick search, settings, fullscreen and screenshot on the right. Layouts,
//! alerts and replay live in the overflow menu, alongside controls that leave a narrow strip.

use crate::icons::{Icon, icons as embedded_icons};
use crate::theme::ThemeExt;
use crate::ui_kit::buttons::IconButton;
use crate::ui_kit::list_item::ListItem;
use egui::Ui;

use super::{
    actions::TopToolbarAction,
    buttons::{IconTextButton, TextIconButton, ToolbarIconButton, Trigger},
    components::{
        chart_grid_selector::ChartGridSelector,
        chart_type_selector::{ChartTypeAction, ChartTypeSelector},
        layout_menu::{LayoutAction, LayoutMenu},
        timeframe_selector::{Timeframe, TimeframeAction, TimeframeSelector, TimeframeUnit},
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
}

use Slot as S;

/// Left part of the strip, in groups divided by separators.
const LEFT: &[&[Slot]] = &[
    &[S::Symbol, S::Compare],
    &[S::Interval],
    &[S::ChartType],
    &[S::Indicators, S::Templates, S::Layouts],
    &[S::Alert, S::Replay],
    &[S::Undo, S::Redo],
];

/// Right-aligned part of the strip, in groups divided by separators.
const RIGHT: &[&[Slot]] = &[
    &[S::LayoutName, S::Save],
    &[S::QuickSearch, S::Settings, S::Fullscreen, S::Screenshot],
];

/// Controls that move into the overflow menu when the strip is too narrow, the first to go
/// first.
const OVERFLOW_ORDER: [Slot; 14] = [
    S::QuickSearch,
    S::Compare,
    S::Templates,
    S::Save,
    S::LayoutName,
    S::Layouts,
    S::Screenshot,
    S::Fullscreen,
    S::Alert,
    S::Indicators,
    S::Settings,
    S::Redo,
    S::Undo,
    S::Replay,
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
    let theme = ui.active_theme();
    let color = if theme.dark {
        theme.ui.border.lerp_to_gamma(theme.ui.text, 0.18)
    } else {
        theme.ui.panel_bg.lerp_to_gamma(theme.ui.text, 0.085)
    };
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
            S::Layouts | S::LayoutName => (&embedded_icons::LAYOUT_GRID, "Chart layout", A::None),
            S::Save => (&embedded_icons::DATABASE, "Save chart", A::Save),
            S::QuickSearch => (
                &embedded_icons::QUICK_SEARCH,
                "Quick Search",
                A::OpenQuickSearch,
            ),
            S::Settings => (&embedded_icons::SETTINGS_GEAR, "Settings", A::OpenSettings),
            S::Fullscreen => (
                &embedded_icons::FULLSCREEN,
                "Fullscreen",
                A::ToggleFullscreen,
            ),
            S::Screenshot => (&embedded_icons::CAMERA, "Snapshot", A::TakeScreenshot),
            S::Interval | S::ChartType => return None,
        })
    }

    /// Whether the control loses its label in a narrow strip.
    fn compacts(self) -> bool {
        matches!(self, S::Indicators | S::Alert | S::Replay)
    }

    /// The host control that hides this slot, if any.
    fn host_control(self) -> Option<Control> {
        match self {
            S::Compare => Some(Control::Compare),
            S::Templates => Some(Control::Templates),
            S::Alert => Some(Control::Alerts),
            S::Replay => Some(Control::Replay),
            S::Save | S::Layouts | S::LayoutName => Some(Control::Layouts),
            _ => None,
        }
    }
}

/// Top toolbar widget
pub struct TopToolbar {
    pub config: TopToolbarConfig,
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
            host_hidden: HashSet::new(),
            indicator_templates: Vec::new(),
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

    /// Shows or hides the buttons of `control`: Compare, Templates, Alerts, Replay, and
    /// Layouts (the Save and Layout setup buttons). The toolbar ignores the other controls.
    pub fn set_control_visible(&mut self, control: Control, visible: bool) {
        crate::ui::app_state::set_control_visible(&mut self.host_hidden, control, visible);
    }

    /// Whether the host left `slot` visible.
    fn is_shown(&self, slot: Slot) -> bool {
        slot.host_control()
            .is_none_or(|control| !self.host_hidden.contains(&control))
    }

    /// Show the full toolbar, reading the active symbol from app state.
    pub fn show_with_state(
        &mut self,
        ui: &mut Ui,
        app_state: &dyn crate::ui::app_state::ChartAppState,
    ) -> TopToolbarAction {
        let mut menu_action = TopToolbarAction::None;
        let action = self.show_with_slots(
            ui,
            app_state,
            |ui| Self::show_menu(ui, &mut menu_action),
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
    /// reserved before built-in controls compact or move into overflow. The closures
    /// handle their own responses and may open menus or return actions to the host.
    pub fn show_with_slots(
        &mut self,
        ui: &mut Ui,
        app_state: &dyn crate::ui::app_state::ChartAppState,
        menu: impl FnOnce(&mut Ui),
        actions: impl FnOnce(&mut Ui),
    ) -> TopToolbarAction {
        ui.horizontal(|ui| {
            ui.set_height(self.config.height);
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.allocate_ui_with_layout(
                egui::vec2(48.0, self.config.height),
                egui::Layout::centered_and_justified(egui::Direction::LeftToRight),
                menu,
            );
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
            let width = ui.horizontal(actions).response.rect.width();
            if (width - actions_width).abs() > 0.5 {
                ui.data_mut(|d| d.insert_temp(width_id, width));
                ui.ctx().request_discard("Host actions changed width");
            }
            action
        })
        .inner
    }

    fn show_menu(ui: &mut Ui, action: &mut TopToolbarAction) {
        let response = IconButton::new(&super::buttons::MENU_ICON)
            .icon_scale(24.0 / 28.0)
            .tooltip("Menu")
            .show(ui);
        egui::Popup::menu(&response).show(|ui| {
            ui.set_width(DESIGN_TOKENS.sizing.dialog.submenu_width);
            for control in [S::QuickSearch, S::Theme] {
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

        // The widest arrangement that fits: full, then icon-only labels, then overflow.
        let available = ui.available_width();
        let mut hidden: Vec<_> = [S::Alert, S::Replay, S::Layouts, S::LayoutName]
            .into_iter()
            .filter(|&c| self.is_shown(c))
            .collect();
        let compact = self.strip_width(ui, symbol, false, &hidden) > available;
        if compact {
            for control in OVERFLOW_ORDER {
                if self.strip_width(ui, symbol, true, &hidden) <= available {
                    break;
                }
                // Host-hidden slots never reach the overflow menu.
                if self.is_shown(control) && !hidden.contains(&control) {
                    hidden.push(control);
                }
            }
        }
        let hidden = hidden.as_slice();
        ui.add_space(DESIGN_TOKENS.spacing.lg);
        self.show_groups(ui, LEFT, compact, hidden, symbol, &mut action);
        let right = self.groups_width(ui, RIGHT, compact, hidden, symbol)
            + Self::overflow_width(hidden)
            + DESIGN_TOKENS.spacing.lg;
        ui.add_space((ui.available_width() - right).max(0.0));
        self.show_groups(ui, RIGHT, compact, hidden, symbol, &mut action);
        if !hidden.is_empty() {
            ui.add_space(gap());
            self.show_overflow(ui, hidden, &mut action);
        }

        action
    }

    /// Width of `control` in the strip.
    fn control_width(&self, ui: &Ui, control: Slot, compact: bool, symbol: &str) -> f32 {
        let icon_button = self.config.btn_size;
        match control {
            S::Symbol => Self::symbol_trigger(symbol).width(ui),
            S::Interval => self.timeframe_selector.trigger().width(ui),
            S::ChartType => self.chart_type_selector.trigger().width(ui),
            S::Compare | S::Templates | S::Layouts => self.config.btn_size,
            S::Save => Self::save_button(&self.config).width(ui),
            S::LayoutName => self.layout_menu.width(ui),
            c if c.compacts() && !compact => c.entry().map_or(icon_button, |(_, label, _)| {
                IconTextButton::new(&embedded_icons::INDICATORS, label, &self.config).width(ui)
            }),
            _ => icon_button,
        }
    }

    /// Width of the visible controls of `groups` with their gaps and separators.
    fn groups_width(
        &self,
        ui: &Ui,
        groups: &[&[Slot]],
        compact: bool,
        hidden: &[Slot],
        symbol: &str,
    ) -> f32 {
        let mut width = 0.0;
        let mut shown = 0_usize;
        for group in groups {
            let visible = group
                .iter()
                .filter(|&&c| !hidden.contains(&c) && self.is_shown(c));
            let (mut group_width, mut count) = (0.0, 0);
            for &control in visible {
                group_width += self.control_width(ui, control, compact, symbol);
                count += 1;
            }
            if count > 0 {
                width += group_width + gap() * (count - 1) as f32;
                shown += 1;
            }
        }
        width + separator() * shown.saturating_sub(1) as f32
    }

    /// Width of the strip with `hidden` controls in the overflow menu.
    fn strip_width(&self, ui: &Ui, symbol: &str, compact: bool, hidden: &[Slot]) -> f32 {
        let left = self.groups_width(ui, LEFT, compact, hidden, symbol);
        let right = self.groups_width(ui, RIGHT, compact, hidden, symbol);
        // The two parts stay at least a separator apart.
        2.0 * DESIGN_TOKENS.spacing.lg + left + separator() + right + Self::overflow_width(hidden)
    }

    fn overflow_width(hidden: &[Slot]) -> f32 {
        if hidden.is_empty() {
            0.0
        } else {
            gap() + super::config::BUTTON_SIZE
        }
    }

    /// Draw the visible controls of `groups`, with separators between groups.
    fn show_groups(
        &mut self,
        ui: &mut Ui,
        groups: &[&[Slot]],
        compact: bool,
        hidden: &[Slot],
        symbol: &str,
        action: &mut TopToolbarAction,
    ) {
        let mut first_group = true;
        for group in groups {
            let visible: Vec<Slot> = group
                .iter()
                .copied()
                .filter(|&c| !hidden.contains(&c) && self.is_shown(c))
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
                self.show_control(ui, control, compact, symbol, action);
            }
        }
    }

    fn show_control(
        &mut self,
        ui: &mut Ui,
        control: Slot,
        compact: bool,
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
                if let TimeframeAction::Select(tf) = self.timeframe_selector.show(ui)
                    && let Some(tf) = tf.to_model()
                {
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
                            egui::RichText::new("SNAPSHOT")
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
                .on_disabled_hover_text(if control == S::Undo {
                    "Nothing to undo"
                } else {
                    "Nothing to redo"
                })
            }
            c if c.compacts() && !compact => {
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

    /// The overflow button and its menu of the controls that left the strip.
    fn show_overflow(&mut self, ui: &mut Ui, hidden: &[Slot], action: &mut TopToolbarAction) {
        let response = IconButton::new(&embedded_icons::UI_MORE_HORIZONTAL)
            .tooltip("More")
            .show(ui);
        egui::Popup::menu(&response).show(|ui| {
            ui.set_width(DESIGN_TOKENS.sizing.dialog.submenu_width);
            let strip_order = LEFT.iter().chain(RIGHT).flat_map(|group| group.iter());
            for control in strip_order.filter(|&c| hidden.contains(c)) {
                if *control == S::Layouts {
                    egui::containers::menu::SubMenuButton::new("Chart layout")
                        .config(
                            egui::containers::menu::MenuConfig::new()
                                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                        )
                        .ui(ui, |ui| {
                            if let Some(mode) = self.chart_grid_selector.show_menu(ui) {
                                *action = TopToolbarAction::Layout(LayoutAction::SelectGrid(mode));
                            }
                        });
                } else if *control == S::LayoutName {
                    egui::containers::menu::SubMenuButton::new("Named layouts")
                        .config(
                            egui::containers::menu::MenuConfig::new()
                                .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside),
                        )
                        .ui(ui, |ui| {
                            let a = self.layout_menu.show_menu(ui);
                            if a != LayoutAction::None {
                                *action = TopToolbarAction::Layout(a);
                            }
                        });
                } else if *control == S::Templates {
                    ui.menu_button("Indicator templates", |ui| {
                        self.show_indicator_templates(ui, action)
                    });
                } else if let Some((icon, label, clicked_action)) = control.entry() {
                    let enabled = match control {
                        S::Undo => self.state.can_undo,
                        S::Redo => self.state.can_redo,
                        _ => true,
                    };
                    if ui
                        .add_enabled(enabled, |ui: &mut Ui| {
                            ListItem::new()
                                .selected(*control == S::Replay && self.state.replay_active)
                                .show_with_icon(ui, label, icon)
                        })
                        .clicked()
                    {
                        if *control == S::Replay {
                            self.state.toggle_replay();
                        }
                        *action = clicked_action;
                    }
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
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            for (width, host_label) in [
                (640.0, "Host action"),
                (1440.0, "Host action"),
                (600.0, "Search   Trade   Publish"),
            ] {
                let ctx = egui::Context::default();
                crate::theme::apply_to_egui(&ctx, &theme);
                let mut toolbar = TopToolbar::new();
                toolbar.timeframe_selector.current = Timeframe::new(1, TimeframeUnit::Hour);
                let mut frame = |events| {
                    let mut host_rect = Rect::NOTHING;
                    let output = ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(width, 50.0))),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            toolbar.show_with_slots(
                                ui,
                                &State,
                                |ui| {
                                    IconButton::new(&embedded_icons::MENU).show(ui);
                                },
                                |ui| {
                                    host_rect = crate::ui_kit::buttons::Button::new(host_label)
                                        .ghost()
                                        .show(ui)
                                        .rect;
                                },
                            );
                            if !ui.ctx().will_discard() {
                                assert!(
                                    ui.min_rect().right() <= width,
                                    "controls exceed the strip"
                                );
                            }
                        },
                    );
                    (output, host_rect)
                };
                let (mut output, host_rect) = frame(vec![]);
                output.textures_delta.clear();
                let mut interval = false;
                for shape in &output.shapes {
                    match &shape.shape {
                        egui::Shape::Text(text) => {
                            assert_ne!(text.galley.text(), "Candles");
                            interval |= text.galley.text() == "1h";
                            assert!(
                                text.galley.rect.translate(text.pos.to_vec2()).right() <= width,
                                "{} outside toolbar at {} with width {}",
                                text.galley.text(),
                                text.pos.x,
                                text.galley.size().x
                            );
                        }
                        egui::Shape::Rect(rect) if rect.rect.height() >= 28.0 => {
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
                let mut previous = None;
                let mut reached_host = false;
                for index in 0..30 {
                    let (output, host_rect) = frame(vec![Event::Key {
                        key: egui::Key::Tab,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: egui::Modifiers::NONE,
                    }]);
                    let focused = ctx
                        .memory(|m| m.focused())
                        .and_then(|id| ctx.read_response(id))
                        .unwrap()
                        .rect;
                    assert!(output.shapes.iter().any(|shape| matches!(&shape.shape,
                        egui::Shape::Rect(r) if r.rect == focused.expand(2.0) && r.stroke.width == 2.0)), "focused control has no ring: {focused:?}");
                    output.drop_without_applying_deltas();
                    if let Some(center) = previous {
                        assert!(
                            focused.center().x >= center,
                            "Tab moved backwards: {focused:?}"
                        );
                    }
                    previous = Some(focused.center().x);
                    if focused == host_rect {
                        assert!(index > 1, "host actions preceded built-in controls");
                        reached_host = true;
                        break;
                    }
                }
                assert!(reached_host);
            }
        }
    }

    #[test]
    fn toolbar_menus_require_a_choice() {
        for theme in [crate::theme::Theme::light(), crate::theme::Theme::dark()] {
            let ctx = egui::Context::default();
            crate::theme::apply_to_egui(&ctx, &theme);
            for overflow in [false, true] {
                for (slot, label, expected) in [
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
                        "Rename layout…",
                        TopToolbarAction::Layout(LayoutAction::Rename(
                            "Unnamed".into(),
                            "Desk".into(),
                        )),
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
                ] {
                    let direct = matches!(slot, S::Undo | S::Redo | S::Replay);
                    if (!overflow && direct)
                        || (overflow && !direct && !matches!(slot, S::Layouts | S::LayoutName))
                    {
                        continue;
                    }
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
                                screen_rect: Some(Rect::from_min_size(
                                    Pos2::ZERO,
                                    vec2(1440.0, 900.0),
                                )),
                                events,
                                ..Default::default()
                            },
                            |ui| {
                                rect = ui
                                    .horizontal(|ui| {
                                        if overflow {
                                            toolbar.show_overflow(ui, &[slot], &mut action);
                                        } else {
                                            toolbar.show_control(
                                                ui,
                                                slot,
                                                false,
                                                "SAMPLE",
                                                &mut action,
                                            );
                                        }
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
                    let pos = if slot == S::Save && !overflow {
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
                    assert_eq!(
                        action,
                        TopToolbarAction::None,
                        "opening the menu must not capture an image"
                    );
                    frame(vec![]);
                    if overflow && !direct {
                        let submenu = if slot == S::Layouts {
                            "Chart layout"
                        } else {
                            "Named layouts"
                        };
                        let (_, _, labels) = frame(vec![]);
                        let pos = labels
                            .iter()
                            .find(|(text, _)| text == submenu)
                            .expect("layout submenu")
                            .1;
                        frame(vec![Event::PointerMoved(pos)]);
                        frame(vec![click(pos, true)]);
                        frame(vec![click(pos, false)]);
                        frame(vec![]);
                    }
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
                        TopToolbarAction::Layout(
                            LayoutAction::Rename(_, _) | LayoutAction::SaveAs(_)
                        )
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
                    assert_eq!(action, expected, "overflow {overflow}");
                    if slot == S::Replay {
                        assert!(toolbar.state.replay_active);
                    }
                }
            }
        }
    }

    #[test]
    fn controls_that_are_off_emit_nothing() {
        type Off = fn(&mut TopToolbar);
        let rows: [(Off, &[TopToolbarAction]); 4] = [
            (|t| t.state.can_undo = false, &[TopToolbarAction::Undo]),
            (|t| t.state.can_redo = false, &[TopToolbarAction::Redo]),
            (
                |t| t.set_control_visible(Control::Compare, false),
                &[TopToolbarAction::OpenCompare],
            ),
            (
                |t| t.set_control_visible(Control::Layouts, false),
                &[TopToolbarAction::Save],
            ),
        ];
        let ctx = egui::Context::default();
        let on = click_along(&ctx, &mut TopToolbar::default());
        for (turn_off, gone) in rows {
            for action in gone {
                assert!(on.contains(action), "{action:?} missing while on");
            }
            let mut toolbar = TopToolbar::default();
            turn_off(&mut toolbar);
            let off = click_along(&ctx, &mut toolbar);
            for action in gone {
                assert!(!off.contains(action), "{action:?} emitted while off");
            }
            assert!(off.contains(&TopToolbarAction::OpenIndicators));
        }
    }
}
