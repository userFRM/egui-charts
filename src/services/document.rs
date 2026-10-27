//! One document of the host's storage, read before it is written.

use super::{Storage, StoreKind};
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
}

/// One named document of the host's [`Storage`]. Nothing is written before it has been read, nor
/// after it could not be read, so a document this release cannot read is never replaced. One
/// write is in flight at a time: a newer document waits for it, replacing any older one waiting.
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
}

impl StoredDocument {
    /// Starts reading the document `name` of `kind`.
    pub(crate) fn open(storage: &dyn Storage, kind: StoreKind, name: &str) -> Self {
        let inbox = Arc::<Inbox>::default();
        storage.load(kind, name, inbox.reply(0));
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
        }
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
        if self.waiting.as_ref() == Some(&document) && self.writing.is_some() {
            return;
        }
        if self.writing.is_some() {
            self.waiting = Some(document);
        } else {
            self.waiting = None;
            self.retry_at = None;
            self.writing = Some(document.clone());
            storage.save(self.kind, &self.name, document, self.inbox.reply(1));
        }
    }

    /// Whether an unconfirmed write could still replace the stored document.
    pub(crate) fn saving(&self) -> bool {
        self.writing.is_some() || self.waiting.is_some()
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
            self.save(storage, document);
            received.extend(self.receive(storage));
        }
        received
    }

    /// Finishes replies already available, including inline writes, without changing the waker.
    pub(crate) fn receive(&mut self, storage: &dyn Storage) -> Vec<Received> {
        let mut received = Vec::new();
        loop {
            let answers = self.inbox.drain().answers;
            if answers.is_empty() {
                break;
            }
            for (key, answer) in answers {
                if key == 0 {
                    let answer = *answer
                        .downcast::<Result<Option<String>, FeedError>>()
                        .expect("document load");
                    match answer {
                        Ok(document) => {
                            self.writable.get_or_insert(true);
                            received.push(Received::Loaded(document));
                        }
                        Err(error) => {
                            self.refuse_writes();
                            received.push(Received::Failed(format!(
                                "Could not read {}: {error}",
                                self.name
                            )));
                        }
                    }
                    continue;
                }
                let document = self.writing.take().expect("document save in flight");
                let answer = *answer
                    .downcast::<Result<(), FeedError>>()
                    .expect("document save");
                match answer {
                    Ok(()) => {
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
            if !self.failing
                && self.writing.is_none()
                && let Some(document) = self.waiting.take()
            {
                self.save(storage, document);
            }
        }
        received
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::Reply;
    use std::sync::Mutex;

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
        for newer in [false, true] {
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
            if newer {
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
            poll(&mut document, 10.99);
            assert!(
                host.0.lock().unwrap().is_empty(),
                "retry must wait one second"
            );
            poll(&mut document, 11.0);
            let (json, reply) = host.0.lock().unwrap().pop().unwrap();
            assert_eq!(json, if newer { "latest" } else { "first" });
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
}
