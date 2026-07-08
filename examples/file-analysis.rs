//! File analysis guide — extract a JPG's metadata as JSON.
//!
//! ```sh
//! API2CONVERT_API_KEY=… cargo run --example file-analysis
//! ```

use api2convert::{Api2Convert, ConvertOptions};

const JPG: &str = "https://example-files.online-convert.com/raster%20image/jpg/example.jpg";

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

    let result = client.convert_with(JPG, "json", ConvertOptions::new().category("metadata"))?;
    println!("job {} is {}", result.job().id, result.job().status.code);

    // The analysis result is a JSON document; download it.
    let bytes = result.contents(None)?;
    println!("metadata JSON ({} bytes)", bytes.len());

    Ok(())
}
