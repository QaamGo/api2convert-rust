//! Download to disk / memory, directory + path-traversal-safe naming, the
//! max-download cap, and download-password override.

mod common;

use std::sync::Arc;

use api2convert::{Api2Convert, OutputFile};
use common::*;

fn output(uri: &str, filename: Option<&str>) -> OutputFile {
    let mut o = OutputFile::with_uri(uri);
    o.filename = filename.map(str::to_string);
    o
}

#[test]
fn saves_into_a_directory_using_the_api_filename() {
    let sender = FakeSender::new();
    sender.push_raw(200, b"PAYLOAD", vec![]);
    let client = client(sender.clone());
    let dir = unique_tmp_dir("save-dir");

    let out = output("https://cdn.example/x", Some("result.png"));
    let path = client.download(&out, None).save(&dir, None).expect("save");

    assert_eq!(path, dir.join("result.png"));
    assert_eq!(std::fs::read(&path).unwrap(), b"PAYLOAD");
    // No password → the download follows storage redirects.
    assert!(sender.last_request().follow_redirects);
    assert!(sender
        .last_request()
        .header("x-oc-download-password")
        .is_none());
}

#[test]
fn directory_target_sanitizes_a_traversal_filename() {
    let sender = FakeSender::new();
    sender.push_raw(200, b"DATA", vec![]);
    let client = client(sender);
    let dir = unique_tmp_dir("save-traversal");

    let out = output("https://cdn.example/x", Some("../../etc/evil"));
    let path = client.download(&out, None).save(&dir, None).expect("save");

    assert_eq!(
        path,
        dir.join("evil"),
        "must not escape the target directory"
    );
    assert!(path.starts_with(&dir));
}

#[test]
fn contents_returns_the_bytes() {
    let sender = FakeSender::new();
    sender.push_raw(200, b"HELLO", vec![]);
    let client = client(sender);

    let out = output("https://cdn.example/x", None);
    let bytes = client
        .download(&out, None)
        .contents(None)
        .expect("contents");
    assert_eq!(bytes, b"HELLO");
}

#[test]
fn max_download_bytes_is_enforced() {
    let sender = FakeSender::new();
    sender.push_raw(200, b"way too many bytes", vec![]);
    let client = Api2Convert::builder()
        .api_key("test-key")
        .max_download_bytes(4)
        .http_sender(sender)
        .sleeper(Arc::new(RecordingSleeper::default()))
        .rng(Arc::new(ZeroRng))
        .build()
        .unwrap();

    let out = output("https://cdn.example/x", None);
    let err = client
        .download(&out, None)
        .contents(None)
        .expect_err("should exceed cap");
    assert!(matches!(err, api2convert::Api2ConvertError::Network(_)));
}

#[test]
fn explicit_password_overrides_the_remembered_one() {
    let sender = FakeSender::new();
    sender.push_raw(200, b"X", vec![]);
    let client = client(sender.clone());

    let out = output("https://cdn.example/x", None);
    let fd = client.download(&out, Some("remembered"));
    fd.contents(Some("explicit")).expect("contents");

    assert_eq!(
        sender.last_request().header("x-oc-download-password"),
        Some("explicit")
    );
}
