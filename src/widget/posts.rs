//! Post and idea content that the host resolves from the links of drawings.

use super::{Chart, ChartEvent};
use crate::data::{FeedError, inbox::Inbox};
use crate::drawings::DrawingToolType;
use crate::model::PostContent;
use crate::services::Posts;
use std::sync::{Arc, Mutex, PoisonError};

pub(super) struct PostSource {
    source: Arc<dyn Posts>,
    inbox: Arc<Inbox>,
    /// Links asked for and not answered yet, by request id.
    pending: Vec<(u64, String)>,
    next_request: u64,
    wake: Arc<Mutex<egui::ViewportId>>,
}

impl Chart {
    /// Connects the host service that resolves the links of post and idea drawings. The chart
    /// asks once for each link whose content it lacks, gives the answer to every drawing with
    /// that link and saves it with them; it never fetches a link itself. Without a source, or
    /// after a failed answer, the card says the link could not be loaded; setting a source
    /// asks for those links again. Replacing or removing the source drops pending answers.
    pub fn set_post_source(&mut self, source: Option<Arc<dyn Posts>>) {
        self.post_source = source.map(|source| PostSource {
            source,
            inbox: Arc::default(),
            pending: Vec::new(),
            next_request: 0,
            wake: Arc::new(Mutex::new(egui::ViewportId::ROOT)),
        });
        for manager in std::iter::once(&mut self.drawings).chain(
            self.symbol_drawings
                .values_mut()
                .map(|(manager, _)| manager),
        ) {
            for drawing in &mut manager.drawings {
                if let Some(post) = &mut drawing.post {
                    post.failed = false;
                }
            }
        }
    }

    pub(super) fn pump_posts(&mut self, ctx: &egui::Context) {
        let Some(source) = &mut self.post_source else {
            for drawing in &mut self.drawings.drawings {
                if let Some(post) = &mut drawing.post {
                    post.failed |= post.content.is_none();
                }
            }
            return;
        };
        *source.wake.lock().unwrap_or_else(PoisonError::into_inner) = ctx.viewport_id();
        for (request, answer) in source.inbox.drain().answers {
            let Some(index) = source.pending.iter().position(|(id, _)| *id == request) else {
                continue;
            };
            let (_, link) = source.pending.swap_remove(index);
            let answer = *answer.downcast::<Result<PostContent, FeedError>>().unwrap();
            if self.drawings.resolve_link(&link, answer.ok().as_ref()) {
                // Saved like an edit, but never undone: undoing would only ask the host again.
                self.history.drawing_edits += 1;
            }
        }
        for drawing in &self.drawings.drawings {
            if !matches!(
                drawing.tool_type,
                DrawingToolType::Tweet | DrawingToolType::Idea
            ) {
                continue;
            }
            let Some(post) = &drawing.post else {
                continue;
            };
            if post.content.is_some()
                || post.failed
                || source.pending.iter().any(|(_, link)| link == &post.link)
            {
                continue;
            }
            let (ctx, wake) = (ctx.clone(), Arc::clone(&source.wake));
            source.inbox.set_waker(move || {
                let viewport = *wake.lock().unwrap_or_else(PoisonError::into_inner);
                ctx.request_repaint_of(viewport);
            });
            let id = source.next_request;
            source.next_request += 1;
            source.pending.push((id, post.link.clone()));
            source.source.resolve(&post.link, source.inbox.reply(id));
        }
    }

    pub(super) fn click_link(&mut self, ctx: &egui::Context, pos: egui::Pos2) {
        if self.drawings.active_tool.is_some() || self.drawings.eraser_mode {
            return;
        }
        if let Some(id) = self.drawings.hit_test(pos)
            && let Some(drawing) = self
                .drawings
                .drawings
                .iter()
                .find(|drawing| drawing.id == id)
            && let Some(post) = &drawing.post
            && drawing.open_row(ctx).is_some_and(|row| row.contains(pos))
        {
            self.host_events.push(ChartEvent::LinkClick {
                drawing: id,
                link: post.link.clone(),
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Reply;
    use crate::drawings::persistence::DrawingStorage;
    use crate::drawings::{ChartPoint, Drawing};
    use crate::model::PostLink;
    use crate::services::{Storage, StoreKind};
    use crate::widget::test_frames::{chart, frame, shapes};
    use chrono::{TimeZone, Utc};
    use egui::{Context, Event, Key, Modifiers, PointerButton, Pos2, Rect, Shape};
    use std::collections::HashMap;

    #[derive(Default)]
    struct Source(Mutex<Vec<(String, Reply<PostContent>)>>);

    impl Posts for Source {
        fn resolve(&self, link: &str, reply: Reply<PostContent>) {
            self.0.lock().unwrap().push((link.into(), reply));
        }
    }

    #[derive(Default)]
    struct Documents(Mutex<HashMap<String, String>>);

    impl Storage for Documents {
        fn list(&self, _: StoreKind, reply: Reply<Vec<String>>) {
            reply.send(Ok(Vec::new()));
        }
        fn load(&self, _: StoreKind, name: &str, reply: Reply<Option<String>>) {
            reply.send(Ok(self.0.lock().unwrap().get(name).cloned()));
        }
        fn save(&self, _: StoreKind, name: &str, document: String, reply: Reply<()>) {
            self.0.lock().unwrap().insert(name.into(), document);
            reply.send(Ok(()));
        }
        fn delete(&self, _: StoreKind, _: &str, reply: Reply<()>) {
            reply.send(Ok(()));
        }
    }

    fn content() -> PostContent {
        PostContent::new(
            "Ada",
            Utc.with_ymd_and_hms(2026, 10, 30, 9, 30, 0).unwrap(),
            "Rates hold",
        )
    }

    /// One request fills every matching card, including locked cards, and saves through both
    /// document storage and per-drawing sync. Reloading asks only for links that failed.
    #[test]
    fn links_resolve_once_and_their_content_is_saved() {
        let ctx = Context::default();
        let documents = Arc::new(Documents::default());
        let source = Arc::new(Source::default());
        let mut chart = chart(&[95.0], &[105.0]);
        chart.set_symbol("A");
        chart.set_storage(documents.clone());
        chart.set_post_source(Some(source.clone()));
        frame(&ctx, &mut chart, 0.0, vec![]);
        let (a, b) = ("https://example.invalid/a", "https://example.invalid/b");
        for (tool, link, locked) in [
            (DrawingToolType::Tweet, a, false),
            (DrawingToolType::Idea, a, true),
            (DrawingToolType::Tweet, b, false),
        ] {
            let mut drawing = Drawing::new(0, tool);
            drawing.chart_points.push(ChartPoint::new(40.0, 100.0));
            drawing.post = Some(PostLink::new(link));
            drawing.locked = locked;
            chart.user_action(|chart| chart.drawings_mut().add_drawing(drawing).unwrap());
        }
        frame(&ctx, &mut chart, 0.1, vec![]);
        let requests = std::mem::take(&mut *source.0.lock().unwrap());
        assert_eq!(
            requests
                .iter()
                .map(|(link, _)| link.as_str())
                .collect::<Vec<_>>(),
            [a, b]
        );
        chart.drawings_mut().set_sync_enabled(true);
        let edits = chart.history.drawing_edits;
        for (link, reply) in requests {
            if link == a {
                reply.send(Ok(content()));
            }
        }
        let painted = shapes(&ctx, &mut chart, 0.2, vec![]);
        assert!(
            painted
                .iter()
                .any(|shape| matches!(shape, Shape::Text(text) if text.galley.text() == "Ada"))
        );
        assert_eq!(chart.history.drawing_edits, edits + 1);
        assert_eq!(
            chart
                .drawings
                .pending_sync
                .lock()
                .unwrap()
                .iter()
                .map(|action| {
                    match action {
                        crate::drawings::manager::DrawingSyncAction::UpdateDrawing(id) => *id,
                        other => panic!("expected an update, got {other:?}"),
                    }
                })
                .collect::<Vec<_>>(),
            chart.drawings.drawings[..2]
                .iter()
                .map(|drawing| drawing.id)
                .collect::<Vec<_>>()
        );
        frame(&ctx, &mut chart, 0.3, vec![]);
        frame(&ctx, &mut chart, 0.4, vec![]);
        assert!(source.0.lock().unwrap().is_empty());
        for drawing in &chart.drawings.drawings[..2] {
            assert_eq!(drawing.post, Some(PostLink::resolved(a, content())));
        }
        let failed = chart.drawings.drawings[2].post.as_ref().unwrap();
        assert!(failed.failed && failed.content.is_none());
        let saved = documents.0.lock().unwrap()["A"].clone();
        let (stored, skipped) = DrawingStorage::from_json(&saved).unwrap();
        assert!(skipped.is_empty());
        for drawing in &stored.drawings[..2] {
            assert_eq!(drawing.drawing.post, Some(PostLink::resolved(a, content())));
        }
        assert_eq!(stored.drawings[2].drawing.post, Some(PostLink::new(b)));
        let new_source = Arc::new(Source::default());
        let mut restored = crate::widget::test_frames::chart(&[95.0], &[105.0]);
        restored.set_symbol("A");
        restored.set_storage(documents);
        restored.set_post_source(Some(new_source.clone()));
        let ctx = Context::default();
        for i in 0..3 {
            frame(&ctx, &mut restored, i as f64, vec![]);
        }
        assert_eq!(
            new_source
                .0
                .lock()
                .unwrap()
                .iter()
                .map(|(link, _)| link.as_str())
                .collect::<Vec<_>>(),
            [b]
        );
        assert_eq!(
            restored.drawings.drawings[0].post,
            Some(PostLink::resolved(a, content()))
        );
        let (_, obsolete) = new_source.0.lock().unwrap().pop().unwrap();
        restored.set_post_source(None);
        frame(&ctx, &mut restored, 3.0, vec![]);
        assert!(restored.drawings.drawings[2].post.as_ref().unwrap().failed);
        restored.set_post_source(Some(new_source.clone()));
        frame(&ctx, &mut restored, 4.0, vec![]);
        let (_, current) = new_source.0.lock().unwrap().pop().unwrap();
        obsolete.send(Ok(content()));
        frame(&ctx, &mut restored, 5.0, vec![]);
        assert!(
            restored.drawings.drawings[2]
                .post
                .as_ref()
                .unwrap()
                .content
                .is_none()
        );
        current.send(Err(FeedError::NotSupported));
        frame(&ctx, &mut restored, 6.0, vec![]);
        assert!(restored.drawings.drawings[2].post.as_ref().unwrap().failed);
        restored.set_post_source(Some(new_source.clone()));
        frame(&ctx, &mut restored, 7.0, vec![]);
        let (_, retry) = new_source.0.lock().unwrap().pop().unwrap();
        retry.send(Ok(content()));
        frame(&ctx, &mut restored, 8.0, vec![]);
        assert_eq!(
            restored.drawings.drawings[2].post,
            Some(PostLink::resolved(b, content()))
        );
    }

    fn button(pos: Pos2, pressed: bool) -> Event {
        Event::PointerButton {
            pos,
            button: PointerButton::Primary,
            pressed,
            modifiers: Modifiers::NONE,
        }
    }

    fn key(key: Key, modifiers: Modifiers) -> Event {
        Event::Key {
            key,
            physical_key: None,
            pressed: true,
            repeat: false,
            modifiers,
        }
    }

    /// Only one address, typed with a resolver attached, replaces hand-typed content with a link.
    #[test]
    fn typed_addresses_become_links_only_with_a_post_source() {
        let address = "https://example.invalid/p/1";
        for (attached, multiline) in [(true, false), (true, true), (false, false)] {
            let ctx = Context::default();
            let source = Arc::new(Source::default());
            let mut chart = chart(&[95.0], &[105.0]);
            if attached {
                chart.set_post_source(Some(source.clone()));
            }
            frame(&ctx, &mut chart, 0.0, vec![]);
            chart
                .drawings_mut()
                .set_active_tool(Some(DrawingToolType::Tweet));
            let pos = chart.get_rendered_price_rect().center();
            frame(&ctx, &mut chart, 0.1, vec![Event::PointerMoved(pos)]);
            frame(&ctx, &mut chart, 0.2, vec![button(pos, true)]);
            frame(&ctx, &mut chart, 0.3, vec![button(pos, false)]);
            let mut input = Vec::new();
            if multiline {
                input.extend([Event::Text("Ada".into()), key(Key::Enter, Modifiers::NONE)]);
            }
            input.push(Event::Text(address.into()));
            frame(&ctx, &mut chart, 0.4, input);
            frame(
                &ctx,
                &mut chart,
                0.5,
                vec![key(Key::Enter, Modifiers::COMMAND)],
            );
            let drawing = &chart.drawings.drawings[0];
            if attached && !multiline {
                assert_eq!(drawing.post, Some(PostLink::new(address)));
                assert_eq!(drawing.text, None);
                frame(&ctx, &mut chart, 0.6, vec![]);
                assert_eq!(source.0.lock().unwrap()[0].0, address);
            } else {
                assert_eq!(drawing.post, None);
                assert_eq!(
                    drawing.text,
                    Some(if multiline {
                        format!("Ada\n{address}")
                    } else {
                        address.into()
                    })
                );
                assert!(source.0.lock().unwrap().is_empty());
            }
        }
    }

    /// A resolved entry is placed intact without an editor, and only the Open row reports a
    /// link click, once for a double click and never while a drawing tool is active.
    #[test]
    fn a_placed_card_keeps_its_post_and_its_open_row_reports_it() {
        let ctx = Context::default();
        let mut chart = chart(&[95.0], &[105.0]);
        frame(&ctx, &mut chart, 0.0, vec![]);
        let mut content = content();
        content.title = "Breakout".into();
        let post = PostLink::resolved("https://example.invalid/a", content);
        chart
            .drawings_mut()
            .start_post_insertion(DrawingToolType::Idea, post.clone());
        let rect = chart.get_rendered_price_rect();
        let pos = rect.min + rect.size() * egui::vec2(0.2, 0.3);
        frame(&ctx, &mut chart, 0.1, vec![Event::PointerMoved(pos)]);
        frame(&ctx, &mut chart, 0.2, vec![button(pos, true)]);
        let painted = shapes(&ctx, &mut chart, 0.3, vec![button(pos, false)]);
        assert_eq!(chart.drawings.drawings.len(), 1);
        let drawing = &chart.drawings.drawings[0];
        assert_eq!(drawing.tool_type, DrawingToolType::Idea);
        assert_eq!(drawing.post, Some(post.clone()));
        let id = drawing.id;
        assert!(chart.drawings.text_editor.is_none());
        let text_pos = |label| {
            painted
                .iter()
                .find_map(|shape| match shape {
                    Shape::Text(text) if text.galley.text() == label => {
                        Some(Rect::from_min_size(text.pos, text.galley.size()).center())
                    }
                    _ => None,
                })
                .unwrap()
        };
        let (title, open) = (text_pos("Breakout"), text_pos("Open"));
        chart.drain_events().for_each(drop);
        let mut time = 1.0;
        let mut click = |chart: &mut Chart, pos| {
            time += 0.1;
            frame(
                &ctx,
                chart,
                time,
                vec![Event::PointerMoved(pos), button(pos, true)],
            );
            time += 0.1;
            frame(&ctx, chart, time, vec![button(pos, false)]);
            chart
                .drain_events()
                .filter(|event| matches!(event, ChartEvent::LinkClick { .. }))
                .collect::<Vec<_>>()
        };
        chart
            .drawings_mut()
            .set_active_tool(Some(DrawingToolType::TrendLine));
        assert!(click(&mut chart, open).is_empty());
        chart.drawings_mut().set_active_tool(None);
        assert!(click(&mut chart, title).is_empty());
        let expected = vec![ChartEvent::LinkClick {
            drawing: id,
            link: post.link,
        }];
        assert_eq!(click(&mut chart, open), expected);
        // Let the previous click expire before starting the double click.
        time += 1.0;
        frame(&ctx, &mut chart, time, vec![]);
        let mut events = Vec::new();
        for i in 1..=4 {
            frame(
                &ctx,
                &mut chart,
                time + i as f64 * 0.05,
                vec![Event::PointerMoved(open), button(open, i % 2 == 1)],
            );
            events.extend(
                chart
                    .drain_events()
                    .filter(|event| matches!(event, ChartEvent::LinkClick { .. })),
            );
        }
        assert_eq!(events, expected);
        assert!(chart.drawings.text_editor.is_some());
    }
}
