//! Official Rust SDK for the [API2Convert](https://www.api2convert.com) file-conversion API.
//!
//! One call uploads (or references a URL), starts the job, polls it to
//! completion and hands you the result:
//!
//! ```no_run
//! use api2convert::{Api2Convert, ConvertOptions};
//!
//! # fn main() -> Result<(), api2convert::Api2ConvertError> {
//! let client = Api2Convert::new("YOUR_API_KEY")?;
//!
//! // Convert a local file and save the result into a directory.
//! let result = client.convert("photo.heic", "jpg")?;
//! let path = result.save("out/", None)?;
//! println!("saved {}", path.display());
//!
//! // Convert a remote URL with target-specific options.
//! let result = client.convert_with(
//!     "https://example.com/input.png",
//!     "jpg",
//!     ConvertOptions::new().option("quality", 85),
//! )?;
//! let bytes = result.contents(None)?;
//! # let _ = bytes;
//! # Ok(())
//! # }
//! ```
//!
//! This crate is one of the official API2Convert SDKs (PHP, Python, Java,
//! Node.js, Go, Ruby, Rust) that implement the same language-agnostic contract
//! (`docs/SDK_CONTRACT.md`) and version together.
//!
//! ## Errors
//!
//! Every fallible call returns [`Result`], whose error is [`Api2ConvertError`].
//! Match it to react to specific conditions:
//!
//! ```no_run
//! # use api2convert::{Api2Convert, Api2ConvertError};
//! # fn f(client: &Api2Convert) {
//! match client.convert("in.png", "not-a-format") {
//!     Ok(result) => { let _ = result; }
//!     Err(Api2ConvertError::Validation(e)) => eprintln!("bad request: {}", e.message),
//!     Err(Api2ConvertError::RateLimit { retry_after, .. }) => {
//!         eprintln!("rate limited; retry after {:?}s", retry_after)
//!     }
//!     Err(e) => eprintln!("failed: {e}"),
//! }
//! # }
//! ```
//!
//! ## Security
//!
//! Account key, per-job upload token and download password ride in custom
//! `X-Oc-*` headers. Secret-bearing requests never follow redirects (only the
//! self-contained, no-secret download path does), uploads use the per-job token
//! (never the account key), and no secret ever appears in an error message. See
//! `SECURITY.md`.

#![forbid(unsafe_code)]

mod client;
mod config;
mod convert_options;
mod data;
mod errors;
mod models;
mod resources;
mod result;
mod transport;
mod upload;
mod version;
mod webhook;

pub mod enums;

pub use client::{Api2Convert, Input};
pub use config::{ClientBuilder, API_KEY_ENV, DEFAULT_BASE_URL};
pub use convert_options::{AsyncOptions, ConvertOptions};
pub use errors::{Api2ConvertError, ApiErrorData, Result};
pub use models::{Conversion, InputFile, Job, JobMessage, OutputFile, Preset, Status};
pub use resources::{
    ContractsResource, ConversionsResource, JobsResource, PresetsResource, StatsResource,
};
pub use result::{ConversionResult, FileDownload};
pub use version::VERSION;
pub use webhook::{WebhookEvent, WebhookVerifier};

// The pluggable transport seam — implement [`HttpSender`] to bring your own
// client or a test fake; [`Sleeper`] / [`Rng`] make backoff injectable.
pub use transport::{Headers, HttpRequest, HttpResponse, HttpSender, Rng, Sleeper};
