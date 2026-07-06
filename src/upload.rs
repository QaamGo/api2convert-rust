//! Streaming multipart upload to the per-job upload server. **Hand-authored** —
//! this step is not in the OpenAPI spec.
//!
//! The request posts `multipart/form-data` (a single `file` part) to
//! `{server}/upload-file/{job_id}` and is authenticated with the per-job
//! **`X-Oc-Token`** header — never the account key. The body is streamed (large
//! files are never buffered), and a single boundary is reused across retry
//! attempts so `Content-Type` always matches the body.

use std::fs::File;
use std::io::{Cursor, Read};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::client::Input;
use crate::errors::{Api2ConvertError, Result};
use crate::models::{InputFile, Job};
use crate::transport::{encode_segment, HttpRequest, Transport};

enum UploadSource {
    Path(std::path::PathBuf),
    Bytes(Arc<Vec<u8>>),
    Reader(Mutex<Option<Box<dyn Read + Send>>>),
}

pub(crate) fn upload(
    transport: &Transport,
    job: &Job,
    file: Input,
    filename: Option<&str>,
) -> Result<InputFile> {
    let server = job
        .server
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            Api2ConvertError::Config(
                "cannot upload: the job has no upload server (create it with process:false first)"
                    .to_string(),
            )
        })?;
    let token = job
        .token
        .as_deref()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            Api2ConvertError::Config("cannot upload: the job has no upload token".to_string())
        })?;

    let (default_name, source, replayable): (String, UploadSource, bool) = match file {
        Input::Url(_) => {
            return Err(Api2ConvertError::Config(
                "cannot upload a URL; add it as a remote input instead".to_string(),
            ))
        }
        Input::Path(path) => {
            if !path.exists() {
                return Err(Api2ConvertError::Config(format!(
                    "file not found: {}",
                    path.display()
                )));
            }
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file")
                .to_string();
            (name, UploadSource::Path(path), true)
        }
        Input::Bytes(bytes) => (
            "file".to_string(),
            UploadSource::Bytes(Arc::new(bytes)),
            true,
        ),
        Input::Reader(reader) => (
            "file".to_string(),
            UploadSource::Reader(Mutex::new(Some(reader))),
            false,
        ),
    };

    let fname = sanitize_filename(filename.unwrap_or(&default_name));
    let boundary = make_boundary();
    let preamble = format!(
        "--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{fname}\"\r\n\
         Content-Type: application/octet-stream\r\n\r\n"
    )
    .into_bytes();
    let epilogue = format!("\r\n--{boundary}--\r\n").into_bytes();
    let content_type = format!("multipart/form-data; boundary={boundary}");

    let source = Arc::new(source);
    let make_body = {
        let source = Arc::clone(&source);
        let preamble = preamble.clone();
        let epilogue = epilogue.clone();
        Box::new(move || -> std::io::Result<Box<dyn Read + Send>> {
            let inner: Box<dyn Read + Send> = match &*source {
                UploadSource::Path(p) => Box::new(File::open(p)?),
                UploadSource::Bytes(b) => Box::new(Cursor::new((**b).clone())),
                UploadSource::Reader(cell) => match cell.lock().unwrap().take() {
                    Some(r) => r,
                    None => {
                        return Err(std::io::Error::other("upload stream already consumed"));
                    }
                },
            };
            let chained = Cursor::new(preamble.clone())
                .chain(inner)
                .chain(Cursor::new(epilogue.clone()));
            Ok(Box::new(chained) as Box<dyn Read + Send>)
        })
    };

    let url = format!(
        "{}/upload-file/{}",
        server.trim_end_matches('/'),
        encode_segment(&job.id)
    );
    let headers = vec![
        ("X-Oc-Token".to_string(), token.to_string()),
        ("Content-Type".to_string(), content_type),
        (
            "User-Agent".to_string(),
            format!(
                "api2convert-rust/{} ({})",
                crate::VERSION,
                std::env::consts::OS
            ),
        ),
    ];

    let req = HttpRequest {
        method: "POST".to_string(),
        url,
        headers,
        body: None,
        make_body: Some(make_body),
        follow_redirects: false,
        replayable,
        timeout: None,
    };

    let v = transport.send(req)?;
    Ok(InputFile::from_value(&v))
}

/// Strip characters that could inject into the `Content-Disposition` header.
fn sanitize_filename(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .filter(|&c| c != '\r' && c != '\n' && c != '"' && c != '\0')
        .collect();
    let trimmed = cleaned.trim();
    if trimmed.is_empty() {
        "file".to_string()
    } else {
        trimmed.to_string()
    }
}

/// A unique multipart boundary, reused across a request's retry attempts.
fn make_boundary() -> String {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let t = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0);
    format!("----api2convert{t:016x}{n:016x}")
}
