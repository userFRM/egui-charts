use super::{Chart, Pane, PaneId, SeriesId};
use crate::config::NavigationButtons;
use crate::localization as l10n;
use egui::{Pos2, Rect, Sense, Ui};

impl Chart {
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
        let axis = if self.config.show_right_axis {
            Rect::from_min_max(Pos2::new(plot.right(), rect.top()), rect.max)
        } else if self.config.show_left_axis {
            Rect::from_min_max(rect.min, Pos2::new(plot.left(), rect.bottom()))
        } else {
            return;
        };
        let pane = &mut self.pane_layout.panes[idx];
        let response = ui
            .interact(
                axis,
                ui.scope_id().with((pane.id, "price_axis")),
                Sense::click_and_drag(),
            )
            .on_hover_cursor(egui::CursorIcon::ResizeVertical);
        if response.dragged()
            && self
                .chart_options
                .handle_scale
                .axis_pressed_mouse_move
                .price
        {
            let anchor_y = ui
                .input(|input| input.pointer.press_origin())
                .map_or(rect.center().y, |pos| pos.y);
            let map = pane
                .scale
                .map(pane.scale.transform(), rect.top(), rect.bottom());
            let anchor = map.transform().to_scale(map.y_to_price(anchor_y));
            pane.scale.zoom(
                (f64::from(response.drag_delta().y) / f64::from(rect.height())).exp(),
                anchor,
            );
            ui.ctx().request_repaint();
        }
        if response.double_clicked()
            && self
                .chart_options
                .handle_scale
                .axis_double_click_reset
                .price
        {
            self.user_action(|chart| chart.pane_layout.panes[idx].scale.reset());
            ui.ctx().request_repaint();
        }
        response.context_menu(|ui| {
            self.user_action(|chart| {
                let pane = &mut chart.pane_layout.panes[idx];
                if ui
                    .selectable_label(!pane.scale.is_manual(), l10n::text(ui.ctx(), "Auto scale"))
                    .clicked()
                {
                    pane.scale.reset();
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
                            pane.scale.options().mode == mode,
                            l10n::text(ui.ctx(), mode.to_string()),
                        )
                        .clicked()
                    {
                        pane.scale.options_mut().mode = mode;
                        pane.scale.reset();
                        ui.close();
                    }
                }
                ui.separator();
                if ui
                    .checkbox(
                        &mut pane.scale.options_mut().invert_scale,
                        l10n::text(ui.ctx(), "Invert scale"),
                    )
                    .changed()
                {
                    ui.close();
                }
            });
        });
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
            self.pane_maximize_button(ui, PaneId::MAIN, rect, plot);
        }
        let movable: Vec<_> = self
            .pane_series(PaneId::MAIN)
            .into_iter()
            .filter(|id| *id != SeriesId::MAIN)
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
        let above = pane_idx.checked_sub(1).map(|at| panes[at].id);
        let below = panes.get(pane_idx + 1).map(|pane| pane.id);
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
                    above.is_some_and(|pane| pane != PaneId::MAIN),
                    3,
                ),
                (
                    "Merge with pane below",
                    below.is_some_and(|pane| pane != PaneId::MAIN),
                    4,
                ),
                ("Remove pane", true, 5),
            ] {
                if ui
                    .add_enabled(enabled, egui::Button::new(l10n::text(ui.ctx(), label)))
                    .clicked()
                {
                    action = Some(command);
                    ui.close();
                }
            }
            let members = self.pane_series(id);
            if !members.is_empty() {
                ui.separator();
            }
            for member in &members {
                ui.menu_button(l10n::text(ui.ctx(), self.member_name(*member)), |ui| {
                    if ui
                        .button(l10n::text(ui.ctx(), "Move to main pane"))
                        .clicked()
                    {
                        action = Some(6);
                        move_member = Some(*member);
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            members.len() > 1,
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
                self.move_series_to_pane(move_member.unwrap(), PaneId::MAIN);
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
            if response.double_clicked() {
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
