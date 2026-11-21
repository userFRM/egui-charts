//! The demo's ideas: a composer and an in-memory feed for ideas and posted chart images.

use chrono::Utc;
use egui_charts::drawings::DrawingToolType;
use egui_charts::ui::ideas::{Idea, IdeaDraft, IdeaKind, IdeaPage, IdeasAction, IdeasPanel};
use egui_charts::ui::widget_bar::NotificationsAction;

use super::{DemoApp, SidePanel};

/// Ideas and posts published in this session, newest first.
pub(super) struct DemoIdeas {
    pub(super) panel: IdeasPanel,
    feed: Vec<Idea>,
    next: u64,
}

impl DemoIdeas {
    pub(super) fn new() -> Self {
        let mut panel = IdeasPanel::new();
        panel.set_publishing(true);
        panel.set_browsing(true);
        Self {
            panel,
            feed: Vec::new(),
            next: 1,
        }
    }

    fn add(&mut self, idea: Idea) {
        self.feed.insert(0, idea);
        let mut page = IdeaPage::default();
        page.items = self.feed.clone();
        self.panel.add_page(page, true);
    }
}

impl DemoApp {
    fn publish_idea(&mut self, draft: &IdeaDraft) {
        let id = format!("demo-{}", self.ideas.next);
        self.ideas.next += 1;
        let mut idea = Idea::new(&id, IdeaKind::Idea, &draft.body, "You", Utc::now());
        idea.title = draft.title.clone();
        idea.symbol = draft.symbol.clone();
        idea.interval = draft.interval.clone();
        self.ideas.add(idea);
        self.ideas.panel.publish_succeeded(&id, None);
        self.notification_host
            .notify_post(format!("Idea published: {}", draft.title), &id);
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

    pub(super) fn handle_ideas(&mut self, action: IdeasAction) {
        match action {
            IdeasAction::Publish(draft) => self.publish_idea(&draft),
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
                self.open_panel = Some(SidePanel::Ideas);
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::controller::{ScreenshotTarget, TopToolbarAction};
    use egui_charts::services::NotificationTarget;

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
        for (action, expected) in [
            (TopToolbarAction::PostImage, false),
            (TopToolbarAction::CopyImageLink, true),
        ] {
            app.handle_top_toolbar(&ctx, action);
            assert!(
                matches!(app.screenshot_requested, Some((ScreenshotTarget::Post { copy_link }, _)) if copy_link == expected)
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
        app.handle_notification(NotificationsAction::OpenPost(id));
        assert!(app.open_panel == Some(SidePanel::Ideas));
        // Opening an image notification selects Short posts rather than leaving an empty Ideas tab.
        let output = ctx.run_ui(Default::default(), |ui| {
            app.ideas.panel.show(ui);
        });
        let attached = output.shapes.iter().any(|shape| matches!(&shape.shape, egui::Shape::Text(text) if text.galley.text().contains("Chart attached")));
        output.drop_without_applying_deltas();
        assert!(attached);
        app.publish_idea(&app.ideas.panel.draft().clone());
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
        app.handle_ideas(IdeasAction::OpenChart {
            symbol: symbol.into(),
            interval: timeframe.as_str().into(),
        });
        assert_eq!(app.state.symbol, symbol);
        assert_eq!(app.state.timeframe, timeframe);
    }
}
