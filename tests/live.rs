//! Live conformance suite — the canonical, cross-SDK set of scenarios that
//! exercises the real API2Convert API end to end. Every scenario mirrors one of
//! the documented guides on api2convert.com, so this file doubles as an
//! executable tour of the SDK: build a client, convert, upload, drive the job
//! lifecycle, run operations, and handle the typed errors.
//!
//! Because these hit the real API and consume quota, every test is `#[ignore]`
//! and additionally skips (passes) unless `API2CONVERT_API_KEY` is set:
//!
//! ```sh
//! API2CONVERT_API_KEY=<key> cargo test --test live -- --ignored --nocapture
//! ```
//!
//! `API2CONVERT_BASE_URL` overrides the host (e.g. a beta environment). Never
//! commit a real key — it is read only from the environment.
//!
//! The 20 positive scenarios map 1:1 to the documented-example catalog; two
//! negative scenarios (`invalid_target_is_a_typed_error`,
//! `authentication_error_leaks_no_secret`) guard the error contract.

use api2convert::{Api2Convert, Api2ConvertError, AsyncOptions, ConvertOptions};
use serde_json::json;

// Public example fixtures (example-files.online-convert.com).
const PDF: &str = "https://example-files.online-convert.com/document/pdf/example.pdf";
const PNG: &str = "https://example-files.online-convert.com/raster%20image/png/example.png";
const JPG: &str = "https://example-files.online-convert.com/raster%20image/jpg/example.jpg";
const JPG_SMALL: &str =
    "https://example-files.online-convert.com/raster%20image/jpg/example_small.jpg";
const WAV: &str = "https://example-files.online-convert.com/audio/wav/example.wav";
const DOCX: &str = "https://example-files.online-convert.com/document/docx/example.docx";
const ZIP: &str = "https://example-files.online-convert.com/archive/zip/example.zip";

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

/// Assert a saved file exists and is non-empty.
fn assert_saved_non_empty(path: &std::path::Path) {
    let meta = std::fs::metadata(path).expect("stat saved output");
    assert!(meta.len() > 0, "saved output should be non-empty");
}

// 1. quickstart — convert remote jpg -> png, fetch the job, download. ---------
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn quickstart() {
    let client = require_client!();

    let result = client
        .convert(JPG, "png")
        .expect("convert remote jpg to png");
    assert!(result.job().is_completed(), "job should complete");

    let job = client.jobs().get(&result.job().id).expect("get job by id");
    assert_eq!(job.id, result.job().id, "get() should return the same job");

    let path = result.save(scratch_dir("quickstart"), None).expect("save");
    assert_saved_non_empty(&path);
}

// 2. convert-files — browse the catalog, then convert jpg -> png. -------------
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn convert_files() {
    let client = require_client!();

    let all = client
        .conversions()
        .list(None, None, None)
        .expect("list full catalog");
    assert!(!all.is_empty(), "the catalog should be non-empty");

    let to_png = client
        .conversions()
        .list(None, Some("png"), None)
        .expect("list conversions to png");
    assert!(!to_png.is_empty(), "there should be conversions to png");

    let result = client.convert(JPG, "png").expect("convert jpg to png");
    assert!(result.job().is_completed(), "job should complete");
}

// 3. uploading-files — upload a local file and convert to png. ----------------
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn uploading_files() {
    let client = require_client!();

    let src = scratch_dir("uploading").join("pixel.png");
    std::fs::write(&src, ONE_PX_PNG).expect("write source file");

    let result = client
        .convert(src.as_path(), "png")
        .expect("upload and convert");
    assert!(result.job().is_completed(), "uploaded job should complete");

    let bytes = result.contents(None).expect("download output");
    assert!(!bytes.is_empty(), "output should be non-empty");
}

// 4. job-lifecycle — manual create -> add input -> start -> wait -> outputs. --
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn job_lifecycle() {
    let client = require_client!();
    let jobs = client.jobs();

    let job = jobs
        .create(
            json!({ "process": false, "conversion": [{ "category": "image", "target": "png" }] }),
            None,
        )
        .expect("create staged job");
    assert!(!job.id.is_empty(), "a created job has an id");

    jobs.add_input(&job.id, json!({ "type": "remote", "source": JPG }))
        .expect("attach remote input");
    jobs.start(&job.id).expect("start job");

    let finished = jobs.wait(&job.id, None, true).expect("wait for job");
    assert!(finished.is_completed(), "job should complete");

    let outputs = jobs.outputs(&job.id).expect("fetch outputs");
    assert!(!outputs.is_empty(), "job should produce outputs");
}

// 5. add-watermark — stamp a png onto a pdf (two remote inputs). --------------
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn add_watermark() {
    let client = require_client!();
    let jobs = client.jobs();

    let job = jobs
        .create(
            json!({
                "process": true,
                "input": [
                    { "type": "remote", "source": PDF },
                    { "type": "remote", "source": PNG }
                ],
                "conversion": [{
                    "category": "document",
                    "target": "pdf",
                    "options": { "stamp": true, "alignment": "center" }
                }]
            }),
            None,
        )
        .expect("create watermark job");

    let finished = jobs.wait(&job.id, None, true).expect("wait for job");
    assert!(finished.is_completed(), "job should complete");
    assert!(!finished.output.is_empty(), "job should produce outputs");
}

// 6. create-thumbnails — first page of a pdf as a png thumbnail. --------------
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn create_thumbnails() {
    let client = require_client!();

    let result = client
        .convert_with(
            PDF,
            "thumbnail",
            ConvertOptions::new()
                .category("operation")
                .option("thumbnail_target", "png")
                .option("width", 300)
                .option("pages", "first")
                .option("dpi", 150),
        )
        .expect("create thumbnail");
    assert!(result.job().is_completed(), "job should complete");

    let path = result.save(scratch_dir("thumbnail"), None).expect("save");
    assert_saved_non_empty(&path);
}

// 7. compress-files — compress a jpg. ----------------------------------------
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn compress_files() {
    let client = require_client!();

    let result = client
        .convert_with(
            JPG,
            "compress",
            ConvertOptions::new()
                .category("operation")
                .option("compression_level", "high"),
        )
        .expect("compress file");
    assert!(result.job().is_completed(), "job should complete");

    let path = result.save(scratch_dir("compress"), None).expect("save");
    assert_saved_non_empty(&path);
}

// 8. create-archives — pack two files into a zip. ----------------------------
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn create_archives() {
    let client = require_client!();
    let jobs = client.jobs();

    let job = jobs
        .create(
            json!({
                "process": true,
                "input": [
                    { "type": "remote", "source": PDF },
                    { "type": "remote", "source": PNG }
                ],
                "conversion": [{ "category": "archive", "target": "zip" }]
            }),
            None,
        )
        .expect("create archive job");

    let finished = jobs.wait(&job.id, None, true).expect("wait for job");
    assert!(finished.is_completed(), "job should complete");
    assert!(!finished.output.is_empty(), "job should produce a zip");
}

// 9. create-hashes — sha256 of a zip. ----------------------------------------
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn create_hashes() {
    let client = require_client!();

    let result = client
        .convert_with(ZIP, "sha256", ConvertOptions::new().category("hash"))
        .expect("hash file");
    assert!(result.job().is_completed(), "job should complete");

    let bytes = result.contents(None).expect("download hash");
    assert!(!bytes.is_empty(), "hash output should be non-empty");
}

// 10. extract-assets — pull embedded assets out of a docx. -------------------
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn extract_assets() {
    let client = require_client!();

    let result = client
        .convert_with(
            DOCX,
            "extract-assets",
            ConvertOptions::new().category("operation"),
        )
        .expect("extract assets");
    assert!(result.job().is_completed(), "job should complete");
    assert!(!result.outputs().is_empty(), "job should produce outputs");
}

// 11. file-analysis — extract jpg metadata as json. --------------------------
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn file_analysis() {
    let client = require_client!();

    let result = client
        .convert_with(JPG, "json", ConvertOptions::new().category("metadata"))
        .expect("analyze file");
    assert!(result.job().is_completed(), "job should complete");
    assert!(!result.outputs().is_empty(), "job should produce output");
}

// 12. compare-files — ssim diff of two images. -------------------------------
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn compare_files() {
    let client = require_client!();
    let jobs = client.jobs();

    let job = jobs
        .create(
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
        )
        .expect("create compare job");

    let finished = jobs.wait(&job.id, None, true).expect("wait for job");
    assert!(finished.is_completed(), "job should complete");
}

// 13. capture-website — screenshot a web page to png. ------------------------
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn capture_website() {
    let client = require_client!();
    let jobs = client.jobs();

    let job = jobs
        .create(
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
        )
        .expect("create screenshot job");

    let finished = jobs.wait(&job.id, None, true).expect("wait for job");
    assert!(finished.is_completed(), "job should complete");
    assert!(
        !finished.output.is_empty(),
        "job should produce a screenshot"
    );
}

// 14. audio-operations — transcode wav -> aac. -------------------------------
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn audio_operations() {
    let client = require_client!();

    let result = client
        .convert_with(
            WAV,
            "aac",
            ConvertOptions::new()
                .category("audio")
                .option("audio_codec", "aac")
                .option("audio_bitrate", 192)
                .option("channels", "stereo")
                .option("frequency", 44100),
        )
        .expect("transcode audio");
    assert!(result.job().is_completed(), "job should complete");

    let path = result.save(scratch_dir("audio"), None).expect("save");
    assert_saved_non_empty(&path);
}

// 15. image-operations — resize a jpg. ---------------------------------------
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn image_operations() {
    let client = require_client!();

    let result = client
        .convert_with(
            JPG,
            "resize-image",
            ConvertOptions::new()
                .category("operation")
                .option("width", 800)
                .option("height", 600)
                .option("resize_by", "px")
                .option("resize_handling", "keep_aspect_ratio_crop"),
        )
        .expect("resize image");
    assert!(result.job().is_completed(), "job should complete");

    let path = result.save(scratch_dir("resize"), None).expect("save");
    assert_saved_non_empty(&path);
}

// 16. webhooks — start an async conversion with a callback. ------------------
//
// A webhook receipt is not testable in CI, so we assert only that the async
// start returns a job with an id — we do not wait for the callback.
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn webhooks() {
    let client = require_client!();

    let job = client
        .convert_async_with(
            DOCX,
            "pdf",
            AsyncOptions::new()
                .category("document")
                .callback("https://your-app.example.com/api2convert/webhook"),
        )
        .expect("start async conversion");
    assert!(!job.id.is_empty(), "async start should return a job id");
}

// 17. presets — list saved presets for a category/target. --------------------
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn presets() {
    let client = require_client!();

    let presets = client
        .presets()
        .list(Some("video"), Some("mp4"), None)
        .expect("list presets");
    // May be empty; the contract is that the call succeeds and returns a list.
    let _len = presets.len();
}

// 18. statistics — fetch monthly usage stats. --------------------------------
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn statistics() {
    let client = require_client!();

    let _stats = client
        .stats()
        .month("2026-06", None)
        .expect("fetch monthly stats");
}

// 19. rate-limits — inspect the account contract. ----------------------------
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn rate_limits() {
    let client = require_client!();

    let _contract = client.contracts().get().expect("fetch contract");
}

// 20. authentication — list jobs with a valid key. ---------------------------
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn authentication() {
    let client = require_client!();

    let jobs = client.jobs().list(None, None).expect("list jobs");
    let _len = jobs.len();
}

// Negative 1. Validation error on an unknown target. -------------------------
//
// The API rejects an unknown target — either synchronously at create time
// (validation) or as a failed job. Both are typed errors you can match on.
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn invalid_target_is_a_typed_error() {
    let client = require_client!();

    match client.convert(JPG, "this-is-not-a-real-target") {
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

// Negative 2. Authentication error, with no secret leak. ---------------------
//
// A bad key produces a typed `Authentication` error carrying the HTTP status.
// The SDK never puts a credential into an error message — assert the bogus key
// does not appear in the rendered error.
#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn authentication_error_leaks_no_secret() {
    // Keep it in the live suite so it exercises the real auth path.
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
