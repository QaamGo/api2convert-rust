//! Client configuration and its [`ClientBuilder`].
//!
//! Every poll/timeout knob is clamped so no caller value can make the poll loop
//! busy-spin (interval floor) or wait unbounded (timeout ceiling) — a safety
//! invariant inherited from the sibling SDKs.

use std::sync::Arc;
use std::time::Duration;

use crate::client::Api2Convert;
use crate::errors::{Api2ConvertError, Result};
use crate::transport::{
    DefaultRng, HttpSender, ReqwestSender, Rng, Sleeper, ThreadSleeper, Transport,
};

/// The default API base URL.
pub const DEFAULT_BASE_URL: &str = "https://api.api2convert.com/v2";

/// The environment variable consulted when no API key is passed explicitly.
pub const API_KEY_ENV: &str = "API2CONVERT_API_KEY";

const MIN_POLL_INTERVAL: Duration = Duration::from_millis(500);
const MAX_POLL_TIMEOUT: Duration = Duration::from_secs(14400);
const MIN_TIMEOUT: Duration = Duration::from_secs(1);

/// Builds an [`Api2Convert`] client. Obtain one via [`Api2Convert::builder`].
pub struct ClientBuilder {
    api_key: Option<String>,
    base_url: String,
    timeout: Duration,
    max_retries: u32,
    poll_interval: Duration,
    poll_max_interval: Duration,
    poll_timeout: Duration,
    max_download_bytes: u64,
    sender: Option<Arc<dyn HttpSender>>,
    sleeper: Option<Arc<dyn Sleeper>>,
    rng: Option<Arc<dyn Rng>>,
}

impl Default for ClientBuilder {
    fn default() -> Self {
        ClientBuilder {
            api_key: None,
            base_url: DEFAULT_BASE_URL.to_string(),
            timeout: Duration::from_secs(30),
            max_retries: 2,
            poll_interval: Duration::from_secs(1),
            poll_max_interval: Duration::from_secs(5),
            poll_timeout: Duration::from_secs(300),
            max_download_bytes: 0,
            sender: None,
            sleeper: None,
            rng: None,
        }
    }
}

impl ClientBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// The account API key. When unset, the `API2CONVERT_API_KEY` environment
    /// variable is used at [`build`](Self::build) time.
    pub fn api_key(mut self, key: impl Into<String>) -> Self {
        self.api_key = Some(key.into());
        self
    }

    /// The API base URL (default [`DEFAULT_BASE_URL`]). A trailing `/` is trimmed.
    pub fn base_url(mut self, url: impl Into<String>) -> Self {
        self.base_url = url.into();
        self
    }

    /// The per-request network timeout for JSON calls (default 30s, min 1s).
    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = d;
        self
    }

    /// The number of automatic retries for transient failures (default 2).
    pub fn max_retries(mut self, n: u32) -> Self {
        self.max_retries = n;
        self
    }

    /// The first poll interval when waiting for a job (default 1s, floored to 500ms).
    pub fn poll_interval(mut self, d: Duration) -> Self {
        self.poll_interval = d;
        self
    }

    /// The upper bound the poll interval backs off to (default 5s).
    pub fn poll_max_interval(mut self, d: Duration) -> Self {
        self.poll_max_interval = d;
        self
    }

    /// How long to wait for a job before giving up (default 300s, capped at 14400s).
    pub fn poll_timeout(mut self, d: Duration) -> Self {
        self.poll_timeout = d;
        self
    }

    /// Cap the size of a downloaded file; a larger response yields a network
    /// error instead of an unbounded read. `0` (the default) means unlimited.
    pub fn max_download_bytes(mut self, n: u64) -> Self {
        self.max_download_bytes = n;
        self
    }

    /// Inject a custom HTTP transport (test seam / bring-your-own client).
    pub fn http_sender(mut self, sender: Arc<dyn HttpSender>) -> Self {
        self.sender = Some(sender);
        self
    }

    /// Inject the delay function used by retry/poll backoff (test seam).
    pub fn sleeper(mut self, sleeper: Arc<dyn Sleeper>) -> Self {
        self.sleeper = Some(sleeper);
        self
    }

    /// Inject the `[0, 1)` jitter source for backoff (test seam).
    pub fn rng(mut self, rng: Arc<dyn Rng>) -> Self {
        self.rng = Some(rng);
        self
    }

    /// Resolve the configuration (clamping every knob) and build the client.
    /// Fails with [`Api2ConvertError::Config`] if no API key is available.
    pub fn build(self) -> Result<Api2Convert> {
        let api_key = self
            .api_key
            .filter(|k| !k.is_empty())
            .or_else(|| std::env::var(API_KEY_ENV).ok().filter(|k| !k.is_empty()))
            .ok_or_else(|| {
                Api2ConvertError::Config(format!(
                    "an API key is required: pass one to the builder or set {API_KEY_ENV}"
                ))
            })?;

        let timeout = clamp_timeout(self.timeout);
        let poll_interval = clamp_poll_interval(self.poll_interval);
        let poll_max_interval = self.poll_max_interval.max(poll_interval);
        let poll_timeout = clamp_poll_timeout(self.poll_timeout);

        let sender: Arc<dyn HttpSender> = match self.sender {
            Some(s) => s,
            None => Arc::new(ReqwestSender::new(timeout)?),
        };
        let sleeper: Arc<dyn Sleeper> = self.sleeper.unwrap_or_else(|| Arc::new(ThreadSleeper));
        let rng: Arc<dyn Rng> = self.rng.unwrap_or_else(|| Arc::new(DefaultRng::new()));

        let transport = Transport::new(
            api_key,
            self.base_url.trim_end_matches('/').to_string(),
            timeout,
            self.max_retries,
            poll_interval,
            poll_max_interval,
            poll_timeout,
            self.max_download_bytes,
            sender,
            sleeper,
            rng,
        );
        Ok(Api2Convert::from_transport(Arc::new(transport)))
    }
}

/// Floor the network timeout so a request can never be given zero time.
fn clamp_timeout(d: Duration) -> Duration {
    d.max(MIN_TIMEOUT)
}

/// Floor the poll interval so the poll loop can never busy-spin.
fn clamp_poll_interval(d: Duration) -> Duration {
    d.max(MIN_POLL_INTERVAL)
}

/// Cap the total poll wait so the loop can never poll unbounded.
fn clamp_poll_timeout(d: Duration) -> Duration {
    d.min(MAX_POLL_TIMEOUT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn poll_interval_is_floored() {
        assert_eq!(
            clamp_poll_interval(Duration::from_millis(1)),
            MIN_POLL_INTERVAL
        );
        assert_eq!(clamp_poll_interval(Duration::ZERO), MIN_POLL_INTERVAL);
        assert_eq!(
            clamp_poll_interval(Duration::from_secs(3)),
            Duration::from_secs(3)
        );
    }

    #[test]
    fn poll_timeout_is_capped() {
        assert_eq!(
            clamp_poll_timeout(Duration::from_secs(100_000)),
            MAX_POLL_TIMEOUT
        );
        assert_eq!(
            clamp_poll_timeout(Duration::from_secs(42)),
            Duration::from_secs(42)
        );
    }

    #[test]
    fn timeout_has_a_floor() {
        assert_eq!(clamp_timeout(Duration::ZERO), MIN_TIMEOUT);
        assert_eq!(
            clamp_timeout(Duration::from_secs(10)),
            Duration::from_secs(10)
        );
    }

    #[test]
    fn missing_api_key_is_a_config_error() {
        // Ensure the env fallback cannot accidentally satisfy this.
        std::env::remove_var(API_KEY_ENV);
        let result = ClientBuilder::new().build();
        assert!(matches!(result, Err(Api2ConvertError::Config(_))));
    }

    #[test]
    fn base_url_trailing_slash_is_trimmed() {
        // A trailing slash on the base URL must not produce a double slash.
        let client = ClientBuilder::new()
            .api_key("test-key")
            .base_url("https://example.test/v2/")
            .build()
            .unwrap();
        assert_eq!(client.debug_base_url(), "https://example.test/v2");
    }
}
