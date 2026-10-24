//! Pane placement, layout state and interaction controls.

use super::{Chart, SeriesId};
use egui::{Pos2, Rect, Sense, Ui};

#[derive(Default)]
pub(super) struct PaneLayout {
    pub(super) panes: Vec<Pane>,
    pub(super) maximized: Option<SeriesId>,
    pub(super) first_axis_frame: Option<u64>,
}

pub(super) struct Pane {
    pub(super) id: SeriesId,
    pub(super) studies: Vec<SeriesId>,
    pub(super) height: Option<f32>,
    pub(super) collapsed: Option<bool>,
    pub(super) scale: crate::scales::PriceScale,
}

impl Chart {
    pub(super) fn sync_panes(&mut self) {
        for pane in &mut self.pane_layout.panes {
            pane.studies.retain(|id| {
                self.study_ids
                    .iter()
                    .position(|study| study == id)
                    .is_some_and(|idx| !self.studies.indicators()[idx].is_overlay())
            });
        }
        self.pane_layout
            .panes
            .retain(|pane| !pane.studies.is_empty());
        for (idx, study) in self.studies.indicators().iter().enumerate() {
            let id = self.study_ids[idx];
            if !study.is_overlay()
                && !self
                    .pane_layout
                    .panes
                    .iter()
                    .any(|pane| pane.studies.contains(&id))
            {
                self.pane_layout.panes.push(Pane {
                    id,
                    studies: vec![id],
                    height: study
                        .pane()
                        .height
                        .filter(|height| height.is_finite() && *height >= 40.0),
                    collapsed: None,
                    scale: crate::scales::PriceScale::new(crate::scales::PriceScaleOptions {
                        scale_margins: crate::scales::PriceScaleMargins {
                            top: 0.0,
                            bottom: 0.0,
                        },
                        ..Default::default()
                    }),
                });
            }
        }
        if self.pane_layout.maximized.is_some_and(|id| {
            id != SeriesId::MAIN
                && !self.pane_layout.panes.iter().any(|pane| {
                    pane.id == id
                        && pane.studies.iter().any(|study| {
                            self.study_ids
                                .iter()
                                .position(|id| id == study)
                                .is_some_and(|idx| self.studies.indicators()[idx].is_visible())
                        })
                })
        }) {
            self.pane_layout.maximized = None;
        }
    }

    /// Moves a study to the main pane, another study's pane, or its own new pane.
    /// Use `SeriesId::MAIN`, the destination study id, or the moved study's own id respectively.
    /// Returns `false` for an unknown id or a custom presentation that rejects placement edits.
    pub fn move_indicator_to_pane(&mut self, study: SeriesId, target: SeriesId) -> bool {
        self.sync_panes();
        let Some(idx) = self.study_ids.iter().position(|id| *id == study) else {
            return false;
        };
        let destination = self
            .pane_layout
            .panes
            .iter()
            .position(|pane| pane.studies.contains(&target));
        if target != SeriesId::MAIN && target != study && destination.is_none() {
            return false;
        }
        if !self.studies.indicators_mut()[idx].set_overlay(target == SeriesId::MAIN) {
            return false;
        }
        let destination_id = destination.map(|idx| self.pane_layout.panes[idx].id);
        for pane in &mut self.pane_layout.panes {
            pane.studies.retain(|id| *id != study);
        }
        if target != SeriesId::MAIN && target != study {
            self.pane_layout
                .panes
                .iter_mut()
                .find(|pane| Some(pane.id) == destination_id)
                .unwrap()
                .studies
                .push(study);
        }
        self.sync_panes();
        true
    }
}

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
        if response.double_clicked() {
            pane.scale.reset();
            ui.ctx().request_repaint();
        }
        response.context_menu(|ui| {
            if ui
                .selectable_label(!pane.scale.is_manual(), "Auto scale")
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
                    .selectable_label(pane.scale.options().mode == mode, mode.to_string())
                    .clicked()
                {
                    pane.scale.options_mut().mode = mode;
                    pane.scale.reset();
                    ui.close();
                }
            }
            ui.separator();
            if ui
                .checkbox(&mut pane.scale.options_mut().invert_scale, "Invert scale")
                .changed()
            {
                ui.close();
            }
        });
    }

    pub(super) fn main_pane_actions(&mut self, ui: &mut Ui, rect: Rect, plot: Rect) {
        if rect.height() < 48.0 {
            return;
        }
        let rect = Rect::from_min_max(rect.min + egui::vec2(0.0, 24.0), rect.max);
        if self
            .studies
            .indicators()
            .iter()
            .any(|study| !study.is_overlay() && study.is_visible())
        {
            self.pane_maximize_button(ui, SeriesId::MAIN, rect, plot);
        }
        if !self
            .studies
            .indicators()
            .iter()
            .any(|study| study.is_overlay())
        {
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
        child.menu_button("...", |ui| {
            for (idx, study) in self
                .studies
                .indicators()
                .iter()
                .enumerate()
                .filter(|(_, study)| study.is_overlay())
            {
                ui.menu_button(study.name(), |ui| {
                    if ui.button("Move to new pane").clicked() {
                        moved = Some((self.study_ids[idx], self.study_ids[idx]));
                        ui.close();
                    }
                    for pane in &self.pane_layout.panes {
                        let title = pane
                            .studies
                            .iter()
                            .find_map(|id| self.study_ids.iter().position(|study| study == id))
                            .map(|idx| self.studies.indicators()[idx].name())
                            .unwrap_or("Study");
                        if ui.button(format!("Move to {title} pane")).clicked() {
                            moved = Some((self.study_ids[idx], pane.studies[0]));
                            ui.close();
                        }
                    }
                });
            }
        });
        if let Some((study, target)) = moved {
            self.move_indicator_to_pane(study, target);
        }
    }

    fn pane_maximize_button(&mut self, ui: &mut Ui, id: SeriesId, rect: Rect, plot: Rect) {
        if rect.height() < 24.0 {
            return;
        }
        let button = Rect::from_min_size(
            Pos2::new(plot.right() - 22.0, rect.top() + 2.0),
            egui::vec2(20.0, 20.0),
        );
        let maximized = self.pane_layout.maximized == Some(id);
        let response = ui
            .interact(button, ui.scope_id().with((id, "maximize")), Sense::click())
            .on_hover_text(if maximized {
                "Restore panes"
            } else {
                "Maximize pane"
            });
        if response.hovered() || maximized || ui.rect_contains_pointer(rect) {
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
            self.pane_layout.maximized = if maximized { None } else { Some(id) };
            ui.ctx().request_repaint();
        }
    }

    pub(super) fn pane_actions(
        &mut self,
        ui: &mut Ui,
        pane_idx: usize,
        id: SeriesId,
        rect: Rect,
        plot: Rect,
        collapsed: bool,
    ) {
        self.pane_maximize_button(ui, id, rect, plot);
        let button = Rect::from_min_size(
            Pos2::new(plot.right() - 42.0, rect.top() + 2.0),
            egui::vec2(20.0, 20.0),
        );
        let mut action = None;
        let mut move_study = None;
        let mut child = ui.new_child(
            egui::UiBuilder::new()
                .id_salt((id, "pane_actions"))
                .max_rect(button),
        );
        child.menu_button("...", |ui| {
            for (label, enabled, command) in [
                ("Move pane up", pane_idx > 0, 0),
                (
                    "Move pane down",
                    pane_idx + 1 < self.pane_layout.panes.len(),
                    1,
                ),
                (
                    if collapsed {
                        "Expand pane"
                    } else {
                        "Collapse pane"
                    },
                    true,
                    2,
                ),
                ("Merge with pane above", pane_idx > 0, 3),
                (
                    "Merge with pane below",
                    pane_idx + 1 < self.pane_layout.panes.len(),
                    4,
                ),
                ("Remove pane", true, 5),
            ] {
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                    action = Some(command);
                    ui.close();
                }
            }
            ui.separator();
            for study in self.pane_layout.panes[pane_idx].studies.clone() {
                let Some(idx) = self.study_ids.iter().position(|id| *id == study) else {
                    continue;
                };
                ui.menu_button(self.studies.indicators()[idx].name(), |ui| {
                    if ui.button("Move to main pane").clicked() {
                        action = Some(6);
                        move_study = Some(study);
                        ui.close();
                    }
                    if ui
                        .add_enabled(
                            self.pane_layout.panes[pane_idx].studies.len() > 1,
                            egui::Button::new("Move to new pane"),
                        )
                        .clicked()
                    {
                        action = Some(7);
                        move_study = Some(study);
                        ui.close();
                    }
                });
            }
        });
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
                let members = std::mem::take(&mut self.pane_layout.panes[pane_idx].studies);
                self.pane_layout.panes[target].studies.extend(members);
                self.sync_panes();
            }
            Some(5) => {
                for study in self.pane_layout.panes[pane_idx].studies.clone() {
                    self.remove_series(study);
                }
                self.sync_panes();
            }
            Some(6 | 7) => {
                let study = move_study.unwrap();
                self.move_indicator_to_pane(
                    study,
                    if action == Some(6) {
                        SeriesId::MAIN
                    } else {
                        study
                    },
                );
            }
            _ => {}
        }
        if action.is_some() {
            ui.ctx().request_repaint();
        }
    }

    pub(super) fn pane_separators(
        &mut self,
        ui: &mut Ui,
        panes: &[(usize, SeriesId, Rect, bool)],
        main: Rect,
    ) {
        if self.pane_layout.maximized.is_some() {
            return;
        }
        for (at, (idx, id, rect, collapsed)) in panes.iter().copied().enumerate() {
            let upper = if at == 0 { main } else { panes[at - 1].2 };
            let line = Rect::from_min_max(
                Pos2::new(rect.left(), upper.bottom()),
                Pos2::new(rect.right(), rect.top()),
            );
            let response = ui
                .interact(
                    line.expand2(egui::vec2(0.0, 3.0)),
                    ui.scope_id().with((id, "separator")),
                    Sense::click_and_drag(),
                )
                .on_hover_cursor(egui::CursorIcon::ResizeVertical);
            use crate::theme::ThemeExt;
            let color = if response.hovered() || response.dragged() {
                ui.visuals().selection.bg_fill
            } else {
                ui.active_theme().chart.grid_line
            };
            ui.painter().rect_filled(line, 0.0, color);
            if response.dragged() && !collapsed && (at == 0 || !panes[at - 1].3) {
                let delta = response.drag_delta().y.clamp(
                    40.0 - upper.height(),
                    (rect.height() - 40.0).max(40.0 - upper.height()),
                );
                for (idx, _, rect, collapsed) in panes {
                    if !collapsed {
                        self.pane_layout.panes[*idx].height = Some(rect.height());
                    }
                }
                self.pane_layout.panes[idx].height = Some(rect.height() - delta);
                if at > 0 {
                    self.pane_layout.panes[panes[at - 1].0].height = Some(upper.height() + delta);
                }
                ui.ctx().request_repaint();
            }
            if response.double_clicked() {
                self.pane_layout.panes[idx].height = None;
                if at > 0 {
                    self.pane_layout.panes[panes[at - 1].0].height = None;
                }
                ui.ctx().request_repaint();
            }
        }
    }
}
