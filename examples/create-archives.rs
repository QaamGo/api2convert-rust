//! Create archives guide — pack two remote files into a single ZIP archive.
//!
//! ```sh
//! API2CONVERT_API_KEY=… cargo run --example create-archives
//! ```

use api2convert::Api2Convert;
use serde_json::json;

const PDF: &str = "https://example-files.online-convert.com/document/pdf/example.pdf";
const PNG: &str = "https://example-files.online-convert.com/raster%20image/png/example.png";

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
    let jobs = client.jobs();

    // Multiple inputs, one archive target.
    let job = jobs.create(
        json!({
            "process": true,
            "input": [
                { "type": "remote", "source": PDF },
                { "type": "remote", "source": PNG }
            ],
            "conversion": [{ "category": "archive", "target": "zip" }]
        }),
        None,
    )?;

    let finished = jobs.wait(&job.id, None, true)?;
    println!("job {} is {}", finished.id, finished.status.code);

    let outputs = jobs.outputs(&job.id)?;
    for out in &outputs {
        println!("archive: {}", out.uri);
    }

    Ok(())
}
