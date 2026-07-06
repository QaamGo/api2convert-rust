//! The multipart upload: correct URL, per-job token (never the account key),
//! `file` field, and guards for a missing server/token.

mod common;

use api2convert::{Api2ConvertError, Job, Status};
use common::*;
use serde_json::{json, Value};

fn staged_job(server: Option<&str>, token: Option<&str>) -> Job {
    Job {
        id: "job1".to_string(),
        status: Status {
            code: "created".to_string(),
            info: None,
        },
        token: token.map(str::to_string),
        server: server.map(str::to_string),
        callback: None,
        conversion: vec![],
        input: vec![],
        output: vec![],
        errors: vec![],
        warnings: vec![],
        raw: Value::Null,
    }
}

#[test]
fn uploads_with_the_job_token_and_never_the_account_key() {
    let sender = FakeSender::new();
    sender.push_ok(json!({"id": "input1", "type": "upload"}));
    let client = client(sender.clone());

    let job = staged_job(Some("https://upload.example"), Some("job-token"));
    let input = client
        .jobs()
        .upload(&job, b"file-bytes-here".to_vec(), Some("doc.pdf"))
        .expect("upload");
    assert_eq!(input.kind, "upload");

    let req = sender.last_request();
    assert_eq!(req.method, "POST");
    assert_eq!(req.path(), "/upload-file/job1");
    assert!(req.url.starts_with("https://upload.example"));
    assert_eq!(req.header("x-oc-token"), Some("job-token"));
    assert!(
        req.header("x-oc-api-key").is_none(),
        "the upload must not carry the account key"
    );
    let ct = req.header("content-type").unwrap();
    assert!(ct.starts_with("multipart/form-data; boundary="));

    let body = req.body_string();
    assert!(body.contains("name=\"file\""));
    assert!(body.contains("filename=\"doc.pdf\""));
    assert!(body.contains("file-bytes-here"));
}

#[test]
fn missing_server_is_a_config_error() {
    let sender = FakeSender::new();
    let client = client(sender);
    let job = staged_job(None, Some("job-token"));
    let err = client
        .jobs()
        .upload(&job, b"x".to_vec(), None)
        .expect_err("no server");
    assert!(matches!(err, Api2ConvertError::Config(_)));
}

#[test]
fn missing_token_is_a_config_error() {
    let sender = FakeSender::new();
    let client = client(sender);
    let job = staged_job(Some("https://upload.example"), None);
    let err = client
        .jobs()
        .upload(&job, b"x".to_vec(), None)
        .expect_err("no token");
    assert!(matches!(err, Api2ConvertError::Config(_)));
}
