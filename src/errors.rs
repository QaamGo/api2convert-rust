//! The typed error hierarchy. A single [`Api2ConvertError`] enum, matched with
//! `match` / `matches!` (the Rust analog of the sibling SDKs' exception classes
//! matched by type). Every fallible SDK call returns [`Result`].
//!
//! **No secret ever appears in an error message** — messages come from the API
//! response body or a status description, never from a header (API key, upload
//! token, download password) or a URL that might carry a signed token.

use serde_json::Value;

use crate::models::{Job, JobMessage};

/// Shorthand for a `Result` whose error is [`Api2ConvertError`].
pub type Result<T> = std::result::Result<T, Api2ConvertError>;

/// Metadata shared by every HTTP error (status ≥ 400).
#[derive(Debug, Clone)]
pub struct ApiErrorData {
    /// The HTTP status code.
    pub status: u16,
    /// A human-readable message (from the response body's `message`, else a
    /// description of the status).
    pub message: String,
    /// The `X-Request-Id` response header, if present — quote it in support requests.
    pub request_id: Option<String>,
    /// The decoded response body (`Value::Null` if it was empty / not JSON).
    pub body: Value,
}

/// Every error the SDK can produce.
#[derive(Debug)]
#[non_exhaustive]
pub enum Api2ConvertError {
    /// Invalid SDK usage or configuration surfaced before/without a network
    /// call (e.g. a missing API key, or an output index out of range).
    Config(String),
    /// A transport failure (DNS/connection/TLS/timeout), a non-JSON 2xx body,
    /// or a redirect that was not followed because the request carried a secret.
    Network(String),
    /// 401 / 403 — authentication failed.
    Authentication(ApiErrorData),
    /// 402 — payment required.
    PaymentRequired(ApiErrorData),
    /// 404 — not found.
    NotFound(ApiErrorData),
    /// 400 / 422 — request validation failed.
    Validation(ApiErrorData),
    /// 429 — rate limited. `retry_after` is the raw (uncapped) `Retry-After` in
    /// seconds, if the server sent one.
    RateLimit {
        data: ApiErrorData,
        retry_after: Option<i64>,
    },
    /// 5xx — server error.
    Server(ApiErrorData),
    /// Any other 4xx — a generic API error.
    Api(ApiErrorData),
    /// The job reached `failed` / `canceled`. Carries the job and its `errors[]`.
    ConversionFailed {
        job: Box<Job>,
        errors: Vec<JobMessage>,
    },
    /// The poll deadline elapsed before the job became terminal.
    ConversionTimeout { job: Box<Job> },
    /// Webhook signature verification failed (missing/wrong signature, or an
    /// unparseable payload).
    SignatureVerification(String),
}

impl Api2ConvertError {
    pub(crate) fn api(
        status: u16,
        message: String,
        request_id: Option<String>,
        body: Value,
    ) -> Self {
        let data = ApiErrorData {
            status,
            message,
            request_id,
            body,
        };
        match status {
            401 | 403 => Api2ConvertError::Authentication(data),
            402 => Api2ConvertError::PaymentRequired(data),
            404 => Api2ConvertError::NotFound(data),
            400 | 422 => Api2ConvertError::Validation(data),
            429 => Api2ConvertError::RateLimit {
                data,
                retry_after: None,
            },
            500..=599 => Api2ConvertError::Server(data),
            _ => Api2ConvertError::Api(data),
        }
    }

    fn http_data(&self) -> Option<&ApiErrorData> {
        match self {
            Api2ConvertError::Authentication(d)
            | Api2ConvertError::PaymentRequired(d)
            | Api2ConvertError::NotFound(d)
            | Api2ConvertError::Validation(d)
            | Api2ConvertError::Server(d)
            | Api2ConvertError::Api(d) => Some(d),
            Api2ConvertError::RateLimit { data, .. } => Some(data),
            _ => None,
        }
    }

    /// The HTTP status code, for the HTTP-error variants.
    pub fn status(&self) -> Option<u16> {
        self.http_data().map(|d| d.status)
    }

    /// The captured `X-Request-Id`, for the HTTP-error variants.
    pub fn request_id(&self) -> Option<&str> {
        self.http_data().and_then(|d| d.request_id.as_deref())
    }

    /// The decoded response body, for the HTTP-error variants.
    pub fn body(&self) -> Option<&Value> {
        self.http_data().map(|d| &d.body)
    }

    /// The raw (uncapped) `Retry-After` seconds — only for a [`RateLimit`](Self::RateLimit).
    pub fn retry_after(&self) -> Option<i64> {
        match self {
            Api2ConvertError::RateLimit { retry_after, .. } => *retry_after,
            _ => None,
        }
    }
}

impl std::fmt::Display for Api2ConvertError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Api2ConvertError::Config(m) => write!(f, "configuration error: {m}"),
            Api2ConvertError::Network(m) => write!(f, "network error: {m}"),
            Api2ConvertError::Authentication(d) => {
                write!(f, "authentication error ({}): {}", d.status, d.message)
            }
            Api2ConvertError::PaymentRequired(d) => {
                write!(f, "payment required ({}): {}", d.status, d.message)
            }
            Api2ConvertError::NotFound(d) => write!(f, "not found ({}): {}", d.status, d.message),
            Api2ConvertError::Validation(d) => {
                write!(f, "validation error ({}): {}", d.status, d.message)
            }
            Api2ConvertError::RateLimit { data, retry_after } => match retry_after {
                Some(s) => write!(
                    f,
                    "rate limited ({}): {} (retry after {}s)",
                    data.status, data.message, s
                ),
                None => write!(f, "rate limited ({}): {}", data.status, data.message),
            },
            Api2ConvertError::Server(d) => write!(f, "server error ({}): {}", d.status, d.message),
            Api2ConvertError::Api(d) => write!(f, "API error ({}): {}", d.status, d.message),
            Api2ConvertError::ConversionFailed { job, errors } => {
                let detail = errors
                    .first()
                    .map(|e| e.message.clone())
                    .or_else(|| job.status.info.clone())
                    .unwrap_or_else(|| format!("status {}", job.status.code));
                write!(f, "conversion failed (job {}): {}", job.id, detail)
            }
            Api2ConvertError::ConversionTimeout { job } => {
                write!(
                    f,
                    "conversion timed out (job {}, last status {})",
                    job.id, job.status.code
                )
            }
            Api2ConvertError::SignatureVerification(m) => {
                write!(f, "signature verification failed: {m}")
            }
        }
    }
}

impl std::error::Error for Api2ConvertError {}
