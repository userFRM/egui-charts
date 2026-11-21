//! Ideas panel: a composer and a browsing feed for ideas and short posts.
//!
//! The host owns accounts, storage and the network. It fills the feed with
//! [`add_page`](IdeasPanel::add_page), answers [`IdeasAction::Publish`] with
//! [`publish_succeeded`](IdeasPanel::publish_succeeded) or
//! [`publish_failed`](IdeasPanel::publish_failed) and opens what the user selects, or places it
//! on a chart. A control whose service the host did not enable with
//! [`set_publishing`](IdeasPanel::set_publishing) or [`set_browsing`](IdeasPanel::set_browsing)
//! is not shown. The draft survives a failed publish, and the panel needs no other part of the
//! library.

use crate::localization as l10n;
use crate::model::{PostContent, PostLink};
use crate::theme::ThemeExt;
use crate::ui_kit::buttons::Button;
use crate::ui_kit::status_message::{MessageKind, status_message};
use crate::ui_kit::{EmptyState, PanelHeader};
use chrono::{DateTime, Utc};
use egui::{RichText, ScrollArea, TextEdit, Ui};

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
}

impl Idea {
    /// An entry without a title, instrument, interval, attachment or tags, neutral and public:
    /// set them as known. A short post keeps the empty title.
    ///
    /// ```
    /// use chrono::Utc;
    /// use egui_charts::ui::ideas::{Idea, IdeaDirection, IdeaKind, IdeaPage};
    ///
    /// let mut idea = Idea::new("1", IdeaKind::Idea, "Breakout above the range", "analyst", Utc::now());
    /// idea.title = "Range breakout".into();
    /// idea.direction = IdeaDirection::Long;
    /// idea.tags = vec!["breakout".into()];
    /// let mut page = IdeaPage::default();
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
        }
    }
}

/// One page of the host's feed.
#[derive(Clone, Debug, PartialEq, Default)]
#[non_exhaustive]
pub struct IdeaPage {
    /// The entries, newest first.
    pub items: Vec<Idea>,
    /// The host's cursor for the next page, or `None` at the end of the feed.
    pub next: Option<String>,
}

/// What the user is composing. It stays in the panel until a publish succeeds.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct IdeaDraft {
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

    /// Shows the entries of `kind` in the feed, as the tabs above it do.
    pub fn set_feed_kind(&mut self, kind: IdeaKind) {
        self.filter = kind;
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
    /// first page or a refresh.
    pub fn add_page(&mut self, page: IdeaPage, replace: bool) {
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
        ui.add(
            TextEdit::singleline(&mut self.draft.title)
                .hint_text(l10n::text(ui.ctx(), "Title"))
                .desired_width(f32::INFINITY),
        );
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
        let ready = !self.draft.title.trim().is_empty()
            && !self.draft.body.trim().is_empty()
            && !self.draft.symbol.is_empty();
        let mut action = IdeasAction::None;
        if Button::new("Publish")
            .enabled(ready && !pending)
            .show(ui)
            .clicked()
        {
            self.publish = Publish::Pending;
            action = IdeasAction::Publish(self.draft.clone());
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
        let shown: Vec<_> = self
            .ideas
            .iter()
            .filter(|idea| idea.kind == self.filter)
            .collect();
        if shown.is_empty() {
            EmptyState::new("Nothing to show yet").show(ui);
        }
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
            });
        }
        if let Some(cursor) = &self.next
            && Button::new("Load more").show(ui).clicked()
        {
            action = IdeasAction::LoadMore(cursor.clone());
        }
        action
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
    #[test]
    fn publish_waits_for_a_title_and_a_text() {
        for (title, body, ready) in [
            ("", "Above the range", false),
            ("  ", "Above the range", false),
            ("Breakout", " ", false),
            ("Breakout", "Above the range", true),
        ] {
            let mut panel = IdeasPanel::new();
            panel.set_publishing(true);
            panel.set_context("AAPL", "1D");
            panel.draft.title = title.into();
            panel.draft.body = body.into();
            let expected = panel.draft().clone();
            assert_eq!(expected.direction, IdeaDirection::Neutral);
            assert!(expected.tags.is_empty());
            assert!(expected.public);
            let action = click(&mut panel, "Publish", None);
            if ready {
                assert_eq!(action, IdeasAction::Publish(expected));
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
                }],
                next: None,
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

    #[test]
    fn controls_of_an_unavailable_service_are_not_shown() {
        for (publishing, browsing) in [(true, false), (false, true), (false, false)] {
            let mut panel = IdeasPanel::new();
            panel.set_publishing(publishing);
            panel.set_browsing(browsing);
            panel.set_context("AAPL", "1D");
            let texts = texts(&mut panel);
            assert_eq!(texts.iter().any(|t| t == "Publish"), publishing);
            assert_eq!(texts.iter().any(|t| t == "Short posts"), browsing);
        }
    }
}
