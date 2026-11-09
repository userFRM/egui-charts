//! Pane placement, layout state and interaction controls.

mod controls;
mod indicator;
mod render;
pub use crate::chart::primitives::PaneId;
pub use indicator::{IndicatorCoordParams, IndicatorPane, IndicatorPaneConfig, PaneInteraction};
pub use render::RenderedIndicatorPane;

use super::volume;
use super::{Chart, SeriesId, Watermark};
#[cfg(feature = "drawings")]
use crate::drawings::persistence::StoredPane;
use egui::{Color32, Rect, Vec2};

/// A study's name and input values, which identify it across charts.
type PaneStudy = (String, Vec<(String, serde_json::Value)>);

/// `value` as a stored pane holds it: parsed back from text, since a stored value can differ
/// from the live one in the last bit.
fn stored_value(value: &crate::model::InputValue) -> serde_json::Value {
    let text = serde_json::to_string(value).unwrap_or_default();
    serde_json::from_str(&text).unwrap_or_default()
}

/// `stored` under the input ids its study takes now, upgraded as `StudyTemplate::create_with`
/// upgrades a template saved under earlier ones, or `None` when it needs no change. `study` is
/// the chart's study it is matched against and `declared` its inputs; a study that lacks an
/// input the upgrade adds, as a host study registered under a built-in's name does, keeps the
/// record as stored.
#[cfg(feature = "studies")]
fn upgraded_study(
    (name, stored): &PaneStudy,
    study: &dyn crate::studies::Indicator,
    declared: &[crate::studies::StudyInput],
) -> Option<PaneStudy> {
    let inputs = stored
        .iter()
        .map(|(id, value)| Some((id.clone(), serde_json::from_value(value.clone()).ok()?)))
        .collect::<Option<_>>()?;
    let template = crate::studies::StudyTemplate {
        name: name.clone(),
        inputs,
        // Read only by the upgrade's presentation changes, which are not kept here.
        settings: study.settings(),
        visible: true,
        scale: None,
    }
    .upgraded(declared)?;
    let takes = template.inputs.iter().all(|(id, _)| {
        stored.iter().any(|(saved, _)| saved == id) || declared.iter().any(|input| input.id == *id)
    });
    // Parsed from the stored text once already: another round trip could move a number by the
    // last bit.
    let inputs = template
        .inputs
        .iter()
        .map(|(id, value)| (id.clone(), serde_json::to_value(value).unwrap_or_default()));
    takes.then(|| (template.name.clone(), inputs.collect()))
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
    /// Every existing pane from top to bottom.
    pub(super) panes: Vec<Pane>,
    pub(super) maximized: Option<PaneId>,
    /// Draw order in the initial price pane, including the main series while placed there.
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
    /// Series and studies in draw order; the initial price pane uses `PaneLayout::main`.
    pub(super) members: Vec<SeriesId>,
    /// Share of the expanded panes' height, relative to the factors of the others.
    pub(super) stretch: f32,
    /// Height to reach on the next layout, which turns it into a stretch factor.
    pub(super) height: Option<f32>,
    /// Keep the pane when its last member leaves it.
    pub(super) preserve: bool,
    pub(super) collapsed: Option<bool>,
    pub(super) axes: crate::model::PaneAxisOptions,
    pub(super) scale: crate::scales::PriceScale,
    /// Named assignments other than the pane's default right scale, in creation order.
    pub(super) scales: Vec<(crate::model::SeriesScale, crate::scales::PriceScale)>,
    /// Panel rect in the last frame; [`Rect::NOTHING`] when the pane was not drawn.
    pub(super) rect: Rect,
    /// Host watermark painted behind this pane's series; see [`Chart::set_pane_watermark`].
    pub(super) watermark: Option<Watermark>,
}

impl Pane {
    pub(super) fn new(id: PaneId) -> Self {
        Self {
            id,
            members: Vec::new(),
            stretch: Self::default_stretch(id),
            height: None,
            preserve: false,
            collapsed: None,
            axes: Default::default(),
            scale: Self::new_scale(),
            scales: Vec::new(),
            rect: Rect::NOTHING,
            watermark: None,
        }
    }

    fn new_scale() -> crate::scales::PriceScale {
        crate::scales::PriceScale::new(crate::scales::PriceScaleOptions {
            scale_margins: crate::scales::PriceScaleMargins {
                top: 0.0,
                bottom: 0.0,
            },
            ..Default::default()
        })
    }

    /// The scale of an assignment, or `None` before anything creates it.
    pub(super) fn own_scale(
        &self,
        key: crate::model::SeriesScale,
    ) -> Option<&crate::scales::PriceScale> {
        if key == crate::model::SeriesScale::Right {
            Some(&self.scale)
        } else {
            self.scales
                .iter()
                .find(|(item, _)| *item == key)
                .map(|(_, scale)| scale)
        }
    }

    pub(super) fn assigned_scale(
        &self,
        key: crate::model::SeriesScale,
    ) -> &crate::scales::PriceScale {
        self.own_scale(key).unwrap_or(&self.scale)
    }

    pub(super) fn assigned_scale_mut(
        &mut self,
        key: crate::model::SeriesScale,
    ) -> &mut crate::scales::PriceScale {
        if key == crate::model::SeriesScale::Right {
            &mut self.scale
        } else {
            let at = match self.scales.iter().position(|(item, _)| *item == key) {
                Some(at) => at,
                None => {
                    self.scales.push((key, Self::new_scale()));
                    self.scales.len() - 1
                }
            };
            &mut self.scales[at].1
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
    fn pane_axes(
        &self,
        id: PaneId,
        rect: Rect,
        plot: Rect,
    ) -> Vec<(crate::model::SeriesScale, Rect)> {
        self.pane_scale_axes(id, rect, plot)
            .filter_map(|(key, rect)| match key {
                super::PaneScale::Named(key) => Some((key, rect)),
                super::PaneScale::Study(_) => None,
            })
            .collect()
    }

    /// Whether `id` is drawn below the price pane when placed there: a host series or a study
    /// that is not an overlay.
    fn pane_member(&self, id: SeriesId) -> bool {
        (id == SeriesId::MAIN && self.main_series_present)
            || self.price_series.iter().any(|item| item.id == id)
            || self
                .study_ids
                .iter()
                .position(|study| *study == id)
                .is_some_and(|idx| {
                    !self.studies.indicators()[idx].is_overlay()
                        || self.pane_index(PaneId::MAIN).is_none()
                })
    }

    /// Whether the host series `id` is drawn by a pane below the price pane.
    pub(super) fn in_pane(&self, id: SeriesId) -> bool {
        self.pane_layout
            .panes
            .iter()
            .any(|pane| pane.members.contains(&id))
    }

    /// Whether `pane` stays in the layout at the next sync. Empty panes are dropped when their
    /// members leave, unless preserved or the final pane.
    fn pane_listed(&self, pane: &Pane) -> bool {
        self.pane_layout.panes.len() == 1
            || pane.preserve
            || pane.members.is_empty()
            || pane.members.iter().any(|id| self.pane_member(*id))
    }

    /// Whether `pane` is drawn below or above the price pane: it is empty, owns a primitive or has a visible member.
    pub(super) fn pane_shown(&self, pane: &Pane) -> bool {
        pane.id != PaneId::MAIN
            && self.pane_listed(pane)
            && (pane.members.is_empty()
                || self
                    .primitives
                    .iter()
                    .any(|item| item.pane == Some(pane.id))
                || pane
                    .members
                    .iter()
                    .any(|id| self.pane_member(*id) && self.is_series_visible(*id)))
    }

    /// Whether rendering needs the shared layout for panes other than the original price pane.
    pub(super) fn has_panes(&self) -> bool {
        self.pane_index(PaneId::MAIN).is_none()
            || self
                .pane_layout
                .panes
                .iter()
                .any(|pane| self.pane_shown(pane))
    }

    pub(super) fn pane_index(&self, pane: PaneId) -> Option<usize> {
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
    pub(super) fn leave_panes(&mut self, id: SeriesId) {
        let mut remaining = self.pane_layout.panes.len();
        self.pane_layout.panes.retain_mut(|pane| {
            let before = pane.members.len();
            pane.members.retain(|member| *member != id);
            let keep = pane.id == PaneId::MAIN
                || pane.members.len() == before
                || !pane.members.is_empty()
                || pane.preserve
                || remaining == 1;
            if !keep {
                remaining -= 1;
            }
            keep
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
        if emptied.len() == self.pane_layout.panes.len() {
            emptied.pop();
        }
        self.pane_layout
            .panes
            .retain(|pane| !emptied.contains(&pane.id));
        let main_present = self.pane_index(PaneId::MAIN).is_some();
        if !main_present {
            let target = self.series_pane(SeriesId::MAIN);
            let unplaced: Vec<_> = self
                .price_series
                .iter()
                .filter(|item| !self.in_pane(item.id))
                .map(|item| item.id)
                .collect();
            if let Some(pane) = self
                .pane_layout
                .panes
                .iter_mut()
                .find(|pane| target.is_none_or(|id| pane.id == id))
            {
                pane.members.extend(unplaced);
            }
        }
        for idx in 0..self.study_ids.len() {
            let id = self.study_ids[idx];
            if (!main_present || !self.studies.indicators()[idx].is_overlay()) && !self.in_pane(id)
            {
                self.add_study_pane(id);
            }
        }
        let main = std::mem::take(&mut self.pane_layout.main);
        self.pane_layout.main = self.main_pane_order(main);
        if self.pane_layout.maximized.is_some_and(|id| {
            !self
                .pane_layout
                .panes
                .iter()
                .any(|pane| pane.id == id && (id == PaneId::MAIN || self.pane_shown(pane)))
        }) {
            self.pane_layout.maximized = None;
        }
    }

    /// The price pane's draw order, including series and studies added since its last frame.
    /// Accepts the existing order to reuse its allocation on each frame.
    pub(super) fn main_pane_order(&self, mut main: Vec<SeriesId>) -> Vec<SeriesId> {
        if self.pane_index(PaneId::MAIN).is_none() {
            main.clear();
            return main;
        }
        let overlays: Vec<_> = self
            .study_ids
            .iter()
            .zip(self.studies.indicators())
            .filter(|(_, study)| study.is_overlay())
            .map(|(id, _)| *id)
            .collect();
        let series: Vec<_> = self
            .price_series
            .iter()
            .filter(|item| !self.in_pane(item.id))
            .map(|item| item.id)
            .collect();
        let main_here = !self.in_pane(SeriesId::MAIN);
        main.retain(|id| {
            (*id == SeriesId::MAIN && main_here) || overlays.contains(id) || series.contains(id)
        });
        if main_here && !main.contains(&SeriesId::MAIN) {
            main.insert(0, SeriesId::MAIN);
        }
        let mut after = main
            .iter()
            .position(|id| *id == SeriesId::MAIN)
            .map_or(0, |at| at + 1);
        for id in series {
            if !main.contains(&id) {
                main.insert(after.min(main.len()), id);
            }
            after = after.max(main.iter().position(|member| *member == id).unwrap_or(0) + 1);
        }
        for id in overlays {
            if !main.contains(&id) {
                main.push(id);
            }
        }
        main
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
        if let Some(idx) = self.study_ids.iter().position(|study| *study == id)
            && self.studies.indicators()[idx].is_overlay()
        {
            self.studies.indicators_mut()[idx].set_overlay(false);
        }
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

    /// Paints a host watermark behind the series in `pane`, sized, aligned and clipped to its
    /// plot. `None` clears only that pane's watermark. It follows pane resizing and ordering,
    /// is hidden with a collapsed pane, and is dropped when the pane is removed.
    /// Returns `false` for an unknown or removed pane without changing another pane.
    pub fn set_pane_watermark(&mut self, pane: PaneId, watermark: Option<Watermark>) -> bool {
        self.sync_panes();
        let Some(at) = self.pane_index(pane) else {
            return false;
        };
        self.pane_layout.panes[at].watermark = watermark;
        true
    }

    /// Removes a pane, its series, studies, markers, price lines and drawings, including locked
    /// drawings. Attached primitives detach on the next frame. The original price pane can be
    /// removed; its retained data still supplies the shared horizontal mapping and study inputs.
    /// A main series removed with it stays out of the layout and is not drawn, while its bars
    /// keep taking [`set_data`](Self::set_data), [`update`](Self::update) and the feed.
    /// Surviving panes keep their ids, order and data. Without the original pane, new bar series
    /// and overlay studies join the surviving main series. When that series is absent, new
    /// bar series use the first pane and studies start in their own panes.
    /// Returns `false` without removing content for an unknown id or the final pane.
    pub fn remove_pane(&mut self, pane: PaneId) -> bool {
        self.sync_panes();
        if self.panes().len() <= 1 {
            return false;
        }
        if self.pane_index(pane).is_none() {
            return false;
        }
        let members = self.pane_series(pane);
        #[cfg(feature = "drawings")]
        {
            let drawings: Vec<_> = self
                .drawings
                .drawings
                .iter()
                .filter(|drawing| {
                    let owner = match drawing.pane {
                        None => Some(self.main_series_pane()),
                        Some(id) => self
                            .series_pane(SeriesId(id))
                            .or_else(|| self.pane_index(PaneId(id)).map(|_| PaneId(id))),
                    };
                    owner == Some(pane)
                })
                .map(|drawing| (drawing.id, drawing.locked))
                .collect();
            for (id, locked) in drawings {
                if locked {
                    self.drawings
                        .edit_drawing(id, |drawing| drawing.locked = false);
                }
                self.drawings.delete_drawing(id);
            }
        }
        let holds_main = members.contains(&SeriesId::MAIN);
        let removes_main = holds_main || (pane == PaneId::MAIN && !self.main_series_present);
        for id in members.into_iter().filter(|id| *id != SeriesId::MAIN) {
            self.remove_series(id);
        }
        // Undo of a recorded removal brings the markers back with the series.
        if removes_main && !self.history.recording() {
            self.marks = Default::default();
            self.change_marks = None;
        }
        self.pane_layout.panes.retain(|item| item.id != pane);
        // After the pane: without the original pane the feed keeps running, with no reload.
        if holds_main {
            self.remove_series(SeriesId::MAIN);
        }
        self.follow_feed_pause();
        self.sync_panes();
        true
    }

    /// Existing panes from top to bottom, including the original price pane until removed.
    /// A study that is not an overlay starts in a new pane at the bottom.
    pub fn panes(&self) -> Vec<PaneId> {
        self.pane_layout
            .panes
            .iter()
            .filter(|pane| self.pane_listed(pane))
            .map(|pane| pane.id)
            .collect()
    }

    /// The bar series and studies drawn in `pane`, in legend order. For the price pane these
    /// are the bar series not moved to another pane and the overlay studies.
    /// Empty for an unknown pane.
    pub fn pane_series(&self, pane: PaneId) -> Vec<SeriesId> {
        self.pane_series_iter(pane).collect()
    }

    pub(in crate::widget) fn pane_series_iter(
        &self,
        pane: PaneId,
    ) -> impl Iterator<Item = SeriesId> + '_ {
        let entry = self.pane_index(pane).map(|at| &self.pane_layout.panes[at]);
        let main = entry.is_some() && pane == PaneId::MAIN;
        (main && self.main_series_present && !self.in_pane(SeriesId::MAIN))
            .then_some(SeriesId::MAIN)
            .into_iter()
            .chain(
                main.then_some(&self.price_series)
                    .into_iter()
                    .flatten()
                    .map(|item| item.id)
                    .filter(|id| !self.in_pane(*id)),
            )
            .chain(
                main.then(|| self.indicators())
                    .into_iter()
                    .flatten()
                    .filter(|(_, study)| study.is_overlay())
                    .map(|(id, _)| id),
            )
            .chain(
                entry
                    .filter(|_| pane != PaneId::MAIN)
                    .into_iter()
                    .flat_map(|item| item.members.iter().copied())
                    .filter(|id| self.pane_member(*id)),
            )
    }

    /// The pane of `other` that corresponds to the study pane `pane` of this chart, for linking
    /// the drawings of charts in a [`ChartLayout`](super::ChartLayout).
    ///
    /// Panes correspond when they hold the same studies in the same order, with equal input
    /// values; an input that only `other`'s study lists counts as equal at its declared default.
    /// Of several such panes the n-th here corresponds to the n-th there. The price pane and
    /// panes without studies have no counterpart.
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
            .filter(|pane| *pane != PaneId::MAIN && self.holds_studies(*pane, studies))
            .nth(rank)
    }

    /// Whether `pane` holds `studies`, in order. A stored study is first read under the input
    /// ids its study takes now, as a study template saved before they changed is; then each
    /// listed input holds its value here, and each input not listed, such as one a later release
    /// added, the default its study declares.
    fn holds_studies(&self, pane: PaneId, studies: &[PaneStudy]) -> bool {
        let shown: Vec<_> = self
            .pane_series(pane)
            .into_iter()
            .filter_map(|id| self.indicators().find(|(study, _)| *study == id))
            .collect();
        shown.len() == studies.len()
            && shown.iter().zip(studies).all(|((_, study), stored)| {
                let inputs = study.inputs();
                #[cfg(feature = "studies")]
                let upgraded = upgraded_study(stored, *study, &inputs);
                #[cfg(feature = "studies")]
                let stored = upgraded.as_ref().unwrap_or(stored);
                let (name, stored) = stored;
                study.name() == name
                    && stored.iter().all(|(id, value)| {
                        inputs
                            .iter()
                            .any(|input| input.id == *id && stored_value(&input.value) == *value)
                    })
                    && inputs.iter().all(|input| {
                        stored.iter().any(|(id, _)| *id == input.id) || input.value == input.default
                    })
            })
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
                let inputs = study
                    .inputs()
                    .into_iter()
                    .map(|input| (input.id, stored_value(&input.value)));
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
        (main && self.pane_index(PaneId::MAIN).is_some()).then_some(PaneId::MAIN)
    }

    /// Moves a bar series, including the main series, or study into `pane`, after its members.
    /// A study moved to the price pane becomes an overlay. Scale assignments join the
    /// destination pane's scales, retaining that pane's options and manual ranges.
    /// The main series keeps its price scale and horizontal mapping.
    /// Returns `false` for an unknown id or pane, or a custom study
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
        } else if self.series_scale(series).is_none() {
            return false;
        }
        if self.series_pane(series) != Some(pane) {
            self.leave_panes(series);
            if series == SeriesId::MAIN && pane == PaneId::MAIN {
                self.pane_layout.main.push(series);
            }
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
        // Create a destination assignment before hosts query or edit it between frames.
        self.series_price_scale_mut(series);
        true
    }

    /// The pane holding the main series, or the initial price pane when it is absent.
    pub(super) fn main_series_pane(&self) -> PaneId {
        self.series_pane(SeriesId::MAIN).unwrap_or(PaneId::MAIN)
    }

    /// The draw order of the pane holding the main series.
    pub(super) fn main_series_order(&self) -> &[SeriesId] {
        let pane = self.main_series_pane();
        if pane == PaneId::MAIN {
            &self.pane_layout.main
        } else {
            &self
                .pane_layout
                .panes
                .iter()
                .find(|item| item.id == pane)
                .expect("main series belongs to pane")
                .members
        }
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

    /// Whether `pane` is kept when it becomes empty. The original price pane and the final
    /// remaining pane are always kept; an unknown or removed pane returns `false`.
    pub fn preserves_empty_pane(&self, pane: PaneId) -> bool {
        self.pane_index(pane).is_some_and(|at| {
            pane == PaneId::MAIN
                || self.pane_layout.panes.len() == 1
                || self.pane_layout.panes[at].preserve
        })
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
        use crate::model::AxisSide;
        let available = (widget_rect.width() - 2.0 * padding).max(0.0);
        let columns = |side| {
            self.pane_layout
                .panes
                .iter()
                .filter(|pane| {
                    pane.id == PaneId::MAIN
                        || pane.id == self.main_series_pane()
                        || self.pane_shown(pane)
                })
                .map(|pane| self.pane_scale_count(pane.id, side))
                .max()
                .unwrap_or(0) as f32
        };
        let right_width = self.measured_axis_width.max(self.config.right_axis_width);
        let right_axis =
            (columns(AxisSide::Right) * right_width).min(right_width.max(available * 0.5));
        let primary_left = self.primary_axis_side() == Some(AxisSide::Left);
        let primary_width = if primary_left {
            self.measured_axis_width.max(self.config.left_axis_width)
        } else {
            0.0
        };
        let extra_columns = self
            .pane_scale_count(self.main_series_pane(), AxisSide::Left)
            .saturating_sub(usize::from(primary_left)) as f32;
        let extra_width = self
            .measured_axis_width
            .max(self.config.left_axis_width)
            .max(self.config.right_axis_width);
        let main_left = primary_width
            + (extra_columns * extra_width)
                .min((available - primary_width - right_axis).max(0.0) * 0.5);
        let other_left = self
            .pane_layout
            .panes
            .iter()
            .filter(|pane| {
                pane.id != self.main_series_pane()
                    && (pane.id == PaneId::MAIN || self.pane_shown(pane))
            })
            .map(|pane| self.pane_scale_count(pane.id, AxisSide::Left))
            .max()
            .unwrap_or(0) as f32
            * self.measured_axis_width.max(self.config.left_axis_width);
        let left_axis = main_left.max(other_left).min(available * 0.5);
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
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    /// A primitive that counts its detachments.
    struct Attachment(Arc<AtomicUsize>);
    impl crate::widget::Primitive for Attachment {
        fn draw(
            &self,
            _: &egui::Painter,
            _: &crate::widget::PrimitiveView<'_>,
            _: crate::widget::PrimitivePass,
        ) {
        }
        fn detached(&mut self) {
            self.0.fetch_add(1, Ordering::Relaxed);
        }
    }

    /// A 2:1 image fits half the padded plot: its height is (pane height - 20) / 2.
    /// The following 10 point line adds 12 points to the aligned block. Layout changes must
    /// move both parts together, and neither text nor an explicitly oversized image may leak.
    #[test]
    fn pane_watermarks_follow_layout_clip_and_export_independently() {
        use crate::widget::{
            ImageExportOptions, PaneId, SeriesId, SeriesOptions, Watermark, WatermarkLine,
        };
        use egui::{Align2, Color32, Shape, load::SizedTexture};
        let ctx = egui::Context::default();
        let mut chart = chart(&[100.0], &[110.0]);
        chart.config_mut().show_volume = false;
        let main_color = Color32::from_rgb(21, 43, 65);
        let pane_color = Color32::from_rgb(32, 54, 76);
        chart.set_watermark(Some(Watermark::default().with_line(WatermarkLine::new(
            "MAIN WATERMARK",
            20.0,
            main_color,
        ))));
        let pane = chart.add_pane();
        for _ in 0..2 {
            let series = chart.add_series(SeriesOptions::default());
            chart.set_data(series, chart.bars(SeriesId::MAIN).to_vec());
            assert!(chart.move_series_to_pane(series, pane));
        }
        let texture = egui::TextureId::User(77);
        let watermark = Watermark::default()
            .with_image(
                egui::Image::from_texture(SizedTexture::new(texture, Vec2::new(200.0, 100.0)))
                    .fit_to_fraction(Vec2::splat(0.5)),
            )
            .with_line(WatermarkLine::new("PANE WATERMARK", 10.0, pane_color))
            .with_padding(10.0);
        let render = |chart: &mut crate::Chart| {
            let mut output = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        Vec2::new(800.0, 600.0),
                    )),
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| {
                        chart.show(ui);
                    });
                },
            );
            let painted = std::mem::take(&mut output.shapes);
            output.drop_without_applying_deltas();
            painted
        };
        for height in [120.0, 240.0] {
            assert!(chart.set_pane_height(pane, height));
            assert!(chart.move_pane(pane, usize::from(height == 120.0)));
            for align in [
                Align2::LEFT_TOP,
                Align2::CENTER_TOP,
                Align2::RIGHT_TOP,
                Align2::LEFT_CENTER,
                Align2::CENTER_CENTER,
                Align2::RIGHT_CENTER,
                Align2::LEFT_BOTTOM,
                Align2::CENTER_BOTTOM,
                Align2::RIGHT_BOTTOM,
            ] {
                assert!(chart.set_pane_watermark(pane, Some(watermark.clone().with_align(align))));
                let painted = render(&mut chart);
                let panel = chart
                    .pane_layout
                    .panes
                    .iter()
                    .find(|item| item.id == pane)
                    .unwrap()
                    .rect;
                let plot = Rect::from_x_y_ranges(
                    chart.get_rendered_price_rect().x_range(),
                    panel.y_range(),
                );
                assert!((plot.height() - height).abs() < 0.01);
                let inner = plot.shrink(10.0);
                let image_height = (height - 20.0) / 2.0;
                let top = match align.y() {
                    egui::Align::Min => inner.top(),
                    egui::Align::Center => inner.center().y - (image_height + 12.0) / 2.0,
                    egui::Align::Max => inner.bottom() - image_height - 12.0,
                };
                let images: Vec<_> = painted
                    .iter()
                    .filter(|shape| shape.shape.texture_id() == texture)
                    .collect();
                assert_eq!(images.len(), 1, "one image per pane, {height} {align:?}");
                let image = images[0].shape.visual_bounding_rect();
                assert!((image.height() - image_height).abs() <= 1.0);
                assert!((image.width() - 2.0 * image_height).abs() <= 1.0);
                assert!((image.top() - top).abs() <= 0.5);
                let edge = |part: Rect| match align.x() {
                    egui::Align::Min => part.left() - inner.left(),
                    egui::Align::Center => part.center().x - inner.center().x,
                    egui::Align::Max => part.right() - inner.right(),
                };
                assert!(edge(image).abs() <= 0.5);
                let text: Vec<_> = painted
                    .iter()
                    .filter_map(|shape| match &shape.shape {
                        Shape::Text(text) if text.fallback_color == pane_color => Some(text),
                        _ => None,
                    })
                    .collect();
                assert_eq!(text.len(), 1);
                let bounds = text[0].galley.rect.translate(text[0].pos.to_vec2());
                assert!((bounds.center().y - top - image_height - 6.0).abs() < 0.01);
                assert!(edge(bounds).abs() < 0.01);
                assert!(painted.iter().any(|shape| matches!(&shape.shape, Shape::Text(text) if text.fallback_color == main_color)));
            }
        }
        let oversized = Watermark::default()
            .with_image(
                egui::Image::from_texture(SizedTexture::new(texture, Vec2::new(2000.0, 1000.0)))
                    .fit_to_exact_size(Vec2::new(2000.0, 1000.0)),
            )
            .with_line(WatermarkLine::new("WIDE ".repeat(200), 20.0, pane_color));
        assert!(chart.set_pane_watermark(pane, Some(oversized)));
        let painted = render(&mut chart);
        let panel = chart
            .pane_layout
            .panes
            .iter()
            .find(|item| item.id == pane)
            .unwrap()
            .rect;
        let plot =
            Rect::from_x_y_ranges(chart.get_rendered_price_rect().x_range(), panel.y_range());
        for shape in painted.iter().filter(|shape| {
            shape.shape.texture_id() == texture
                || matches!(&shape.shape, Shape::Text(text) if text.fallback_color == pane_color)
        }) {
            assert!(
                plot.contains_rect(shape.clip_rect),
                "watermark escaped its plot: {shape:?}"
            );
        }
        assert!(chart.set_pane_watermark(pane, Some(watermark.clone())));
        for include_watermark in [true, false] {
            let mut image = chart
                .export_image(
                    &ctx,
                    &ImageExportOptions {
                        size: [800, 600],
                        include_title: false,
                        include_timestamp: false,
                        include_watermark,
                        ..Default::default()
                    },
                )
                .unwrap();
            for color in [main_color, pane_color] {
                assert_eq!(image.primitives.iter().any(|primitive| matches!(&primitive.primitive,
                    egui::epaint::Primitive::Mesh(mesh) if mesh.vertices.iter().any(|vertex| vertex.color == color))), include_watermark);
            }
            assert_eq!(
                image
                    .primitives
                    .iter()
                    .any(|primitive| matches!(&primitive.primitive,
                egui::epaint::Primitive::Mesh(mesh) if mesh.texture_id == texture)),
                include_watermark
            );
            image.textures_delta.clear();
        }
        assert!(chart.set_pane_watermark(pane, None));
        let painted = render(&mut chart);
        assert!(
            !painted
                .iter()
                .any(|shape| shape.shape.texture_id() == texture)
        );
        assert!(painted.iter().any(
            |shape| matches!(&shape.shape, Shape::Text(text) if text.fallback_color == main_color)
        ));
        assert!(chart.set_pane_watermark(pane, Some(watermark.clone())));
        assert!(
            render(&mut chart)
                .iter()
                .any(|shape| shape.shape.texture_id() == texture)
        );
        assert!(chart.remove_pane(pane));
        assert!(!chart.set_pane_watermark(pane, None));
        assert!(chart.pane_layout.panes.iter().all(|item| item.id != pane));
        let replacement = chart.add_pane();
        assert_ne!(replacement, pane);
        assert!(
            chart
                .pane_layout
                .panes
                .iter()
                .find(|item| item.id == replacement)
                .unwrap()
                .watermark
                .is_none()
        );
        let painted = render(&mut chart);
        assert!(
            !painted
                .iter()
                .any(|shape| shape.shape.texture_id() == texture)
        );
        assert!(chart.set_pane_watermark(replacement, Some(watermark)));
        assert!(
            render(&mut chart)
                .iter()
                .any(|shape| shape.shape.texture_id() == texture)
        );
        chart
            .pane_layout
            .panes
            .iter_mut()
            .find(|item| item.id == replacement)
            .unwrap()
            .collapsed = Some(true);
        assert!(
            !render(&mut chart)
                .iter()
                .any(|shape| shape.shape.texture_id() == texture)
        );
        chart
            .pane_layout
            .panes
            .iter_mut()
            .find(|item| item.id == replacement)
            .unwrap()
            .collapsed = Some(false);
        assert!(chart.move_series_to_pane(SeriesId::MAIN, replacement));
        let painted = render(&mut chart);
        assert!(
            painted
                .iter()
                .any(|shape| shape.shape.texture_id() == texture)
        );
        assert!(painted.iter().any(
            |shape| matches!(&shape.shape, Shape::Text(text) if text.fallback_color == main_color)
        ));
        assert!(chart.remove_pane(PaneId::MAIN));
        assert!(!chart.set_pane_watermark(PaneId::MAIN, None));
        let painted = render(&mut chart);
        assert!(
            painted
                .iter()
                .any(|shape| shape.shape.texture_id() == texture)
        );
        assert!(!painted.iter().any(
            |shape| matches!(&shape.shape, Shape::Text(text) if text.fallback_color == main_color)
        ));
    }

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

    /// Removing the original pane keeps the remaining panes' identities and shared time
    /// positions, deletes only its content, and round-trips without recreating it.
    #[test]
    fn removing_original_pane_preserves_survivors_and_cleans_up_content() {
        use crate::model::{LogicalRange, Marker, PriceLine};
        use crate::widget::test_frames::{frame, shapes};
        use crate::widget::{PaneId, SeriesId, SeriesOptions};
        use egui::Color32;

        for removed_series_first in [false, true] {
            let ctx = Context::default();
            let mut chart = chart(&[100.0], &[110.0]);
            chart.config.show_volume = false;
            chart.chart_options.crosshair.mode = crate::config::CrosshairMode::Normal;
            let data = chart.bars(SeriesId::MAIN).to_vec();
            let marker = Marker::new(data[70].time);
            chart.set_markers(SeriesId::MAIN, vec![marker.clone()]);
            let line = chart
                .add_price_line(SeriesId::MAIN, PriceLine::new(105.0, Color32::RED))
                .unwrap();
            let overlay = chart.add_indicator(Box::new(crate::studies::SMA::new(3)));
            let removed = chart.add_series(SeriesOptions::default());
            chart.set_data(removed, data.clone());
            let detached = Arc::new(AtomicUsize::new(0));
            chart.attach_primitive(overlay, Attachment(detached.clone()));
            chart.attach_primitive(removed, Attachment(detached.clone()));

            let survivor_color = Color32::from_rgb(11, 123, 45);
            let survivors: Vec<_> = (0..2)
                .map(|_| {
                    let series = chart.add_series(SeriesOptions {
                        color: survivor_color,
                        ..Default::default()
                    });
                    chart.set_data(series, data.clone());
                    let pane = chart.add_pane();
                    assert!(chart.move_series_to_pane(series, pane));
                    (pane, series)
                })
                .collect();
            let (first, series) = survivors[0];
            let second = survivors[1].0;
            let study = chart.add_indicator(Box::new(crate::studies::RSI::new(3)));
            assert!(chart.move_series_to_pane(study, first));
            chart
                .series_price_scale_mut(series)
                .unwrap()
                .set_range(90.0, 130.0);
            assert!(chart.move_pane(second, 0));
            chart.set_markers(series, vec![marker]);
            let survivor_line = chart
                .add_price_line(series, PriceLine::new(105.0, Color32::BLUE))
                .unwrap();
            let survivor_detached = Arc::new(AtomicUsize::new(0));
            chart.attach_primitive(series, Attachment(survivor_detached.clone()));
            #[cfg(feature = "drawings")]
            for (id, pane) in [(1, None), (2, Some(overlay.0)), (3, Some(first.0))] {
                let mut drawing = crate::drawings::Drawing::new(
                    id,
                    crate::drawings::DrawingToolType::HorizontalLine,
                );
                drawing.chart_points = vec![crate::drawings::ChartPoint::new(70.0, 105.0)];
                drawing.pane = pane;
                drawing.locked = true;
                chart.drawings_mut().drawings.push(drawing);
            }
            frame(&ctx, &mut chart, 0.0, vec![]);
            assert!(chart.set_visible_logical_range(LogicalRange::new(60.0, 80.0)));
            frame(&ctx, &mut chart, 0.1, vec![]);
            let x = chart.time_to_x(data[70].time).unwrap();
            let view = chart.get_time_scale_state();
            #[cfg(feature = "ui")]
            let original = chart.snapshot().unwrap();
            if removed_series_first {
                assert!(chart.remove_series(SeriesId::MAIN));
            }
            assert!(chart.user_action(|chart| chart.remove_pane(PaneId::MAIN)));
            assert!(!chart.remove_pane(PaneId::MAIN));
            assert!(
                chart
                    .shared_price_scale(crate::model::SeriesScale::Right)
                    .is_none()
            );
            assert_eq!(chart.panes(), [second, first]);
            assert_eq!(chart.pane_series(first), [series, study]);
            assert!(chart.pane_series(PaneId::MAIN).is_empty());
            assert_eq!(chart.series_pane(SeriesId::MAIN), None);
            assert_eq!(chart.series_pane(overlay), None);
            assert!(chart.price_line(line).is_none());
            assert!(chart.price_line(survivor_line).is_some());
            assert_eq!(chart.markers(series).len(), 1);
            assert_eq!(chart.bars(series), data);
            assert!(chart.markers(SeriesId::MAIN).is_empty());
            frame(&ctx, &mut chart, 0.2, vec![]);
            assert_eq!(detached.load(Ordering::Relaxed), 2);
            assert_eq!(survivor_detached.load(Ordering::Relaxed), 0);
            assert_eq!(chart.get_time_scale_state(), view);
            let range = chart.series_price_scale(series).unwrap().range();
            assert_eq!((range.min, range.max), (90.0, 130.0));
            assert!((chart.time_to_x(data[70].time).unwrap() - x).abs() < 0.01);
            let heights: Vec<_> = chart
                .panes()
                .iter()
                .map(|pane| chart.pane_height(*pane).unwrap())
                .collect();
            assert!(
                (heights[0] - heights[1]).abs() < 0.01,
                "equal stretches share all remaining height"
            );
            let rect = chart
                .pane_layout
                .panes
                .iter()
                .find(|pane| pane.id == first)
                .unwrap()
                .rect;
            let pos = Pos2::new(x, rect.center().y);
            assert!(
                (rect.bottom() - chart.time_axis_rect.top()).abs() < 0.01,
                "surviving panes fill the plot height"
            );
            chart.drain_events().for_each(drop);
            let painted = shapes(&ctx, &mut chart, 0.3, vec![egui::Event::PointerMoved(pos)]);
            assert!(painted.iter().any(|shape| matches!(shape,
                egui::Shape::Path(path) if path.stroke.color == egui::epaint::ColorMode::Solid(survivor_color)
            )), "surviving series still paint");
            assert!(chart.drain_events().any(|event| matches!(event,
                crate::widget::ChartEvent::CrosshairMoved(Some(pointer))
                    if pointer.time == Some(data[70].time) && pointer.pane == 1
            )));
            for (step, (_, id)) in [survivors[1], survivors[0]].into_iter().enumerate() {
                let pos = Pos2::new(x, chart.series_price_map(id).unwrap().price_to_y(110.0));
                let events = [true, false].map(|pressed| egui::Event::PointerButton {
                    pos,
                    pressed,
                    button: egui::PointerButton::Primary,
                    modifiers: Default::default(),
                });
                frame(
                    &ctx,
                    &mut chart,
                    0.31 + step as f64 * 0.02,
                    vec![egui::Event::PointerMoved(pos)],
                );
                frame(&ctx, &mut chart, 0.32 + step as f64 * 0.02, events.to_vec());
                assert_eq!(
                    chart.selection.selected_id(),
                    Some(crate::chart::selection::ChartElementId::Series(id))
                );
            }
            #[cfg(feature = "drawings")]
            assert_eq!(
                chart
                    .drawings()
                    .drawings
                    .iter()
                    .map(|drawing| drawing.id)
                    .collect::<Vec<_>>(),
                [3]
            );

            #[cfg(feature = "ui")]
            {
                let snapshot = chart.snapshot().unwrap();
                let json = serde_json::to_string(&snapshot).unwrap();
                let snapshot = serde_json::from_str(&json).unwrap();
                let mut restored =
                    crate::widget::Chart::new(crate::model::BarData::from_bars(data.clone()));
                restored.restore(&snapshot).unwrap();
                let new_series = restored.restored_series_id(series.0).unwrap();
                restored.set_data(new_series, data.clone());
                frame(&Context::default(), &mut restored, 0.0, vec![]);
                assert!(!restored.panes().contains(&PaneId::MAIN));
                assert_eq!(restored.panes().len(), 2);
                assert_eq!(restored.markers(new_series).len(), 1);
                assert_eq!(restored.price_lines(new_series).count(), 1);
                let new_study = restored.restored_series_id(study.0).unwrap();
                assert_eq!(
                    restored.pane_series(restored.panes()[1]),
                    [new_series, new_study]
                );
                assert_eq!(restored.snapshot().unwrap().panes.len(), 2);
                #[cfg(feature = "drawings")]
                assert_eq!(restored.drawings().drawings.len(), 1);
                restored.restore(&original).unwrap();
                assert!(restored.panes().contains(&PaneId::MAIN));
                assert!(restored.is_main_series_visible());
            }
            assert!(chart.undo());
            assert!(chart.panes().contains(&PaneId::MAIN));
            #[cfg(feature = "drawings")]
            assert!(
                chart
                    .drawings()
                    .drawings
                    .iter()
                    .all(|drawing| drawing.locked)
            );
            assert!(chart.redo());
            assert_eq!(chart.panes(), [second, first]);
            assert!(chart.remove_pane(second));
            assert!(
                !chart.remove_pane(first),
                "the final pane and its members stay"
            );
            assert_eq!(chart.pane_series(first), [series, study]);
            assert!(chart.remove_series(series));
            assert!(chart.remove_series(study));
            assert_eq!(chart.panes(), [first]);
            assert!(chart.preserves_empty_pane(first));
            frame(&ctx, &mut chart, 0.4, vec![]);
            assert_eq!(
                chart.panes(),
                [first],
                "automatic cleanup also keeps a final pane"
            );
            let added = chart.add_series(SeriesOptions::default());
            assert_eq!(chart.series_pane(added), Some(first));
            chart
                .series_price_scale_mut(added)
                .unwrap()
                .set_range(80.0, 140.0);
            chart.set_data(added, data.clone());
            frame(&ctx, &mut chart, 0.5, vec![]);
            assert_eq!(chart.series_pane(added), Some(first));
            let range = chart.series_price_scale(added).unwrap().range();
            assert_eq!((range.min, range.max), (80.0, 140.0));
            let background = Color32::from_rgb(7, 13, 19);
            chart.config.background_color = background;
            chart.config.background_style = crate::config::BackgroundStyle::Solid;
            for (step, visible) in [false, true].into_iter().enumerate() {
                chart.set_series_visible(added, visible);
                chart.pane_layout.panes[0].collapsed = Some(visible);
                let painted = shapes(&ctx, &mut chart, 0.6 + step as f64 * 0.01, vec![]);
                assert!(
                    painted.iter().any(|shape| matches!(shape,
                        egui::Shape::Rect(rect) if rect.fill == background
                            && rect.rect.width() > 700.0 && rect.rect.height() > 500.0
                    )),
                    "a chart with only hidden or collapsed panes still paints its canvas"
                );
            }
            let added_study = chart.add_indicator(Box::new(crate::studies::SMA::new(3)));
            let pane = chart.series_pane(added_study).unwrap();
            assert_ne!(pane, first);
            assert_ne!(pane, PaneId::MAIN);
            let study = chart
                .indicators()
                .find(|(id, _)| *id == added_study)
                .unwrap()
                .1;
            assert!(!study.is_overlay());
            // The retained source still supplies closes: (110 + 110 + 110) / 3 = 110.
            assert_eq!(study.plot_value(0, 70), Some(110.0));
            #[cfg(feature = "ui")]
            assert_eq!(chart.snapshot().unwrap().studies[0].pane, 1);
            frame(&ctx, &mut chart, 0.7, vec![]);
            assert_eq!(chart.series_pane(added_study), Some(pane));
        }
    }

    /// The initial pane owns only the content still placed there, including after restoration.
    #[test]
    fn removing_original_pane_preserves_relocated_main_series() {
        use crate::model::{LogicalRange, Marker, PriceLine};
        use crate::widget::test_frames::frame;
        use crate::widget::{PaneId, SeriesId};

        let ctx = Context::default();
        let mut chart = chart(&[100.0], &[110.0]);
        chart.config.show_volume = false;
        let data = chart.bars(SeriesId::MAIN).to_vec();
        let overlay = chart.add_indicator(Box::new(crate::studies::SMA::new(3)));
        let empty = chart.add_pane();
        let survivor = chart.add_pane();
        assert!(chart.move_series_to_pane(SeriesId::MAIN, survivor));
        chart.set_markers(SeriesId::MAIN, vec![Marker::new(data[70].time)]);
        chart
            .add_price_line(SeriesId::MAIN, PriceLine::new(105.0, egui::Color32::RED))
            .unwrap();
        let detached = Arc::new(AtomicUsize::new(0));
        chart.attach_primitive(SeriesId::MAIN, Attachment(detached.clone()));
        #[cfg(feature = "drawings")]
        for (id, pane) in [(1, None), (2, Some(SeriesId::MAIN.0))] {
            let mut drawing =
                crate::drawings::Drawing::new(id, crate::drawings::DrawingToolType::HorizontalLine);
            drawing.chart_points = vec![crate::drawings::ChartPoint::new(70.0, 105.0)];
            drawing.pane = pane;
            chart.drawings_mut().drawings.push(drawing);
        }
        frame(&ctx, &mut chart, 0.0, vec![]);
        assert!(chart.set_visible_logical_range(LogicalRange::new(60.0, 80.0)));
        frame(&ctx, &mut chart, 0.1, vec![]);
        let x = chart.time_to_x(data[70].time).unwrap();
        assert!(chart.remove_pane(PaneId::MAIN));
        frame(&ctx, &mut chart, 0.2, vec![]);
        assert_eq!(chart.panes(), [empty, survivor]);
        assert_eq!(chart.pane_series(survivor), [SeriesId::MAIN]);
        assert_eq!(chart.series_pane(overlay), None);
        assert!(chart.is_main_series_visible());
        assert_eq!(chart.bars(SeriesId::MAIN), data);
        assert_eq!(chart.markers(SeriesId::MAIN).len(), 1);
        assert_eq!(chart.price_lines(SeriesId::MAIN).count(), 1);
        assert_eq!(detached.load(Ordering::Relaxed), 0);
        assert!((chart.time_to_x(data[70].time).unwrap() - x).abs() < 0.01);
        let added = chart.add_series(crate::widget::SeriesOptions::default());
        chart.set_data(added, data.clone());
        let study = chart.add_indicator(Box::new(crate::studies::SMA::new(3)));
        assert_eq!(chart.series_pane(added), Some(survivor));
        assert_eq!(chart.series_pane(study), Some(survivor));
        assert_eq!(chart.panes(), [empty, survivor]);
        assert!(chart.remove_pane(empty));
        frame(&ctx, &mut chart, 0.3, vec![]);
        assert!(chart.pane_height(survivor).unwrap() > 500.0);
        #[cfg(feature = "drawings")]
        assert_eq!(chart.drawings().drawings.len(), 2);
        #[cfg(feature = "ui")]
        {
            let snapshot = chart.snapshot().unwrap();
            let mut restored =
                crate::widget::Chart::new(crate::model::BarData::from_bars(data.clone()));
            for step in 0..2 {
                restored.restore(&snapshot).unwrap();
                restored.set_data(restored.restored_series_id(added.0).unwrap(), data.clone());
                frame(&Context::default(), &mut restored, step as f64, vec![]);
                assert_eq!(restored.panes().len(), 1);
                assert_ne!(restored.panes()[0], PaneId::MAIN);
                assert_eq!(
                    restored.series_pane(SeriesId::MAIN),
                    Some(restored.panes()[0])
                );
                assert!(restored.is_main_series_visible());
                assert_eq!(restored.markers(SeriesId::MAIN).len(), 1);
                assert_eq!(restored.price_lines(SeriesId::MAIN).count(), 1);
            }
        }
        assert!(!chart.remove_pane(survivor));
    }

    /// Undo of a user action that removed the original price pane with the main series
    /// in it brings back the main series' markers and change markers, as undo of the series'
    /// own removal does.
    #[test]
    fn undo_of_price_pane_removal_restores_main_markers() {
        use crate::model::{ChangeMarkers, Marker};
        use crate::widget::{PaneId, SeriesId};

        let removals: [fn(&mut crate::widget::Chart) -> bool; 2] = [
            |chart| chart.remove_series(SeriesId::MAIN),
            |chart| chart.remove_pane(PaneId::MAIN),
        ];
        let undone: Vec<_> = removals
            .into_iter()
            .map(|remove| {
                let mut chart = chart(&[100.0], &[110.0]);
                chart.add_indicator(Box::new(crate::studies::RSI::new(3)));
                let time = chart.bars(SeriesId::MAIN)[70].time;
                chart.set_markers(SeriesId::MAIN, vec![Marker::new(time)]);
                chart.set_change_markers(SeriesId::MAIN, Some(ChangeMarkers::default()));
                let state = |chart: &crate::widget::Chart| {
                    (
                        chart.markers(SeriesId::MAIN).len(),
                        chart.change_markers(SeriesId::MAIN).is_some(),
                    )
                };
                assert!(chart.user_action(remove));
                assert!(chart.undo());
                let undone = state(&chart);
                assert!(chart.redo());
                assert_eq!(state(&chart), (0, false), "redo removes them again");
                undone
            })
            .collect();
        assert_eq!(undone, [(1, true), (1, true)]);
    }
}
