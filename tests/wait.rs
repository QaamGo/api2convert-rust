//! Polling: waits until terminal, raises on failure, and times out past the deadline.

mod common;

use std::time::Duration;

use api2convert::Api2ConvertError;
use common::*;
use serde_json::json;

#[test]
fn polls_until_completed() {
    let sender = FakeSender::new();
    sender.push_ok(json!({"id": "j", "status": {"code": "processing"}}));
    sender.push_ok(json!({"id": "j", "status": {"code": "queued"}}));
    sender.push_ok(json!({"id": "j", "status": {"code": "completed"}}));
    let client = client(sender.clone());

    let job = client
        .jobs()
        .wait("j", Some(Duration::from_secs(60)), true)
        .expect("wait");
    assert!(job.is_completed());
    assert_eq!(sender.request_count(), 3);
}

#[test]
fn failed_job_raises_conversion_failed() {
    let sender = FakeSender::new();
    sender.push_ok(json!({
        "id": "j",
        "status": {"code": "failed"},
        "errors": [{"message": "bad input", "code": 4001}]
    }));
    let client = client(sender);

    let err = client
        .jobs()
        .wait("j", Some(Duration::from_secs(60)), true)
        .expect_err("fail");
    match err {
        Api2ConvertError::ConversionFailed { job, errors } => {
            assert!(job.is_failed());
            assert_eq!(errors.first().unwrap().message, "bad input");
        }
        other => panic!("wrong variant: {other}"),
    }
}

#[test]
fn throw_on_failure_false_returns_the_failed_job() {
    let sender = FakeSender::new();
    sender.push_ok(json!({"id": "j", "status": {"code": "failed"}}));
    let client = client(sender);

    let job = client
        .jobs()
        .wait("j", Some(Duration::from_secs(60)), false)
        .expect("returns job");
    assert!(job.is_failed());
}

#[test]
fn times_out_past_deadline() {
    let sender = FakeSender::new();
    sender.push_ok(json!({"id": "j", "status": {"code": "processing"}}));
    let client = client(sender);

    let err = client
        .jobs()
        .wait("j", Some(Duration::ZERO), true)
        .expect_err("timeout");
    assert!(matches!(err, Api2ConvertError::ConversionTimeout { .. }));
}

#[test]
fn poll_interval_backs_off_by_1_5x_up_to_the_max() {
    use std::sync::Arc;
    let sender = FakeSender::new();
    let sleeper = Arc::new(RecordingSleeper::default());
    // processing x3 then completed -> three pauses between four polls.
    for _ in 0..3 {
        sender.push_ok(json!({"id": "j", "status": {"code": "processing"}}));
    }
    sender.push_ok(json!({"id": "j", "status": {"code": "completed"}}));
    let client = client_with(sender.clone(), Arc::clone(&sleeper));

    client
        .jobs()
        .wait("j", Some(Duration::from_secs(600)), true)
        .expect("wait");

    assert_eq!(sender.request_count(), 4);
    // Default poll_interval 1s, growth x1.5, zero jitter.
    assert_eq!(
        sleeper.durations(),
        vec![
            Duration::from_secs_f64(1.0),
            Duration::from_secs_f64(1.5),
            Duration::from_secs_f64(2.25),
        ]
    );
}
