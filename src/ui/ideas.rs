//! Ideas panel: a composer and a browsing feed for ideas and short posts.
//!
//! The host owns accounts, storage and the network. It fills the feed with
//! [`add_page`](IdeasPanel::add_page), answers [`IdeasAction::Publish`] with
//! [`publish_succeeded`](IdeasPanel::publish_succeeded) or
//! [`publish_failed`](IdeasPanel::publish_failed) and opens what the user selects, or places it
//! on a chart. A control whose service the host did not enable with
//! [`set_publishing`](IdeasPanel::set_publishing) or [`set_browsing`](IdeasPanel::set_browsing)
//! is not shown. The draft survives a failed publish, and the panel needs no other part of the
//! library. With [`set_reactions`](IdeasPanel::set_reactions) entries show their counts and ask
//! the host to like, comment on and follow them, and the feed offers the views the host pages.

use crate::localization as l10n;
use crate::model::{PostContent, PostLink};
use crate::theme::ThemeExt;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::status_message::{MessageKind, status_message};
use crate::ui_kit::{EmptyState, PanelHeader};
use chrono::{DateTime, Utc};
use egui::{RichText, ScrollArea, TextEdit, Ui};
use std::collections::HashMap;

/// What a feed entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum IdeaKind {
    /// A titled analysis of a symbol, usually with a chart attached.
    #[default]
    Idea,
    /// A short note without a title.
    ShortPost,
    /// A recorded walkthrough. Its [`attachment`](Idea::attachment) is the preview picture, and
    /// [`IdeasAction::Open`] asks the host to play it.
    Video,
}

/// Which way an idea expects the price to move.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum IdeaDirection {
    /// No call on the direction.
    #[default]
    Neutral,
    /// The price should rise.
    Long,
    /// The price should fall.
    Short,
}

/// An entry of the host's feed.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub struct Idea {
    /// The host's identifier of the entry.
    pub id: String,
    /// Idea or short post.
    pub kind: IdeaKind,
    /// The title; empty for a short post.
    pub title: String,
    /// The text.
    pub body: String,
    /// Who published it.
    pub author: String,
    /// When it was published.
    pub time: DateTime<Utc>,
    /// The instrument it is about.
    pub symbol: String,
    /// The chart interval it was made on, as the host names it.
    pub interval: String,
    /// The host's reference to the attached chart image, or for a video its preview picture, if
    /// any: a link or a key.
    pub attachment: Option<String>,
    /// Which way the idea expects the price to move; neutral by default and for posts.
    pub direction: IdeaDirection,
    /// The author's tags, in their order. Empty by default.
    pub tags: Vec<String>,
    /// Whether the entry is publicly discoverable. True by default; false shows it as private.
    pub public: bool,
    /// How many users liked it, as the host counts. 0 by default.
    pub likes: u64,
    /// How many comments it has, as the host counts. 0 by default.
    pub comments: u64,
    /// Whether the signed-in user likes it. False by default.
    pub liked: bool,
    /// Whether the signed-in user follows it, receiving its updates. False by default.
    pub followed: bool,
}

impl Idea {
    /// An entry without a title, instrument, interval, attachment or tags, neutral and public:
    /// set them as known. A short post keeps the empty title.
    ///
    /// ```
    /// use chrono::Utc;
    /// use egui_charts::ui::ideas::{Idea, IdeaDirection, IdeaKind, IdeaPage, IdeaView};
    ///
    /// let mut idea = Idea::new("1", IdeaKind::Idea, "Breakout above the range", "analyst", Utc::now());
    /// idea.title = "Range breakout".into();
    /// idea.direction = IdeaDirection::Long;
    /// idea.tags = vec!["breakout".into()];
    /// idea.likes = 12;
    /// let mut page = IdeaPage::default();
    /// page.view = IdeaView::Liked;
    /// page.items.push(idea);
    /// ```
    pub fn new(
        id: impl Into<String>,
        kind: IdeaKind,
        body: impl Into<String>,
        author: impl Into<String>,
        time: DateTime<Utc>,
    ) -> Self {
        Self {
            id: id.into(),
            kind,
            title: String::new(),
            body: body.into(),
            author: author.into(),
            time,
            symbol: String::new(),
            interval: String::new(),
            attachment: None,
            direction: IdeaDirection::Neutral,
            tags: Vec::new(),
            public: true,
            likes: 0,
            comments: 0,
            liked: false,
            followed: false,
        }
    }
}

/// A request about one feed entry that the host answers with
/// [`IdeasPanel::request_succeeded`] or [`IdeasPanel::request_failed`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum IdeaRequest {
    /// [`IdeasAction::Like`].
    Like,
    /// [`IdeasAction::Comment`].
    Comment,
    /// [`IdeasAction::Follow`].
    Follow,
}

/// Which entries the host lists in the feed.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum IdeaView {
    /// The host's feed.
    #[default]
    All,
    /// Entries the user liked.
    Liked,
    /// Entries the user commented on.
    Commented,
    /// Entries the user follows.
    Followed,
}

/// One page of the host's feed.
#[derive(Clone, Debug, PartialEq, Default)]
#[non_exhaustive]
pub struct IdeaPage {
    /// The entries, newest first.
    pub items: Vec<Idea>,
    /// The host's cursor for the next page, or `None` at the end of the feed.
    pub next: Option<String>,
    /// The view this page belongs to; a page for another view than the one shown is ignored.
    /// All by default.
    pub view: IdeaView,
}

/// What the user is composing. It stays in the panel until a publish succeeds.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct IdeaDraft {
    /// What is published: a short post when the feed shows the Short posts tab, otherwise an
    /// idea. A short post has an empty title.
    pub kind: IdeaKind,
    /// The title.
    pub title: String,
    /// The text.
    pub body: String,
    /// The instrument it is about; the host sets it from the chart.
    pub symbol: String,
    /// The chart interval, as the host names it.
    pub interval: String,
    /// Whether to attach an image of the chart; the host takes it with the chart's image export.
    pub attach_chart: bool,
    /// Which way the idea expects the price to move; neutral until the user picks one.
    pub direction: IdeaDirection,
    /// The tags the user typed, comma separated in the composer: trimmed, without empty ones,
    /// each once in the order typed. Empty by default.
    pub tags: Vec<String>,
    /// Whether the idea is publicly discoverable; when false, the host shares it only with
    /// people who have its link. True by default.
    pub public: bool,
}

/// What the user asked of the host.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum IdeasAction {
    /// Nothing was asked.
    None,
    /// Publish the draft, then answer with [`IdeasPanel::publish_succeeded`] or
    /// [`IdeasPanel::publish_failed`].
    Publish(IdeaDraft),
    /// Fetch the page after `cursor` and pass it to [`IdeasPanel::add_page`].
    LoadMore(String),
    /// Like the entry, or take the like back when `like` is false. Answer with
    /// [`IdeasPanel::request_succeeded`] or [`IdeasPanel::request_failed`] for
    /// [`IdeaRequest::Like`].
    Like {
        /// The entry's identifier.
        id: String,
        /// Whether the user now likes it.
        like: bool,
    },
    /// Post `text`, trimmed and not empty, as a comment on the entry. Answer as for
    /// [`IdeaRequest::Comment`].
    Comment {
        /// The entry's identifier.
        id: String,
        /// The comment.
        text: String,
    },
    /// Follow the entry, or stop following it when `follow` is false. Answer as for
    /// [`IdeaRequest::Follow`].
    Follow {
        /// The entry's identifier.
        id: String,
        /// Whether the user now follows it.
        follow: bool,
    },
    /// Replace the feed with the first page of this view: answer with [`IdeasPanel::add_page`]
    /// and `replace` set, the page's [`view`](IdeaPage::view) being this one. Load more continues
    /// the shown view, which [`IdeasPanel::view`] reports.
    ShowView(IdeaView),
    /// Open the entry with this identifier.
    Open(String),
    /// Show the symbol and interval of the entry on the chart.
    OpenChart {
        /// The instrument.
        symbol: String,
        /// The interval, as the host names it.
        interval: String,
    },
    /// Place the entry on the chart as an idea card: pass the link to
    /// [`DrawingManager::start_post_insertion`](crate::drawings::DrawingManager::start_post_insertion)
    /// with [`DrawingToolType::Idea`](crate::drawings::DrawingToolType::Idea), and the next
    /// click on the chart places it. The link is the entry's identifier, already resolved to its
    /// title, author, time, text and attachment; the attachment shows as the card's picture when
    /// the host's egui image loaders resolve it.
    Place(PostLink),
}

/// Where a publish stands.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
enum Publish {
    #[default]
    Idle,
    Pending,
    Done {
        id: String,
        link: Option<String>,
    },
    Failed(String),
}

/// The ideas panel.
#[derive(Debug)]
pub struct IdeasPanel {
    draft: IdeaDraft,
    tags: String,
    publish: Publish,
    ideas: Vec<Idea>,
    next: Option<String>,
    can_publish: bool,
    can_browse: bool,
    /// Which kind the feed shows.
    filter: IdeaKind,
    reactions: bool,
    view: IdeaView,
    /// True from a view switch until a page of that view arrives.
    loading: bool,
    /// The requests about entries waiting for the host, as `None`, or refused, with the reason.
    requests: HashMap<(String, IdeaRequest), Option<String>>,
    /// The entry whose comment field is open.
    commenting: Option<String>,
    comment: String,
}

impl Default for IdeasPanel {
    fn default() -> Self {
        Self::new()
    }
}

impl IdeasPanel {
    /// A panel with neither service enabled.
    pub fn new() -> Self {
        Self {
            draft: IdeaDraft {
                kind: IdeaKind::Idea,
                title: String::new(),
                body: String::new(),
                symbol: String::new(),
                interval: String::new(),
                attach_chart: true,
                direction: IdeaDirection::Neutral,
                tags: Vec::new(),
                public: true,
            },
            tags: String::new(),
            publish: Publish::Idle,
            ideas: Vec::new(),
            next: None,
            can_publish: false,
            can_browse: false,
            filter: IdeaKind::Idea,
            reactions: false,
            view: IdeaView::All,
            loading: false,
            requests: HashMap::new(),
            commenting: None,
            comment: String::new(),
        }
    }

    /// Shows the composer, or hides it when `available` is false.
    pub fn set_publishing(&mut self, available: bool) {
        self.can_publish = available;
    }

    /// Shows the feed, or hides it when `available` is false.
    pub fn set_browsing(&mut self, available: bool) {
        self.can_browse = available;
    }

    /// Shows the like, comment and follow controls, the counts and the view choice, or hides them
    /// when `available` is false. Hidden by default.
    pub fn set_reactions(&mut self, available: bool) {
        self.reactions = available;
    }

    /// The view the feed shows.
    pub fn view(&self) -> IdeaView {
        self.view
    }

    /// Shows the entries of `kind` in the feed, as the tabs above it do. On Short posts the
    /// composer writes a short post.
    pub fn set_feed_kind(&mut self, kind: IdeaKind) {
        self.filter = kind;
        self.draft.kind = draft_kind(kind);
    }

    /// Sets the instrument and interval the draft is about, normally those of the chart.
    pub fn set_context(&mut self, symbol: impl Into<String>, interval: impl Into<String>) {
        self.draft.symbol = symbol.into();
        self.draft.interval = interval.into();
    }

    /// The draft being composed.
    pub fn draft(&self) -> &IdeaDraft {
        &self.draft
    }

    /// Adds a page to the end of the feed, or replaces the feed when `replace` is set, as for the
    /// first page or a refresh. A page of another view than the one shown is ignored.
    pub fn add_page(&mut self, page: IdeaPage, replace: bool) {
        if page.view != self.view {
            return;
        }
        self.loading = false;
        if replace {
            self.ideas.clear();
        }
        self.ideas.extend(page.items);
        self.next = page.next;
    }

    /// Records that the host published the draft as `id`, which clears its title, text, tags
    /// and direction.
    pub fn publish_succeeded(&mut self, id: impl Into<String>, link: Option<String>) {
        self.draft.title.clear();
        self.draft.body.clear();
        self.tags.clear();
        self.draft.tags.clear();
        self.draft.direction = IdeaDirection::Neutral;
        self.publish = Publish::Done {
            id: id.into(),
            link,
        };
    }

    /// Records that publishing failed. The draft stays for another attempt.
    pub fn publish_failed(&mut self, reason: impl Into<String>) {
        self.publish = Publish::Failed(reason.into());
    }

    /// Records that the host carried out `request` for `idea`, whose counts and state replace the
    /// listed entry with its identifier. In the Liked or Followed view an entry the user no longer
    /// likes or follows leaves the list.
    pub fn request_succeeded(&mut self, request: IdeaRequest, idea: Idea) {
        self.requests.remove(&(idea.id.clone(), request));
        if request == IdeaRequest::Comment && self.commenting.as_ref() == Some(&idea.id) {
            self.commenting = None;
            self.comment.clear();
        }
        let gone = match self.view {
            IdeaView::Liked => !idea.liked,
            IdeaView::Followed => !idea.followed,
            _ => false,
        };
        if let Some(at) = self.ideas.iter().position(|listed| listed.id == idea.id) {
            if gone {
                self.ideas.remove(at);
            } else {
                self.ideas[at] = idea;
            }
        }
    }

    /// Records that the host could not carry out `request` for the entry `id`. The entry keeps its
    /// state, the reason shows under it with Dismiss, and a comment keeps its text. An answer to
    /// no pending request is ignored.
    pub fn request_failed(
        &mut self,
        id: impl Into<String>,
        request: IdeaRequest,
        reason: impl Into<String>,
    ) {
        if let Some(state @ None) = self.requests.get_mut(&(id.into(), request)) {
            *state = Some(reason.into());
        }
    }

    /// Shows the panel in `ui` and returns what the user asked of the host.
    pub fn show(&mut self, ui: &mut Ui) -> IdeasAction {
        let mut action = IdeasAction::None;
        PanelHeader::new("Ideas").show_plain(ui);
        if !self.can_publish && !self.can_browse {
            EmptyState::new("Ideas are not available")
                .fill_height()
                .show(ui);
            return action;
        }
        ScrollArea::vertical()
            .id_salt("ideas_scroll")
            .auto_shrink([false; 2])
            .show(ui, |ui| {
                if self.can_publish {
                    action = self.composer(ui);
                }
                if self.can_browse {
                    let browse = self.feed(ui);
                    if browse != IdeasAction::None {
                        action = browse;
                    }
                }
            });
        action
    }

    fn composer(&mut self, ui: &mut Ui) -> IdeasAction {
        let weak = ui.visuals().weak_text_color();
        self.draft.kind = draft_kind(self.filter);
        let post = self.draft.kind == IdeaKind::ShortPost;
        if !post {
            ui.add(
                TextEdit::singleline(&mut self.draft.title)
                    .hint_text(l10n::text(ui.ctx(), "Title"))
                    .desired_width(f32::INFINITY),
            );
        }
        ui.add(
            TextEdit::multiline(&mut self.draft.body)
                .hint_text(l10n::text(ui.ctx(), "What is your idea?"))
                .desired_rows(4)
                .desired_width(f32::INFINITY),
        );
        ui.label(
            RichText::new(format!("{} {}", self.draft.symbol, self.draft.interval)).color(weak),
        );
        ui.horizontal(|ui| {
            for (direction, label) in [
                (IdeaDirection::Long, "Long"),
                (IdeaDirection::Short, "Short"),
                (IdeaDirection::Neutral, "Neutral"),
            ] {
                ui.selectable_value(
                    &mut self.draft.direction,
                    direction,
                    l10n::text(ui.ctx(), label),
                );
            }
        });
        if ui
            .add(
                TextEdit::singleline(&mut self.tags)
                    .hint_text(l10n::text(ui.ctx(), "Tags, separated by commas"))
                    .desired_width(f32::INFINITY),
            )
            .changed()
        {
            self.draft.tags = parse_tags(&self.tags);
        }
        ui.horizontal(|ui| {
            for (public, label) in [(true, "Public"), (false, "Private")] {
                ui.selectable_value(&mut self.draft.public, public, l10n::text(ui.ctx(), label));
            }
        });
        ui.checkbox(
            &mut self.draft.attach_chart,
            l10n::text(ui.ctx(), "Attach chart image"),
        );
        let pending = self.publish == Publish::Pending;
        let ready = !self.draft.body.trim().is_empty()
            && (post || (!self.draft.title.trim().is_empty() && !self.draft.symbol.is_empty()));
        let mut action = IdeasAction::None;
        if Button::new("Publish")
            .enabled(ready && !pending)
            .show(ui)
            .clicked()
        {
            self.publish = Publish::Pending;
            let mut draft = self.draft.clone();
            if post {
                // The title typed on the Ideas tab stays in the panel for an idea.
                draft.title.clear();
            }
            action = IdeasAction::Publish(draft);
        }
        match &self.publish {
            Publish::Idle => {}
            Publish::Pending => {
                ui.label(RichText::new(l10n::text(ui.ctx(), "Publishing")).color(weak));
            }
            Publish::Done { id, link } => {
                let text = link.as_deref().unwrap_or(id);
                status_message(
                    ui,
                    MessageKind::Success,
                    &format!("{} {text}", l10n::text(ui.ctx(), "Published")),
                );
            }
            Publish::Failed(reason) => {
                status_message(ui, MessageKind::Error, reason);
            }
        }
        ui.separator();
        action
    }

    fn feed(&mut self, ui: &mut Ui) -> IdeasAction {
        let mut action = IdeasAction::None;
        let theme = ui.active_theme();
        let weak = ui.visuals().weak_text_color();
        ui.horizontal(|ui| {
            for (kind, label) in [
                (IdeaKind::Idea, "Ideas"),
                (IdeaKind::ShortPost, "Short posts"),
                (IdeaKind::Video, "Videos"),
            ] {
                ui.selectable_value(&mut self.filter, kind, l10n::text(ui.ctx(), label));
            }
        });
        if self.reactions {
            let mut view = self.view;
            ui.horizontal(|ui| {
                for (choice, label) in [
                    (IdeaView::All, "All"),
                    (IdeaView::Liked, "Liked"),
                    (IdeaView::Commented, "Commented"),
                    (IdeaView::Followed, "Followed"),
                ] {
                    ui.selectable_value(&mut view, choice, l10n::text(ui.ctx(), label));
                }
            });
            if view != self.view {
                self.view = view;
                self.ideas.clear();
                self.next = None;
                self.loading = true;
                action = IdeasAction::ShowView(view);
            }
        }
        let shown: Vec<_> = self
            .ideas
            .iter()
            .filter(|idea| idea.kind == self.filter)
            .collect();
        if shown.is_empty() {
            if self.loading {
                ui.label(RichText::new(l10n::text(ui.ctx(), "Loading")).color(weak));
            } else {
                EmptyState::new("Nothing to show yet").show(ui);
            }
        }
        // The request asked and the failure dismissed this frame, applied after the list.
        let mut asked = None;
        let mut dismissed = None;
        for idea in shown {
            ui.group(|ui| {
                ui.set_width(ui.available_width());
                if !idea.title.is_empty() {
                    ui.label(RichText::new(&idea.title).strong());
                }
                ui.label(&idea.body);
                let call = match idea.direction {
                    IdeaDirection::Long => Some(("Long", theme.ui.success)),
                    IdeaDirection::Short => Some(("Short", theme.ui.danger_text)),
                    _ => None,
                };
                if let Some((label, color)) = call {
                    ui.label(
                        RichText::new(l10n::text(ui.ctx(), label))
                            .strong()
                            .color(color),
                    );
                }
                if !idea.tags.is_empty() {
                    let tags: Vec<String> = idea
                        .tags
                        .iter()
                        .map(|tag| {
                            if tag.starts_with('#') {
                                tag.clone()
                            } else {
                                format!("#{tag}")
                            }
                        })
                        .collect();
                    ui.label(RichText::new(tags.join(" ")).color(weak));
                }
                let video = idea.kind == IdeaKind::Video;
                let mut meta = format!(
                    "{} \u{b7} {} \u{b7} {} {}",
                    idea.author,
                    idea.time.format("%Y-%m-%d %H:%M"),
                    idea.symbol,
                    idea.interval
                );
                if video {
                    meta += &format!(" \u{b7} {}", l10n::text(ui.ctx(), "Video"));
                } else if idea.attachment.is_some() {
                    meta += &format!(" \u{b7} {}", l10n::text(ui.ctx(), "Chart attached"));
                }
                if !idea.public {
                    meta += &format!(" \u{b7} {}", l10n::text(ui.ctx(), "Private"));
                }
                ui.label(RichText::new(meta).color(weak));
                if self.reactions {
                    let likes = l10n::format_text(
                        ui.ctx(),
                        "Likes: {count}",
                        &[("count", idea.likes.to_string())],
                    );
                    let comments = l10n::format_text(
                        ui.ctx(),
                        "Comments: {count}",
                        &[("count", idea.comments.to_string())],
                    );
                    ui.label(RichText::new(format!("{likes} \u{b7} {comments}")).color(weak));
                }
                ui.horizontal(|ui| {
                    let open = if video { "Watch video" } else { "Open" };
                    if ui.link(l10n::text(ui.ctx(), open)).clicked() {
                        action = IdeasAction::Open(idea.id.clone());
                    }
                    if ui.link(l10n::text(ui.ctx(), "Show on chart")).clicked() {
                        action = IdeasAction::OpenChart {
                            symbol: idea.symbol.clone(),
                            interval: idea.interval.clone(),
                        };
                    }
                    if ui.link(l10n::text(ui.ctx(), "Place on chart")).clicked() {
                        let mut content =
                            PostContent::new(idea.author.clone(), idea.time, idea.body.clone());
                        content.title = idea.title.clone();
                        content.picture = idea.attachment.clone();
                        action = IdeasAction::Place(PostLink::resolved(idea.id.clone(), content));
                    }
                });
                if !self.reactions {
                    return;
                }
                let pending = |request: IdeaRequest| {
                    matches!(self.requests.get(&(idea.id.clone(), request)), Some(None))
                };
                ui.horizontal(|ui| {
                    for (request, on, labels) in [
                        (
                            IdeaRequest::Like,
                            idea.liked,
                            ["Like", "Unlike", "Liking…", "Unliking…"],
                        ),
                        (
                            IdeaRequest::Follow,
                            idea.followed,
                            ["Follow", "Unfollow", "Following…", "Unfollowing…"],
                        ),
                    ] {
                        let waiting = pending(request);
                        let [ask, take_back, asking, taking_back] = labels;
                        let label = match (waiting, on) {
                            (false, false) => ask,
                            (false, true) => take_back,
                            (true, false) => asking,
                            (true, true) => taking_back,
                        };
                        if Button::new(label)
                            .small()
                            .enabled(!waiting)
                            .show(ui)
                            .clicked()
                        {
                            asked = Some((idea.id.clone(), request));
                            action = if request == IdeaRequest::Like {
                                IdeasAction::Like {
                                    id: idea.id.clone(),
                                    like: !on,
                                }
                            } else {
                                IdeasAction::Follow {
                                    id: idea.id.clone(),
                                    follow: !on,
                                }
                            };
                        }
                        if waiting {
                            ui.spinner();
                        }
                    }
                    if ui.link(l10n::text(ui.ctx(), "Comment")).clicked() {
                        self.commenting = Some(idea.id.clone());
                    }
                });
                if self.commenting.as_ref() == Some(&idea.id) {
                    let waiting = pending(IdeaRequest::Comment);
                    ui.horizontal(|ui| {
                        ui.add(
                            TextEdit::singleline(&mut self.comment)
                                .id_salt(("idea_comment", &idea.id))
                                .hint_text(l10n::text(ui.ctx(), "Write a comment")),
                        );
                        let label = if waiting { "Sending…" } else { "Send" };
                        if Button::new(label)
                            .small()
                            .enabled(!waiting && !self.comment.trim().is_empty())
                            .show(ui)
                            .clicked()
                        {
                            asked = Some((idea.id.clone(), IdeaRequest::Comment));
                            action = IdeasAction::Comment {
                                id: idea.id.clone(),
                                text: self.comment.trim().to_owned(),
                            };
                        }
                        if waiting {
                            ui.spinner();
                        }
                    });
                }
                for request in [IdeaRequest::Like, IdeaRequest::Comment, IdeaRequest::Follow] {
                    if let Some(Some(reason)) = self.requests.get(&(idea.id.clone(), request)) {
                        status_message(ui, MessageKind::Error, reason);
                        if Button::new("Dismiss").small().show(ui).clicked() {
                            dismissed = Some((idea.id.clone(), request));
                        }
                    }
                }
            });
        }
        if let Some(key) = asked {
            self.requests.insert(key, None);
        }
        if let Some(key) = dismissed {
            self.requests.remove(&key);
        }
        if let Some(cursor) = &self.next
            && Button::new("Load more").show(ui).clicked()
        {
            action = IdeasAction::LoadMore(cursor.clone());
        }
        action
    }
}

/// What the composer publishes while the feed shows `filter`: a short post on Short posts, an idea
/// on every other tab.
fn draft_kind(filter: IdeaKind) -> IdeaKind {
    if filter == IdeaKind::ShortPost {
        IdeaKind::ShortPost
    } else {
        IdeaKind::Idea
    }
}

/// The tags of a comma-separated list: trimmed, without empty ones, each once in the order typed.
fn parse_tags(text: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for tag in text.split(',').map(str::trim).filter(|tag| !tag.is_empty()) {
        if !tags.iter().any(|kept| kept == tag) {
            tags.push(tag.to_owned());
        }
    }
    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, Modifiers, PointerButton, RawInput, Rect, Shape};

    fn click(panel: &mut IdeasPanel, label: &str, typed: Option<&str>) -> IdeasAction {
        let ctx = egui::Context::default();
        let mut action = IdeasAction::None;
        let mut step = |events| {
            ctx.run_ui(
                RawInput {
                    screen_rect: Some(Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(800.0, 600.0),
                    )),
                    events,
                    ..Default::default()
                },
                |ui| {
                    action = panel.show(ui);
                },
            )
        };
        let output = step(vec![]);
        let pos = output.shapes.iter().find_map(|shape| match &shape.shape {
            Shape::Text(text) if text.galley.text() == label => {
                Some(Rect::from_min_size(text.pos, text.galley.size()).center())
            }
            _ => None,
        });
        output.drop_without_applying_deltas();
        let pos = pos.expect(label);
        step(vec![Event::PointerMoved(pos)]).drop_without_applying_deltas();
        for pressed in [true, false] {
            step(vec![Event::PointerButton {
                pos,
                button: PointerButton::Primary,
                pressed,
                modifiers: Modifiers::NONE,
            }])
            .drop_without_applying_deltas();
        }
        if let Some(text) = typed {
            step(vec![Event::Text(text.into())]).drop_without_applying_deltas();
        }
        action
    }

    /// The texts the panel paints in one frame, in paint order.
    fn texts(panel: &mut IdeasPanel) -> Vec<String> {
        let ctx = egui::Context::default();
        let output = ctx.run_ui(Default::default(), |ui| {
            panel.show(ui);
        });
        let texts = output
            .shapes
            .iter()
            .filter_map(|shape| match &shape.shape {
                Shape::Text(text) => Some(text.galley.text().to_owned()),
                _ => None,
            })
            .collect();
        output.drop_without_applying_deltas();
        texts
    }

    /// Whitespace is not a title or a description, and a disabled Publish keeps the draft idle.
    /// A short post needs only its text, and is sent without a title or symbol.
    #[test]
    fn publish_waits_for_a_title_and_a_text() {
        for (kind, title, body, symbol, ready) in [
            (IdeaKind::Idea, "", "Above the range", "AAPL", false),
            (IdeaKind::Idea, "  ", "Above the range", "AAPL", false),
            (IdeaKind::Idea, "Breakout", " ", "AAPL", false),
            (IdeaKind::Idea, "Breakout", "Above the range", "", false),
            (IdeaKind::Idea, "Breakout", "Above the range", "AAPL", true),
            (IdeaKind::ShortPost, "", "Quick note", "", true),
            (IdeaKind::ShortPost, "Ignored title", "Quick note", "AAPL", true),
            (IdeaKind::ShortPost, "", "  ", "AAPL", false),
        ] {
            let mut panel = IdeasPanel::new();
            panel.set_publishing(true);
            panel.set_browsing(true);
            panel.set_feed_kind(kind);
            panel.set_context(symbol, "1D");
            panel.draft.title = title.into();
            panel.draft.body = body.into();
            let mut expected = panel.draft().clone();
            assert_eq!(expected.kind, kind);
            assert_eq!(expected.direction, IdeaDirection::Neutral);
            assert!(expected.tags.is_empty());
            assert!(expected.public);
            if kind == IdeaKind::ShortPost {
                expected.title.clear();
            }
            let action = click(&mut panel, "Publish", None);
            if ready {
                assert_eq!(action, IdeasAction::Publish(expected));
                assert_eq!(panel.draft().title, title);
            } else {
                assert_eq!(action, IdeasAction::None, "title={title:?}, body={body:?}");
                assert_eq!(panel.publish, Publish::Idle);
            }
        }
    }

    /// The visible controls reach the host draft, with tags trimmed and deduplicated exactly.
    #[test]
    fn the_composer_sends_direction_tags_and_visibility() {
        let mut panel = IdeasPanel::new();
        panel.set_publishing(true);
        panel.set_context("AAPL", "1D");
        panel.draft.title = "Breakout".into();
        panel.draft.body = "Above the range".into();
        click(&mut panel, "Short", None);
        click(&mut panel, "Private", None);
        click(
            &mut panel,
            "Tags, separated by commas",
            Some(" btc, breakout,, BTC ,btc"),
        );
        let mut expected = panel.draft().clone();
        expected.direction = IdeaDirection::Short;
        expected.tags = vec!["btc".into(), "breakout".into(), "BTC".into()];
        expected.public = false;
        assert_eq!(
            click(&mut panel, "Publish", None),
            IdeasAction::Publish(expected)
        );
    }

    /// Placement carries the entry's identifier and all its content, including the picture.
    #[test]
    fn place_on_chart_reports_the_entry_as_a_resolved_link() {
        use chrono::TimeZone;
        let time = Utc.with_ymd_and_hms(2026, 10, 30, 9, 30, 0).unwrap();
        let mut panel = IdeasPanel::new();
        panel.set_browsing(true);
        panel.add_page(
            IdeaPage {
                items: vec![Idea {
                    id: "42".into(),
                    kind: IdeaKind::Idea,
                    title: "Breakout".into(),
                    body: "Above the range".into(),
                    author: "Ada".into(),
                    time,
                    symbol: "AAPL".into(),
                    interval: "1D".into(),
                    attachment: Some("https://example.invalid/42.png".into()),
                    direction: IdeaDirection::Neutral,
                    tags: Vec::new(),
                    public: true,
                    likes: 0,
                    comments: 0,
                    liked: false,
                    followed: false,
                }],
                next: None,
                view: IdeaView::All,
            },
            true,
        );
        let action = click(&mut panel, "Place on chart", None);
        let mut content = PostContent::new("Ada", time, "Above the range");
        content.title = "Breakout".into();
        content.picture = Some("https://example.invalid/42.png".into());
        assert_eq!(
            action,
            IdeasAction::Place(PostLink::resolved("42", content))
        );
    }

    /// Guards what an entry shows of its author's choices: the direction in words, the tags on
    /// one line, the private mark, and for a video its own tab, mark and Watch video link.
    #[test]
    fn entries_show_direction_tags_privacy_and_videos() {
        use chrono::TimeZone;
        let time = Utc.with_ymd_and_hms(2026, 10, 30, 9, 30, 0).unwrap();
        let mut long = Idea::new("a", IdeaKind::Idea, "Above the range", "Ada", time);
        long.direction = IdeaDirection::Long;
        long.tags = vec!["btc".into(), "#breakout".into()];
        long.public = false;
        let mut short = Idea::new("b", IdeaKind::Idea, "Below the range", "Bo", time);
        short.direction = IdeaDirection::Short;
        let mut video = Idea::new("v1", IdeaKind::Video, "Reading the open", "Cy", time);
        video.attachment = Some("https://example.invalid/v1.png".into());
        let mut panel = IdeasPanel::new();
        panel.set_browsing(true);
        panel.add_page(
            IdeaPage {
                items: vec![long, short, video],
                next: None,
                view: IdeaView::All,
            },
            true,
        );
        let shown = texts(&mut panel);
        assert!(shown.iter().any(|t| t == "Long"));
        assert!(shown.iter().any(|t| t == "Short"));
        assert_eq!(shown.iter().filter(|t| *t == "#btc #breakout").count(), 1);
        let meta = |author: &str| {
            shown
                .iter()
                .find(|t| t.starts_with(&format!("{author} \u{b7}")))
                .cloned()
                .unwrap()
        };
        assert!(meta("Ada").ends_with(" \u{b7} Private"));
        assert!(!meta("Bo").contains("Private"));
        assert!(!shown.iter().any(|t| t == "Reading the open"));

        click(&mut panel, "Videos", None);
        let shown = texts(&mut panel);
        let meta = shown.iter().find(|t| t.starts_with("Cy \u{b7}")).unwrap();
        assert!(meta.contains(" \u{b7} Video") && !meta.contains("Chart attached"));
        assert!(!shown.iter().any(|t| t == "Above the range"));
        assert_eq!(
            click(&mut panel, "Watch video", None),
            IdeasAction::Open("v1".into())
        );
    }

    /// Guards the reaction controls: each asks the host once and waits for its answer, a refusal
    /// shows with Dismiss and keeps the entry, the answer replaces the entry's counts and state, a
    /// comment keeps its text until it is posted, and a view loads its own pages, ignoring a late
    /// page of another view and dropping an entry the user no longer likes.
    #[test]
    fn reactions_ask_the_host_and_show_pending_failed_and_paged_views() {
        use chrono::TimeZone;
        let time = Utc.with_ymd_and_hms(2026, 10, 30, 9, 30, 0).unwrap();
        let entry = || Idea::new("1", IdeaKind::Idea, "Above the range", "Ada", time);
        let page = |view, items| {
            let mut page = IdeaPage::default();
            page.items = items;
            page.view = view;
            page
        };
        let panel = || {
            let mut panel = IdeasPanel::new();
            panel.set_browsing(true);
            panel.set_reactions(true);
            panel.add_page(page(IdeaView::All, vec![entry()]), true);
            panel
        };
        let shows = |panel: &mut IdeasPanel, text: &str| texts(panel).iter().any(|t| t == text);
        for (request, label, pending, done) in [
            (IdeaRequest::Like, "Like", "Liking…", "Unlike"),
            (IdeaRequest::Follow, "Follow", "Following…", "Unfollow"),
        ] {
            let mut panel = panel();
            let like = request == IdeaRequest::Like;
            let asked = if like {
                IdeasAction::Like {
                    id: "1".into(),
                    like: true,
                }
            } else {
                IdeasAction::Follow {
                    id: "1".into(),
                    follow: true,
                }
            };
            assert_eq!(click(&mut panel, label, None), asked);
            assert!(shows(&mut panel, pending));
            assert_eq!(click(&mut panel, pending, None), IdeasAction::None);
            panel.request_failed("1", request, "offline");
            assert!(shows(&mut panel, "offline"));
            click(&mut panel, "Dismiss", None);
            assert!(!shows(&mut panel, "offline") && shows(&mut panel, label));
            assert_eq!(click(&mut panel, label, None), asked);
            let mut answered = entry();
            if like {
                answered.likes = 1;
                answered.liked = true;
            } else {
                answered.followed = true;
            }
            panel.request_succeeded(request, answered);
            assert!(shows(&mut panel, done) && !shows(&mut panel, pending));
            let counts = format!("Likes: {} \u{b7} Comments: 0", u8::from(like));
            assert!(shows(&mut panel, &counts), "{counts}");
        }

        let mut panel = panel();
        click(&mut panel, "Comment", None);
        click(&mut panel, "Write a comment", Some("Nice "));
        let asked = IdeasAction::Comment {
            id: "1".into(),
            text: "Nice".into(),
        };
        assert_eq!(click(&mut panel, "Send", None), asked);
        assert!(shows(&mut panel, "Sending…"));
        panel.request_failed("1", IdeaRequest::Comment, "offline");
        assert_eq!(panel.comment, "Nice ");
        assert!(shows(&mut panel, "offline"));
        assert_eq!(click(&mut panel, "Send", None), asked);
        let mut answered = entry();
        answered.comments = 1;
        panel.request_succeeded(IdeaRequest::Comment, answered);
        assert!(panel.comment.is_empty() && panel.commenting.is_none());
        assert!(shows(&mut panel, "Likes: 0 \u{b7} Comments: 1"));

        let mut panel = panel();
        assert_eq!(
            click(&mut panel, "Liked", None),
            IdeasAction::ShowView(IdeaView::Liked)
        );
        assert_eq!(panel.view(), IdeaView::Liked);
        assert!(shows(&mut panel, "Loading") && !shows(&mut panel, "Above the range"));
        panel.add_page(page(IdeaView::All, vec![entry()]), false);
        assert!(shows(&mut panel, "Loading") && !shows(&mut panel, "Above the range"));
        let mut liked = entry();
        liked.liked = true;
        liked.likes = 1;
        panel.add_page(page(IdeaView::Liked, vec![liked.clone()]), true);
        assert!(shows(&mut panel, "Above the range") && !shows(&mut panel, "Loading"));
        liked.liked = false;
        liked.likes = 0;
        panel.request_succeeded(IdeaRequest::Like, liked);
        assert!(!shows(&mut panel, "Above the range"));
    }

    #[test]
    fn a_failed_publish_keeps_the_draft_and_a_successful_one_clears_it() {
        let mut panel = IdeasPanel::new();
        panel.set_context("AAPL", "1D");
        panel.draft.title = "Breakout".into();
        panel.draft.body = "Above the range".into();
        panel.draft.direction = IdeaDirection::Long;
        panel.tags = "btc".into();
        panel.draft.tags = vec!["btc".into()];
        panel.draft.public = false;
        let draft = panel.draft().clone();
        panel.publish_failed("offline");
        assert_eq!(panel.draft(), &draft);
        assert_eq!(panel.tags, "btc");
        assert_eq!(panel.draft().title, "Breakout");
        assert_eq!(panel.draft().body, "Above the range");
        panel.publish_succeeded("42", Some("https://example.invalid/42".into()));
        assert_eq!(
            (panel.draft().title.as_str(), panel.draft().body.as_str()),
            ("", "")
        );
        assert_eq!(panel.draft().symbol, "AAPL");
        assert_eq!(panel.draft().interval, "1D");
        assert!(panel.draft().attach_chart);
        assert!(panel.tags.is_empty());
        assert!(panel.draft().tags.is_empty());
        assert_eq!(panel.draft().direction, IdeaDirection::Neutral);
        assert!(!panel.draft().public);
    }

    /// Each control shows only once the host enables its service; reactions stay hidden on a listed
    /// entry until then.
    #[test]
    fn controls_of_an_unavailable_service_are_not_shown() {
        for (publishing, browsing) in [(true, false), (false, true), (false, false)] {
            let mut panel = IdeasPanel::new();
            panel.set_publishing(publishing);
            panel.set_browsing(browsing);
            panel.set_context("AAPL", "1D");
            let mut page = IdeaPage::default();
            page.items = vec![Idea::new("1", IdeaKind::Idea, "Text", "Ada", Utc::now())];
            panel.add_page(page, true);
            let texts = texts(&mut panel);
            assert_eq!(texts.iter().any(|t| t == "Publish"), publishing);
            assert_eq!(texts.iter().any(|t| t == "Short posts"), browsing);
            assert_eq!(texts.iter().any(|t| t == "Open"), browsing);
            for hidden in ["Like", "Comment", "Follow", "Liked"] {
                assert!(!texts.iter().any(|t| t == hidden), "{hidden}");
            }
            assert!(!texts.iter().any(|t| t.starts_with("Likes: ")));
        }
    }
}
