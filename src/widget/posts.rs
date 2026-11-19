//! Post and idea content that the host resolves from the links of drawings.

use super::{Chart, ChartEvent};
use crate::data::{FeedError, inbox::Inbox};
use crate::drawings::{ChartPoint, DrawingManager, DrawingToolType};
use crate::model::PostContent;
use crate::services::Posts;
use chrono::{DateTime, Utc};
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
    ///
    /// A card the user places stands on the bar of the time its content was published, at the
    /// price where it was placed: as it is placed when it carries its content, otherwise when
    /// its link resolves, unless the card was moved or locked first. A time before the loaded
    /// history keeps the card on that time until the bars arrive. Cards a host adds or loads
    /// keep their anchors.
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
        self.move_resolved_posts();
    }

    /// Anchors post or idea card `id`, which the user placed this frame, on the bar of the time
    /// its content was published, at the price where it was placed. A card whose content is not
    /// known yet is remembered, to move there when the content arrives.
    pub(super) fn place_new_post(&self, manager: &mut DrawingManager, id: usize) {
        let Some(drawing) = manager.drawings.iter_mut().find(|d| {
            d.id == id
                && !d.locked
                && matches!(d.tool_type, DrawingToolType::Tweet | DrawingToolType::Idea)
        }) else {
            return;
        };
        let Some(&anchor) = drawing.chart_points.first() else {
            return;
        };
        match drawing.post.as_ref().and_then(|post| post.content.as_ref()) {
            Some(content) => {
                drawing.chart_points[0] = self.publication_anchor(content.time, anchor.price);
            }
            None => manager
                .unplaced_posts
                .push((id, self.drawing_point_time(&anchor), anchor)),
        }
    }

    /// Moves each card placed before its content was known to the bar of the time the content
    /// was published, once it has arrived. A card that was removed, locked or moved meanwhile, or
    /// whose placement editor closed without a link, is forgotten and keeps its anchor.
    fn move_resolved_posts(&mut self) {
        let editing = self
            .drawings
            .text_editor
            .as_ref()
            .filter(|editor| editor.placed)
            .map(|editor| editor.id);
        let mut moves = Vec::new();
        let unplaced = std::mem::take(&mut self.drawings.unplaced_posts);
        let waiting = unplaced
            .into_iter()
            .filter(|&(id, time, placed)| {
                let Some((index, drawing)) = (self.drawings.drawings.iter().enumerate())
                    .find(|(_, d)| d.id == id && !d.locked)
                else {
                    return false;
                };
                let Some(anchor) = drawing.chart_points.first() else {
                    return false;
                };
                let unmoved = anchor.price == placed.price
                    && match time {
                        Some(time) => self.drawing_point_time(anchor) == Some(time),
                        None => anchor.bar_idx == placed.bar_idx,
                    };
                match drawing.post.as_ref().map(|post| post.content.as_ref()) {
                    _ if !unmoved => false,
                    // The editor opened at placement may still give the card a link.
                    None => editing == Some(id),
                    Some(None) => true,
                    Some(Some(content)) => {
                        moves.push((index, self.publication_anchor(content.time, anchor.price)));
                        false
                    }
                }
            })
            .collect();
        self.drawings.unplaced_posts = waiting;
        for (index, point) in moves {
            self.drawings.drawings[index].chart_points[0] = point;
        }
    }

    /// The anchor of a card published at `time`, at `price`: on the bar or price-based element
    /// whose period holds `time`, or on its slot in the whitespace after the last bar. A time
    /// before the first bar, or a chart without bars, keeps the card on `time` until the bars
    /// arrive, as [`Chart::create_drawing`] does.
    fn publication_anchor(&self, time: DateTime<Utc>, price: f64) -> ChartPoint {
        let index = crate::chart::rendering::candles::price_based::element_index_at(
            &self.state,
            self.chart_type,
            &self.chart_type_params,
            self.older_bars_drawn,
            time,
        )
        .map(|index| index as f64)
        .or_else(|| self.time_to_index(time));
        match index {
            Some(index) if index >= 0.0 && index.is_finite() => {
                ChartPoint::new(index.floor() as f32, price)
            }
            _ => self.anchor_at_time(time, price),
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
    use crate::model::{Bar, BarData, ChartType, ChartTypeParams, PostLink};
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
            // A card a host adds keeps its anchor.
            assert_eq!(drawing.chart_points[0].bar_idx, 40.0);
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
        // A card loaded from storage keeps its anchor.
        assert_eq!(restored.drawings.drawings[2].chart_points[0].bar_idx, 40.0);
    }

    /// A card the user places stands on the bar of its content's publication time, at the
    /// price where it was placed: at once when it is placed with its content, otherwise when its
    /// link resolves, within the undo step that placed or linked it. A time before the bars
    /// keeps the card on that time, and one after them on its slot. A card moved or locked
    /// before the answer keeps its anchor.
    #[test]
    fn placed_cards_move_to_the_time_their_content_was_published() {
        #[derive(Clone, Copy, PartialEq)]
        enum Placed {
            /// Tweet tool, click, address typed into the editor.
            Typed,
            /// `start_post_insertion` with a link to resolve, then a click.
            Linked,
            /// `start_post_insertion` with resolved content, then a click.
            Resolved,
        }
        #[derive(Clone, Copy, PartialEq)]
        enum Meanwhile {
            Nothing,
            Moved,
            Locked,
        }
        let bar = |i: i64| Utc.timestamp_opt(1_700_000_000 + i * 86_400, 0).unwrap();
        let hours = chrono::TimeDelta::hours;
        let link = "https://example.invalid/p/1";
        // Each row: how the card is placed, what happens before the answer, when its content
        // was published and the anchor time expected after the answer; `None` keeps the anchor
        // the card had before it.
        for (placed, meanwhile, published, expected) in [
            (
                Placed::Typed,
                Meanwhile::Nothing,
                bar(20) + hours(18),
                Some(bar(20)),
            ),
            (
                Placed::Linked,
                Meanwhile::Nothing,
                bar(20) + hours(18),
                Some(bar(20)),
            ),
            (
                Placed::Resolved,
                Meanwhile::Nothing,
                bar(20) + hours(18),
                Some(bar(20)),
            ),
            (
                Placed::Linked,
                Meanwhile::Nothing,
                bar(-3) - hours(6),
                Some(bar(-3) - hours(6)),
            ),
            (
                Placed::Linked,
                Meanwhile::Nothing,
                bar(101) + hours(1),
                Some(bar(101)),
            ),
            (Placed::Linked, Meanwhile::Moved, bar(20) + hours(18), None),
            (Placed::Linked, Meanwhile::Locked, bar(20) + hours(18), None),
        ] {
            let ctx = Context::default();
            let source = Arc::new(Source::default());
            let mut chart = chart(&[95.0], &[105.0]);
            chart.set_post_source(Some(source.clone()));
            frame(&ctx, &mut chart, 0.0, vec![]);
            let content = PostContent::new("Ada", published, "Rates hold");
            match placed {
                Placed::Typed => {
                    chart
                        .drawings_mut()
                        .set_active_tool(Some(DrawingToolType::Tweet));
                }
                Placed::Linked => chart
                    .drawings_mut()
                    .start_post_insertion(DrawingToolType::Idea, PostLink::new(link)),
                Placed::Resolved => chart.drawings_mut().start_post_insertion(
                    DrawingToolType::Tweet,
                    PostLink::resolved(link, content.clone()),
                ),
            }
            let rect = chart.get_rendered_price_rect();
            let pos = rect.min + rect.size() * egui::vec2(0.2, 0.3);
            frame(&ctx, &mut chart, 0.1, vec![Event::PointerMoved(pos)]);
            frame(&ctx, &mut chart, 0.2, vec![button(pos, true)]);
            frame(&ctx, &mut chart, 0.3, vec![button(pos, false)]);
            let id = chart.drawings.drawings[0].id;
            let price = chart.drawings.drawings[0].chart_points[0].price;
            if placed == Placed::Typed {
                frame(&ctx, &mut chart, 0.4, vec![Event::Text(link.into())]);
                frame(
                    &ctx,
                    &mut chart,
                    0.5,
                    vec![key(Key::Enter, Modifiers::COMMAND)],
                );
            }
            let before = chart.drawing_points(id).unwrap();
            match meanwhile {
                Meanwhile::Nothing => {}
                Meanwhile::Moved => chart.set_drawing_points(id, &[(bar(30), 100.0)]).unwrap(),
                Meanwhile::Locked => chart
                    .drawings_mut()
                    .edit_drawing(id, |drawing| drawing.locked = true)
                    .unwrap(),
            }
            let moved_or_locked = chart.drawing_points(id).unwrap();
            frame(&ctx, &mut chart, 0.6, vec![]);
            for (_, reply) in std::mem::take(&mut *source.0.lock().unwrap()) {
                reply.send(Ok(content.clone()));
            }
            frame(&ctx, &mut chart, 0.7, vec![]);
            frame(&ctx, &mut chart, 0.8, vec![]);
            let drawing = &chart.drawings.drawings[0];
            assert_eq!(
                drawing.post.as_ref().and_then(|post| post.content.as_ref()),
                Some(&content)
            );
            let anchor = chart.drawing_points(id).unwrap();
            match expected {
                Some(time) => assert_eq!(anchor, [(time, price)], "{published}"),
                None => assert_eq!(anchor, moved_or_locked, "{published}"),
            }
            if meanwhile != Meanwhile::Nothing {
                continue;
            }
            // Undoing the step that placed or linked the card takes the move with it.
            assert!(chart.undo());
            if placed == Placed::Typed {
                assert_eq!(chart.drawings.drawings[0].post, None);
                assert_eq!(chart.drawing_points(id).unwrap(), before);
            } else {
                assert!(chart.drawings.drawings.is_empty());
            }
            assert!(chart.redo());
            assert_eq!(chart.drawing_points(id).unwrap(), anchor);
        }
    }

    /// On a chart of price-based elements a card stands on the element whose period holds its
    /// publication time, the one completed next, as signals and restored drawings do.
    #[test]
    fn a_card_on_price_based_elements_stands_on_the_element_holding_its_time() {
        let minute = |i: i64| chrono::DateTime::UNIX_EPOCH + chrono::TimeDelta::minutes(i);
        let bars = (0..10)
            .map(|i| Bar::from_value(minute(i), 100.0 + 10.0 * i as f64))
            .collect();
        let mut chart = Chart::new(BarData::from_bars(bars));
        chart.set_chart_type(ChartType::Renko);
        chart.set_chart_type_params(ChartTypeParams {
            renko_brick_size: 10.0,
            ..Default::default()
        });
        let ctx = Context::default();
        frame(&ctx, &mut chart, 0.0, vec![]);
        let published = minute(2) + chrono::TimeDelta::seconds(30);
        let content = PostContent::new("Ada", published, "Rates hold");
        chart.drawings_mut().start_post_insertion(
            DrawingToolType::Tweet,
            PostLink::resolved("https://example.invalid/p/1", content),
        );
        let rect = chart.get_rendered_price_rect();
        let pos = rect.min + rect.size() * egui::vec2(0.2, 0.3);
        frame(&ctx, &mut chart, 0.1, vec![Event::PointerMoved(pos)]);
        frame(&ctx, &mut chart, 0.2, vec![button(pos, true)]);
        frame(&ctx, &mut chart, 0.3, vec![button(pos, false)]);
        let id = chart.drawings.drawings[0].id;
        // One brick completes each minute; the brick completed at minute 3 holds 00:02:30.
        assert_eq!(chart.drawing_points(id).unwrap()[0].0, minute(3));
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
        let rect = chart.get_rendered_price_rect();
        let pos = rect.min + rect.size() * egui::vec2(0.2, 0.3);
        let mut content = content();
        content.title = "Breakout".into();
        // Published at the clicked time, so the card stays in view where it is placed.
        content.time = chart.x_to_time(pos.x).unwrap();
        let post = PostLink::resolved("https://example.invalid/a", content);
        chart
            .drawings_mut()
            .start_post_insertion(DrawingToolType::Idea, post.clone());
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
