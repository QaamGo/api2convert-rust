//! Coverage for the derived resource methods: conversions catalog, presets CRUD,
//! stats periods, and contracts.

mod common;

use common::*;
use serde_json::json;

#[test]
fn conversions_options_returns_the_first_rows_options() {
    let sender = FakeSender::new();
    sender.push_ok(json!([
        {"target": "png", "category": "image", "options": {"quality": {"type": "integer"}}}
    ]));
    let client = client(sender.clone());

    let opts = client.options("png", Some("image")).expect("options");
    assert!(opts.contains_key("quality"));

    let path = sender.last_request().path();
    assert!(path.starts_with("/conversions?"));
    assert!(path.contains("target=png"));
    assert!(path.contains("category=image"));
}

#[test]
fn conversions_list_returns_rows() {
    let sender = FakeSender::new();
    sender.push_ok(json!([
        {"target": "png", "category": "image"},
        {"target": "jpg", "category": "image"}
    ]));
    let client = client(sender);

    let rows = client
        .conversions()
        .list(Some("image"), None, None)
        .expect("list");
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0]["target"], "png");
}

#[test]
fn presets_list() {
    let sender = FakeSender::new();
    sender.push_ok(json!([{"id": "p1", "name": "My preset", "target": "png"}]));
    let client = client(sender);
    let presets = client.presets().list(None, None, None).expect("list");
    assert_eq!(presets.len(), 1);
    assert_eq!(presets[0].name, "My preset");
}

#[test]
fn presets_create() {
    let sender = FakeSender::new();
    sender.push_ok(json!({"id": "p2", "name": "New", "target": "jpg"}));
    let client = client(sender.clone());
    let created = client
        .presets()
        .create(json!({"name": "New", "target": "jpg"}))
        .expect("create");
    assert_eq!(created.id.as_deref(), Some("p2"));
    assert_eq!(sender.last_request().method, "POST");
    assert_eq!(sender.last_request().path(), "/presets");
}

#[test]
fn presets_get() {
    let sender = FakeSender::new();
    sender.push_ok(json!({"id": "p1", "name": "My preset"}));
    let client = client(sender.clone());
    client.presets().get("p1").expect("get");
    assert_eq!(sender.last_request().path(), "/presets/p1");
}

#[test]
fn presets_update() {
    let sender = FakeSender::new();
    sender.push_ok(json!({"id": "p1", "name": "Renamed"}));
    let client = client(sender.clone());
    let updated = client
        .presets()
        .update("p1", json!({"name": "Renamed"}))
        .expect("update");
    assert_eq!(updated.name, "Renamed");
    assert_eq!(sender.last_request().method, "PATCH");
    assert_eq!(sender.last_request().path(), "/presets/p1");
}

#[test]
fn presets_delete() {
    let sender = FakeSender::new();
    sender.push_raw(200, b"", vec![]);
    let client = client(sender.clone());
    client.presets().delete("p1").expect("delete");
    assert_eq!(sender.last_request().method, "DELETE");
    assert_eq!(sender.last_request().path(), "/presets/p1");
}

#[test]
fn stats_periods_build_the_right_paths() {
    let sender = FakeSender::new();
    sender.push_ok(json!({"conversions": 5}));
    sender.push_ok(json!({"conversions": 50}));
    sender.push_ok(json!({"conversions": 500}));
    sender.push_ok(json!({"conversions": 5}));
    let client = client(sender.clone());

    client.stats().day("2026-07-06", None).expect("day");
    assert_eq!(sender.request_at(0).path(), "/stats/day/2026-07-06/all");

    client
        .stats()
        .month("2026-07", Some("image"))
        .expect("month");
    assert_eq!(sender.request_at(1).path(), "/stats/month/2026-07/image");

    client.stats().year("2026", None).expect("year");
    assert_eq!(sender.request_at(2).path(), "/stats/year/2026/all");
}

#[test]
fn contracts_get() {
    let sender = FakeSender::new();
    sender.push_ok(json!({"plan": "business"}));
    let client = client(sender.clone());

    let c = client.contracts().get().expect("contracts");
    assert_eq!(c["plan"], "business");
    assert_eq!(sender.last_request().path(), "/contracts");
}
