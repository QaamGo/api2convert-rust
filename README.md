# API2Convert Rust SDK

Official Rust SDK for the [API2Convert](https://www.api2convert.com) file-conversion API.

One call uploads (or references a URL), starts the job, polls it to completion and hands you the
result. It is one of the official API2Convert SDKs (PHP, Python, Java, Node.js, Go, Ruby, Rust) that
implement the same language-agnostic contract ([`docs/SDK_CONTRACT.md`](docs/SDK_CONTRACT.md)) and
version together.

- **Blocking / synchronous** API (like the Python, Go, Java, Ruby and PHP SDKs).
- **Lean dependencies** — `reqwest`, `serde_json`, `hmac`, `sha2`.
- **Secure by construction** — secret headers never cross a redirect, uploads use the per-job token
  (never the account key), and no secret ever appears in an error. See [`SECURITY.md`](SECURITY.md).

## Install

```toml
[dependencies]
api2convert = "10"
```

Requires Rust 1.86+ (set by the `reqwest` dependency tree).

## Quickstart

```rust
use api2convert::Api2Convert;

fn main() -> Result<(), api2convert::Api2ConvertError> {
    let client = Api2Convert::new("YOUR_API_KEY")?;

    // Convert a local file and save the result into a directory
    // (the API's filename is used, sanitized against path traversal).
    let result = client.convert("photo.heic", "jpg")?;
    let path = result.save("out/", None)?;
    println!("saved {}", path.display());

    Ok(())
}
```

The API key also comes from the `API2CONVERT_API_KEY` environment variable:

```rust
let client = api2convert::Api2Convert::from_env()?;
```

## Inputs

`convert` accepts a local path, a remote URL, in-memory bytes, or a streaming reader. A string
starting with `http://` / `https://` is treated as a URL (sent as a remote input); anything else is a
local path.

```rust
client.convert("https://example.com/in.png", "jpg")?;   // remote URL
client.convert("in.png", "jpg")?;                         // local path
client.convert(std::fs::read("in.png").unwrap(), "jpg")?; // bytes
client.convert(Input::reader(std::fs::File::open("in.png").unwrap()), "jpg")?; // stream
```

## Conversion options

Target-specific options are a separate map from the SDK controls, so an API option key can never
collide with an SDK argument:

```rust
use api2convert::{Api2Convert, ConvertOptions};

let result = client.convert_with(
    "in.png",
    "jpg",
    ConvertOptions::new()
        .option("quality", 85)
        .option("strip", true)
        .category("image"),
)?;
let bytes = result.contents(None)?;
```

Discover the valid options for a target:

```rust
let options = client.options("jpg", Some("image"))?;
```

## Download password

A password given at conversion time is remembered on the result and applied automatically:

```rust
use api2convert::{Api2Convert, ConvertOptions};

let result = client.convert_with(
    "secret.pdf",
    "png",
    ConvertOptions::new().download_password("s3cr3t"),
)?;
result.save("out/", None)?; // password applied automatically
```

## Async (start now, download later / via webhook)

```rust
use api2convert::{Api2Convert, AsyncOptions};

let job = client.convert_async_with(
    "in.png",
    "jpg",
    AsyncOptions::new().callback("https://your.app/webhook"),
)?;
println!("started job {}", job.id);
```

Verify the webhook delivery in your handler (pass the **raw** request body and the `X-Oc-Signature`
header):

```rust
use api2convert::Api2Convert;

let event = Api2Convert::webhooks().construct_event(raw_body, signature, secret)?;
println!("job {} is {}", event.job.id, event.job.status.code);
```

## Errors

Every fallible call returns `Result<_, Api2ConvertError>`. Match it to react to specific conditions:

```rust
use api2convert::{Api2Convert, Api2ConvertError};

match client.convert("in.png", "not-a-format") {
    Ok(result) => { let _ = result; }
    Err(Api2ConvertError::Validation(e)) => eprintln!("bad request: {}", e.message),
    Err(Api2ConvertError::RateLimit { retry_after, .. }) => {
        eprintln!("rate limited; retry after {:?}s", retry_after)
    }
    Err(Api2ConvertError::ConversionFailed { errors, .. }) => {
        eprintln!("job failed: {errors:?}")
    }
    Err(e) => eprintln!("failed: {e}"),
}
```

HTTP-error variants expose `status()`, `request_id()` (the `X-Request-Id`) and `body()`.

## Full lifecycle control

`convert` is built on the `jobs()` resource, which you can drive directly:

```rust
use api2convert::Api2Convert;
use serde_json::json;

let job = client.jobs().create(json!({
    "conversion": [{ "target": "png" }],
    "process": false
}), None)?;
client.jobs().upload(&job, "in.jpg", None)?;
client.jobs().start(&job.id)?;
let job = client.jobs().wait(&job.id, None, true)?;
let outputs = client.jobs().outputs(&job.id)?;
```

Other resources: `conversions()`, `presets()`, `stats()`, `contracts()`.

## Configuration

```rust
use std::time::Duration;
use api2convert::Api2Convert;

let client = Api2Convert::builder()
    .api_key("YOUR_API_KEY")
    .timeout(Duration::from_secs(60))
    .max_retries(3)
    .poll_interval(Duration::from_secs(1))
    .poll_timeout(Duration::from_secs(600))
    .build()?;
```

| Knob | Default | Notes |
|---|---|---|
| `base_url` | `https://api.api2convert.com/v2` | trailing `/` trimmed |
| `timeout` | 30s | JSON requests; min 1s |
| `max_retries` | 2 | transient failures |
| `poll_interval` | 1s | floored to 500ms |
| `poll_max_interval` | 5s | backoff ceiling |
| `poll_timeout` | 300s | capped at 14400s (4h) |
| `max_download_bytes` | 0 (unlimited) | cap downloaded size |

## Examples

One runnable example per documented guide lives in [`examples/`](examples/). Each
reads the key from `API2CONVERT_API_KEY` (and honors `API2CONVERT_BASE_URL`):

```sh
API2CONVERT_API_KEY=<key> cargo run --example quickstart
```

| Example | Guide |
|---|---|
| [`quickstart`](examples/quickstart.rs) | Convert a remote JPG to PNG, fetch the job, download |
| [`convert-files`](examples/convert-files.rs) | Browse the conversions catalog, then convert |
| [`uploading-files`](examples/uploading-files.rs) | Upload a local file and convert it |
| [`job-lifecycle`](examples/job-lifecycle.rs) | Drive create → add input → start → wait → outputs |
| [`add-watermark`](examples/add-watermark.rs) | Stamp a PNG watermark onto a PDF |
| [`create-thumbnails`](examples/create-thumbnails.rs) | Render a PDF page as a PNG thumbnail |
| [`compress-files`](examples/compress-files.rs) | Compress a JPG |
| [`create-archives`](examples/create-archives.rs) | Pack two files into a ZIP |
| [`create-hashes`](examples/create-hashes.rs) | Compute the SHA-256 of a ZIP |
| [`extract-assets`](examples/extract-assets.rs) | Extract embedded assets from a DOCX |
| [`file-analysis`](examples/file-analysis.rs) | Extract a JPG's metadata as JSON |
| [`compare-files`](examples/compare-files.rs) | SSIM-diff two images |
| [`capture-website`](examples/capture-website.rs) | Screenshot a web page to PNG |
| [`audio-operations`](examples/audio-operations.rs) | Transcode WAV to AAC |
| [`image-operations`](examples/image-operations.rs) | Resize a JPG |
| [`webhooks`](examples/webhooks.rs) | Start an async conversion with a callback |
| [`presets`](examples/presets.rs) | List saved conversion presets |
| [`statistics`](examples/statistics.rs) | Fetch monthly usage statistics |
| [`rate-limits`](examples/rate-limits.rs) | Inspect the account contract / limits |
| [`authentication`](examples/authentication.rs) | Verify the key by listing jobs |

## Testing

```sh
cargo test                              # unit + offline + security
cargo test --test security              # redirect/leak guarantees only
API2CONVERT_API_KEY=<key> cargo test --test live -- --ignored   # live conformance
```

The [live conformance suite](tests/live.rs) is the executable twin of the
examples above: one live test per guide runs the same operation against the real
API and asserts success, plus two negative tests (an unknown target is a typed
validation error; a bad key is typed and never leaked).

It runs automatically against the real API on every release tag (see
`.github/workflows/live-conformance.yml`), so a published version is always
verified end to end.

## License

MIT © Qaamgo Media GmbH.
