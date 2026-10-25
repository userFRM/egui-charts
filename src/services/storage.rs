//! Named text documents kept by the host on behalf of the library.

use crate::data::{MaybeSend, Reply};

/// What a stored document holds. Each kind is its own namespace of names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum StoreKind {
    /// The user's settings: theme, last symbol and interval, drawing toolbar favourites. One
    /// document.
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
    /// Recently opened instruments, newest first.
    RecentSymbols,
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
}
