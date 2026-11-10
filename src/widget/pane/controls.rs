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
                .on_hover_cursor(egui::CursorIcon::ResizeVertical);
            if response.dragged()
                && self.chart_options.handle_scale.enabled
                && self
                    .chart_options
                    .handle_scale
                    .axis_pressed_mouse_move
                    .price
            {
                let anchor_y = ui
                    .input(|input| input.pointer.press_origin())
                    .map_or(rect.center().y, |pos| pos.y);
                let scale = self.pane_price_scale_mut(pane_id, key);
                let map = scale.map(scale.transform(), rect.top(), rect.bottom());
                let anchor = map.transform().to_scale(map.y_to_price(anchor_y));
                scale.zoom(
                    (f64::from(response.drag_delta().y) / f64::from(rect.height())).exp(),
                    anchor,
                );
                ui.ctx().request_repaint();
            }
            if response.double_clicked()
                && !self.pane_collapse_click(ui.ctx())
                && self.chart_options.handle_scale.enabled
                && self
                    .chart_options
                    .handle_scale
                    .axis_double_click_reset
                    .price
            {
                self.user_action(|chart| chart.pane_price_scale_mut(pane_id, key).reset());
                ui.ctx().request_repaint();
            }
            response.context_menu(|ui| {
                self.user_action(|chart| {
                    let scale = chart.pane_price_scale_mut(pane_id, key);
                    if ui
                        .selectable_label(!scale.is_manual(), l10n::text(ui.ctx(), "Auto scale"))
                        .clicked()
                    {
                        scale.reset();
                        ui.close();
                    }
                    ui.separator();
                    use crate::scales::PriceScaleMode;
                    for mode in [
                        PriceScaleMode::Normal,
                        PriceScaleMode::Logarithmic,
                        PriceScaleMode::Percentage,
                        PriceScaleMode::IndexedTo100,
                    ] {
                        if ui
                            .selectable_label(
                                scale.options().mode == mode,
                                l10n::text(ui.ctx(), mode.to_string()),
                            )
                            .clicked()
                        {
                            scale.options_mut().mode = mode;
                            scale.reset();
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui
                        .checkbox(
                            &mut scale.options_mut().invert_scale,
                            l10n::text(ui.ctx(), "Invert scale"),
                        )
                        .changed()
                    {
                        ui.close();
                    }
                    ui.separator();
                    let options = chart.pane_axis_options(pane_id).expect("present pane");
                    for (label, side) in [
                        ("Axes on left", crate::model::AxisSide::Left),
                        ("Axes on right", crate::model::AxisSide::Right),
                    ] {
                        if ui
                            .selectable_label(
                                options.side == Some(side),
                                l10n::text(ui.ctx(), label),
                            )
                            .clicked()
                        {
                            chart.set_pane_axis_options(
                                pane_id,
                                crate::model::PaneAxisOptions {
                                    side: (options.side != Some(side)).then_some(side),
                                    ..options
                                },
                            );
                            ui.close();
                        }
                    }
                    ui.menu_button(l10n::text(ui.ctx(), "Join price scales"), |ui| {
                        for (label, side) in [
                            ("At left edge", crate::model::AxisSide::Left),
                            ("At right edge", crate::model::AxisSide::Right),
                        ] {
                            if ui
                                .selectable_label(false, l10n::text(ui.ctx(), label))
                                .clicked()
                            {
                                chart.merge_pane_scales(pane_id, side);
                                ui.close();
                            }
                        }
                    });
                    let mut hide_unused = options.hide_unused;
                    if ui
                        .checkbox(&mut hide_unused, l10n::text(ui.ctx(), "Hide unused axes"))
                        .changed()
                    {
                        chart.set_pane_axis_options(
                            pane_id,
                            crate::model::PaneAxisOptions {
                                hide_unused,
                                ..options
                            },
                        );
                        ui.close();
                    }
                });
            });
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
        if movable.is_empty() || !hovered {
            return;
        }
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
        egui::containers::menu::MenuButton::from_button(
            egui::Button::new("...").frame_when_inactive(false),
        )
        .ui(&mut child, |ui| {
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
        self.pane_maximize_button(ui, id, rect, plot);
        let visible = self.pane_buttons_visible(ui, rect);
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
        let members = self.pane_series(id);
        let editable = members.iter().all(|member| {
            !self.study_ids.contains(member) || self.indicator_user_edit_enabled(*member)
        });
        child.menu_button("...", |ui| {
            for (label, enabled, command) in [
                ("Move pane up", above.is_some(), 0),
                ("Move pane down", below.is_some(), 1),
                (
                    if collapsed {
                        "Expand pane"
                    } else {
                        "Collapse pane"
                    },
                    true,
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
            Some(0) => self.pane_layout.panes.swap(pane_idx, pane_idx - 1),
            Some(1) => self.pane_layout.panes.swap(pane_idx, pane_idx + 1),
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
            if response.dragged() && !collapsed && !upper_collapsed {
                let delta = response.drag_delta().y.clamp(
                    40.0 - upper.height(),
                    (rect.height() - 40.0).max(40.0 - upper.height()),
                );
                // The other panes keep their heights: these two exchange pixels.
                self.pane_layout.panes[upper_idx].height = Some(upper.height() + delta);
                self.pane_layout.panes[idx].height = Some(rect.height() - delta);
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

    /// Pane controls respect study permissions while host pane operations remain available.
    #[test]
    fn pane_menu_keeps_read_only_study() {
        use crate::studies::{RSI, SMA};
        use egui_kittest::{Harness, kittest::Queryable};

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
                let mut harness = Harness::builder()
                    .with_size(egui::vec2(800.0, 600.0))
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
                harness.hover_at(egui::pos2(400.0, 300.0));
                harness.run();
                harness.get_by_label("...").click();
                harness.run();
                if command == "price member" && !enabled {
                    assert!(harness.query_by_label("RSI").is_none());
                    assert!(harness.query_by_label("SMA").is_some());
                } else {
                    let action = if command.starts_with("Move to") || command == "price member" {
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

    #[test]
    fn pointer_command_double_click_collapses_and_restores_the_pointed_pane() {
        for location in ["plot", "header", "axis", "price", "moved-price", "disabled"] {
            for modifiers in [
                Modifiers::CTRL.plus(Modifiers::COMMAND),
                Modifiers::MAC_CMD.plus(Modifiers::COMMAND),
            ] {
                let ctx = Context::default();
                let mut chart = crate::widget::test_frames::chart(&[100.0; 100], &[110.0; 100]);
                let study = chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
                let study_pane = chart.series_pane(study).unwrap();
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
                let id = if location == "price" || location == "moved-price" {
                    chart.main_series_pane()
                } else {
                    study_pane
                };
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
                let click = |rect: Rect| match location {
                    "header" => rect.left_top() + egui::vec2(30.0, 10.0),
                    "axis" => rect.right_center() - egui::vec2(10.0, 0.0),
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
