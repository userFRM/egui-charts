//! Pane placement, layout state and interaction controls.

mod controls;
mod indicator;
mod render;
pub use indicator::{IndicatorCoordParams, IndicatorPane, IndicatorPaneConfig, PaneInteraction};
pub use render::RenderedIndicatorPane;

use super::volume;
use super::{Chart, SeriesId};
#[cfg(feature = "drawings")]
use crate::drawings::persistence::StoredPane;
use egui::{Color32, Rect, Vec2};

/// A study's name and input values, which identify it across charts.
type PaneStudy = (String, Vec<(String, serde_json::Value)>);

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
    /// Overlay draw order from back to front, with the main series as the divider.
    pub(super) main: Vec<SeriesId>,
}

impl Default for PaneLayout {
    fn default() -> Self {
        Self {
            panes: vec![Pane::new(PaneId::MAIN)],
            maximized: None,
            main: vec![SeriesId::MAIN],
        }
    }
}

/// A change of a study's place in the draw order of its pane, where each item is drawn over the
/// ones before it. In the main pane the price series is one of the items.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VisualOrder {
    /// Draw over every other item of the pane.
    BringToFront,
    /// Swap places with the item drawn just over it.
    BringForward,
    /// Swap places with the item drawn just under it.
    SendBackward,
    /// Draw under every other item of the pane.
    SendToBack,
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

    /// The price mapping of the pane's scale over its panel rect in the last frame.
    pub(super) fn price_map(&self) -> crate::scales::PriceMap {
        self.scale
            .map(self.scale.transform(), self.rect.top(), self.rect.bottom())
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
        let overlays: Vec<_> = self
            .study_ids
            .iter()
            .zip(self.studies.indicators())
            .filter(|(_, study)| study.is_overlay())
            .map(|(id, _)| *id)
            .collect();
        let series: Vec<_> = self.main_pane_series().map(|item| item.id).collect();
        let main = &mut self.pane_layout.main;
        main.retain(|id| *id == SeriesId::MAIN || overlays.contains(id) || series.contains(id));
        if !main.contains(&SeriesId::MAIN) {
            main.insert(0, SeriesId::MAIN);
        }
        let mut after = main
            .iter()
            .position(|id| *id == SeriesId::MAIN)
            .unwrap_or(0)
            + 1;
        for id in series {
            if !main.contains(&id) {
                main.insert(after.min(main.len()), id);
            }
            after = main.iter().position(|member| *member == id).unwrap_or(0) + 1;
        }
        for id in overlays {
            if !main.contains(&id) {
                main.push(id);
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

    /// The pane height the study `id` prefers, if it declares a usable one.
    fn study_pane_height(&self, id: SeriesId) -> Option<f32> {
        self.study_ids
            .iter()
            .position(|study| *study == id)
            .and_then(|idx| self.studies.indicators()[idx].pane().height)
            .filter(|height| height.is_finite() && *height >= 40.0)
    }

    /// Appends a pane holding the study `id`, at the study's preferred height.
    pub(super) fn add_study_pane(&mut self, id: SeriesId) {
        let height = self.study_pane_height(id);
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

    /// The pane of `other` that corresponds to the study pane `pane` of this chart, for linking
    /// the drawings of charts in a [`ChartLayout`](super::ChartLayout).
    ///
    /// Panes correspond when they hold the same studies, with equal names and input values in
    /// the same order. Of several such panes the n-th here corresponds to the n-th there. The
    /// price pane and panes without studies have no counterpart.
    pub fn linked_pane(&self, pane: PaneId, other: &Chart) -> Option<PaneId> {
        let (studies, rank) = self.pane_identity(pane)?;
        other.identified_pane(&studies, rank)
    }

    /// The studies of study pane `pane` and its rank among the panes holding the same ones, or
    /// `None` for the price pane and panes without studies.
    fn pane_identity(&self, pane: PaneId) -> Option<(Vec<PaneStudy>, usize)> {
        if pane == PaneId::MAIN {
            return None;
        }
        let studies = self.pane_studies(pane);
        if studies.is_empty() {
            return None;
        }
        let rank = self
            .panes()
            .into_iter()
            .take_while(|other| *other != pane)
            .filter(|other| *other != PaneId::MAIN && self.pane_studies(*other) == studies)
            .count();
        Some((studies, rank))
    }

    /// The `rank`-th study pane holding `studies`, from the top.
    fn identified_pane(&self, studies: &[PaneStudy], rank: usize) -> Option<PaneId> {
        self.panes()
            .into_iter()
            .filter(|pane| *pane != PaneId::MAIN && self.pane_studies(*pane) == studies)
            .nth(rank)
    }

    /// The identity of study pane `pane` for a stored drawing, as in [`Self::linked_pane`].
    #[cfg(feature = "drawings")]
    pub(crate) fn stored_pane(&self, pane: PaneId) -> Option<StoredPane> {
        self.pane_identity(pane)
            .map(|(studies, rank)| StoredPane { studies, rank })
    }

    /// The study a drawing stored in `stored` belongs to on this chart: the first study of the
    /// corresponding pane, or `None`, the price pane, when there is none.
    #[cfg(feature = "drawings")]
    pub(crate) fn stored_pane_owner(&self, stored: &StoredPane) -> Option<usize> {
        let pane = self.identified_pane(&stored.studies, stored.rank)?;
        self.pane_series(pane).first().map(|id| id.0)
    }

    /// The name and input values of each study in `pane`, in legend order.
    fn pane_studies(&self, pane: PaneId) -> Vec<PaneStudy> {
        self.pane_series(pane)
            .into_iter()
            .filter_map(|id| self.indicators().find(|(study, _)| *study == id))
            .map(|(_, study)| {
                // Parsed from text like a stored pane's values, which can differ in the last bit.
                let inputs = study.inputs().into_iter().map(|input| {
                    let text = serde_json::to_string(&input.value).unwrap_or_default();
                    (input.id, serde_json::from_str(&text).unwrap_or_default())
                });
                (study.name().to_owned(), inputs.collect())
            })
            .collect()
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
            let height = self.study_pane_height(series);
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
    /// Moves a study to a new pane directly above or below its current pane.
    /// Returns `false` for an unknown study or a presentation that rejects placement edits.
    #[cfg(feature = "ui")]
    pub(crate) fn move_indicator_to_new_pane(&mut self, study: SeriesId, above: bool) -> bool {
        self.sync_panes();
        let Some(from) = self
            .series_pane(study)
            .and_then(|pane| self.pane_index(pane))
        else {
            return false;
        };
        let source = &self.pane_layout.panes[from];
        let kept = source.id == PaneId::MAIN || source.preserve || source.members.len() > 1;
        if !self.move_indicator_to_pane(study, study) {
            return false;
        }
        let to = from + usize::from(kept && !above);
        let pane = self
            .series_pane(study)
            .expect("study was moved to a new pane");
        self.move_pane(pane, to)
    }

    /// Moves study `id` in the draw order of its pane, among the pane's studies and, in the main
    /// pane, the price series. Returns `false` for an id that is not a study of the chart.
    pub fn reorder_indicator(&mut self, id: SeriesId, order: VisualOrder) -> bool {
        if !self.study_ids.contains(&id) {
            return false;
        }
        self.sync_panes();
        let layout = &mut self.pane_layout;
        let Some(items) = std::iter::once(&mut layout.main)
            .chain(layout.panes.iter_mut().map(|pane| &mut pane.members))
            .find(|items| items.contains(&id))
        else {
            return false;
        };
        let at = items.iter().position(|item| *item == id).unwrap();
        let last = items.len() - 1;
        let to = match order {
            VisualOrder::BringToFront => last,
            VisualOrder::BringForward => (at + 1).min(last),
            VisualOrder::SendBackward => at.saturating_sub(1),
            VisualOrder::SendToBack => 0,
        };
        let item = items.remove(at);
        items.insert(to, item);
        true
    }
}

/// Pre-computed layout rectangles for the chart's sub-regions.
///
/// Use [`Chart::calculate_layout_rects`] to obtain these rects for a given
/// widget area. They are useful for external hit-testing (e.g., determining
/// whether a click landed on the price area, volume area, or legend).
#[derive(Clone, Copy, Debug)]
#[non_exhaustive]
pub struct ChartLayoutRects {
    /// The overall widget rect (entire chart area including axes and padding).
    pub widget_rect: Rect,
    /// The main price/candle area where OHLC data is rendered.
    pub price_rect: Rect,
    /// The volume area inside the price pane (empty if volume is hidden).
    pub volume_rect: Rect,
    /// The legend/OHLC info area at the top of the chart.
    pub legend_rect: Rect,
}

impl Default for ChartLayoutRects {
    fn default() -> Self {
        Self {
            widget_rect: Rect::NOTHING,
            price_rect: Rect::NOTHING,
            volume_rect: Rect::NOTHING,
            legend_rect: Rect::NOTHING,
        }
    }
}

impl Chart {
    /// Calculate the layout sub-rects for a given widget rect.
    ///
    /// Given the overall widget area, this computes where the price chart,
    /// volume bars, and legend/OHLC header will be drawn. Useful for
    /// external hit-testing, overlay placement, or custom drawing on top
    /// of specific chart regions.
    ///
    /// The layout respects current config flags like `show_ohlc_info`,
    /// `show_time_labels`, and `show_volume`.
    pub fn calculate_layout_rects(&self, widget_rect: Rect) -> ChartLayoutRects {
        self.pane_layout_rects(widget_rect, self.config.show_time_labels)
    }

    pub(in crate::widget) fn pane_layout_rects(
        &self,
        widget_rect: Rect,
        show_time_labels: bool,
    ) -> ChartLayoutRects {
        let padding = self
            .config
            .padding
            .max(0.0)
            .min(widget_rect.width().min(widget_rect.height()).max(0.0) / 2.0);
        let study_axes = if self.config.show_left_axis || self.config.show_right_axis {
            self.studies
                .indicators()
                .iter()
                .filter(|study| {
                    study.is_visible()
                        && study.is_overlay()
                        && !study.is_volume()
                        && study.scale() == crate::studies::StudyScale::Independent
                })
                .count() as f32
                * self
                    .measured_axis_width
                    .max(self.config.right_axis_width)
                    .max(self.config.left_axis_width)
        } else {
            0.0
        };
        let series_axes = self.series_axis_keys();
        let series_left = series_axes
            .iter()
            .filter(|key| **key != crate::model::SeriesScale::Right)
            .count() as f32
            * self
                .measured_axis_width
                .max(self.config.right_axis_width)
                .max(self.config.left_axis_width);
        let series_right = if series_axes.contains(&crate::model::SeriesScale::Right) {
            self.measured_axis_width.max(self.config.right_axis_width)
        } else {
            0.0
        };
        let study_axes = study_axes + series_left;
        let left_axis = if self.config.show_left_axis {
            self.config.left_axis_width.max(self.measured_axis_width)
        } else {
            0.0
        };
        let right_axis = if self.config.show_right_axis {
            self.config.right_axis_width.max(self.measured_axis_width)
        } else {
            0.0
        } + series_right;
        let study_axes = study_axes
            .min((widget_rect.width() - 2.0 * padding - left_axis - right_axis).max(0.0) * 0.5);
        let left_axis = left_axis + study_axes;
        let bottom_padding = if show_time_labels {
            self.time_axis_height()
        } else {
            0.0
        };
        let price_rect = Rect::from_min_size(
            widget_rect.min + Vec2::new((padding + left_axis).min(widget_rect.width()), padding),
            Vec2::new(
                (widget_rect.width() - 2.0 * padding - left_axis - right_axis).max(0.0),
                (widget_rect.height() - 2.0 * padding - bottom_padding).max(0.0),
            ),
        );
        let legend_rect = if self.config.show_ohlc_info {
            price_rect.shrink(8.0).intersect(Rect::from_min_size(
                price_rect.min + Vec2::splat(8.0),
                Vec2::new(price_rect.width(), 24.0),
            ))
        } else {
            Rect::NOTHING
        };
        let volume_rect = if self.shows_volume() {
            volume::overlay_rect(price_rect, &self.config)
        } else {
            Rect::ZERO
        };

        ChartLayoutRects {
            widget_rect,
            price_rect,
            volume_rect,
            legend_rect,
        }
    }
}

#[cfg(all(test, feature = "studies"))]
mod tests {
    use crate::widget::test_frames::chart;
    use egui::{Context, Pos2, RawInput, Rect, Vec2};

    /// A host series moved into a pane leaves the price scale, panes share the height by
    /// stretch factor across resizes, and a pane can sit above the price pane.
    #[test]
    fn host_panes_take_series_share_height_by_stretch_and_reorder() {
        use crate::widget::{PaneId, SeriesId, SeriesOptions};
        let ctx = Context::default();
        let mut chart = chart(&[100.0], &[110.0]);
        let series = chart.add_series(SeriesOptions::default());
        let bars = (chart.bars(SeriesId::MAIN).iter().enumerate())
            .map(|(i, bar)| {
                let close = 1000.0 + i as f64;
                crate::model::Bar::new(bar.time, close, close, close, close, 1.0)
            })
            .collect();
        chart.set_data(series, bars);
        let pane = chart.add_pane();
        assert!(chart.move_series_to_pane(series, pane));
        assert!(chart.set_pane_stretch_factor(pane, 3.0));
        assert!(chart.move_pane(pane, 0));
        assert_eq!(chart.panes(), [pane, PaneId::MAIN]);
        assert_eq!(chart.pane_series(pane), [series]);
        assert_eq!(chart.pane_series(PaneId::MAIN), [SeriesId::MAIN]);
        let show = |chart: &mut crate::widget::Chart, height: f32| {
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(800.0, height))),
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| chart.show(ui));
                },
            )
            .drop_without_applying_deltas();
        };
        show(&mut chart, 600.0);
        let heights = |chart: &crate::widget::Chart| {
            (
                chart.pane_height(pane).unwrap(),
                chart.pane_height(PaneId::MAIN).unwrap(),
            )
        };
        let (top, main) = heights(&chart);
        assert!((top - main).abs() < 0.01, "equal factors share equally");
        assert!(chart.pane_layout.panes[0].rect.bottom() <= chart.get_rendered_price_rect().top());
        let range = chart.pane_layout.panes[0].scale.range();
        assert!(range.min < 1000.0 && range.max > 1099.0);
        assert!(chart.get_chart_mapping().price.range().max < 1000.0);
        assert!(chart.set_pane_height(pane, 200.0));
        show(&mut chart, 600.0);
        let (top, main) = heights(&chart);
        assert!((top - 200.0).abs() < 0.01);
        let share = top / (top + main);
        show(&mut chart, 400.0);
        let (top, main) = heights(&chart);
        assert!(
            (top / (top + main) - share).abs() < 1e-4,
            "a resize keeps the shares"
        );
        // A factor near f32::MAX gives the pane most of the height instead of overflowing.
        assert!(chart.set_pane_stretch_factor(pane, 3e38));
        show(&mut chart, 600.0);
        let (top, main) = heights(&chart);
        assert!(top.is_finite() && top > main, "{top} {main}");
        assert!(chart.set_preserve_empty_pane(pane, true));
        assert!(chart.move_series_to_pane(series, PaneId::MAIN));
        assert_eq!(chart.panes(), [pane, PaneId::MAIN]);
        assert!(chart.set_preserve_empty_pane(pane, false));
        assert!(chart.move_series_to_pane(series, pane));
        assert!(chart.move_series_to_pane(series, PaneId::MAIN));
        assert_eq!(chart.panes(), [PaneId::MAIN]);
    }

    #[test]
    fn pane_defaults_do_not_depend_on_the_study_name() {
        let ctx = Context::default();
        let mut chart = chart(&[100.0], &[110.0]);
        chart.add_indicator(Box::new(
            crate::studies::CustomIndicator::new("RSI", |bars| {
                bars.iter()
                    .map(|_| crate::studies::IndicatorValue::Single(200.0))
                    .collect()
            })
            .with_overlay(false),
        ));
        crate::widget::test_frames::frame(&ctx, &mut chart, 0.0, Vec::new());
        let pane = &chart.get_rendered_indicator_panes()[0];
        // A flat value of 200 gets 10% of a 1% span (2): [199.8, 200.2].
        assert!((pane.y_min - 199.8).abs() < 1e-9);
        assert!((pane.y_max - 200.2).abs() < 1e-9);
    }
}
