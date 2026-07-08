//! Quickstart guide — convert a remote JPG to PNG, fetch the job, download the result.
//!
//! ```sh
//! API2CONVERT_API_KEY=… cargo run --example quickstart
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

    // Convert a remote JPG to PNG: create → poll to completion → result.
    let result = client.convert(JPG, "png")?;
    println!("job {} is {}", result.job().id, result.job().status.code);

    // Fetch the same job by id.
    let job = client.jobs().get(&result.job().id)?;
    println!("fetched job {} with {} output(s)", job.id, job.output.len());

    // Download the output into ./out/ (the API filename is used).
    let path = result.save("out/", None)?;
    println!("saved {}", path.display());

    Ok(())
}
