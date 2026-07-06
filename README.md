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

## Testing

```sh
cargo test                              # unit + offline + security
cargo test --test security              # redirect/leak guarantees only
API2CONVERT_API_KEY=<key> cargo test --test live -- --ignored   # live conformance
```

## License

MIT © Qaamgo Media GmbH.
