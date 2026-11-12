//! One HTTP request to a configured endpoint, on both targets.

use super::Failure;

/// One request with every placeholder filled. It has no `Debug`, so a key in its URL or its
/// headers cannot reach a log line by accident.
pub(crate) struct Request {
    /// Where the request goes.
    pub(crate) url: String,
    /// The headers of the profile, values filled. The client or the browser adds its own.
    pub(crate) headers: Vec<(String, String)>,
    /// `None` sends `GET`. A body is sent with `POST`.
    pub(crate) body: Option<String>,
}

/// Sends `request` and calls `done` once, with the text of the response body or with the reason
/// there is none. `done` runs on another thread natively and as a browser task on the web,
/// never inside this call, so a caller may hold a lock while it calls.
///
/// A status outside 200 to 299 is [`Failure::Status`]. A failed connection, no response within
/// ten seconds and a response the browser withholds are [`Failure::Refused`]. A body over 8 MiB
/// and a body that is not UTF-8 are [`Failure::Unreadable`].
pub(crate) fn fetch(
    _request: Request,
    _done: impl FnOnce(Result<String, Failure>) + Send + 'static,
) {
    unimplemented!("part A")
}
