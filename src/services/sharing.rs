//! Sharing of layouts and watchlists through the host's own accounts and storage.

use crate::data::{MaybeSend, Reply};

/// What a [`Sharing`] service shares.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ShareKind {
    /// A named chart layout, as kept in [`StoreKind::Layouts`](super::StoreKind::Layouts).
    Layout,
    /// A watchlist, by its [`Watchlist::id`](crate::model::Watchlist::id).
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

/// What the recipients of a shared document may do with it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum ShareAccess {
    /// Recipients may only view the document.
    #[default]
    ViewOnly,
    /// Recipients may view the document and save a copy of it.
    ViewAndCopy,
}

/// The sharing operations a [`Sharing`] service offers for one [`ShareKind`].
///
/// ```
/// use egui_charts::services::{ShareAccess, ShareOffer};
/// let mut offer = ShareOffer::new(true, false);
/// offer.revoke = true;
/// offer.switch = true;
/// assert!(offer.link && offer.revoke && offer.switch && !offer.copy);
/// assert_eq!(ShareAccess::default(), ShareAccess::ViewOnly);
/// ```
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ShareOffer {
    /// Whether [`Sharing::link`] is available.
    pub link: bool,
    /// Whether [`Sharing::copy`] is available.
    pub copy: bool,
    /// Whether menus offer [`Sharing::revoke`] on a document reported as shared.
    pub revoke: bool,
    /// Whether menus show a switch that shares the document through [`Sharing::share`] and stops
    /// sharing it through [`Sharing::revoke`], with a choice of [`ShareAccess`], and offer Copy
    /// link only while [`Sharing::shared`] reports the document shared. Without it the menus keep
    /// their Copy link, Save a copy and Stop sharing rows.
    pub switch: bool,
}

impl ShareOffer {
    /// An offer of the operations the flags name.
    pub fn new(link: bool, copy: bool) -> Self {
        Self {
            link,
            copy,
            revoke: false,
            switch: false,
        }
    }
}

/// Sharing a layout or watchlist, supplied by a host that has accounts and somewhere to keep
/// shared documents. The library owns neither: it shows the controls the host offers and reports
/// the host's answers. It shows shared state and can ask the host to stop sharing. Without a
/// service the layout menu and the watchlist panel offer no sharing.
///
/// Methods are called on the UI thread and return at once; answer through the [`Reply`] you were
/// given, from any thread, runtime or browser task. Answer an error, such as
/// [`FeedError::Other`](crate::data::FeedError::Other) with the reason, to reject an operation:
/// the menu shows it. The methods that act on one document name it by `document`: a layout by
/// its name, a watchlist by its [`Watchlist::id`](crate::model::Watchlist::id), which stays
/// through renames and differs between lists of the same name.
pub trait Sharing: MaybeSend + 'static {
    /// Which operations to offer for `kind`. The default offers none.
    fn offers(&self, kind: ShareKind) -> ShareOffer {
        let _ = kind;
        ShareOffer::default()
    }

    /// Whether `document` of `kind` is currently shared by link. Menus read this every frame,
    /// so answer from memory. The default reports no shared state.
    fn shared(&self, kind: ShareKind, document: &str) -> bool {
        let _ = (kind, document);
        false
    }

    /// Revokes the link for `document` of `kind`. Answer success once [`shared`](Self::shared)
    /// reports false, or an error for the menu to display. Unsupported by default.
    fn revoke(&self, kind: ShareKind, document: &str, reply: Reply<()>) {
        let _ = (kind, document);
        reply.send(Err(crate::data::FeedError::NotSupported));
    }

    /// What the recipients of the shared `document` of `kind` may do. Read every frame the
    /// menu is open while [`ShareOffer::switch`] is offered and [`shared`](Self::shared) reports
    /// the document shared, so answer from memory. The default is [`ShareAccess::ViewOnly`].
    fn access(&self, kind: ShareKind, document: &str) -> ShareAccess {
        let _ = (kind, document);
        ShareAccess::ViewOnly
    }

    /// Shares `document` of `kind` with `access`, or changes the access of a document
    /// already shared. Answer success once [`shared`](Self::shared) reports it shared and
    /// [`access`](Self::access) reports `access`, or an error for the menu to display. Links
    /// answered by [`link`](Self::link) afterwards say whether a copy is allowed. Unsupported by
    /// default.
    fn share(&self, kind: ShareKind, document: &str, access: ShareAccess, reply: Reply<()>) {
        let _ = (kind, document, access);
        reply.send(Err(crate::data::FeedError::NotSupported));
    }

    /// A view-only link to `document` of `kind`. The library copies the link to the clipboard
    /// and tells the user whether a recipient may save a copy.
    fn link(&self, kind: ShareKind, document: &str, reply: Reply<ShareLink>) {
        let _ = (kind, document);
        reply.send(Err(crate::data::FeedError::NotSupported));
    }

    /// Saves a copy of `document` of `kind` for the user and answers the copy's name. A layout
    /// copy is read back from [`Storage`](super::Storage), so the host writes it there; a
    /// watchlist copy is added to the panel's lists under the answered name, with an id of its
    /// own.
    fn copy(&self, kind: ShareKind, document: &str, reply: Reply<String>) {
        let _ = (kind, document);
        reply.send(Err(crate::data::FeedError::NotSupported));
    }
}
