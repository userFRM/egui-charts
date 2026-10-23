//! Top Toolbar Widget
//!
//! The symbol search pill, interval and chart type dropdowns, indicators, alert, replay,
//! undo and redo on the left; theme, layouts, save, search, settings, fullscreen and screenshot
//! on the right. When the strip is too narrow, Indicators, Alert and Replay
//! lose their labels, then the least important controls move into an overflow menu at the right.

use crate::icons::{Icon, icons as embedded_icons};
use crate::theme::ThemeExt;
use crate::ui_kit::buttons::{Button, ButtonSize, IconButton};
use crate::ui_kit::list_item::ListItem;
use egui::Ui;

use crate::ext::UiExt;

use super::{
    actions::TopToolbarAction,
    buttons::{TextIconButton, ToolbarIconButton, Trigger},
    components::{
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
    &[S::Indicators, S::Templates],
    &[S::Alert, S::Replay],
    &[S::Undo, S::Redo],
];

/// Right-aligned part of the strip, in groups divided by separators.
const RIGHT: &[&[Slot]] = &[
    &[S::Theme, S::Layouts, S::Save],
    &[S::QuickSearch, S::Settings, S::Fullscreen, S::Screenshot],
];

/// Controls that move into the overflow menu when the strip is too narrow, the first to go
/// first. Quick search opens the same dialog as the symbol pill.
const OVERFLOW_ORDER: [Slot; 8] = [
    S::Compare,
    S::Templates,
    S::Save,
    S::Layouts,
    S::Screenshot,
    S::Fullscreen,
    S::QuickSearch,
    S::Theme,
];

/// Space between the controls of a group.
fn gap() -> f32 {
    DESIGN_TOKENS.spacing.sm
}

/// Width of `UiExt::toolbar_separator` with its padding.
fn separator() -> f32 {
    2.0 * DESIGN_TOKENS.spacing.xl + DESIGN_TOKENS.sizing.toolbar.separator_width
}

/// Width of a kit icon-and-text button: 12 px padding on each side of a 20 px icon, a 4 px
/// gap and the laid-out label.
fn labelled_button_width(ui: &Ui, label: &str) -> f32 {
    let font = egui::TextStyle::Button.resolve(ui.style());
    let galley = ui
        .painter()
        .layout_no_wrap(label.to_owned(), font, egui::Color32::PLACEHOLDER);
    let spacing = &DESIGN_TOKENS.spacing;
    2.0 * spacing.xl + DESIGN_TOKENS.sizing.icon_md + spacing.sm + galley.size().x
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
                &embedded_icons::PLUS,
                "Compare or Add Symbol",
                A::OpenCompare,
            ),
            S::Indicators => (&embedded_icons::INDICATORS, "Indicators", A::OpenIndicators),
            S::Templates => (
                &embedded_icons::LAYOUT_GRID,
                "Indicator templates",
                A::OpenIndicatorTemplates,
            ),
            S::Alert => (&embedded_icons::BELL, "Alert", A::CreateAlert),
            S::Replay => (&embedded_icons::REPLAY, "Replay", A::ToggleReplay),
            S::Undo => (&embedded_icons::UNDO, "Undo", A::Undo),
            S::Redo => (&embedded_icons::REDO, "Redo", A::Redo),
            S::Theme => (
                &embedded_icons::THEME_TOGGLE,
                "Toggle Theme",
                A::ThemeToggle,
            ),
            S::Layouts => (
                &embedded_icons::LAYOUT_SETUP,
                "Manage Layouts",
                A::Layout(LayoutAction::OpenMenu),
            ),
            S::Save => (&embedded_icons::DATABASE, "Save Chart", A::Save),
            S::QuickSearch => (
                &embedded_icons::QUICK_SEARCH,
                "Quick Search",
                A::OpenSymbolSearch,
            ),
            S::Settings => (&embedded_icons::SETTINGS_GEAR, "Settings", A::OpenSettings),
            S::Fullscreen => (
                &embedded_icons::FULLSCREEN,
                "Fullscreen",
                A::ToggleFullscreen,
            ),
            S::Screenshot => (
                &embedded_icons::CAMERA,
                "Take Screenshot",
                A::TakeScreenshot,
            ),
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
            S::Save | S::Layouts => Some(Control::Layouts),
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
    /// Layout selector dropdown
    pub layout_menu: LayoutMenu,
    /// Controls the host hid.
    host_hidden: HashSet<Control>,
}

impl Default for TopToolbar {
    fn default() -> Self {
        Self {
            config: TopToolbarConfig::default(),
            state: TopToolbarState::default(),
            timeframe_selector: TimeframeSelector::new(Timeframe::new(1, TimeframeUnit::Day)),
            chart_type_selector: ChartTypeSelector::new(ChartType::Candles),
            layout_menu: LayoutMenu::new(),
            host_hidden: HashSet::new(),
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
        ui.horizontal(|ui| {
            ui.set_height(self.config.height);
            self.show_contents_desktop(ui, app_state.active_symbol())
        })
        .inner
    }

    /// Show the toolbar inline (same layout, no outer frame).
    pub fn show_inline(
        &mut self,
        ui: &mut Ui,
        app_state: &dyn crate::ui::app_state::ChartAppState,
    ) -> TopToolbarAction {
        ui.horizontal(|ui| {
            ui.set_height(self.config.height);
            self.show_contents_desktop(ui, app_state.active_symbol())
        })
        .inner
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
        let fits =
            |compact, hidden: &[Slot]| self.strip_width(ui, symbol, compact, hidden) <= available;
        // Host-hidden slots never reach the overflow menu.
        let order: Vec<Slot> = OVERFLOW_ORDER
            .into_iter()
            .filter(|&c| self.is_shown(c))
            .collect();
        let (compact, hidden) = if fits(false, &[]) {
            (false, &order[..0])
        } else {
            let n = (0..order.len())
                .find(|&n| fits(true, &order[..n]))
                .unwrap_or(order.len());
            (true, &order[..n])
        };

        ui.add_space(DESIGN_TOKENS.spacing.lg);
        self.show_groups(ui, LEFT, compact, hidden, symbol, &mut action);
        let right = self.groups_width(ui, RIGHT, compact, hidden, symbol)
            + Self::overflow_width(hidden)
            + DESIGN_TOKENS.spacing.lg;
        ui.add_space((ui.available_width() - right).max(0.0));
        self.show_groups(ui, RIGHT, compact, hidden, symbol, &mut action);
        if !hidden.is_empty() {
            ui.add_space(gap());
            Self::show_overflow(ui, hidden, &mut action);
        }

        action
    }

    /// Width of `control` in the strip.
    fn control_width(&self, ui: &Ui, control: Slot, compact: bool, symbol: &str) -> f32 {
        let icon_button = ButtonSize::MD.pixels();
        match control {
            S::Symbol => Self::symbol_trigger(symbol).width(ui),
            S::Interval => self.timeframe_selector.trigger().width(ui),
            S::ChartType => self.chart_type_selector.trigger().width(ui),
            S::Compare | S::Templates | S::Layouts => self.config.btn_size,
            S::Save => Self::save_button(&self.config).width(ui),
            c if c.compacts() && !compact => c.entry().map_or(icon_button, |(_, label, _)| {
                labelled_button_width(ui, label)
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
            gap() + ButtonSize::MD.pixels()
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
                ui.toolbar_separator();
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
                    *action = TopToolbarAction::ChartStyleSelected(ct.name().to_string());
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
            S::Compare | S::Templates | S::Layouts => {
                ToolbarIconButton::new(icon, label, &self.config).show(ui)
            }
            S::Save => Self::save_button(&self.config).show(ui),
            S::Undo => IconButton::new(icon)
                .enabled(self.state.can_undo)
                .tooltip(if self.state.can_undo {
                    label
                } else {
                    "Nothing to undo"
                })
                .show(ui),
            S::Redo => IconButton::new(icon)
                .enabled(self.state.can_redo)
                .tooltip(if self.state.can_redo {
                    label
                } else {
                    "Nothing to redo"
                })
                .show(ui),
            c if c.compacts() && !compact => Button::icon_and_text(icon, label).ghost().show(ui),
            _ => IconButton::new(icon).tooltip(label).show(ui),
        };
        if response.clicked() {
            if control == S::Replay {
                self.state.toggle_replay();
            }
            *action = clicked_action;
        }
    }

    /// The overflow button and its menu of the controls that left the strip.
    fn show_overflow(ui: &mut Ui, hidden: &[Slot], action: &mut TopToolbarAction) {
        let response = IconButton::new(&embedded_icons::UI_MORE_HORIZONTAL)
            .tooltip("More")
            .show(ui);
        egui::Popup::menu(&response).show(|ui| {
            ui.set_width(DESIGN_TOKENS.sizing.dialog.submenu_width);
            let strip_order = LEFT.iter().chain(RIGHT).flat_map(|group| group.iter());
            for control in strip_order.filter(|&c| hidden.contains(c)) {
                if let Some((icon, label, clicked_action)) = control.entry()
                    && ListItem::new().show_with_icon(ui, label, icon).clicked()
                {
                    *action = clicked_action;
                }
            }
        });
    }

    fn symbol_trigger(symbol: &str) -> Trigger<'static> {
        Trigger {
            icon: Some(&embedded_icons::QUICK_SEARCH),
            label: symbol.to_owned(),
            name: format!("Symbol search: {symbol}"),
            min_width: 0.0,
        }
    }

    fn save_button(config: &TopToolbarConfig) -> TextIconButton<'static> {
        TextIconButton::new("Save", &embedded_icons::CHEVRON_DOWN, "Save Chart", config)
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
    fn controls_that_are_off_emit_nothing() {
        type Off = fn(&mut TopToolbar);
        let rows: [(Off, &[TopToolbarAction]); 7] = [
            (|t| t.state.can_undo = false, &[TopToolbarAction::Undo]),
            (|t| t.state.can_redo = false, &[TopToolbarAction::Redo]),
            (
                |t| t.set_control_visible(Control::Compare, false),
                &[TopToolbarAction::OpenCompare],
            ),
            (
                |t| t.set_control_visible(Control::Templates, false),
                &[TopToolbarAction::OpenIndicatorTemplates],
            ),
            (
                |t| t.set_control_visible(Control::Alerts, false),
                &[TopToolbarAction::CreateAlert],
            ),
            (
                |t| t.set_control_visible(Control::Replay, false),
                &[TopToolbarAction::ToggleReplay],
            ),
            (
                |t| t.set_control_visible(Control::Layouts, false),
                &[
                    TopToolbarAction::Save,
                    TopToolbarAction::Layout(LayoutAction::OpenMenu),
                ],
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
