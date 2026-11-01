//! Content of the posts and ideas that drawings link to.

use crate::data::{MaybeSend, Reply};
use crate::model::PostContent;

/// Host service that resolves the link of a post or idea drawing into its content. Attach with
/// `Chart::set_post_source`.
///
/// Called on the UI thread; return promptly and answer through the reply from any thread,
/// runtime or browser task. The chart never fetches a link itself. It asks once for each link
/// whose content it lacks, gives the answer to every drawing with that link and saves it with
/// them. An error, or a reply dropped unanswered, shows the link as not loaded until the source
/// is set again or the drawing is loaded again.
pub trait Posts: MaybeSend + 'static {
    /// The content `link` names: an address the user typed, or the host's reference to a post
    /// or idea, such as a feed entry identifier. It may be any text; check it before fetching.
    fn resolve(&self, link: &str, reply: Reply<PostContent>);
}
