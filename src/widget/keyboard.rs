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
        self.keys_for(action).next()
    }

    pub(crate) fn keys_for(
        &self,
        action: KeyboardAction,
    ) -> impl Iterator<Item = KeyboardShortcut> + '_ {
        self.bindings
            .iter()
            .enumerate()
            .filter_map(move |(index, (key, bound))| {
                (*bound == action
                    && !self.bindings[..index]
                        .iter()
                        .any(|(earlier, _)| earlier == key))
                .then_some(*key)
            })
    }

    /// Consume the first bound key press, leaving other input for its owner.
    pub(crate) fn consume(
        &self,
        ctx: &Context,
        stop_at_text: bool,
        accepts: impl Fn(KeyboardAction, KeyboardShortcut) -> bool,
    ) -> Option<(KeyboardAction, Event)> {
        ctx.input_mut(|input| {
            let (index, action) = input
                .events
                .iter()
                .enumerate()
                .take_while(|(_, event)| {
                    !stop_at_text || !matches!(event, Event::Text(text) if starts_entry(text))
                })
                .find_map(|(index, event)| {
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
                    let (shortcut, action) = self.bindings.iter().find(|(shortcut, _)| {
                        let matches = modifiers.matches_exact(shortcut.modifiers)
                            || (matches!(key, egui::Key::Plus | egui::Key::Questionmark)
                                && modifiers.matches_logically(shortcut.modifiers)
                                && modifiers.alt == shortcut.modifiers.alt);
                        key == shortcut.logical_key && matches
                    })?;
                    accepts(*action, *shortcut).then_some((index, *action))
                })?;
            let event = input.events.remove(index);
            if let Event::Key { key, .. } = &event {
                let symbol = key.symbol_or_name();
                if symbol.len() == 1
                    && input
                        .events
                        .get(index)
                        .is_some_and(|next| matches!(next, Event::Text(text) if text.eq_ignore_ascii_case(symbol)))
                {
                    input.events.remove(index);
                }
            }
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
            if self.shortcut(*action).is_some()
                && !categories
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

fn starts_entry(text: &str) -> bool {
    text.chars()
        .next()
        .is_some_and(|c| !c.is_control() && !c.is_whitespace())
}

/// A shortcut that needs the host's dialogs, layout or clipboard.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ShortcutRequest {
    /// Execute an application command.
    Action(KeyboardAction),
    /// Paste clipboard text. The host recognizes its drawing format before inserting anything.
    PasteDrawing(String),
    /// Open symbol search with the typed text.
    SymbolSearch(String),
    /// Open interval entry with the typed digits; a comma opens it empty.
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
        if self.keyboard.last_pass == Some(pass) {
            return;
        }
        self.keyboard.last_pass = Some(pass);
        ctx.data_mut(|data| {
            data.insert_temp(
                egui::Id::NULL.with("chart_shortcuts"),
                self.keyboard.map.clone(),
            )
        });
        if self.menu_open {
            self.suppress_shortcuts(ctx);
        }
        if !self.chart_options.keyboard.enabled || shortcuts_suppressed(ctx) {
            return;
        }
        let focused = ctx.memory(|memory| memory.focused());
        let elsewhere = focused.is_some() && focused != self.keyboard.chart_id;
        if elsewhere && !egui::Popup::is_any_open(ctx) {
            let text_focused = ctx.text_edit_focused();
            while let Some((action, _)) =
                self.keyboard.map.consume(ctx, false, |action, shortcut| {
                    matches!(
                        action,
                        KeyboardAction::OpenQuickSearch | KeyboardAction::ShowKeyboardHelp
                    ) && (!text_focused
                        || shortcut.modifiers.command
                        || shortcut.modifiers.ctrl
                        || shortcut.modifiers.alt
                        || shortcut.logical_key == egui::Key::F1)
                })
            {
                self.keyboard.requests.push(ShortcutRequest::Action(action));
            }
        }
        if elsewhere
            && self
                .keyboard
                .chart_id
                .is_some_and(|id| ctx.memory(|memory| memory.had_focus_last_frame(id)))
        {
            while let Some((action, _)) = self.keyboard.map.consume(ctx, false, |action, _| {
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
        while let Some((action, event)) = {
            #[cfg(feature = "drawings")]
            while self.nudge_selected_drawing(ctx) {}
            self.keyboard.map.consume(ctx, true, |_, _| true)
        } {
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
            let pending = self.keyboard.requests.len();
            self.apply_keyboard_action(action, rect);
            if self.keyboard.requests.len() != pending {
                return;
            }
        }
        let text = ctx.input_mut(|input| {
            let Some(index) = input
                .events
                .iter()
                .position(|event| matches!(event, Event::Text(text) if starts_entry(text)))
            else {
                return String::new();
            };
            let Event::Text(text) = input.events.remove(index) else {
                unreachable!()
            };
            text
        });
        if !text.is_empty() {
            // A comma opens interval entry without becoming part of it, as leading digits do.
            self.keyboard
                .requests
                .push(if let Some(rest) = text.strip_prefix(',') {
                    ShortcutRequest::IntervalEntry(rest.to_owned())
                } else if text.starts_with(|c: char| c.is_ascii_digit()) {
                    ShortcutRequest::IntervalEntry(text)
                } else {
                    ShortcutRequest::SymbolSearch(text)
                });
        }
    }

    #[cfg(feature = "drawings")]
    fn nudge_selected_drawing(&mut self, ctx: &Context) -> bool {
        use egui::{Key, Modifiers};
        if !self.drawings.selected_ids().any(|id| {
            self.drawings
                .drawings
                .iter()
                .any(|d| d.id == id && !d.locked)
        }) {
            return false;
        }
        let key = ctx.input_mut(|input| {
            for (index, event) in input.events.iter().enumerate() {
                match event {
                    Event::Text(text) if starts_entry(text) => return None,
                    Event::Copy | Event::Paste(_) => return None,
                    Event::Key {
                        key,
                        modifiers,
                        pressed: true,
                        ..
                    } => {
                        if *modifiers == Modifiers::NONE
                            && matches!(
                                key,
                                Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown
                            )
                        {
                            let key = *key;
                            input.events.remove(index);
                            return Some(key);
                        }
                        if self.keyboard.map.bindings.iter().any(|(shortcut, _)| {
                            shortcut.logical_key == *key
                                && modifiers.matches_exact(shortcut.modifiers)
                        }) {
                            return None;
                        }
                    }
                    _ => {}
                }
            }
            None
        });
        let Some(key) = key else {
            return false;
        };
        let step = self
            .instrument()
            .and_then(|i| i.tick_size.filter(|s| s.is_finite() && *s > 0.0))
            .unwrap_or_else(|| {
                let decimals = self
                    .instrument()
                    .and_then(crate::scales::instrument_price_precision)
                    .unwrap_or(2);
                10.0_f64.powi(-i32::from(decimals))
            });
        let (bars, price) = match key {
            Key::ArrowLeft => (-1.0, 0.0),
            Key::ArrowRight => (1.0, 0.0),
            Key::ArrowUp => (0.0, step),
            _ => (0.0, -step),
        };
        let main_map = self.get_chart_mapping();
        let pane_maps: Vec<_> = self
            .last_rendered_indicator_panes
            .iter()
            .filter_map(|pane| Some((self.study_ids.get(pane.indicator_idx)?.0, pane.mapping)))
            .collect();
        self.user_action(|chart| {
            chart.drawings.edit_selected(|drawing| {
                if let Some([x, y]) = drawing.screen_anchor
                    && let Some(anchor) = drawing.chart_points.first()
                {
                    let map = pane_maps
                        .iter()
                        .find(|(id, _)| Some(*id) == drawing.pane)
                        .map_or(&main_map, |(_, map)| map);
                    drawing.screen_anchor = Some([
                        x + bars * map.bar_spacing / map.rect.width(),
                        y + (map.price_to_y(anchor.price + price) - map.price_to_y(anchor.price))
                            / map.rect.height(),
                    ]);
                }
                for point in &mut drawing.chart_points {
                    point.bar_idx += bars;
                    point.price += price;
                }
            });
        });
        true
    }

    fn apply_keyboard_action(&mut self, action: KeyboardAction, chart_rect: egui::Rect) {
        match action {
            KeyboardAction::Undo => {
                self.undo();
            }
            KeyboardAction::Redo => {
                self.redo();
            }
            _ => self.user_action(|chart| chart.apply_keyboard_edit(action, chart_rect)),
        }
    }

    fn apply_keyboard_edit(&mut self, action: KeyboardAction, chart_rect: egui::Rect) {
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
                let anchor = self.zoom_anchor(rect.width() / 2.0, rect.width());
                self.state
                    .time_scale_mut()
                    .zoom(delta, anchor, rect.left(), rect.width());
            }
            A::ScrollToRealTime => self.scroll_to_real_time(true),
            A::ScrollToFirstBar => {
                // A range spans `length() + 1` bars, so its left index becomes 0.
                let scale = self.state.time_scale();
                let position = scale.visible_logical_range().length() - scale.base_idx() as f32;
                self.scroll_to_position(position, true);
            }
            A::FitContent => {
                self.pending_scroll_time = None;
                self.state.time_scale_mut().fit_content();
            }
            A::ResetChart | A::ResetZoom => self.reset_chart_view(),
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
            #[cfg(feature = "drawings")]
            A::DrawingTool(tool) => self.drawings.set_active_tool(Some(tool)),
            #[cfg(feature = "drawings")]
            A::ToggleDrawings => self.drawings.toggle_visibility_all(),
            #[cfg(feature = "drawings")]
            A::CompleteDrawing => {
                if self.drawings.active_tool.is_some_and(|tool| {
                    tool.interaction_mode() == crate::drawings::DrawingInteractionMode::MultiPoint
                }) && self.drawings.curr_drawing.is_some()
                {
                    self.drawings.complete_curr_drawing();
                }
            }
            // Without a selected drawing, the key removes the selected study.
            #[cfg(feature = "drawings")]
            A::DeleteSelectedDrawing => match self.selection.selected_id() {
                Some(super::ChartElementId::Indicator(id))
                    if self.drawings.selected_ids().next().is_none() =>
                {
                    self.remove_series(id);
                }
                _ => self.drawings.delete_selected(),
            },
            #[cfg(feature = "drawings")]
            A::Cancel => {
                self.drawings.cancel_curr_drawing();
                self.drawings.deselect();
                self.drawings.set_active_tool(None);
                self.drawings.set_eraser_mode(false);
            }
            #[cfg(not(feature = "drawings"))]
            A::DeleteSelectedDrawing => {
                if let Some(super::ChartElementId::Indicator(id)) = self.selection.selected_id() {
                    self.remove_series(id);
                }
            }
            #[cfg(not(feature = "drawings"))]
            A::DrawingTool(_) | A::ToggleDrawings | A::CompleteDrawing | A::Cancel => {}
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

    #[cfg(feature = "drawings")]
    #[test]
    fn arrows_nudge_selected_drawings_with_one_undo_per_press() {
        use crate::drawings::{ChartPoint, Drawing, DrawingToolType};
        use crate::widget::test_frames::frame;
        for (locked, selected) in [(false, true), (true, true), (false, false)] {
            let ctx = Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            chart.set_instrument(Some(
                crate::model::Instrument::new("TEST").with_tick_size(0.25),
            ));
            let mut drawing = Drawing::new(7, DrawingToolType::TrendLine);
            drawing.chart_points = vec![ChartPoint::new(30.0, 14.0), ChartPoint::new(70.0, 17.0)];
            drawing.completed = true;
            drawing.locked = locked;
            chart.drawings_mut().drawings.push(drawing);
            if selected {
                chart.drawings_mut().select(7);
            }
            frame(&ctx, &mut chart, 0.0, vec![]);
            let view = chart.get_time_scale_state();
            for (i, (key, bars, price)) in [
                (Key::ArrowLeft, -1.0, 0.0),
                (Key::ArrowRight, 1.0, 0.0),
                (Key::ArrowUp, 0.0, 0.25),
                (Key::ArrowDown, 0.0, -0.25),
            ]
            .into_iter()
            .enumerate()
            {
                let before = chart.drawings().drawings[0].chart_points.clone();
                frame(
                    &ctx,
                    &mut chart,
                    1.0 + i as f64,
                    vec![Event::Key {
                        key,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    }],
                );
                for (old, point) in before
                    .iter()
                    .zip(&chart.drawings().drawings[0].chart_points)
                {
                    assert_eq!(
                        point.bar_idx,
                        old.bar_idx + if selected && !locked { bars } else { 0.0 }
                    );
                    assert_eq!(
                        point.price,
                        old.price + if selected && !locked { price } else { 0.0 }
                    );
                }
                if selected && !locked {
                    assert_eq!(chart.get_time_scale_state(), view);
                    assert!(chart.undo());
                    assert_eq!(chart.drawings().drawings[0].chart_points, before);
                    assert!(!chart.can_undo());
                    assert!(chart.redo());
                    assert!(chart.undo());
                } else if i < 2 {
                    assert_ne!(chart.get_time_scale_state(), view);
                    assert!(chart.undo());
                }
            }
        }
    }

    #[test]
    fn reset_chart_restores_time_and_every_price_scale() {
        use crate::model::SeriesScale;
        use crate::widget::test_frames::frame;
        for (spacing, margin, pixels, expected_offset) in [
            (6.0, 2.5, None, 2.5),
            (9.0, 4.0, None, 4.0),
            (8.0, 2.5, Some(64.0), 8.0),
        ] {
            for action in [
                Some(KeyboardAction::ResetChart),
                Some(KeyboardAction::ResetZoom),
                None,
            ] {
                let ctx = Context::default();
                let mut chart = chart(&[100.0], &[200.0]);
                let mut options = chart.chart_options().clone();
                options.time_scale.bar_spacing = spacing;
                options.time_scale.right_offset = margin;
                options.time_scale.right_offset_pixels = pixels;
                chart = chart.with_chart_options(options);
                let id = chart.add_series(crate::widget::SeriesOptions {
                    scale: SeriesScale::Left,
                    ..Default::default()
                });
                let pane = chart.add_pane();
                chart.set_preserve_empty_pane(pane, true);
                frame(&ctx, &mut chart, 0.0, vec![]);
                chart.state.time_scale_mut().set_bar_spacing(20.0);
                chart.state.time_scale_mut().set_right_offset(-30.0);
                chart.price_scale_mut().set_range(150.0, 150.4);
                chart
                    .series_price_scale_mut(id)
                    .unwrap()
                    .set_range(2.0, 3.0);
                chart
                    .pane_layout
                    .panes
                    .last_mut()
                    .unwrap()
                    .scale
                    .set_range(40.0, 50.0);
                chart.set_visible_bars(5);
                chart.scroll_to_position(-50.0, true);
                chart.scroll_to_time(chart.state.shown()[10].time);
                if let Some(action) = action {
                    chart.apply_keyboard_action(action, chart.get_rendered_price_rect());
                } else {
                    chart.user_action(|chart| chart.reset_chart_view());
                }
                for step in 0..3 {
                    if step == 1 {
                        assert!(chart.undo());
                    } else if step == 2 {
                        assert!(chart.redo());
                    }
                    frame(&ctx, &mut chart, 0.1 + step as f64 * 0.1, vec![]);
                    let manual = step == 1;
                    if step == 0 {
                        assert_eq!(chart.get_time_scale_state(), (spacing, expected_offset));
                    }
                    assert_eq!(chart.state.price_scale().is_manual(), manual);
                    assert_eq!(chart.series_price_scale(id).unwrap().is_manual(), manual);
                    assert!(
                        chart
                            .pane_layout
                            .panes
                            .iter()
                            .skip(1)
                            .all(|pane| pane.scale.is_manual() == manual)
                    );
                }
            }
        }
    }

    #[test]
    fn navigation_and_panel_keys_follow_the_shared_action_map() {
        use KeyboardAction as A;
        let alt_shift = Modifiers::ALT.plus(Modifiers::SHIFT);
        for (key, modifiers, expected) in [
            (Key::ArrowLeft, Modifiers::NONE, Some(A::PanLeft)),
            (Key::ArrowLeft, Modifiers::ALT, None),
            (Key::ArrowRight, Modifiers::COMMAND, Some(A::PanRightFast)),
            (Key::ArrowUp, Modifiers::COMMAND, Some(A::ZoomIn)),
            (Key::ArrowLeft, alt_shift, Some(A::ScrollToFirstBar)),
            (Key::Home, Modifiers::NONE, Some(A::ScrollToFirstBar)),
            (Key::ArrowRight, alt_shift, Some(A::ScrollToRealTime)),
            (Key::End, Modifiers::NONE, Some(A::ScrollToRealTime)),
            (Key::Period, Modifiers::NONE, Some(A::OpenLayout)),
            (Key::D, Modifiers::ALT, Some(A::OpenDataWindow)),
            (Key::E, alt_shift, Some(A::ToggleSession)),
        ] {
            let ctx = Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            let latest = chart.state.time_scale().live_edge_offset();
            let mut before_offset = 0.0;
            let mut before_spacing = 0.0;
            let mut page = 0.0;
            for (pass, time) in [0.0, 0.1, 0.2, 0.3, 2.0, 2.1].into_iter().enumerate() {
                if pass == 2 {
                    // Zoom into history so neither end is already in view.
                    chart.state.time_scale_mut().set_bar_spacing(20.0);
                    chart.scroll_to_position(-40.0, false);
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
                        time: Some(time),
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
            let requests = chart.take_shortcut_requests();
            let offset = chart.state.time_scale().right_offset();
            let spacing = chart.state.time_scale().bar_spacing();
            match expected {
                Some(A::PanLeft) => assert_eq!(offset, before_offset + 1.0),
                Some(A::PanRightFast) => assert_eq!(offset, before_offset - page),
                Some(A::ZoomIn) => assert!(spacing > before_spacing),
                Some(A::ScrollToFirstBar) => {
                    let left = chart.visible_logical_range().unwrap().left;
                    assert!(left.abs() < 1e-3, "{key:?}: left edge at bar {left}");
                }
                Some(A::ScrollToRealTime) => assert_eq!(offset, latest, "{key:?}"),
                Some(action) => {
                    assert_eq!(requests, vec![ShortcutRequest::Action(action)], "{key:?}")
                }
                None => assert_eq!(offset, before_offset),
            }
            if expected != Some(A::ZoomIn) {
                assert_eq!(spacing, before_spacing, "{key:?} keeps the zoom");
            }
        }
    }

    #[test]
    fn rebinding_changes_chart_input_and_bound_characters_do_not_start_entry() {
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
            (Event::Text(",".into()), PriceScaleMode::Logarithmic),
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
                } else if text == "," {
                    ShortcutRequest::IntervalEntry(String::new())
                } else {
                    ShortcutRequest::SymbolSearch(text)
                };
                assert_eq!(chart.take_shortcut_requests(), vec![request]);
            }
        }
        chart.shortcuts_mut().bindings.push((
            KeyboardShortcut::new(Modifiers::NONE, Key::F),
            KeyboardAction::FitContent,
        ));
        ctx.run_ui(
            RawInput {
                events: vec![
                    Event::Key {
                        key: Key::F,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    },
                    Event::Text("f".into()),
                ],
                ..Default::default()
            },
            |ui| {
                chart.show(ui);
            },
        )
        .drop_without_applying_deltas();
        assert!(
            chart.take_shortcut_requests().is_empty(),
            "a bound character must not also open symbol search"
        );
    }

    #[test]
    fn text_fields_keep_typing_and_panel_keys_but_command_search_still_opens() {
        let ctx = Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        let mut text = "label".to_owned();
        for pass in 0..8 {
            if pass == 7 {
                chart.shortcuts_mut().bindings.insert(
                    0,
                    (
                        KeyboardShortcut::new(Modifiers::COMMAND, Key::K),
                        KeyboardAction::FitContent,
                    ),
                );
            }
            let map = chart.shortcuts();
            let visible = map
                .by_category()
                .into_iter()
                .flat_map(|(_, actions)| actions)
                .any(|action| action == KeyboardAction::OpenQuickSearch);
            assert_eq!(
                visible,
                pass != 7,
                "help must not advertise a shadowed binding"
            );
            assert_eq!(
                map.shortcut(KeyboardAction::OpenQuickSearch).is_some(),
                pass != 7
            );
            let events = match pass {
                2 => vec![
                    Event::Key {
                        key: Key::Questionmark,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::SHIFT,
                    },
                    Event::Text("?".into()),
                ],
                3 | 7 => vec![Event::Key {
                    key: Key::K,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: Modifiers::COMMAND,
                }],
                4 => vec![
                    Event::Key {
                        key: Key::Period,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::NONE,
                    },
                    Event::Text(".".into()),
                ],
                5 | 6 => vec![Event::Key {
                    key: if pass == 5 { Key::D } else { Key::E },
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: if pass == 5 {
                        Modifiers::ALT
                    } else {
                        Modifiers::ALT.plus(Modifiers::SHIFT)
                    },
                }],
                _ => vec![],
            };
            ctx.run_ui(
                RawInput {
                    events,
                    ..Default::default()
                },
                |ui| {
                    chart.dispatch_shortcuts(ui.ctx());
                    let field = ui.text_edit_singleline(&mut text);
                    chart.show(ui);
                    if pass < 2 {
                        field.request_focus();
                    }
                },
            )
            .drop_without_applying_deltas();
            let expected = if pass == 3 {
                vec![ShortcutRequest::Action(KeyboardAction::OpenQuickSearch)]
            } else {
                vec![]
            };
            assert_eq!(
                chart.take_shortcut_requests(),
                expected,
                "field pass {pass}"
            );
        }
        assert_eq!(text, "label?.");
    }

    #[test]
    #[cfg(feature = "drawings")]
    fn typed_entry_keeps_following_edit_keys() {
        let ctx = Context::default();
        let mut chart = crate::widget::test_frames::chart(&[10.0], &[20.0]);
        chart
            .drawings_mut()
            .drawings
            .push(crate::drawings::Drawing::new(
                7,
                crate::drawings::DrawingToolType::TrendLine,
            ));
        chart.drawings_mut().select(7);
        for pass in 0..2 {
            ctx.run_ui(
                RawInput {
                    events: if pass == 1 {
                        vec![
                            Event::Text("AB".into()),
                            Event::Key {
                                key: Key::Backspace,
                                physical_key: None,
                                pressed: true,
                                repeat: false,
                                modifiers: Modifiers::NONE,
                            },
                        ]
                    } else {
                        vec![]
                    },
                    ..Default::default()
                },
                |ui| {
                    chart.show(ui).request_focus();
                },
            )
            .drop_without_applying_deltas();
        }
        assert_eq!(
            chart.drawings().drawings.len(),
            1,
            "text entry owns subsequent keys in the same input batch"
        );
        assert_eq!(
            chart.take_shortcut_requests(),
            vec![ShortcutRequest::SymbolSearch("AB".into())]
        );
        assert!(
            ctx.input(|input| input.key_pressed(Key::Backspace)),
            "the host entry field keeps the following edit key"
        );
    }

    #[test]
    fn opening_search_keeps_following_text() {
        let ctx = Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        ctx.run_ui(
            RawInput {
                events: vec![
                    Event::Key {
                        key: Key::K,
                        physical_key: None,
                        pressed: true,
                        repeat: false,
                        modifiers: Modifiers::COMMAND,
                    },
                    Event::Text("RSI".into()),
                ],
                ..Default::default()
            },
            |ui| chart.dispatch_shortcuts(ui.ctx()),
        )
        .drop_without_applying_deltas();
        assert_eq!(
            chart.take_shortcut_requests(),
            vec![ShortcutRequest::Action(KeyboardAction::OpenQuickSearch)]
        );
        assert!(
            ctx.input(|input| input.events.contains(&Event::Text("RSI".into()))),
            "opening a host dialog hands the remaining text to it"
        );
    }
}
