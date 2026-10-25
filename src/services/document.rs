//! One document of the host's storage, read before it is written.

use super::{Storage, StoreKind};
use crate::data::FeedError;
use crate::data::inbox::Inbox;
use std::sync::Arc;

/// An answer [`StoredDocument::poll`] received.
pub(crate) enum Received {
    /// The document as stored, or `None` when there is none.
    Loaded(Option<String>),
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
    writing: bool,
    waiting: Option<String>,
    /// Whether the last write failed, so that a run of failures is reported once.
    failing: bool,
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
            writing: false,
            waiting: None,
            failing: false,
        }
    }

    /// Whether the document is still being read.
    pub(crate) fn reading(&self) -> bool {
        self.writable.is_none()
    }

    /// Never writes the document, after its content could not be understood.
    pub(crate) fn refuse_writes(&mut self) {
        self.writable = Some(false);
        self.waiting = None;
    }

    /// Writes `document` after the write in flight. Dropped before the document has been read,
    /// since the caller saves again from what it read, and when it may not be written.
    pub(crate) fn save(&mut self, storage: &dyn Storage, document: String) {
        if self.writable != Some(true) {
            return;
        }
        if self.writing {
            self.waiting = Some(document);
        } else {
            self.writing = true;
            storage.save(self.kind, &self.name, document, self.inbox.reply(1));
        }
    }

    /// Takes the answers that arrived, sends the document waiting for them and wakes the UI on
    /// the next answer.
    pub(crate) fn poll(&mut self, storage: &dyn Storage, ctx: &egui::Context) -> Vec<Received> {
        let repaint = ctx.clone();
        self.inbox.set_waker(move || repaint.request_repaint());
        self.receive(storage)
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
                self.writing = false;
                let answer = *answer
                    .downcast::<Result<(), FeedError>>()
                    .expect("document save");
                match answer {
                    Ok(()) => self.failing = false,
                    Err(error) if !self.failing => {
                        self.failing = true;
                        received.push(Received::Failed(format!(
                            "Could not save {}: {error}",
                            self.name
                        )));
                    }
                    Err(_) => {}
                }
            }
            if !self.writing
                && let Some(document) = self.waiting.take()
            {
                self.save(storage, document);
            }
        }
        received
    }
}
