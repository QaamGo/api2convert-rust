//! Compress files guide — shrink a JPG with the compress operation.
//!
//! ```sh
//! API2CONVERT_API_KEY=… cargo run --example compress-files
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

    let result = client.convert_with(
        JPG,
        "compress",
        ConvertOptions::new()
            .category("operation")
            .option("compression_level", "high"),
    )?;
    println!("job {} is {}", result.job().id, result.job().status.code);

    let path = result.save("out/", None)?;
    println!("saved {}", path.display());

    Ok(())
}
