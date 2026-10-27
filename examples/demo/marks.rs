//! Synthetic annotations and scheduled events for the demo's own bars.

use super::{DemoApp, DemoSymbol};
use egui::{Color32, Vec2};
use egui_charts::data::Reply;
use egui_charts::drawings::FontWeight;
use egui_charts::model::{
    Bar, ChangeMarkers, EventKind, Marker, MarkerPos, MarkerShape, PriceLine, TimeScaleEvent,
};
use egui_charts::services::{Marks, MarksPage, MarksRequest};
use egui_charts::widget::{SeriesId, Watermark, WatermarkLine};
use std::sync::Arc;

const LEVEL: Color32 = Color32::from_rgb(255, 152, 0);

impl DemoApp {
    /// Annotates the bars on show with the time-axis events and, when the API samples are on, a
    /// fill and a stop marked at their prices, marks of the live changes of the close, signal bar
    /// marks, a watermark of the symbol and its company and the demo level at the recent high.
    pub(super) fn annotate(&mut self) {
        let samples = self.api_samples;
        self.refresh_strategy_signals();
        self.chart.set_mark_source(Some(Arc::new(DemoMarks {
            bars: self.bars.clone(),
            samples,
            notifications: self
                .chart
                .event_visibility()
                .show_news_notifications
                .then(|| self.notification_host.clone()),
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
                    .with_color(Color32::from_rgb(41, 98, 255))
                    .with_text("Fill")
                    .at_price(bar.open, MarkerPos::AtPriceMiddle),
            );
        }
        if let Some(bar) = at(12) {
            markers.push(
                Marker::new(bar.time)
                    .with_shape(MarkerShape::ArrowUp)
                    .with_color(Color32::from_rgb(242, 54, 69))
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

/// Time-axis events every 40 bars and, with `samples`, a signal mark on each of those bars.
pub(super) struct DemoMarks {
    pub bars: Vec<Bar>,
    pub samples: bool,
    pub notifications: Option<Arc<super::feeds::DemoNotifications>>,
}

impl Marks for DemoMarks {
    fn marks(&self, request: MarksRequest, reply: Reply<MarksPage>) {
        let mut page = MarksPage::default();
        for (index, bar) in
            self.bars.iter().enumerate().filter(|(i, bar)| {
                i % 40 == 0 && bar.time >= request.from && bar.time <= request.to
            })
        {
            let kind = match (index / 40) % 6 {
                0 => EventKind::Dividend,
                1 => EventKind::Split,
                2 => EventKind::Earnings,
                3 => EventKind::Economic,
                4 => EventKind::News,
                _ => EventKind::Idea,
            };
            page.events.push(TimeScaleEvent {
                id: format!("demo-event-{index}"),
                time: bar.time,
                kind,
                title: format!("Synthetic {} event", kind.label()),
                details: "Demonstration data supplied by the host.".into(),
            });
            if kind == EventKind::News
                && let Some(notifications) = &self.notifications
            {
                notifications.notify_event(&request.symbol, page.events.last().unwrap());
            }
            if self.samples {
                page.bar_marks.push(
                    Marker::buy_signal(bar.time)
                        .with_tooltip("Synthetic signal supplied by the demo host."),
                );
            }
        }
        reply.send(Ok(page));
    }
}
