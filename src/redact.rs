//! Credential redaction for cloud connectors.
//!
//! Cloud `credentials` ride in the plaintext request body, so they must never
//! surface where a value object or an SDK-emitted string could leak them. This
//! module centralizes the masks the contract mandates:
//!
//! - the **whole `credentials` object** collapses to [`MARKER`] on every
//!   object-inspection path (a manual [`std::fmt::Debug`], see [`crate::cloud`]);
//! - any `parameters` leaf whose key contains a sensitive token
//!   ([`is_sensitive_key`], case-insensitive substring) collapses to [`MARKER`]
//!   (see [`parameters`]);
//! - the decoded error body is deep-walked ([`redact_body`]) as
//!   belt-and-suspenders — the API only ever echoes field *names*, never a
//!   credential *value*, but a future server/proxy change must not leak one.

use serde_json::{Map, Value};

/// The fixed, fleet-wide redaction marker (D9).
pub(crate) const MARKER: &str = "[REDACTED]";

/// Case-insensitive substrings that mark a key as carrying a secret.
const SENSITIVE_SUBSTRINGS: [&str; 11] = [
    "token",
    "password",
    "passwd",
    "secret",
    "key",
    "keyfile",
    "credential",
    "passphrase",
    "sas",
    "sig",
    "signature",
];

/// Whether a key name marks its value as sensitive (case-insensitive substring
/// match). `keyfile` is redundant with `key` but kept for parity with the fleet.
pub(crate) fn is_sensitive_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    SENSITIVE_SUBSTRINGS
        .iter()
        .any(|needle| lower.contains(needle))
}

/// A copy of a `parameters` map with sensitive leaves masked to [`MARKER`]: any
/// key matching [`is_sensitive_key`] has its value replaced; nested objects are
/// walked recursively. Non-secret keys (`bucket`, `host`, `file`, `container`,
/// `projectid`, …) are left untouched. Used by the manual `Debug` impls.
pub(crate) fn parameters(params: &Map<String, Value>) -> Map<String, Value> {
    let mut out = Map::new();
    for (key, value) in params {
        if is_sensitive_key(key) {
            out.insert(key.clone(), Value::String(MARKER.to_string()));
        } else {
            out.insert(key.clone(), redact_value(value));
        }
    }
    out
}

/// Deep-walk a decoded error body and mask the value of every sensitive key
/// (including a flattened/dotted key like
/// `input.0.credentials.secretaccesskey`) to [`MARKER`]. Non-object/array values
/// pass through unchanged.
pub(crate) fn redact_body(body: &Value) -> Value {
    redact_value(body)
}

fn redact_value(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out = Map::new();
            for (key, v) in map {
                if is_sensitive_key(key) {
                    out.insert(key.clone(), Value::String(MARKER.to_string()));
                } else {
                    out.insert(key.clone(), redact_value(v));
                }
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(redact_value).collect()),
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn sensitive_key_matches_substrings_case_insensitively() {
        assert!(is_sensitive_key("secretaccesskey"));
        assert!(is_sensitive_key("Password"));
        assert!(is_sensitive_key("accountKey"));
        assert!(is_sensitive_key("SAS"));
        assert!(is_sensitive_key("token"));
        assert!(!is_sensitive_key("bucket"));
        assert!(!is_sensitive_key("host"));
        assert!(!is_sensitive_key("projectid"));
    }

    #[test]
    fn parameters_masks_only_sensitive_leaves() {
        let p = json!({"bucket": "b", "token": "PARAMSECRET"})
            .as_object()
            .unwrap()
            .clone();
        let red = parameters(&p);
        assert_eq!(red["bucket"], json!("b"));
        assert_eq!(red["token"], json!(MARKER));
    }

    #[test]
    fn redact_body_walks_nested_and_dotted_keys() {
        let body = json!({
            "message": "Validation failed",
            "errors": {"input.0.credentials.secretaccesskey": "SUPERSECRET123"}
        });
        let red = redact_body(&body);
        let s = red.to_string();
        assert!(!s.contains("SUPERSECRET123"));
        assert!(s.contains(MARKER));
        // Non-secret text survives.
        assert_eq!(red["message"], json!("Validation failed"));
    }
}
