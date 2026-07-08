//! Webhooks guide — start an async conversion with a callback URL, then (in your
//! HTTP handler) verify the delivery.
//!
//! ```sh
//! API2CONVERT_API_KEY=… cargo run --example webhooks
//! ```

use api2convert::{Api2Convert, AsyncOptions};

const DOCX: &str = "https://example-files.online-convert.com/document/docx/example.docx";
const CALLBACK: &str = "https://your-app.example.com/api2convert/webhook";

/// Build a client from `API2CONVERT_API_KEY` (and optional `API2CONVERT_BASE_URL`).
fn client() -> Api2Convert {
    let mut builder = Api2Convert::builder();
    if let Ok(base) = std::env::var("API2CONVERT_BASE_URL") {
        if !base.is_empty() {
            builder = builder.base_url(base);
        }
    }
    builder
        .build()
        .expect("set API2CONVERT_API_KEY (and optionally API2CONVERT_BASE_URL)")
}

fn main() -> Result<(), api2convert::Api2ConvertError> {
    let client = client();

    // Start the conversion and return immediately; the API POSTs the callback
    // URL when the job finishes. We do not wait here.
    let job = client.convert_async_with(
        DOCX,
        "pdf",
        AsyncOptions::new().category("document").callback(CALLBACK),
    )?;
    println!("started job {} ({})", job.id, job.status.code);
    println!("api2convert will POST {CALLBACK} when it finishes");

    Ok(())
}

/// In your webhook HTTP handler, verify the delivery against the `X-Oc-Signature`
/// header using your signing secret. Pass the **raw** request body (never a
/// re-serialized copy). An empty secret skips verification.
#[allow(dead_code)]
fn verify_delivery(raw_body: &[u8], signature: Option<&str>, secret: &str) {
    match Api2Convert::webhooks().construct_event(raw_body, signature, secret) {
        Ok(event) => println!(
            "verified webhook: job {} is {}",
            event.job.id, event.job.status.code
        ),
        Err(e) => eprintln!("invalid webhook: {e}"),
    }
}
