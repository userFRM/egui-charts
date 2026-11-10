//! Visible-range requests for host-owned annotations and events.

use super::{Chart, ChartEvent, SeriesId};
use crate::chart::coords::ChartMapping;
use crate::chart::rendering::RenderContext;
use crate::data::{FeedError, inbox::Inbox};
use crate::model::{Bar, EventVisibility, Marker, MarkerZOrder, TimeScaleEvent};
use crate::services::{Marks, MarksPage, MarksRequest};
use chrono::{DateTime, Utc};
use std::sync::{Arc, Mutex, PoisonError};

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
        self.mark_hits.borrow_mut().extend(
            hits.into_iter()
                .filter(|(mark, _)| mark.id.is_some())
                .map(|(_, rect)| rect),
        );
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
        let title_hit = !stroke_hit
            && pos.y <= y
            && line.title.as_ref().is_some_and(|title| {
                let font = crate::chart::rendering::axes::axis_font(painter.ctx());
                // Only lay out a title the pointer can be over: its rows stand on the line, each
                // within a pixel of the font's row height once rounded to pixels.
                let row = painter.ctx().fonts_mut(|fonts| fonts.row_height(&font))
                    + painter.pixels_per_point().recip();
                if y - pos.y > row * (1 + title.matches('\n').count()) as f32 {
                    return false;
                }
                let galley = painter.layout_no_wrap(title.clone(), font, line.color);
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

    /// The last host mark painted at `pos` in the previous frame, for selection gating.
    /// Drawing tools and the eraser take the click first.
    pub(super) fn mark_at(&self, pos: egui::Pos2) -> Option<&egui::Rect> {
        #[cfg(feature = "drawings")]
        if self.drawings.active_tool.is_some() || self.drawings.eraser_mode {
            return None;
        }
        self.clickable_marks
            .iter()
            .rev()
            .find(|rect| rect.contains(pos))
    }

    /// Reports a click only for the host marker selected by this frame's paint order.
    pub(super) fn click_mark(&mut self, position: &super::ChartPointer) {
        #[cfg(feature = "drawings")]
        if self.drawings.active_tool.is_some() || self.drawings.eraser_mode {
            return;
        }
        if let Some(super::PointerTarget::Marker { series, id, time }) = &position.target {
            self.selection.deselect();
            #[cfg(feature = "drawings")]
            self.drawings.deselect();
            if let Some(id) = id {
                self.host_events.push(ChartEvent::MarkClick {
                    id: id.clone(),
                    series: *series,
                    time: *time,
                });
            }
        }
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
            // Every row of a title stands on its line, up to the top of the first.
            let font = crate::chart::rendering::axes::axis_font(&ctx);
            let row = ctx.fonts_mut(|fonts| fonts.row_height(&font));
            for (text, rows) in [("Level", 1.0), ("Two\nrows", 2.0)] {
                let titled = line.clone().with_width(0.0).with_title(text);
                assert!(chart.update_price_line(handle, titled));
                let top = egui::pos2(title.x, pos.y - rows * row + 1.0);
                assert_eq!(probe(&ctx, &mut chart, top, &mut tick).target, line_target);
            }
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

    /// A custom series' renderer covers the annotations painted before it, not those after it.
    #[test]
    fn renderer_under_annotation_keeps_the_annotation_target() {
        use crate::model::{LineStyle, MarkerShape, PriceLine};
        use crate::studies::{CustomIndicator, IndicatorValue};
        use crate::widget::{
            ChartElementId, PointerTarget, Primitive, PrimitivePass, PrimitiveView, SeriesOptions,
            VisualOrder,
        };
        use egui::{Color32, CursorIcon, Event, Painter, Pos2};

        const RENDER: Color32 = Color32::from_rgb(1, 2, 3);
        const MARK: Color32 = Color32::from_rgb(4, 5, 6);
        const LINE: Color32 = Color32::from_rgb(7, 8, 9);
        /// Paints a dot at bar 70, price 110 on the series pass and claims the pointer within 3 px of 110.
        struct Claim;
        impl Primitive for Claim {
            fn draw(&self, painter: &Painter, view: &PrimitiveView<'_>, pass: PrimitivePass) {
                if pass == PrimitivePass::Series {
                    let centre = Pos2::new(view.index_to_x(70.0), view.mapping.price_to_y(110.0));
                    painter.circle_filled(centre, 3.0, RENDER);
                }
            }
            fn hit_test(&self, pos: Pos2, view: &PrimitiveView<'_>) -> Option<CursorIcon> {
                (view.mapping.rect.contains(pos)
                    && (pos.y - view.mapping.price_to_y(110.0)).abs() <= 3.0)
                    .then_some(CursorIcon::PointingHand)
            }
        }
        let first = |shapes: &[Shape], color: Color32| {
            shapes.iter().position(|shape| match shape {
                Shape::Circle(circle) => circle.fill == color,
                Shape::LineSegment { stroke, .. } => stroke.color == color,
                _ => false,
            })
        };
        // 0: main-series marker over a renderer sent behind the main series. 1: added-series
        // price line over a renderer sent behind that series. 2: default order, renderer last.
        for row in 0..3 {
            let ctx = Context::default();
            let mut chart = test_frames::chart(&[95.0], &[105.0]);
            chart.config_mut().show_volume = false;
            let custom = chart.add_custom_series(
                Box::new(
                    CustomIndicator::new("Under", |bars| {
                        vec![IndicatorValue::Single(110.0); bars.len()]
                    })
                    .with_overlay(true),
                ),
                Claim,
            );
            let time = chart.bars(SeriesId::MAIN)[70].time;
            let (annotation, color) = if row == 0 {
                assert!(chart.reorder_indicator(custom, VisualOrder::SendToBack));
                chart.set_markers(
                    SeriesId::MAIN,
                    vec![
                        Marker::new(time)
                            .at_price(110.0, MarkerPos::AtPriceMiddle)
                            .with_shape(MarkerShape::Circle)
                            .with_color(MARK)
                            .with_id("over"),
                    ],
                );
                (
                    PointerTarget::Marker {
                        series: SeriesId::MAIN,
                        id: Some("over".into()),
                        time,
                    },
                    MARK,
                )
            } else {
                let series = chart.add_series(SeriesOptions::default());
                chart.set_data(series, chart.bars(SeriesId::MAIN).to_vec());
                if row == 1 {
                    assert!(chart.reorder_indicator(custom, VisualOrder::SendBackward));
                }
                let line = PriceLine::new(110.0, LINE)
                    .with_style(LineStyle::Solid)
                    .with_axis_label(false);
                let id = chart.add_price_line(series, line).unwrap();
                (PointerTarget::PriceLine { series, id }, LINE)
            };
            chart
                .series_price_scale_mut(SeriesId::MAIN)
                .unwrap()
                .set_range(90.0, 120.0);
            for tick in [0.0, 0.1, 0.2] {
                test_frames::frame(&ctx, &mut chart, tick, vec![]);
            }
            let pos = Pos2::new(
                chart.get_chart_mapping().idx_to_x(70),
                chart
                    .series_price_map(SeriesId::MAIN)
                    .unwrap()
                    .price_to_y(110.0),
            );
            test_frames::frame(&ctx, &mut chart, 0.3, vec![Event::PointerMoved(Pos2::ZERO)]);
            chart.drain_events().for_each(drop);
            let shapes = test_frames::shapes(&ctx, &mut chart, 0.4, vec![Event::PointerMoved(pos)]);
            let hover = chart.drain_events().find_map(|event| match event {
                ChartEvent::CrosshairMoved(Some(point)) => Some(point.target),
                _ => None,
            });
            // The expected target follows the frame's own paint order, not the code under test.
            let (renderer, painted) = (first(&shapes, RENDER), first(&shapes, color));
            assert!(renderer.is_some() && painted.is_some(), "row {row}");
            assert_eq!(painted > renderer, row < 2, "row {row}");
            let expected = Some(if row < 2 {
                annotation
            } else {
                PointerTarget::Element(ChartElementId::Indicator(custom))
            });
            assert_eq!(hover.as_ref(), Some(&expected), "row {row}: hover");
            for (tick, pressed) in [(0.5, true), (0.6, false)] {
                test_frames::frame(
                    &ctx,
                    &mut chart,
                    tick,
                    vec![Event::PointerButton {
                        pos,
                        button: egui::PointerButton::Primary,
                        pressed,
                        modifiers: Default::default(),
                    }],
                );
            }
            let click = chart.drain_events().find_map(|event| match event {
                ChartEvent::Click { position, .. } => Some(position.target),
                _ => None,
            });
            assert_eq!(click, Some(expected), "row {row}: click");
        }
    }

    /// A top marker covered by an above-pass primitive leaves the drawing as the target.
    #[cfg(feature = "drawings")]
    #[test]
    fn covered_top_marker_keeps_the_drawing_target() {
        use crate::model::MarkerOptions;
        use crate::widget::{
            PointerTarget, Primitive, PrimitiveHit, PrimitivePass, PrimitiveView, SeriesOptions,
        };
        use egui::{Event, Pos2};

        struct Foreground;
        impl Primitive for Foreground {
            fn hit_test_result(&self, pos: Pos2, view: &PrimitiveView<'_>) -> Option<PrimitiveHit> {
                (view.mapping.rect.contains(pos)
                    && (pos.y - view.mapping.price_to_y(100.0)).abs() <= 3.0)
                    .then(|| {
                        let mut hit = PrimitiveHit::new(egui::CursorIcon::PointingHand);
                        hit.pass = PrimitivePass::Above;
                        hit
                    })
            }
        }
        for secondary in [false, true] {
            let ctx = Context::default();
            let mut chart = test_frames::chart(&[95.0], &[105.0]);
            chart.config_mut().show_volume = false;
            let series = if secondary {
                let id = chart.add_series(SeriesOptions {
                    chart_type: crate::model::ChartType::Candles,
                    ..Default::default()
                });
                chart.set_data(id, chart.bars(SeriesId::MAIN).to_vec());
                let pane = chart.add_pane();
                assert!(chart.move_series_to_pane(id, pane));
                id
            } else {
                SeriesId::MAIN
            };
            chart.set_marker_options(
                series,
                MarkerOptions {
                    auto_scale: false,
                    z_order: MarkerZOrder::Top,
                },
            );
            let mut drawing = crate::drawings::Drawing::with_color(
                77,
                crate::drawings::DrawingToolType::HorizontalLine,
                [0, 0, 255, 255],
            );
            drawing.chart_points = vec![crate::drawings::ChartPoint::new(70.0, 100.0)];
            drawing.pane = secondary.then_some(series.0);
            chart.drawings_mut().drawings.push(drawing);
            assert!(chart.attach_primitive(series, Foreground).is_some());
            let time = chart.bars(series)[70].time;
            let mut tick = 0.0;
            test_frames::frame(&ctx, &mut chart, tick, vec![]);
            let pos = Pos2::new(
                chart.get_chart_mapping().idx_to_x(70),
                chart.series_price_map(series).unwrap().price_to_y(100.0),
            );
            for marked in [false, true] {
                let marks = if marked {
                    vec![
                        Marker::new(time)
                            .at_price(100.0, MarkerPos::AtPriceMiddle)
                            .with_id("top"),
                    ]
                } else {
                    Vec::new()
                };
                chart.set_markers(series, marks);
                tick += 0.1;
                test_frames::frame(
                    &ctx,
                    &mut chart,
                    tick,
                    vec![Event::PointerMoved(Pos2::ZERO)],
                );
                chart.drain_events().for_each(drop);
                tick += 0.1;
                test_frames::frame(&ctx, &mut chart, tick, vec![Event::PointerMoved(pos)]);
                let point = chart
                    .drain_events()
                    .find_map(|event| match event {
                        ChartEvent::CrosshairMoved(Some(point)) => Some(point),
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("pointer in plot at {pos:?}"));
                assert_eq!(
                    (point.target, point.primitive_hit),
                    (Some(PointerTarget::Drawing(77)), None),
                    "secondary {secondary}, marked {marked}"
                );
            }
        }
    }

    /// A chart without bars paints no marks, so a click there reports none, from its first
    /// empty frame on.
    #[test]
    fn click_on_an_emptied_chart_reports_no_removed_mark() {
        for settled in [false, true] {
            let ctx = Context::default();
            let mut chart = test_frames::chart(&[95.0], &[105.0]);
            chart.config_mut().show_volume = false;
            test_frames::frame(&ctx, &mut chart, 0.0, vec![]);
            let time = chart.state.shown()[70].time;
            chart.set_markers(
                SeriesId::MAIN,
                vec![Marker::new(time).with_pos(MarkerPos::InBar).with_id("old")],
            );
            test_frames::frame(&ctx, &mut chart, 0.1, vec![]);
            test_frames::frame(&ctx, &mut chart, 0.2, vec![]);
            let pos = chart.clickable_marks.last().unwrap().center();
            let button = |pressed| {
                vec![egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                }]
            };
            if !settled {
                test_frames::frame(&ctx, &mut chart, 0.3, button(true));
            }
            // The host drops the marks and the bars, as a symbol change does until new bars arrive.
            chart.set_markers(SeriesId::MAIN, Vec::new());
            chart.set_data(SeriesId::MAIN, Vec::new());
            if settled {
                test_frames::frame(&ctx, &mut chart, 0.3, vec![]);
                test_frames::frame(&ctx, &mut chart, 0.4, vec![]);
                chart.drain_events().for_each(drop);
                test_frames::frame(&ctx, &mut chart, 0.5, button(true));
            }
            test_frames::frame(&ctx, &mut chart, 0.6, button(false));
            let events: Vec<_> = chart
                .drain_events()
                .filter_map(|event| match event {
                    ChartEvent::Click { position, .. } => {
                        Some(format!("Click {:?}", position.target))
                    }
                    ChartEvent::MarkClick { id, .. } => Some(format!("MarkClick {id}")),
                    _ => None,
                })
                .collect();
            assert_eq!(events, ["Click None"], "settled {settled}");
        }
    }

    /// Hovering a pane makes no heap allocation for each titled price line away from the pointer.
    #[cfg(feature = "studies")]
    #[test]
    fn hover_allocates_nothing_per_titled_price_line() {
        use crate::model::PriceLine;
        use egui::{Color32, Event, Pos2, RawInput, Rect, vec2};

        let hover_cost = |titled: bool| {
            let ctx = Context::default();
            let mut chart = test_frames::chart(&[95.0], &[105.0]);
            chart.config_mut().show_volume = false;
            for i in 0..24 {
                let line = PriceLine::new(100.0 + f64::from(i) * 0.04, Color32::BLUE)
                    .with_axis_label(false);
                let line = if titled {
                    line.with_title(format!("Line {i}"))
                } else {
                    line
                };
                assert!(chart.add_price_line(SeriesId::MAIN, line).is_some());
            }
            test_frames::frame(&ctx, &mut chart, 0.0, vec![]);
            let plot = chart.get_rendered_price_rect();
            let map = chart.series_price_map(SeriesId::MAIN).unwrap();
            // On the time axis under the plot, then far below and far above every line and title.
            let at = |y| Pos2::new(plot.center().x, y);
            let positions = [
                at(plot.bottom() + 8.0),
                at(map.price_to_y(96.0)),
                at(map.price_to_y(104.0)),
            ];
            assert!(!plot.contains(positions[0]));
            assert!(plot.contains(positions[1]) && plot.contains(positions[2]));
            let mut tick = 0.0;
            positions.map(|pos| {
                for _ in 0..3 {
                    tick += 0.1;
                    test_frames::frame(&ctx, &mut chart, tick, vec![Event::PointerMoved(pos)]);
                }
                chart.drain_events().for_each(drop);
                (0..3)
                    .map(|_| {
                        tick += 0.1;
                        let input = RawInput {
                            screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))),
                            time: Some(tick),
                            ..Default::default()
                        };
                        let (count, output) =
                            crate::widget::studies_data::tests::allocations(|| {
                                ctx.run_ui(input, |ui| {
                                    egui::CentralPanel::default().show(ui, |ui| {
                                        chart.show(ui);
                                    });
                                })
                            });
                        output.drop_without_applying_deltas();
                        count as i64
                    })
                    .min()
                    .unwrap()
            })
        };
        let (plain, titled) = (hover_cost(false), hover_cost(true));
        // Titles do not change while the pointer moves, so they cost no more to hover than none.
        // Each count is taken against the same chart off the plot.
        for at in [1, 2] {
            assert!(
                (titled[at] - titled[0]) - (plain[at] - plain[0]) < 24,
                "hover allocations at {at}: untitled lines {plain:?}, titled lines {titled:?}"
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
        let hits = chart.mark_hits.borrow();
        assert_eq!(hits.len(), 6);
        let mapping = chart.get_chart_mapping();
        for series_hits in hits.chunks_exact(3) {
            for (rect, index) in series_hits.iter().zip([102.0, left + 0.25, 90.0]) {
                assert!(rect.x_range().contains(mapping.index_to_x(index)));
            }
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

    /// Covered marks cannot report a second click on the annotation underneath.
    #[test]
    fn click_on_primitive_over_marker_reports_no_mark() {
        use crate::widget::{
            PointerTarget, Primitive, PrimitiveHit, PrimitivePass, PrimitiveView, SeriesOptions,
        };
        struct Cover;
        impl Primitive for Cover {
            fn hit_test_result(
                &self,
                pos: egui::Pos2,
                view: &PrimitiveView<'_>,
            ) -> Option<PrimitiveHit> {
                (view.mapping.rect.contains(pos)
                    && (pos.y - view.mapping.price_to_y(100.0)).abs() < 3.0)
                    .then(|| {
                        let mut hit = PrimitiveHit::new(egui::CursorIcon::PointingHand);
                        hit.pass = PrimitivePass::Above;
                        hit
                    })
            }
        }
        for secondary in [false, true] {
            for cover in [
                "primitive",
                "drawing",
                "uncovered",
                "fresh",
                "top",
                "eraser",
            ] {
                if !cfg!(feature = "drawings") && matches!(cover, "drawing" | "top" | "eraser") {
                    continue;
                }
                let ctx = Context::default();
                let mut chart = test_frames::chart(&[95.0], &[105.0]);
                chart.config.show_volume = false;
                let id = if secondary {
                    let id = chart.add_series(SeriesOptions {
                        chart_type: crate::model::ChartType::Candles,
                        ..Default::default()
                    });
                    chart.set_data(id, chart.bars(SeriesId::MAIN).to_vec());
                    let pane = chart.add_pane();
                    chart.move_series_to_pane(id, pane);
                    id
                } else {
                    SeriesId::MAIN
                };
                #[cfg(feature = "drawings")]
                if cover == "eraser" {
                    chart.drawings_mut().eraser_mode = true;
                }
                let time = chart.bars(id)[70].time;
                chart.set_marker_options(
                    id,
                    crate::model::MarkerOptions {
                        auto_scale: false,
                        z_order: if cover == "top" {
                            MarkerZOrder::Top
                        } else {
                            MarkerZOrder::Normal
                        },
                    },
                );
                chart.set_markers(
                    id,
                    vec![
                        Marker::new(if cover == "fresh" {
                            chart.bars(id)[60].time
                        } else {
                            time
                        })
                        .at_price(100.0, MarkerPos::AtPriceMiddle)
                        .with_id("under"),
                    ],
                );
                let primitive =
                    (cover == "primitive").then(|| chart.attach_primitive(id, Cover).unwrap());
                #[cfg(feature = "drawings")]
                if matches!(cover, "drawing" | "top") {
                    let mut drawing = crate::drawings::Drawing::with_color(
                        77,
                        crate::drawings::DrawingToolType::HorizontalLine,
                        [0, 0, 255, 255],
                    );
                    drawing.chart_points = vec![crate::drawings::ChartPoint::new(70.0, 100.0)];
                    drawing.pane = secondary.then_some(id.0);
                    chart.drawings_mut().drawings.push(drawing);
                }
                test_frames::frame(&ctx, &mut chart, 0.0, vec![]);
                let pos = egui::pos2(
                    chart.get_chart_mapping().idx_to_x(70),
                    chart.series_price_map(id).unwrap().price_to_y(100.0),
                );
                test_frames::frame(&ctx, &mut chart, 0.1, vec![egui::Event::PointerMoved(pos)]);
                if cover == "fresh" {
                    chart.set_markers(
                        id,
                        vec![
                            Marker::new(time)
                                .at_price(100.0, MarkerPos::AtPriceMiddle)
                                .with_id("under"),
                        ],
                    );
                }
                chart.drain_events().for_each(drop);
                let click = |pressed| egui::Event::PointerButton {
                    pos,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: Default::default(),
                };
                test_frames::frame(&ctx, &mut chart, 0.2, vec![click(true), click(false)]);
                let events: Vec<_> = chart.drain_events().collect();
                let target = events
                    .iter()
                    .find_map(|event| match event {
                        ChartEvent::Click { position, .. } => position.target.clone(),
                        _ => None,
                    })
                    .unwrap_or_else(|| panic!("secondary={secondary}, cover={cover}: {events:?}"));
                let covered = matches!(cover, "primitive" | "drawing" | "eraser");
                if let Some(primitive) = primitive {
                    assert_eq!(target, PointerTarget::Primitive(primitive));
                }
                if cover == "drawing" {
                    assert_eq!(target, PointerTarget::Drawing(77));
                }
                if !covered {
                    assert!(matches!(target, PointerTarget::Marker { .. }));
                }
                let marks: Vec<_> = events
                    .iter()
                    .filter(|event| matches!(event, ChartEvent::MarkClick { .. }))
                    .collect();
                assert_eq!(
                    marks.len(),
                    usize::from(!covered),
                    "secondary={secondary}, cover={cover}: {events:?}"
                );
                if cover != "eraser" {
                    assert_eq!(
                        chart.selected_element(),
                        None,
                        "secondary={secondary}, cover={cover}"
                    );
                }
                #[cfg(feature = "drawings")]
                if cover == "top" {
                    assert_eq!(chart.drawings.selected_ids().count(), 0);
                }
            }
        }
    }

    /// A primary click reports only the top mark's host ID, if it has one, and selects nothing.
    #[test]
    fn clicking_overlapping_marks_reports_the_top_one_and_selects_nothing() {
        for unnamed_on_top in [false, true] {
            let ctx = Context::default();
            let mut chart = test_frames::chart(&[95.0], &[105.0]);
            chart.config_mut().show_volume = false;
            test_frames::frame(&ctx, &mut chart, 0.0, vec![]);
            let time = chart.state.shown()[70].time;
            let mark = |id: &str| Marker::new(time).with_pos(MarkerPos::InBar).with_id(id);
            let unnamed = || Marker::new(time).with_pos(MarkerPos::InBar);
            chart.set_markers(
                SeriesId::MAIN,
                if unnamed_on_top {
                    vec![mark("first"), mark("top"), unnamed()]
                } else {
                    vec![unnamed(), mark("first"), mark("top")]
                },
            );
            test_frames::frame(&ctx, &mut chart, 0.1, vec![]);
            test_frames::frame(&ctx, &mut chart, 0.2, vec![]);
            let pos = chart.clickable_marks.last().unwrap().center();
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
            let expected = if unnamed_on_top {
                Vec::new()
            } else {
                vec![ChartEvent::MarkClick {
                    id: "top".into(),
                    series: SeriesId::MAIN,
                    time,
                }]
            };
            assert_eq!(clicks, expected);
            assert_eq!(chart.selected_element(), None);
        }
    }
}
