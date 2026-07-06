//! Transport edge cases: unexpected redirect on the JSON path, non-JSON 2xx,
//! empty 2xx body, the idempotency-key header, and the standard request headers.

mod common;

use api2convert::Api2ConvertError;
use common::*;
use serde_json::json;

#[test]
fn unexpected_redirect_on_json_path_is_a_network_error() {
    let sender = FakeSender::new();
    sender.push_raw(
        302,
        b"",
        vec![("location".into(), "https://elsewhere.example/x".into())],
    );
    let client = client_no_retry(sender);

    let err = client
        .jobs()
        .get("j")
        .expect_err("3xx must not be treated as success");
    assert!(matches!(err, Api2ConvertError::Network(_)));
}

#[test]
fn non_json_2xx_is_a_network_error() {
    let sender = FakeSender::new();
    sender.push_raw(200, b"this is not json{", vec![]);
    let client = client(sender);

    let err = client.jobs().get("j").expect_err("non-JSON 2xx must error");
    assert!(matches!(err, Api2ConvertError::Network(_)));
}

#[test]
fn empty_2xx_body_is_accepted() {
    let sender = FakeSender::new();
    sender.push_raw(200, b"", vec![]);
    let client = client(sender);

    // cancel ignores the (empty) body and must succeed.
    client.jobs().cancel("j").expect("cancel with empty body");
}

#[test]
fn idempotency_key_header_is_sent_when_provided() {
    let sender = FakeSender::new();
    sender.push_ok(json!({"id": "job1", "status": {"code": "created"}}));
    let client = client(sender.clone());

    client
        .jobs()
        .create(json!({}), Some("idem-key-123"))
        .expect("create");
    assert_eq!(
        sender.last_request().header("idempotency-key"),
        Some("idem-key-123")
    );
}

#[test]
fn idempotency_key_header_absent_by_default() {
    let sender = FakeSender::new();
    sender.push_ok(json!({"id": "job1", "status": {"code": "created"}}));
    let client = client(sender.clone());

    client.jobs().create(json!({}), None).expect("create");
    assert!(sender.last_request().header("idempotency-key").is_none());
}

#[test]
fn account_requests_carry_the_standard_headers() {
    let sender = FakeSender::new();
    sender.push_ok(json!({"id": "job1", "status": {"code": "created"}}));
    let client = client(sender.clone());

    client.jobs().get("job1").expect("get");
    let req = sender.last_request();
    assert_eq!(req.header("x-oc-api-key"), Some("test-key"));
    assert_eq!(req.header("accept"), Some("application/json"));
    assert!(req
        .header("user-agent")
        .unwrap()
        .starts_with("api2convert-rust/"));
}
