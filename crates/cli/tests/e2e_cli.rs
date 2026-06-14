//! End-to-end CLI journey test.
//!
//! Drives the real `nolgia` binary through subprocess execution against a
//! locally-mounted `wiremock` server that stands in for the production API.
//! No real Nolgia API, fal, or filesystem outside `tempfile` is touched.
//!
//! Run with: `cargo test --features e2e --test e2e_cli`. Without the
//! `e2e` feature these tests are skipped so they don't slow the default
//! unit-test loop.

#![cfg_attr(not(feature = "e2e"), allow(dead_code, unused_imports))]

use assert_cmd::Command;
use predicates::prelude::*;
use serde_json::{json, Value};
use std::fs;
use tempfile::TempDir;
use uuid::Uuid;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const USER_ID: &str = "550e8400-e29b-41d4-a716-446655440000";
const TEST_EMAIL: &str = "e2e@nolgia.ai";
const TEST_TOKEN: &str = "e2e-token-do-not-use-in-prod";

#[cfg_attr(not(feature = "e2e"), ignore)]
#[tokio::test]
async fn end_to_end_journey_auth_status_generate_assets() {
    let api = MockServer::start().await;
    let files = MockServer::start().await;
    let temp = TempDir::new().expect("tempdir");

    mount_files_endpoint(&files).await;
    mount_account_endpoint(&api).await;
    mount_generate_image_endpoint(&api, &files.uri()).await;
    mount_assets_list_endpoint(&api, &files.uri()).await;

    let out_path = temp.path().join("journey.png");

    nolgia_cmd(&api, &temp)
        .args(["account", "me"])
        .assert()
        .success()
        .stdout(predicate::str::contains(TEST_EMAIL));

    nolgia_cmd(&api, &temp)
        .args([
            "gen",
            "image",
            "--prompt",
            "an end to end robot",
            "--out",
            out_path.to_str().expect("utf8 path"),
        ])
        .assert()
        .success();

    let bytes = fs::read(&out_path).expect("downloaded asset bytes");
    assert!(!bytes.is_empty(), "downloaded asset should not be empty");

    let assets_json = nolgia_cmd(&api, &temp)
        .args(["--json", "assets", "list"])
        .assert()
        .success()
        .get_output()
        .stdout
        .clone();
    let parsed: Value = serde_json::from_slice(&assets_json).expect("assets list is JSON");
    let items = parsed
        .get("items")
        .and_then(Value::as_array)
        .expect("items array");
    assert!(!items.is_empty(), "expected at least one asset");
}

#[cfg_attr(not(feature = "e2e"), ignore)]
#[test]
fn cli_help_advertises_every_subcommand() {
    let temp = TempDir::new().expect("tempdir");
    base_cmd(&temp)
        .arg("--help")
        .assert()
        .success()
        .stdout(predicate::str::contains("auth"))
        .stdout(predicate::str::contains("gen"))
        .stdout(predicate::str::contains("assets"))
        .stdout(predicate::str::contains("account"))
        .stdout(predicate::str::contains("billing"));
}

fn nolgia_cmd(api: &MockServer, temp: &TempDir) -> Command {
    let mut cmd = base_cmd(temp);
    cmd.arg("--api-url").arg(api.uri());
    cmd.env("NOLGIA_TOKEN", TEST_TOKEN);
    cmd
}

fn base_cmd(temp: &TempDir) -> Command {
    let mut cmd = Command::cargo_bin("nolgia").expect("nolgia binary built");
    cmd.env_remove("NOLGIA_TOKEN");
    cmd.env("HOME", temp.path());
    cmd.env("XDG_CONFIG_HOME", temp.path());
    cmd.env("XDG_DATA_HOME", temp.path());
    cmd
}

async fn mount_files_endpoint(files: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/image.png"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(vec![0x89, 0x50, 0x4e, 0x47]))
        .mount(files)
        .await;
}

async fn mount_account_endpoint(api: &MockServer) {
    Mock::given(method("GET"))
        .and(path("/v1/me"))
        .respond_with(ResponseTemplate::new(200).set_body_json(user_json()))
        .mount(api)
        .await;
}

async fn mount_generate_image_endpoint(api: &MockServer, files_base: &str) {
    let body = json!({
        "request_id": "req_e2e_journey",
        "asset": asset_json(&format!("{files_base}/image.png")),
        "seed": null,
    });
    Mock::given(method("POST"))
        .and(path("/v1/generate/image"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(api)
        .await;
}

async fn mount_assets_list_endpoint(api: &MockServer, files_base: &str) {
    let body = json!({
        "items": [asset_json(&format!("{files_base}/image.png"))],
    });
    Mock::given(method("GET"))
        .and(path("/v1/assets"))
        .respond_with(ResponseTemplate::new(200).set_body_json(body))
        .mount(api)
        .await;
}

fn asset_json(url: &str) -> Value {
    json!({
        "id": Uuid::new_v4(),
        "user_id": USER_ID,
        "modality": "image",
        "model": "fal-ai/flux-pro/v1.1",
        "signed_url": url,
        "expires_at": "2999-01-01T00:00:00Z",
        "created_at": "2026-06-13T00:00:00Z",
    })
}

fn user_json() -> Value {
    json!({
        "id": USER_ID,
        "email": TEST_EMAIL,
        "name": "E2E Tester",
        "image_url": null,
        "created_at": "2026-06-13T00:00:00Z",
    })
}
