//! Sharing state and operations in the layout menu and the watchlist panel, driven by the
//! host's [`Sharing`] service.

use crate::data::FeedError;
use crate::data::inbox::Inbox;
use crate::localization as l10n;
use crate::services::{ShareAccess, ShareKind, ShareLink, Sharing};
use egui::Ui;
use std::sync::Arc;

/// An operation on the document `name`.
#[derive(Clone, Debug, PartialEq)]
enum Operation {
    Link(String),
    Copy(String),
    Revoke(String),
    Share(String, ShareAccess),
}

/// What the last operation came to, for the document it was made on.
#[derive(Debug)]
enum Outcome {
    Link(ShareLink),
    Copy(String),
    Revoked,
    Shared(ShareAccess),
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
                Operation::Share(_, access) => {
                    answer.downcast::<Result<(), FeedError>>().map_or_else(
                        |_| Outcome::Failed(FeedError::Other("answer of another request".into())),
                        |answer| match *answer {
                            Ok(()) => Outcome::Shared(*access),
                            Err(error) => Outcome::Failed(error),
                        },
                    )
                }
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
    /// the last operation on it. Shows nothing without offers, shared state or a revoke result.
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
        if !offer.link && !offer.copy && !offer.switch && !shared && !revoked && !revoking {
            return false;
        }
        ui.separator();
        let idle = self.pending.is_none();
        if offer.switch {
            let label = match kind {
                ShareKind::Layout => "Share layout",
                ShareKind::Watchlist => "Share list",
            };
            let mut on = shared;
            // Sharing starts view only; the access choice below widens it.
            if ui
                .add_enabled(
                    idle,
                    egui::Checkbox::new(&mut on, l10n::text(ui.ctx(), label)),
                )
                .changed()
            {
                if on {
                    self.pending = Some(Operation::Share(name.to_owned(), ShareAccess::ViewOnly));
                    sharing.share(kind, name, ShareAccess::ViewOnly, self.inbox.reply(0));
                } else {
                    self.pending = Some(Operation::Revoke(name.to_owned()));
                    sharing.revoke(kind, name, self.inbox.reply(0));
                }
                ui.ctx().request_repaint();
            }
            if shared {
                let current = sharing.access(kind, name);
                for (access, text) in [
                    (ShareAccess::ViewOnly, "View only"),
                    (ShareAccess::ViewAndCopy, "View and copy"),
                ] {
                    if ui
                        .add_enabled(
                            idle,
                            egui::RadioButton::new(current == access, l10n::text(ui.ctx(), text)),
                        )
                        .clicked()
                        && current != access
                    {
                        self.pending = Some(Operation::Share(name.to_owned(), access));
                        sharing.share(kind, name, access, self.inbox.reply(0));
                        ui.ctx().request_repaint();
                    }
                }
            }
        } else if shared {
            ui.weak(l10n::text(ui.ctx(), "Shared by link"));
        }
        let mut copy = false;
        if offer.link
            && (!offer.switch || shared)
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
            && !offer.switch
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
                        Operation::Share(..) => "Sharing…",
                    },
                ),
            ))
        } else {
            self.last.as_ref().and_then(|(operation, outcome)| {
                let (Operation::Link(shared)
                | Operation::Copy(shared)
                | Operation::Revoke(shared)
                | Operation::Share(shared, _)) = operation;
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
                    Outcome::Shared(access) => (
                        false,
                        l10n::text(
                            ui.ctx(),
                            match access {
                                ShareAccess::ViewOnly => "Shared. Recipients can only view it.",
                                ShareAccess::ViewAndCopy => {
                                    "Shared. Recipients can view it and save a copy."
                                }
                            },
                        ),
                    ),
                    Outcome::Failed(error) => (
                        true,
                        l10n::format_text(
                            ui.ctx(),
                            if matches!(operation, Operation::Revoke(_)) {
                                "Could not stop sharing: {0}"
                            } else {
                                "Could not share: {0}"
                            },
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
        struct Host {
            stopped: Mutex<bool>,
            calls: Mutex<Vec<(ShareKind, String, Reply<()>)>>,
        }
        impl Sharing for Host {
            fn offers(&self, _: ShareKind) -> ShareOffer {
                ShareOffer {
                    revoke: true,
                    ..Default::default()
                }
            }
            fn shared(&self, _: ShareKind, name: &str) -> bool {
                name == "Shared" && !*self.stopped.lock().unwrap()
            }
            fn revoke(&self, kind: ShareKind, name: &str, reply: Reply<()>) {
                self.calls
                    .lock()
                    .unwrap()
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
        assert!(
            h.query_by_role_and_label(egui::Role::Label, "Shared by link")
                .is_none()
        );
        assert!(h.query_by_label("Stop sharing").is_none());
        h.state_mut().1 = "Shared".into();
        h.run();
        assert!(
            h.query_by_role_and_label(egui::Role::Label, "Shared by link")
                .is_some()
        );
        h.get_by_label("Stop sharing").click();
        h.run();
        assert!(
            h.get_by_label("Stop sharing")
                .accesskit_node()
                .is_disabled()
        );
        assert!(
            h.query_by_role_and_label(egui::Role::Label, "Stopping sharing…")
                .is_some()
        );
        *host.stopped.lock().unwrap() = true;
        h.run();
        assert!(
            h.query_by_role_and_label(egui::Role::Label, "Stopping sharing…")
                .is_some()
        );
        *host.stopped.lock().unwrap() = false;
        let (kind, name, reply) = host.calls.lock().unwrap().pop().unwrap();
        assert_eq!((kind, name.as_str()), (ShareKind::Watchlist, "Shared"));
        reply.send(Err(FeedError::Other("locked".into())));
        h.run();
        assert!(
            h.query_by_role_and_label(egui::Role::Label, "Could not stop sharing: locked")
                .is_some()
        );
        assert!(
            !h.get_by_label("Stop sharing")
                .accesskit_node()
                .is_disabled()
        );
        h.get_by_label("Stop sharing").click();
        h.run();
        let (_, _, reply) = host.calls.lock().unwrap().pop().unwrap();
        *host.stopped.lock().unwrap() = true;
        reply.send(Ok(()));
        h.run();
        assert!(
            h.query_by_role_and_label(egui::Role::Label, "Sharing stopped")
                .is_some()
        );
        assert!(
            h.query_by_role_and_label(egui::Role::Label, "Shared by link")
                .is_none()
        );
        assert!(h.query_by_label("Stop sharing").is_none());
    }

    /// The switch shares view only, widens or narrows the access, gates Copy link on the shared
    /// state the host reports and stops sharing; a host without it keeps its rows.
    #[test]
    fn the_sharing_switch_shares_with_the_chosen_access() {
        use egui_kittest::{
            Harness,
            kittest::{NodeT, Queryable},
        };
        use std::sync::Mutex;
        type Calls = Vec<(Option<ShareAccess>, Reply<()>)>;
        /// The access the document is shared with, if it is, and the calls waiting for answers.
        #[derive(Default)]
        struct Host(Mutex<(Option<ShareAccess>, Calls)>, bool);
        impl Sharing for Host {
            fn offers(&self, _: ShareKind) -> ShareOffer {
                let mut offer = ShareOffer::new(true, false);
                offer.switch = self.1;
                offer
            }
            fn shared(&self, _: ShareKind, _: &str) -> bool {
                self.0.lock().unwrap().0.is_some()
            }
            fn access(&self, _: ShareKind, _: &str) -> ShareAccess {
                self.0.lock().unwrap().0.unwrap_or_default()
            }
            fn share(&self, _: ShareKind, _: &str, access: ShareAccess, reply: Reply<()>) {
                self.0.lock().unwrap().1.push((Some(access), reply));
            }
            fn revoke(&self, _: ShareKind, _: &str, reply: Reply<()>) {
                self.0.lock().unwrap().1.push((None, reply));
            }
        }
        let harness = |host: Arc<Host>| {
            let mut controls = ShareControls::default();
            controls.set_service(host);
            let mut h = Harness::builder().allow_missing_glyphs().build_ui_state(
                |ui, controls: &mut ShareControls| {
                    controls.poll(ui.ctx());
                    controls.show(ui, ShareKind::Layout, "A");
                },
                controls,
            );
            h.run();
            h
        };
        // Answers the one waiting call, after setting the state the host then reports.
        let answer = |host: &Host, state: Option<ShareAccess>, result: Result<(), FeedError>| {
            let mut guard = host.0.lock().unwrap();
            let (access, reply) = guard.1.pop().unwrap();
            assert!(guard.1.is_empty());
            if result.is_ok() {
                guard.0 = state;
            }
            drop(guard);
            reply.send(result);
            access
        };
        let shown = |h: &Harness<'_, ShareControls>, label: &str| {
            h.query_by_role_and_label(egui::Role::Label, label)
                .is_some()
        };
        let host = Arc::new(Host(Mutex::default(), true));
        let mut h = harness(host.clone());
        let switch = |h: &Harness<'_, ShareControls>| {
            let node = h.get_by_label("Share layout");
            let node = node.accesskit_node();
            (
                node.toggled() == Some(egui::accesskit::Toggled::True),
                node.is_disabled(),
            )
        };
        assert_eq!(switch(&h), (false, false));
        assert!(h.query_by_label("Copy link").is_none());
        assert!(h.query_by_label("View only").is_none());

        h.get_by_label("Share layout").click();
        h.run();
        assert!(shown(&h, "Sharing…"));
        assert_eq!(switch(&h), (false, true));
        let sent = answer(&host, Some(ShareAccess::ViewOnly), Ok(()));
        assert_eq!(sent, Some(ShareAccess::ViewOnly));
        h.run();
        assert!(shown(&h, "Shared. Recipients can only view it."));
        assert_eq!(switch(&h), (true, false));
        assert!(h.query_by_label("Copy link").is_some());
        assert_eq!(
            h.get_by_label("View only").accesskit_node().toggled(),
            Some(egui::accesskit::Toggled::True)
        );

        h.get_by_label("View and copy").click();
        h.run();
        let sent = answer(&host, Some(ShareAccess::ViewAndCopy), Ok(()));
        assert_eq!(sent, Some(ShareAccess::ViewAndCopy));
        h.run();
        assert!(shown(&h, "Shared. Recipients can view it and save a copy."));
        h.get_by_label("View and copy").click();
        h.run();
        assert!(
            host.0.lock().unwrap().1.is_empty(),
            "the chosen access sends nothing"
        );

        h.get_by_label("Share layout").click();
        h.run();
        assert_eq!(answer(&host, None, Ok(())), None);
        h.run();
        assert!(shown(&h, "Sharing stopped"));
        assert!(h.query_by_label("Copy link").is_none());
        assert!(h.query_by_label("View only").is_none());

        h.get_by_label("Share layout").click();
        h.run();
        answer(&host, None, Err(FeedError::Other("quota".into())));
        h.run();
        assert!(shown(&h, "Could not share: quota"));
        assert_eq!(switch(&h), (false, false));

        // Without the switch the host's link row stays as it was.
        let h = harness(Arc::new(Host(Mutex::default(), false)));
        assert!(h.query_by_label("Copy link").is_some());
        assert!(h.query_by_label("Share layout").is_none());
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
                Operation::Share(ref name, access) => {
                    host.share(kind, name, access, controls.inbox.reply(0))
                }
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
