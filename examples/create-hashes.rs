//! Create hashes guide — compute the SHA-256 checksum of a remote ZIP.
//!
//! ```sh
//! API2CONVERT_API_KEY=… cargo run --example create-hashes
//! ```

use api2convert::{Api2Convert, ConvertOptions};

const ZIP: &str = "https://example-files.online-convert.com/archive/zip/example.zip";

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

    let result = client.convert_with(ZIP, "sha256", ConvertOptions::new().category("hash"))?;
    println!("job {} is {}", result.job().id, result.job().status.code);

    // The hash output is a small text file; read it into memory.
    let bytes = result.contents(None)?;
    println!("sha256: {}", String::from_utf8_lossy(&bytes).trim());

    Ok(())
}
