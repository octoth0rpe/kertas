//! Loading images from the web (#97), once the user allows it for a
//! document.
//!
//! GPUI loads `https://` images through the app's HTTP client, which does
//! nothing until one is set. This one fetches with the system's URL
//! loading (`NSURLSession`), so it follows the system's proxy and
//! certificate settings, and keeps what it fetched so printing can use it.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};

use block2::RcBlock;
use futures::channel::oneshot;
use futures::future::BoxFuture;
use gpui::App;
use gpui::http_client::http::HeaderValue;
use gpui::http_client::{AsyncBody, HttpClient, Request, Response, Uri, Url, anyhow};
use kertas::links;
use kertas::model::Model;
use objc2_foundation::{
    NSData, NSError, NSHTTPURLResponse, NSString, NSURL, NSURLResponse, NSURLSession,
};

/// Makes GPUI fetch web images with [`SystemHttpClient`].
pub fn init(cx: &mut App) {
    cx.set_http_client(Arc::new(SystemHttpClient));
}

/// The web images fetched so far, by URL.
fn fetched() -> &'static Mutex<HashMap<String, Arc<[u8]>>> {
    static FETCHED: OnceLock<Mutex<HashMap<String, Arc<[u8]>>>> = OnceLock::new();
    FETCHED.get_or_init(Mutex::default)
}

/// The image fetched from `url`, if it has been.
pub fn fetched_image(url: &str) -> Option<Arc<[u8]>> {
    fetched().lock().unwrap().get(&normalized(url)).cloned()
}

/// Which of the web images in `model` have been fetched.
pub fn fetched_in(model: &Model) -> HashSet<String> {
    let mut urls = HashSet::new();
    for item in &model.items {
        item.rendered.each_text(&mut |text| {
            for image in &text.images {
                let url = image.destination.trim();
                if links::is_web(url) && fetched_image(url).is_some() {
                    urls.insert(url.to_owned());
                }
            }
        });
    }
    urls
}

/// `url` as GPUI asks for it, which may differ in small ways from how
/// it's written (`HTTPS:` for `https:`, say).
fn normalized(url: &str) -> String {
    url.parse::<Uri>()
        .map_or_else(|_| url.to_owned(), |uri| uri.to_string())
}

/// An HTTP client for GPUI that only GETs, which is all images need.
struct SystemHttpClient;

impl HttpClient for SystemHttpClient {
    fn user_agent(&self) -> Option<&HeaderValue> {
        None
    }

    fn proxy(&self) -> Option<&Url> {
        None
    }

    fn send(
        &self,
        request: Request<AsyncBody>,
    ) -> BoxFuture<'static, gpui::http_client::Result<Response<AsyncBody>>> {
        let url = request.uri().to_string();
        let (sender, receiver) = oneshot::channel::<Result<(u16, Vec<u8>), String>>();
        let sender = Mutex::new(Some(sender));
        let Some(ns_url) = NSURL::URLWithString(&NSString::from_str(&url)) else {
            return Box::pin(async move { Err(anyhow!("not a URL: {url}")) });
        };
        let handler = RcBlock::new(
            move |data: *mut NSData, response: *mut NSURLResponse, error: *mut NSError| {
                // SAFETY: NSURLSession passes valid objects or null.
                let (data, response, error) =
                    unsafe { (data.as_ref(), response.as_ref(), error.as_ref()) };
                let result = match (error, data) {
                    (Some(error), _) => Err(error.localizedDescription().to_string()),
                    (None, None) => Err("no data".to_owned()),
                    (None, Some(data)) => {
                        let status = response
                            .and_then(|response| response.downcast_ref::<NSHTTPURLResponse>())
                            .map_or(200, |response| response.statusCode() as u16);
                        Ok((status, data.to_vec()))
                    }
                };
                if let Some(sender) = sender.lock().unwrap().take() {
                    sender.send(result).ok();
                }
            },
        );
        // SAFETY: the handler only uses thread-safe state, so it can run on
        // the session's queue.
        let task = unsafe {
            NSURLSession::sharedSession().dataTaskWithURL_completionHandler(&ns_url, &handler)
        };
        task.resume();
        Box::pin(async move {
            let (status, body) = receiver
                .await
                .map_err(|_| anyhow!("loading {url} was cancelled"))?
                .map_err(|error| anyhow!("loading {url}: {error}"))?;
            if (200..300).contains(&status) {
                fetched()
                    .lock()
                    .unwrap()
                    .insert(url, Arc::from(body.as_slice()));
            }
            Ok(Response::builder()
                .status(status)
                .body(AsyncBody::from(body))?)
        })
    }
}
