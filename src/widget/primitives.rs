//! Host primitives attached to series, and custom series drawn by them.

use super::{Chart, ChartElementId, PointerTarget, SeriesId};
use crate::chart::coords::ChartMapping;
use crate::chart::primitives::{Primitive, PrimitiveId, PrimitivePass, PrimitiveView};
use crate::model::Bar;
use crate::studies::Indicator;
use egui::{CursorIcon, Painter, Pos2};
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
    pub(super) fn draw(&self, painter: &Painter, view: &PrimitiveView<'_>, pass: PrimitivePass) {
        let factor =
            super::merge::factor(view.mapping.bar_spacing, painter.ctx().pixels_per_point());
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
#[derive(Clone, Copy)]
pub(super) struct PrimitiveHit {
    /// The custom series a renderer draws, or the primitive itself.
    pub(super) target: PointerTarget,
    pub(super) cursor: CursorIcon,
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
        if self.in_pane(series) {
            return None;
        }
        let mapping = if series == SeriesId::MAIN {
            self.main_series_visible.then_some(*coords)?
        } else if let Some(item) = self.price_series.iter().find(|item| item.id == series) {
            item.visible.then(|| ChartMapping {
                price: self.series_map(item, coords.rect),
                ..*coords
            })?
        } else {
            let idx = self.study_ids.iter().position(|id| *id == series)?;
            let study = self.rendered_studies().indicators().get(idx)?;
            (study.is_visible()
                && study.is_overlay()
                && (!study.is_volume()
                    || matches!(study.scale(), crate::studies::StudyScale::Shared(_))))
            .then(|| crate::chart::renderers::study_mapping(study.as_ref(), coords))?
        };
        Some(self.primitive_view(series, mapping))
    }

    /// The primitives drawn in the main pane, with their views.
    pub(super) fn main_primitives<'a>(
        &'a self,
        coords: &'a ChartMapping,
    ) -> impl DoubleEndedIterator<Item = (&'a Attached, PrimitiveView<'a>)> {
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
                    .then(|| self.primitive_view(item.series, *coords))
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
            if studies.get(idx).is_some_and(|study| {
                study.is_visible()
                    && study.is_overlay()
                    && study.scale() == crate::studies::StudyScale::Main
            }) && self.study_autoscale[idx]
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
        for (item, view) in self
            .main_primitives(coords)
            .filter(|(item, _)| pass != PrimitivePass::Series || !item.replaces)
        {
            item.draw(&painter, &view, pass);
        }
    }
}

/// The latest attached primitive under `pos`.
pub(super) fn primitive_at<'a>(
    primitives: impl DoubleEndedIterator<Item = (&'a Attached, PrimitiveView<'a>)>,
    pos: Pos2,
) -> Option<PrimitiveHit> {
    primitives.rev().find_map(|(item, view)| {
        let cursor = item.get().hit_test(pos, &view)?;
        Some(PrimitiveHit {
            target: if item.replaces {
                PointerTarget::Element(ChartElementId::Indicator(item.series))
            } else {
                PointerTarget::Primitive(item.id)
            },
            cursor,
        })
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
    use egui::{Color32, Event, Shape};

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
        chart.add_indicator(Box::new(
            CustomIndicator::new("Mid", |bars| {
                bars.iter()
                    .map(|bar| IndicatorValue::Single((bar.high + bar.low) / 2.0))
                    .collect()
            })
            .with_color(study),
        ));
        chart.attach_primitive(SeriesId::MAIN, Layers);
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
    }
}
