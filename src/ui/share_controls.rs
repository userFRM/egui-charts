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
    Revoke(String),
}

/// What the last operation came to, for the document it was made on.
#[derive(Debug)]
enum Outcome {
    Link(ShareLink),
    Copy(String),
    Revoked,
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

impl std::fmt::Debug for ShareControls {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ShareControls").finish_non_exhaustive()
    }
}

impl ShareControls {
    pub(crate) fn set_service(&mut self, sharing: Arc<dyn Sharing>) {
        self.inbox = Arc::default();
        self.pending = None;
        self.last = None;
        self.sharing = Some(sharing);
    }

    /// Takes the host's answer, copying a link to the clipboard. Returns the saved copy's name.
    pub(crate) fn poll(&mut self, ctx: &egui::Context) -> Option<String> {
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
                Operation::Revoke(_) => answer.downcast::<Result<(), FeedError>>().map_or_else(
                    |_| Outcome::Failed(FeedError::Other("answer of another request".into())),
                    |answer| match *answer {
                        Ok(()) => Outcome::Revoked,
                        Err(error) => Outcome::Failed(error),
                    },
                ),
                Operation::Copy(_) => answer.downcast::<Result<String, FeedError>>().map_or_else(
                    |_| Outcome::Failed(FeedError::Other("answer of another request".into())),
                    |answer| match *answer {
                        Ok(copy) => {
                            copied = Some(copy.clone());
                            Outcome::Copy(copy)
                        }
                        Err(error) => Outcome::Failed(error),
                    },
                ),
            };
            self.last = Some((operation, outcome));
        }
        copied
    }

    /// Shows the rows the service offers for the document `name`, then the progress or result of
    /// the last operation on it. Shows nothing when the service offers nothing for `kind`.
    /// Returns true when Save a copy was chosen, so the caller can keep the document as it was.
    pub(crate) fn show(&mut self, ui: &mut Ui, kind: ShareKind, name: &str) -> bool {
        let Some(sharing) = self.sharing.clone() else {
            return false;
        };
        let offer = sharing.offers(kind);
        let shared = sharing.shared(kind, name);
        // Keep the host's revoke answer visible after shared state disappears.
        let revoked = self.last.as_ref().is_some_and(
            |(operation, _)| matches!(operation, Operation::Revoke(document) if document == name),
        );
        let revoking =
            matches!(&self.pending, Some(Operation::Revoke(document)) if document == name);
        if !offer.link && !offer.copy && !shared && !revoked && !revoking {
            return false;
        }
        ui.separator();
        if shared {
            ui.weak(l10n::text(ui.ctx(), "Shared by link"));
        }
        let mut copy = false;
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
            copy = true;
            ui.ctx().request_repaint();
        }
        if offer.revoke
            && shared
            && ui
                .add_enabled(
                    self.pending.is_none(),
                    egui::Button::new(l10n::text(ui.ctx(), "Stop sharing")),
                )
                .clicked()
        {
            self.pending = Some(Operation::Revoke(name.to_owned()));
            sharing.revoke(kind, name, self.inbox.reply(0));
            ui.ctx().request_repaint();
        }
        let status = if let Some(pending) = &self.pending {
            Some((
                false,
                l10n::text(
                    ui.ctx(),
                    match pending {
                        Operation::Link(_) => "Getting the link…",
                        Operation::Copy(_) => "Saving a copy…",
                        Operation::Revoke(_) => "Stopping sharing…",
                    },
                ),
            ))
        } else {
            self.last.as_ref().and_then(|(operation, outcome)| {
                let (Operation::Link(shared) | Operation::Copy(shared) | Operation::Revoke(shared)) = operation;
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
                    Outcome::Revoked => (false, l10n::text(ui.ctx(), "Sharing stopped")),
                    Outcome::Failed(error) => (
                        true,
                        l10n::format_text(
                            ui.ctx(),
                            if matches!(operation, Operation::Revoke(_)) { "Could not stop sharing: {0}" } else { "Could not share: {0}" },
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
        copy
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

    /// Revoke carries the document identity, waits for the host and renders both answers.
    #[test]
    fn stop_sharing_reports_the_hosts_state_and_answer() {
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        use std::sync::Mutex;
        #[derive(Default)]
        struct Host(Mutex<(bool, Vec<(ShareKind, String, Reply<()>)>)>);
        impl Sharing for Host {
            fn offers(&self, _: ShareKind) -> ShareOffer {
                let mut offer = ShareOffer::default();
                offer.revoke = true;
                offer
            }
            fn shared(&self, _: ShareKind, name: &str) -> bool {
                name == "Shared" && !self.0.lock().unwrap().0
            }
            fn revoke(&self, kind: ShareKind, name: &str, reply: Reply<()>) {
                self.0
                    .lock()
                    .unwrap()
                    .1
                    .push((kind, name.to_owned(), reply));
            }
        }
        let host = Arc::new(Host::default());
        let mut controls = ShareControls::default();
        controls.set_service(host.clone());
        let mut h = Harness::builder().allow_missing_glyphs().build_ui_state(
            |ui, (controls, name): &mut (ShareControls, String)| {
                controls.poll(ui.ctx());
                controls.show(ui, ShareKind::Watchlist, name);
            },
            (controls, "Private".into()),
        );
        assert!(h.query_by_value("Shared by link").is_none());
        assert!(h.query_by_label("Stop sharing").is_none());
        h.state_mut().1 = "Shared".into();
        h.run();
        assert!(h.query_by_value("Shared by link").is_some());
        h.get_by_label("Stop sharing").click();
        h.run();
        assert!(
            h.get_by_label("Stop sharing")
                .accesskit_node()
                .is_disabled()
        );
        assert!(h.query_by_value("Stopping sharing…").is_some());
        host.0.lock().unwrap().0 = true;
        h.run();
        assert!(h.query_by_value("Stopping sharing…").is_some());
        host.0.lock().unwrap().0 = false;
        let (kind, name, reply) = host.0.lock().unwrap().1.pop().unwrap();
        assert_eq!((kind, name.as_str()), (ShareKind::Watchlist, "Shared"));
        reply.send(Err(FeedError::Other("locked".into())));
        h.run();
        assert!(h.query_by_value("Could not stop sharing: locked").is_some());
        assert!(
            !h.get_by_label("Stop sharing")
                .accesskit_node()
                .is_disabled()
        );
        h.get_by_label("Stop sharing").click();
        h.run();
        let (_, _, reply) = host.0.lock().unwrap().1.pop().unwrap();
        host.0.lock().unwrap().0 = true;
        reply.send(Ok(()));
        h.run();
        assert!(h.query_by_value("Sharing stopped").is_some());
        assert!(h.query_by_value("Shared by link").is_none());
        assert!(h.query_by_label("Stop sharing").is_none());
    }

    /// A link reaches the clipboard with the host's copy policy, a copy returns its saved name,
    /// and a rejection is kept for the menu to show.
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
                Operation::Revoke(ref name) => host.revoke(kind, name, controls.inbox.reply(0)),
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
        assert_eq!((copied, text), (Some("a copy".into()), None));

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
