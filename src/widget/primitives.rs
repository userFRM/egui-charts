//! Host primitives attached to series, and custom series drawn by them.

use super::{Chart, ChartElementId, PointerTarget, SeriesId};
use crate::chart::coords::ChartMapping;
use crate::chart::primitives::{
    Primitive, PrimitiveHit, PrimitiveId, PrimitivePass, PrimitiveView,
};
use crate::model::Bar;
use crate::studies::Indicator;
use egui::{Painter, Pos2};
use std::sync::{Arc, Mutex, MutexGuard};

/// A primitive and the series it is attached to.
#[derive(Clone)]
pub(super) struct Attached {
    pub(super) id: PrimitiveId,
    pub(super) series: SeriesId,
    /// Shared with image exports, which draw the same primitives.
    primitive: Arc<Mutex<dyn Primitive>>,
    /// Draws the study `series` in place of its plots.
    pub(super) replaces: bool,
    /// The series' bars last reported; `None` until `attached` ran.
    pub(super) seen: Option<Seen>,
}

/// What identifies the bars of a series: a revision that changes with any bar but the last, the
/// number of bars and the last bar, bitwise.
pub(super) type Seen = (
    u64,
    usize,
    Option<(chrono::DateTime<chrono::Utc>, [u64; 5])>,
);

fn seen(revision: u64, bars: &[Bar]) -> Seen {
    let last = bars.last().map(|bar| {
        (
            bar.time,
            [bar.open, bar.high, bar.low, bar.close, bar.volume].map(f64::to_bits),
        )
    });
    (revision, bars.len(), last)
}

impl Attached {
    pub(super) fn draw_background(
        &self,
        painter: &Painter,
        view: &PrimitiveView<'_>,
        pass: PrimitivePass,
        factor: usize,
    ) {
        self.get()
            .draw_background_merged(painter, view, pass, factor);
    }

    pub(super) fn draw(
        &self,
        painter: &Painter,
        view: &PrimitiveView<'_>,
        pass: PrimitivePass,
        factor: usize,
    ) {
        self.get().draw_merged(painter, view, pass, factor);
    }

    pub(super) fn get(&self) -> MutexGuard<'_, dyn Primitive + 'static> {
        // A primitive that panicked in a hook keeps the state it had.
        self.primitive
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

/// A primitive under the pointer and the cursor it asks for.
#[derive(Clone)]
pub(super) struct Hit {
    /// The custom series a renderer draws, or the primitive itself.
    pub(super) target: PointerTarget,
    pub(super) result: PrimitiveHit,
}

impl Chart {
    /// Attaches `primitive` to `series`, the main series, an added bar series or a study, to
    /// draw in the pane of the series on its price scale from the next [`show`](Self::show); see
    /// [`Primitive`]. Several primitives draw in the order they were attached.
    ///
    /// Returns `None`, dropping `primitive`, when `series` is not present.
    pub fn attach_primitive(
        &mut self,
        series: SeriesId,
        primitive: impl Primitive + 'static,
    ) -> Option<PrimitiveId> {
        self.attach(series, Arc::new(Mutex::new(primitive)), false)
    }

    /// Detaches a primitive, calling its [`detached`](Primitive::detached) hook if it was
    /// attached in a frame. Returns `false` for an unknown or already detached id.
    pub fn detach_primitive(&mut self, id: PrimitiveId) -> bool {
        let Some(at) = self.primitives.iter().position(|item| item.id == id) else {
            return false;
        };
        let item = self.primitives.remove(at);
        if item.seen.is_some() {
            item.get().detached();
        }
        true
    }

    /// Adds a series type the host defines, and returns its id. `study` supplies its values,
    /// which the legend, the crosshair, the last-value tag and the autoscale of its pane read as
    /// for any study, in its own pane unless it is an overlay. `renderer` draws its points in
    /// place of the study's plots and hit tests them: a hit selects the series and is reported
    /// as [`ChartElementId::Indicator`]. The series is shown, hidden and removed like a study.
    pub fn add_custom_series(
        &mut self,
        study: Box<dyn Indicator>,
        renderer: impl Primitive + 'static,
    ) -> SeriesId {
        let id = self.add_indicator(study);
        self.attach(id, Arc::new(Mutex::new(renderer)), true);
        id
    }

    fn attach(
        &mut self,
        series: SeriesId,
        primitive: Arc<Mutex<dyn Primitive>>,
        replaces: bool,
    ) -> Option<PrimitiveId> {
        self.primitive_source(series)?;
        let id = PrimitiveId(self.next_primitive);
        self.next_primitive += 1;
        self.primitives.push(Attached {
            id,
            series,
            primitive,
            replaces,
            seen: None,
        });
        Some(id)
    }

    /// The bars reported to the primitives of `series` and what identifies them, `None` when the
    /// series is not present. Studies follow the main bars they are computed from.
    fn primitive_source(&self, series: SeriesId) -> Option<(Seen, &[Bar])> {
        let bars = self.state.data().bars.as_slice();
        let main = (seen(self.state.revision(), bars), bars);
        if series == SeriesId::MAIN {
            self.main_series_present.then_some(main)
        } else if let Some(item) = self.price_series.iter().find(|item| item.id == series) {
            Some((seen(item.revision, &item.bars), &item.bars))
        } else {
            self.study_ids.contains(&series).then_some(main)
        }
    }

    /// Runs the lifecycle hooks before a frame is drawn: `detached` for the primitives of
    /// removed series, `attached` for new ones and `data_changed` when their bars changed.
    pub(super) fn sync_primitives(&mut self, ctx: &egui::Context) {
        let mut primitives = std::mem::take(&mut self.primitives);
        primitives.retain_mut(|item| {
            let Some((current, bars)) = self.primitive_source(item.series) else {
                if item.seen.is_some() {
                    item.get().detached();
                }
                return false;
            };
            if item.seen != Some(current) {
                let mut primitive = item.get();
                if item.seen.is_none() {
                    primitive.attached(ctx, item.series);
                }
                primitive.data_changed(bars);
                drop(primitive);
                item.seen = Some(current);
            }
            true
        });
        // The hooks have no access to the chart, so no primitive was attached meanwhile.
        self.primitives = primitives;
    }

    /// Marks every primitive as up to date with this chart's bars, so that an image export
    /// draws them without running their hooks again.
    pub(super) fn settle_primitives(&mut self) {
        for at in 0..self.primitives.len() {
            let current = self
                .primitive_source(self.primitives[at].series)
                .map(|(current, _)| current);
            self.primitives[at].seen = current;
        }
    }

    pub(super) fn primitive_view(
        &self,
        series: SeriesId,
        mapping: ChartMapping,
    ) -> PrimitiveView<'_> {
        let (start, end) = self.state.visible_range();
        PrimitiveView {
            series,
            study: self
                .study_ids
                .iter()
                .position(|id| *id == series)
                .and_then(|idx| self.rendered_studies().indicators().get(idx))
                .map(|study| study.as_ref()),
            mapping,
            bars: self.state.shown(),
            visible: start..end,
        }
    }

    /// The view of a primitive drawn in the main pane, from the main pane's mapping. `None`
    /// when its series is hidden or a study in a pane of its own.
    fn main_view(&self, series: SeriesId, coords: &ChartMapping) -> Option<PrimitiveView<'_>> {
        if self.series_pane(series) != Some(self.main_series_pane()) {
            return None;
        }
        let mapping = if series == SeriesId::MAIN {
            self.main_series_visible.then_some(coords.clone())?
        } else if let Some(item) = self.price_series.iter().find(|item| item.id == series) {
            item.visible.then(|| ChartMapping {
                price: self.series_map(item, coords.rect),
                ..coords.clone()
            })?
        } else {
            let idx = self.study_ids.iter().position(|id| *id == series)?;
            let study = self.rendered_studies().indicators().get(idx)?;
            (study.is_visible()
                && (!study.is_volume()
                    || matches!(study.scale(), crate::studies::StudyScale::Shared(_))))
            .then(|| crate::chart::rendering::study_mapping(study.as_ref(), coords))?
        };
        Some(self.primitive_view(series, mapping))
    }

    /// The primitives drawn in the main pane, with their views.
    pub(super) fn main_primitives<'a>(
        &'a self,
        coords: &'a ChartMapping,
    ) -> impl DoubleEndedIterator<Item = (&'a Attached, PrimitiveView<'a>)> + Clone {
        self.primitives
            .iter()
            .filter_map(move |item| Some((item, self.main_view(item.series, coords)?)))
    }

    /// The primitives of every visible series, for the time axis the panes share. Those in a
    /// study pane get the main pane's mapping.
    pub(super) fn time_axis_primitives<'a>(
        &'a self,
        coords: &'a ChartMapping,
    ) -> impl Iterator<Item = (&'a Attached, PrimitiveView<'a>)> {
        self.primitives.iter().filter_map(move |item| {
            let view = self.main_view(item.series, coords).or_else(|| {
                self.is_series_visible(item.series)
                    .then(|| self.primitive_view(item.series, coords.clone()))
            })?;
            Some((item, view))
        })
    }

    /// The primitives attached to `series`.
    pub(super) fn primitives_of(
        &self,
        series: SeriesId,
    ) -> impl DoubleEndedIterator<Item = &Attached> {
        self.primitives
            .iter()
            .filter(move |item| item.series == series)
    }

    /// The prices the primitives of `series` add to the fit of its scale.
    pub(super) fn primitive_prices(&self, series: SeriesId) -> impl Iterator<Item = f64> + '_ {
        let (start, end) = self.state.visible_range();
        self.primitives_of(series)
            .filter_map(move |item| item.get().autoscale(self.state.shown(), start..end))
            .flat_map(|(min, max)| [min, max])
    }

    /// The prices the primitives of the main series and of the overlay studies on the main
    /// scale add to its fit.
    pub(super) fn main_primitive_prices(&self) -> Vec<f64> {
        let studies = self.rendered_studies().indicators();
        let mut prices = Vec::new();
        if self.main_series_visible {
            prices.extend(self.primitive_prices(SeriesId::MAIN));
        }
        for (idx, id) in self.study_ids.iter().enumerate() {
            if self.series_pane(*id) == Some(self.main_series_pane())
                && studies.get(idx).is_some_and(|study| {
                    study.is_visible() && study.scale() == crate::studies::StudyScale::Main
                })
                && self.study_autoscale[idx]
                && !self.config.scale_price_chart_only
            {
                prices.extend(self.primitive_prices(*id));
            }
        }
        prices
    }

    /// Whether a renderer draws the study `series` in place of its plots.
    pub(super) fn has_renderer(&self, series: SeriesId) -> bool {
        self.primitives_of(series).any(|item| item.replaces)
    }

    /// Draws the main pane's primitives in `pass`, clipped to the plot.
    pub(super) fn paint_primitives(
        &self,
        painter: &Painter,
        coords: &ChartMapping,
        pass: PrimitivePass,
    ) {
        let painter = painter.with_clip_rect(painter.clip_rect().intersect(coords.rect));
        let shapes = backgrounds(
            &painter,
            self.main_pass_primitives(coords),
            pass,
            self.merge_groups.factor(),
        );
        let grouped = !shapes.is_empty();
        paint_backgrounds(&painter, shapes);
        if grouped {
            for (item, view) in self.main_pass_primitives(coords) {
                item.draw(&painter, &view, pass, self.merge_groups.factor());
            }
        } else {
            for (item, view) in self.main_primitives(coords) {
                item.draw(&painter, &view, pass, self.merge_groups.factor());
            }
        }
    }

    /// Includes renderers of volume overlays, whose foregrounds normally draw with the series.
    pub(super) fn main_pass_primitives<'a>(
        &'a self,
        coords: &'a ChartMapping,
    ) -> impl DoubleEndedIterator<Item = (&'a Attached, PrimitiveView<'a>)> + Clone {
        self.primitives.iter().filter_map(move |item| {
            let view = self.main_view(item.series, coords).or_else(|| {
                if !item.replaces || self.series_pane(item.series) != Some(self.main_series_pane())
                {
                    return None;
                }
                let mut view = self.primitive_view(item.series, coords.clone());
                let study = view.study?;
                if !study.is_visible() {
                    return None;
                }
                view.mapping = crate::chart::rendering::study_mapping(study, coords);
                Some(view)
            })?;
            Some((item, view))
        })
    }

    /// The visible primitives of a secondary pane, in attachment order, on their own scales.
    pub(super) fn pane_primitives<'a>(
        &'a self,
        members: &'a [SeriesId],
        coords: &'a ChartMapping,
    ) -> impl DoubleEndedIterator<Item = (&'a Attached, PrimitiveView<'a>)> + Clone {
        self.primitives.iter().filter_map(move |item| {
            if !members.contains(&item.series) || !self.is_series_visible(item.series) {
                return None;
            }
            let mut view = self.primitive_view(item.series, coords.clone());
            let pane = self.series_pane(item.series)?;
            if let Some(study) = view.study {
                view.mapping = self.pane_study_mapping(pane, study, coords);
            } else if let Some(series) = self
                .price_series
                .iter()
                .find(|series| series.id == item.series)
            {
                view.mapping.price = self.series_map(series, coords.rect);
            }
            Some((item, view))
        })
    }
}

/// Defers background shapes until the series have drawn, retaining their clip rectangles.
/// An empty result keeps the existing series foreground order.
pub(super) fn backgrounds<'a>(
    painter: &Painter,
    primitives: impl Iterator<Item = (&'a Attached, PrimitiveView<'a>)>,
    pass: PrimitivePass,
    factor: usize,
) -> Vec<egui::epaint::ClippedShape> {
    let start = painter
        .ctx()
        .graphics_mut(|graphics| graphics.entry(painter.layer_id()).next_idx());
    for (item, view) in primitives {
        item.draw_background(painter, &view, pass, factor);
    }
    painter.ctx().graphics_mut(|graphics| {
        let list = graphics.entry(painter.layer_id());
        let end = list.next_idx();
        let mut shapes = Vec::new();
        for index in start.0..end.0 {
            list.mutate_shape(egui::layers::ShapeIdx(index), |shape| {
                let empty = egui::epaint::ClippedShape::new(shape.clip_rect, egui::Shape::Noop);
                shapes.push(std::mem::replace(shape, empty));
            });
        }
        shapes
    })
}

/// Inserts deferred backgrounds without applying the painter's opacity a second time.
pub(super) fn paint_backgrounds(painter: &Painter, shapes: Vec<egui::epaint::ClippedShape>) {
    painter.ctx().graphics_mut(|graphics| {
        let list = graphics.entry(painter.layer_id());
        for shape in shapes {
            let index = list.add(shape.clip_rect, egui::Shape::Noop);
            list.mutate_shape(index, |entry| *entry = shape);
        }
    });
}

/// Draws one complete pane pass, with every background before any foreground.
pub(super) fn paint_pass<'a>(
    painter: &Painter,
    primitives: impl Iterator<Item = (&'a Attached, PrimitiveView<'a>)> + Clone,
    pass: PrimitivePass,
    factor: usize,
) {
    for (item, view) in primitives.clone() {
        item.draw_background(painter, &view, pass, factor);
    }
    for (item, view) in primitives {
        item.draw(painter, &view, pass, factor);
    }
}

/// Chooses by pass, then distance, keeping `first` on a tie.
pub(super) fn best_hit(first: Option<Hit>, second: Option<Hit>) -> Option<Hit> {
    match (first, second) {
        (Some(first), Some(second)) => {
            if second.result.pass > first.result.pass
                || (second.result.pass == first.result.pass
                    && second.result.distance < first.result.distance)
            {
                Some(second)
            } else {
                Some(first)
            }
        }
        (first, second) => first.or(second),
    }
}

/// The best hit under `pos`, with the latest attached primitive winning ties.
pub(super) fn primitive_at<'a>(
    primitives: impl DoubleEndedIterator<Item = (&'a Attached, PrimitiveView<'a>)>,
    pos: Pos2,
) -> Option<Hit> {
    primitives.rev().fold(None, |best, (item, view)| {
        let Some(result) = item.get().hit_test_result(pos, &view) else {
            return best;
        };
        if !result.distance.is_finite() || result.distance < 0.0 {
            return best;
        }
        best_hit(
            best,
            Some(Hit {
                target: if item.replaces {
                    PointerTarget::Element(ChartElementId::Indicator(item.series))
                } else {
                    PointerTarget::Primitive(item.id)
                },
                result,
            }),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::chart::primitives::{PriceAxisLabel, TimeAxisLabel};
    use crate::model::Bar;
    use crate::studies::{CustomIndicator, IndicatorValue};
    use crate::widget::test_frames::{chart, frame, shapes};
    use crate::widget::{ChartEvent, SeriesOptions};
    use egui::{Color32, CursorIcon, Event, Shape};

    type Log = Arc<Mutex<Vec<String>>>;

    struct Probe(&'static str, Log);

    impl Primitive for Probe {
        fn attached(&mut self, _: &egui::Context, _: SeriesId) {
            self.1.lock().unwrap().push(format!("{} attached", self.0));
        }
        fn data_changed(&mut self, bars: &[Bar]) {
            let line = format!("{} data {}", self.0, bars.len());
            self.1.lock().unwrap().push(line);
        }
        fn detached(&mut self) {
            self.1.lock().unwrap().push(format!("{} detached", self.0));
        }
    }

    /// `axis` is the main price axis of the pane: its column beside the plot and its plot's
    /// rows, without the corner beside the time axis.
    #[test]
    fn main_axis_primitives_draw_in_their_column_and_rows() {
        struct AxisRect(Arc<Mutex<Option<egui::Rect>>>);
        impl Primitive for AxisRect {
            fn draw_price_axis(&self, _: &egui::Painter, axis: egui::Rect, _: &PrimitiveView<'_>) {
                *self.0.lock().unwrap() = Some(axis);
            }
        }
        for side in [crate::model::AxisSide::Right, crate::model::AxisSide::Left] {
            let ctx = egui::Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            chart.place_pane_scales(crate::widget::PaneId::MAIN, side);
            let seen = Arc::default();
            chart.attach_primitive(SeriesId::MAIN, AxisRect(Arc::clone(&seen)));
            frame(&ctx, &mut chart, 0.0, vec![]);
            let axis = seen.lock().unwrap().unwrap();
            let plot = chart.get_rendered_price_rect();
            assert_eq!(axis.y_range(), plot.y_range(), "{side:?}");
            let touching = match side {
                crate::model::AxisSide::Right => axis.left() == plot.right(),
                crate::model::AxisSide::Left => axis.right() == plot.left(),
            };
            assert!(touching, "{side:?}: {axis:?} beside {plot:?}");
        }
    }

    #[test]
    fn hooks_follow_attachment_bar_changes_and_removal() {
        let ctx = egui::Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        let bars = chart.bars(SeriesId::MAIN).to_vec();
        let line = chart.add_series(SeriesOptions::default());
        chart.set_data(line, bars[..50].to_vec());
        let log = Log::default();
        let main = chart.attach_primitive(SeriesId::MAIN, Probe("main", log.clone()));
        chart.attach_primitive(line, Probe("line", log.clone()));
        assert!(
            chart
                .attach_primitive(SeriesId(99), Probe("absent", log.clone()))
                .is_none()
        );
        let take = || std::mem::take(&mut *log.lock().unwrap());
        frame(&ctx, &mut chart, 0.0, vec![]);
        frame(&ctx, &mut chart, 0.1, vec![]);
        assert_eq!(
            take(),
            [
                "main attached",
                "main data 100",
                "line attached",
                "line data 50"
            ]
        );
        chart.update(line, bars[50].clone());
        frame(&ctx, &mut chart, 0.2, vec![]);
        assert_eq!(take(), ["line data 51"]);
        let mut corrected = bars[10].clone();
        corrected.close = 15.0;
        chart.update(line, corrected);
        frame(&ctx, &mut chart, 0.25, vec![]);
        assert_eq!(take(), ["line data 51"], "a correction before the last bar");
        let mut last = bars[99].clone();
        last.close = 15.0;
        chart.update(SeriesId::MAIN, last);
        frame(&ctx, &mut chart, 0.3, vec![]);
        assert_eq!(take(), ["main data 100"]);
        chart.remove_series(line);
        frame(&ctx, &mut chart, 0.4, vec![]);
        assert_eq!(take(), ["line detached"]);
        assert!(chart.detach_primitive(main.unwrap()));
        assert!(!chart.detach_primitive(main.unwrap()));
        assert_eq!(take(), ["main detached"]);
    }

    const BELOW: Color32 = Color32::from_rgb(1, 2, 3);
    const SERIES: Color32 = Color32::from_rgb(4, 5, 6);
    const ABOVE: Color32 = Color32::from_rgb(7, 8, 9);

    #[test]
    fn backgrounds_precede_foregrounds_in_every_pass_and_pane() {
        struct Ordered {
            id: u8,
            merged: bool,
            log: Log,
        }
        impl Ordered {
            fn record(
                &self,
                painter: &Painter,
                view: &PrimitiveView<'_>,
                pass: PrimitivePass,
                background: bool,
            ) {
                self.log
                    .lock()
                    .unwrap()
                    .push(format!("{pass:?} {background} {}", self.id));
                painter.rect_filled(
                    egui::Rect::from_min_size(view.mapping.rect.min, egui::vec2(8.0, 8.0)),
                    0.0,
                    Color32::from_rgb(self.id, background as u8, pass as u8),
                );
            }
        }
        impl Primitive for Ordered {
            fn draw_background(
                &self,
                painter: &Painter,
                view: &PrimitiveView<'_>,
                pass: PrimitivePass,
            ) {
                self.record(painter, view, pass, true);
            }
            fn draw(&self, painter: &Painter, view: &PrimitiveView<'_>, pass: PrimitivePass) {
                self.record(painter, view, pass, false);
            }
            fn draw_background_merged(
                &self,
                painter: &Painter,
                view: &PrimitiveView<'_>,
                pass: PrimitivePass,
                factor: usize,
            ) {
                if self.merged {
                    assert_eq!(factor, 4);
                    self.record(painter, view, pass, true);
                } else {
                    self.draw_background(painter, view, pass);
                }
            }
            fn draw_merged(
                &self,
                painter: &Painter,
                view: &PrimitiveView<'_>,
                pass: PrimitivePass,
                factor: usize,
            ) {
                if self.merged {
                    assert_eq!(factor, 4);
                    self.record(painter, view, pass, false);
                } else {
                    self.draw(painter, view, pass);
                }
            }
        }
        for (pane, move_main) in [(false, false), (true, false), (false, true), (true, true)] {
            for kind in ["main", "bars", "study", "custom", "custom_volume"] {
                for merged in [false, true] {
                    if (kind == "custom_volume" && !cfg!(feature = "studies"))
                        || (move_main && kind == "main")
                    {
                        continue;
                    }
                    let ctx = egui::Context::default();
                    let mut chart = chart(&[10.0], &[20.0]);
                    let other = chart.add_indicator(Box::new(
                        CustomIndicator::new("Other", |bars| {
                            vec![IndicatorValue::Single(15.0); bars.len()]
                        })
                        .with_overlay(!pane),
                    ));
                    let log = Log::default();
                    let probe = |id| Ordered {
                        id,
                        merged,
                        log: log.clone(),
                    };
                    let study = || -> Box<dyn Indicator> {
                        #[cfg(feature = "studies")]
                        if kind == "custom_volume" {
                            let mut volume = crate::studies::Volume::default();
                            volume.set_overlay(!pane);
                            return Box::new(volume);
                        }
                        Box::new(
                            CustomIndicator::new("Ordered", |bars| {
                                bars.iter()
                                    .map(|bar| IndicatorValue::Single(bar.close))
                                    .collect()
                            })
                            .with_overlay(!pane),
                        )
                    };
                    let id = match kind {
                        "main" => {
                            if pane {
                                let destination = chart.add_pane();
                                assert!(chart.move_series_to_pane(SeriesId::MAIN, destination));
                            }
                            SeriesId::MAIN
                        }
                        "bars" => {
                            let id = chart.add_series(SeriesOptions::default());
                            chart.set_data(id, chart.bars(SeriesId::MAIN).to_vec());
                            if pane {
                                let destination = chart.add_pane();
                                chart.move_series_to_pane(id, destination);
                            }
                            id
                        }
                        "study" => chart.add_indicator(study()),
                        "custom" | "custom_volume" => chart.add_custom_series(study(), probe(1)),
                        _ => unreachable!(),
                    };
                    if !kind.starts_with("custom") {
                        chart.attach_primitive(id, probe(1));
                    }
                    if pane {
                        let destination = chart.series_pane(id).unwrap();
                        assert!(chart.move_series_to_pane(other, destination));
                    }
                    chart.attach_primitive(other, probe(2));
                    if move_main {
                        let destination = if pane {
                            chart.series_pane(id).unwrap()
                        } else {
                            chart.add_pane()
                        };
                        assert!(chart.move_series_to_pane(SeriesId::MAIN, destination));
                    }
                    if merged {
                        let mut options = chart.chart_options().clone();
                        options.time_scale.min_bar_spacing = 0.0;
                        chart.set_chart_options(options);
                        chart.apply_synced_time_scale(0.25, 0.0);
                    }
                    frame(&ctx, &mut chart, 0.0, vec![]);
                    log.lock().unwrap().clear();
                    let painted = shapes(&ctx, &mut chart, 0.1, vec![]);
                    let mut expected = Vec::new();
                    let mut colors = Vec::new();
                    for pass in [
                        PrimitivePass::Below,
                        PrimitivePass::Series,
                        PrimitivePass::Above,
                    ] {
                        for background in [true, false] {
                            for id in [1, 2] {
                                expected.push(format!("{pass:?} {background} {id}"));
                                colors.push(Color32::from_rgb(id, background as u8, pass as u8));
                            }
                        }
                    }
                    assert_eq!(
                        *log.lock().unwrap(),
                        expected,
                        "{kind}, pane={pane}, move_main={move_main}, merged={merged}"
                    );
                    let actual: Vec<_> = painted
                        .iter()
                        .filter_map(|shape| match shape {
                            Shape::Rect(rect) if colors.contains(&rect.fill) => Some(rect.fill),
                            _ => None,
                        })
                        .collect();
                    assert_eq!(
                        actual, colors,
                        "paint order: {kind}, pane={pane}, move_main={move_main}"
                    );
                    chart.set_series_visible(id, false);
                    chart.set_series_visible(other, false);
                    log.lock().unwrap().clear();
                    frame(&ctx, &mut chart, 0.2, vec![]);
                    assert!(log.lock().unwrap().is_empty(), "hidden {kind}");
                }
            }
        }
    }

    /// Fills a corner of the plot in each pass and asks for prices 0 to 500.
    struct Layers;

    impl Primitive for Layers {
        fn draw(&self, painter: &Painter, view: &PrimitiveView<'_>, pass: PrimitivePass) {
            let fill = match pass {
                PrimitivePass::Below => BELOW,
                PrimitivePass::Series => SERIES,
                PrimitivePass::Above => ABOVE,
            };
            let corner = egui::Rect::from_min_size(view.mapping.rect.min, egui::vec2(9.0, 9.0));
            painter.rect_filled(corner, 0.0, fill);
        }
        fn autoscale(&self, _: &[Bar], _: std::ops::Range<usize>) -> Option<(f64, f64)> {
            Some((0.0, 500.0))
        }
        fn price_axis_labels(&self, _: &PrimitiveView<'_>) -> Vec<PriceAxisLabel> {
            vec![PriceAxisLabel {
                price: 250.0,
                text: "Level".into(),
                color: SERIES,
            }]
        }
        fn time_axis_labels(&self, view: &PrimitiveView<'_>) -> Vec<TimeAxisLabel> {
            vec![TimeAxisLabel {
                time: view.bars[90].time,
                text: "A session label much wider than any price on the axis".into(),
                color: ABOVE,
            }]
        }
    }

    #[test]
    fn passes_layer_around_series_and_studies_and_widen_the_scale() {
        let ctx = egui::Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        let study = Color32::from_rgb(10, 11, 12);
        let study_id = chart.add_indicator(Box::new(
            CustomIndicator::new("Mid", |bars| {
                bars.iter()
                    .map(|bar| IndicatorValue::Single((bar.high + bar.low) / 2.0))
                    .collect()
            })
            .with_color(study),
        ));
        let primitive = chart.attach_primitive(SeriesId::MAIN, Layers).unwrap();
        frame(&ctx, &mut chart, 0.0, vec![]);
        let shapes = shapes(&ctx, &mut chart, 0.1, vec![]);
        let first = |found: &dyn Fn(&Shape) -> bool| shapes.iter().position(found).unwrap();
        let fill =
            |color| move |shape: &Shape| matches!(shape, Shape::Rect(rect) if rect.fill == color);
        let candle = first(&fill(chart.config.bullish_color));
        let line = first(&|shape| match shape {
            Shape::Path(path) => path.stroke.color == egui::epaint::ColorMode::Solid(study),
            Shape::LineSegment { stroke, .. } => stroke.color == study,
            _ => false,
        });
        let (below, series, above) = (
            first(&fill(BELOW)),
            first(&fill(SERIES)),
            first(&fill(ABOVE)),
        );
        assert!(below < candle && candle < line && line < series && series < above);
        let range = chart.state().price_scale().range();
        assert!(range.min <= 0.0 && range.max >= 500.0, "{range:?}");
        let text = |label: &str| {
            shapes
                .iter()
                .any(|shape| matches!(shape, Shape::Text(text) if text.galley.text() == label))
        };
        assert!(text("Level") && text("A session label much wider than any price on the axis"));
        // A time label is on the time axis and does not size the price axis.
        assert!(
            chart.measured_axis_width < 120.0,
            "{}",
            chart.measured_axis_width
        );
        chart.detach_primitive(primitive);
        for (owner, price_only, expected) in [
            (SeriesId::MAIN, true, (-62.5, 562.5)),
            (study_id, true, (8.75, 21.25)),
            (study_id, false, (-62.5, 562.5)),
        ] {
            let primitive = chart.attach_primitive(owner, Layers).unwrap();
            chart.config_mut().scale_price_chart_only = price_only;
            let painted = super::super::test_frames::shapes(&ctx, &mut chart, 0.2, vec![]);
            let range = chart.state().price_scale().range();
            assert_eq!((range.min, range.max), expected);
            assert!(
                painted.iter().any(fill(ABOVE)),
                "the primitive still paints"
            );
            chart.detach_primitive(primitive);
        }
    }

    #[test]
    fn bar_series_primitives_follow_pane_moves_and_layer_around_the_bars() {
        use crate::model::ChartType;
        use crate::widget::PaneId;

        let ctx = egui::Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        let color = Color32::from_rgb(10, 11, 12);
        let id = chart.add_series(SeriesOptions {
            chart_type: ChartType::Candles,
            up_color: color,
            ..Default::default()
        });
        chart.set_data(id, chart.bars(SeriesId::MAIN).to_vec());
        chart.attach_primitive(id, Layers);
        let pane = chart.add_pane();
        assert!(chart.set_preserve_empty_pane(pane, true));
        for destination in [pane, PaneId::MAIN, pane] {
            assert!(chart.move_series_to_pane(id, destination));
            let painted = shapes(&ctx, &mut chart, 0.0, vec![]);
            let map = chart.series_price_map(id).unwrap();
            assert!(map.range().contains(map.transform().to_scale(500.0)));
            if destination != PaneId::MAIN {
                assert!(chart.last_price_map.range().max < 100.0);
            }
            let rects = |fill| {
                painted
                    .iter()
                    .enumerate()
                    .filter_map(move |(at, shape)| match shape {
                        Shape::Rect(rect)
                            if rect.fill == fill
                                && (fill == color || rect.rect.size() == egui::vec2(9.0, 9.0)) =>
                        {
                            Some((at, rect.rect))
                        }
                        _ => None,
                    })
            };
            let (below, rect) = rects(BELOW).next().unwrap();
            assert_eq!(rects(BELOW).count(), 1, "drawn only in its own pane");
            let candle = rects(color).next().unwrap().0;
            let series = rects(SERIES).next().unwrap().0;
            let above = rects(ABOVE).next().unwrap().0;
            assert!(
                below < candle && candle < series && series < above,
                "{destination:?}: {below} < {candle} < {series} < {above}"
            );
            let pane_rect = chart
                .pane_layout
                .panes
                .iter()
                .find(|item| item.id == destination)
                .unwrap()
                .rect;
            assert!(pane_rect.contains_rect(rect));
        }
        chart.set_series_visible(id, false);
        assert!(
            !shapes(&ctx, &mut chart, 0.2, vec![])
                .iter()
                .any(|shape| matches!(shape, Shape::Rect(rect) if rect.fill == BELOW))
        );
    }

    /// Draws a dot at each value of its study, fills a corner of the plot above everything, and
    /// claims the whole plot for the pointer.
    struct Dots;

    impl Primitive for Dots {
        fn draw(&self, painter: &Painter, view: &PrimitiveView<'_>, pass: PrimitivePass) {
            if pass == PrimitivePass::Above {
                let corner = egui::Rect::from_min_size(view.mapping.rect.min, egui::vec2(9.0, 9.0));
                painter.rect_filled(corner, 0.0, ABOVE);
            }
            for index in view
                .visible
                .clone()
                .filter(|_| pass == PrimitivePass::Series)
            {
                if let Some(value) = view.study.and_then(|study| study.plot_value(0, index)) {
                    let centre = Pos2::new(
                        view.index_to_x(index as f64),
                        view.mapping.price_to_y(value),
                    );
                    painter.circle_filled(centre, 2.0, SERIES);
                }
            }
        }
        fn hit_test(&self, pos: Pos2, view: &PrimitiveView<'_>) -> Option<CursorIcon> {
            view.mapping
                .rect
                .contains(pos)
                .then_some(CursorIcon::PointingHand)
        }
    }

    #[test]
    fn object_hits_rank_by_pass_distance_and_paint_order_in_every_pane() {
        struct Objects(Arc<Mutex<Vec<(egui::Vec2, PrimitiveHit)>>>, bool);
        impl Primitive for Objects {
            fn draw_background(
                &self,
                painter: &Painter,
                view: &PrimitiveView<'_>,
                pass: PrimitivePass,
            ) {
                if self.1 && pass == PrimitivePass::Series {
                    painter.rect_filled(view.mapping.rect, 0.0, BELOW);
                }
            }
            fn hit_test_result(&self, pos: Pos2, view: &PrimitiveView<'_>) -> Option<PrimitiveHit> {
                self.0
                    .lock()
                    .unwrap()
                    .iter()
                    .map(|(offset, hit)| {
                        let mut hit = hit.clone();
                        hit.distance = pos.distance(view.mapping.rect.center() + *offset);
                        hit
                    })
                    .filter(|hit| hit.distance <= 20.0)
                    .max_by(|left, right| {
                        left.pass
                            .cmp(&right.pass)
                            .then_with(|| right.distance.total_cmp(&left.distance))
                    })
            }
        }
        struct Invalid(f32);
        impl Primitive for Invalid {
            fn hit_test_result(&self, _: Pos2, _: &PrimitiveView<'_>) -> Option<PrimitiveHit> {
                Some(PrimitiveHit {
                    distance: self.0,
                    pass: PrimitivePass::Above,
                    ..PrimitiveHit::new(CursorIcon::Wait)
                })
            }
        }
        let object = |id: &str, cursor, offset| {
            (
                offset,
                PrimitiveHit {
                    object_id: Some(id.into()),
                    data: Some(Arc::new(serde_json::json!({ "object": id }))),
                    ..PrimitiveHit::new(cursor)
                },
            )
        };
        for (secondary, move_main, backgrounds) in
            [(false, false), (true, false), (false, true), (true, true)]
                .into_iter()
                .flat_map(|(secondary, move_main)| {
                    [false, true].map(|backgrounds| (secondary, move_main, backgrounds))
                })
        {
            let ctx = egui::Context::default();
            let mut chart = chart(&[10.0], &[20.0]);
            chart.config.show_volume = false;
            let objects = Arc::new(Mutex::new(vec![
                object("upper", CursorIcon::ResizeVertical, egui::vec2(0.0, -8.0)),
                object("lower", CursorIcon::Grab, egui::vec2(0.0, 8.0)),
            ]));
            let series = chart.add_custom_series(
                Box::new(
                    CustomIndicator::new("Objects", |bars| {
                        vec![IndicatorValue::Single(15.0); bars.len()]
                    })
                    .with_overlay(!secondary),
                ),
                Objects(objects.clone(), backgrounds),
            );
            let rival = chart.add_custom_series(
                Box::new(CustomIndicator::new("Rival", |bars| {
                    vec![IndicatorValue::Single(15.0); bars.len()]
                })),
                Objects(
                    Arc::new(Mutex::new(vec![object(
                        "rival",
                        CursorIcon::Wait,
                        egui::vec2(0.0, -18.0),
                    )])),
                    backgrounds,
                ),
            );
            assert!(chart.move_series_to_pane(rival, chart.series_pane(series).unwrap()));
            if move_main {
                let destination = if secondary {
                    chart.series_pane(series).unwrap()
                } else {
                    chart.add_pane()
                };
                assert!(chart.move_series_to_pane(SeriesId::MAIN, destination));
                // Keep the main series behind the renderers, as in the original layout.
                for id in [series, rival] {
                    assert!(chart.reorder_indicator(id, super::super::VisualOrder::BringToFront));
                }
            }
            let first = Arc::new(Mutex::new(vec![object(
                "first",
                CursorIcon::Crosshair,
                egui::vec2(3.0, 4.0),
            )]));
            let second = Arc::new(Mutex::new(vec![object(
                "second",
                CursorIcon::PointingHand,
                egui::vec2(0.0, 6.0),
            )]));
            let first_id = chart
                .attach_primitive(series, Objects(first.clone(), backgrounds))
                .unwrap();
            let second_id = chart
                .attach_primitive(series, Objects(second.clone(), backgrounds))
                .unwrap();
            for time in [0.0, 0.1, 0.2] {
                frame(&ctx, &mut chart, time, vec![]);
            }
            let plot = if chart.series_pane(series) == Some(chart.main_series_pane()) {
                chart.last_rendered_price_rect
            } else {
                chart.last_rendered_indicator_panes[0].chart_rect
            };
            let renderer = PointerTarget::Element(ChartElementId::Indicator(series));
            let mut pos = plot.center();
            // At the centre, sqrt(3^2 + 4^2) = 5 beats 6; both renderer objects are 8 away.
            for (step, (target, id, distance, cursor)) in [
                (
                    PointerTarget::Primitive(first_id),
                    "first",
                    5.0,
                    CursorIcon::Crosshair,
                ),
                (
                    PointerTarget::Primitive(second_id),
                    "second",
                    5.0,
                    CursorIcon::PointingHand,
                ),
                (
                    PointerTarget::Primitive(first_id),
                    "first",
                    5.0,
                    CursorIcon::Crosshair,
                ),
                (renderer, "lower", 8.0, CursorIcon::Grab),
                (
                    PointerTarget::Primitive(second_id),
                    "second",
                    5.0,
                    CursorIcon::PointingHand,
                ),
                (renderer, "upper", 8.0, CursorIcon::ResizeVertical),
                (renderer, "lower", 0.0, CursorIcon::Grab),
            ]
            .into_iter()
            .enumerate()
            {
                match step {
                    1 => second.lock().unwrap()[0].0.y = 5.0,
                    2 => second.lock().unwrap()[0].1.pass = PrimitivePass::Below,
                    3 => first.lock().unwrap()[0].1.pass = PrimitivePass::Below,
                    4 => {
                        for (_, hit) in objects.lock().unwrap().iter_mut() {
                            hit.pass = PrimitivePass::Above;
                        }
                        second.lock().unwrap()[0].1.pass = PrimitivePass::Above;
                    }
                    5 => {
                        second.lock().unwrap()[0].1.pass = PrimitivePass::Below;
                        objects.lock().unwrap()[1].1.pass = PrimitivePass::Below;
                    }
                    6 => {
                        pos.y += 8.0;
                        objects.lock().unwrap()[1].1.pass = PrimitivePass::Above;
                    }
                    _ => {}
                }
                chart.drain_events().for_each(drop);
                let time = 1.0 + step as f64;
                let actual_cursor = frame(&ctx, &mut chart, time, vec![Event::PointerMoved(pos)]);
                let pointer = chart
                    .drain_events()
                    .find_map(|event| match event {
                        ChartEvent::CrosshairMoved(Some(pointer)) => Some(pointer),
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(
                    pointer.target,
                    Some(target),
                    "secondary={secondary}, move_main={move_main}, backgrounds={backgrounds}, step={step}"
                );
                let hit = pointer.primitive_hit.as_ref().unwrap();
                assert_eq!(hit.object_id.as_deref(), Some(id));
                assert_eq!(
                    hit.data.as_deref(),
                    Some(&serde_json::json!({ "object": id }))
                );
                assert_eq!((hit.distance, hit.cursor), (distance, cursor));
                assert_eq!(actual_cursor, cursor);
                frame(&ctx, &mut chart, time + 0.1, vec![]);
                assert!(
                    !chart
                        .drain_events()
                        .any(|event| matches!(event, ChartEvent::CrosshairMoved(_)))
                );
                for pressed in [true, false] {
                    frame(
                        &ctx,
                        &mut chart,
                        time + 0.2,
                        vec![Event::PointerButton {
                            pos,
                            button: egui::PointerButton::Primary,
                            pressed,
                            modifiers: Default::default(),
                        }],
                    );
                }
                assert!(chart.drain_events().any(|event| matches!(event,
                    ChartEvent::Click { position, .. }
                        if position.target == pointer.target && position.primitive_hit == pointer.primitive_hit
                )));
            }
            // Payload changes under a stationary pointer still reach the host.
            objects.lock().unwrap()[1].1.data =
                Some(Arc::new(serde_json::json!({ "updated": true })));
            frame(&ctx, &mut chart, 9.0, vec![]);
            assert!(chart.drain_events().any(|event| matches!(event,
                ChartEvent::CrosshairMoved(Some(pointer))
                    if pointer.primitive_hit.as_ref().unwrap().data.as_deref() == Some(&serde_json::json!({ "updated": true }))
            )));
            chart.set_series_visible(rival, false);
            // Legacy hits retain their cursor and the latest attachment wins a tie.
            chart.attach_primitive(series, Dots);
            let latest = chart.attach_primitive(series, Dots).unwrap();
            objects.lock().unwrap().clear();
            for distance in [f32::NAN, f32::INFINITY, -1.0] {
                let invalid = chart.attach_primitive(series, Invalid(distance)).unwrap();
                frame(&ctx, &mut chart, 9.05, vec![]);
                assert_eq!(
                    chart.host_events.target,
                    Some(PointerTarget::Primitive(latest))
                );
                assert_eq!(
                    chart.host_events.primitive_hit,
                    Some(PrimitiveHit::new(CursorIcon::PointingHand))
                );
                chart.detach_primitive(invalid);
            }
            chart.set_series_visible(series, false);
            frame(&ctx, &mut chart, 9.1, vec![]);
            assert!(chart.host_events.primitive_hit.is_none());
        }
    }

    #[test]
    fn custom_series_draws_and_hit_tests_in_place_of_its_plots() {
        let ctx = egui::Context::default();
        let mut chart = chart(&[10.0], &[20.0]);
        let color = Color32::from_rgb(10, 11, 12);
        let id = chart.add_custom_series(
            Box::new(
                CustomIndicator::new("Range", |bars| {
                    bars.iter()
                        .map(|bar| IndicatorValue::Single(bar.high - bar.low))
                        .collect()
                })
                .with_overlay(false)
                .with_color(color),
            ),
            Dots,
        );
        #[cfg(feature = "drawings")]
        {
            let mut drawing = crate::drawings::Drawing::with_color(
                1,
                crate::drawings::DrawingToolType::HorizontalLine,
                [255, 0, 0, 255],
            );
            drawing.chart_points = vec![crate::drawings::ChartPoint::new(50.0, 10.0)];
            drawing.pane = chart.series_pane(id).map(|pane| pane.0);
            chart.drawings_mut().drawings.push(drawing);
        }
        frame(&ctx, &mut chart, 0.0, vec![]);
        let shapes = shapes(&ctx, &mut chart, 0.1, vec![]);
        #[cfg(feature = "drawings")]
        {
            // The last pass of a study pane is above the pane's drawings.
            let red = |shape: &Shape| match shape {
                Shape::Path(path) => {
                    path.stroke.color == egui::epaint::ColorMode::Solid(Color32::RED)
                }
                Shape::LineSegment { stroke, .. } => stroke.color == Color32::RED,
                // Unchanged drawings paint their cached triangles in the same place in the list.
                Shape::Mesh(mesh) => mesh.triangles().any(|triangle| {
                    triangle
                        .into_iter()
                        .all(|index| mesh.vertices[index as usize].color == Color32::RED)
                }),
                _ => false,
            };
            let above = |shape: &Shape| matches!(shape, Shape::Rect(rect) if rect.fill == ABOVE);
            let drawing = shapes
                .iter()
                .position(red)
                .expect("the red drawing is painted");
            let last_pass = shapes
                .iter()
                .position(above)
                .expect("the last pass is painted");
            assert!(
                drawing < last_pass,
                "the last pass must paint above the drawing"
            );
        }
        assert!(
            shapes
                .iter()
                .any(|shape| matches!(shape, Shape::Circle(dot) if dot.fill == SERIES))
        );
        assert!(!shapes.iter().any(|shape| match shape {
            Shape::Path(path) => path.stroke.color == egui::epaint::ColorMode::Solid(color),
            Shape::LineSegment { stroke, .. } => stroke.color == color,
            _ => false,
        }));
        let pane = chart.last_rendered_indicator_panes[0].chart_rect;
        // Away from the values, where the study's own line hit test would miss.
        let pos = Pos2::new(pane.center().x, pane.bottom() - 2.0);
        chart.drain_events().for_each(drop);
        frame(&ctx, &mut chart, 0.2, vec![Event::PointerMoved(pos)]);
        let events: Vec<_> = chart.drain_events().collect();
        let Some(ChartEvent::CrosshairMoved(Some(pointer))) = events.last() else {
            panic!("{events:?}")
        };
        assert_eq!(
            pointer.target,
            Some(PointerTarget::Element(ChartElementId::Indicator(id)))
        );
        assert_eq!(
            pointer.primitive_hit,
            Some(PrimitiveHit::new(CursorIcon::PointingHand))
        );
        let click = |pressed| Event::PointerButton {
            pos,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: Default::default(),
        };
        frame(&ctx, &mut chart, 0.3, vec![click(true), click(false)]);
        assert_eq!(
            chart.selection.selected_id(),
            Some(ChartElementId::Indicator(id))
        );
        let pane = chart.series_pane(id).unwrap();
        chart.set_preserve_empty_pane(pane, true);
        #[cfg(feature = "drawings")]
        chart.drawings_mut().drawings.clear();
        let cover = chart.add_indicator(Box::new(CustomIndicator::new("Cover", |bars| {
            vec![IndicatorValue::Single(15.0); bars.len()]
        })));
        for (step, destination) in [super::super::PaneId::MAIN, pane].into_iter().enumerate() {
            chart.move_series_to_pane(id, destination);
            chart.move_series_to_pane(cover, destination);
            for (layer, (order, expected)) in [
                (super::super::VisualOrder::SendToBack, cover),
                (super::super::VisualOrder::BringToFront, id),
            ]
            .into_iter()
            .enumerate()
            {
                chart.drain_events().for_each(drop);
                chart.reorder_indicator(id, order);
                let time = 1.0 + step as f64 + layer as f64 * 0.2;
                frame(&ctx, &mut chart, time, vec![]);
                let map = if destination == super::super::PaneId::MAIN {
                    chart.get_chart_mapping().price
                } else {
                    chart.series_price_map(cover).unwrap()
                };
                let pos = Pos2::new(chart.get_chart_mapping().idx_to_x(70), map.price_to_y(15.0));
                frame(&ctx, &mut chart, time + 0.1, vec![Event::PointerMoved(pos)]);
                assert!(chart.drain_events().any(|event| matches!(event,
                    ChartEvent::CrosshairMoved(Some(pointer))
                        if pointer.target == Some(PointerTarget::Element(ChartElementId::Indicator(expected)))
                )), "{destination:?} {order:?}");
            }
        }
    }
}
