//! Compare files guide — diff two images with the SSIM comparison operation.
//!
//! ```sh
//! API2CONVERT_API_KEY=… cargo run --example compare-files
//! ```

use api2convert::Api2Convert;
use serde_json::json;

const JPG_SMALL: &str =
    "https://example-files.online-convert.com/raster%20image/jpg/example_small.jpg";
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
    let jobs = client.jobs();

    // Two images in, one diff out.
    let job = jobs.create(
        json!({
            "process": true,
            "input": [
                { "type": "remote", "source": JPG_SMALL },
                { "type": "remote", "source": JPG }
            ],
            "conversion": [{
                "category": "operation",
                "target": "compare-image",
                "options": { "method": "ssim", "threshold": 5, "diff_color": "red" }
            }]
        }),
        None,
    )?;

    let finished = jobs.wait(&job.id, None, true)?;
    println!("job {} is {}", finished.id, finished.status.code);

    for out in &jobs.outputs(&job.id)? {
        println!("diff: {}", out.uri);
    }

    Ok(())
}
