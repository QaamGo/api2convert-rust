//! Dynamic path segments are percent-encoded so they cannot inject path structure.

mod common;

use common::*;
use serde_json::json;

#[test]
fn job_id_segment_is_encoded() {
    let sender = FakeSender::new();
    sender.push_ok(json!({"id": "x", "status": {"code": "completed"}}));
    let client = client(sender.clone());

    let _ = client.jobs().get("weird id/../x").expect("get");
    assert_eq!(sender.last_request().path(), "/jobs/weird%20id%2F..%2Fx");
}

#[test]
fn stats_path_is_built_from_segments() {
    let sender = FakeSender::new();
    sender.push_ok(json!({}));
    let client = client(sender.clone());

    let _ = client.stats().day("2026-07-06", None).expect("stats");
    assert_eq!(sender.last_request().path(), "/stats/day/2026-07-06/all");
}

#[test]
fn list_query_is_encoded() {
    let sender = FakeSender::new();
    sender.push_ok(json!([]));
    let client = client(sender.clone());

    let _ = client
        .jobs()
        .list(Some("completed"), Some(2))
        .expect("list");
    let path = sender.last_request().path();
    assert!(path.starts_with("/jobs?"));
    assert!(path.contains("page=2"));
    assert!(path.contains("status=completed"));
}
