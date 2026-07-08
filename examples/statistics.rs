//! Statistics guide — fetch conversion usage statistics for a month.
//!
//! ```sh
//! API2CONVERT_API_KEY=… cargo run --example statistics
//! ```

use api2convert::Api2Convert;

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

    let stats = client.stats().month("2026-06", None)?;
    println!("usage stats for 2026-06: {stats}");

    Ok(())
}
