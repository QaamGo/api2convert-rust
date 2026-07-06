//! Deep coverage of the hand-authored multipart upload: exact body framing,
//! the three input kinds (path / bytes / reader), boundary reuse across a retry,
//! one-shot readers not being retried, filename-injection sanitization, and
//! job-id encoding in the upload URL.

mod common;

use std::io::Cursor;

use api2convert::{Api2ConvertError, Input, Job, Status};
use common::*;
use serde_json::{json, Value};

fn staged_job() -> Job {
    Job {
        id: "job1".to_string(),
        status: Status {
            code: "created".to_string(),
            info: None,
        },
        token: Some("job-token".to_string()),
        server: Some("https://upload.example".to_string()),
        callback: None,
        conversion: vec![],
        input: vec![],
        output: vec![],
        errors: vec![],
        warnings: vec![],
        raw: Value::Null,
    }
}

/// Extract the `boundary=` value from a recorded multipart Content-Type.
fn boundary_of(req: &Recorded) -> String {
    let ct = req.header("content-type").expect("content-type");
    let marker = "boundary=";
    let idx = ct.find(marker).expect("boundary= present");
    ct[idx + marker.len()..].trim().to_string()
}

#[test]
fn multipart_body_is_correctly_framed() {
    let sender = FakeSender::new();
    sender.push_ok(json!({"id": "in1", "type": "upload"}));
    let client = client(sender.clone());

    client
        .jobs()
        .upload(&staged_job(), b"PAYLOAD-BYTES".to_vec(), Some("report.pdf"))
        .expect("upload");

    let req = sender.last_request();
    let boundary = boundary_of(&req);
    assert!(!boundary.is_empty());

    let body = req.body_string();
    assert!(
        body.starts_with(&format!("--{boundary}\r\n")),
        "body must open with the boundary"
    );
    assert!(body.contains("Content-Disposition: form-data; name=\"file\"; filename=\"report.pdf\""));
    assert!(body.contains("Content-Type: application/octet-stream\r\n\r\n"));
    assert!(body.contains("PAYLOAD-BYTES"));
    assert!(
        body.ends_with(&format!("\r\n--{boundary}--\r\n")),
        "body must close with the terminating boundary"
    );
}

#[test]
fn multipart_from_path_uses_the_basename() {
    let sender = FakeSender::new();
    sender.push_ok(json!({"id": "in1", "type": "upload"}));
    let client = client(sender.clone());

    let dir = unique_tmp_dir("upload-path");
    let file = dir.join("my document.png");
    std::fs::write(&file, b"PNGDATA").unwrap();

    client
        .jobs()
        .upload(&staged_job(), file.as_path(), None)
        .expect("upload");

    let body = sender.last_request().body_string();
    assert!(body.contains("filename=\"my document.png\""));
    assert!(body.contains("PNGDATA"));
}

#[test]
fn multipart_from_reader_streams_with_default_filename() {
    let sender = FakeSender::new();
    sender.push_ok(json!({"id": "in1", "type": "upload"}));
    let client = client(sender.clone());

    let reader = Input::reader(Cursor::new(b"STREAMED".to_vec()));
    client
        .jobs()
        .upload(&staged_job(), reader, None)
        .expect("upload");

    let body = sender.last_request().body_string();
    assert!(body.contains("filename=\"file\""));
    assert!(body.contains("STREAMED"));
}

#[test]
fn explicit_filename_overrides_and_injection_is_sanitized() {
    let sender = FakeSender::new();
    sender.push_ok(json!({"id": "in1", "type": "upload"}));
    let client = client(sender.clone());

    // A filename attempting CRLF header injection + a quote must be neutralized.
    let evil = "evil\"\r\nX-Injected: 1\r\n\r\nnope.png";
    client
        .jobs()
        .upload(&staged_job(), b"X".to_vec(), Some(evil))
        .expect("upload");

    let body = sender.last_request().body_string();
    // Header injection is prevented: the CRLF is stripped, so "X-Injected: 1"
    // can never appear as its own header line (the leftover text is harmless
    // inside the quoted filename value).
    assert!(
        !body.lines().any(|l| l.trim() == "X-Injected: 1"),
        "a CRLF-injected header line must not be produced"
    );
    let disposition_line = body
        .lines()
        .find(|l| l.starts_with("Content-Disposition:"))
        .expect("disposition line");
    // The injected quote is stripped: the line has exactly 4 quotes (two around
    // name="file", two around the filename) — a leaked quote would make 5.
    assert_eq!(
        disposition_line.matches('"').count(),
        4,
        "the injected quote must be stripped"
    );
    assert!(!disposition_line.contains('\r'));
}

#[test]
fn boundary_is_reused_across_a_retry() {
    let sender = FakeSender::new();
    // 429 is retryable for any method; a bytes body is replayable, so the upload
    // is retried — and the SAME boundary must be reused so Content-Type matches.
    sender.push_raw(429, b"", vec![]);
    sender.push_ok(json!({"id": "in1", "type": "upload"}));
    let client = client(sender.clone());

    client
        .jobs()
        .upload(&staged_job(), b"AGAIN".to_vec(), Some("f.bin"))
        .expect("upload");

    assert_eq!(
        sender.request_count(),
        2,
        "429 should retry a replayable upload"
    );
    let first = sender.request_at(0);
    let second = sender.request_at(1);
    assert_eq!(
        boundary_of(&first),
        boundary_of(&second),
        "boundary must be reused"
    );
    assert_eq!(first.body, second.body, "the exact body must be replayed");
}

#[test]
fn one_shot_reader_is_not_retried() {
    let sender = FakeSender::new();
    // Only one canned response: a retry would panic ("no canned response left").
    sender.push_raw(429, b"", vec![]);
    let client = client(sender.clone());

    let reader = Input::reader(Cursor::new(b"ONE-SHOT".to_vec()));
    let err = client
        .jobs()
        .upload(&staged_job(), reader, None)
        .expect_err("a non-replayable reader must not be retried");
    assert!(matches!(err, Api2ConvertError::RateLimit { .. }));
    assert_eq!(sender.request_count(), 1);
}

#[test]
fn upload_url_encodes_the_job_id() {
    let sender = FakeSender::new();
    sender.push_ok(json!({"id": "in1", "type": "upload"}));
    let client = client(sender.clone());

    let mut job = staged_job();
    job.id = "weird/id".to_string();
    client
        .jobs()
        .upload(&job, b"X".to_vec(), None)
        .expect("upload");

    assert_eq!(sender.last_request().path(), "/upload-file/weird%2Fid");
}
