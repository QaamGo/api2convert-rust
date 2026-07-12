//! Shared test helpers: an injectable [`FakeSender`] (records requests, returns
//! canned responses), a recording sleeper and a zero-jitter RNG for
//! deterministic offline tests, plus a minimal loopback HTTP server for the
//! black-box security suite.

#![allow(dead_code)]

use std::collections::VecDeque;
use std::io::{BufRead, BufReader, Cursor, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use api2convert::{
    Api2Convert, Api2ConvertError, Headers, HttpRequest, HttpResponse, HttpSender, Rng, Sleeper,
};

// ----------------------------- FakeSender --------------------------------

#[derive(Clone)]
pub struct Canned {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    /// When true, `send` records the request then returns a transport error
    /// (simulating DNS/connection/TLS failure) instead of this response.
    pub network_error: bool,
}

#[derive(Clone)]
pub struct Recorded {
    pub method: String,
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub follow_redirects: bool,
}

impl Recorded {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn has_header(&self, name: &str) -> bool {
        self.header(name).is_some()
    }

    /// The URL path (+ query), i.e. everything after the host.
    pub fn path(&self) -> String {
        if let Some(idx) = self.url.find("://") {
            let rest = &self.url[idx + 3..];
            if let Some(slash) = rest.find('/') {
                return rest[slash..].to_string();
            }
        }
        self.url.clone()
    }

    pub fn body_json(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap_or(serde_json::Value::Null)
    }

    pub fn body_string(&self) -> String {
        String::from_utf8_lossy(&self.body).to_string()
    }
}

pub struct FakeSender {
    responses: Mutex<VecDeque<Canned>>,
    requests: Mutex<Vec<Recorded>>,
}

impl FakeSender {
    pub fn new() -> Arc<Self> {
        Arc::new(FakeSender {
            responses: Mutex::new(VecDeque::new()),
            requests: Mutex::new(Vec::new()),
        })
    }

    pub fn push(&self, c: Canned) {
        self.responses.lock().unwrap().push_back(c);
    }

    pub fn push_json(&self, status: u16, v: serde_json::Value) {
        self.push(Canned {
            status,
            headers: vec![("content-type".into(), "application/json".into())],
            body: serde_json::to_vec(&v).unwrap(),
            network_error: false,
        });
    }

    /// Queue a 200 JSON response.
    pub fn push_ok(&self, v: serde_json::Value) {
        self.push_json(200, v);
    }

    /// Queue a raw-body response with custom headers (e.g. a download).
    pub fn push_raw(&self, status: u16, body: &[u8], headers: Vec<(String, String)>) {
        self.push(Canned {
            status,
            headers,
            body: body.to_vec(),
            network_error: false,
        });
    }

    /// Queue a simulated transport failure (the request is still recorded).
    pub fn push_network_error(&self) {
        self.push(Canned {
            status: 0,
            headers: vec![],
            body: vec![],
            network_error: true,
        });
    }

    pub fn requests(&self) -> Vec<Recorded> {
        self.requests.lock().unwrap().clone()
    }

    pub fn request_count(&self) -> usize {
        self.requests.lock().unwrap().len()
    }

    pub fn request_at(&self, i: usize) -> Recorded {
        self.requests.lock().unwrap()[i].clone()
    }

    pub fn last_request(&self) -> Recorded {
        self.requests.lock().unwrap().last().unwrap().clone()
    }
}

impl HttpSender for FakeSender {
    fn send(&self, req: &HttpRequest) -> Result<HttpResponse, Api2ConvertError> {
        let body = if let Some(mb) = &req.make_body {
            let mut r = mb().expect("make_body");
            let mut b = Vec::new();
            r.read_to_end(&mut b).expect("read make_body");
            b
        } else {
            req.body.clone().unwrap_or_default()
        };
        self.requests.lock().unwrap().push(Recorded {
            method: req.method.clone(),
            url: req.url.clone(),
            headers: req.headers.clone(),
            body,
            follow_redirects: req.follow_redirects,
        });
        let c = self
            .responses
            .lock()
            .unwrap()
            .pop_front()
            .expect("FakeSender: no canned response left");
        if c.network_error {
            return Err(Api2ConvertError::Network(
                "simulated transport failure".to_string(),
            ));
        }
        let mut headers = Headers::new();
        for (k, v) in &c.headers {
            headers.insert(k.clone(), v.clone());
        }
        Ok(HttpResponse {
            status: c.status,
            headers,
            body: Box::new(Cursor::new(c.body)),
        })
    }
}

// ----------------------------- Sleeper / Rng ------------------------------

#[derive(Default)]
pub struct RecordingSleeper {
    slept: Mutex<Vec<Duration>>,
}

impl RecordingSleeper {
    pub fn durations(&self) -> Vec<Duration> {
        self.slept.lock().unwrap().clone()
    }

    pub fn count(&self) -> usize {
        self.slept.lock().unwrap().len()
    }
}

impl Sleeper for RecordingSleeper {
    fn sleep(&self, dur: Duration) {
        self.slept.lock().unwrap().push(dur);
    }
}

pub struct ZeroRng;

impl Rng for ZeroRng {
    fn next_f64(&self) -> f64 {
        0.0
    }
}

// ----------------------------- test clients -------------------------------

pub fn client_with(sender: Arc<FakeSender>, sleeper: Arc<RecordingSleeper>) -> Api2Convert {
    Api2Convert::builder()
        .api_key("test-key")
        .base_url("http://api.test")
        .http_sender(sender)
        .sleeper(sleeper)
        .rng(Arc::new(ZeroRng))
        .build()
        .expect("build test client")
}

pub fn client(sender: Arc<FakeSender>) -> Api2Convert {
    client_with(sender, Arc::new(RecordingSleeper::default()))
}

pub fn client_no_retry(sender: Arc<FakeSender>) -> Api2Convert {
    Api2Convert::builder()
        .api_key("test-key")
        .base_url("http://api.test")
        .max_retries(0)
        .http_sender(sender)
        .sleeper(Arc::new(RecordingSleeper::default()))
        .rng(Arc::new(ZeroRng))
        .build()
        .expect("build test client")
}

/// A fresh, unique temp directory for filesystem tests (parallel-safe).
pub fn unique_tmp_dir(tag: &str) -> std::path::PathBuf {
    use std::sync::atomic::AtomicU64;
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let t = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("a2c-{tag}-{}-{t}-{n}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

// ----------------------------- loopback server ----------------------------

pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl Reply {
    pub fn ok(body: &[u8]) -> Reply {
        Reply {
            status: 200,
            headers: vec![],
            body: body.to_vec(),
        }
    }

    pub fn redirect(location: &str) -> Reply {
        Reply {
            status: 302,
            headers: vec![("Location".into(), location.to_string())],
            body: Vec::new(),
        }
    }

    pub fn status(code: u16, body: &[u8]) -> Reply {
        Reply {
            status: code,
            headers: vec![],
            body: body.to_vec(),
        }
    }
}

#[derive(Clone)]
pub struct ServerRequest {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
}

impl ServerRequest {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// Whether any secret-bearing `X-Api2convert-*` (or legacy `X-Oc-*`) header was received.
    pub fn has_secret_header(&self) -> bool {
        self.headers.iter().any(|(k, _)| {
            let lk = k.to_ascii_lowercase();
            lk.starts_with("x-api2convert-") || lk.starts_with("x-oc-")
        })
    }
}

/// A minimal single-threaded loopback HTTP/1.1 server for security tests.
pub struct TestServer {
    pub base: String,
    hits: Arc<Mutex<Vec<ServerRequest>>>,
    shutdown: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl TestServer {
    pub fn start<F>(handler: F) -> TestServer
    where
        F: Fn(&ServerRequest) -> Reply + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind loopback");
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let base = format!("http://127.0.0.1:{port}");
        let hits = Arc::new(Mutex::new(Vec::new()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let handler: Arc<dyn Fn(&ServerRequest) -> Reply + Send + Sync> = Arc::new(handler);

        let thread_hits = Arc::clone(&hits);
        let thread_shutdown = Arc::clone(&shutdown);
        let handle = thread::spawn(move || loop {
            if thread_shutdown.load(Ordering::Relaxed) {
                break;
            }
            match listener.accept() {
                Ok((stream, _)) => {
                    let _ = handle_conn(stream, handler.as_ref(), &thread_hits);
                }
                Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    thread::sleep(Duration::from_millis(5));
                }
                Err(_) => break,
            }
        });

        TestServer {
            base,
            hits,
            shutdown,
            handle: Some(handle),
        }
    }

    pub fn hits(&self) -> usize {
        self.hits.lock().unwrap().len()
    }

    pub fn requests(&self) -> Vec<ServerRequest> {
        self.hits.lock().unwrap().clone()
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        self.shutdown.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

fn handle_conn(
    stream: TcpStream,
    handler: &(dyn Fn(&ServerRequest) -> Reply + Send + Sync),
    hits: &Arc<Mutex<Vec<ServerRequest>>>,
) -> std::io::Result<()> {
    stream.set_nonblocking(false)?;
    let mut reader = BufReader::new(stream.try_clone()?);

    let mut request_line = String::new();
    reader.read_line(&mut request_line)?;
    if request_line.trim().is_empty() {
        return Ok(());
    }
    let mut parts = request_line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();

    let mut headers: Vec<(String, String)> = Vec::new();
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        let n = reader.read_line(&mut line)?;
        if n == 0 {
            break;
        }
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed.is_empty() {
            break;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            let k = k.trim().to_string();
            let v = v.trim().to_string();
            if k.eq_ignore_ascii_case("content-length") {
                content_length = v.parse().unwrap_or(0);
            }
            headers.push((k, v));
        }
    }
    if content_length > 0 {
        let mut body = vec![0u8; content_length];
        reader.read_exact(&mut body)?;
    }

    let req = ServerRequest {
        method,
        path,
        headers,
    };
    let reply = handler(&req);
    hits.lock().unwrap().push(req);

    let mut stream = stream;
    let mut resp = format!("HTTP/1.1 {} X\r\n", reply.status);
    for (k, v) in &reply.headers {
        resp.push_str(&format!("{k}: {v}\r\n"));
    }
    resp.push_str(&format!("Content-Length: {}\r\n", reply.body.len()));
    resp.push_str("Connection: close\r\n\r\n");
    stream.write_all(resp.as_bytes())?;
    stream.write_all(&reply.body)?;
    stream.flush()?;
    Ok(())
}
