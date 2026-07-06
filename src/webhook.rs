//! Webhook signature verification and parsing. **Hand-authored.** Obtain a
//! verifier via [`Api2Convert::webhooks`](crate::Api2Convert::webhooks) — it
//! needs no configured client.

use hmac::{Hmac, Mac};
use serde_json::Value;
use sha2::Sha256;

use crate::errors::{Api2ConvertError, Result};
use crate::models::Job;

type HmacSha256 = Hmac<Sha256>;

/// A verified/parsed webhook delivery: the [`Job`] it describes plus the full
/// decoded payload.
#[derive(Debug, Clone)]
pub struct WebhookEvent {
    pub job: Job,
    pub payload: Value,
}

/// Verifies and parses webhook deliveries.
pub struct WebhookVerifier;

impl WebhookVerifier {
    /// Verify the `X-Oc-Signature` (HMAC-SHA256 over the **raw** body,
    /// hex-encoded, compared in constant time) and then parse the payload. An
    /// **empty `secret` skips verification** (for accounts not yet on signed
    /// webhooks). A missing/wrong signature — or an unparseable payload —
    /// yields [`Api2ConvertError::SignatureVerification`].
    pub fn construct_event(
        &self,
        payload: &[u8],
        signature: Option<&str>,
        secret: &str,
    ) -> Result<WebhookEvent> {
        if !secret.is_empty() {
            let sig = signature
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .ok_or_else(|| {
                    Api2ConvertError::SignatureVerification("missing signature".to_string())
                })?;
            let provided = decode_hex(sig).ok_or_else(|| {
                Api2ConvertError::SignatureVerification("signature is not valid hex".to_string())
            })?;
            let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).map_err(|_| {
                Api2ConvertError::SignatureVerification("invalid signing secret".to_string())
            })?;
            mac.update(payload);
            mac.verify_slice(&provided).map_err(|_| {
                Api2ConvertError::SignatureVerification("signature mismatch".to_string())
            })?;
        }
        self.parse(payload)
    }

    /// Parse a webhook payload **without** verifying its signature (for the
    /// pre-signed-webhooks period).
    pub fn parse(&self, payload: &[u8]) -> Result<WebhookEvent> {
        let value: Value = serde_json::from_slice(payload).map_err(|_| {
            Api2ConvertError::SignatureVerification("webhook payload is not valid JSON".to_string())
        })?;
        if !value.is_object() {
            return Err(Api2ConvertError::SignatureVerification(
                "webhook payload is not a JSON object".to_string(),
            ));
        }
        let job = Job::from_value(&value);
        Ok(WebhookEvent {
            job,
            payload: value,
        })
    }
}

fn decode_hex(s: &str) -> Option<Vec<u8>> {
    if s.len() % 2 != 0 {
        return None;
    }
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).ok())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decode_hex_handles_valid_and_invalid() {
        assert_eq!(decode_hex("00ff10"), Some(vec![0x00, 0xff, 0x10]));
        assert_eq!(decode_hex(""), Some(vec![]));
        assert_eq!(decode_hex("abc"), None); // odd length
        assert_eq!(decode_hex("zz"), None); // non-hex
    }
}
