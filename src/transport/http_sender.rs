//! The pluggable transport seam. The [`Transport`](super::Transport) builds a
//! library-agnostic [`HttpRequest`] and hands it to an [`HttpSender`]; the
//! default sender is backed by `reqwest`, but tests inject a fake, and advanced
//! callers can bring their own. [`Sleeper`] and [`Rng`] are the retry/backoff
//! seams (a fake sleeper makes tests instant; a fixed RNG makes backoff
//! deterministic).

use std::io::Read;
use std::time::Duration;

use crate::errors::Api2ConvertError;

/// A library-agnostic HTTP request.
pub struct HttpRequest {
    pub method: String,
    pub url: String,
    /// Header name/value pairs to set on the request.
    pub headers: Vec<(String, String)>,
    /// A fully materialized, replayable body (used for JSON requests).
    pub body: Option<Vec<u8>>,
    /// A factory that produces a fresh body reader for each send attempt (used
    /// for streaming uploads so a retry re-opens the file). Takes precedence
    /// over [`body`](Self::body).
    #[allow(clippy::type_complexity)]
    pub make_body: Option<Box<dyn Fn() -> std::io::Result<Box<dyn Read + Send>> + Send + Sync>>,
    /// Whether redirects may be followed. Only the self-contained, no-secret
    /// download path opts in; every secret-bearing request keeps this `false`.
    pub follow_redirects: bool,
    /// Whether the body can be re-sent. `false` for one-shot reader bodies —
    /// such a request is never retried.
    pub replayable: bool,
    /// The total request timeout, or `None` for no total cap (downloads /
    /// uploads, which may legitimately run long).
    pub timeout: Option<Duration>,
}

/// A library-agnostic HTTP response. The body is streamed, never buffered by
/// the sender.
pub struct HttpResponse {
    pub status: u16,
    pub headers: Headers,
    pub body: Box<dyn Read + Send>,
}

/// A case-insensitive collection of response headers.
#[derive(Default, Clone)]
pub struct Headers(Vec<(String, String)>);

impl Headers {
    pub fn new() -> Self {
        Headers(Vec::new())
    }

    /// Add a header. The name is stored lower-cased for case-insensitive lookup.
    pub fn insert(&mut self, name: impl Into<String>, value: impl Into<String>) {
        self.0
            .push((name.into().to_ascii_lowercase(), value.into()));
    }

    /// The first value for `name` (case-insensitive), if any.
    pub fn get(&self, name: &str) -> Option<&str> {
        let n = name.to_ascii_lowercase();
        self.0
            .iter()
            .find(|(k, _)| *k == n)
            .map(|(_, v)| v.as_str())
    }
}

/// Sends an [`HttpRequest`] and returns an [`HttpResponse`]. A genuine transport
/// failure is returned as [`Api2ConvertError::Network`]; a non-2xx status is
/// **not** an error at this layer (the transport maps it).
pub trait HttpSender: Send + Sync {
    fn send(&self, req: &HttpRequest) -> Result<HttpResponse, Api2ConvertError>;
}

/// The delay function used by retry and poll backoff. The default sleeps the
/// current thread; tests inject a no-op recorder.
pub trait Sleeper: Send + Sync {
    fn sleep(&self, dur: Duration);
}

/// A `[0, 1)` random source for backoff jitter. Tests inject a fixed value for
/// deterministic backoff.
pub trait Rng: Send + Sync {
    fn next_f64(&self) -> f64;
}

/// Default [`Sleeper`]: `std::thread::sleep`.
pub(crate) struct ThreadSleeper;

impl Sleeper for ThreadSleeper {
    fn sleep(&self, dur: Duration) {
        if !dur.is_zero() {
            std::thread::sleep(dur);
        }
    }
}

/// Default [`Rng`]: a small `xorshift64` seeded from the wall clock. Jitter does
/// not need cryptographic quality; concurrent draws may interleave harmlessly.
pub(crate) struct DefaultRng {
    state: std::sync::atomic::AtomicU64,
}

impl DefaultRng {
    pub(crate) fn new() -> Self {
        use std::time::{SystemTime, UNIX_EPOCH};
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E37_79B9_7F4A_7C15)
            | 1;
        DefaultRng {
            state: std::sync::atomic::AtomicU64::new(seed),
        }
    }
}

impl Rng for DefaultRng {
    fn next_f64(&self) -> f64 {
        use std::sync::atomic::Ordering;
        let mut x = self.state.load(Ordering::Relaxed);
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state.store(x, Ordering::Relaxed);
        // Take the top 53 bits into the unit interval.
        ((x >> 11) as f64) / ((1u64 << 53) as f64)
    }
}
