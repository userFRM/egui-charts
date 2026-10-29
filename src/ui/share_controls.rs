//! The copy-link and save-a-copy rows of the layout menu and the watchlist panel, driven by the
//! host's [`Sharing`] service.

use crate::data::FeedError;
use crate::data::inbox::Inbox;
use crate::localization as l10n;
use crate::services::{ShareKind, ShareLink, Sharing};
use egui::Ui;
use std::sync::Arc;

/// An operation on the document `name`.
#[derive(Clone, Debug, PartialEq)]
enum Operation {
    Link(String),
    Copy(String),
}

/// What the last operation came to, for the document it was made on.
enum Outcome {
    Link(ShareLink),
    Copy(String),
    Failed(FeedError),
}

/// Rows offering the operations a [`Sharing`] service reports, with the state of the one in
/// progress and the result of the last.
#[derive(Default)]
pub(crate) struct ShareControls {
    sharing: Option<Arc<dyn Sharing>>,
    inbox: Arc<Inbox>,
    pending: Option<Operation>,
    last: Option<(Operation, Outcome)>,
}

impl ShareControls {
    pub(crate) fn set_service(&mut self, sharing: Arc<dyn Sharing>) {
        self.inbox = Arc::default();
        self.pending = None;
        self.last = None;
        self.sharing = Some(sharing);
    }

    /// Takes the host's answer, copying a link to the clipboard. Returns the document a saved
    /// copy was made of and the copy's name.
    pub(crate) fn poll(&mut self, ctx: &egui::Context) -> Option<(String, String)> {
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        let mut copied = None;
        for (_, answer) in self.inbox.drain().answers {
            let Some(operation) = self.pending.take() else {
                continue;
            };
            let outcome = match &operation {
                Operation::Link(_) => answer
                    .downcast::<Result<ShareLink, FeedError>>()
                    .map_or_else(
                        |_| Outcome::Failed(FeedError::Other("answer of another request".into())),
                        |answer| match *answer {
                            Ok(link) => {
                                ctx.copy_text(link.url.clone());
                                Outcome::Link(link)
                            }
                            Err(error) => Outcome::Failed(error),
                        },
                    ),
                Operation::Copy(name) => {
                    answer.downcast::<Result<String, FeedError>>().map_or_else(
                        |_| Outcome::Failed(FeedError::Other("answer of another request".into())),
                        |answer| match *answer {
                            Ok(copy) => {
                                copied = Some((name.clone(), copy.clone()));
                                Outcome::Copy(copy)
                            }
                            Err(error) => Outcome::Failed(error),
                        },
                    )
                }
            };
            self.last = Some((operation, outcome));
        }
        copied
    }

    /// Shows the rows the service offers for the document `name`, then the progress or result of
    /// the last operation on it. Shows nothing when the service offers nothing for `kind`.
    pub(crate) fn show(&mut self, ui: &mut Ui, kind: ShareKind, name: &str) {
        let Some(sharing) = self.sharing.clone() else {
            return;
        };
        let offer = sharing.offers(kind);
        if !offer.link && !offer.copy {
            return;
        }
        ui.separator();
        let idle = self.pending.is_none();
        if offer.link
            && ui
                .add_enabled(idle, egui::Button::new(l10n::text(ui.ctx(), "Copy link")))
                .clicked()
        {
            self.pending = Some(Operation::Link(name.to_owned()));
            sharing.link(kind, name, self.inbox.reply(0));
            ui.ctx().request_repaint();
        }
        if offer.copy
            && ui
                .add_enabled(
                    self.pending.is_none(),
                    egui::Button::new(l10n::text(ui.ctx(), "Save a copy")),
                )
                .clicked()
        {
            self.pending = Some(Operation::Copy(name.to_owned()));
            sharing.copy(kind, name, self.inbox.reply(0));
            ui.ctx().request_repaint();
        }
        let status = if let Some(pending) = &self.pending {
            Some((
                false,
                l10n::text(
                    ui.ctx(),
                    if matches!(pending, Operation::Link(_)) {
                        "Getting the link…"
                    } else {
                        "Saving a copy…"
                    },
                ),
            ))
        } else {
            self.last.as_ref().and_then(|(operation, outcome)| {
                let (Operation::Link(shared) | Operation::Copy(shared)) = operation;
                (shared == name).then(|| match outcome {
                    Outcome::Link(link) => (
                        false,
                        l10n::text(
                            ui.ctx(),
                            if link.copy_allowed {
                                "Link copied. Recipients can view it and save a copy."
                            } else {
                                "Link copied. Recipients can only view it."
                            },
                        ),
                    ),
                    Outcome::Copy(copy) => (
                        false,
                        l10n::format_text(ui.ctx(), "Saved as {0}", &[("0", copy.clone())]),
                    ),
                    Outcome::Failed(error) => (
                        true,
                        l10n::format_text(
                            ui.ctx(),
                            "Could not share: {0}",
                            &[("0", error.to_string())],
                        ),
                    ),
                })
            })
        };
        if let Some((error, text)) = status {
            if error {
                ui.colored_label(ui.visuals().error_fg_color, text);
            } else {
                ui.weak(text);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Reply;
    use crate::services::ShareOffer;

    /// Shares layouts as view-only links that cannot be copied, and rejects watchlists.
    struct Host;

    impl Sharing for Host {
        fn offers(&self, _: ShareKind) -> ShareOffer {
            ShareOffer::new(true, true)
        }
        fn link(&self, kind: ShareKind, name: &str, reply: Reply<ShareLink>) {
            reply.send(match kind {
                ShareKind::Layout => Ok(ShareLink::new(format!("https://host/{name}"), false)),
                _ => Err(FeedError::Other("not allowed".into())),
            });
        }
        fn copy(&self, _: ShareKind, name: &str, reply: Reply<String>) {
            reply.send(Ok(format!("{name} copy")));
        }
    }

    /// A link reaches the clipboard with the host's copy policy, a copy names the document it
    /// came from, and a rejection is kept for the menu to show.
    #[test]
    fn answers_become_clipboard_text_copies_and_errors() {
        let ctx = egui::Context::default();
        let host: Arc<dyn Sharing> = Arc::new(Host);
        let mut controls = ShareControls::default();
        controls.set_service(host.clone());
        let run = |controls: &mut ShareControls, operation: Operation, kind| {
            controls.pending = Some(operation.clone());
            match operation {
                Operation::Link(ref name) => host.link(kind, name, controls.inbox.reply(0)),
                Operation::Copy(ref name) => host.copy(kind, name, controls.inbox.reply(0)),
            }
            let mut copied = None;
            let output = ctx.run_ui(Default::default(), |ui| copied = controls.poll(ui.ctx()));
            let text = output.platform_output.commands.iter().find_map(|command| {
                if let egui::OutputCommand::CopyText(text) = command {
                    Some(text.clone())
                } else {
                    None
                }
            });
            output.drop_without_applying_deltas();
            (copied, text)
        };

        let (copied, text) = run(
            &mut controls,
            Operation::Link("a".into()),
            ShareKind::Layout,
        );
        assert_eq!((copied, text.as_deref()), (None, Some("https://host/a")));
        let link = ShareLink::new("https://host/a", false);
        assert!(matches!(&controls.last, Some((_, Outcome::Link(l))) if *l == link));

        let (copied, text) = run(
            &mut controls,
            Operation::Copy("a".into()),
            ShareKind::Layout,
        );
        assert_eq!((copied, text), (Some(("a".into(), "a copy".into())), None));

        let (_, text) = run(
            &mut controls,
            Operation::Link("w".into()),
            ShareKind::Watchlist,
        );
        assert_eq!(text, None);
        assert!(matches!(
            &controls.last,
            Some((_, Outcome::Failed(FeedError::Other(reason)))) if reason == "not allowed"
        ));
        assert!(controls.pending.is_none());
    }
}
