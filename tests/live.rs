//! Live conformance suite — the canonical, cross-SDK set of scenarios that
//! exercises the real API2Convert API end to end. Every scenario is written to
//! read like a usage example, so this file doubles as an executable tour of the
//! SDK: build a client, convert, discover, drive the job lifecycle, and handle
//! the typed errors.
//!
//! Because these hit the real API and consume quota, every test is `#[ignore]`
//! and additionally skips unless `API2CONVERT_API_KEY` is set:
//!
//! ```sh
//! API2CONVERT_API_KEY=<key> cargo test --test live -- --ignored --nocapture
//! ```
//!
//! `API2CONVERT_BASE_URL` overrides the host (e.g. a beta environment). Never
//! commit a real key — it is read only from the environment.
//!
//! The seven scenarios mirror the shared spec implemented by every api2convert
//! SDK (php, python, java, go, nodejs, dotnet, ruby, rust):
//!
//! 1. [`convert_remote_url_to_png`]                 — one-call convert of a URL
//! 2. [`upload_local_file_and_convert`]             — multipart upload of a file
//! 3. [`convert_with_options`]                      — apply conversion options
//! 4. [`discover_conversion_catalog`]               — options/catalog discovery
//! 5. [`manual_job_lifecycle_and_inspection`]       — create → input → start → wait
//! 6. [`invalid_target_is_a_typed_error`]           — validation error handling
//! 7. [`authentication_error_leaks_no_secret`]      — auth error, no key leak

use api2convert::{Api2Convert, Api2ConvertError, ConvertOptions};
use serde_json::json;

/// A small, stable public image used as a remote input.
const REMOTE_JPG: &str =
    "https://example-files.online-convert.com/raster%20image/jpg/example_small.jpg";

/// A minimal valid 1×1 PNG, written to disk to exercise the real multipart
/// upload handshake (remote-URL inputs skip upload entirely).
const ONE_PX_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
    0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

/// Build a client from the environment, or `None` (skip) when no key is set.
///
/// This is the idiomatic construction: `Api2Convert::from_env()` reads
/// `API2CONVERT_API_KEY`; here we also honor `API2CONVERT_BASE_URL` so the same
/// suite can target prod or a beta host.
fn live_client() -> Option<Api2Convert> {
    let key = std::env::var("API2CONVERT_API_KEY").ok()?;
    if key.is_empty() {
        return None;
    }
    let mut builder = Api2Convert::builder().api_key(key);
    if let Ok(base) = std::env::var("API2CONVERT_BASE_URL") {
        if !base.is_empty() {
            builder = builder.base_url(base);
        }
    }
    builder.build().ok()
}

/// A per-test scratch directory (tests run in parallel, so each needs its own).
fn scratch_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("a2c-live-{}-{tag}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("create scratch dir");
    dir
}

/// Skip the test (pass) unless a live client is available. Usage:
/// `let client = require_client!();`.
macro_rules! require_client {
    () => {
        match live_client() {
            Some(client) => client,
            None => {
                eprintln!("skipping: API2CONVERT_API_KEY not set");
                return;
            }
        }
    };
}

// 1. One-call convert of a remote URL ---------------------------------------
//
// The simplest usage: hand `convert` a URL and a target format. The SDK creates
// a server-side-fetch job, polls it to completion, and hands back a result you
// can save straight to disk.
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn convert_remote_url_to_png() {
    let client = require_client!();

    let result = client
        .convert(REMOTE_JPG, "png")
        .expect("convert remote URL");
    assert!(result.job().is_completed(), "job should complete");

    let path = result
        .save(scratch_dir("remote"), None)
        .expect("save output");
    let meta = std::fs::metadata(&path).expect("stat output");
    assert!(meta.len() > 0, "output should be non-empty");
}

// 2. Upload and convert a local file ----------------------------------------
//
// For a local path (or bytes / a reader), the SDK stages the job, streams the
// file to the per-job upload server (authenticated with the job's `X-Oc-Token`,
// never your account key), starts it, polls, and downloads.
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn upload_local_file_and_convert() {
    let client = require_client!();

    let src = scratch_dir("upload").join("pixel.png");
    std::fs::write(&src, ONE_PX_PNG).expect("write source file");

    let result = client
        .convert(src.as_path(), "jpg")
        .expect("convert uploaded file");
    assert!(result.job().is_completed(), "uploaded job should complete");

    let bytes = result.contents(None).expect("download output");
    assert!(!bytes.is_empty(), "converted output should be non-empty");
    // A JPEG starts with the SOI marker 0xFF 0xD8.
    assert_eq!(&bytes[0..2], &[0xFF, 0xD8], "output should be a JPEG");
}

// 3. Apply conversion options -----------------------------------------------
//
// Pass target-specific options through `ConvertOptions`. They are kept strictly
// separate from the SDK's own controls, so an option key can never collide with
// an SDK argument. Discover the valid keys for a target with `client.options`
// (see the next scenario); here we re-encode at a lower JPEG quality.
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn convert_with_options() {
    let client = require_client!();

    let result = client
        .convert_with(
            REMOTE_JPG,
            "jpg",
            // Add e.g. .option("width", 64).option("height", 64) to resize.
            ConvertOptions::new().option("quality", 50),
        )
        .expect("convert with options");
    assert!(result.job().is_completed(), "job should complete");

    let bytes = result.contents(None).expect("download output");
    assert!(!bytes.is_empty(), "converted output should be non-empty");
}

// 4. Discover the conversion catalog ----------------------------------------
//
// `conversions().list` and `options` describe what the API can do — which
// targets exist and which options each accepts. Neither consumes conversion
// quota, so they are cheap to call before building a request.
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn discover_conversion_catalog() {
    let client = require_client!();

    // Which conversions target `jpg`?
    let conversions = client
        .conversions()
        .list(None, Some("jpg"), None)
        .expect("list conversions");
    assert!(
        !conversions.is_empty(),
        "the catalog should list at least one conversion to jpg"
    );

    // The option schema for a target (type / enum / default / range per option).
    let _options = client
        .options("png", Some("image"))
        .expect("fetch option schema");
}

// 5. Drive the full job lifecycle by hand -----------------------------------
//
// `convert` is built from these primitives. Driving them yourself unlocks
// compound/merge jobs, custom inputs, and step-by-step inspection: create a
// staged job, attach an input, start it, wait for completion, then inspect the
// job's status and output metadata.
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn manual_job_lifecycle_and_inspection() {
    let client = require_client!();
    let jobs = client.jobs();

    // Stage a job (process: false) so we can attach inputs before starting.
    let job = jobs
        .create(
            json!({ "process": false, "conversion": [{ "target": "png" }] }),
            None,
        )
        .expect("create staged job");
    assert!(!job.id.is_empty(), "a created job has an id");

    // Attach a remote input, then start processing.
    jobs.add_input(&job.id, json!({ "type": "remote", "source": REMOTE_JPG }))
        .expect("attach remote input");
    jobs.start(&job.id).expect("start job");

    // Poll to a terminal status.
    let finished = jobs.wait(&job.id, None, true).expect("wait for job");
    assert!(finished.is_completed(), "job should complete");

    // Inspect the outputs — both from the finished job and via the outputs API.
    assert!(!finished.output.is_empty(), "job should have an output");
    let outputs = jobs.outputs(&job.id).expect("fetch outputs");
    assert_eq!(
        outputs.len(),
        finished.output.len(),
        "outputs() should match the job's output list"
    );
    let out = &finished.output[0];
    assert!(!out.uri.is_empty(), "output has a download URI");
    assert!(
        out.size.map(|s| s > 0).unwrap_or(true),
        "output size, if reported, should be positive"
    );
}

// 6. Validation error on an unknown target ----------------------------------
//
// The API rejects an unknown target — either synchronously at create time
// (validation) or as a failed job. Both are typed errors you can match on.
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn invalid_target_is_a_typed_error() {
    let client = require_client!();

    match client.convert(REMOTE_JPG, "this-is-not-a-real-target") {
        Ok(_) => panic!("unknown target should fail"),
        Err(err) => assert!(
            matches!(
                err,
                Api2ConvertError::Validation(_) | Api2ConvertError::ConversionFailed { .. }
            ),
            "unexpected error: {err}"
        ),
    }
}

// 7. Authentication error, with no secret leak ------------------------------
//
// A bad key produces a typed `Authentication` error carrying the HTTP status.
// Crucially, the SDK never puts a credential into an error message — we assert
// the bogus key does not appear in the rendered error.
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn authentication_error_leaks_no_secret() {
    // This test only needs the API to be reachable, not a real key — but keep it
    // in the live suite so it exercises the real auth path.
    let _ = require_client!();

    const BOGUS_KEY: &str = "a2c-invalid-key-for-testing";
    let mut builder = Api2Convert::builder().api_key(BOGUS_KEY);
    if let Ok(base) = std::env::var("API2CONVERT_BASE_URL") {
        if !base.is_empty() {
            builder = builder.base_url(base);
        }
    }
    let client = builder.build().expect("build client with bogus key");

    match client.jobs().list(None, None) {
        Ok(_) => panic!("a bad key must not authenticate"),
        Err(err) => {
            assert!(
                matches!(err, Api2ConvertError::Authentication(_)),
                "expected an authentication error, got: {err}"
            );
            assert!(
                matches!(err.status(), Some(401) | Some(403)),
                "expected HTTP 401/403, got {:?}",
                err.status()
            );
            assert!(
                !format!("{err}").contains(BOGUS_KEY),
                "the error message must not leak the API key"
            );
        }
    }
}
