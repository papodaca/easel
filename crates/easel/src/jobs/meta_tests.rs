use std::sync::Arc;

use easel_core::error::ProviderError;
use easel_core::history::HistoryStore;
use easel_core::http::{HttpResponse, Method};
use easel_core::job::{EncodedImage, JobRequest, JobState};
use easel_core::model_spec::ProviderId;
use gpui::{TestAppContext, prelude::*};

use crate::jobs::JobRunner;
use crate::net::{FakeTransport, HttpTransport, KeyResolver, MapKeys};

fn http(status: u16, body: impl AsRef<[u8]>) -> HttpResponse {
    HttpResponse {
        status,
        headers: Vec::new(),
        body: body.as_ref().to_vec(),
    }
}

fn png() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/pixel.png"
    ))
    .unwrap()
}

fn jpeg() -> Vec<u8> {
    vec![0xFF, 0xD8, 0xFF, 0xD9]
}

fn webp() -> Vec<u8> {
    let mut bytes = b"RIFF".to_vec();
    bytes.extend_from_slice(&4u32.to_le_bytes());
    bytes.extend_from_slice(b"WEBP");
    bytes
}

fn b64(bytes: &[u8]) -> String {
    EncodedImage {
        media_type: "image/png".into(),
        bytes: bytes.to_vec(),
        width: 1,
        height: 1,
        content_hash: String::new(),
    }
    .base64()
}

fn body(output_format: Option<&str>, items: &[&str]) -> String {
    let format = output_format
        .map(|format| format!(r#""output_format":"{format}","#))
        .unwrap_or_default();
    format!(r#"{{{format}"data":[{}]}}"#, items.join(","))
}

fn item(bytes: &[u8]) -> String {
    format!(r#"{{"b64_json":"{}"}}"#, b64(bytes))
}

struct Harness {
    runner: gpui::Entity<JobRunner>,
    transport: Arc<FakeTransport>,
    history: Arc<HistoryStore>,
    root: std::path::PathBuf,
}

fn open(
    cx: &mut TestAppContext,
    script: Vec<Result<HttpResponse, crate::net::TransportError>>,
) -> Harness {
    let transport = Arc::new(FakeTransport::scripted(script));
    let keys: Arc<dyn KeyResolver> =
        Arc::new(MapKeys::default().insert(ProviderId::Meta, "meta-test-key"));
    let root = std::env::temp_dir().join(format!(
        "easel-meta-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let history = Arc::new(HistoryStore::open(&root).unwrap());
    let runner = cx.update({
        let transport = transport.clone() as Arc<dyn HttpTransport>;
        let history = history.clone();
        move |cx| {
            cx.new(|_| {
                JobRunner::new(
                    easel_core::providers::resolve_model,
                    transport,
                    keys,
                    history,
                )
            })
        }
    });
    Harness {
        runner,
        transport,
        history,
        root,
    }
}

fn request() -> JobRequest {
    JobRequest {
        model_id: "muse-image-1.0".into(),
        compiled_prompt: "a harbor".into(),
        params: [
            ("size".into(), serde_json::json!("auto")),
            ("output_format".into(), serde_json::json!("webp")),
            ("reasoning_strength".into(), serde_json::json!("high")),
            ("enable_image_search".into(), serde_json::json!(true)),
            ("enable_web_search".into(), serde_json::json!(true)),
            ("enable_shell".into(), serde_json::json!(true)),
        ]
        .into_iter()
        .collect(),
        images: Vec::new(),
        box_snapshot: Vec::new(),
        created_at: "2026-10-27T00:00:00Z".into(),
    }
}

fn submit(cx: &mut TestAppContext, harness: &Harness) {
    harness
        .runner
        .update(cx, |runner, cx| runner.submit(request(), cx));
    cx.run_until_parked();
}

fn assert_one_post(harness: &Harness) {
    let seen = harness.transport.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].method, Method::Post);
    assert!(
        seen.iter().all(|request| request.method != Method::Get),
        "{seen:?}"
    );
}

fn history_error(harness: &Harness) -> ProviderError {
    let listed = harness.history.list().unwrap();
    let JobState::Failed { error } = &listed[0].state else {
        panic!("{:?}", listed);
    };
    error.clone()
}

fn assert_no_image(root: &std::path::Path, id: &str) {
    let dir = root.join(id);
    for name in ["result.jpg", "result.png", "result.webp"] {
        assert!(!dir.join(name).exists(), "{name}");
    }
}

fn stored(harness: &Harness, name: &str) -> Vec<u8> {
    let listed = harness.history.list().unwrap();
    let id = listed[0].id.as_str();
    let bytes = std::fs::read(harness.root.join(id).join(name)).unwrap();
    let meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(harness.root.join(id).join("meta.json")).unwrap())
            .unwrap();
    assert_eq!(meta["provider"], "meta");
    assert_eq!(meta["interaction_id"], "");
    assert!(meta["poll_url"].is_null());
    bytes
}

#[gpui::test]
fn webp_bytes_land_without_a_poll(cx: &mut TestAppContext) {
    let bytes = webp();
    let harness = open(
        cx,
        vec![Ok(http(200, body(Some("webp"), &[&item(&bytes)])))],
    );
    submit(cx, &harness);
    assert_one_post(&harness);
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(states[0], JobState::Done { .. }), "{states:?}");
    assert_eq!(stored(&harness, "result.webp"), bytes);
}

#[gpui::test]
fn png_bytes_write_png(cx: &mut TestAppContext) {
    let bytes = png();
    let harness = open(cx, vec![Ok(http(200, body(Some("png"), &[&item(&bytes)])))]);
    submit(cx, &harness);
    assert_eq!(stored(&harness, "result.png"), bytes);
}

#[gpui::test]
fn jpeg_bytes_win_over_a_missing_output_format(cx: &mut TestAppContext) {
    let bytes = jpeg();
    let harness = open(cx, vec![Ok(http(200, body(None, &[&item(&bytes)])))]);
    submit(cx, &harness);
    assert_eq!(stored(&harness, "result.jpg"), bytes);
}

#[gpui::test]
fn unrecognized_bytes_fall_back_to_output_format(cx: &mut TestAppContext) {
    let bytes = b"not-an-image".to_vec();
    let harness = open(
        cx,
        vec![Ok(http(200, body(Some("jpeg"), &[&item(&bytes)])))],
    );
    submit(cx, &harness);
    assert_eq!(stored(&harness, "result.jpg"), bytes);
}

#[gpui::test]
fn two_data_items_keep_the_first(cx: &mut TestAppContext) {
    let first = png();
    let second = b"second-picture";
    let harness = open(
        cx,
        vec![Ok(http(
            200,
            body(Some("png"), &[&item(&first), &item(second)]),
        ))],
    );
    submit(cx, &harness);
    assert_one_post(&harness);
    assert_eq!(stored(&harness, "result.png"), first);
}

#[gpui::test]
fn a_url_only_body_is_no_image_and_is_not_fetched(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![Ok(http(
            200,
            r#"{"data":[{"url":"https://example.com/out.webp"}]}"#,
        ))],
    );
    submit(cx, &harness);
    assert_one_post(&harness);
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(
        matches!(
            states[0],
            JobState::Failed {
                error: ProviderError::NoImage
            }
        ),
        "{states:?}"
    );
    assert!(matches!(history_error(&harness), ProviderError::NoImage));
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_no_image(&harness.root, &id);
}

#[gpui::test]
fn empty_b64_json_is_no_image(cx: &mut TestAppContext) {
    for body in [
        r#"{"data":[{"b64_json":""}]}"#,
        r#"{"data":[{"b64_json":null}]}"#,
    ] {
        let harness = open(cx, vec![Ok(http(200, body))]);
        submit(cx, &harness);
        assert_one_post(&harness);
        assert!(
            matches!(history_error(&harness), ProviderError::NoImage),
            "{body}"
        );
        let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
        assert_no_image(&harness.root, &id);
    }
}

#[gpui::test]
fn empty_data_is_blocked(cx: &mut TestAppContext) {
    let harness = open(cx, vec![Ok(http(200, r#"{"data":[]}"#))]);
    submit(cx, &harness);
    assert!(matches!(history_error(&harness), ProviderError::Blocked));
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_no_image(&harness.root, &id);
}

#[gpui::test]
fn content_policy_violation_is_blocked(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![Ok(http(
            400,
            r#"{"error":{"code":"content_policy_violation","message":"no"}}"#,
        ))],
    );
    submit(cx, &harness);
    assert_eq!(harness.transport.seen().len(), 1);
    assert!(matches!(history_error(&harness), ProviderError::Blocked));
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_no_image(&harness.root, &id);
}

#[gpui::test]
fn another_400_is_http_with_no_row(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![Ok(http(400, r#"{"error":{"code":"invalid_request"}}"#))],
    );
    submit(cx, &harness);
    assert!(harness.history.list().unwrap().is_empty());
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(
        matches!(
            states[0],
            JobState::Failed {
                error: ProviderError::Http { status: 400, .. }
            }
        ),
        "{states:?}"
    );
}

#[gpui::test]
fn http_429_is_rate_limited_with_no_row(cx: &mut TestAppContext) {
    let harness = open(cx, vec![Ok(http(429, r#"{"error":{"message":"slow"}}"#))]);
    submit(cx, &harness);
    assert_eq!(harness.transport.seen().len(), 1);
    assert!(harness.history.list().unwrap().is_empty());
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(
        matches!(
            states[0],
            JobState::Failed {
                error: ProviderError::RateLimited { retry_after: None }
            }
        ),
        "{states:?}"
    );
}

#[gpui::test]
fn http_503_is_http_with_no_row_and_one_post(cx: &mut TestAppContext) {
    let harness = open(cx, vec![Ok(http(503, "unavailable"))]);
    submit(cx, &harness);
    assert_eq!(harness.transport.seen().len(), 1);
    assert!(harness.history.list().unwrap().is_empty());
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(
        matches!(
            states[0],
            JobState::Failed {
                error: ProviderError::Http { status: 503, .. }
            }
        ),
        "{states:?}"
    );
}

#[gpui::test]
fn http_401_is_auth_and_writes_a_failed_row(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![Ok(http(401, r#"{"error":{"message":"bad key"}}"#))],
    );
    submit(cx, &harness);
    assert!(matches!(history_error(&harness), ProviderError::Auth));
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_no_image(&harness.root, &id);
}

#[gpui::test]
fn dismiss_during_the_post_drops_a_late_200(cx: &mut TestAppContext) {
    let bytes = webp();
    let harness = open(
        cx,
        vec![Ok(http(200, body(Some("webp"), &[&item(&bytes)])))],
    );
    harness
        .runner
        .update(cx, |runner, cx| runner.submit(request(), cx));
    harness
        .runner
        .update(cx, |runner, cx| runner.dismiss(0, cx));
    cx.run_until_parked();
    assert!(harness.history.list().unwrap().is_empty());
}
