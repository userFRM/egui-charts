//! Posts and ideas that post and idea drawings link to.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The content of a post or idea, as the host resolves a link to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PostContent {
    /// The title of an idea; empty for a post, whose card then leads with the author.
    #[serde(default)]
    pub title: String,
    /// Who published it.
    pub author: String,
    /// When it was published.
    pub time: DateTime<Utc>,
    /// The text.
    pub text: String,
    /// The address of its picture, such as an idea's chart image, loaded through the host's egui
    /// image loaders as an image drawing's address is; `None` without one. The chart loads it
    /// through those loaders only, so answer with addresses they should fetch.
    #[serde(default)]
    pub picture: Option<String>,
}

impl PostContent {
    /// A post by `author`, published at `time`, with `text` and no title or picture.
    pub fn new(author: impl Into<String>, time: DateTime<Utc>, text: impl Into<String>) -> Self {
        Self {
            title: String::new(),
            author: author.into(),
            time,
            text: text.into(),
            picture: None,
        }
    }
}

/// The post or idea a drawing links to, and its content once the host resolved it.
///
/// A drawing saves both, so a saved layout shows the content without asking the host again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct PostLink {
    /// The address of the post or idea, or the host's reference to it, such as a feed entry
    /// identifier.
    pub link: String,
    /// What the host resolved the link to, or `None` while it has not.
    #[serde(default)]
    pub content: Option<PostContent>,
    /// The host could not resolve the link, or the chart has no service to ask. Not saved, so
    /// the link is asked for again when the drawing is loaded.
    #[serde(skip)]
    pub(crate) failed: bool,
}

impl PostLink {
    /// `link`, waiting for its content.
    pub fn new(link: impl Into<String>) -> Self {
        Self {
            link: link.into(),
            content: None,
            failed: false,
        }
    }

    /// `link` with the content it resolves to, which the chart then does not ask for.
    pub fn resolved(link: impl Into<String>, content: PostContent) -> Self {
        Self {
            link: link.into(),
            content: Some(content),
            failed: false,
        }
    }
}
