//! Synthetic annotations and scheduled events for the demo's own bars.

use egui_charts::data::Reply;
use egui_charts::model::{Bar, EventKind, Marker, TimeScaleEvent};
use egui_charts::services::{Marks, MarksPage, MarksRequest};

pub(super) struct DemoMarks(pub Vec<Bar>);

impl Marks for DemoMarks {
    fn marks(&self, request: MarksRequest, reply: Reply<MarksPage>) {
        let mut page = MarksPage::default();
        for (index, bar) in
            self.0.iter().enumerate().filter(|(i, bar)| {
                i % 40 == 0 && bar.time >= request.from && bar.time <= request.to
            })
        {
            let kind = match (index / 40) % 5 {
                0 => EventKind::Dividend,
                1 => EventKind::Split,
                2 => EventKind::Earnings,
                3 => EventKind::Economic,
                _ => EventKind::News,
            };
            page.events.push(TimeScaleEvent {
                id: format!("demo-event-{index}"),
                time: bar.time,
                kind,
                title: format!("Synthetic {} event", kind.label()),
                details: "Demonstration data supplied by the host.".into(),
            });
            page.bar_marks.push(
                Marker::buy_signal(bar.time)
                    .with_tooltip("Synthetic signal supplied by the demo host."),
            );
        }
        reply.send(Ok(page));
    }
}
