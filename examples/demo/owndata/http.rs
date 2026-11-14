//! One HTTP request to a configured endpoint, on both targets.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use super::{Failure, after, endpoint};

/// How long a request waits for its response.
const DEADLINE: Duration = Duration::from_secs(10);

// shortcut: the 8 MiB cap is applied to the downloaded body, so a larger response is still
// transferred. Upgrade to a streamed read when a profile needs pages that large.
const LARGEST_BODY: usize = 8 << 20;

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
pub(crate) fn fetch(request: Request, done: impl FnOnce(Result<String, Failure>) + Send + 'static) {
    // Neither the client nor a browser fetch bounds the wait for the response, so the first of
    // the response and the deadline answers and the other is discarded.
    let done: Arc<Mutex<Option<Done>>> = Arc::new(Mutex::new(Some(Box::new(done))));
    let take =
        |done: &Mutex<Option<Done>>| done.lock().unwrap_or_else(PoisonError::into_inner).take();
    let answered = Arc::new(AtomicBool::new(false));
    let late = done.clone();
    after(DEADLINE, answered.clone(), move || {
        if let Some(done) = take(&late) {
            let seconds = DEADLINE.as_secs();
            done(Err(Failure::Refused(format!(
                "no response within {seconds} seconds"
            ))));
        }
    });
    send(request, move |result| {
        // Releases the timer.
        answered.store(true, Ordering::Release);
        if let Some(done) = take(&done) {
            done(result);
        }
    });
}

/// The caller's callback, which the response and the deadline race for.
type Done = Box<dyn FnOnce(Result<String, Failure>) + Send>;

/// The text of a response with `status`, whose body is `None` when it is not UTF-8.
fn text(status: u16, body: Option<String>) -> Result<String, Failure> {
    if !(200..=299).contains(&status) {
        return Err(Failure::Status(status));
    }
    let body = body.ok_or_else(|| Failure::Unreadable("the body is not UTF-8 text".to_owned()))?;
    if body.len() > LARGEST_BODY {
        return Err(Failure::Unreadable(
            "the body is larger than 8 MiB".to_owned(),
        ));
    }
    Ok(body)
}

/// Why no response arrived. A client's message can repeat the address, whose query may hold a
/// key, so the address is shown as its endpoint.
fn refused(reason: &str, url: &str) -> Failure {
    Failure::Refused(reason.replace(url, endpoint(url)))
}

/// Sends `request` and calls `respond` from the client's own thread.
#[cfg(not(target_arch = "wasm32"))]
fn send(request: Request, respond: impl FnOnce(Result<String, Failure>) + Send + 'static) {
    let Request { url, headers, body } = request;
    // Not `Request::post`, which adds a content type of its own. The headers are replaced as a
    // whole, because `Headers::insert` keeps a second header of the same name.
    let mut call = ehttp::Request::get(&url);
    call.headers = ehttp::Headers { headers };
    if let Some(body) = body {
        call.method = ehttp::Method::POST;
        call.body = body.into_bytes();
    }
    ehttp::fetch(call, move |result| {
        respond(match result {
            Ok(response) => text(response.status, String::from_utf8(response.bytes).ok()),
            Err(reason) => Err(refused(&reason, &url)),
        });
    });
}

/// Sends `request` and calls `respond` from a task of the page.
#[cfg(target_arch = "wasm32")]
fn send(request: Request, respond: impl FnOnce(Result<String, Failure>) + Send + 'static) {
    use wasm_bindgen::JsCast as _;
    wasm_bindgen_futures::spawn_local(async move {
        respond(match exchange(&request).await {
            Ok((status, body)) => text(status, body),
            Err(error) => {
                let thrown = error.dyn_ref::<web_sys::js_sys::Error>();
                let reason = thrown.map_or_else(
                    || error.as_string().unwrap_or_else(|| format!("{error:?}")),
                    |thrown| String::from(thrown.message()),
                );
                Err(refused(&reason, &request.url))
            }
        });
    });
}

/// The status and the body of the page's fetch of `request`, or what the browser threw: a
/// header name it refuses, or a response it could not get or withholds.
#[cfg(target_arch = "wasm32")]
async fn exchange(request: &Request) -> Result<(u16, Option<String>), wasm_bindgen::JsValue> {
    use wasm_bindgen::{JsCast as _, JsValue};
    use wasm_bindgen_futures::JsFuture;
    let init = web_sys::RequestInit::new();
    init.set_method(if request.body.is_some() {
        "POST"
    } else {
        "GET"
    });
    init.set_mode(web_sys::RequestMode::Cors);
    let headers = web_sys::Headers::new()?;
    for (name, value) in &request.headers {
        headers.set(name, value)?;
    }
    init.set_headers(&headers);
    if let Some(body) = &request.body {
        init.set_body(&JsValue::from_str(body));
    }
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("the page has no window"))?;
    let response = JsFuture::from(window.fetch_with_str_and_init(&request.url, &init)).await?;
    let response: web_sys::Response = response.dyn_into()?;
    let body = JsFuture::from(response.text()?).await?;
    Ok((response.status(), body.as_string()))
}
