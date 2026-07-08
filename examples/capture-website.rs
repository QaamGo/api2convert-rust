//! Capture website guide — screenshot a web page to PNG with the screenshot engine.
//!
//! ```sh
//! API2CONVERT_API_KEY=… cargo run --example capture-website
//! ```

use api2convert::Api2Convert;
use serde_json::json;

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

    // The input uses the `screenshot` engine; the conversion renders a PNG.
    let job = jobs.create(
        json!({
            "process": true,
            "input": [{
                "type": "remote",
                "source": "https://www.online-convert.com",
                "engine": "screenshot",
                "options": {
                    "screen_width": 1280,
                    "screen_height": 1024,
                    "device_scale_factor": 1
                }
            }],
            "conversion": [{ "category": "image", "target": "png" }]
        }),
        None,
    )?;

    let finished = jobs.wait(&job.id, None, true)?;
    println!("job {} is {}", finished.id, finished.status.code);

    for out in &jobs.outputs(&job.id)? {
        println!("screenshot: {}", out.uri);
    }

    Ok(())
}
