//! Rate limits guide — inspect the account contract (plan, quotas, limits).
//!
//! ```sh
//! API2CONVERT_API_KEY=… cargo run --example rate-limits
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

    let contract = client.contracts().get()?;
    println!("account contract: {contract}");

    Ok(())
}
