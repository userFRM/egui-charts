//! Sharing of layouts and watchlists through the host's own accounts and storage.

use crate::data::{MaybeSend, Reply};

/// What a [`Sharing`] service shares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ShareKind {
    /// A named chart layout, as kept in [`StoreKind::Layouts`](super::StoreKind::Layouts).
    Layout,
    /// A watchlist, by its name.
    Watchlist,
}

/// A view-only link to a shared document and what its recipient may do with it.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub struct ShareLink {
    /// The link, which opens the document for viewing only.
    pub url: String,
    /// Whether a recipient may save a copy of the document.
    pub copy_allowed: bool,
}

impl ShareLink {
    /// A view-only link to `url`, whose recipient may save a copy when `copy_allowed`.
    pub fn new(url: impl Into<String>, copy_allowed: bool) -> Self {
        Self {
            url: url.into(),
            copy_allowed,
        }
    }
}

/// The sharing operations a [`Sharing`] service offers for one [`ShareKind`].
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ShareOffer {
    /// Whether [`Sharing::link`] is available.
    pub link: bool,
    /// Whether [`Sharing::copy`] is available.
    pub copy: bool,
}

impl ShareOffer {
    /// An offer of the operations the flags name.
    pub fn new(link: bool, copy: bool) -> Self {
        Self { link, copy }
    }
}

/// Sharing a layout or watchlist, supplied by a host that has accounts and somewhere to keep
/// shared documents. The library owns neither: it shows the controls the host offers and reports
/// the host's answers. Without a service the layout menu and the watchlist panel offer no sharing.
///
/// Methods are called on the UI thread and return at once; answer through the [`Reply`] you were
/// given, from any thread, runtime or browser task. Answer an error, such as
/// [`FeedError::Other`](crate::data::FeedError::Other) with the reason, to reject an operation:
/// the menu shows it. The documents are the ones the host already holds under `name`.
pub trait Sharing: MaybeSend + 'static {
    /// Which operations to offer for `kind`. The default offers none.
    fn offers(&self, kind: ShareKind) -> ShareOffer {
        let _ = kind;
        ShareOffer::default()
    }

    /// A view-only link to the document `name` of `kind`. The library copies the link to the
    /// clipboard and tells the user whether a recipient may save a copy.
    fn link(&self, kind: ShareKind, name: &str, reply: Reply<ShareLink>) {
        let _ = (kind, name);
        reply.send(Err(crate::data::FeedError::NotSupported));
    }

    /// Saves a copy of the document `name` of `kind` for the user and answers the copy's name.
    /// A layout copy is read back from [`Storage`](super::Storage), so the host writes it
    /// there; a watchlist copy is added to the panel's lists under the answered name.
    fn copy(&self, kind: ShareKind, name: &str, reply: Reply<String>) {
        let _ = (kind, name);
        reply.send(Err(crate::data::FeedError::NotSupported));
    }
}
