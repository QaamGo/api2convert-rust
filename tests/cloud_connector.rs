//! Cloud-connector parity fixtures (milestone D-5), mirroring the canonical
//! cross-SDK fixtures (`api2convert-cloud-connector-parity-fixtures.md`):
//!
//! - **Fixture 1** — create-payload: what `convert_cloud()` serializes.
//! - **Fixture 2** — read hydration, incl. unknown-provider round-trip.
//! - **Fixture 3** — credential redaction (3a object rendering, 3b/3c error
//!   text + error-body deep-walk, 3d sensitive `parameters` leaf).
//!
//! All three are offline: fixture 1 asserts what the SDK serializes; fixtures
//! 2–3 feed the SDK a canned response/error.

mod common;

use api2convert::cloud::provider;
use api2convert::{Api2Convert, Api2ConvertError, CloudInput, ConvertOptions, OutputTarget};
use common::*;
use serde_json::{json, Value};

const SECRET: &str = "SUPERSECRET123";
const MARKER: &str = "[REDACTED]";

/// The exact input descriptor fixture 1 expects the SDK to serialize.
fn expected_input() -> Value {
    json!({
        "type": "cloud",
        "source": "amazons3",
        "parameters": {"bucket": "my-bucket", "file": "in/photo.png"},
        "credentials": {"accesskeyid": "AKIA_TEST", "secretaccesskey": "SECRET_TEST"}
    })
}

/// The exact output_target descriptor fixture 1 expects — note: no `status` key.
fn expected_output_target() -> Value {
    json!({
        "type": "ftp",
        "parameters": {"host": "ftp.example.com", "file": "/out/photo.jpg"},
        "credentials": {"username": "u", "password": "p"}
    })
}

// ---- Fixture 1: create-payload (what convert_cloud() serializes) ---------------------------

#[test]
fn fixture1_convert_serializes_cloud_input_and_output_target() {
    let sender = FakeSender::new();
    // create → started job; wait() polls once to a completed job with no local output.
    sender.push_json(
        201,
        json!({"id": "job-1", "status": {"code": "incomplete"}}),
    );
    sender.push_ok(json!({"id": "job-1", "status": {"code": "completed"}}));
    let client = client(sender.clone());

    let input = CloudInput::amazon_s3("my-bucket", "in/photo.png", "AKIA_TEST", "SECRET_TEST");
    let target = OutputTarget::of(provider::FTP)
        .parameter("host", "ftp.example.com")
        .parameter("file", "/out/photo.jpg")
        .credential("username", "u")
        .credential("password", "p");

    let result = client
        .convert_cloud_with(input, "jpg", ConvertOptions::new().output_target(target))
        .expect("convert_cloud");

    // An output-target job yields 0 local outputs — that is not an error.
    assert!(result.outputs().is_empty());

    let body = sender.request_at(0).body_json();

    // 1) a cloud input is a started job (like a remote URL), not staged/uploaded.
    assert_eq!(body["process"], json!(true));

    // 2) input[0] carries the flat/lowercase keys exactly as the factory emits them.
    assert_eq!(body["input"], json!([expected_input()]));

    // 3) conversion[0].output_target[0] serializes {type,parameters,credentials} and NO status.
    assert_eq!(
        body["conversion"][0]["output_target"],
        json!([expected_output_target()])
    );
    assert!(body["conversion"][0]["output_target"][0]
        .get("status")
        .is_none());

    // Output targets never leak into the conversion options map.
    assert!(body["conversion"][0].get("options").is_none());
}

#[test]
fn fixture1_raw_create_path_produces_byte_identical_output_target() {
    let sender = FakeSender::new();
    sender.push_json(201, json!({"id": "job-1", "status": {"code": "completed"}}));
    let client = client(sender.clone());

    client
        .jobs()
        .create(
            json!({
                "process": true,
                "input": [CloudInput::amazon_s3("my-bucket", "in/photo.png", "AKIA_TEST", "SECRET_TEST").to_value()],
                "conversion": [{
                    "target": "jpg",
                    "output_target": [OutputTarget::new(
                        provider::FTP,
                        json!({"host": "ftp.example.com", "file": "/out/photo.jpg"}).as_object().unwrap().clone(),
                        json!({"username": "u", "password": "p"}).as_object().unwrap().clone(),
                    ).to_value()],
                }]
            }),
            None,
        )
        .expect("create");

    let body = sender.request_at(0).body_json();

    // Both the convert() outputTargets control and the raw create map yield the same bytes.
    assert_eq!(body["input"], json!([expected_input()]));
    assert_eq!(
        body["conversion"][0]["output_target"],
        json!([expected_output_target()])
    );
}

#[test]
fn add_input_accepts_cloud_input_builder() {
    let sender = FakeSender::new();
    sender.push_ok(json!({"id": "in-1", "type": "cloud", "source": "ftp"}));
    let client = client(sender.clone());

    client
        .jobs()
        .add_input(
            "job-1",
            CloudInput::ftp("ftp.example.com", "in/a.png", "u", "p"),
        )
        .expect("add_input");

    let body = sender.request_at(0).body_json();
    assert_eq!(body["type"], "cloud");
    assert_eq!(body["source"], "ftp");
    assert_eq!(
        body["parameters"],
        json!({"host": "ftp.example.com", "file": "in/a.png"})
    );
    assert_eq!(
        body["credentials"],
        json!({"username": "u", "password": "p"})
    );
}

// ---- Fixture 2: read hydration (a GET /jobs/{id} response) ---------------------------------

#[test]
fn fixture2_hydrates_cloud_input_and_output_target() {
    let sender = FakeSender::new();
    sender.push_ok(json!({
        "id": "job-1",
        "status": {"code": "completed"},
        "input": [{
            "id": "in-1",
            "type": "cloud",
            "source": "amazons3",
            "status": "ready",
            "parameters": {"bucket": "my-bucket", "file": "in/photo.png"},
            "credentials": {}
        }],
        "conversion": [{
            "id": "c-1",
            "target": "jpg",
            "output_target": [{
                "type": "ftp",
                "parameters": {"host": "ftp.example.com", "file": "/out/photo.jpg"},
                "credentials": {},
                "status": "uploading"
            }]
        }]
    }));
    let job = client(sender).jobs().get("job-1").expect("get");

    // 1) input source is a RAW string; parameters surface.
    let input = &job.input[0];
    assert_eq!(input.source.as_deref(), Some("amazons3"));
    assert_eq!(input.status.as_deref(), Some("ready"));
    assert_eq!(
        Value::Object(input.parameters.clone()),
        json!({"bucket": "my-bucket", "file": "in/photo.png"})
    );

    // 2) output target status/parameters/type surface.
    let out = &job.conversion[0].output_target[0];
    assert_eq!(out.kind, "ftp");
    assert_eq!(out.status.as_deref(), Some("uploading"));
    assert_eq!(
        Value::Object(out.parameters.clone()),
        json!({"host": "ftp.example.com", "file": "/out/photo.jpg"})
    );

    // 3) credentials are never surfaced (the API returns them empty; the SDK does not hydrate).
    assert!(out.credentials.is_empty());
}

#[test]
fn fixture2_unknown_provider_round_trips_untyped() {
    let sender = FakeSender::new();
    sender.push_ok(json!({
        "id": "job-1",
        "status": {"code": "completed"},
        "input": [{"id": "in-1", "type": "cloud", "source": "r2", "status": "ready"}],
        "conversion": [{
            "target": "jpg",
            "output_target": [{"type": "r2", "status": "waiting"}]
        }]
    }));
    let job = client(sender).jobs().get("job-1").expect("get");

    // An unknown provider string hydrates without any enum parse throwing.
    assert_eq!(job.input[0].source.as_deref(), Some("r2"));
    assert_eq!(job.conversion[0].output_target[0].kind, "r2");
    assert_eq!(
        job.conversion[0].output_target[0].status.as_deref(),
        Some("waiting")
    );
}

// ---- Fixture 3: credential redaction (the security test) ----------------------------------

// 3a — object rendering.
#[test]
fn fixture3a_cloud_input_debug_masks_credentials() {
    let rendered = format!("{:?}", CloudInput::amazon_s3("b", "f", "AKIA", SECRET));
    assert!(!rendered.contains(SECRET));
    assert!(rendered.contains(MARKER));
    // Non-secret parameters still render.
    assert!(rendered.contains("bucket"));
}

#[test]
fn fixture3a_output_target_debug_masks_credentials() {
    let rendered = format!(
        "{:?}",
        OutputTarget::of(provider::FTP)
            .parameter("host", "ftp.example.com")
            .credential("username", "u")
            .credential("password", SECRET)
    );
    assert!(!rendered.contains(SECRET));
    assert!(rendered.contains(MARKER));
}

// 3b + 3c — error text and error-body deep-walk.
#[test]
fn fixture3bc_create_path_error_never_leaks_submitted_credential() {
    let sender = FakeSender::new();
    // A 422 whose decoded body echoes the submitted secret in a nested/dotted key (belt-and-
    // suspenders: the real API echoes field *names* only). The convert request body itself
    // carried the secret in credentials — it must not surface on the error either.
    sender.push_json(
        422,
        json!({
            "message": "Validation failed",
            "errors": {"input.0.credentials.secretaccesskey": SECRET}
        }),
    );
    let client = client_no_retry(sender);

    let err = match client.convert_cloud(CloudInput::amazon_s3("b", "f", "AKIA", SECRET), "jpg") {
        Ok(_) => panic!("expected a validation error"),
        Err(e) => e,
    };

    assert!(matches!(err, Api2ConvertError::Validation(_)));
    // 3b: no secret in the Display message.
    assert!(!err.to_string().contains(SECRET));
    // 3c: the deep-walk masks the echoed secret to the marker in the attached body.
    let body = err.body().expect("error carries a body").to_string();
    assert!(!body.contains(SECRET));
    assert!(body.contains(MARKER));
}

// 3d — sensitive parameters leaf.
#[test]
fn fixture3d_sensitive_parameters_leaf_is_masked_in_rendering() {
    let rendered = format!(
        "{:?}",
        CloudInput::of(provider::AMAZONS3)
            .parameter("token", "PARAMSECRET")
            .parameter("bucket", "b")
    );
    assert!(!rendered.contains("PARAMSECRET"));
    assert!(rendered.contains(MARKER));
    // A non-secret key renders normally.
    assert!(rendered.contains("bucket"));
}

// ---- Unit: the provider vocabulary --------------------------------------------------------

#[test]
fn provider_vocabulary() {
    assert_eq!(
        provider::ALL,
        [
            "amazons3",
            "azure",
            "ftp",
            "gdrive",
            "googlecloud",
            "youtube"
        ]
    );
}

/// Type-checks that the async cloud entry points compile with output targets.
#[allow(dead_code)]
fn _api_surface_compiles(client: &Api2Convert) {
    let _ = client.convert_cloud(CloudInput::ftp("h", "f", "u", "p"), "jpg");
    let _ = client.convert_cloud_async(CloudInput::ftp("h", "f", "u", "p"), "jpg");
    let _ = client.convert_cloud_async_with(
        CloudInput::ftp("h", "f", "u", "p"),
        "jpg",
        api2convert::AsyncOptions::new().output_target(OutputTarget::of("gdrive")),
    );
}
