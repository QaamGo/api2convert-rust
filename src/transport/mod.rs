//! The transport layer: authentication, retries with jittered backoff, HTTP
//! status → typed-error mapping, JSON (de)serialization and the download
//! redirect policy. Built on the pluggable [`HttpSender`] seam so tests can
//! inject a fake and the redirect/leak guarantees can be proved end-to-end.

mod http_sender;
mod reqwest_sender;

use std::io::Read;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub(crate) use http_sender::{DefaultRng, ThreadSleeper};
pub use http_sender::{Headers, HttpRequest, HttpResponse, HttpSender, Rng, Sleeper};
pub(crate) use reqwest_sender::ReqwestSender;

use serde_json::{Map, Value};

use crate::errors::{Api2ConvertError, Result};
use crate::version::VERSION;

const RETRYABLE_STATUSES: [u16; 5] = [429, 500, 502, 503, 504];
const MAX_BACKOFF: f64 = 8.0;
const MAX_RETRY_AFTER: Duration = Duration::from_secs(120);

/// Resolved client configuration + the transport seams. Shared behind an `Arc`
/// by the client, its resources and every result/download handle.
pub(crate) struct Transport {
    api_key: String,
    base_url: String,
    timeout: Duration,
    max_retries: u32,
    poll_interval: Duration,
    poll_max_interval: Duration,
    poll_timeout: Duration,
    max_download_bytes: u64,
    sender: Arc<dyn HttpSender>,
    sleeper: Arc<dyn Sleeper>,
    rng: Arc<dyn Rng>,
}

#[allow(clippy::too_many_arguments)]
impl Transport {
    pub(crate) fn new(
        api_key: String,
        base_url: String,
        timeout: Duration,
        max_retries: u32,
        poll_interval: Duration,
        poll_max_interval: Duration,
        poll_timeout: Duration,
        max_download_bytes: u64,
        sender: Arc<dyn HttpSender>,
        sleeper: Arc<dyn Sleeper>,
        rng: Arc<dyn Rng>,
    ) -> Self {
        Transport {
            api_key,
            base_url,
            timeout,
            max_retries,
            poll_interval,
            poll_max_interval,
            poll_timeout,
            max_download_bytes,
            sender,
            sleeper,
            rng,
        }
    }

    pub(crate) fn poll_interval(&self) -> Duration {
        self.poll_interval
    }
    pub(crate) fn poll_max_interval(&self) -> Duration {
        self.poll_max_interval
    }
    pub(crate) fn poll_timeout(&self) -> Duration {
        self.poll_timeout
    }
    pub(crate) fn max_download_bytes(&self) -> u64 {
        self.max_download_bytes
    }

    #[cfg(test)]
    pub(crate) fn debug_base_url(&self) -> String {
        self.base_url.clone()
    }

    fn user_agent(&self) -> String {
        format!("api2convert-rust/{} ({})", VERSION, std::env::consts::OS)
    }

    fn base_headers(&self) -> Vec<(String, String)> {
        vec![
            ("Accept".to_string(), "application/json".to_string()),
            ("User-Agent".to_string(), self.user_agent()),
        ]
    }

    fn build_url(&self, path: &str, query: &[(&str, String)]) -> String {
        let mut url = format!("{}/{}", self.base_url, path.trim_start_matches('/'));
        if !query.is_empty() {
            url.push('?');
            let encoded: Vec<String> = query
                .iter()
                .map(|(k, v)| format!("{}={}", pct_encode(k), pct_encode(v)))
                .collect();
            url.push_str(&encoded.join("&"));
        }
        url
    }

    /// A JSON account request (`X-Api2convert-Api-Key`). `query` values are URL-encoded;
    /// dynamic path segments must already be encoded via [`encode_segment`].
    pub(crate) fn account_request(
        &self,
        method: &str,
        path: &str,
        query: &[(&str, String)],
        body: Option<Value>,
        idempotency_key: Option<&str>,
    ) -> Result<Value> {
        let mut headers = self.base_headers();
        headers.push(("X-Api2convert-Api-Key".to_string(), self.api_key.clone()));

        let body_bytes = match &body {
            Some(v) => {
                headers.push(("Content-Type".to_string(), "application/json".to_string()));
                Some(serde_json::to_vec(v).map_err(|e| {
                    Api2ConvertError::Config(format!("failed to encode request body: {e}"))
                })?)
            }
            None => None,
        };
        if let Some(key) = idempotency_key {
            if !key.is_empty() {
                headers.push(("Idempotency-Key".to_string(), key.to_string()));
            }
        }

        let req = HttpRequest {
            method: method.to_string(),
            url: self.build_url(path, query),
            headers,
            body: body_bytes,
            make_body: None,
            follow_redirects: false,
            replayable: true,
            timeout: Some(self.timeout),
        };
        self.send(req)
    }

    /// Send a fully-built request and decode a JSON body. Used for account
    /// requests and the multipart upload.
    pub(crate) fn send(&self, req: HttpRequest) -> Result<Value> {
        let resp = self.execute(req)?;
        self.interpret(resp)
    }

    /// The retry loop: send, and on a retryable outcome sleep and try again,
    /// honoring the idempotency rule so a bare `POST` is never blindly re-sent.
    fn execute(&self, req: HttpRequest) -> Result<HttpResponse> {
        let idempotent = is_idempotent(&req);
        let mut attempt: u32 = 0;
        loop {
            attempt += 1;
            match self.sender.send(&req) {
                Ok(resp) => {
                    if attempt <= self.max_retries && RETRYABLE_STATUSES.contains(&resp.status) {
                        let retry = if resp.status == 429 {
                            req.replayable
                        } else {
                            req.replayable && idempotent
                        };
                        if retry {
                            let delay = self.retry_delay(attempt, resp.headers.get("retry-after"));
                            drop(resp);
                            self.sleeper.sleep(delay);
                            continue;
                        }
                    }
                    return Ok(resp);
                }
                Err(e) => {
                    if attempt <= self.max_retries && req.replayable && idempotent {
                        let delay = self.retry_delay(attempt, None);
                        self.sleeper.sleep(delay);
                        continue;
                    }
                    return Err(e);
                }
            }
        }
    }

    /// Decode a JSON response, mapping a non-2xx status to a typed error and an
    /// unfollowed redirect / non-JSON 2xx to a network error.
    fn interpret(&self, mut resp: HttpResponse) -> Result<Value> {
        if (300..400).contains(&resp.status) {
            return Err(Api2ConvertError::Network(
                "the API responded with an unexpected redirect".to_string(),
            ));
        }
        let bytes = read_all(&mut resp.body)?;
        if resp.status >= 400 {
            return Err(self.error_from(resp.status, &resp.headers, &bytes));
        }
        if bytes.is_empty() {
            return Ok(Value::Object(Map::new()));
        }
        serde_json::from_slice(&bytes).map_err(|_| {
            Api2ConvertError::Network("the API returned a non-JSON response".to_string())
        })
    }

    fn error_from(&self, status: u16, headers: &Headers, bytes: &[u8]) -> Api2ConvertError {
        // Belt-and-suspenders: deep-redact the decoded error body before it lands on the
        // exception. Cloud credentials ride in the plaintext request body; the API only ever
        // echoes field *names* (never a value), but a future server/proxy change must not be
        // able to surface a secret through `err.body()`. The `message` is server-provided text
        // and is never derived from the request body.
        let raw: Value = serde_json::from_slice(bytes).unwrap_or(Value::Null);
        let body = crate::redact::redact_body(&raw);
        let message = body
            .get("message")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| status_reason(status));
        let request_id = headers.get("x-request-id").map(str::to_string);
        let mut err = Api2ConvertError::api(status, message, request_id, body);
        if status == 429 {
            if let Api2ConvertError::RateLimit { retry_after, .. } = &mut err {
                *retry_after = headers
                    .get("retry-after")
                    .and_then(parse_retry_after)
                    .map(|d| d.as_secs() as i64);
            }
        }
        err
    }

    fn retry_delay(&self, attempt: u32, retry_after: Option<&str>) -> Duration {
        if let Some(h) = retry_after {
            if let Some(d) = parse_retry_after(h) {
                return d.min(MAX_RETRY_AFTER);
            }
        }
        let base = 0.5 * 2f64.powi((attempt - 1) as i32);
        let capped = base.min(MAX_BACKOFF);
        let jitter = 1.0 + 0.25 * self.rng.next_f64();
        Duration::from_secs_f64(capped * jitter)
    }

    /// Sleep between poll attempts, with the same upward jitter as retry backoff.
    pub(crate) fn poll_pause(&self, interval: Duration) {
        let jitter = 1.0 + 0.25 * self.rng.next_f64();
        self.sleeper
            .sleep(Duration::from_secs_f64(interval.as_secs_f64() * jitter));
    }

    /// Open a download stream. Any `X-Api2convert-*` (or legacy `X-Oc-*`) header marks the request as
    /// secret-bearing, which disables redirect following; if such a request
    /// receives a redirect it is surfaced as an error (never a silent empty
    /// file, never a secret forwarded to another host).
    pub(crate) fn open_download(
        &self,
        uri: &str,
        download_password: Option<&str>,
    ) -> Result<HttpResponse> {
        let mut headers = vec![("User-Agent".to_string(), self.user_agent())];
        if let Some(pw) = download_password {
            if !pw.is_empty() {
                headers.push((
                    "X-Api2convert-Download-Password".to_string(),
                    pw.to_string(),
                ));
            }
        }
        let carries_secret = headers.iter().any(|(k, _)| {
            let lk = k.to_ascii_lowercase();
            lk.starts_with("x-api2convert-") || lk.starts_with("x-oc-")
        });

        let req = HttpRequest {
            method: "GET".to_string(),
            url: uri.to_string(),
            headers,
            body: None,
            make_body: None,
            follow_redirects: !carries_secret,
            replayable: true,
            timeout: None,
        };

        let mut resp = self.execute(req)?;
        if (300..400).contains(&resp.status) {
            return Err(Api2ConvertError::Network(
                "the download did not resolve: a redirect was not followed because the request \
                 carried a secret header"
                    .to_string(),
            ));
        }
        if resp.status >= 400 {
            let bytes = read_all(&mut resp.body).unwrap_or_default();
            return Err(self.error_from(resp.status, &resp.headers, &bytes));
        }
        Ok(resp)
    }
}

fn is_idempotent(req: &HttpRequest) -> bool {
    let method_ok = matches!(
        req.method.to_ascii_uppercase().as_str(),
        "GET" | "HEAD" | "PUT" | "DELETE" | "OPTIONS" | "TRACE"
    );
    let has_idempotency_key = req
        .headers
        .iter()
        .any(|(k, v)| k.eq_ignore_ascii_case("idempotency-key") && !v.is_empty());
    method_ok || has_idempotency_key
}

fn read_all(body: &mut Box<dyn Read + Send>) -> Result<Vec<u8>> {
    let mut buf = Vec::new();
    body.read_to_end(&mut buf)
        .map_err(|_| Api2ConvertError::Network("failed to read response body".to_string()))?;
    Ok(buf)
}

/// Percent-encode reserved bytes, keeping the unreserved set (`A-Za-z0-9-._~`).
pub(crate) fn pct_encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for &b in s.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Encode a single dynamic path segment (job/preset id, stats period/filter) so
/// it cannot inject extra path structure.
pub(crate) fn encode_segment(s: &str) -> String {
    pct_encode(s)
}

/// Parse a `Retry-After` header: an integer number of seconds, or an HTTP-date.
fn parse_retry_after(value: &str) -> Option<Duration> {
    let v = value.trim();
    if let Ok(secs) = v.parse::<i64>() {
        return Some(Duration::from_secs(secs.max(0) as u64));
    }
    let target = parse_http_date(v)?;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    Some(Duration::from_secs((target - now).max(0) as u64))
}

/// Parse an RFC 1123 HTTP-date (`Wed, 21 Oct 2015 07:28:00 GMT`) into unix seconds.
fn parse_http_date(s: &str) -> Option<i64> {
    let tokens: Vec<&str> = s.split_whitespace().collect();
    if tokens.len() < 5 {
        return None;
    }
    let day: i64 = tokens[1].parse().ok()?;
    if !(1..=31).contains(&day) {
        return None;
    }
    let month = month_num(tokens[2])?;
    let year: i64 = tokens[3].parse().ok()?;
    // Reject absurd years: a real HTTP-date has a 4-digit year, and bounding it
    // keeps the arithmetic below well within i64 (a crafted huge year in a
    // server `Retry-After` would otherwise overflow and panic in debug builds).
    if !(0..=999_999).contains(&year) {
        return None;
    }
    let time: Vec<&str> = tokens[4].split(':').collect();
    if time.len() != 3 {
        return None;
    }
    let hh: i64 = time[0].parse().ok()?;
    let mm: i64 = time[1].parse().ok()?;
    let ss: i64 = time[2].parse().ok()?;
    let days = days_from_civil(year, month, day);
    Some(days * 86400 + hh * 3600 + mm * 60 + ss)
}

fn month_num(m: &str) -> Option<i64> {
    Some(match m {
        "Jan" => 1,
        "Feb" => 2,
        "Mar" => 3,
        "Apr" => 4,
        "May" => 5,
        "Jun" => 6,
        "Jul" => 7,
        "Aug" => 8,
        "Sep" => 9,
        "Oct" => 10,
        "Nov" => 11,
        "Dec" => 12,
        _ => return None,
    })
}

/// Days since 1970-01-01 (Howard Hinnant's `days_from_civil`).
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

fn status_reason(status: u16) -> String {
    let reason = match status {
        400 => "Bad Request",
        401 => "Unauthorized",
        402 => "Payment Required",
        403 => "Forbidden",
        404 => "Not Found",
        409 => "Conflict",
        422 => "Unprocessable Entity",
        429 => "Too Many Requests",
        500 => "Internal Server Error",
        502 => "Bad Gateway",
        503 => "Service Unavailable",
        504 => "Gateway Timeout",
        _ => "",
    };
    if reason.is_empty() {
        format!("HTTP {status}")
    } else {
        format!("HTTP {status} {reason}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pct_encode_keeps_unreserved_and_escapes_the_rest() {
        assert_eq!(pct_encode("abcABC123-._~"), "abcABC123-._~");
        assert_eq!(pct_encode("a b/c?d"), "a%20b%2Fc%3Fd");
        assert_eq!(encode_segment("../evil"), "..%2Fevil");
    }

    #[test]
    fn retry_after_seconds_form() {
        assert_eq!(parse_retry_after("5"), Some(Duration::from_secs(5)));
        assert_eq!(parse_retry_after("  0 "), Some(Duration::from_secs(0)));
        assert_eq!(parse_retry_after("-3"), Some(Duration::from_secs(0)));
        assert_eq!(parse_retry_after("garbage"), None);
    }

    #[test]
    fn days_from_civil_known_dates() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(1970, 1, 2), 1);
        assert_eq!(days_from_civil(2000, 1, 1), 10957);
    }

    #[test]
    fn http_date_parses_to_unix_seconds() {
        // Wed, 21 Oct 2015 07:28:00 GMT == 1445412480
        assert_eq!(
            parse_http_date("Wed, 21 Oct 2015 07:28:00 GMT"),
            Some(1_445_412_480)
        );
        assert_eq!(parse_http_date("not a date"), None);
    }

    #[test]
    fn http_date_rejects_absurd_values_without_overflowing() {
        // A crafted huge year in a server Retry-After must not overflow/panic.
        assert_eq!(
            parse_http_date("Mon, 01 Jan 300000000000 00:00:00 GMT"),
            None
        );
        assert_eq!(
            parse_http_date("Mon, 01 Jan 99999999999999999 00:00:00 GMT"),
            None
        );
        assert_eq!(parse_http_date("Mon, 99 Jan 2020 00:00:00 GMT"), None);
    }

    #[test]
    fn idempotency_rules() {
        let mk = |method: &str, headers: Vec<(String, String)>| HttpRequest {
            method: method.to_string(),
            url: "http://x".to_string(),
            headers,
            body: None,
            make_body: None,
            follow_redirects: false,
            replayable: true,
            timeout: None,
        };
        assert!(is_idempotent(&mk("GET", vec![])));
        assert!(is_idempotent(&mk("delete", vec![])));
        assert!(!is_idempotent(&mk("POST", vec![])));
        assert!(is_idempotent(&mk(
            "POST",
            vec![("Idempotency-Key".into(), "abc".into())]
        )));
        assert!(!is_idempotent(&mk(
            "POST",
            vec![("Idempotency-Key".into(), "".into())]
        )));
    }
}
