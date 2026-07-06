//! The default [`HttpSender`], backed by `reqwest`'s blocking client.
//!
//! Redirect policy is client-level in `reqwest`, so this holds **two** clients:
//!
//! - `no_redirect` ([`Policy::none`]) never follows a redirect. It serves every
//!   secret-bearing request — the account key, the per-job upload token and the
//!   download password ride in custom `X-Oc-*` headers that an auto-following
//!   client could forward to a redirect target on another host. With this
//!   policy a 3xx comes back verbatim, and the transport turns it into an error.
//! - `follow` ([`Policy::limited`]) follows redirects, and serves **only** the
//!   self-contained, no-secret download path (storage/CDN URLs legitimately
//!   redirect).
//!
//! The choice is made per request from [`HttpRequest::follow_redirects`].
//!
//! Error messages here are sanitized: `reqwest`'s own `Display` can include the
//! request URL, and a download URL may carry a signed token in its query — so we
//! classify the failure kind and never echo the URL.

use std::time::Duration;

use reqwest::blocking::{Body, Client};
use reqwest::redirect::Policy;

use super::http_sender::{Headers, HttpRequest, HttpResponse, HttpSender};
use crate::errors::Api2ConvertError;

pub(crate) struct ReqwestSender {
    no_redirect: Client,
    follow: Client,
}

impl ReqwestSender {
    pub(crate) fn new(connect_timeout: Duration) -> Result<Self, Api2ConvertError> {
        let build = |policy: Policy| {
            Client::builder()
                .connect_timeout(connect_timeout)
                // Never attach a `Referer`. reqwest defaults it on, and on a
                // followed download redirect it would send the previous URL —
                // including a signed token in its query — to the (cross-host)
                // redirect target, leaking a bearer secret on the one path that
                // is allowed to follow redirects.
                .referer(false)
                .redirect(policy)
                .build()
                .map_err(|e| {
                    Api2ConvertError::Config(format!(
                        "failed to build HTTP client: {}",
                        err_kind(&e)
                    ))
                })
        };
        Ok(ReqwestSender {
            no_redirect: build(Policy::none())?,
            follow: build(Policy::limited(5))?,
        })
    }
}

impl HttpSender for ReqwestSender {
    fn send(&self, req: &HttpRequest) -> Result<HttpResponse, Api2ConvertError> {
        // Validate the URL up front so a malformed URI is a clean network error
        // (never a panic, never a raw parse error, never echoing a signed URL).
        reqwest::Url::parse(&req.url)
            .map_err(|_| Api2ConvertError::Network("invalid request URL".to_string()))?;

        let method = reqwest::Method::from_bytes(req.method.as_bytes())
            .map_err(|_| Api2ConvertError::Network("invalid HTTP method".to_string()))?;

        let client = if req.follow_redirects {
            &self.follow
        } else {
            &self.no_redirect
        };

        let mut rb = client.request(method, &req.url);
        if let Some(t) = req.timeout {
            rb = rb.timeout(t);
        }
        for (name, value) in &req.headers {
            rb = rb.header(name.as_str(), value.as_str());
        }
        if let Some(make_body) = &req.make_body {
            let reader = make_body().map_err(|_| {
                Api2ConvertError::Network("failed to open request body".to_string())
            })?;
            rb = rb.body(Body::new(reader));
        } else if let Some(bytes) = &req.body {
            rb = rb.body(bytes.clone());
        }

        let resp = rb
            .send()
            .map_err(|e| Api2ConvertError::Network(err_kind(&e)))?;

        let status = resp.status().as_u16();
        let mut headers = Headers::new();
        for (name, value) in resp.headers().iter() {
            if let Ok(v) = value.to_str() {
                headers.insert(name.as_str(), v);
            }
        }

        Ok(HttpResponse {
            status,
            headers,
            body: Box::new(resp),
        })
    }
}

/// A secret-free description of a `reqwest` error (never the URL).
fn err_kind(e: &reqwest::Error) -> String {
    if e.is_timeout() {
        "request timed out".to_string()
    } else if e.is_connect() {
        "connection failed".to_string()
    } else if e.is_redirect() {
        "too many redirects".to_string()
    } else if e.is_request() {
        "malformed request".to_string()
    } else if e.is_body() || e.is_decode() {
        "failed to read response body".to_string()
    } else {
        "transport error".to_string()
    }
}
