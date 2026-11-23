use super::{Chart, Pane, PaneId, SeriesId};
use crate::config::NavigationButtons;
use crate::localization as l10n;
use egui::{Pos2, Rect, Sense, Ui};

impl Chart {
    /// Whether this frame is a Ctrl/Command double-click that collapses the pointed pane. Pane
    /// and chart handlers drop their own double-click on it. A maximised pane has no other pane
    /// to collapse, and a point outside every pane such as the time axis has none to name, so
    /// both keep the ordinary double-click.
    pub(in crate::widget) fn pane_collapse_click(&self, ctx: &egui::Context) -> bool {
        self.has_panes()
            && self.pane_layout.maximized.is_none()
            && self.pointer_commands_enabled(ctx)
            && ctx.input(|input| {
                input.modifiers.command
                    && !input.modifiers.alt
                    && !input.modifiers.shift
                    && input
                        .pointer
                        .button_double_clicked(egui::PointerButton::Primary)
                    && input.pointer.interact_pos().is_some_and(|pos| {
                        self.pane_layout
                            .panes
                            .iter()
                            .any(|pane| pane.rect.contains(pos))
                    })
            })
    }

    /// Collapses or restores the pane under the pointer, as one undoable edit that keeps its
    /// stretch, and focuses the chart.
    pub(super) fn pane_collapse_shortcut(&mut self, ui: &Ui) {
        if !ui.is_enabled() || !self.pane_collapse_click(ui.ctx()) {
            return;
        }
        let pos = ui.input(|input| input.pointer.interact_pos());
        let pane = self.pane_layout.panes.iter().position(|pane| {
            pos.is_some_and(|pos| pane.rect.contains(pos)) && ui.rect_contains_pointer(pane.rect)
        });
        if let Some(at) = pane {
            let collapsed = self.pane_layout.panes[at].rect.height() <= 24.0;
            self.user_action(|chart| chart.pane_layout.panes[at].collapsed = Some(!collapsed));
            if let Some(id) = self.keyboard.chart_id {
                ui.memory_mut(|memory| memory.request_focus(id));
            }
            ui.ctx().request_repaint();
        }
    }

    /// Header row of a collapsed price pane; a click restores it.
    pub(super) fn collapsed_price_header(&mut self, ui: &mut Ui, rect: Rect) {
        let response = ui.interact(rect, ui.scope_id().with("collapsed_price"), Sense::click());
        ui.painter().text(
            rect.left_center() + egui::vec2(8.0, 0.0),
            egui::Align2::LEFT_CENTER,
            &self.symbol,
            egui::TextStyle::Body.resolve(ui.style()),
            ui.visuals().text_color(),
        );
        let label = l10n::text(ui.ctx(), "Expand pane");
        response.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &label));
        if response.clicked() && !ui.input(|input| input.modifiers.command) {
            let id = self.main_series_pane();
            self.user_action(|chart| {
                if let Some(pane) = chart
                    .pane_layout
                    .panes
                    .iter_mut()
                    .find(|pane| pane.id == id)
                {
                    pane.collapsed = Some(false);
                }
            });
        }
        response.on_hover_text(label);
    }

    pub(super) fn pane_scale_interaction(
        &mut self,
        ui: &mut Ui,
        idx: usize,
        rect: Rect,
        plot: Rect,
    ) {
        if rect.height() <= 24.0 {
            return;
        }
        let axes = self.pane_axes(self.pane_layout.panes[idx].id, rect, plot);
        for (key, axis) in axes {
            let pane_id = self.pane_layout.panes[idx].id;
            let response = ui
                .interact(
                    axis,
                    ui.scope_id().with((pane_id, key, "price_axis")),
                    Sense::click_and_drag(),
                )
                .on_hover_cursor(self.price_axis_cursor());
            let gestures = self.price_axis_gestures(ui, &response, Rect::EVERYTHING);
            if gestures.reset {
                self.user_action(|chart| chart.pane_price_scale_mut(pane_id, key).reset());
                ui.ctx().request_repaint();
            }
            // The pane is already painted: show the zoomed scale on the next frame.
            if gestures.zoom(
                self.pane_price_scale_mut(pane_id, key),
                rect.top(),
                rect.bottom(),
            ) {
                ui.ctx().request_repaint();
            }
        }
    }

    /// Whether the buttons of the pane drawn in `rect` show, by the configured visibility.
    fn pane_buttons_visible(&self, ui: &Ui, rect: Rect) -> bool {
        match self.config.pane_buttons {
            NavigationButtons::Always => true,
            NavigationButtons::OnHover => {
                ui.rect_contains_pointer(rect) || egui::Popup::is_any_open(ui.ctx())
            }
            NavigationButtons::Never => false,
        }
    }

    pub(in crate::widget) fn main_pane_actions(&mut self, ui: &mut Ui, rect: Rect, plot: Rect) {
        if rect.height() < 48.0 {
            return;
        }
        let hovered = self.pane_buttons_visible(ui, rect);
        let rect = Rect::from_min_max(rect.min + egui::vec2(0.0, 24.0), rect.max);
        if self.has_panes() {
            self.pane_maximize_button(ui, self.main_series_pane(), rect, plot);
        }
        let movable: Vec<_> = self
            .pane_series(self.main_series_pane())
            .into_iter()
            .filter(|id| {
                *id != SeriesId::MAIN
                    && (!self.study_ids.contains(id) || self.indicator_user_edit_enabled(*id))
            })
            .collect();
        let pane = self
            .has_panes()
            .then(|| {
                let id = self.main_series_pane();
                self.pane_layout.panes.iter().position(|pane| pane.id == id)
            })
            .flatten();
        if (movable.is_empty() && pane.is_none()) || !hovered {
            return;
        }
        let [above, below] =
            [false, true].map(|down| pane.and_then(|at| self.drawn_neighbour(at, down)));
        // A maximized pane hides the stack, so these entries would change only hidden state.
        let stacked = self.pane_layout.maximized.is_none();
        let button = Rect::from_min_size(
            Pos2::new(plot.right() - 42.0, rect.top() + 2.0),
            egui::vec2(20.0, 20.0),
        );
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .id_salt("main_pane_actions")
                .max_rect(button),
        );
        let mut moved = None;
        let mut command = None;
        egui::containers::menu::MenuButton::from_button(
            egui::Button::new("...").frame_when_inactive(false),
        )
        .ui(&mut child, |ui| {
            if pane.is_some() {
                for (label, enabled, pick) in [
                    ("Move pane up", stacked && above.is_some(), 0),
                    ("Move pane down", stacked && below.is_some(), 1),
                    ("Collapse pane", stacked, 2),
                ] {
                    if ui
                        .add_enabled(enabled, egui::Button::new(l10n::text(ui.ctx(), label)))
                        .clicked()
                    {
                        command = Some(pick);
                        ui.close();
                    }
                }
                if !movable.is_empty() {
                    ui.separator();
                }
            }
            for id in movable {
                ui.menu_button(l10n::text(ui.ctx(), self.member_name(id)), |ui| {
                    if ui
                        .button(l10n::text(ui.ctx(), "Move to new pane"))
                        .clicked()
                    {
                        moved = Some((id, None));
                        ui.close();
                    }
                    for pane in &self.pane_layout.panes {
                        if pane.id == PaneId::MAIN || !self.pane_listed(pane) {
                            continue;
                        }
                        let title = pane
                            .members
                            .first()
                            .map_or("empty", |member| self.member_name(*member));
                        if ui
                            .button(l10n::format_text(
                                ui.ctx(),
                                "Move to {title} pane",
                                &[("title", title.to_string())],
                            ))
                            .clicked()
                        {
                            moved = Some((id, Some(pane.id)));
                            ui.close();
                        }
                    }
                });
            }
        });
        if let (Some(at), Some(command)) = (pane, command) {
            self.user_action(|chart| match command {
                0 => chart.shift_pane(at, above.unwrap()),
                1 => chart.shift_pane(at, below.unwrap()),
                _ => chart.pane_layout.panes[at].collapsed = Some(true),
            });
            ui.ctx().request_repaint();
        }
        if let Some((id, pane)) = moved {
            self.user_action(|chart| match pane {
                Some(pane) => chart.move_series_to_pane(id, pane),
                None => chart.move_to_new_pane(id),
            });
        }
    }

    fn pane_maximize_button(&mut self, ui: &mut Ui, id: PaneId, rect: Rect, plot: Rect) {
        if rect.height() < 24.0 || self.config.pane_buttons == NavigationButtons::Never {
            return;
        }
        let button = Rect::from_min_size(
            Pos2::new(plot.right() - 22.0, rect.top() + 2.0),
            egui::vec2(20.0, 20.0),
        );
        let maximized = self.pane_layout.maximized == Some(id);
        let response = ui
            .interact(button, ui.scope_id().with((id, "maximize")), Sense::click())
            .on_hover_text(l10n::text(
                ui.ctx(),
                if maximized {
                    "Restore panes"
                } else {
                    "Maximize pane"
                },
            ));
        if self.config.pane_buttons == NavigationButtons::Always
            || response.hovered()
            || maximized
            || ui.rect_contains_pointer(rect)
        {
            let color = ui.visuals().text_color();
            let icon = button.shrink(5.0);
            if maximized {
                ui.painter().rect_stroke(
                    icon.translate(egui::vec2(2.0, -2.0)),
                    0.0,
                    egui::Stroke::new(1.0, color),
                    egui::StrokeKind::Inside,
                );
            }
            ui.painter().rect_stroke(
                icon,
                0.0,
                egui::Stroke::new(1.0, color),
                egui::StrokeKind::Inside,
            );
        }
        if response.clicked() {
            self.user_action(|chart| {
                chart.pane_layout.maximized = if maximized { None } else { Some(id) };
            });
            ui.ctx().request_repaint();
        }
    }

    pub(super) fn pane_actions(
        &mut self,
        ui: &mut Ui,
        pane_idx: usize,
        id: PaneId,
        rect: Rect,
        plot: Rect,
        collapsed: bool,
    ) {
        let visible = self.pane_buttons_visible(ui, rect);
        let [drawn_above, drawn_below] =
            [false, true].map(|down| self.drawn_neighbour(pane_idx, down));
        // As on the price pane, the top row is left to the corner menu and link controls of a
        // `ChartLayout`.
        let rect = if drawn_above.is_none() && rect.height() >= 48.0 {
            Rect::from_min_max(rect.min + egui::vec2(0.0, 24.0), rect.max)
        } else {
            rect
        };
        self.pane_maximize_button(ui, id, rect, plot);
        let button = Rect::from_min_size(
            Pos2::new(plot.right() - 42.0, rect.top() + 2.0),
            egui::vec2(20.0, 20.0),
        );
        let mut action = None;
        let mut move_member = None;
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .id_salt((id, "pane_actions"))
                .max_rect(button),
        );
        // The menu stays in the frame while hidden, so a menu opened in one pane survives its neighbours.
        if !visible {
            child.set_invisible();
        }
        let panes = &self.pane_layout.panes;
        let main_pane = panes
            .iter()
            .any(|pane| pane.id == PaneId::MAIN)
            .then_some(PaneId::MAIN)
            .or_else(|| self.series_pane(SeriesId::MAIN));
        let above = pane_idx.checked_sub(1).map(|at| panes[at].id);
        let below = panes.get(pane_idx + 1).map(|pane| pane.id);
        // A maximized pane hides the stack, so these entries would change only hidden state.
        let stacked = self.pane_layout.maximized.is_none();
        child.menu_button("...", |ui| {
            let members = self.pane_series(id);
            let editable = members.iter().all(|member| {
                !self.study_ids.contains(member) || self.indicator_user_edit_enabled(*member)
            });
            for (label, enabled, command) in [
                ("Move pane up", stacked && drawn_above.is_some(), 0),
                ("Move pane down", stacked && drawn_below.is_some(), 1),
                (
                    if collapsed {
                        "Expand pane"
                    } else {
                        "Collapse pane"
                    },
                    stacked,
                    2,
                ),
                (
                    "Merge with pane above",
                    editable && above.is_some_and(|pane| pane != PaneId::MAIN),
                    3,
                ),
                (
                    "Merge with pane below",
                    editable && below.is_some_and(|pane| pane != PaneId::MAIN),
                    4,
                ),
                ("Remove pane", editable && panes.len() > 1, 5),
            ] {
                if ui
                    .add_enabled(enabled, egui::Button::new(l10n::text(ui.ctx(), label)))
                    .clicked()
                {
                    action = Some(command);
                    ui.close();
                }
            }
            if !members.is_empty() {
                ui.separator();
            }
            for member in &members {
                let editable =
                    !self.study_ids.contains(member) || self.indicator_user_edit_enabled(*member);
                ui.menu_button(l10n::text(ui.ctx(), self.member_name(*member)), |ui| {
                    if ui
                        .add_enabled(
                            editable && main_pane.is_some(),
                            egui::Button::new(l10n::text(ui.ctx(), "Move to main pane")),
                        )
                        .clicked()
                    {
                        action = Some(6);
                        move_member = Some(*member);
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            editable && members.len() > 1,
                            egui::Button::new(l10n::text(ui.ctx(), "Move to new pane")),
                        )
                        .clicked()
                    {
                        action = Some(7);
                        move_member = Some(*member);
                        ui.close();
                    }
                });
            }
        });
        if action.is_some() {
            self.begin_user_action();
        }
        match action {
            Some(0) => self.shift_pane(pane_idx, drawn_above.unwrap()),
            Some(1) => self.shift_pane(pane_idx, drawn_below.unwrap()),
            Some(2) => self.pane_layout.panes[pane_idx].collapsed = Some(!collapsed),
            Some(3 | 4) => {
                let target = if action == Some(3) {
                    pane_idx - 1
                } else {
                    pane_idx + 1
                };
                let members = std::mem::take(&mut self.pane_layout.panes[pane_idx].members);
                self.pane_layout.panes[target].members.extend(members);
                if !self.pane_layout.panes[pane_idx].preserve {
                    self.pane_layout.panes.remove(pane_idx);
                }
                self.sync_panes();
            }
            Some(5) => {
                self.remove_pane(id);
            }
            Some(6) => {
                self.move_series_to_pane(move_member.unwrap(), main_pane.unwrap());
            }
            Some(7) => {
                self.move_to_new_pane(move_member.unwrap());
            }
            _ => {}
        }
        if action.is_some() {
            self.end_user_action();
            ui.ctx().request_repaint();
        }
    }

    /// Moves the pane at `at` to its nearest drawn neighbour at `to`. Hidden panes between
    /// them keep their place next to the panes not moved.
    fn shift_pane(&mut self, at: usize, to: usize) {
        let panes = &mut self.pane_layout.panes;
        if to < at {
            panes[to..=at].rotate_right(1);
        } else {
            panes[at..=to].rotate_left(1);
        }
    }

    /// The index of the nearest pane drawn below or above the pane at `at`.
    fn drawn_neighbour(&self, at: usize, down: bool) -> Option<usize> {
        let panes = &self.pane_layout.panes;
        let drawn = |idx: &usize| self.pane_drawn(&panes[*idx]);
        if down {
            (at + 1..panes.len()).find(drawn)
        } else {
            (0..at).rev().find(drawn)
        }
    }

    /// Separators between consecutive drawn panes, given top to bottom as the pane index, its
    /// panel rect and whether it is collapsed.
    pub(super) fn pane_separators(&mut self, ui: &mut Ui, panes: &[(usize, Rect, bool)]) {
        if self.pane_layout.maximized.is_some() {
            return;
        }
        let options = self.pane_options;
        for pair in panes.windows(2) {
            let [(upper_idx, upper, upper_collapsed), (idx, rect, collapsed)] = [pair[0], pair[1]];
            let line = Rect::from_min_max(
                Pos2::new(rect.left(), upper.bottom()),
                Pos2::new(rect.right(), rect.top()),
            );
            let response = ui.interact(
                line.expand2(egui::vec2(0.0, 3.0)),
                ui.scope_id()
                    .with((self.pane_layout.panes[idx].id, "separator")),
                if options.resizable {
                    Sense::click_and_drag()
                } else {
                    Sense::hover()
                },
            );
            use crate::theme::ThemeExt;
            let color = if options.resizable && (response.hovered() || response.dragged()) {
                options
                    .separator_hover_color
                    .unwrap_or(ui.visuals().selection.bg_fill)
            } else {
                options
                    .separator_color
                    .unwrap_or(ui.active_theme().chart.grid_line)
            };
            ui.painter().rect_filled(line, 0.0, color);
            if !options.resizable {
                continue;
            }
            let response = response.on_hover_cursor(egui::CursorIcon::ResizeVertical);
            // Heights follow the whole drag from the press, so movement past a limit is kept.
            if response.drag_started() {
                ui.data_mut(|data| data.insert_temp(response.id, (upper.height(), rect.height())));
            }
            if response.dragged()
                && !collapsed
                && !upper_collapsed
                && let Some(((upper_height, height), total)) = ui
                    .data(|data| data.get_temp::<(f32, f32)>(response.id))
                    .zip(response.total_drag_delta())
            {
                let delta = total.y.clamp(
                    40.0 - upper_height,
                    (height - 40.0).max(40.0 - upper_height),
                );
                // The other panes keep their heights: these two exchange pixels.
                self.pane_layout.panes[upper_idx].height = Some(upper_height + delta);
                self.pane_layout.panes[idx].height = Some(height - delta);
                ui.ctx().request_repaint();
            }
            if response.double_clicked() && !self.pane_collapse_click(ui.ctx()) {
                for at in [upper_idx, idx] {
                    let pane = &mut self.pane_layout.panes[at];
                    pane.height = None;
                    pane.stretch = Pane::default_stretch(pane.id);
                }
                ui.ctx().request_repaint();
            }
        }
    }
}

#[cfg(all(test, feature = "studies", feature = "drawings"))]
mod tests {
    use super::*;
    use egui::{Context, Event, Modifiers, PointerButton, RawInput};

    /// The price pane's own menu moves and collapses it in one undoable step and follows the
    /// pane button visibility setting; the collapsed header expands it again.
    #[test]
    fn price_pane_moves_and_collapses_from_its_own_menu() {
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        for visibility in [NavigationButtons::Never, NavigationButtons::OnHover] {
            let mut chart = crate::widget::test_frames::chart(&[100.0, 105.0], &[110.0, 115.0]);
            let rsi = chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
            let rsi_pane = chart.series_pane(rsi).unwrap();
            chart.config_mut().show_volume = false;
            chart.config_mut().pane_buttons = visibility;
            let mut harness = Harness::builder()
                .with_size(egui::vec2(800.0, 600.0))
                .allow_missing_glyphs()
                .with_accessibility_check(false)
                .build_ui_state(
                    |ui, chart: &mut Chart| {
                        chart.show(ui);
                    },
                    chart,
                );
            harness.run();
            let rect = harness.state().pane_layout.panes[0].rect;
            harness.hover_at(rect.center());
            harness.run();
            let menus: Vec<_> = harness
                .query_all_by_label("...")
                .filter(|node| rect.contains(node.rect().center()))
                .collect();
            if visibility == NavigationButtons::Never {
                assert!(menus.is_empty());
                continue;
            }
            assert_eq!(menus.len(), 1);
            menus[0].click();
            harness.run();
            assert!(
                harness
                    .get_by_label("Move pane up")
                    .accesskit_node()
                    .is_disabled()
            );
            harness.get_by_label("Move pane down").click();
            harness.run();
            assert_eq!(harness.state().panes(), [rsi_pane, PaneId::MAIN]);
            assert!(harness.state_mut().undo());
            assert_eq!(harness.state().panes(), [PaneId::MAIN, rsi_pane]);
            assert!(!harness.state_mut().undo());
            assert!(harness.state_mut().redo());
            harness.run();
            assert_eq!(harness.state().panes(), [rsi_pane, PaneId::MAIN]);
            let rect = harness.state().pane_layout.panes[1].rect;
            harness.hover_at(rect.center());
            harness.run();
            harness
                .query_all_by_label("...")
                .find(|node| rect.contains(node.rect().center()))
                .unwrap()
                .click();
            harness.run();
            // The study header also announces Collapse pane; select the popup entry.
            harness
                .query_all_by_label("Collapse pane")
                .find(|node| node.rect().left() > rect.center().x)
                .unwrap()
                .click();
            harness.run();
            harness.run();
            assert!(harness.state().pane_height(PaneId::MAIN).unwrap() <= 24.0);
            assert!(harness.state_mut().undo());
            harness.run();
            assert!(harness.state().pane_height(PaneId::MAIN).unwrap() > 24.0);
            assert!(harness.state_mut().redo());
            harness.run();
            harness.get_by_label("Expand pane").click();
            harness.run();
            assert!(harness.state().pane_height(PaneId::MAIN).unwrap() > 24.0);
        }
    }

    /// While a pane is maximized, its menu has no entry that changes only the panes it hides,
    /// and a click on the header of a maximized study pane does not collapse it. Restore panes
    /// brings back the order and heights from before maximizing, and the maximize and restore
    /// steps are the only undo steps.
    #[test]
    fn maximized_pane_menu_and_header_leave_the_hidden_panes_alone() {
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        for price in [true, false] {
            let mut chart = crate::widget::test_frames::chart(&[100.0, 105.0], &[110.0, 115.0]);
            let rsi = chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
            chart.add_indicator(Box::new(crate::studies::MACD::new(12, 26, 9)));
            let rsi_pane = chart.series_pane(rsi).unwrap();
            chart.config_mut().show_volume = false;
            // The maximized pane has a drawn pane above and below it.
            let target = if price {
                assert!(chart.move_pane(rsi_pane, 0));
                PaneId::MAIN
            } else {
                rsi_pane
            };
            let mut harness = Harness::builder()
                .with_size(egui::vec2(800.0, 600.0))
                .allow_missing_glyphs()
                .with_accessibility_check(false)
                .build_ui_state(
                    |ui, chart: &mut Chart| {
                        chart.show(ui);
                    },
                    chart,
                );
            harness.run();
            let layout = |chart: &Chart| -> Vec<_> {
                chart
                    .panes()
                    .into_iter()
                    .map(|id| (id, chart.pane_height(id)))
                    .collect()
            };
            let before = layout(harness.state());
            assert!(!harness.state().can_undo());
            harness
                .state_mut()
                .user_action(|chart| chart.pane_layout.maximized = Some(target));
            harness.run();
            if !price {
                let pane = harness.state().get_rendered_indicator_panes()[0].clone();
                let header = pane.chart_rect.min + egui::vec2(10.0, 12.0);
                harness.hover_at(header);
                harness.run();
                for pressed in [true, false] {
                    harness.event(Event::PointerButton {
                        pos: header,
                        button: PointerButton::Primary,
                        pressed,
                        modifiers: Modifiers::NONE,
                    });
                }
                harness.run();
                let chart = harness.state();
                let stored = chart.pane_layout.panes.iter().find(|p| p.id == rsi_pane);
                assert_eq!(stored.unwrap().collapsed, None, "header click");
            }
            let rect = harness
                .state()
                .pane_layout
                .panes
                .iter()
                .find(|pane| pane.id == target)
                .unwrap()
                .rect;
            harness.hover_at(rect.center());
            harness.run();
            harness
                .query_all_by_label("...")
                .find(|node| rect.contains(node.rect().center()))
                .unwrap()
                .click();
            harness.run();
            for entry in ["Move pane up", "Move pane down", "Collapse pane"] {
                // The study header also announces Collapse pane; select the popup entry.
                let node = harness
                    .query_all_by_label(entry)
                    .find(|node| node.rect().left() > rect.center().x)
                    .unwrap();
                assert!(
                    node.accesskit_node().is_disabled(),
                    "price={price}: {entry}"
                );
            }
            // As the Restore panes button does.
            harness
                .state_mut()
                .user_action(|chart| chart.pane_layout.maximized = None);
            harness.run();
            assert_eq!(layout(harness.state()), before, "price={price}");
            let chart = harness.state_mut();
            assert!(chart.undo());
            assert_eq!(chart.pane_layout.maximized, Some(target));
            assert!(chart.undo());
            assert!(!chart.can_undo(), "price={price}");
        }
    }

    /// Pane controls respect study permissions while host pane operations remain available.
    #[test]
    fn pane_menu_keeps_read_only_study() {
        use crate::studies::{RSI, SMA};
        use egui_kittest::{Harness, kittest::Queryable};

        for (theme, size) in [
            (crate::theme::Theme::light(), egui::vec2(320.0, 450.0)),
            (crate::theme::Theme::dark(), egui::vec2(320.0, 450.0)),
            (crate::theme::Theme::light(), egui::vec2(800.0, 600.0)),
            (crate::theme::Theme::dark(), egui::vec2(800.0, 600.0)),
        ] {
            for enabled in [false, true] {
                for command in [
                    "Remove pane",
                    "Merge with pane above",
                    "Merge with pane below",
                    "Move to main pane",
                    "Move to new pane",
                    "price member",
                ] {
                    let mut chart = crate::widget::test_frames::chart(&[100.0; 20], &[110.0; 20]);
                    chart.add_indicator(Box::new(RSI::new(7)));
                    let study = chart.add_indicator(Box::new(RSI::new(14)));
                    let pane = chart.series_pane(study).unwrap();
                    let below = chart.add_indicator(Box::new(RSI::new(21)));
                    let below_pane = chart.series_pane(below).unwrap();
                    let peer = chart.add_indicator(Box::new(SMA::new(1)));
                    chart.move_series_to_pane(peer, pane);
                    chart.set_indicator_user_edit_enabled(study, enabled);
                    if command == "price member" {
                        chart.move_series_to_pane(study, PaneId::MAIN);
                        chart.move_series_to_pane(peer, PaneId::MAIN);
                    }
                    let original = chart.series_pane(study);
                    let setup_theme = theme.clone();
                    let mut harness = Harness::builder()
                        .with_context_setup(move |ctx| {
                            crate::theme::apply_to_egui(ctx, &setup_theme);
                        })
                        .with_size(size)
                        .build_ui_state(
                            |ui, chart: &mut Chart| {
                                let rect = ui.max_rect();
                                if command == "price member" {
                                    chart.main_pane_actions(ui, rect, rect);
                                } else if let Some(at) =
                                    chart.pane_layout.panes.iter().position(|p| p.id == pane)
                                {
                                    chart.pane_actions(ui, at, pane, rect, rect, false);
                                }
                            },
                            chart,
                        );
                    harness.hover_at((size / 2.0).to_pos2());
                    harness.run();
                    harness.get_by_label("...").click();
                    harness.run();
                    if command == "price member" && !enabled {
                        assert!(harness.query_by_label("RSI").is_none());
                        assert!(harness.query_by_label("SMA").is_some());
                    } else {
                        let action = if command.starts_with("Move to") || command == "price member"
                        {
                            harness.get_by_label("RSI").click();
                            harness.run();
                            if command == "price member" {
                                "Move to new pane"
                            } else {
                                command
                            }
                        } else {
                            command
                        };
                        assert!(
                            Rect::from_min_size(Pos2::ZERO, size)
                                .contains_rect(harness.get_by_label(action).rect()),
                            "{command} at {size:?}"
                        );
                        harness.get_by_label(action).click();
                        harness.run();
                    }
                    let chart = harness.state();
                    if enabled {
                        assert_ne!(chart.series_pane(study), original, "{command}");
                    } else {
                        assert_eq!(chart.series_pane(study), original, "{command}");
                        assert!(
                            chart.indicators().any(|(id, _)| id == peer),
                            "no partial removal"
                        );
                        let chart = harness.state_mut();
                        assert!(chart.move_series_to_pane(study, below_pane));
                        assert!(chart.move_to_new_pane(study));
                        let pane = chart.series_pane(study).unwrap();
                        assert!(chart.remove_pane(pane));
                        assert!(chart.indicators().all(|(id, _)| id != study));
                    }
                }
            }
        }
    }

    #[test]
    fn pointer_command_double_click_collapses_and_restores_the_pointed_pane() {
        for location in [
            "plot",
            "header",
            "axis",
            "price",
            "moved-price",
            "disabled",
            "maximized",
            "maximized-price",
        ] {
            for modifiers in [
                Modifiers::CTRL.plus(Modifiers::COMMAND),
                Modifiers::MAC_CMD.plus(Modifiers::COMMAND),
            ] {
                let ctx = Context::default();
                let mut chart = crate::widget::test_frames::chart(&[100.0; 100], &[110.0; 100]);
                let study = chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
                let study_pane = chart.series_pane(study).unwrap();
                if location.starts_with("maximized") {
                    chart.add_indicator(Box::new(crate::studies::RSI::new(7)));
                }
                if location == "price" {
                    chart.add_indicator(Box::new(crate::studies::RSI::new(7)));
                    for pane in &mut chart.pane_layout.panes {
                        if pane.id != PaneId::MAIN {
                            pane.collapsed = Some(false);
                        }
                    }
                }
                if location == "moved-price" {
                    chart.move_series_to_pane(SeriesId::MAIN, study_pane);
                }
                let id = if location.ends_with("price") {
                    chart.main_series_pane()
                } else {
                    study_pane
                };
                if location.starts_with("maximized") {
                    chart.pane_layout.maximized = Some(id);
                }
                let screen_height = std::cell::Cell::new(600.0);
                let mut time = 0.0;
                let mut frame = |chart: &mut Chart, events| {
                    time += 0.02;
                    ctx.run_ui(
                        RawInput {
                            screen_rect: Some(Rect::from_min_size(
                                Pos2::ZERO,
                                egui::vec2(800.0, screen_height.get()),
                            )),
                            time: Some(time),
                            events,
                            ..Default::default()
                        },
                        |ui| {
                            if location == "disabled" {
                                ui.disable();
                            }
                            chart.show(ui);
                        },
                    )
                    .drop_without_applying_deltas();
                };
                for _ in 0..3 {
                    frame(&mut chart, vec![]);
                }
                let at = chart
                    .pane_layout
                    .panes
                    .iter()
                    .position(|pane| pane.id == id)
                    .unwrap();
                let height = chart.pane_height(id).unwrap();
                let stretch = chart.pane_stretch_factor(id).unwrap();
                // Left of the first bar the price pane's plot has no series to hit.
                let first_bar = chart.get_chart_mapping().idx_to_x(0);
                let click = |rect: Rect| match location {
                    "header" => rect.left_top() + egui::vec2(30.0, 10.0),
                    "axis" => rect.right_center() - egui::vec2(10.0, 0.0),
                    "maximized-price" => {
                        Pos2::new((rect.left() + first_bar) / 2.0, rect.center().y)
                    }
                    _ => rect.center(),
                };
                let pos = click(chart.pane_layout.panes[at].rect);
                for pressed in [true, false, true, false] {
                    frame(
                        &mut chart,
                        vec![
                            Event::ModifiersChanged(modifiers),
                            Event::PointerMoved(pos),
                            Event::PointerButton {
                                pos,
                                pressed,
                                button: PointerButton::Primary,
                                modifiers,
                            },
                        ],
                    );
                }
                frame(&mut chart, vec![]);
                if location.starts_with("maximized") {
                    // There it is the plain double-click: one step that brings back every pane.
                    assert_eq!(chart.pane_layout.maximized, None, "{modifiers:?}");
                    assert_eq!(chart.pane_layout.panes.len(), 3, "{location}");
                    assert!(
                        chart
                            .pane_layout
                            .panes
                            .iter()
                            .all(|pane| pane.rect.height() > 24.0),
                        "{modifiers:?}"
                    );
                    assert!(chart.undo());
                    assert_eq!(chart.pane_layout.maximized, Some(id));
                    assert!(!chart.can_undo());
                    continue;
                }
                if location == "disabled" {
                    assert_eq!(chart.pane_height(id), Some(height));
                    assert_eq!(chart.pane_layout.maximized, None);
                    continue;
                }
                assert_eq!(
                    chart.pane_height(id),
                    Some(24.0),
                    "{location} {modifiers:?}"
                );
                assert_eq!(chart.pane_layout.maximized, None);
                assert_eq!(chart.pane_stretch_factor(id), Some(stretch));
                if location == "price" {
                    // Three headers fit here, but an expanded study needs at least 40 px.
                    screen_height.set(115.0);
                    frame(&mut chart, vec![]);
                    assert!(
                        chart
                            .pane_layout
                            .panes
                            .iter()
                            .all(|pane| pane.rect.height() <= 24.0)
                    );
                    screen_height.set(600.0);
                    frame(&mut chart, vec![]);
                }
                let focus = ctx.memory(|memory| memory.focused());
                #[cfg(feature = "ui")]
                {
                    let snapshot = chart.snapshot().unwrap();
                    let mut restored =
                        crate::widget::test_frames::chart(&[100.0; 100], &[110.0; 100]);
                    restored.restore(&snapshot).unwrap();
                    frame(&mut restored, vec![]);
                    let saved_pane = restored.panes()[at];
                    assert_eq!(
                        restored.pane_height(saved_pane),
                        Some(24.0),
                        "saved {location}"
                    );
                }
                assert!(chart.undo());
                frame(&mut chart, vec![]);
                assert!((chart.pane_height(id).unwrap() - height).abs() < 0.01);
                assert!(chart.redo());
                frame(&mut chart, vec![]);
                // Separate the next double click from the first one's click count.
                for _ in 0..40 {
                    frame(&mut chart, vec![]);
                }
                let pos = chart.pane_layout.panes[at].rect.center();
                for pressed in [true, false, true, false] {
                    frame(
                        &mut chart,
                        vec![
                            Event::PointerMoved(pos),
                            Event::PointerButton {
                                pos,
                                pressed,
                                button: PointerButton::Primary,
                                modifiers,
                            },
                        ],
                    );
                }
                frame(&mut chart, vec![]);
                assert!(
                    (chart.pane_height(id).unwrap() - height).abs() < 0.01,
                    "restored {location}"
                );
                assert_eq!(ctx.memory(|memory| memory.focused()), focus);
                assert!(
                    chart.take_indicator_settings_request().is_none(),
                    "{location} {modifiers:?}"
                );
            }
        }
    }

    /// The move entries of a pane's menu pass the hidden panes next to it.
    #[test]
    fn move_pane_entries_skip_hidden_panes() {
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };

        for hide_rsi in [false, true] {
            let mut chart = crate::widget::test_frames::chart(&[100.0; 20], &[110.0; 20]);
            let rsi = chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
            let macd = chart.add_indicator(Box::new(crate::studies::MACD::new(12, 26, 9)));
            let [rsi_pane, macd_pane] = [rsi, macd].map(|id| chart.series_pane(id).unwrap());
            let (hidden, pane, expected) = if hide_rsi {
                (rsi, macd_pane, [macd_pane, PaneId::MAIN, rsi_pane])
            } else {
                (macd, rsi_pane, [rsi_pane, PaneId::MAIN, macd_pane])
            };
            chart.set_series_visible(hidden, false);
            let mut harness = Harness::builder()
                .with_size(egui::vec2(800.0, 600.0))
                .build_ui_state(
                    |ui, chart: &mut Chart| {
                        let rect = ui.max_rect();
                        let at = chart.pane_layout.panes.iter().position(|p| p.id == pane);
                        chart.pane_actions(ui, at.unwrap(), pane, rect, rect, false);
                    },
                    chart,
                );
            harness.hover_at(egui::pos2(400.0, 300.0));
            harness.run();
            harness.get_by_label("...").click();
            harness.run();
            let down = harness.get_by_label("Move pane down");
            assert!(down.accesskit_node().is_disabled(), "{hide_rsi}");
            harness.get_by_label("Move pane up").click();
            harness.run();
            assert_eq!(harness.state().panes(), expected, "{hide_rsi}");
        }
    }

    /// The menu of the pane drawn at the top of a chart in a layout opens from any point of its
    /// button, which the layout's corner menu covered, and the corner menu still opens.
    #[test]
    fn top_pane_menu_opens_beside_the_layout_corner_menu() {
        use crate::studies::RSI;
        use crate::widget::layout::ChartLayout;
        use egui_kittest::{Harness, kittest::Queryable};

        for (top, entry, moved_to) in [(false, "Move pane up", 0), (true, "Move pane down", 1)] {
            let mut chart = crate::widget::test_frames::chart(&[100.0, 105.0], &[110.0, 115.0]);
            let study = chart.add_indicator(Box::new(RSI::new(14)));
            let pane = chart.series_pane(study).unwrap();
            if top {
                assert!(chart.move_pane(pane, 0));
            }
            let mut harness = Harness::builder()
                .with_size(egui::vec2(800.0, 600.0))
                // The RSI legend writes a missing value as "∅", and the text field of the layout
                // menu has no accessible name.
                .allow_missing_glyphs()
                .with_accessibility_check(false)
                .build_ui_state(
                    |ui, state: &mut (ChartLayout, Chart)| {
                        let (layout, chart) = state;
                        layout.show(ui, &mut [chart]);
                    },
                    (ChartLayout::default(), chart),
                );
            let rect = harness.state().1.pane_layout.panes[usize::from(!top)].rect;
            harness.hover_at(rect.center());
            harness.run();
            // The point of the pane's menu button nearest the corner button.
            let corner = harness.get_by_label("\u{2026}").rect();
            let at = harness
                .get_by_label("...")
                .rect()
                .shrink(1.0)
                .clamp(corner.center());
            harness.hover_at(at);
            harness.run();
            for pressed in [true, false] {
                harness.event(Event::PointerButton {
                    pos: at,
                    button: PointerButton::Primary,
                    pressed,
                    modifiers: Modifiers::NONE,
                });
            }
            harness.run();
            harness.get_by_label(entry).click();
            harness.run();
            let order = harness.state().1.panes();
            assert_eq!(
                order.iter().position(|id| *id == pane),
                Some(moved_to),
                "top={top}"
            );
            harness.hover_at(rect.center());
            harness.run();
            harness.get_by_label("\u{2026}").click();
            harness.run();
            assert!(
                harness.query_by_label("Detach chart").is_some(),
                "top={top}"
            );
        }
    }

    #[test]
    fn command_double_click_outside_every_pane_keeps_the_time_axis_reset() {
        for modifiers in [
            Modifiers::CTRL.plus(Modifiers::COMMAND),
            Modifiers::MAC_CMD.plus(Modifiers::COMMAND),
        ] {
            let ctx = Context::default();
            let mut chart = crate::widget::test_frames::chart(&[100.0; 100], &[110.0; 100]);
            chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
            let mut time = 0.0;
            let mut frame = |chart: &mut Chart, events| {
                time += 0.02;
                crate::widget::test_frames::frame(&ctx, chart, time, events);
            };
            for _ in 0..3 {
                frame(&mut chart, vec![]);
            }
            let rects = |chart: &Chart| -> Vec<_> {
                chart
                    .pane_layout
                    .panes
                    .iter()
                    .map(|pane| pane.rect)
                    .collect()
            };
            let before = rects(&chart);
            chart.state.time_scale_mut().set_bar_spacing(12.0);
            frame(&mut chart, vec![]);
            // The time axis lies below every pane, so no pane can collapse here.
            let axis = chart.time_axis_rect.center();
            assert!(before.iter().all(|rect| !rect.contains(axis)));
            for pressed in [true, false, true, false] {
                frame(
                    &mut chart,
                    vec![
                        Event::ModifiersChanged(modifiers),
                        Event::PointerMoved(axis),
                        Event::PointerButton {
                            pos: axis,
                            pressed,
                            button: PointerButton::Primary,
                            modifiers,
                        },
                    ],
                );
            }
            frame(&mut chart, vec![]);
            assert_eq!(rects(&chart), before, "no pane collapses");
            assert_eq!(chart.state.time_scale().bar_spacing(), 6.0);
        }
    }
}
