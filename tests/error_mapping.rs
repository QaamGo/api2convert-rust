//! HTTP status → typed-error mapping, `X-Request-Id` capture, and 429 `Retry-After`.

mod common;

use api2convert::Api2ConvertError;
use common::*;
use serde_json::json;

fn error_for(status: u16, headers: Vec<(String, String)>) -> Api2ConvertError {
    let sender = FakeSender::new();
    sender.push_raw(
        status,
        serde_json::to_vec(&json!({"message": "boom"}))
            .unwrap()
            .as_slice(),
        {
            let mut h = vec![("content-type".to_string(), "application/json".to_string())];
            h.extend(headers);
            h
        },
    );
    let client = client_no_retry(sender);
    client.jobs().get("job1").expect_err("expected an error")
}

#[test]
fn maps_status_codes_to_variants() {
    assert!(matches!(
        error_for(401, vec![]),
        Api2ConvertError::Authentication(_)
    ));
    assert!(matches!(
        error_for(403, vec![]),
        Api2ConvertError::Authentication(_)
    ));
    assert!(matches!(
        error_for(402, vec![]),
        Api2ConvertError::PaymentRequired(_)
    ));
    assert!(matches!(
        error_for(404, vec![]),
        Api2ConvertError::NotFound(_)
    ));
    assert!(matches!(
        error_for(400, vec![]),
        Api2ConvertError::Validation(_)
    ));
    assert!(matches!(
        error_for(422, vec![]),
        Api2ConvertError::Validation(_)
    ));
    assert!(matches!(
        error_for(500, vec![]),
        Api2ConvertError::Server(_)
    ));
    assert!(matches!(error_for(418, vec![]), Api2ConvertError::Api(_)));
}

#[test]
fn exposes_status_message_and_request_id() {
    let err = error_for(404, vec![("x-request-id".into(), "req-abc".into())]);
    assert_eq!(err.status(), Some(404));
    assert_eq!(err.request_id(), Some("req-abc"));
    match err {
        Api2ConvertError::NotFound(d) => assert_eq!(d.message, "boom"),
        other => panic!("wrong variant: {other}"),
    }
}

#[test]
fn rate_limit_exposes_retry_after() {
    let err = error_for(429, vec![("retry-after".into(), "17".into())]);
    assert!(matches!(err, Api2ConvertError::RateLimit { .. }));
    assert_eq!(err.status(), Some(429));
    assert_eq!(err.retry_after(), Some(17));
}
