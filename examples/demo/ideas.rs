//! The demo's ideas: a composer and an in-memory feed for ideas and posted chart images.

use std::collections::HashSet;

use chrono::{TimeDelta, Utc};
use egui_charts::drawings::DrawingToolType;
use egui_charts::ui::ideas::{
    Idea, IdeaDirection, IdeaDraft, IdeaKind, IdeaPage, IdeaRequest, IdeaView, IdeasAction,
    IdeasPanel,
};
use egui_charts::ui::widget_bar::NotificationsAction;

use super::{DemoApp, ExportFormat, SidePanel};

/// Ideas and posts published in this session, newest first, then two seeded examples.
pub(super) struct DemoIdeas {
    pub(super) panel: IdeasPanel,
    feed: Vec<Idea>,
    /// The entries the user commented on.
    commented: HashSet<String>,
    next: u64,
    /// The entry shown in full, by its identifier.
    shown: Option<String>,
}

impl DemoIdeas {
    pub(super) fn new() -> Self {
        let mut panel = IdeasPanel::new();
        panel.set_publishing(true);
        panel.set_browsing(true);
        panel.set_reactions(true);
        let now = Utc::now();
        let mut idea = Idea::new(
            "demo-seed-idea",
            IdeaKind::Idea,
            "Price closed above the three-week range on rising volume.",
            "Demo analyst",
            now - TimeDelta::hours(3),
        );
        idea.title = "Range breakout".into();
        idea.direction = IdeaDirection::Long;
        idea.tags = vec!["breakout".into(), "INDEXA".into()];
        idea.symbol = "INDEXA".into();
        idea.interval = "1D".into();
        let mut video = Idea::new(
            "demo-seed-video",
            IdeaKind::Video,
            "A walkthrough of the first hour of the session.",
            "Demo analyst",
            now - TimeDelta::hours(5),
        );
        video.title = "Reading the open".into();
        video.symbol = "INDEXA".into();
        video.interval = "5min".into();
        let mut ideas = Self {
            panel,
            feed: vec![idea, video],
            commented: HashSet::new(),
            next: 1,
            shown: None,
        };
        ideas.panel.add_page(ideas.page(IdeaView::All), true);
        ideas
    }

    fn add(&mut self, idea: Idea) {
        self.feed.insert(0, idea);
        self.panel.add_page(self.page(self.panel.view()), true);
    }

    /// The whole feed of `view`, as one page.
    fn page(&self, view: IdeaView) -> IdeaPage {
        let mut page = IdeaPage::default();
        page.items = self
            .feed
            .iter()
            .filter(|idea| match view {
                IdeaView::Liked => idea.liked,
                IdeaView::Commented => self.commented.contains(&idea.id),
                IdeaView::Followed => idea.followed,
                _ => true,
            })
            .cloned()
            .collect();
        page.view = view;
        page
    }

    /// Applies `change` to the entry `id` and answers `request` at once.
    fn react(&mut self, id: &str, request: IdeaRequest, change: impl FnOnce(&mut Idea)) {
        match self.feed.iter_mut().find(|idea| idea.id == id) {
            Some(idea) => {
                change(idea);
                let idea = idea.clone();
                self.panel.request_succeeded(request, idea);
            }
            None => self.panel.request_failed(id, request, "Unknown idea"),
        }
    }
}

impl DemoApp {
    fn publish_idea(&mut self, ctx: &egui::Context, draft: &IdeaDraft) {
        let id = format!("demo-{}", self.ideas.next);
        let mut idea = Idea::new(&id, draft.kind, &draft.body, "You", Utc::now());
        idea.title = draft.title.clone();
        idea.symbol = draft.symbol.clone();
        idea.interval = draft.interval.clone();
        idea.direction = draft.direction;
        idea.tags = draft.tags.clone();
        idea.public = draft.public;
        if draft.attach_chart {
            match self.export_image_bytes(ctx, &Default::default(), ExportFormat::Png, false) {
                Ok(png) => {
                    let uri = format!("bytes://{id}.png");
                    ctx.include_bytes(uri.clone(), png);
                    idea.attachment = Some(uri);
                }
                Err(error) => {
                    self.ideas.panel.publish_failed(error.to_string());
                    return;
                }
            }
        }
        self.ideas.next += 1;
        self.ideas.add(idea);
        self.ideas.panel.publish_succeeded(&id, None);
        let title = if draft.kind == IdeaKind::ShortPost {
            "Post published".to_owned()
        } else {
            format!("Idea published: {}", draft.title)
        };
        self.notification_host.notify_post(title, &id);
    }

    pub(super) fn post_image(
        &mut self,
        ctx: &egui::Context,
        png: Vec<u8>,
        copy_link: bool,
    ) -> String {
        let id = format!("demo-{}", self.ideas.next);
        self.ideas.next += 1;
        let uri = format!("bytes://{id}.png");
        ctx.include_bytes(uri.clone(), png);
        let mut post = Idea::new(
            &id,
            IdeaKind::ShortPost,
            format!("{} {}", self.state.symbol, self.state.timeframe.as_str()),
            "You",
            Utc::now(),
        );
        post.symbol = self.state.symbol.clone();
        post.interval = self.state.timeframe.as_str().into();
        post.attachment = Some(uri.clone());
        self.ideas.add(post);
        self.notification_host.notify_post("Image posted", &id);
        if copy_link {
            // shortcut: the demo has no image host, so the copied link resolves only inside the demo; a host returns its own URL.
            ctx.copy_text(uri);
            "Image posted to Ideas; link copied".into()
        } else {
            "Image posted to Ideas".into()
        }
    }

    pub(super) fn handle_ideas(&mut self, ctx: &egui::Context, action: IdeasAction) {
        match action {
            IdeasAction::Publish(draft) => self.publish_idea(ctx, &draft),
            IdeasAction::OpenChart { symbol, interval } => {
                self.open_catalog_symbol(&symbol);
                if let Ok(timeframe) = interval.parse() {
                    self.switch_timeframe(timeframe);
                }
            }
            IdeasAction::Place(link) => self
                .chart
                .drawings_mut()
                .start_post_insertion(DrawingToolType::Idea, link),
            IdeasAction::Open(id) => self.ideas.shown = Some(id),
            IdeasAction::Like { id, like } => {
                self.ideas.react(&id, IdeaRequest::Like, |idea| {
                    if idea.liked != like {
                        idea.liked = like;
                        idea.likes = if like {
                            idea.likes.saturating_add(1)
                        } else {
                            idea.likes.saturating_sub(1)
                        };
                    }
                });
            }
            IdeasAction::Comment { id, .. } => {
                self.ideas.commented.insert(id.clone());
                self.ideas.react(&id, IdeaRequest::Comment, |idea| {
                    idea.comments = idea.comments.saturating_add(1);
                });
            }
            IdeasAction::Follow { id, follow } => {
                self.ideas
                    .react(&id, IdeaRequest::Follow, |idea| idea.followed = follow);
            }
            IdeasAction::ShowView(view) => {
                let page = self.ideas.page(view);
                self.ideas.panel.add_page(page, true);
            }
            _ => {}
        }
    }

    pub(super) fn handle_notification(&mut self, action: NotificationsAction) {
        match action {
            NotificationsAction::SelectSymbol(symbol) => self.open_catalog_symbol(&symbol),
            NotificationsAction::OpenPost(id) => {
                if let Some(post) = self.ideas.feed.iter().find(|post| post.id == id) {
                    self.ideas.panel.set_feed_kind(post.kind);
                }
                self.ideas.shown = Some(id);
                self.open_panel = Some(SidePanel::Ideas);
            }
            NotificationsAction::OpenRoom(room) => {
                self.chats.open_room(room);
                self.open_panel = Some(SidePanel::Chats);
            }
            NotificationsAction::OpenStory(id) => {
                if let Some((highlight, story)) = super::feeds::story(&id, self.live.now()) {
                    let (panel, side) = if highlight {
                        (&mut self.highlights, SidePanel::Highlights)
                    } else {
                        (&mut self.news, SidePanel::News)
                    };
                    panel.open_story(story);
                    self.open_panel = Some(side);
                }
            }
            _ => {}
        }
    }

    /// The feed entry chosen with Open, in full, until the user closes it.
    pub(super) fn show_idea(&mut self, ctx: &egui::Context) {
        let Some(id) = &self.ideas.shown else {
            return;
        };
        let Some(idea) = self.ideas.feed.iter().find(|idea| idea.id == *id) else {
            self.ideas.shown = None;
            return;
        };
        let title = match idea.kind {
            IdeaKind::ShortPost => "Short post",
            _ => "Idea",
        };
        let mut open = true;
        egui_charts::ui_kit::modal::Modal::new("idea_view", title)
            .width(480.0)
            .show(ctx, &mut open, |ui| {
                egui::ScrollArea::vertical().show(ui, |ui| {
                    if !idea.title.is_empty() {
                        ui.label(egui::RichText::new(&idea.title).strong());
                    }
                    ui.label(&idea.body);
                    ui.label(
                        egui::RichText::new(format!(
                            "{} \u{b7} {} \u{b7} {} {}",
                            idea.author,
                            idea.time.format("%Y-%m-%d %H:%M"),
                            idea.symbol,
                            idea.interval
                        ))
                        .color(ui.visuals().weak_text_color()),
                    );
                    if let Some(uri) = &idea.attachment {
                        ui.add(
                            egui::Image::new(uri.as_str())
                                .alt_text("Attached chart")
                                .max_width(ui.available_width()),
                        );
                    }
                });
            });
        if !open {
            self.ideas.shown = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::{ScreenshotTarget, TopToolbarAction};
    use egui_charts::services::{Notification, NotificationTarget};

    /// Every publishing entry reaches the feed and the corresponding notification opens it.
    #[test]
    fn publish_and_post_image_reach_the_ideas_panel_and_notifications() {
        let ctx = egui::Context::default();
        let mut app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx.clone()));
        assert!(app.top_toolbar.state.can_publish_ideas);
        assert!(app.top_toolbar.state.can_publish_image);
        app.handle_top_toolbar(&ctx, TopToolbarAction::Publish);
        assert!(app.open_panel == Some(SidePanel::Ideas));
        app.handle_top_toolbar(&ctx, TopToolbarAction::Publish);
        assert!(app.open_panel.is_none());
        // The snapshot shortcut runs Copy link, the entry the snapshot menu shows its keys on.
        app.handle_shortcut_action(&ctx, egui_charts::config::KeyboardAction::Snapshot);
        assert!(matches!(
            app.screenshot_requested,
            Some((ScreenshotTarget::Post { copy_link: true }, ..))
        ));
        for (action, expected) in [
            (TopToolbarAction::PostImage, false),
            (TopToolbarAction::CopyImageLink, true),
        ] {
            app.handle_top_toolbar(&ctx, action);
            assert!(
                matches!(app.screenshot_requested, Some((ScreenshotTarget::Post { copy_link }, ..)) if copy_link == expected)
            );
        }
        app.post_image(&ctx, vec![0], false);
        let post = &app.ideas.feed[0];
        assert_eq!(post.kind, IdeaKind::ShortPost);
        assert!(post.attachment.as_ref().unwrap().starts_with("bytes://"));
        let id = post.id.clone();
        let entries = app.notifications.poll(&ctx);
        assert_eq!(
            entries.last().unwrap().target,
            Some(NotificationTarget::Post(id.clone()))
        );
        app.open_panel = None;
        app.handle_notification(NotificationsAction::OpenPost(id.clone()));
        assert!(app.open_panel == Some(SidePanel::Ideas));
        assert_eq!(app.ideas.shown.as_deref(), Some(id.as_str()));
        // Opening an image notification selects Short posts rather than leaving an empty Ideas tab.
        let output = ctx.run_ui(Default::default(), |ui| {
            app.ideas.panel.show(ui);
        });
        let attached = output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains("Chart attached")));
        output.drop_without_applying_deltas();
        assert!(attached);
        // The Short posts tab publishes a short post; the Ideas tab publishes an idea.
        app.ideas.panel.set_feed_kind(IdeaKind::Idea);
        app.publish_idea(&ctx, &app.ideas.panel.draft().clone());
        assert_eq!(app.ideas.feed[0].kind, IdeaKind::Idea);
        let id = app.ideas.feed[0].id.clone();
        app.open_panel = None;
        app.handle_notification(NotificationsAction::OpenPost(id));
        assert!(app.open_panel == Some(SidePanel::Ideas));
        let symbol = crate::sampledata::DemoSymbol::catalog()
            .iter()
            .find(|spec| spec.symbol != app.state.symbol)
            .unwrap()
            .symbol;
        let timeframe = egui_charts::model::Timeframe::Day1;
        assert_ne!(app.state.timeframe, timeframe);
        app.handle_ideas(
            &ctx,
            IdeasAction::OpenChart {
                symbol: symbol.into(),
                interval: timeframe.as_str().into(),
            },
        );
        assert_eq!(app.state.symbol, symbol);
        assert_eq!(app.state.timeframe, timeframe);
        assert!(app.ideas.feed[0].attachment.is_some());
        let mut draft = app.ideas.panel.draft().clone();
        draft.attach_chart = false;
        app.publish_idea(&ctx, &draft);
        assert!(app.ideas.feed[0].attachment.is_none());
    }

    /// Guards the feed's and the notifications' open buttons in the running demo: Open room on a
    /// chat notification shows that room with its composer, and Open shows the chosen entry in
    /// full rather than the newest one.
    #[test]
    fn open_shows_the_feed_entry_in_full_and_open_room_the_room() {
        use egui::Role;
        use egui_kittest::kittest::{NodeT as _, Queryable as _};

        let mut h = super::super::tests::calendar_news_harness(SidePanel::Notifications);
        let mut entry = Notification::new(Utc::now(), "Message sent to Crypto");
        entry.target = Some(NotificationTarget::Room("Crypto".into()));
        h.state_mut().notification_host.publish(entry);
        h.run_steps(5);
        // Accessibility clicks: where an entry sits depends on what else the demo shows.
        h.get_by_role_and_label(Role::Button, "Open room")
            .click_accesskit();
        h.run_steps(5);
        assert!(h.state().open_panel == Some(SidePanel::Chats));
        h.get_by_role_and_label(Role::TextInput, "Message Crypto");

        let ctx = h.ctx.clone();
        let mut draft = h.state().ideas.panel.draft().clone();
        draft.title = "Retest of the weekly high".into();
        draft.body = "Buyers held the level twice before the close.".into();
        draft.symbol = "SAMPLE".into();
        draft.interval = "1m".into();
        draft.attach_chart = true;
        h.state_mut().publish_idea(&ctx, &draft);
        let idea = h.state().ideas.feed[0].clone();
        assert!(idea.attachment.is_some());
        // A newer short post, which the Ideas tab hides: Open must show the entry it belongs to.
        h.state_mut().post_image(&ctx, vec![0], false);
        h.state_mut().open_panel = Some(SidePanel::Ideas);
        h.run_steps(5);
        {
            let mut node = h.get_by_label("Retest of the weekly high");
            let open = loop {
                node = node.parent().expect("the entry has an Open link");
                if let Some(open) = node.query_all_by_role_and_label(Role::Link, "Open").next() {
                    break open;
                }
            };
            open.click_accesskit();
        }
        h.run_steps(5);
        let meta = format!(
            "You \u{b7} {} \u{b7} SAMPLE 1m",
            idea.time.format("%Y-%m-%d %H:%M")
        );
        // The modal's contents are not its accessibility children, so match them by where they sit.
        let dialog = h.get_by_role_and_label(Role::Dialog, "Idea").rect();
        let title = "Retest of the weekly high";
        let body = "Buyers held the level twice before the close.";
        for label in [title, body, meta.as_str()] {
            let mut nodes = h.query_all_by_label(label);
            assert!(
                nodes.any(|node| dialog.contains_rect(node.rect())),
                "{label}"
            );
        }
        let image = h.get_by_role_and_label(Role::Image, "Attached chart");
        assert!(dialog.contains_rect(image.rect()));
    }

    /// Guards the demo's answers to the feed controls: a like and the Liked view, a short post
    /// without a title, an idea's direction and tags, and the seeded video on its own tab.
    #[test]
    fn social_controls_reach_the_demo_feed() {
        use egui::Role;
        use egui_kittest::{Harness, kittest::Queryable};
        let ctx = egui::Context::default();
        let app = DemoApp::new(&eframe::CreationContext::_new_kittest(ctx));
        let mut h = Harness::builder()
            .with_size(egui::vec2(420.0, 1600.0))
            .with_context_setup(egui_extras::install_image_loaders)
            .build_ui_state(
                |ui, app: &mut Box<DemoApp>| {
                    let symbol = app.state.symbol.clone();
                    let interval = app.state.timeframe.as_str();
                    app.ideas.panel.set_context(symbol, interval);
                    let action = app.ideas.panel.show(ui);
                    app.handle_ideas(ui.ctx(), action);
                },
                app,
            );
        h.run();
        h.get_by_label("Range breakout");
        h.get_by_role_and_label(Role::Button, "Like").click();
        h.run();
        h.get_by_role_and_label(Role::Button, "Unlike");
        h.get_by_label("Likes: 1 \u{b7} Comments: 0");
        h.get_by_role_and_label(Role::Button, "Liked").click();
        h.run();
        h.get_by_label("Range breakout");
        h.get_by_role_and_label(Role::Button, "Videos").click();
        h.run();
        assert!(h.query_by_label("Reading the open").is_none());
        h.get_by_role_and_label(Role::Button, "All").click();
        h.run();
        h.get_by_label("Reading the open");

        let body = |h: &mut Harness<'_, Box<DemoApp>>, text: &str| {
            h.get_by(|node| node.placeholder() == Some("What is your idea?"))
                .focus();
            h.run();
            h.get_by(|node| node.placeholder() == Some("What is your idea?"))
                .type_text(text);
            h.run();
        };
        h.get_by_role_and_label(Role::Button, "Short posts").click();
        h.run();
        h.get_by_label("Attach chart image").click();
        body(&mut h, "Quick note");
        h.get_by_role_and_label(Role::Button, "Publish").click();
        h.run();
        let post = &h.state().ideas.feed[0];
        assert_eq!(
            (post.kind, post.title.as_str(), post.body.as_str()),
            (IdeaKind::ShortPost, "", "Quick note")
        );

        h.get_by_role_and_label(Role::Button, "Ideas").click();
        h.run();
        h.get_by(|node| node.placeholder() == Some("Title")).focus();
        h.run();
        h.get_by(|node| node.placeholder() == Some("Title"))
            .type_text("Higher low");
        body(&mut h, "Buyers stepped in again.");
        h.get_by_role_and_label(Role::Button, "Long").click();
        h.get_by(|node| node.placeholder() == Some("Tags, separated by commas"))
            .focus();
        h.run();
        h.get_by(|node| node.placeholder() == Some("Tags, separated by commas"))
            .type_text("trend");
        h.run();
        h.get_by_role_and_label(Role::Button, "Publish").click();
        h.run();
        let idea = &h.state().ideas.feed[0];
        assert_eq!(
            (
                idea.kind,
                idea.title.as_str(),
                idea.direction,
                idea.tags.clone()
            ),
            (
                IdeaKind::Idea,
                "Higher low",
                IdeaDirection::Long,
                vec!["trend".to_owned()]
            )
        );
    }
}
