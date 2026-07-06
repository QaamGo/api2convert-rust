//! Live conformance suite. Hits the real API and consumes quota, so every test
//! is `#[ignore]` and additionally skips unless `API2CONVERT_API_KEY` is set:
//!
//! ```sh
//! API2CONVERT_API_KEY=<key> cargo test --test live -- --ignored
//! ```
//!
//! `API2CONVERT_BASE_URL` overrides the host (e.g. a beta environment). Never
//! commit a real key — it is read only from the environment.

use api2convert::{Api2Convert, Api2ConvertError};

const REMOTE_JPG: &str =
    "https://example-files.online-convert.com/raster%20image/jpg/example_small.jpg";

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

#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn converts_remote_image_to_png() {
    let Some(client) = live_client() else {
        eprintln!("skipping: API2CONVERT_API_KEY not set");
        return;
    };

    let result = client.convert(REMOTE_JPG, "png").expect("convert");
    assert!(result.job().is_completed(), "job should complete");

    let dir = std::env::temp_dir().join(format!("a2c-live-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = result.save(&dir, None).expect("save");
    let meta = std::fs::metadata(&path).expect("stat output");
    assert!(meta.len() > 0, "output should be non-empty");
}

#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn invalid_target_is_a_typed_failure() {
    let Some(client) = live_client() else {
        eprintln!("skipping: API2CONVERT_API_KEY not set");
        return;
    };

    // The API rejects an unknown target either at create time (validation) or as
    // a failed job — accept either as a correct, typed failure.
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

#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn options_catalog_is_reachable() {
    let Some(client) = live_client() else {
        eprintln!("skipping: API2CONVERT_API_KEY not set");
        return;
    };
    // Just assert the call succeeds; the catalog contents vary.
    let _ = client.options("png", Some("image")).expect("options");
}

/// A minimal valid 1×1 PNG, uploaded to exercise the real multipart upload
/// handshake (remote-URL inputs skip upload entirely).
const ONE_PX_PNG: &[u8] = &[
    0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0x00, 0x00, 0x00, 0x0D, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x02, 0x00, 0x00, 0x00, 0x90, 0x77, 0x53,
    0xDE, 0x00, 0x00, 0x00, 0x0C, 0x49, 0x44, 0x41, 0x54, 0x08, 0xD7, 0x63, 0xF8, 0xCF, 0xC0, 0x00,
    0x00, 0x00, 0x03, 0x01, 0x01, 0x00, 0x18, 0xDD, 0x8D, 0xB0, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45,
    0x4E, 0x44, 0xAE, 0x42, 0x60, 0x82,
];

#[test]
#[ignore = "live: requires API2CONVERT_API_KEY and consumes quota"]
fn converts_local_bytes_via_multipart_upload() {
    let Some(client) = live_client() else {
        eprintln!("skipping: API2CONVERT_API_KEY not set");
        return;
    };

    // Bytes input → the SDK stages the job, uploads via multipart to the per-job
    // server (X-Oc-Token), starts it, polls, and downloads.
    let result = client
        .convert_with(
            ONE_PX_PNG.to_vec(),
            "jpg",
            api2convert::ConvertOptions::new().filename("pixel.png"),
        )
        .expect("convert via upload");
    assert!(result.job().is_completed(), "uploaded job should complete");

    let bytes = result.contents(None).expect("download");
    assert!(!bytes.is_empty(), "converted output should be non-empty");
    // A JPEG starts with the SOI marker 0xFF 0xD8.
    assert_eq!(&bytes[0..2], &[0xFF, 0xD8], "output should be a JPEG");
}
