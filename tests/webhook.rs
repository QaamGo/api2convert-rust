//! Webhook signature verification and parsing.

use api2convert::{Api2Convert, Api2ConvertError};
use hmac::{Hmac, Mac};
use sha2::Sha256;

fn sign(secret: &str, payload: &[u8]) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
    mac.update(payload);
    mac.finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

#[test]
fn verifies_a_valid_signature() {
    let payload = br#"{"id":"job1","status":{"code":"completed"}}"#;
    let secret = "whsec_test";
    let sig = sign(secret, payload);

    let event = Api2Convert::webhooks()
        .construct_event(payload, Some(&sig), secret)
        .expect("valid signature should verify");
    assert_eq!(event.job.id, "job1");
    assert!(event.job.is_completed());
}

#[test]
fn rejects_a_wrong_signature() {
    let payload = br#"{"id":"job1"}"#;
    let err = Api2Convert::webhooks()
        .construct_event(payload, Some("00deadbeef"), "whsec_test")
        .expect_err("wrong signature");
    assert!(matches!(err, Api2ConvertError::SignatureVerification(_)));
}

#[test]
fn missing_signature_with_a_secret_is_rejected() {
    let payload = br#"{"id":"j"}"#;
    let err = Api2Convert::webhooks()
        .construct_event(payload, None, "whsec_test")
        .expect_err("missing signature");
    assert!(matches!(err, Api2ConvertError::SignatureVerification(_)));
}

#[test]
fn empty_secret_skips_verification() {
    let payload = br#"{"id":"job7","status":{"code":"processing"}}"#;
    let event = Api2Convert::webhooks()
        .construct_event(payload, None, "")
        .expect("empty secret should skip verification");
    assert_eq!(event.job.id, "job7");
}

#[test]
fn parse_requires_a_json_object() {
    let hooks = Api2Convert::webhooks();
    assert!(hooks.parse(br#"{"id":"j"}"#).is_ok());
    assert!(hooks.parse(b"[]").is_err());
    assert!(hooks.parse(b"not json").is_err());
}
