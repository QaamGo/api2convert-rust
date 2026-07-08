//! Presets guide — list saved conversion presets for a category and target.
//!
//! ```sh
//! API2CONVERT_API_KEY=… cargo run --example presets
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

    let presets = client.presets().list(Some("video"), Some("mp4"), None)?;
    println!("{} preset(s) for video/mp4", presets.len());
    for preset in &presets {
        println!("- {} ({:?})", preset.name, preset.target);
    }

    Ok(())
}
