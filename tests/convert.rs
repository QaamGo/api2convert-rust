//! The one-call `convert` flows: remote URL vs. local file, and download-password
//! transparency.

mod common;

use common::*;
use serde_json::json;

#[test]
fn remote_url_creates_a_single_started_job_then_polls() {
    let sender = FakeSender::new();
    // create (started) then a poll that reports completion with an output.
    sender.push_ok(json!({"id": "job1", "status": {"code": "processing"}}));
    sender.push_ok(json!({
        "id": "job1",
        "status": {"code": "completed"},
        "output": [{"uri": "https://cdn.example/out.png", "filename": "out.png"}]
    }));
    let client = client(sender.clone());

    let result = client
        .convert("https://example.com/in.jpg", "png")
        .expect("convert");

    assert_eq!(result.output().unwrap().uri, "https://cdn.example/out.png");

    // request[0]: POST /jobs with a remote input and process:true.
    let create = sender.request_at(0);
    assert_eq!(create.method, "POST");
    assert_eq!(create.path(), "/jobs");
    let body = create.body_json();
    assert_eq!(body["conversion"][0]["target"], "png");
    assert_eq!(body["input"][0]["type"], "remote");
    assert_eq!(body["input"][0]["source"], "https://example.com/in.jpg");
    assert_eq!(body["process"], true);

    // request[1]: GET /jobs/job1 (poll).
    let poll = sender.request_at(1);
    assert_eq!(poll.method, "GET");
    assert_eq!(poll.path(), "/jobs/job1");
}

#[test]
fn local_file_stages_uploads_starts_then_polls() {
    let sender = FakeSender::new();
    // create (staged, carries server+token) → upload → start → poll(completed).
    sender.push_ok(json!({
        "id": "job1",
        "status": {"code": "created"},
        "server": "https://upload.example",
        "token": "job-token"
    }));
    sender.push_ok(json!({"id": "input1", "type": "upload"}));
    sender.push_ok(json!({"id": "job1", "status": {"code": "processing"}}));
    sender.push_ok(json!({
        "id": "job1",
        "status": {"code": "completed"},
        "output": [{"uri": "https://cdn.example/out.png"}]
    }));
    let client = client(sender.clone());

    let result = client
        .convert(b"binary-file-bytes".to_vec(), "png")
        .expect("convert");
    assert_eq!(result.output().unwrap().uri, "https://cdn.example/out.png");

    let create = sender.request_at(0);
    assert_eq!(create.method, "POST");
    assert_eq!(create.path(), "/jobs");
    assert_eq!(create.body_json()["process"], false);

    let upload = sender.request_at(1);
    assert_eq!(upload.method, "POST");
    assert_eq!(upload.path(), "/upload-file/job1");
    assert!(upload.url.starts_with("https://upload.example"));

    let start = sender.request_at(2);
    assert_eq!(start.method, "PATCH");
    assert_eq!(start.path(), "/jobs/job1");
    assert_eq!(start.body_json()["process"], true);

    let poll = sender.request_at(3);
    assert_eq!(poll.method, "GET");
}

#[test]
fn download_password_is_sent_on_create_and_remembered_for_download() {
    let sender = FakeSender::new();
    sender.push_ok(json!({"id": "job1", "status": {"code": "processing"}}));
    sender.push_ok(json!({
        "id": "job1",
        "status": {"code": "completed"},
        "output": [{"uri": "https://cdn.example/out.png"}]
    }));
    sender.push_raw(200, b"IMAGE", vec![]);
    let client = client(sender.clone());

    let result = client
        .convert_with(
            "https://example.com/in.jpg",
            "png",
            api2convert::ConvertOptions::new().download_password("s3cr3t"),
        )
        .expect("convert");

    // create carried download_passwords: ["s3cr3t"].
    let create = sender.request_at(0);
    assert_eq!(create.body_json()["download_passwords"][0], "s3cr3t");

    // download applies the remembered password automatically.
    let bytes = result.contents(None).expect("download");
    assert_eq!(bytes, b"IMAGE");
    let dl = sender.last_request();
    assert_eq!(dl.header("x-oc-download-password"), Some("s3cr3t"));
    assert!(
        !dl.follow_redirects,
        "a password download must not follow redirects"
    );
}

#[test]
fn convert_async_url_returns_without_polling() {
    let sender = FakeSender::new();
    sender.push_ok(json!({"id": "job1", "status": {"code": "processing"}}));
    let client = client(sender.clone());

    let job = client
        .convert_async("https://example.com/in.jpg", "png")
        .expect("convert_async");
    assert_eq!(job.id, "job1");
    // A single create call — no polling.
    assert_eq!(sender.request_count(), 1);
    assert_eq!(sender.request_at(0).method, "POST");
    assert_eq!(sender.request_at(0).path(), "/jobs");
}

#[test]
fn convert_async_local_uploads_and_starts_without_polling() {
    let sender = FakeSender::new();
    sender.push_ok(json!({
        "id": "job1",
        "status": {"code": "created"},
        "server": "https://upload.example",
        "token": "job-token"
    }));
    sender.push_ok(json!({"id": "in1", "type": "upload"}));
    sender.push_ok(json!({"id": "job1", "status": {"code": "processing"}}));
    let client = client(sender.clone());

    let job = client
        .convert_async(b"bytes".to_vec(), "png")
        .expect("convert_async");
    assert_eq!(job.id, "job1");
    // create + upload + start, but no GET poll.
    assert_eq!(sender.request_count(), 3);
    assert_eq!(sender.request_at(2).method, "PATCH");
    assert!(sender.requests().iter().all(|r| r.method != "GET"));
}

#[test]
fn convert_async_with_callback_sets_notify_status() {
    let sender = FakeSender::new();
    sender.push_ok(json!({"id": "job1", "status": {"code": "processing"}}));
    let client = client(sender.clone());

    client
        .convert_async_with(
            "https://example.com/in.jpg",
            "png",
            api2convert::AsyncOptions::new().callback("https://your.app/hook"),
        )
        .expect("convert_async");

    let body = sender.request_at(0).body_json();
    assert_eq!(body["callback"], "https://your.app/hook");
    assert_eq!(body["notify_status"], true);
}

#[test]
fn output_index_selects_the_right_output() {
    let sender = FakeSender::new();
    sender.push_ok(json!({"id": "job1", "status": {"code": "processing"}}));
    sender.push_ok(json!({
        "id": "job1",
        "status": {"code": "completed"},
        "output": [
            {"uri": "https://cdn.example/first.png"},
            {"uri": "https://cdn.example/second.png"}
        ]
    }));
    let client = client(sender);

    let result = client
        .convert_with(
            "https://example.com/in.jpg",
            "png",
            api2convert::ConvertOptions::new().output_index(1),
        )
        .expect("convert");
    assert_eq!(
        result.output().unwrap().uri,
        "https://cdn.example/second.png"
    );
    assert_eq!(result.outputs().len(), 2);
}
