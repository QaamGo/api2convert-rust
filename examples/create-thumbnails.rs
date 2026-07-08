//! Create thumbnails guide — render the first page of a PDF as a PNG thumbnail.
//!
//! ```sh
//! API2CONVERT_API_KEY=… cargo run --example create-thumbnails
//! ```

use api2convert::{Api2Convert, ConvertOptions};

const PDF: &str = "https://example-files.online-convert.com/document/pdf/example.pdf";

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
        PDF,
        "thumbnail",
        ConvertOptions::new()
            .category("operation")
            .option("thumbnail_target", "png")
            .option("width", 300)
            .option("pages", "first")
            .option("dpi", 150),
    )?;
    println!("job {} is {}", result.job().id, result.job().status.code);

    let path = result.save("out/", None)?;
    println!("saved {}", path.display());

    Ok(())
}
