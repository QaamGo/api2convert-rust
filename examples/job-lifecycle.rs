//! Job lifecycle guide — drive create → add input → start → wait → outputs by hand.
//!
//! ```sh
//! API2CONVERT_API_KEY=… cargo run --example job-lifecycle
//! ```

use api2convert::Api2Convert;
use serde_json::json;

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

    // Stage a job (process: false) so inputs can be attached before it starts.
    let job = jobs.create(
        json!({ "process": false, "conversion": [{ "category": "image", "target": "png" }] }),
        None,
    )?;
    println!("created staged job {}", job.id);

    // Attach a remote input, then start processing.
    jobs.add_input(&job.id, json!({ "type": "remote", "source": JPG }))?;
    jobs.start(&job.id)?;

    // Poll to a terminal status.
    let finished = jobs.wait(&job.id, None, true)?;
    println!("job {} is {}", finished.id, finished.status.code);

    // Inspect the outputs.
    let outputs = jobs.outputs(&job.id)?;
    for out in &outputs {
        println!("output: {}", out.uri);
    }

    Ok(())
}
