//! Uploading files guide — upload a local file and convert it to PNG in one call.
//!
//! ```sh
//! API2CONVERT_API_KEY=… cargo run --example uploading-files
//! ```

use api2convert::Api2Convert;

/// A minimal valid 1×1 PNG, written to disk to exercise the real upload path.
const ONE_PX_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
    0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

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

    // Write a small local file to upload.
    let src = std::env::temp_dir().join("a2c-uploading-files.png");
    std::fs::write(&src, ONE_PX_PNG).expect("write local file");

    // One call stages the job, uploads the file with the per-job token, starts,
    // polls and returns the result.
    let result = client.convert(src.as_path(), "png")?;
    println!("job {} is {}", result.job().id, result.job().status.code);

    let bytes = result.contents(None)?;
    println!("downloaded {} bytes", bytes.len());

    Ok(())
}
