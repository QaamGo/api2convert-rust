//! Convert files guide — browse the conversions catalog, then convert a JPG to PNG.
//!
//! ```sh
//! API2CONVERT_API_KEY=… cargo run --example convert-files
//! ```

use api2convert::Api2Convert;

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

    // The full catalog of supported conversions (cheap; no quota).
    let all = client.conversions().list(None, None, None)?;
    println!("catalog lists {} conversions", all.len());

    // Filter it down to conversions that target PNG.
    let to_png = client.conversions().list(None, Some("png"), None)?;
    println!("{} conversion(s) target png", to_png.len());

    // Convert the JPG to PNG.
    let result = client.convert(JPG, "png")?;
    println!("job {} is {}", result.job().id, result.job().status.code);

    let path = result.save("out/", None)?;
    println!("saved {}", path.display());

    Ok(())
}
