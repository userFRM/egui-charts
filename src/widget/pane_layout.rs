//! Pane placement, layout state and interaction controls.

use super::{Chart, SeriesId};
use egui::{Color32, Pos2, Rect, Sense, Ui};

/// Identifies a pane of a chart. [`PaneId::MAIN`] is the price pane; [`Chart::add_pane`] and
/// [`Chart::panes`] return the others. An id stays valid until its pane is removed and is never
/// reused by the chart.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PaneId(pub(super) usize);

impl PaneId {
    /// The price pane, which draws the main series and its overlays.
    pub const MAIN: PaneId = PaneId(0);
}

/// Behaviour and colours of the separators between panes, set with
/// [`Chart::with_pane_options`] or [`Chart::pane_options_mut`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PaneOptions {
    /// Dragging a separator resizes the panes on either side of it, and a double click returns
    /// both to their default stretch factors. `true` by default.
    pub resizable: bool,
    /// Separator colour; `None` uses the theme's grid line colour.
    pub separator_color: Option<Color32>,
    /// Colour of a separator under the pointer or being dragged; `None` uses the selection
    /// colour. Unused when the panes are not resizable.
    pub separator_hover_color: Option<Color32>,
}

impl Default for PaneOptions {
    fn default() -> Self {
        Self {
            resizable: true,
            separator_color: None,
            separator_hover_color: None,
        }
    }
}

#[derive(Clone)]
pub(super) struct PaneLayout {
    /// Every pane from top to bottom, the price pane included.
    pub(super) panes: Vec<Pane>,
    pub(super) maximized: Option<PaneId>,
}

impl Default for PaneLayout {
    fn default() -> Self {
        Self {
            panes: vec![Pane::new(PaneId::MAIN)],
            maximized: None,
        }
    }
}

#[derive(Clone)]
pub(super) struct Pane {
    pub(super) id: PaneId,
    /// Studies and host series drawn in the pane, in legend order; empty for the price pane.
    pub(super) members: Vec<SeriesId>,
    /// Share of the expanded panes' height, relative to the factors of the others.
    pub(super) stretch: f32,
    /// Height to reach on the next layout, which turns it into a stretch factor.
    pub(super) height: Option<f32>,
    /// Keep the pane when its last member leaves it.
    pub(super) preserve: bool,
    pub(super) collapsed: Option<bool>,
    pub(super) scale: crate::scales::PriceScale,
    /// Panel rect in the last frame; [`Rect::NOTHING`] when the pane was not drawn.
    pub(super) rect: Rect,
}

impl Pane {
    fn new(id: PaneId) -> Self {
        Self {
            id,
            members: Vec::new(),
            stretch: Self::default_stretch(id),
            height: None,
            preserve: false,
            collapsed: None,
            scale: crate::scales::PriceScale::new(crate::scales::PriceScaleOptions {
                scale_margins: crate::scales::PriceScaleMargins {
                    top: 0.0,
                    bottom: 0.0,
                },
                ..Default::default()
            }),
            rect: Rect::NOTHING,
        }
    }

    /// The price pane takes three shares of the height and every other pane one.
    pub(super) fn default_stretch(id: PaneId) -> f32 {
        if id == PaneId::MAIN { 3.0 } else { 1.0 }
    }
}

impl Chart {
    /// Whether `id` is drawn below the price pane when placed there: a host series or a study
    /// that is not an overlay.
    fn pane_member(&self, id: SeriesId) -> bool {
        self.price_series.iter().any(|item| item.id == id)
            || self
                .study_ids
                .iter()
                .position(|study| *study == id)
                .is_some_and(|idx| !self.studies.indicators()[idx].is_overlay())
    }

    /// Whether the host series `id` is drawn by a pane below the price pane.
    pub(super) fn in_pane(&self, id: SeriesId) -> bool {
        self.pane_layout
            .panes
            .iter()
            .any(|pane| pane.members.contains(&id))
    }

    /// Whether `pane` stays in the layout at the next sync, which drops a pane whose members
    /// have all left it unless it is preserved.
    fn pane_listed(&self, pane: &Pane) -> bool {
        pane.preserve
            || pane.members.is_empty()
            || pane.members.iter().any(|id| self.pane_member(*id))
    }

    /// Whether `pane` is drawn below or above the price pane: it is empty or has a visible member.
    pub(super) fn pane_shown(&self, pane: &Pane) -> bool {
        pane.id != PaneId::MAIN
            && self.pane_listed(pane)
            && (pane.members.is_empty()
                || pane
                    .members
                    .iter()
                    .any(|id| self.pane_member(*id) && self.is_series_visible(*id)))
    }

    /// Whether the chart draws any pane besides the price pane.
    pub(super) fn has_panes(&self) -> bool {
        self.pane_layout
            .panes
            .iter()
            .any(|pane| self.pane_shown(pane))
    }

    fn pane_index(&self, pane: PaneId) -> Option<usize> {
        self.pane_layout
            .panes
            .iter()
            .position(|item| item.id == pane && self.pane_listed(item))
    }

    /// The legend name of a study or host series.
    fn member_name(&self, id: SeriesId) -> &str {
        if let Some(idx) = self.study_ids.iter().position(|study| *study == id) {
            self.studies.indicators()[idx].name()
        } else {
            self.price_series
                .iter()
                .find(|item| item.id == id)
                .map_or("Series", |item| item.options.name.as_str())
        }
    }

    /// Removes `id` from its pane and drops that pane if it became empty and is not preserved.
    fn leave_panes(&mut self, id: SeriesId) {
        self.pane_layout.panes.retain_mut(|pane| {
            let before = pane.members.len();
            pane.members.retain(|member| *member != id);
            pane.members.len() == before || !pane.members.is_empty() || pane.preserve
        });
    }

    pub(super) fn sync_panes(&mut self) {
        let mut emptied = Vec::new();
        for at in 0..self.pane_layout.panes.len() {
            let mut members = std::mem::take(&mut self.pane_layout.panes[at].members);
            let before = members.len();
            members.retain(|id| self.pane_member(*id));
            let pane = &mut self.pane_layout.panes[at];
            if before > 0 && members.is_empty() && !pane.preserve {
                emptied.push(pane.id);
            }
            pane.members = members;
        }
        self.pane_layout
            .panes
            .retain(|pane| !emptied.contains(&pane.id));
        for idx in 0..self.study_ids.len() {
            let id = self.study_ids[idx];
            if !self.studies.indicators()[idx].is_overlay() && !self.in_pane(id) {
                self.add_study_pane(id);
            }
        }
        if self.pane_layout.maximized.is_some_and(|id| {
            id != PaneId::MAIN
                && !self
                    .pane_layout
                    .panes
                    .iter()
                    .any(|pane| pane.id == id && self.pane_shown(pane))
        }) {
            self.pane_layout.maximized = None;
        }
    }

    /// Appends a pane holding the study `id`, at the study's preferred height.
    pub(super) fn add_study_pane(&mut self, id: SeriesId) {
        let height = self
            .study_ids
            .iter()
            .position(|study| *study == id)
            .and_then(|idx| self.studies.indicators()[idx].pane().height)
            .filter(|height| height.is_finite() && *height >= 40.0);
        self.add_pane();
        let pane = self.pane_layout.panes.last_mut().expect("pane was added");
        pane.height = height;
        pane.members.push(id);
    }

    /// Adds an empty pane at the bottom and returns its id. Fill it with
    /// [`move_series_to_pane`](Self::move_series_to_pane). Once its last series or study leaves
    /// it the pane is removed, unless [`set_preserve_empty_pane`](Self::set_preserve_empty_pane)
    /// keeps it.
    pub fn add_pane(&mut self) -> PaneId {
        let id = PaneId(self.next_series);
        self.next_series += 1;
        self.pane_layout.panes.push(Pane::new(id));
        id
    }

    /// Removes a pane with the series and studies it draws. Returns `false` for the price pane
    /// or an unknown id.
    pub fn remove_pane(&mut self, pane: PaneId) -> bool {
        self.sync_panes();
        if pane == PaneId::MAIN {
            return false;
        }
        let Some(at) = self.pane_index(pane) else {
            return false;
        };
        for id in self.pane_layout.panes[at].members.clone() {
            self.remove_series(id);
        }
        self.pane_layout.panes.retain(|item| item.id != pane);
        self.sync_panes();
        true
    }

    /// The panes from top to bottom, the price pane included. A study that is not an overlay
    /// starts in a new pane at the bottom.
    pub fn panes(&self) -> Vec<PaneId> {
        self.pane_layout
            .panes
            .iter()
            .filter(|pane| self.pane_listed(pane))
            .map(|pane| pane.id)
            .collect()
    }

    /// The bar series and studies drawn in `pane`, in legend order. For the price pane these
    /// are the main series, the host series not moved to another pane and the overlay studies.
    /// Empty for an unknown pane.
    pub fn pane_series(&self, pane: PaneId) -> Vec<SeriesId> {
        if pane == PaneId::MAIN {
            self.main_series_present
                .then_some(SeriesId::MAIN)
                .into_iter()
                .chain(
                    self.price_series
                        .iter()
                        .map(|item| item.id)
                        .filter(|id| !self.in_pane(*id)),
                )
                .chain(
                    self.indicators()
                        .filter(|(_, study)| study.is_overlay())
                        .map(|(id, _)| id),
                )
                .collect()
        } else {
            self.pane_index(pane)
                .map(|at| {
                    self.pane_layout.panes[at]
                        .members
                        .iter()
                        .copied()
                        .filter(|id| self.pane_member(*id))
                        .collect()
                })
                .unwrap_or_default()
        }
    }

    /// The pane drawing the bar series or study `series`, or `None` for an unknown id.
    pub fn series_pane(&self, series: SeriesId) -> Option<PaneId> {
        if let Some(pane) = self
            .pane_layout
            .panes
            .iter()
            .find(|pane| pane.members.contains(&series))
            .filter(|_| self.pane_member(series))
        {
            return Some(pane.id);
        }
        let main = if series == SeriesId::MAIN {
            self.main_series_present
        } else {
            self.price_series.iter().any(|item| item.id == series)
                || self
                    .indicators()
                    .any(|(id, study)| id == series && study.is_overlay())
        };
        main.then_some(PaneId::MAIN)
    }

    /// Moves a host series or study into `pane`, after the series and studies already there.
    /// A study moved to the price pane becomes an overlay. Below the price pane a host series
    /// shares the pane's price scale instead of its [`SeriesScale`](crate::model::SeriesScale).
    /// Returns `false` for the main series, an unknown id or pane, or a custom study
    /// presentation that rejects placement edits.
    pub fn move_series_to_pane(&mut self, series: SeriesId, pane: PaneId) -> bool {
        self.sync_panes();
        if self.pane_index(pane).is_none() {
            return false;
        }
        if let Some(idx) = self.study_ids.iter().position(|id| *id == series) {
            self.record_study(series);
            if !self.studies.indicators_mut()[idx].set_overlay(pane == PaneId::MAIN) {
                return false;
            }
        } else if !self.price_series.iter().any(|item| item.id == series) {
            return false;
        }
        if self.series_pane(series) != Some(pane) {
            self.leave_panes(series);
            if let Some(target) = self
                .pane_layout
                .panes
                .iter_mut()
                .find(|item| item.id == pane && pane != PaneId::MAIN)
            {
                target.members.push(series);
            }
        }
        self.sync_panes();
        true
    }

    /// Moves a host series or study into a new pane at the bottom.
    fn move_to_new_pane(&mut self, series: SeriesId) -> bool {
        self.sync_panes();
        let pane = self.add_pane();
        let moved = self.move_series_to_pane(series, pane);
        if moved {
            let height = self
                .study_ids
                .iter()
                .position(|study| *study == series)
                .and_then(|idx| self.studies.indicators()[idx].pane().height)
                .filter(|height| height.is_finite() && *height >= 40.0);
            if let Some(item) = self
                .pane_layout
                .panes
                .iter_mut()
                .find(|item| item.id == pane)
            {
                item.height = height;
            }
        } else {
            self.pane_layout.panes.retain(|item| item.id != pane);
        }
        moved
    }

    /// Moves a study to the main pane, another study's pane, or its own new pane.
    /// Use `SeriesId::MAIN`, the destination study id, or the moved study's own id respectively.
    /// Returns `false` for an unknown id or a custom presentation that rejects placement edits.
    /// [`move_series_to_pane`](Self::move_series_to_pane) addresses panes by [`PaneId`].
    pub fn move_indicator_to_pane(&mut self, study: SeriesId, target: SeriesId) -> bool {
        self.sync_panes();
        if !self.study_ids.contains(&study) {
            return false;
        }
        if target == study {
            return self.move_to_new_pane(study);
        }
        let pane = if target == SeriesId::MAIN {
            PaneId::MAIN
        } else {
            match self.series_pane(target) {
                Some(pane) if pane != PaneId::MAIN && self.study_ids.contains(&target) => pane,
                _ => return false,
            }
        };
        self.move_series_to_pane(study, pane)
    }

    /// Moves `pane` to position `index` from the top, the price pane counting as a pane; an
    /// index past the bottom moves it to the bottom. Returns `false` for an unknown pane.
    pub fn move_pane(&mut self, pane: PaneId, index: usize) -> bool {
        self.sync_panes();
        let Some(at) = self.pane_index(pane) else {
            return false;
        };
        let item = self.pane_layout.panes.remove(at);
        let index = index.min(self.pane_layout.panes.len());
        self.pane_layout.panes.insert(index, item);
        true
    }

    /// Swaps the positions of two panes, either of which may be the price pane. Returns `false`
    /// for an unknown pane.
    pub fn swap_panes(&mut self, first: PaneId, second: PaneId) -> bool {
        self.sync_panes();
        let (Some(first), Some(second)) = (self.pane_index(first), self.pane_index(second)) else {
            return false;
        };
        self.pane_layout.panes.swap(first, second);
        true
    }

    /// The height of `pane` in logical pixels as drawn in the last frame, or `None` when it was
    /// not drawn, such as a hidden pane or one left out while another is maximized.
    pub fn pane_height(&self, pane: PaneId) -> Option<f32> {
        self.pane_layout
            .panes
            .iter()
            .find(|item| item.id == pane)
            .map(|item| item.rect.height())
            .filter(|height| *height > 0.0)
    }

    /// Gives `pane` this height in logical pixels from the next frame by setting its stretch
    /// factor, so that later resizes keep the proportions. The other expanded panes share the
    /// rest by their own factors, and every pane keeps at least 40 pixels. Returns `false` for
    /// an unknown pane or a height that is not finite and positive.
    pub fn set_pane_height(&mut self, pane: PaneId, height: f32) -> bool {
        self.sync_panes();
        let Some(at) = self.pane_index(pane) else {
            return false;
        };
        if !(height.is_finite() && height > 0.0) {
            return false;
        }
        self.pane_layout.panes[at].height = Some(height);
        true
    }

    /// The stretch factor of `pane`, or `None` for an unknown pane. Expanded panes share the
    /// chart height in proportion to their factors: 3 for the price pane and 1 for the others
    /// by default. A study's preferred pane height, a dragged separator and
    /// [`set_pane_height`](Self::set_pane_height) set it on the frame that lays the pane out.
    pub fn pane_stretch_factor(&self, pane: PaneId) -> Option<f32> {
        self.pane_index(pane)
            .map(|at| self.pane_layout.panes[at].stretch)
    }

    /// Sets the stretch factor of `pane`, replacing a height not yet applied. Returns `false`
    /// for an unknown pane or a factor that is not finite and positive.
    pub fn set_pane_stretch_factor(&mut self, pane: PaneId, factor: f32) -> bool {
        self.sync_panes();
        let Some(at) = self.pane_index(pane) else {
            return false;
        };
        if !(factor.is_finite() && factor > 0.0) {
            return false;
        }
        let pane = &mut self.pane_layout.panes[at];
        pane.stretch = factor;
        pane.height = None;
        true
    }

    /// Keeps `pane` when its last series or study leaves it, instead of removing it. Returns
    /// `false` for the price pane or an unknown id.
    pub fn set_preserve_empty_pane(&mut self, pane: PaneId, preserve: bool) -> bool {
        self.sync_panes();
        let Some(at) = self.pane_index(pane).filter(|_| pane != PaneId::MAIN) else {
            return false;
        };
        self.pane_layout.panes[at].preserve = preserve;
        true
    }

    /// Whether `pane` is kept when it becomes empty; the price pane always is.
    pub fn preserves_empty_pane(&self, pane: PaneId) -> bool {
        pane == PaneId::MAIN
            || self
                .pane_index(pane)
                .is_some_and(|at| self.pane_layout.panes[at].preserve)
    }

    /// Sets the behaviour and colours of the pane separators (builder pattern).
    pub fn with_pane_options(mut self, options: PaneOptions) -> Self {
        self.pane_options = options;
        self
    }

    /// The behaviour and colours of the pane separators.
    pub fn pane_options(&self) -> &PaneOptions {
        &self.pane_options
    }

    /// The behaviour and colours of the pane separators, to change them from the next frame.
    pub fn pane_options_mut(&mut self) -> &mut PaneOptions {
        &mut self.pane_options
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
        });
    }

    pub(super) fn main_pane_actions(&mut self, ui: &mut Ui, rect: Rect, plot: Rect) {
        if rect.height() < 48.0 {
            return;
        }
        let rect = Rect::from_min_max(rect.min + egui::vec2(0.0, 24.0), rect.max);
        if self.has_panes() {
            self.pane_maximize_button(ui, PaneId::MAIN, rect, plot);
        }
        let movable: Vec<_> = self
            .pane_series(PaneId::MAIN)
            .into_iter()
            .filter(|id| *id != SeriesId::MAIN)
            .collect();
        if movable.is_empty() {
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
            for id in movable {
                ui.menu_button(self.member_name(id), |ui| {
                    if ui.button("Move to new pane").clicked() {
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
                        if ui.button(format!("Move to {title} pane")).clicked() {
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
                if ui.add_enabled(enabled, egui::Button::new(label)).clicked() {
                    action = Some(command);
                    ui.close();
                }
            }
            let members = self.pane_series(id);
            if !members.is_empty() {
                ui.separator();
            }
            for member in &members {
                ui.menu_button(self.member_name(*member), |ui| {
                    if ui.button("Move to main pane").clicked() {
                        action = Some(6);
                        move_member = Some(*member);
                        ui.close();
                    }
                    if ui
                        .add_enabled(members.len() > 1, egui::Button::new("Move to new pane"))
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
