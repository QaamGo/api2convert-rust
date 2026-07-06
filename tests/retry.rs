//! Retry policy: idempotent requests retry on 5xx; a bare POST does not; 429
//! retries for any method; an Idempotency-Key opts a POST back into retrying.

mod common;

use std::sync::Arc;
use std::time::Duration;

use api2convert::Api2ConvertError;
use common::*;
use serde_json::json;

#[test]
fn idempotent_get_retries_on_503_then_succeeds() {
    let sender = FakeSender::new();
    let sleeper = Arc::new(RecordingSleeper::default());
    sender.push_raw(503, b"", vec![]);
    sender.push_ok(json!({"id": "job1", "status": {"code": "completed"}}));
    let client = client_with(sender.clone(), Arc::clone(&sleeper));

    let job = client.jobs().get("job1").expect("get");
    assert!(job.is_completed());
    assert_eq!(sender.request_count(), 2, "GET should retry once");
    // First backoff with zero jitter: 0.5 * 2^0 = 0.5s.
    assert_eq!(sleeper.durations(), vec![Duration::from_secs_f64(0.5)]);
}

#[test]
fn bare_post_is_not_retried_on_503() {
    let sender = FakeSender::new();
    sender.push_raw(503, b"", vec![]);
    let client = client(sender.clone());

    let err = client
        .jobs()
        .create(json!({}), None)
        .expect_err("should fail");
    assert!(matches!(err, Api2ConvertError::Server(_)));
    assert_eq!(
        sender.request_count(),
        1,
        "a non-idempotent POST must not be re-sent"
    );
}

#[test]
fn post_with_idempotency_key_retries() {
    let sender = FakeSender::new();
    sender.push_raw(503, b"", vec![]);
    sender.push_ok(json!({"id": "job1", "status": {"code": "created"}}));
    let client = client(sender.clone());

    let job = client
        .jobs()
        .create(json!({}), Some("idem-1"))
        .expect("create");
    assert_eq!(job.id, "job1");
    assert_eq!(sender.request_count(), 2);
}

#[test]
fn post_retries_on_429_for_any_method() {
    let sender = FakeSender::new();
    sender.push_raw(429, b"", vec![]);
    sender.push_ok(json!({"id": "job1", "status": {"code": "created"}}));
    let client = client(sender.clone());

    let job = client.jobs().create(json!({}), None).expect("create");
    assert_eq!(job.id, "job1");
    assert_eq!(sender.request_count(), 2, "429 retries even a bare POST");
}

#[test]
fn retries_on_502_and_504() {
    let sender = FakeSender::new();
    sender.push_raw(502, b"", vec![]);
    sender.push_raw(504, b"", vec![]);
    sender.push_ok(json!({"id": "job1", "status": {"code": "completed"}}));
    let client = client(sender.clone());

    let job = client.jobs().get("job1").expect("get");
    assert!(job.is_completed());
    assert_eq!(sender.request_count(), 3, "502 and 504 are retryable");
}

#[test]
fn idempotent_get_retries_on_a_network_error() {
    let sender = FakeSender::new();
    sender.push_network_error();
    sender.push_ok(json!({"id": "job1", "status": {"code": "completed"}}));
    let client = client(sender.clone());

    let job = client.jobs().get("job1").expect("get");
    assert!(job.is_completed());
    assert_eq!(sender.request_count(), 2);
}

#[test]
fn bare_post_is_not_retried_on_a_network_error() {
    let sender = FakeSender::new();
    sender.push_network_error();
    let client = client(sender.clone());

    let err = client
        .jobs()
        .create(json!({}), None)
        .expect_err("should fail");
    assert!(matches!(err, Api2ConvertError::Network(_)));
    assert_eq!(sender.request_count(), 1);
}

#[test]
fn backoff_grows_across_two_retries() {
    let sender = FakeSender::new();
    let sleeper = Arc::new(RecordingSleeper::default());
    sender.push_raw(503, b"", vec![]);
    sender.push_raw(503, b"", vec![]);
    sender.push_ok(json!({"id": "job1", "status": {"code": "completed"}}));
    let client = client_with(sender.clone(), Arc::clone(&sleeper));

    client.jobs().get("job1").expect("get");
    assert_eq!(sender.request_count(), 3);
    // Zero jitter: 0.5 * 2^0 = 0.5s, then 0.5 * 2^1 = 1.0s.
    assert_eq!(
        sleeper.durations(),
        vec![Duration::from_secs_f64(0.5), Duration::from_secs_f64(1.0)]
    );
}
