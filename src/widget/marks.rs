//! Visible-range requests for host-owned annotations and events.

use super::{Chart, ChartEvent, SeriesId};
use crate::chart::coords::ChartMapping;
use crate::chart::rendering::RenderContext;
use crate::data::{FeedError, inbox::Inbox};
use crate::model::{Bar, EventVisibility, Marker, MarkerZOrder, TimeScaleEvent};
use crate::services::{Marks, MarksPage, MarksRequest};
use chrono::{DateTime, Utc};
use std::sync::{Arc, Mutex, PoisonError};

/// Where a bar mark with a host ID was painted.
pub(super) struct MarkHit {
    id: String,
    series: SeriesId,
    time: DateTime<Utc>,
    rect: egui::Rect,
}

pub(super) struct MarkSource {
    source: Arc<dyn Marks>,
    inbox: Arc<Inbox>,
    request: Option<MarksRequest>,
    wake: Arc<Mutex<egui::ViewportId>>,
    pub(super) page: MarksPage,
    error: Option<FeedError>,
}

/// A time, as markers and time scale events carry.
pub(crate) trait Timed {
    fn time(&self) -> DateTime<Utc>;
}

impl Timed for Marker {
    fn time(&self) -> DateTime<Utc> {
        self.time
    }
}

impl Timed for TimeScaleEvent {
    fn time(&self) -> DateTime<Utc> {
        self.time
    }
}

/// Items in supplied painting order, indexed by time so a frame visits only those near the view.
#[derive(Clone)]
pub(crate) struct TimeOrdered<T> {
    items: Vec<T>,
    /// Indices into `items` by time; equal times keep the supplied order.
    by_time: Vec<usize>,
}

impl<T> Default for TimeOrdered<T> {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            by_time: Vec::new(),
        }
    }
}

impl<T: Timed> From<Vec<T>> for TimeOrdered<T> {
    fn from(items: Vec<T>) -> Self {
        let mut by_time: Vec<usize> = (0..items.len()).collect();
        by_time.sort_by_key(|&index| items[index].time());
        Self { items, by_time }
    }
}

impl<T> std::ops::Deref for TimeOrdered<T> {
    type Target = [T];

    fn deref(&self) -> &[T] {
        &self.items
    }
}

impl<T: Timed> TimeOrdered<T> {
    /// Indices timed from `from` through `to`, a side open when `None`, in supplied order.
    pub(crate) fn within(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
    ) -> Vec<usize> {
        let time = |index: &usize| self.items[*index].time();
        let start = from.map_or(0, |from| self.by_time.partition_point(|i| time(i) < from));
        let end = to.map_or(self.by_time.len(), |to| {
            self.by_time.partition_point(|i| time(i) <= to)
        });
        if end.saturating_sub(start) == self.items.len() {
            return (0..self.items.len()).collect();
        }
        let mut indices = self.by_time[start..end.max(start)].to_vec();
        indices.sort_unstable();
        indices
    }
}

/// Times a point and a bar beyond `rect` in `coords`, open past the data or without an inverse.
/// This includes every marker or badge drawn in the rect; renderers still test each position.
pub(super) fn window(
    bars: &[Bar],
    coords: &ChartMapping,
    rect: egui::Rect,
) -> (Option<DateTime<Utc>>, Option<DateTime<Utc>>) {
    if bars.is_empty() || !(f32::EPSILON..).contains(&coords.bar_spacing) {
        return (None, None);
    }
    let last = (bars.len() - 1) as f64;
    let left = (coords.x_to_idx_f64(rect.left() - 1.0) - 1.0).floor();
    let right = (coords.x_to_idx_f64(rect.right() + 1.0) + 1.0).ceil();
    (
        (left > 0.0 && left <= last).then(|| bars[left as usize].time),
        (right >= 0.0 && right < last).then(|| bars[right as usize].time),
    )
}

impl Chart {
    /// Paints the markers of the visible bar series whose `MarkerOptions` place them at
    /// `layer`: the main series' own and source bar marks, and those of the added series.
    pub(super) fn paint_marks(
        &self,
        ctx: &RenderContext,
        coords: &ChartMapping,
        layer: MarkerZOrder,
    ) {
        // Normal markers of added series, and all markers in other panes, are painted with
        // each series.
        for item in self
            .main_pane_series()
            .filter(|item| layer != MarkerZOrder::Normal && item.marker_options.z_order == layer)
        {
            let map = self.series_map(item, ctx.rect);
            self.paint_series_markers(ctx.painter, ctx.rect, item, map);
        }
        if !self.main_series_visible {
            return;
        }
        if layer == MarkerZOrder::Normal
            && let Some(marks) = self
                .change_marks
                .as_ref()
                .filter(|_| !self.chart_type.is_time_independent())
        {
            marks.paint(ctx.painter, |time, close| {
                let index = crate::model::index_at(self.state.shown(), time)?;
                let x = coords.index_to_x(index);
                Some(egui::Pos2::new(x, coords.price.price_to_y(close)))
            });
        }
        if self.marker_options.z_order != layer {
            return;
        }
        let source: &[Marker] = self
            .mark_source
            .as_ref()
            .map_or(&[], |source| &source.page.bar_marks);
        if self.marks.is_empty() && source.is_empty() {
            return;
        }
        // Source pages already cover the view; keep manual then source painting order and keys.
        let (from, to) = window(self.state.shown(), coords, ctx.rect);
        let marks = self
            .marks
            .within(from, to)
            .into_iter()
            .map(|index| (index, &self.marks[index]))
            .chain(
                source
                    .iter()
                    .enumerate()
                    .map(|(index, mark)| (self.marks.len() + index, mark)),
            );
        let mapping = ChartMapping {
            start_idx: 0,
            ..coords.clone()
        };
        if self.chart_type.is_time_independent() {
            let mapped = crate::chart::rendering::candles::price_based::markers(
                marks,
                &self.state,
                self.chart_type,
                &self.chart_type_params,
                self.older_bars_drawn,
            );
            let hits = crate::chart::rendering::render_markers(
                ctx,
                mapped.iter().map(|(index, mark)| (*index, mark)),
                self.state.shown(),
                &coords.price,
                &mapping,
            );
            self.record_mark_hits(ctx.painter, SeriesId::MAIN, hits);
        } else {
            let hits = crate::chart::rendering::render_markers(
                ctx,
                marks,
                self.state.shown(),
                &coords.price,
                &mapping,
            );
            self.record_mark_hits(ctx.painter, SeriesId::MAIN, hits);
        }
    }

    /// Paints a secondary pane's markers at their configured layer in series order.
    pub(super) fn paint_pane_marks(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        members: &[SeriesId],
        layer: MarkerZOrder,
    ) {
        let painter = painter.with_clip_rect(painter.clip_rect().intersect(rect));
        for id in members {
            if let Some(item) = self
                .price_series
                .iter()
                .find(|item| item.id == *id && item.visible && item.marker_options.z_order == layer)
            {
                self.paint_series_markers(&painter, rect, item, self.series_map(item, rect));
            }
        }
    }

    /// Resolves hover against painted marks and retains host-identified rectangles for clicks.
    pub(super) fn record_mark_hits(
        &self,
        painter: &egui::Painter,
        series: SeriesId,
        hits: Vec<(&Marker, egui::Rect)>,
    ) {
        if let Some(pos) = painter.ctx().pointer_interact_pos()
            && let Some((mark, _)) = hits.iter().rev().find(|(_, rect)| rect.contains(pos))
        {
            *self.host_events.annotation_target.borrow_mut() = Some(super::PointerTarget::Marker {
                series,
                id: mark.id.clone(),
                time: mark.time,
            });
        }
        self.mark_hits
            .borrow_mut()
            .extend(hits.into_iter().filter_map(|(mark, rect)| {
                Some(MarkHit {
                    id: mark.id.clone()?,
                    series,
                    time: mark.time,
                    rect,
                })
            }));
    }

    /// A later painted hit covers any annotation found underneath it.
    pub(super) fn cover_annotation_hit(&self) {
        self.host_events.annotation_target.borrow_mut().take();
    }

    /// Paints a host price line and records only its visible plot stroke and title.
    pub(super) fn paint_annotation_line(
        &self,
        painter: &egui::Painter,
        rect: egui::Rect,
        map: &crate::scales::PriceMap,
        series: SeriesId,
        id: super::PriceLineId,
        line: &crate::model::PriceLine,
    ) {
        crate::chart::rendering::render_price_line(
            painter,
            rect,
            map,
            line,
            rect.left(),
            false,
            rect.right(),
        );
        let clip = rect.intersect(painter.clip_rect());
        let Some(pos) = painter
            .ctx()
            .pointer_interact_pos()
            .filter(|pos| clip.is_positive() && clip.contains(*pos))
        else {
            return;
        };
        if !map.range().contains(map.transform().to_scale(line.price)) {
            return;
        }
        let y = map.price_to_y(line.price);
        let stroke_hit = line.width > 0.0 && (pos.y - y).abs() <= (line.width * 0.5).max(3.0);
        let title_hit = line.title.as_ref().is_some_and(|title| {
            let galley = painter.layout_no_wrap(
                title.clone(),
                crate::chart::rendering::axes::axis_font(painter.ctx()),
                line.color,
            );
            let title_rect = egui::Align2::LEFT_BOTTOM
                .anchor_size(egui::pos2(rect.left(), y), galley.size())
                .intersect(clip);
            title_rect.is_positive() && title_rect.contains(pos)
        });
        if stroke_hit || title_hit {
            *self.host_events.annotation_target.borrow_mut() =
                Some(super::PointerTarget::PriceLine { series, id });
        }
    }

    /// Starts a frame's record of painted marks; clicks resolve against the previous frame's.
    pub(super) fn begin_mark_hits(&mut self) {
        self.clickable_marks = std::mem::take(self.mark_hits.get_mut());
    }

    /// The mark painted on top at `pos`: the last of those painted there. Drawing tools and
    /// the eraser take the click first.
    pub(super) fn mark_at(&self, pos: egui::Pos2) -> Option<&MarkHit> {
        #[cfg(feature = "drawings")]
        if self.drawings.active_tool.is_some() || self.drawings.eraser_mode {
            return None;
        }
        self.clickable_marks
            .iter()
            .rev()
            .find(|hit| hit.rect.contains(pos))
    }

    /// Reports a click on a mark with a host ID.
    pub(super) fn click_mark(&mut self, pos: egui::Pos2) {
        let Some(hit) = self.mark_at(pos) else {
            return;
        };
        let event = ChartEvent::MarkClick {
            id: hit.id.clone(),
            series: hit.series,
            time: hit.time,
        };
        self.host_events.push(event);
    }

    /// Connects a source of bar marks and time-axis events. Replacing or removing the source
    /// clears its marks and pending replies; manually supplied annotations are retained.
    pub fn set_mark_source(&mut self, source: Option<Arc<dyn Marks>>) {
        self.mark_source = source.map(|source| MarkSource {
            source,
            inbox: Arc::default(),
            request: None,
            wake: Arc::new(Mutex::new(egui::ViewportId::ROOT)),
            page: MarksPage::default(),
            error: None,
        });
    }

    /// Requests fresh marks on the next frame, for example after a host-side event update.
    pub fn reload_marks(&mut self) {
        if let Some(source) = &mut self.mark_source {
            source.request = None;
            source.inbox = Arc::default();
            source.page = MarksPage::default();
            source.error = None;
        }
    }

    /// Takes the last mark request error. Retry with `reload_marks`; errors never clear manual
    /// markers. Answers for a previous symbol, interval or viewport are ignored.
    pub fn take_mark_error(&mut self) -> Option<FeedError> {
        self.mark_source.as_mut()?.error.take()
    }

    /// Supplies typed corporate, economic and news events independently of a mark source.
    /// Replaces the previous list; `set_events` continues to accept generic badges.
    pub fn set_time_scale_events(&mut self, events: Vec<TimeScaleEvent>) {
        self.time_scale_events = events.into();
    }

    /// Visibility of typed time-axis events, shared with the chart settings dialog.
    pub fn event_visibility(&self) -> &EventVisibility {
        &self.event_visibility
    }

    /// Changes event visibility without discarding the supplied events or fetching them again.
    pub fn set_event_visibility(&mut self, visibility: EventVisibility) {
        self.event_visibility = visibility;
    }

    pub(super) fn pump_marks(&mut self, ctx: &egui::Context) {
        let range = self.visible_time_range();
        let Some(source) = &mut self.mark_source else {
            return;
        };
        *source.wake.lock().unwrap_or_else(PoisonError::into_inner) = ctx.viewport_id();
        let request = range
            .filter(|range| range.from < range.to)
            .map(|range| MarksRequest {
                symbol: self.symbol.clone(),
                timeframe: self.timeframe.unwrap_or_default(),
                from: range.from,
                to: range.to,
            });
        if request != source.request {
            // A separate inbox per request prevents abandoned replies accumulating during pans.
            source.inbox = Arc::default();
            let (ctx, wake) = (ctx.clone(), Arc::clone(&source.wake));
            source.inbox.set_waker(move || {
                let viewport = *wake.lock().unwrap_or_else(PoisonError::into_inner);
                ctx.request_repaint_of(viewport);
            });
            source.page = MarksPage::default();
            source.error = None;
            source.request = request;
            if let Some(request) = &source.request {
                source.source.marks(request.clone(), source.inbox.reply(0));
            }
        }
        for (_, answer) in source.inbox.drain().answers {
            match *answer.downcast::<Result<MarksPage, FeedError>>().unwrap() {
                Ok(page) => source.page = page,
                Err(error) => source.error = Some(error),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Reply;
    use crate::model::{EventKind, Marker, MarkerPos, Timeframe};
    use crate::widget::{SeriesId, test_frames};
    use egui::{Context, Shape};
    use std::sync::Mutex;

    /// Pointer payloads identify current annotations on the owning pane and clear on edits.
    #[test]
    fn annotation_pointer_targets_follow_paint_order_and_removal() {
        use crate::model::{MarkerOptions, PriceLine};
        use crate::widget::{ChartElementId, PointerTarget, SeriesOptions};
        use egui::{Color32, Event, Pos2};

        fn probe(
            ctx: &Context,
            chart: &mut Chart,
            pos: Pos2,
            tick: &mut f64,
        ) -> super::super::ChartPointer {
            *tick += 0.1;
            test_frames::frame(ctx, chart, *tick, vec![Event::PointerMoved(Pos2::ZERO)]);
            chart.drain_events().for_each(drop);
            *tick += 0.1;
            test_frames::frame(ctx, chart, *tick, vec![Event::PointerMoved(pos)]);
            chart
                .drain_events()
                .find_map(|event| match event {
                    ChartEvent::CrosshairMoved(Some(point)) => Some(point),
                    _ => None,
                })
                .unwrap_or_else(|| panic!("pointer in plot at {pos:?}, frame {tick}"))
        }

        for placement in 0..4 {
            let ctx = Context::default();
            let mut chart = test_frames::chart(&[95.0], &[105.0]);
            chart.config_mut().show_volume = false;
            let series = if placement == 1 || placement == 2 {
                let id = chart.add_series(SeriesOptions {
                    chart_type: crate::model::ChartType::Candles,
                    ..Default::default()
                });
                chart.set_data(id, chart.bars(SeriesId::MAIN).to_vec());
                id
            } else {
                SeriesId::MAIN
            };
            if placement >= 2 {
                let pane = chart.add_pane();
                assert!(chart.move_series_to_pane(series, pane));
            }
            chart.set_marker_options(
                series,
                MarkerOptions {
                    auto_scale: false,
                    ..Default::default()
                },
            );
            let time = chart.bars(series)[70].time;
            let mut marker = Marker::new(time).with_pos(MarkerPos::AtPriceMiddle);
            marker.price = Some(100.0);
            chart.set_markers(
                series,
                vec![
                    marker.clone().with_id("first"),
                    marker.clone().with_id("last"),
                ],
            );
            let mut tick = 0.0;
            test_frames::frame(&ctx, &mut chart, tick, vec![]);
            let rect = if placement == 2 {
                chart
                    .pane_layout
                    .panes
                    .iter()
                    .find(|pane| pane.members.contains(&series))
                    .unwrap()
                    .rect
            } else {
                chart.get_rendered_price_rect()
            };
            let pos = egui::pos2(
                chart.get_chart_mapping().idx_to_x(70),
                chart.series_price_map(series).unwrap().price_to_y(100.0),
            );
            assert!(
                rect.contains(pos),
                "placement {placement}: {pos:?} in {rect:?}"
            );
            // On a linear scale 100 is halfway between 95 and 105, including scale padding.
            let map = chart.series_price_map(series).unwrap();
            assert!((pos.y - (map.price_to_y(95.0) + map.price_to_y(105.0)) * 0.5).abs() < 0.001);
            let target = |id: Option<&str>| {
                Some(PointerTarget::Marker {
                    series,
                    id: id.map(str::to_owned),
                    time,
                })
            };
            assert_eq!(
                probe(&ctx, &mut chart, pos, &mut tick).target,
                target(Some("last")),
                "placement {placement}"
            );
            chart.set_markers(
                series,
                vec![
                    marker.clone().with_id("last"),
                    marker.clone().with_id("first"),
                ],
            );
            assert_eq!(
                probe(&ctx, &mut chart, pos, &mut tick).target,
                target(Some("first"))
            );
            chart.set_markers(series, vec![marker.clone()]);
            assert_eq!(probe(&ctx, &mut chart, pos, &mut tick).target, target(None));
            chart.set_markers(
                series,
                vec![Marker {
                    size: 0.0,
                    ..marker.clone().with_id("hidden")
                }],
            );
            assert!(!matches!(
                probe(&ctx, &mut chart, pos, &mut tick).target,
                Some(PointerTarget::Marker { .. })
            ));
            chart.set_markers(series, vec![marker.clone().with_id("stable")]);
            let line = PriceLine::new(100.0, Color32::BLUE).with_axis_label(false);
            let handle = chart.add_price_line(series, line.clone()).unwrap();
            let line_target = Some(PointerTarget::PriceLine { series, id: handle });
            assert_eq!(
                probe(&ctx, &mut chart, pos, &mut tick).target,
                if series == SeriesId::MAIN {
                    line_target.clone()
                } else {
                    target(Some("stable"))
                }
            );
            chart.set_markers(series, vec![]);
            assert_eq!(probe(&ctx, &mut chart, pos, &mut tick).target, line_target);
            assert!(chart.price_line(handle).is_some());
            let later = chart.add_price_line(series, line.clone()).unwrap();
            assert_eq!(
                probe(&ctx, &mut chart, pos, &mut tick).target,
                Some(PointerTarget::PriceLine { series, id: later })
            );
            assert!(chart.remove_price_line(later));
            assert_eq!(probe(&ctx, &mut chart, pos, &mut tick).target, line_target);

            assert!(chart.update_price_line(handle, line.clone().with_width(0.0)));
            assert!(!matches!(
                probe(&ctx, &mut chart, pos, &mut tick).target,
                Some(PointerTarget::PriceLine { .. })
            ));
            assert!(
                chart.update_price_line(handle, line.clone().with_width(0.0).with_title("Level"))
            );
            let title = egui::pos2(chart.get_rendered_price_rect().left() + 5.0, pos.y - 6.0);
            assert_eq!(
                probe(&ctx, &mut chart, title, &mut tick).target,
                line_target
            );
            assert!(chart.update_price_line(handle, line.clone()));
            assert_eq!(probe(&ctx, &mut chart, pos, &mut tick).target, line_target);
            assert!(chart.update_price_line(handle, PriceLine::new(1_000.0, Color32::BLUE)));
            assert!(!matches!(
                probe(&ctx, &mut chart, pos, &mut tick).target,
                Some(PointerTarget::PriceLine { .. })
            ));
            assert!(chart.update_price_line(handle, line));
            assert_eq!(probe(&ctx, &mut chart, pos, &mut tick).target, line_target);
            chart.set_series_visible(series, false);
            tick += 0.1;
            test_frames::frame(&ctx, &mut chart, tick, vec![]);
            assert!(
                chart.drain_events().any(|event| matches!(event,
                    ChartEvent::CrosshairMoved(point) if point.as_ref().is_none_or(|point| !matches!(
                        point.target,
                        Some(PointerTarget::PriceLine { .. } | PointerTarget::Marker { .. })
                    ))
                )),
                "stationary hiding clears the annotation, including an undrawn pane"
            );
            chart.set_series_visible(series, true);
            assert_eq!(probe(&ctx, &mut chart, pos, &mut tick).target, line_target);
            assert!(chart.remove_price_line(handle));
            tick += 0.1;
            test_frames::frame(&ctx, &mut chart, tick, vec![]);
            assert!(chart.drain_events().any(|event| matches!(event, ChartEvent::CrosshairMoved(Some(point)) if point.target != line_target)), "stationary removal");
            assert!(chart.price_line(handle).is_none());
            chart.set_markers(series, vec![marker.with_id("removed")]);
            assert_eq!(
                probe(&ctx, &mut chart, pos, &mut tick).target,
                target(Some("removed"))
            );
            chart.set_markers(series, vec![]);
            tick += 0.1;
            test_frames::frame(&ctx, &mut chart, tick, vec![]);
            assert!(chart.drain_events().any(|event| matches!(event, ChartEvent::CrosshairMoved(Some(point)) if !matches!(point.target, Some(PointerTarget::Marker { .. })))), "stationary marker removal");
            let outside = egui::pos2(
                chart.get_rendered_price_rect().left() - 1.0,
                rect.center().y,
            );
            tick += 0.1;
            test_frames::frame(&ctx, &mut chart, tick, vec![Event::PointerMoved(outside)]);
            assert!(
                chart
                    .drain_events()
                    .any(|event| matches!(event, ChartEvent::CrosshairMoved(None)))
            );
            struct Overlay(crate::widget::PrimitivePass);
            impl crate::widget::Primitive for Overlay {
                fn hit_test_result(
                    &self,
                    pos: Pos2,
                    view: &crate::widget::PrimitiveView<'_>,
                ) -> Option<crate::widget::PrimitiveHit> {
                    (view.mapping.rect.contains(pos)
                        && (pos.y - view.mapping.price_to_y(100.0)).abs() <= 3.0)
                        .then(|| {
                            let mut hit =
                                crate::widget::PrimitiveHit::new(egui::CursorIcon::PointingHand);
                            hit.pass = self.0;
                            hit
                        })
                }
            }
            let primitive = chart
                .attach_primitive(series, Overlay(crate::widget::PrimitivePass::Series))
                .unwrap();
            chart.set_markers(
                series,
                vec![Marker {
                    price: Some(100.0),
                    ..Marker::new(time)
                        .with_pos(MarkerPos::AtPriceMiddle)
                        .with_id("layered")
                }],
            );
            for layer in [
                MarkerZOrder::Normal,
                MarkerZOrder::AboveSeries,
                MarkerZOrder::Top,
            ] {
                chart.set_marker_options(
                    series,
                    MarkerOptions {
                        auto_scale: false,
                        z_order: layer,
                    },
                );
                let point = probe(&ctx, &mut chart, pos, &mut tick);
                if layer == MarkerZOrder::Normal {
                    assert_eq!(point.target, Some(PointerTarget::Primitive(primitive)));
                    assert!(point.primitive_hit.is_some());
                } else {
                    assert_eq!(point.target, target(Some("layered")));
                    assert!(point.primitive_hit.is_none());
                }
            }
            assert!(chart.detach_primitive(primitive));
            #[cfg(feature = "drawings")]
            {
                let mut drawing = crate::drawings::Drawing::with_color(
                    77,
                    crate::drawings::DrawingToolType::HorizontalLine,
                    [0, 0, 255, 255],
                );
                drawing.chart_points = vec![crate::drawings::ChartPoint::new(70.0, 100.0)];
                drawing.pane = (series != SeriesId::MAIN).then_some(series.0);
                chart.drawings_mut().drawings.push(drawing);
                for layer in [
                    MarkerZOrder::Normal,
                    MarkerZOrder::AboveSeries,
                    MarkerZOrder::Top,
                ] {
                    chart.set_marker_options(
                        series,
                        MarkerOptions {
                            auto_scale: false,
                            z_order: layer,
                        },
                    );
                    assert_eq!(
                        probe(&ctx, &mut chart, pos, &mut tick).target,
                        if layer == MarkerZOrder::Top {
                            target(Some("layered"))
                        } else {
                            Some(PointerTarget::Drawing(77))
                        }
                    );
                }
                chart.drawings_mut().drawings.clear();
            }
            for pass in [
                crate::widget::PrimitivePass::Below,
                crate::widget::PrimitivePass::Above,
            ] {
                let primitive = chart.attach_primitive(series, Overlay(pass)).unwrap();
                let point = probe(&ctx, &mut chart, pos, &mut tick);
                assert_eq!(
                    point.target,
                    if pass == crate::widget::PrimitivePass::Above {
                        Some(PointerTarget::Primitive(primitive))
                    } else {
                        target(Some("layered"))
                    }
                );
                assert!(chart.detach_primitive(primitive));
            }
            chart.set_markers(series, vec![]);
            let candle = egui::pos2(
                pos.x,
                chart.series_price_map(series).unwrap().price_to_y(102.0),
            );
            assert!(matches!(
                probe(&ctx, &mut chart, candle, &mut tick).target,
                Some(PointerTarget::Element(ChartElementId::Series(_)))
            ));

            let whitespace: Vec<_> = chart
                .bars(series)
                .iter()
                .map(|bar| Bar::whitespace(bar.time))
                .collect();
            chart.set_data(SeriesId::MAIN, whitespace.clone());
            if series != SeriesId::MAIN {
                chart.set_data(series, whitespace);
            }
            chart.chart_options.time_scale.ignore_whitespace_indices = true;
            chart.chart_options.time_scale.ignore_hidden_indices = true;
            chart
                .series_price_scale_mut(series)
                .unwrap()
                .set_range(95.0, 105.0);
            chart.set_markers(
                series,
                vec![
                    Marker::new(time)
                        .at_price(100.0, MarkerPos::AtPriceMiddle)
                        .with_id("filtered"),
                ],
            );
            let point = probe(&ctx, &mut chart, pos, &mut tick);
            assert_eq!(point.target, target(Some("filtered")));
            assert!(point.time.is_none() && point.bar.is_none());
            assert!(point.series_bars.is_empty() && point.studies.is_empty());
            chart.set_markers(series, vec![]);
            let line = chart
                .add_price_line(series, PriceLine::new(100.0, Color32::BLUE))
                .unwrap();
            let point = probe(&ctx, &mut chart, pos, &mut tick);
            assert_eq!(
                point.target,
                Some(PointerTarget::PriceLine { series, id: line })
            );
            assert!(point.time.is_none() && point.bar.is_none());
            assert!(chart.remove_price_line(line));
            tick += 0.1;
            test_frames::frame(&ctx, &mut chart, tick, vec![]);
            assert!(
                chart
                    .drain_events()
                    .any(|event| matches!(event, ChartEvent::CrosshairMoved(None)))
            );
        }
    }

    #[derive(Default)]
    struct Source(Mutex<Vec<(MarksRequest, Reply<MarksPage>)>>);

    impl Marks for Source {
        fn marks(&self, request: MarksRequest, reply: Reply<MarksPage>) {
            self.0.lock().unwrap().push((request, reply));
        }
    }

    /// Requests track the view, accept only its latest reply and keep manual annotations.
    #[test]
    fn requests_follow_the_view_and_discard_stale_answers() {
        let ctx = Context::default();
        let mut chart = test_frames::chart(&[95.0], &[105.0]);
        chart.config_mut().show_volume = false;
        test_frames::frame(&ctx, &mut chart, 0.0, vec![]);
        let time = chart.state.shown()[70].time;
        chart.set_markers(SeriesId::MAIN, vec![Marker::new(time).with_text("Manual")]);
        let source = Arc::new(Source::default());
        chart.set_mark_source(Some(source.clone()));
        test_frames::frame(&ctx, &mut chart, 0.01, vec![]);
        let (request, stale) = source.0.lock().unwrap().pop().unwrap();
        let range = chart.visible_time_range().unwrap();
        assert_eq!((request.from, request.to), (range.from, range.to));
        chart.pump_marks(&ctx);
        assert!(
            source.0.lock().unwrap().is_empty(),
            "an unchanged view must not refetch"
        );
        chart.state.time_scale_mut().set_bar_spacing(20.0);
        chart.pump_marks(&ctx);
        let (new_request, reply) = source.0.lock().unwrap().pop().unwrap();
        assert_ne!(request, new_request);
        let page = MarksPage {
            bar_marks: vec![
                Marker::new(time)
                    .with_pos(MarkerPos::Top)
                    .with_text("Source bar"),
            ],
            time_scale_marks: vec![Marker::event(time, "Axis").with_id("source-axis")],
            ..Default::default()
        };
        stale.send(Ok(page.clone()));
        chart.pump_marks(&ctx);
        assert!(
            chart
                .mark_source
                .as_ref()
                .unwrap()
                .page
                .bar_marks
                .is_empty()
        );
        reply.send(Ok(page));
        chart.pump_marks(&ctx);
        let shapes = test_frames::shapes(&ctx, &mut chart, 0.1, vec![]);
        for label in ["Manual", "Source bar", "Axis"] {
            assert!(
                shapes
                    .iter()
                    .any(|s| matches!(s, Shape::Text(t) if t.galley.text() == label)),
                "missing {label}"
            );
        }
        chart.set_visible_bars(50);
        let shapes = test_frames::shapes(&ctx, &mut chart, 0.2, vec![]);
        assert!(
            !shapes
                .iter()
                .any(|s| matches!(s, Shape::Text(t) if t.galley.text() == "Source bar")),
            "a layout change must invalidate source marks before painting"
        );
        chart.reload_marks();
        chart.pump_marks(&ctx);
        source
            .0
            .lock()
            .unwrap()
            .pop()
            .unwrap()
            .1
            .send(Err(FeedError::Network("offline".into())));
        chart.pump_marks(&ctx);
        assert_eq!(
            chart.take_mark_error(),
            Some(FeedError::Network("offline".into()))
        );
        chart.reload_marks();
        chart.pump_marks(&ctx);
        source
            .0
            .lock()
            .unwrap()
            .pop()
            .unwrap()
            .1
            .send(Ok(MarksPage::default()));
        chart.pump_marks(&ctx);
        assert!(
            chart
                .mark_source
                .as_ref()
                .unwrap()
                .page
                .bar_marks
                .is_empty()
        );
        assert_eq!(chart.marks.len(), 1);
        for timeframe in [Timeframe::Hour1, Timeframe::Day1] {
            chart.set_timeframe(timeframe);
            chart.pump_marks(&ctx);
            assert_eq!(
                source.0.lock().unwrap().pop().unwrap().0.timeframe,
                timeframe
            );
        }
        chart.set_symbol("NEXT");
        chart.pump_marks(&ctx);
        assert_eq!(source.0.lock().unwrap().pop().unwrap().0.symbol, "NEXT");
    }

    /// Windowing keeps sub-bar edge marks, future-margin marks and supplied painting order.
    #[test]
    fn markers_near_the_view_are_drawn_and_hit_in_list_order() {
        let ctx = Context::default();
        let mut chart = test_frames::chart(&[95.0], &[105.0]);
        chart.config_mut().show_volume = false;
        let bars = chart.bars(SeriesId::MAIN).to_vec();
        let series = chart.add_series(crate::widget::SeriesOptions::default());
        chart.set_data(series, bars.clone());
        test_frames::frame(&ctx, &mut chart, 0.0, vec![]);
        chart.apply_synced_time_scale(20.0, 4.0);
        test_frames::frame(&ctx, &mut chart, 0.1, vec![]);
        let left = chart
            .get_chart_mapping()
            .x_to_idx_f64(chart.get_rendered_price_rect().left());
        let time = |index| crate::model::time_at(&bars, index).unwrap();
        let marks: Vec<_> = [
            ("far-left", 0.0),
            ("future-in", 102.0),
            ("edge-in", left + 0.25),
            ("future-out", 104.0),
            ("edge-out", left - 0.25),
            ("middle", 90.0),
        ]
        .into_iter()
        .map(|(id, index)| Marker::new(time(index)).with_id(id))
        .collect();
        chart.set_markers(SeriesId::MAIN, marks.clone());
        chart.set_markers(series, marks);
        chart.set_time_scale_events(
            [0.0, 90.0, 102.0, 104.0]
                .into_iter()
                .map(|index| TimeScaleEvent {
                    id: index.to_string(),
                    time: time(index),
                    kind: EventKind::News,
                    title: "Headline".into(),
                    details: String::new(),
                })
                .collect(),
        );
        let shapes = test_frames::shapes(&ctx, &mut chart, 0.2, vec![]);
        for series in [SeriesId::MAIN, series] {
            let hits = chart.mark_hits.borrow();
            let ids: Vec<_> = hits
                .iter()
                .filter(|hit| hit.series == series)
                .map(|hit| hit.id.as_str())
                .collect();
            assert_eq!(ids, ["future-in", "edge-in", "middle"]);
        }
        assert_eq!(
            shapes
                .iter()
                .filter(|shape| matches!(shape,
            Shape::Circle(circle) if circle.radius == 10.0 && circle.stroke.width == 1.2))
                .count(),
            2
        );
    }

    /// Every category is retained while hidden and restored by the same chart settings.
    #[test]
    fn category_visibility_filters_events_without_discarding_them() {
        let ctx = Context::default();
        let mut chart = test_frames::chart(&[95.0], &[105.0]);
        chart.config_mut().show_volume = false;
        let kinds = [
            EventKind::Dividend,
            EventKind::Split,
            EventKind::Earnings,
            EventKind::Economic,
            EventKind::News,
            EventKind::Idea,
        ];
        chart.set_time_scale_events(
            kinds
                .iter()
                .map(|&kind| TimeScaleEvent {
                    id: kind.label().into(),
                    time: chart.state.shown()[70].time,
                    kind,
                    title: "Host headline".into(),
                    details: "Host details".into(),
                })
                .collect(),
        );
        for separate_pane in [false, true] {
            if separate_pane {
                #[cfg(feature = "studies")]
                chart.add_indicator(Box::new(crate::studies::RSI::new(14)));
                #[cfg(not(feature = "studies"))]
                continue;
            }
            for selected in [
                None,
                Some(0),
                Some(1),
                Some(2),
                Some(3),
                Some(4),
                Some(5),
                None,
            ] {
                let visibility = EventVisibility {
                    show_dividends: selected == Some(0),
                    show_splits: selected == Some(1),
                    show_earnings: selected == Some(2),
                    // Breaks follow their own switch, not the earnings badge.
                    show_earnings_breaks: selected == Some(1),
                    show_economic_events: selected == Some(3),
                    show_news: selected == Some(4),
                    show_news_notifications: false,
                    show_ideas: selected == Some(5),
                };
                #[cfg(feature = "ui")]
                {
                    use crate::widget::settings::ChartSettingsState;
                    let mut settings = ChartSettingsState::from_chart(&chart);
                    settings.events = visibility.clone();
                    settings.apply_to_chart(&mut chart).unwrap();
                    assert_eq!(ChartSettingsState::from_chart(&chart).events, visibility);
                }
                #[cfg(not(feature = "ui"))]
                chart.set_event_visibility(visibility);
                let shapes = test_frames::shapes(&ctx, &mut chart, 0.0, vec![]);
                for (index, kind) in kinds.into_iter().enumerate() {
                    assert_eq!(
                        shapes.iter().any(
                            |s| matches!(s, Shape::Text(t) if t.galley.text() == kind.label())
                        ),
                        selected == Some(index),
                        "{kind:?}"
                    );
                }
                let info = crate::theme::ThemeExt::active_theme(&ctx).ui.info;
                assert_eq!(
                    shapes
                        .iter()
                        .any(|s| matches!(s, Shape::LineSegment { points, stroke }
                        if points[0].x == points[1].x && stroke.color == info)),
                    selected == Some(1),
                    "earnings break"
                );
                assert_eq!(chart.time_scale_events.len(), kinds.len());
                if separate_pane && selected.is_some() {
                    let badge = shapes
                        .iter()
                        .find_map(|s| match s {
                            Shape::Circle(circle)
                                if circle.radius == 10.0 && circle.stroke.width == 1.2 =>
                            {
                                Some(circle.center)
                            }
                            _ => None,
                        })
                        .expect("an event badge above the shared time axis");
                    assert!(
                        badge.y > chart.get_rendered_price_rect().bottom(),
                        "the event rail must follow the lowest pane's time axis"
                    );
                }
            }
        }
    }

    /// A primary click on marks reports the host ID of the one painted on top, with its series
    /// and time, and selects nothing; marks without an ID are not reported.
    #[test]
    fn clicking_overlapping_marks_reports_the_top_one_and_selects_nothing() {
        let ctx = Context::default();
        let mut chart = test_frames::chart(&[95.0], &[105.0]);
        chart.config_mut().show_volume = false;
        test_frames::frame(&ctx, &mut chart, 0.0, vec![]);
        let time = chart.state.shown()[70].time;
        let mark = |id: &str| Marker::new(time).with_pos(MarkerPos::InBar).with_id(id);
        chart.set_markers(
            SeriesId::MAIN,
            vec![mark("first"), mark("top"), Marker::new(time)],
        );
        test_frames::frame(&ctx, &mut chart, 0.1, vec![]);
        test_frames::frame(&ctx, &mut chart, 0.2, vec![]);
        let pos = chart.clickable_marks.last().unwrap().rect.center();
        for pressed in [true, false] {
            test_frames::frame(
                &ctx,
                &mut chart,
                0.3,
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                }],
            );
        }
        let clicks: Vec<_> = chart
            .drain_events()
            .filter(|event| matches!(event, ChartEvent::MarkClick { .. }))
            .collect();
        assert_eq!(
            clicks,
            [ChartEvent::MarkClick {
                id: "top".into(),
                series: SeriesId::MAIN,
                time
            }]
        );
        assert_eq!(chart.selected_element(), None);
    }
}
