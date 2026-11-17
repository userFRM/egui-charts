//! Documents of one kind written over the revision they were read at.

use super::document::{COPY_NAMES, copy_name};
use super::{ConflictChoice, Revision, SaveOutcome, Storage, StoreKind, Versioned};
use crate::data::FeedError;
use crate::data::inbox::{Answer, Inbox};
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

/// What became of a write, from [`Revisions::saved`].
#[derive(Debug)]
pub(crate) enum Written {
    /// The host kept it.
    Saved,
    /// The host refused it because the stored document is not the version read: the stored
    /// version, or `None` if it was deleted. [`Revisions::refuse`] keeps the document for the
    /// user's choice.
    Refused(Option<Versioned>),
    /// The write failed.
    Failed(FeedError),
}

/// How a refused write was settled, from [`Revisions::poll`], for its owner to finish.
#[derive(Debug)]
pub(crate) enum Settled {
    /// Write this unsaved document again: it now replaces the stored version.
    Overwrite(String),
    /// The unsaved document was kept as the new document of this name. A `Reload` of the
    /// original follows.
    Copied(String),
    /// Take the stored version, `None` if it was deleted, in place of the unsaved document.
    Reload(Option<String>),
    /// The copy could not be kept, for this reason; the write stays refused.
    CopyFailed(String),
}

/// A refused write, until the user's choice has taken effect.
#[derive(Debug)]
struct Refused {
    /// The newest unsaved document.
    draft: String,
    /// The stored version the write was refused for, `None` if it was deleted.
    stored: Option<Versioned>,
    /// The kept copy being written.
    copy: Option<LocalCopy>,
}

/// The unsaved document being written as a document of its own for
/// [`ConflictChoice::KeepLocalCopy`].
#[derive(Debug)]
struct LocalCopy {
    /// The answer key of its writes.
    key: u64,
    /// 1 for `<name> (local copy)`, 2 for `<name> (local copy 2)` and so on.
    number: u32,
    /// The draft written last.
    draft: String,
    /// The revision the copy was written at, so that a newer draft replaces only this copy.
    revision: Option<Revision>,
}

/// The revisions the documents of one kind were read or written at, so that each write is made
/// over the revision read and a document never read is only created, and the writes the host
/// refused, which wait for the user's choice. The owner sends one call per document at a time.
///
/// A host that answers a versioned call with [`FeedError::NotSupported`] keeps no revisions:
/// documents are then read with [`Storage::load`] and written with [`Storage::save`]. Recent
/// symbols are always written so, each client's newest list replacing the stored one.
#[derive(Debug)]
pub(crate) struct Revisions {
    kind: StoreKind,
    /// `None` until the host answered a versioned call, then whether it keeps revisions.
    versioned: Option<bool>,
    /// The revision each document was last read or written at.
    read: HashMap<String, Revision>,
    /// The refused writes, by document name.
    refused: BTreeMap<String, Refused>,
    /// Answers to the writes of kept copies.
    copies: Arc<Inbox>,
    /// The answer key of the next kept copy.
    next_copy: u64,
    /// Choices that took effect, for the next [`poll`](Self::poll).
    settled: Vec<(String, Settled)>,
}

impl Revisions {
    /// Documents of `kind`, none read yet.
    pub(crate) fn new(kind: StoreKind) -> Self {
        Self {
            kind,
            // Every client writes its own newest list; a choice between two would only lose the
            // symbols just opened.
            versioned: (kind == StoreKind::RecentSymbols).then_some(false),
            read: HashMap::new(),
            refused: BTreeMap::new(),
            copies: Arc::default(),
            next_copy: 0,
            settled: Vec::new(),
        }
    }

    /// Reads `name`, answering `key` of `inbox`; [`loaded`](Self::loaded) takes the answer.
    pub(crate) fn load(&self, storage: &dyn Storage, name: &str, inbox: &Arc<Inbox>, key: u64) {
        if self.versioned == Some(false) {
            storage.load(self.kind, name, inbox.reply(key));
        } else {
            storage.load_versioned(self.kind, name, inbox.reply(key));
        }
    }

    /// The document a [`load`](Self::load) of `name` answered, `None` if there is none, with its
    /// revision remembered. Reading the stored version settles a refused write of it, as
    /// reloading does. Returns `None` once a host that keeps no revisions refused the versioned
    /// read: `name` is then read again with [`Storage::load`], answering `key` of `inbox`.
    pub(crate) fn loaded(
        &mut self,
        storage: &dyn Storage,
        name: &str,
        answer: Answer,
        inbox: &Arc<Inbox>,
        key: u64,
    ) -> Option<Result<Option<String>, FeedError>> {
        let answer = match answer.downcast::<Result<Option<Versioned>, FeedError>>() {
            Ok(answer) => *answer,
            Err(answer) => {
                return Some(
                    *answer
                        .downcast::<Result<Option<String>, FeedError>>()
                        .expect("document load"),
                );
            }
        };
        match answer {
            // Several reads may be in flight when the first refusal arrives.
            Err(FeedError::NotSupported) if self.versioned != Some(true) => {
                self.versioned = Some(false);
                self.load(storage, name, inbox, key);
                None
            }
            Ok(stored) => {
                self.versioned = Some(true);
                if self
                    .refused
                    .get(name)
                    .is_some_and(|refused| refused.copy.is_none())
                {
                    self.refused.remove(name);
                }
                Some(Ok(self.adopt(name, stored)))
            }
            Err(error) => Some(Err(error)),
        }
    }

    /// Writes `document` as `name` over the revision read, or as a new document when none was
    /// read, answering `key` of `inbox`; [`saved`](Self::saved) takes the answer.
    pub(crate) fn save(
        &self,
        storage: &dyn Storage,
        name: &str,
        document: String,
        inbox: &Arc<Inbox>,
        key: u64,
    ) {
        if self.versioned == Some(false) {
            storage.save(self.kind, name, document, inbox.reply(key));
        } else {
            let expected = self.read.get(name).cloned();
            storage.save_versioned(self.kind, name, document, expected, inbox.reply(key));
        }
    }

    /// What became of the write of `document` as `name` that a [`save`](Self::save) answered,
    /// with the new revision remembered. Returns `None` once a host that keeps no revisions
    /// refused the versioned write: it is then sent again with [`Storage::save`], answering `key`
    /// of `inbox`.
    pub(crate) fn saved(
        &mut self,
        storage: &dyn Storage,
        name: &str,
        document: &str,
        answer: Answer,
        inbox: &Arc<Inbox>,
        key: u64,
    ) -> Option<Written> {
        let answer = match answer.downcast::<Result<SaveOutcome, FeedError>>() {
            Ok(answer) => *answer,
            Err(answer) => {
                let answer = *answer
                    .downcast::<Result<(), FeedError>>()
                    .expect("document save");
                return Some(answer.map_or_else(Written::Failed, |()| Written::Saved));
            }
        };
        match answer {
            Err(FeedError::NotSupported) if self.versioned != Some(true) => {
                self.versioned = Some(false);
                self.save(storage, name, document.to_owned(), inbox, key);
                None
            }
            Ok(SaveOutcome::Saved(revision)) => {
                self.versioned = Some(true);
                self.read.insert(name.to_owned(), revision);
                Some(Written::Saved)
            }
            Ok(SaveOutcome::Conflict(stored)) => {
                self.versioned = Some(true);
                Some(Written::Refused(stored))
            }
            Err(error) => Some(Written::Failed(error)),
        }
    }

    /// Keeps `draft`, the document the host refused to write as `name` over `stored`, until the
    /// user's choice for it is given to [`resolve`](Self::resolve).
    pub(crate) fn refuse(&mut self, name: &str, draft: String, stored: Option<Versioned>) {
        let refused = Refused {
            draft,
            stored,
            copy: None,
        };
        self.refused.insert(name.to_owned(), refused);
    }

    /// Makes `document` the unsaved document of `name` if a write of it was refused, returning
    /// whether it was: the newest document then waits for the choice in place of the older one.
    pub(crate) fn hold(&mut self, name: &str, document: &str) -> bool {
        let Some(refused) = self.refused.get_mut(name) else {
            return false;
        };
        document.clone_into(&mut refused.draft);
        true
    }

    /// Whether a write of `name` waits for the user's choice or for its kept copy.
    pub(crate) fn refused(&self, name: &str) -> bool {
        self.refused.contains_key(name)
    }

    /// Whether any write waits for the user's choice or for its kept copy.
    pub(crate) fn unsettled(&self) -> bool {
        !self.refused.is_empty()
    }

    /// The names whose refused write waits for the user's choice, in order, without those whose
    /// kept copy is being written.
    pub(crate) fn conflicts(&self) -> impl Iterator<Item = &str> {
        self.refused
            .iter()
            .filter(|(_, refused)| refused.copy.is_none())
            .map(|(name, _)| name.as_str())
    }

    /// Forgets the revision of `name`, after it was deleted or its stored version could not be
    /// read, and its refused write unless a copy of it is being kept: its next write creates it,
    /// and is refused while a document of that name exists.
    pub(crate) fn forget(&mut self, name: &str) {
        self.read.remove(name);
        if self
            .refused
            .get(name)
            .is_some_and(|refused| refused.copy.is_none())
        {
            self.refused.remove(name);
        }
    }

    /// Settles the refused write of `name` as the user chose; [`poll`](Self::poll) returns the
    /// result. Keeping a copy writes it first. Does nothing while no write of `name` waits for a
    /// choice.
    pub(crate) fn resolve(&mut self, storage: &dyn Storage, name: &str, choice: ConflictChoice) {
        let Some(refused) = self
            .refused
            .get_mut(name)
            .filter(|refused| refused.copy.is_none())
        else {
            return;
        };
        if choice == ConflictChoice::KeepLocalCopy {
            refused.copy = Some(LocalCopy {
                key: self.next_copy,
                number: 1,
                draft: refused.draft.clone(),
                revision: None,
            });
            self.next_copy += 1;
            self.write_copy(storage, name);
            return;
        }
        let refused = self.refused.remove(name).expect("refused write");
        let stored = self.adopt(name, refused.stored);
        let settled = match choice {
            ConflictChoice::Retry => Settled::Overwrite(refused.draft),
            _ => Settled::Reload(stored),
        };
        self.settled.push((name.to_owned(), settled));
    }

    /// The choices that took effect since the last call, finishing the kept copies the host
    /// answered, and wakes the UI on the next answer.
    pub(crate) fn poll(
        &mut self,
        storage: &dyn Storage,
        ctx: &egui::Context,
    ) -> Vec<(String, Settled)> {
        let repaint = ctx.clone();
        self.copies.set_waker(move || repaint.request_repaint());
        loop {
            let answers = self.copies.drain().answers;
            if answers.is_empty() {
                break;
            }
            for (key, answer) in answers {
                let name = self.refused.iter().find_map(|(name, refused)| {
                    let copy = refused.copy.as_ref()?;
                    (copy.key == key).then(|| name.clone())
                });
                if let Some(name) = name {
                    self.copied(storage, &name, answer);
                }
            }
        }
        std::mem::take(&mut self.settled)
    }

    /// Writes the kept copy of `name` under the name of its number: created by its first write,
    /// so that no document of that name is replaced, and replaced only over its own revision.
    fn write_copy(&self, storage: &dyn Storage, name: &str) {
        let copy = self.refused[name].copy.as_ref().expect("copy in progress");
        storage.save_versioned(
            self.kind,
            &copy_name(name, copy.number),
            copy.draft.clone(),
            copy.revision.clone(),
            self.copies.reply(copy.key),
        );
    }

    /// Finishes a write of the kept copy of `name`, as a stored document's copy is finished: a
    /// newer draft is written over the copy, a name another document holds moves the copy to the
    /// next number, and once the copy holds the newest draft the stored version is taken.
    fn copied(&mut self, storage: &dyn Storage, name: &str, answer: Answer) {
        let answer = *answer
            .downcast::<Result<SaveOutcome, FeedError>>()
            .expect("copy save");
        let refused = self.refused.get_mut(name).expect("refused write");
        let copy = refused.copy.as_mut().expect("copy in progress");
        match answer {
            Ok(SaveOutcome::Saved(revision)) if copy.draft != refused.draft => {
                copy.draft.clone_from(&refused.draft);
                copy.revision = Some(revision);
                self.write_copy(storage, name);
            }
            Ok(SaveOutcome::Saved(revision)) => {
                let kept = copy_name(name, copy.number);
                self.read.insert(kept.clone(), revision);
                let refused = self.refused.remove(name).expect("refused write");
                let stored = self.adopt(name, refused.stored);
                self.settled.push((name.to_owned(), Settled::Copied(kept)));
                self.settled
                    .push((name.to_owned(), Settled::Reload(stored)));
            }
            Ok(SaveOutcome::Conflict(_)) if copy.number < COPY_NAMES => {
                copy.number += 1;
                copy.revision = None;
                copy.draft.clone_from(&refused.draft);
                self.write_copy(storage, name);
            }
            outcome => {
                refused.copy = None;
                let reason = match outcome {
                    Err(error) => error.to_string(),
                    _ => format!("{COPY_NAMES} copies exist already"),
                };
                self.settled
                    .push((name.to_owned(), Settled::CopyFailed(reason)));
            }
        }
    }

    /// Remembers the revision of `stored`, the version of `name` just read, and returns its
    /// document.
    fn adopt(&mut self, name: &str, stored: Option<Versioned>) -> Option<String> {
        match stored {
            Some(stored) => {
                self.read.insert(name.to_owned(), stored.revision);
                Some(stored.document)
            }
            None => {
                self.read.remove(name);
                None
            }
        }
    }
}
