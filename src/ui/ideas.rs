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
}

/// An entry of the host's feed.
#[derive(Clone, Debug, PartialEq)]
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
    /// The host's reference to the attached chart image, if any: a link or a key.
    pub attachment: Option<String>,
}

/// One page of the host's feed.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct IdeaPage {
    /// The entries, newest first.
    pub items: Vec<Idea>,
    /// The host's cursor for the next page, or `None` at the end of the feed.
    pub next: Option<String>,
}

/// What the user is composing. It stays in the panel until a publish succeeds.
#[derive(Clone, Debug, PartialEq, Eq)]
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
pub struct IdeasPanel {
    draft: IdeaDraft,
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
            },
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

    /// Records that the host published the draft as `id`, which clears the draft.
    pub fn publish_succeeded(&mut self, id: impl Into<String>, link: Option<String>) {
        self.draft.title.clear();
        self.draft.body.clear();
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
        ui.checkbox(
            &mut self.draft.attach_chart,
            l10n::text(ui.ctx(), "Attach chart image"),
        );
        let pending = self.publish == Publish::Pending;
        let ready = !self.draft.body.trim().is_empty() && !self.draft.symbol.is_empty();
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
        let weak = ui.visuals().weak_text_color();
        ui.horizontal(|ui| {
            for (kind, label) in [
                (IdeaKind::Idea, "Ideas"),
                (IdeaKind::ShortPost, "Short posts"),
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
                let mut meta = format!(
                    "{} \u{b7} {} \u{b7} {} {}",
                    idea.author,
                    idea.time.format("%Y-%m-%d %H:%M"),
                    idea.symbol,
                    idea.interval
                );
                if idea.attachment.is_some() {
                    meta += &format!(" \u{b7} {}", l10n::text(ui.ctx(), "Chart attached"));
                }
                ui.label(RichText::new(meta).color(weak));
                ui.horizontal(|ui| {
                    if ui.link(l10n::text(ui.ctx(), "Open")).clicked() {
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Placement carries the entry's identifier and all its content, including the picture.
    #[test]
    fn place_on_chart_reports_the_entry_as_a_resolved_link() {
        use chrono::TimeZone;
        use egui::{Context, Event, Modifiers, PointerButton, RawInput, Rect, Shape};
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
                }],
                next: None,
            },
            true,
        );
        let ctx = Context::default();
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
        let pos = output
            .shapes
            .iter()
            .find_map(|shape| match &shape.shape {
                Shape::Text(text) if text.galley.text() == "Place on chart" => {
                    Some(Rect::from_min_size(text.pos, text.galley.size()).center())
                }
                _ => None,
            })
            .unwrap();
        output.drop_without_applying_deltas();
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
        let mut content = PostContent::new("Ada", time, "Above the range");
        content.title = "Breakout".into();
        content.picture = Some("https://example.invalid/42.png".into());
        assert_eq!(
            action,
            IdeasAction::Place(PostLink::resolved("42", content))
        );
    }

    #[test]
    fn a_failed_publish_keeps_the_draft_and_a_successful_one_clears_it() {
        let mut panel = IdeasPanel::new();
        panel.set_context("AAPL", "1D");
        panel.draft.title = "Breakout".into();
        panel.draft.body = "Above the range".into();
        panel.publish_failed("offline");
        assert_eq!(panel.draft().title, "Breakout");
        assert_eq!(panel.draft().body, "Above the range");
        panel.publish_succeeded("42", Some("https://example.invalid/42".into()));
        assert_eq!(
            (panel.draft().title.as_str(), panel.draft().body.as_str()),
            ("", "")
        );
        assert_eq!(panel.draft().symbol, "AAPL");
    }

    #[test]
    fn controls_of_an_unavailable_service_are_not_shown() {
        for (publishing, browsing) in [(true, false), (false, true), (false, false)] {
            let mut panel = IdeasPanel::new();
            panel.set_publishing(publishing);
            panel.set_browsing(browsing);
            panel.set_context("AAPL", "1D");
            let ctx = egui::Context::default();
            let output = ctx.run_ui(Default::default(), |ui| {
                panel.show(ui);
            });
            let texts: Vec<String> = output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::Shape::Text(text) => Some(text.galley.text().to_owned()),
                    _ => None,
                })
                .collect();
            output.drop_without_applying_deltas();
            assert_eq!(texts.iter().any(|t| t == "Publish"), publishing);
            assert_eq!(texts.iter().any(|t| t == "Short posts"), browsing);
        }
    }
}
