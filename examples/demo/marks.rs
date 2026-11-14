//! Synthetic annotations and scheduled events for the demo's own bars.

use super::{DemoApp, DemoSymbol, LiveFeed};
use chrono::{DateTime, Datelike, Utc};
use egui::{Color32, Vec2};
use egui_charts::data::Reply;
use egui_charts::drawings::FontWeight;
use egui_charts::model::{
    BarClock, ChangeMarkers, EventKind, Marker, MarkerPos, MarkerShape, PriceLine, TimeScaleEvent,
    Timeframe,
};
use egui_charts::services::{Marks, MarksPage, MarksRequest};
use egui_charts::widget::{SeriesId, Watermark, WatermarkLine};
use std::sync::{Arc, Mutex, PoisonError};

const LEVEL: Color32 = Color32::from_rgb(255, 152, 0);
const FILL: Color32 = Color32::from_rgb(41, 98, 255);
const STOP: Color32 = Color32::from_rgb(242, 54, 69);

impl DemoApp {
    /// Annotates the bars on show with the time-axis events and, when the API samples are on, a
    /// fill and a stop marked at their prices, marks of the live changes of the close, signal bar
    /// marks, a watermark of the symbol and its company and the demo level at the recent high.
    pub(super) fn annotate(&mut self) {
        if self.own_data.is_some() {
            return;
        }
        let samples = self.api_samples;
        self.refresh_strategy_signals();
        self.chart.set_mark_source(Some(Arc::new(DemoMarks {
            live: self.live.clone(),
            extended_hours: self.chart.extended_hours(),
            samples,
            notifications: self
                .chart
                .event_visibility()
                .show_news_notifications
                .then(|| self.notification_host.clone()),
            times: Default::default(),
        })));
        self.chart
            .set_change_markers(SeriesId::MAIN, samples.then(ChangeMarkers::default));
        let recent = &self.bars[self.bars.len().saturating_sub(30)..];
        let low = recent
            .iter()
            .map(|bar| bar.low)
            .fold(f64::INFINITY, f64::min);
        let high = recent
            .iter()
            .map(|bar| bar.high)
            .fold(f64::NEG_INFINITY, f64::max);
        let at = |back: usize| {
            let at = self.bars.len().checked_sub(back).filter(|_| samples)?;
            Some(&self.bars[at])
        };
        let mut markers = Vec::new();
        if let Some(bar) = at(30) {
            markers.push(
                Marker::new(bar.time)
                    .with_shape(MarkerShape::Diamond)
                    .with_color(FILL)
                    .with_text("Fill")
                    .at_price(bar.open, MarkerPos::AtPriceMiddle),
            );
        }
        if let Some(bar) = at(12) {
            markers.push(
                Marker::new(bar.time)
                    .with_shape(MarkerShape::ArrowUp)
                    .with_color(STOP)
                    .with_text("Stop")
                    .at_price(low, MarkerPos::AtPriceBottom),
            );
        }
        self.chart.set_markers(SeriesId::MAIN, markers);
        if !samples {
            self.chart.set_watermark(None);
            self.remove_demo_level();
            return;
        }
        let spec = DemoSymbol::find(&self.state.symbol);
        let gray = Color32::from_rgba_unmultiplied(120, 123, 134, 56);
        self.chart.set_watermark(Some(
            Watermark::default()
                .with_image(
                    egui_charts::icons::icons::CHART_CANDLES
                        .as_image_tinted(Vec2::splat(72.0), gray),
                )
                .with_line(
                    WatermarkLine::new(spec.symbol, 64.0, gray).with_weight(FontWeight::Bold),
                )
                .with_line(WatermarkLine::new(spec.company_name, 20.0, gray)),
        ));
        if high.is_finite() {
            self.set_demo_level(high);
        }
    }

    /// Moves the demo level to `price` through its handle, adding it when there is none.
    pub(super) fn set_demo_level(&mut self, price: f64) {
        let line = PriceLine::new(price, LEVEL)
            .with_title("Demo level")
            .with_axis_label_colors(LEVEL, Color32::BLACK);
        if !self
            .demo_level
            .is_some_and(|id| self.chart.update_price_line(id, line.clone()))
        {
            self.demo_level = self.chart.add_price_line(SeriesId::MAIN, line);
        }
    }

    /// Removes the demo level through its handle.
    pub(super) fn remove_demo_level(&mut self) {
        if let Some(id) = self.demo_level.take() {
            self.chart.remove_price_line(id);
        }
    }
}

/// Takes the fill, stop and demo level `annotate` adds out of a saved chart, so earlier saves do
/// not bring samples back into a session that did not turn them on.
pub(super) fn remove_samples(snapshot: &mut egui_charts::widget::ChartSnapshot) {
    let Some(saved) = &mut snapshot.main_annotations else {
        return;
    };
    saved.markers.retain(|marker| {
        let text = marker.text.as_deref();
        !(text == Some("Fill") && marker.color == FILL
            || text == Some("Stop") && marker.color == STOP)
    });
    saved
        .price_lines
        .retain(|line| !(line.title.as_deref() == Some("Demo level") && line.color == LEVEL));
}

/// Time-axis events at a stable cadence per interval, with symbol/time identities.
/// Requests read the current feed, including newly streamed and older daily bars.
pub(super) struct DemoMarks {
    pub live: Arc<LiveFeed>,
    pub extended_hours: bool,
    pub samples: bool,
    pub notifications: Option<Arc<super::feeds::DemoNotifications>>,
    pub times: Mutex<Option<BarTimes>>,
}

/// Bar times of a symbol and interval on their clock, kept until the next bar opens so pans do
/// not rebuild history.
type BarTimes = (
    &'static str,
    Timeframe,
    BarClock,
    DateTime<Utc>,
    Vec<DateTime<Utc>>,
);

impl Marks for DemoMarks {
    fn marks(&self, request: MarksRequest, reply: Reply<MarksPage>) {
        reply.send(Ok(self.annotations(&request)));
    }
}

impl DemoMarks {
    fn annotations(&self, request: &MarksRequest) -> MarksPage {
        let mut page = MarksPage::default();
        let Some(spec) = DemoSymbol::catalog()
            .iter()
            .find(|spec| spec.symbol.eq_ignore_ascii_case(&request.symbol))
        else {
            return page;
        };
        let now = self.live.now();
        let mut cache = self.times.lock().unwrap_or_else(PoisonError::into_inner);
        if !cache
            .as_ref()
            .is_some_and(|(symbol, timeframe, clock, slot, _)| {
                *symbol == spec.symbol
                    && *timeframe == request.timeframe
                    && clock.bar_open(now) == *slot
            })
        {
            let other = (self.live.instrument().symbol != spec.symbol).then(|| {
                let feed = self.live.fork();
                feed.set_symbol(spec.symbol);
                feed
            });
            let live = other.as_ref().unwrap_or(&self.live);
            let extended = self.extended_hours;
            let mut clock = BarClock::new(request.timeframe);
            if let Some(session) = live.instrument().session {
                clock = clock.with_session(crate::sampledata::us_equity_zone(), session, extended);
            }
            let times = live
                .snapshot(request.timeframe, clock.clone(), extended)
                .iter()
                .map(|bar| bar.time)
                .collect();
            *cache = Some((
                spec.symbol,
                request.timeframe,
                clock.clone(),
                clock.bar_open(now),
                times,
            ));
        }
        let times = &cache.as_ref().expect("cached above").4;
        let first = times.partition_point(|time| *time < request.from);
        let end = times.partition_point(|time| *time <= request.to.min(now));
        for &time in &times[first..end.max(first)] {
            let months = i64::from(time.year() - 1970) * 12 + i64::from(time.month0());
            let index = match request.timeframe {
                Timeframe::Month1 => months,
                Timeframe::Months(count) => months.div_euclid(i64::from(count.get())),
                Timeframe::Week1 => (time.timestamp().div_euclid(86_400) - 4).div_euclid(7),
                Timeframe::Weeks(count) => {
                    (time.timestamp().div_euclid(86_400) - 4).div_euclid(7 * i64::from(count.get()))
                }
                timeframe => time
                    .timestamp_millis()
                    .div_euclid(timeframe.duration_ms().max(1)),
            } - spec.seed as i64;
            if index.rem_euclid(40) != 0 {
                continue;
            }
            let kind = match index.div_euclid(40).rem_euclid(6) {
                0 => EventKind::Dividend,
                1 => EventKind::Split,
                2 => EventKind::Earnings,
                3 => EventKind::Economic,
                4 => EventKind::News,
                _ => EventKind::Idea,
            };
            page.events.push(TimeScaleEvent {
                id: format!("demo-event-{}-{}", spec.symbol, time.timestamp_millis()),
                time,
                kind,
                title: format!("Synthetic {} event", kind.label()),
                details: "Demonstration data supplied by the host.".into(),
            });
            if kind == EventKind::News
                && let Some(notifications) = &self.notifications
            {
                notifications.notify_event(spec.symbol, page.events.last().unwrap());
            }
            if self.samples {
                page.bar_marks.push(
                    Marker::buy_signal(time)
                        .with_tooltip("Synthetic signal supplied by the demo host."),
                );
            }
        }
        page
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeDelta;

    #[test]
    fn annotations_follow_live_time_and_paged_history_with_stable_ids() {
        let now = "2026-11-03T15:00:00Z".parse().unwrap();
        let live = Arc::new(LiveFeed::new("SAMPLE", now));
        super::super::tests::finish_feed(&live);
        let source = DemoMarks {
            live: live.clone(),
            extended_hours: false,
            samples: true,
            notifications: None,
            times: Default::default(),
        };
        let mut request = MarksRequest {
            symbol: "SAMPLE".into(),
            timeframe: Timeframe::Min1,
            from: now - TimeDelta::hours(2),
            to: now,
        };
        let initial = source.annotations(&request);
        assert!(!initial.events.is_empty());
        request.from = now + TimeDelta::minutes(1);
        request.to = now + TimeDelta::minutes(41);
        assert!(
            source.annotations(&request).events.is_empty(),
            "future events wait for synthetic time"
        );
        for minute in 1..=41 {
            live.advance(now + TimeDelta::minutes(minute));
        }
        let advanced = source.annotations(&request);
        assert!(
            !advanced.events.is_empty(),
            "the attached source must acquire new live events"
        );
        assert_eq!(advanced.events.len(), advanced.bar_marks.len());
        assert!(advanced.events.iter().all(|event| event.time > now));
        request.from = now - TimeDelta::hours(2);
        request.to = now;
        assert_eq!(
            source.annotations(&request).events,
            initial.events,
            "streaming must not shift existing event identities"
        );
        request.timeframe = Timeframe::Day1;
        request.from = now - TimeDelta::days(1000);
        request.to = now - TimeDelta::days(900);
        let older = source.annotations(&request);
        assert!(!older.events.is_empty());
        assert_eq!(source.annotations(&request).events, older.events);
        request.symbol = "BTCUSD".into();
        let other = source.annotations(&request);
        assert!(!other.events.is_empty());
        assert!(
            other
                .events
                .iter()
                .all(|event| event.id.starts_with("demo-event-BTCUSD-"))
        );
        request.symbol = "SAMPLE".into();
        assert_eq!(source.annotations(&request).events, older.events);
        assert_eq!(
            older
                .events
                .iter()
                .map(|event| &event.id)
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            older.events.len()
        );
    }
}
