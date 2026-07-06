//! Verify a webhook delivery.
//!
//! Reads the raw request body from stdin and verifies it against the
//! `X-Oc-Signature` header value (`A2C_WEBHOOK_SIGNATURE`) using your signing
//! secret (`A2C_WEBHOOK_SECRET`). An empty/unset secret skips verification (the
//! pre-signed-webhooks period).
//!
//! ```sh
//! A2C_WEBHOOK_SECRET=whsec_… A2C_WEBHOOK_SIGNATURE=… \
//!   cargo run --example webhook < delivery.json
//! ```
//!
//! In a real service, wire this into your HTTP handler: pass the exact raw
//! request body (never a re-serialized copy) and the `X-Oc-Signature` header.

use std::io::Read;
use std::process::exit;

use api2convert::Api2Convert;

fn main() {
    let mut body = Vec::new();
    if std::io::stdin().read_to_end(&mut body).is_err() {
        eprintln!("failed to read the webhook body from stdin");
        exit(1);
    }

    let signature = std::env::var("A2C_WEBHOOK_SIGNATURE").ok();
    let secret = std::env::var("A2C_WEBHOOK_SECRET").unwrap_or_default();

    match Api2Convert::webhooks().construct_event(&body, signature.as_deref(), &secret) {
        Ok(event) => {
            println!(
                "verified webhook: job {} is {}",
                event.job.id, event.job.status.code
            );
        }
        Err(e) => {
            eprintln!("invalid webhook: {e}");
            exit(1);
        }
    }
}
