//! Visible-range requests for host-owned annotations and events.

use super::Chart;
use crate::data::{FeedError, inbox::Inbox};
use crate::model::{EventVisibility, TimeScaleEvent};
use crate::services::{Marks, MarksPage, MarksRequest};
use std::sync::{Arc, Mutex, PoisonError};

pub(super) struct MarkSource {
    source: Arc<dyn Marks>,
    inbox: Arc<Inbox>,
    request: Option<MarksRequest>,
    wake: Arc<Mutex<egui::ViewportId>>,
    pub(super) page: MarksPage,
    error: Option<FeedError>,
}

impl Chart {
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
        self.time_scale_events = events;
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
        chart.set_mark_source(None);
        assert!(chart.mark_source.is_none());
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
            for selected in [None, Some(0), Some(1), Some(2), Some(3), Some(4), None] {
                let visibility = EventVisibility {
                    show_dividends: selected == Some(0),
                    show_splits: selected == Some(1),
                    show_earnings: selected == Some(2),
                    show_economic_events: selected == Some(3),
                    show_news: selected == Some(4),
                };
                #[cfg(feature = "ui")]
                {
                    use crate::ui::top_toolbar::components::settings_menu::ChartSettingsState;
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
                assert_eq!(chart.time_scale_events.len(), 5);
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
}
