//! Shared shortcut bindings and chart command dispatch.

use crate::config::KeyboardAction;
use egui::{Context, Event, KeyboardShortcut};

/// Editable bindings shared by input dispatch, menus and shortcut help.
///
/// Remove a binding to disable it, or replace its key or action to rebind it.
/// The first binding wins if several actions use the same key combination.
#[derive(Clone, Debug)]
pub struct ShortcutMap {
    /// Key combinations and their actions, in precedence order.
    pub bindings: Vec<(KeyboardShortcut, KeyboardAction)>,
}

impl Default for ShortcutMap {
    fn default() -> Self {
        Self {
            bindings: KeyboardAction::all_by_category()
                .into_iter()
                .flat_map(|(_, actions)| actions)
                .flat_map(|action| action.shortcuts().iter().map(move |key| (*key, action)))
                .collect(),
        }
    }
}

impl ShortcutMap {
    /// The first binding for an action, for a platform-formatted menu hint.
    pub fn shortcut(&self, action: KeyboardAction) -> Option<KeyboardShortcut> {
        self.bindings
            .iter()
            .find_map(|(key, bound)| (*bound == action).then_some(*key))
    }

    /// Consume the first bound key press, leaving other input for its owner.
    fn consume(
        &self,
        ctx: &Context,
        accepts: impl Fn(KeyboardAction) -> bool,
    ) -> Option<(KeyboardAction, Event)> {
        ctx.input_mut(|input| {
            let (index, action) = input.events.iter().enumerate().find_map(|(index, event)| {
                let (key, modifiers) = match event {
                    Event::Key {
                        key,
                        pressed: true,
                        modifiers,
                        ..
                    } => (*key, *modifiers),
                    Event::Copy => (egui::Key::C, egui::Modifiers::COMMAND),
                    Event::Paste(_) => (egui::Key::V, egui::Modifiers::COMMAND),
                    _ => return None,
                };
                self.bindings.iter().find_map(|(shortcut, action)| {
                    let matches = modifiers.matches_exact(shortcut.modifiers)
                        || (matches!(key, egui::Key::Plus | egui::Key::Questionmark)
                            && modifiers.matches_logically(shortcut.modifiers)
                            && modifiers.alt == shortcut.modifiers.alt);
                    (accepts(*action) && key == shortcut.logical_key && matches)
                        .then_some((index, *action))
                })
            })?;
            let event = input.events.remove(index);
            Some((action, event))
        })
    }

    /// Bound actions grouped for help, including bindings added by the host.
    pub fn by_category(&self) -> Vec<(&'static str, Vec<KeyboardAction>)> {
        let mut categories = KeyboardAction::all_by_category();
        for (_, actions) in &mut categories {
            actions.retain(|action| self.shortcut(*action).is_some());
        }
        let mut custom = Vec::new();
        for (_, action) in &self.bindings {
            if !categories
                .iter()
                .any(|(_, actions)| actions.contains(action))
                && !custom.contains(action)
            {
                custom.push(*action);
            }
        }
        if !custom.is_empty() {
            categories.push(("Custom", custom));
        }
        categories.retain(|(_, actions)| !actions.is_empty());
        categories
    }
}

/// A shortcut that needs the host's dialogs, layout or clipboard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ShortcutRequest {
    /// Execute an application command.
    Action(KeyboardAction),
    /// Paste clipboard text. The host recognizes its drawing format before inserting anything.
    PasteDrawing(String),
    /// Open symbol search with the typed text.
    SymbolSearch(String),
    /// Open interval entry with the typed digits.
    IntervalEntry(String),
}

#[derive(Default)]
pub(crate) struct KeyboardState {
    pub map: ShortcutMap,
    pub requests: Vec<ShortcutRequest>,
    pub chart_id: Option<egui::Id>,
    pub last_pass: Option<u64>,
    pub focus_elsewhere: bool,
}

pub(crate) fn shortcuts_suppressed(ctx: &Context) -> bool {
    ctx.data(|data| data.get_temp::<u64>(egui::Id::NULL.with("chart_keyboard_capture")))
        == Some(ctx.cumulative_pass_nr())
}

impl super::Chart {
    /// Bindings used by this chart. Hosts pass this same map to help and menus.
    pub fn shortcuts(&self) -> &ShortcutMap {
        &self.keyboard.map
    }

    /// Edit this chart's keyboard bindings.
    pub fn shortcuts_mut(&mut self) -> &mut ShortcutMap {
        &mut self.keyboard.map
    }

    /// Drain commands that need host UI or a host-owned service.
    pub fn take_shortcut_requests(&mut self) -> Vec<ShortcutRequest> {
        std::mem::take(&mut self.keyboard.requests)
    }

    /// Give host dialogs keyboard priority over every chart and drawing toolbar this pass.
    pub fn suppress_shortcuts(&mut self, ctx: &Context) {
        let pass = ctx.cumulative_pass_nr();
        ctx.data_mut(|data| data.insert_temp(egui::Id::NULL.with("chart_keyboard_capture"), pass));
        self.keyboard.focus_elsewhere = true;
    }

    /// Dispatch keys before other application controls, after dialogs have had their turn.
    ///
    /// `show` also calls this for standalone charts. Each pass dispatches at most once.
    /// Text fields, other focused widgets and open popups keep their input.
    pub fn dispatch_shortcuts(&mut self, ctx: &Context) {
        let pass = ctx.cumulative_pass_nr();
        if !self.chart_options.keyboard.enabled
            || shortcuts_suppressed(ctx)
            || self.keyboard.last_pass == Some(pass)
        {
            return;
        }
        self.keyboard.last_pass = Some(pass);
        ctx.data_mut(|data| {
            data.insert_temp(
                egui::Id::NULL.with("chart_shortcuts"),
                self.keyboard.map.clone(),
            )
        });
        let focused = ctx.memory(|memory| memory.focused());
        let elsewhere = focused.is_some() && focused != self.keyboard.chart_id;
        if elsewhere && self.chart_options.keyboard.enabled && !egui::Popup::is_any_open(ctx) {
            while let Some((action, _)) = self.keyboard.map.consume(ctx, |action| {
                matches!(
                    action,
                    KeyboardAction::OpenQuickSearch | KeyboardAction::ShowKeyboardHelp
                )
            }) {
                self.keyboard.requests.push(ShortcutRequest::Action(action));
            }
        }
        if elsewhere
            && self
                .keyboard
                .chart_id
                .is_some_and(|id| ctx.memory(|memory| memory.had_focus_last_frame(id)))
        {
            while let Some((action, _)) = self.keyboard.map.consume(ctx, |action| {
                matches!(
                    action,
                    KeyboardAction::CycleChartFocus | KeyboardAction::CycleChartFocusBack
                )
            }) {
                self.keyboard.requests.push(ShortcutRequest::Action(action));
            }
        }
        let was_elsewhere = std::mem::replace(&mut self.keyboard.focus_elsewhere, elsewhere);
        if elsewhere
            || egui::Popup::is_any_open(ctx)
            || (was_elsewhere
                && ctx
                    .input(|i| i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Enter)))
        {
            return;
        }
        while let Some((action, event)) = self.keyboard.map.consume(ctx, |_| true) {
            if action == KeyboardAction::PasteDrawing
                && let Event::Paste(text) = event
            {
                self.keyboard
                    .requests
                    .push(ShortcutRequest::PasteDrawing(text));
                continue;
            }
            let rect = if self.last_rendered_price_rect.width().is_finite()
                && self.last_rendered_price_rect.width() > 0.0
            {
                self.last_rendered_price_rect
            } else {
                ctx.content_rect()
            };
            self.apply_keyboard_action(action, rect);
        }
        let text = ctx.input_mut(|input| {
            if input.modifiers.ctrl || input.modifiers.command || input.modifiers.alt {
                return String::new();
            }
            let mut text = String::new();
            input.events.retain(|event| {
                if let Event::Text(value) = event
                    && value
                        .chars()
                        .next()
                        .is_some_and(|c| c.is_ascii_alphanumeric())
                {
                    text.push_str(value);
                    return false;
                }
                true
            });
            text
        });
        if !text.is_empty() {
            let interval = text.starts_with(|c: char| c.is_ascii_digit());
            self.keyboard.requests.push(if interval {
                ShortcutRequest::IntervalEntry(text)
            } else {
                ShortcutRequest::SymbolSearch(text)
            });
        }
    }

    fn apply_keyboard_action(&mut self, action: KeyboardAction, chart_rect: egui::Rect) {
        use crate::scales::PriceScaleMode;
        use KeyboardAction as A;
        match action {
            A::PanLeft | A::PanRight | A::PanLeftFast | A::PanRightFast => {
                self.pending_scroll_time = None;
                self.tracking_mode_active = false;
                let amount = if matches!(action, A::PanLeftFast | A::PanRightFast) {
                    self.state
                        .time_scale()
                        .visible_logical_range()
                        .length()
                        .ceil()
                } else {
                    self.chart_options.keyboard.pan_amount
                };
                self.state.time_scale_mut().scroll_bars(
                    if matches!(action, A::PanLeft | A::PanLeftFast) {
                        -amount
                    } else {
                        amount
                    },
                );
            }
            A::ZoomIn | A::ZoomOut | A::PageUp | A::PageDown => {
                self.pending_scroll_time = None;
                self.tracking_mode_active = false;
                let step = self.chart_options.keyboard.zoom_step;
                let delta = match action {
                    A::ZoomIn => step,
                    A::ZoomOut => -step,
                    A::PageUp => step * 3.0,
                    _ => -step * 3.0,
                };
                let rect = chart_rect;
                self.state.time_scale_mut().zoom(
                    delta,
                    rect.width() / 2.0,
                    rect.left(),
                    rect.width(),
                );
            }
            A::ScrollToRealTime => {
                self.pending_scroll_time = None;
                self.state.time_scale_mut().scroll_to_realtime();
            }
            A::FitContent => {
                self.pending_scroll_time = None;
                self.state.time_scale_mut().fit_content();
            }
            A::ResetChart | A::ResetZoom => {
                self.pending_scroll_time = None;
                self.state
                    .time_scale_mut()
                    .set_bar_spacing(self.chart_options.time_scale.bar_spacing);
                self.state.time_scale_mut().scroll_to_realtime();
                self.state.price_scale_mut().reset();
            }
            A::ToggleLogScale | A::TogglePercentageScale => {
                let mode = if action == A::ToggleLogScale {
                    PriceScaleMode::Logarithmic
                } else {
                    PriceScaleMode::Percentage
                };
                let scale = self.state.price_scale_mut();
                scale.options_mut().mode = if scale.options().mode == mode {
                    PriceScaleMode::Normal
                } else {
                    mode
                };
                scale.reset();
            }
            A::InvertScale => {
                let options = self.state.price_scale_mut().options_mut();
                options.invert_scale = !options.invert_scale;
            }
            A::ToggleAutoScale => {
                let scale = self.state.price_scale_mut();
                if scale.is_manual() {
                    scale.reset();
                } else {
                    let range = scale.range();
                    scale.set_range(range.min, range.max);
                }
            }
            A::ToggleGrid => self.config.show_grid = !self.config.show_grid,
            A::ToggleVolume => self.config.show_volume = !self.config.show_volume,
            A::DrawingTool(tool) => self.drawings.set_active_tool(Some(tool)),
            A::ToggleDrawings => self.drawings.toggle_visibility_all(),
            A::Undo => {
                self.drawings.undo();
            }
            A::Redo => {
                self.drawings.redo();
            }
            A::CompleteDrawing => {
                if self.drawings.active_tool.is_some_and(|tool| {
                    tool.interaction_mode() == crate::drawings::DrawingInteractionMode::MultiPoint
                }) && self.drawings.curr_drawing.is_some()
                {
                    self.drawings.complete_curr_drawing();
                }
            }
            A::DeleteSelectedDrawing => self.drawings.delete_selected(),
            A::Cancel => {
                self.drawings.cancel_curr_drawing();
                self.drawings.deselect();
                self.drawings.set_active_tool(None);
                self.drawings.set_eraser_mode(false);
            }
            _ => self.keyboard.requests.push(ShortcutRequest::Action(action)),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::scales::PriceScaleMode;
    use crate::widget::test_frames::chart;
    use egui::{Key, Modifiers, RawInput};

    /// Modified arrows must not fall through to plain panning. Rebinding and removing a
    /// binding must change real widget input, and typed text must reach host entry dialogs.
    #[test]
    fn configured_shortcuts_keep_modifiers_and_route_host_text() {
        for (key, modifiers, pans) in [
            (Key::ArrowLeft, Modifiers::NONE, true),
            (Key::ArrowLeft, Modifiers::ALT, false),
            (Key::ArrowRight, Modifiers::COMMAND, true),
            (Key::ArrowUp, Modifiers::COMMAND, false),
        ] {
            let ctx = Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            let mut before_offset = 0.0;
            let mut before_spacing = 0.0;
            let mut page = 0.0;
            for pass in 0..3 {
                if pass == 2 {
                    before_offset = chart.state.time_scale().right_offset();
                    before_spacing = chart.state.time_scale().bar_spacing();
                    page = chart
                        .state
                        .time_scale()
                        .visible_logical_range()
                        .length()
                        .ceil();
                }
                ctx.run_ui(
                    RawInput {
                        screen_rect: Some(egui::Rect::from_min_size(
                            egui::Pos2::ZERO,
                            egui::vec2(800.0, 600.0),
                        )),
                        events: if pass == 2 {
                            vec![Event::Key {
                                key,
                                modifiers,
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                            }]
                        } else {
                            vec![]
                        },
                        ..Default::default()
                    },
                    |ui| {
                        let response = chart.show(ui);
                        if pass < 2 {
                            response.request_focus();
                        }
                    },
                )
                .drop_without_applying_deltas();
            }
            if pans {
                let expected = if modifiers == Modifiers::NONE {
                    before_offset + 1.0
                } else {
                    before_offset - page
                };
                assert_eq!(
                    chart.state.time_scale().right_offset(),
                    expected,
                    "{key:?}, {modifiers:?}"
                );
            } else if key == Key::ArrowUp {
                assert!(chart.state.time_scale().bar_spacing() > before_spacing);
            } else {
                assert_eq!(chart.state.time_scale().right_offset(), before_offset);
            }
        }
        let ctx = Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        chart
            .shortcuts_mut()
            .bindings
            .retain(|(_, action)| *action != KeyboardAction::PanLeft);
        chart.shortcuts_mut().bindings.push((
            KeyboardShortcut::new(Modifiers::NONE, Key::F2),
            KeyboardAction::ToggleLogScale,
        ));
        for (event, expected) in [
            (
                Event::Key {
                    key: Key::ArrowLeft,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
                PriceScaleMode::Normal,
            ),
            (
                Event::Key {
                    key: Key::F2,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::NONE,
                },
                PriceScaleMode::Logarithmic,
            ),
            (Event::Text("IBM".into()), PriceScaleMode::Logarithmic),
            (Event::Text("15".into()), PriceScaleMode::Logarithmic),
        ] {
            let before = chart.state.time_scale().right_offset();
            ctx.run_ui(
                RawInput {
                    events: vec![event.clone()],
                    ..Default::default()
                },
                |ui| {
                    chart.show(ui).request_focus();
                },
            )
            .drop_without_applying_deltas();
            assert_eq!(chart.state.price_scale().options().mode, expected);
            assert_eq!(chart.state.time_scale().right_offset(), before);
            if let Event::Text(text) = event {
                let request = if text == "15" {
                    ShortcutRequest::IntervalEntry(text)
                } else {
                    ShortcutRequest::SymbolSearch(text)
                };
                assert_eq!(chart.take_shortcut_requests(), vec![request]);
            }
        }
    }
}
