//! Named text documents kept by the host on behalf of the library.

use super::Events;
use crate::data::{FeedError, MaybeSend, Reply};

/// What a stored document holds. Each kind is its own namespace of names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StoreKind {
    /// The user's settings: theme, last symbol and interval, drawing toolbar favourites and the
    /// open layout. One document, named `settings`.
    Settings,
    /// The drawings of one symbol, one document per symbol, named by the symbol.
    Drawings,
    /// Named alert presets in a versioned JSON document.
    AlertPresets,
    /// Named chart display settings templates.
    ChartTemplates,
    /// Named sets of study inputs and presentation settings.
    StudyTemplates,
    /// Named drawing property templates, without chart anchors.
    DrawingTemplates,
    /// The user's notes on one symbol, one document per symbol, named by the symbol.
    SymbolNotes,
    /// Named chart layouts: the arrangement of the charts and the symbol, interval, chart type,
    /// studies, panes, scales, settings and drawings of each.
    Layouts,
    /// Recently opened instruments, newest first.
    RecentSymbols,
    /// Named screener screens: the universe, column set, filters, sort order and description.
    Screens,
}

/// The host's revision of a stored document, such as a version counter, an entity tag or a
/// content hash. The library compares revisions for equality only and hands them back unchanged.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Revision(
    /// The host's own token, different for every stored version of a document.
    pub String,
);

/// A stored document with the revision it was read at.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Versioned {
    /// The document as stored.
    pub document: String,
    /// Its revision.
    pub revision: Revision,
}

/// The host's answer to [`Storage::save_versioned`].
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum SaveOutcome {
    /// The document was written, at this revision.
    Saved(Revision),
    /// Nothing was written because the stored document is not at the expected revision. Carries
    /// the stored version so that the user can load it, or `None` if it was deleted meanwhile.
    Conflict(Option<Versioned>),
}

/// How the user settles a refused save. Until one is chosen the unsaved document stays in memory
/// and nothing is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum ConflictChoice {
    /// Load the stored version and drop the unsaved changes.
    ReloadRemote,
    /// Save the unsaved changes as a separate document named after the original with
    /// " (local copy)" appended, then load the stored version.
    KeepLocalCopy,
    /// Write the unsaved changes over the stored version, which is now the expected revision.
    Retry,
}

/// Where the library keeps settings and drawings between sessions: files, browser storage or a
/// server, as the host chooses. The library reads and writes nothing on its own.
///
/// Documents are JSON text with a version field that the library writes and migrates. Store them
/// verbatim and hand them back unchanged. Names come from the library (a drawings document is named
/// by its symbol) and may contain any character, `/` and `..` included: a host that keeps documents
/// as files must encode the name rather than use it as a path, and `list` returns the names as they
/// were saved.
///
/// These methods are called on the UI thread and must return at once. Do the work elsewhere and
/// answer through the [`Reply`] you were given, from any thread, runtime or browser task. A failed
/// `save` keeps the state in memory and is reported to the user once, so answer with an error
/// rather than dropping a document. The library waits for the answer to one call before it makes
/// another for the same document, so different calls may be worked on concurrently. Renaming is a
/// `save` under the new name and, once that succeeds, a `delete` of the old one.
///
/// A host shared by several clients can also keep revisions, with
/// [`load_versioned`](Storage::load_versioned), [`save_versioned`](Storage::save_versioned) and
/// [`watch`](Storage::watch). The library then writes a document only over the revision it read,
/// so a client never overwrites a newer version unnoticed: a refused save keeps the unsaved
/// document in memory and is settled with a [`ConflictChoice`]. A host that answers
/// `load_versioned` with [`FeedError::NotSupported`], as the default does, keeps the plain
/// behaviour above. Switching accounts means a new `Storage`; the library drops the old one's
/// pending answers and streams, so nothing of the old account reaches the new one.
///
/// ```
/// use std::collections::HashMap;
/// use std::sync::Mutex;
///
/// use egui_charts::data::Reply;
/// use egui_charts::services::{Storage, StoreKind};
///
/// /// Documents held in memory, lost when the application exits. Answers inline.
/// #[derive(Default)]
/// struct InMemory(Mutex<HashMap<(StoreKind, String), String>>);
///
/// impl Storage for InMemory {
///     fn list(&self, kind: StoreKind, reply: Reply<Vec<String>>) {
///         let documents = self.0.lock().unwrap();
///         let names = documents
///             .keys()
///             .filter(|(k, _)| *k == kind)
///             .map(|(_, name)| name.clone())
///             .collect();
///         drop(documents);
///         reply.send(Ok(names));
///     }
///
///     fn load(&self, kind: StoreKind, name: &str, reply: Reply<Option<String>>) {
///         let document = self.0.lock().unwrap().get(&(kind, name.to_owned())).cloned();
///         reply.send(Ok(document));
///     }
///
///     fn save(&self, kind: StoreKind, name: &str, document: String, reply: Reply<()>) {
///         self.0.lock().unwrap().insert((kind, name.to_owned()), document);
///         reply.send(Ok(()));
///     }
///
///     fn delete(&self, kind: StoreKind, name: &str, reply: Reply<()>) {
///         self.0.lock().unwrap().remove(&(kind, name.to_owned()));
///         reply.send(Ok(()));
///     }
/// }
///
/// let storage: std::sync::Arc<dyn Storage> = std::sync::Arc::new(InMemory::default());
/// # let _ = storage;
/// ```
pub trait Storage: MaybeSend + 'static {
    /// The names of the stored documents of `kind`, in any order.
    fn list(&self, kind: StoreKind, reply: Reply<Vec<String>>);

    /// The document `name` of `kind`. Answer `Ok(None)` only when you know there is no such
    /// document, and an error whenever it cannot be read, including when you cannot tell whether
    /// it exists, so the library never saves over it.
    fn load(&self, kind: StoreKind, name: &str, reply: Reply<Option<String>>);

    /// Stores `document` as `name` of `kind`, replacing any document of that name. Answer `Ok`
    /// only once it is kept.
    fn save(&self, kind: StoreKind, name: &str, document: String, reply: Reply<()>);

    /// Removes the document `name` of `kind`. Removing a document that does not exist succeeds.
    fn delete(&self, kind: StoreKind, name: &str, reply: Reply<()>);

    /// Like [`load`](Self::load), with the revision of the document. Answer
    /// [`FeedError::NotSupported`] if the host keeps no revisions; the library then uses `load`
    /// and `save` for that document. By default every call is refused so.
    fn load_versioned(&self, kind: StoreKind, name: &str, reply: Reply<Option<Versioned>>) {
        let _ = (kind, name);
        reply.send(Err(FeedError::NotSupported));
    }

    /// Stores `document` only if the stored one is at revision `expected`, or does not exist when
    /// `expected` is `None`. Answer [`SaveOutcome::Saved`] with the new revision once it is kept.
    /// If the stored document differs, change nothing and answer [`SaveOutcome::Conflict`] with
    /// the stored version. The check and the write must be one atomic step.
    fn save_versioned(
        &self,
        kind: StoreKind,
        name: &str,
        document: String,
        expected: Option<Revision>,
        reply: Reply<SaveOutcome>,
    ) {
        let _ = (kind, name, document, expected);
        reply.send(Err(FeedError::NotSupported));
    }

    /// Pushes the new revision each time another client changes the document `name` of `kind`,
    /// until `events` is inactive. Writes of this client may be pushed too; the library ignores
    /// the revisions it knows. Called after the document was read, only for the library's own
    /// documents that follow remote changes, and by default pushes nothing.
    fn watch(&self, kind: StoreKind, name: &str, events: Events<Revision>) {
        let _ = (kind, name, events);
    }
}
