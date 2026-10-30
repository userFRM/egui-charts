//! One document of the host's storage, read before it is written.

use super::{ConflictChoice, Listener, Revision, SaveOutcome, Storage, StoreKind, Versioned};
use crate::data::FeedError;
use crate::data::inbox::Inbox;
use std::sync::Arc;
use std::time::Duration;

/// An answer [`StoredDocument::poll`] received.
pub(crate) enum Received {
    /// The document as stored, or `None` when there is none.
    Loaded(Option<String>),
    /// The exact document acknowledged by a successful write.
    Saved(String),
    /// A read failed, or the first of a run of failed writes. The message names the document.
    Failed(String),
    /// The host refused a write because the stored document changed; the unsaved document waits
    /// for [`StoredDocument::resolve`].
    Conflict,
}

/// Keys of the answers in the inbox.
const LOAD: u64 = 0;
const SAVE: u64 = 1;
const LOAD_VERSIONED: u64 = 2;
const SAVE_VERSIONED: u64 = 3;
const COPY: u64 = 4;
const REFRESH: u64 = 5;
const CHANGED: u64 = 6;

/// How many names a kept copy tries, from `<name> (local copy)` to `<name> (local copy 100)`,
/// before it is reported as failed.
const COPY_NAMES: u32 = 100;

/// The unsaved document being written as a separate copy for [`ConflictChoice::KeepLocalCopy`].
struct LocalCopy {
    /// 1 for the name `<name> (local copy)`, 2 for `<name> (local copy 2)` and so on.
    number: u32,
    /// The draft being written.
    draft: String,
    /// The revision this copy was written at, so that a newer draft replaces only this copy.
    revision: Option<Revision>,
}

/// One named document of the host's [`Storage`]. Nothing is written before it has been read, nor
/// after it could not be read, so a document this release cannot read is never replaced. One
/// write is in flight at a time: a newer document waits for it, replacing any older one waiting.
///
/// If the host keeps revisions, a write is made over the revision read, and one the host refuses
/// leaves the newest unsaved document waiting until [`resolve`](Self::resolve) settles it.
pub(crate) struct StoredDocument {
    kind: StoreKind,
    name: String,
    inbox: Arc<Inbox>,
    /// `None` while reading, then whether it may be written.
    writable: Option<bool>,
    writing: Option<String>,
    waiting: Option<String>,
    /// Whether the last write failed, so that a run of failures is reported once.
    failing: bool,
    /// Input time of the last poll and the earliest automatic retry after a failure.
    now: f64,
    retry_at: Option<f64>,
    /// Whether the host keeps revisions, once its first read answered.
    versioned: Option<bool>,
    /// The revision the document was last read or written at; `None` while none exists.
    revision: Option<Revision>,
    /// The stored version a write was refused for, `Some(None)` if it was deleted.
    conflict: Option<Option<Versioned>>,
    /// The copy being written after the user chose to keep one.
    copy: Option<LocalCopy>,
    /// Whether a read after a remote change is in flight.
    refreshing: bool,
    /// The newest revision another client reported that this document has not read.
    notified: Option<Revision>,
    /// Whether the owner has changes it has not passed to [`save`](Self::save) yet.
    held: bool,
    /// Answers produced by [`resolve`](Self::resolve), delivered with the next ones.
    ready: Vec<Received>,
    changes: Option<Listener>,
}

impl StoredDocument {
    /// Starts reading the document `name` of `kind`.
    pub(crate) fn open(storage: &dyn Storage, kind: StoreKind, name: &str) -> Self {
        let inbox = Arc::<Inbox>::default();
        storage.load_versioned(kind, name, inbox.reply(LOAD_VERSIONED));
        Self {
            kind,
            name: name.to_owned(),
            inbox,
            writable: None,
            writing: None,
            waiting: None,
            failing: false,
            now: 0.0,
            retry_at: None,
            versioned: None,
            revision: None,
            conflict: None,
            copy: None,
            refreshing: false,
            notified: None,
            held: false,
            ready: Vec::new(),
            changes: None,
        }
    }

    /// Follows remote changes of a document the host keeps revisions of: a change by another
    /// client is read and delivered as [`Received::Loaded`] once nothing is unsaved.
    pub(crate) fn follow_changes(&mut self, storage: &dyn Storage) {
        let (listener, events) = Listener::new(&self.inbox, CHANGED);
        storage.watch(self.kind, &self.name, events);
        self.changes = Some(listener);
    }

    /// Whether the document is still being read.
    #[cfg(feature = "drawings")]
    pub(crate) fn reading(&self) -> bool {
        self.writable.is_none()
    }

    /// Never writes the document, after its content could not be understood.
    pub(crate) fn refuse_writes(&mut self) {
        self.writable = Some(false);
        self.waiting = None;
        self.retry_at = None;
    }

    /// Whether the owner has changes it has not passed to [`save`](Self::save) yet. Remote
    /// changes are not read meanwhile, so that they cannot replace those changes: they are read
    /// once nothing is unsaved, and a write made before then is refused if one happened.
    pub(crate) fn hold(&mut self, unsaved: bool) {
        self.held = unsaved;
    }

    /// Writes `document` after the write in flight. Dropped before the document has been read,
    /// since the caller saves again from what it read, and when it may not be written.
    pub(crate) fn save(&mut self, storage: &dyn Storage, document: String) {
        if self.writable != Some(true) {
            return;
        }
        if self.writing.as_ref() == Some(&document) {
            self.waiting = None;
            return;
        }
        if self.writing.is_some() || self.retry_at.is_some() || self.conflict.is_some() {
            self.waiting = Some(document);
        } else {
            self.waiting = None;
            self.retry_at = None;
            self.writing = Some(document.clone());
            if self.versioned == Some(true) {
                storage.save_versioned(
                    self.kind,
                    &self.name,
                    document,
                    self.revision.clone(),
                    self.inbox.reply(SAVE_VERSIONED),
                );
            } else {
                storage.save(self.kind, &self.name, document, self.inbox.reply(SAVE));
            }
        }
    }

    /// Whether an unconfirmed write could still replace the stored document.
    pub(crate) fn saving(&self) -> bool {
        self.writing.is_some() || self.waiting.is_some()
    }

    /// Whether the host refused the newest write because the stored document changed, until the
    /// choice that [`resolve`](Self::resolve) is given has taken effect.
    pub(crate) fn conflicted(&self) -> bool {
        self.conflict.is_some()
    }

    /// Whether the choice to keep a local copy is taking effect.
    pub(crate) fn copying(&self) -> bool {
        self.copy.is_some()
    }

    /// Settles a refused write. The unsaved document stays in memory until the choice has taken
    /// effect, and a failed copy leaves the conflict open.
    pub(crate) fn resolve(&mut self, storage: &dyn Storage, choice: ConflictChoice) {
        let Some(remote) = self.conflict.clone() else {
            return;
        };
        if self.copy.is_some() {
            return;
        }
        match (choice, self.waiting.clone()) {
            (ConflictChoice::Retry, draft) => {
                self.conflict = None;
                self.revision = remote.map(|stored| stored.revision);
                if let Some(draft) = draft {
                    self.waiting = None;
                    self.save(storage, draft);
                }
            }
            (ConflictChoice::KeepLocalCopy, Some(draft)) => {
                self.copy = Some(LocalCopy {
                    number: 1,
                    draft,
                    revision: None,
                });
                self.write_copy(storage);
            }
            _ => self.adopt(remote),
        }
    }

    /// Writes the kept copy under the name of its number. Its first write creates it, so that a
    /// document of that name is never replaced; a later one replaces only this copy.
    fn write_copy(&self, storage: &dyn Storage) {
        let copy = self.copy.as_ref().expect("copy in progress");
        let name = match copy.number {
            1 => format!("{} (local copy)", self.name),
            number => format!("{} (local copy {number})", self.name),
        };
        storage.save_versioned(
            self.kind,
            &name,
            copy.draft.clone(),
            copy.revision.clone(),
            self.inbox.reply(COPY),
        );
    }

    /// Takes the stored version over the unsaved document.
    fn adopt(&mut self, remote: Option<Versioned>) {
        self.conflict = None;
        self.waiting = None;
        self.revision = remote.as_ref().map(|stored| stored.revision.clone());
        self.ready
            .push(Received::Loaded(remote.map(|stored| stored.document)));
    }

    /// Whether reading succeeded and no write remains unacknowledged.
    #[cfg(feature = "drawings")]
    pub(crate) fn flushed(&self) -> bool {
        self.writable == Some(true) && !self.saving()
    }

    /// Takes the answers that arrived, sends the document waiting for them and wakes the UI on
    /// the next answer.
    pub(crate) fn poll(&mut self, storage: &dyn Storage, ctx: &egui::Context) -> Vec<Received> {
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        self.now = ctx.input(|input| input.time);
        let mut received = self.receive(storage);
        if self.retry_at.is_some_and(|at| at <= self.now) {
            received.extend(self.flush(storage));
        }
        if let Some(at) = self.retry_at {
            ctx.request_repaint_after(Duration::from_secs_f64((at - self.now).max(0.0)));
        }
        received
    }

    /// Retries retained writes now, draining inline replies without waiting for asynchronous ones.
    pub(crate) fn flush(&mut self, storage: &dyn Storage) -> Vec<Received> {
        let mut received = self.receive(storage);
        if self.writing.is_none()
            && let Some(document) = self.waiting.take()
        {
            self.retry_at = None;
            self.save(storage, document);
            received.extend(self.receive(storage));
        }
        received
    }

    /// The first read of a document: its content, and whether the host keeps revisions.
    fn loaded(&mut self, document: Option<String>, revision: Option<Revision>) -> Received {
        self.writable.get_or_insert(true);
        self.revision = revision;
        Received::Loaded(document)
    }

    /// Whether reading the stored document could replace nothing unsaved.
    fn idle(&self) -> bool {
        self.conflict.is_none() && !self.saving() && !self.held
    }

    /// Reads the stored document again once nothing is unsaved, after another client changed it.
    fn refresh(&mut self, storage: &dyn Storage) {
        if self.versioned == Some(true)
            && !self.refreshing
            && self.idle()
            && let Some(revision) = self.notified.take()
            && self.revision.as_ref() != Some(&revision)
        {
            self.refreshing = true;
            storage.load_versioned(self.kind, &self.name, self.inbox.reply(REFRESH));
        }
    }

    /// Finishes replies already available, including inline writes, without changing the waker.
    pub(crate) fn receive(&mut self, storage: &dyn Storage) -> Vec<Received> {
        let mut received = std::mem::take(&mut self.ready);
        loop {
            self.refresh(storage);
            let answers = self.inbox.drain().answers;
            if answers.is_empty() {
                break;
            }
            for (key, answer) in answers {
                match key {
                    LOAD_VERSIONED => {
                        let answer = *answer
                            .downcast::<Result<Option<Versioned>, FeedError>>()
                            .expect("document load");
                        match answer {
                            Err(FeedError::NotSupported) => {
                                self.versioned = Some(false);
                                storage.load(self.kind, &self.name, self.inbox.reply(LOAD));
                            }
                            Ok(stored) => {
                                self.versioned = Some(true);
                                let (document, revision) = match stored {
                                    Some(Versioned { document, revision }) => {
                                        (Some(document), Some(revision))
                                    }
                                    None => (None, None),
                                };
                                received.push(self.loaded(document, revision));
                            }
                            Err(error) => received.push(self.unreadable(&error)),
                        }
                    }
                    LOAD => {
                        let answer = *answer
                            .downcast::<Result<Option<String>, FeedError>>()
                            .expect("document load");
                        match answer {
                            Ok(document) => received.push(self.loaded(document, None)),
                            Err(error) => received.push(self.unreadable(&error)),
                        }
                    }
                    COPY => self.copied(storage, answer, &mut received),
                    CHANGED => {
                        let revision = *answer.downcast::<Revision>().expect("remote change");
                        // Revisions this document read or wrote are known.
                        if self.revision.as_ref() != Some(&revision) {
                            self.notified = Some(revision);
                        }
                    }
                    REFRESH => {
                        self.refreshing = false;
                        let answer = *answer
                            .downcast::<Result<Option<Versioned>, FeedError>>()
                            .expect("document refresh");
                        if let Ok(stored) = answer {
                            if self.idle() {
                                self.revision =
                                    stored.as_ref().map(|stored| stored.revision.clone());
                                received
                                    .push(Received::Loaded(stored.map(|stored| stored.document)));
                            } else if let Some(stored) = stored {
                                // Read again once nothing is unsaved; a write before then is
                                // refused.
                                self.notified.get_or_insert(stored.revision);
                            }
                        }
                    }
                    _ => self.written(key, answer, &mut received),
                }
            }
            if !self.failing
                && self.writing.is_none()
                && let Some(document) = self.waiting.take()
            {
                self.save(storage, document);
            }
        }
        received
    }

    fn unreadable(&mut self, error: &FeedError) -> Received {
        self.refuse_writes();
        Received::Failed(format!("Could not read {}: {error}", self.name))
    }

    /// Finishes a write of the kept copy. A newer draft is written over the copy, and a name that
    /// another document holds moves the copy to the next one; once the copy holds the newest
    /// draft, the stored version is taken.
    fn copied(
        &mut self,
        storage: &dyn Storage,
        answer: Box<dyn std::any::Any + Send>,
        received: &mut Vec<Received>,
    ) {
        let answer = *answer
            .downcast::<Result<SaveOutcome, FeedError>>()
            .expect("copy save");
        let newest = self.waiting.clone();
        let copy = self.copy.as_mut().expect("copy in progress");
        match answer {
            Ok(SaveOutcome::Saved(revision)) => match newest {
                Some(newest) if newest != copy.draft => {
                    copy.draft = newest;
                    copy.revision = Some(revision);
                    self.write_copy(storage);
                }
                _ => {
                    self.copy = None;
                    let remote = self.conflict.clone().flatten();
                    self.adopt(remote);
                    received.append(&mut self.ready);
                }
            },
            Ok(SaveOutcome::Conflict(_)) if copy.number < COPY_NAMES => {
                copy.number += 1;
                copy.revision = None;
                if let Some(newest) = newest {
                    copy.draft = newest;
                }
                self.write_copy(storage);
            }
            outcome => {
                self.copy = None;
                let reason = match outcome {
                    Err(error) => error.to_string(),
                    _ => format!("{COPY_NAMES} copies exist already"),
                };
                received.push(Received::Failed(format!(
                    "Could not keep a copy of {}: {reason}",
                    self.name
                )));
            }
        }
    }

    /// Finishes the answer to the write in flight.
    fn written(
        &mut self,
        key: u64,
        answer: Box<dyn std::any::Any + Send>,
        received: &mut Vec<Received>,
    ) {
        let document = self.writing.take().expect("document save in flight");
        let answer = if key == SAVE_VERSIONED {
            *answer
                .downcast::<Result<SaveOutcome, FeedError>>()
                .expect("document save")
        } else {
            (*answer
                .downcast::<Result<(), FeedError>>()
                .expect("document save"))
            .map(|()| SaveOutcome::Saved(Revision(String::new())))
        };
        match answer {
            Ok(SaveOutcome::Conflict(remote)) => {
                // The newest unsaved document stays; a newer one may already wait.
                self.waiting.get_or_insert(document);
                self.conflict = Some(remote);
                self.retry_at = None;
                // The host answered, so a later failure starts a new run.
                self.failing = false;
                received.push(Received::Conflict);
            }
            Ok(SaveOutcome::Saved(revision)) => {
                if key == SAVE_VERSIONED {
                    self.revision = Some(revision);
                }
                self.failing = false;
                self.retry_at = None;
                received.push(Received::Saved(document));
            }
            Err(error) => {
                self.waiting.get_or_insert(document);
                self.retry_at = Some(self.now + 1.0);
                if !self.failing {
                    received.push(Received::Failed(format!(
                        "Could not save {}: {error}",
                        self.name
                    )));
                }
                self.failing = true;
            }
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::data::Reply;
    use crate::services::Events;
    use std::collections::HashMap;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicBool, Ordering};

    #[derive(Default)]
    struct Host(Mutex<Vec<(String, Reply<()>)>>);

    impl Storage for Host {
        fn list(&self, _: StoreKind, _: Reply<Vec<String>>) {
            unreachable!()
        }
        fn load(&self, _: StoreKind, _: &str, reply: Reply<Option<String>>) {
            reply.send(Ok(None));
        }
        fn save(&self, _: StoreKind, _: &str, document: String, reply: Reply<()>) {
            self.0.lock().unwrap().push((document, reply));
        }
        fn delete(&self, _: StoreKind, _: &str, _: Reply<()>) {
            unreachable!()
        }
    }

    #[test]
    fn failed_writes_retain_the_latest_document_and_retry_once_per_second() {
        for (before_failure, after_failure) in [(false, false), (true, false), (false, true)] {
            let host = Host::default();
            let ctx = egui::Context::default();
            let mut document = StoredDocument::open(&host, StoreKind::Drawings, "A");
            let poll = |document: &mut StoredDocument, time| {
                let mut received = Vec::new();
                ctx.run_ui(
                    egui::RawInput {
                        time: Some(time),
                        ..Default::default()
                    },
                    |_| {
                        received = document.poll(&host, &ctx);
                    },
                )
                .drop_without_applying_deltas();
                received
            };
            poll(&mut document, 9.0);
            document.save(&host, "first".into());
            if before_failure {
                document.save(&host, "latest".into());
            }
            let (_, reply) = host.0.lock().unwrap().pop().unwrap();
            reply.send(Err(FeedError::Network("offline".into())));
            assert!(matches!(
                poll(&mut document, 10.0).as_slice(),
                [Received::Failed(_)]
            ));
            assert!(document.saving(), "a failed write is still dirty");
            assert!(host.0.lock().unwrap().is_empty());
            if after_failure {
                document.save(&host, "latest".into());
            }
            poll(&mut document, 10.99);
            assert!(
                host.0.lock().unwrap().is_empty(),
                "retry must wait one second"
            );
            poll(&mut document, 11.0);
            let (json, reply) = host.0.lock().unwrap().pop().unwrap();
            assert_eq!(
                json,
                if before_failure || after_failure {
                    "latest"
                } else {
                    "first"
                }
            );
            reply.send(Err(FeedError::Network("offline".into())));
            assert!(
                poll(&mut document, 11.0).is_empty(),
                "report a run of failures once"
            );
            poll(&mut document, 11.99);
            assert!(host.0.lock().unwrap().is_empty());
            poll(&mut document, 12.0);
            let (json, reply) = host.0.lock().unwrap().pop().unwrap();
            reply.send(Ok(()));
            assert!(
                matches!(poll(&mut document, 12.0).as_slice(), [Received::Saved(saved)] if saved == &json)
            );
            assert!(!document.saving());
            poll(&mut document, 600.0);
            assert!(
                host.0.lock().unwrap().is_empty(),
                "acknowledged writes need no retry"
            );
        }
    }

    /// A host that keeps revisions as a counter and pushes every change to its watchers. Its
    /// versioned writes fail while the flag is set.
    #[derive(Default)]
    pub(crate) struct Revised(Mutex<Remote>, AtomicBool);

    /// The documents with their counters, and the watchers.
    type Remote = (HashMap<String, (String, u64)>, Vec<Events<Revision>>);

    impl Revised {
        pub(crate) fn put(&self, name: &str, document: &str) -> Revision {
            let mut state = self.0.lock().unwrap();
            let revision = state.0.get(name).map_or(1, |(_, n)| n + 1);
            state.0.insert(name.into(), (document.into(), revision));
            let revision = Revision(revision.to_string());
            state
                .1
                .iter()
                .for_each(|events| events.push(revision.clone()));
            revision
        }

        pub(crate) fn get(&self, name: &str) -> Option<String> {
            self.0.lock().unwrap().0.get(name).map(|(d, _)| d.clone())
        }

        /// Deletes `name`, as another client does.
        pub(crate) fn remove(&self, name: &str) {
            self.0.lock().unwrap().0.remove(name);
        }
    }

    impl Storage for Revised {
        fn list(&self, _: StoreKind, _: Reply<Vec<String>>) {
            unreachable!()
        }
        fn load(&self, _: StoreKind, _: &str, _: Reply<Option<String>>) {
            unreachable!()
        }
        fn save(&self, _: StoreKind, name: &str, document: String, reply: Reply<()>) {
            self.put(name, &document);
            reply.send(Ok(()));
        }
        fn delete(&self, _: StoreKind, _: &str, _: Reply<()>) {
            unreachable!()
        }
        fn load_versioned(&self, _: StoreKind, name: &str, reply: Reply<Option<Versioned>>) {
            let state = self.0.lock().unwrap();
            reply.send(Ok(state.0.get(name).map(|(document, n)| Versioned {
                document: document.clone(),
                revision: Revision(n.to_string()),
            })));
        }
        fn save_versioned(
            &self,
            _: StoreKind,
            name: &str,
            document: String,
            expected: Option<Revision>,
            reply: Reply<SaveOutcome>,
        ) {
            if self.1.load(Ordering::Relaxed) {
                return reply.send(Err(FeedError::Network("offline".into())));
            }
            let current = self.0.lock().unwrap().0.get(name).cloned();
            let stored = current.map(|(document, n)| Versioned {
                document,
                revision: Revision(n.to_string()),
            });
            if stored.as_ref().map(|v| &v.revision) != expected.as_ref() {
                return reply.send(Ok(SaveOutcome::Conflict(stored)));
            }
            reply.send(Ok(SaveOutcome::Saved(self.put(name, &document))));
        }
        fn watch(&self, _: StoreKind, _: &str, events: Events<Revision>) {
            self.0.lock().unwrap().1.push(events);
        }
    }

    #[test]
    fn a_write_over_a_newer_revision_is_refused_and_the_draft_waits_for_the_users_choice() {
        // The choice, a copy kept earlier, an edit made right after choosing, the document
        // stored, the copies `A (local copy)` and `A (local copy 2)`, and what is loaded.
        for (choice, earlier, edit, stored, copies, loaded) in [
            (
                ConflictChoice::ReloadRemote,
                None,
                None,
                "theirs",
                [None, None],
                Some("theirs"),
            ),
            (
                ConflictChoice::KeepLocalCopy,
                None,
                Some("mine latest"),
                "theirs",
                [Some("mine latest"), None],
                Some("theirs"),
            ),
            (
                ConflictChoice::KeepLocalCopy,
                Some("kept before"),
                None,
                "theirs",
                [Some("kept before"), Some("mine again")],
                Some("theirs"),
            ),
            (
                ConflictChoice::Retry,
                None,
                None,
                "mine again",
                [None, None],
                None,
            ),
        ] {
            let host = Revised::default();
            host.put("A", "base");
            if let Some(earlier) = earlier {
                host.put("A (local copy)", earlier);
            }
            let mut first = StoredDocument::open(&host, StoreKind::Drawings, "A");
            let mut second = StoredDocument::open(&host, StoreKind::Drawings, "A");
            for document in [&mut first, &mut second] {
                assert!(matches!(
                    document.receive(&host).as_slice(),
                    [Received::Loaded(Some(base))] if base == "base"
                ));
            }
            second.save(&host, "theirs".into());
            second.receive(&host);
            first.save(&host, "mine".into());
            assert!(matches!(
                first.receive(&host).as_slice(),
                [Received::Conflict]
            ));
            assert!(first.conflicted() && first.saving());
            first.save(&host, "mine again".into());
            first.flush(&host);
            assert_eq!(
                host.get("A").as_deref(),
                Some("theirs"),
                "nothing overwritten"
            );
            first.resolve(&host, choice);
            if let Some(edit) = edit {
                first.save(&host, edit.into());
            }
            let received = first.receive(&host);
            assert_eq!(host.get("A").as_deref(), Some(stored), "{choice:?}");
            let kept = ["A (local copy)", "A (local copy 2)"].map(|name| host.get(name));
            assert_eq!(kept.each_ref().map(Option::as_deref), copies, "{choice:?}");
            match loaded {
                Some(remote) => assert!(
                    matches!(received.as_slice(), [Received::Loaded(Some(d))] if d == remote),
                    "{choice:?}"
                ),
                None => assert!(
                    matches!(received.as_slice(), [Received::Saved(d)] if d == "mine again")
                ),
            }
            assert!(!first.conflicted() && !first.saving(), "{choice:?}");
        }
    }

    #[test]
    fn a_followed_document_loads_remote_changes_and_a_dropped_one_stops_listening() {
        let host = Revised::default();
        host.put("A", "base");
        let mut mine = StoredDocument::open(&host, StoreKind::Settings, "A");
        mine.follow_changes(&host);
        mine.receive(&host);
        let mut theirs = StoredDocument::open(&host, StoreKind::Settings, "A");
        theirs.receive(&host);
        theirs.save(&host, "theirs".into());
        theirs.receive(&host);
        assert!(matches!(
            mine.receive(&host).as_slice(),
            [Received::Loaded(Some(d))] if d == "theirs"
        ));
        // Its own write comes back as a known revision and reads nothing.
        mine.save(&host, "mine".into());
        assert!(matches!(
            mine.receive(&host).as_slice(),
            [Received::Saved(_)]
        ));
        assert!(mine.receive(&host).is_empty());
        // Changes while a write is refused are read once the conflict is settled.
        host.put("A", "theirs again");
        mine.save(&host, "mine again".into());
        assert!(matches!(
            mine.receive(&host).as_slice(),
            [Received::Conflict]
        ));
        host.put("A", "newest");
        assert!(mine.receive(&host).is_empty());
        mine.resolve(&host, ConflictChoice::ReloadRemote);
        assert!(matches!(
            mine.receive(&host).as_slice(),
            [Received::Loaded(Some(a)), Received::Loaded(Some(b))]
                if a == "theirs again" && b == "newest"
        ));
        let events = host.0.lock().unwrap().1[0].clone();
        drop(mine);
        assert!(!events.is_active());
    }

    #[test]
    fn a_failed_write_after_a_conflict_is_reported() {
        let host = Revised::default();
        host.put("A", "base");
        let mut document = StoredDocument::open(&host, StoreKind::Drawings, "A");
        document.receive(&host);
        host.1.store(true, Ordering::Relaxed);
        document.save(&host, "mine".into());
        assert!(matches!(
            document.receive(&host).as_slice(),
            [Received::Failed(_)]
        ));
        host.1.store(false, Ordering::Relaxed);
        host.put("A", "theirs");
        assert!(matches!(
            document.flush(&host).as_slice(),
            [Received::Conflict]
        ));
        host.1.store(true, Ordering::Relaxed);
        document.resolve(&host, ConflictChoice::Retry);
        assert!(matches!(
            document.receive(&host).as_slice(),
            [Received::Failed(_)]
        ));
    }
}
