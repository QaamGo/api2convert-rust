//! Black-box security suite. Drives the public API through the **real** reqwest
//! transport against loopback servers to prove the redirect/leak guarantees:
//! secret-bearing requests never follow redirects (so a secret never reaches a
//! redirect target), while the self-contained, no-secret download path does.

mod common;

use api2convert::{Api2Convert, Api2ConvertError, Job, OutputFile, Status};
use common::*;
use serde_json::{json, Value};

fn dummy_job(server: &str) -> Job {
    Job {
        id: "job1".to_string(),
        status: Status {
            code: "created".to_string(),
            info: None,
        },
        token: Some("job-token".to_string()),
        server: Some(server.to_string()),
        callback: None,
        conversion: vec![],
        input: vec![],
        output: vec![],
        errors: vec![],
        warnings: vec![],
        raw: Value::Null,
    }
}

#[test]
fn account_key_is_not_forwarded_across_a_redirect() {
    let evil = TestServer::start(|_| Reply::ok(b"evil"));
    let evil_base = evil.base.clone();
    let api = TestServer::start(move |_| Reply::redirect(&format!("{evil_base}/steal")));

    let client = Api2Convert::builder()
        .api_key("super-secret-account-key")
        .base_url(&api.base)
        .build()
        .unwrap();

    let result = client.jobs().create(json!({"conversion": []}), None);
    assert!(
        matches!(result, Err(Api2ConvertError::Network(_))),
        "an authenticated request must not follow a redirect"
    );
    assert_eq!(
        evil.hits(),
        0,
        "the account key must never reach the redirect target"
    );
}

#[test]
fn upload_uses_the_token_and_does_not_follow_redirects() {
    let evil = TestServer::start(|_| Reply::ok(b"evil"));
    let evil_base = evil.base.clone();
    let upload = TestServer::start(move |_| Reply::redirect(&format!("{evil_base}/steal")));

    let client = Api2Convert::builder()
        .api_key("super-secret-account-key")
        .base_url("http://127.0.0.1:1")
        .build()
        .unwrap();

    let job = dummy_job(&upload.base);
    let res = client.jobs().upload(&job, b"data".to_vec(), Some("f.bin"));
    assert!(res.is_err(), "upload must not follow the redirect");
    assert_eq!(evil.hits(), 0);

    let reqs = upload.requests();
    assert_eq!(reqs.len(), 1);
    assert_eq!(reqs[0].header("x-oc-token"), Some("job-token"));
    assert!(
        reqs[0].header("x-oc-api-key").is_none(),
        "upload must not carry the account key"
    );
}

#[test]
fn download_password_is_not_forwarded_across_a_redirect() {
    let evil = TestServer::start(|_| Reply::ok(b"evil"));
    let evil_base = evil.base.clone();
    let storage = TestServer::start(move |_| Reply::redirect(&format!("{evil_base}/leak")));

    let client = Api2Convert::builder()
        .api_key("k")
        .base_url("http://127.0.0.1:1")
        .build()
        .unwrap();

    let out = OutputFile::with_uri(format!("{}/file", storage.base));
    let res = client.download(&out, Some("dl-secret")).contents(None);
    assert!(
        res.is_err(),
        "a password download must not follow a redirect"
    );
    assert_eq!(
        evil.hits(),
        0,
        "the download password must never reach the redirect target"
    );

    let reqs = storage.requests();
    assert_eq!(reqs.len(), 1);
    assert_eq!(
        reqs[0].header("x-oc-download-password"),
        Some("dl-secret"),
        "the intended host does receive the password"
    );
}

#[test]
fn passwordless_download_follows_a_storage_redirect() {
    let cdn = TestServer::start(|_| Reply::ok(b"PAYLOAD"));
    let cdn_base = cdn.base.clone();
    let storage = TestServer::start(move |_| Reply::redirect(&format!("{cdn_base}/real")));

    let client = Api2Convert::builder()
        .api_key("k")
        .base_url("http://127.0.0.1:1")
        .build()
        .unwrap();

    let out = OutputFile::with_uri(format!("{}/file", storage.base));
    let bytes = client
        .download(&out, None)
        .contents(None)
        .expect("a passwordless download should follow the redirect");
    assert_eq!(bytes, b"PAYLOAD");
    assert_eq!(cdn.hits(), 1);

    // The follow client must not attach a Referer: the download URL can carry a
    // signed token, and it must never reach the redirect target.
    let cdn_reqs = cdn.requests();
    assert_eq!(cdn_reqs.len(), 1);
    assert!(
        cdn_reqs[0].header("referer").is_none(),
        "the signed download URL must not leak to the redirect target via Referer"
    );
}
