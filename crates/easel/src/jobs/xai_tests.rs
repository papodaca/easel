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

fn data_item(bytes: &[u8], mime: Option<&str>) -> String {
    let encoded = b64(bytes);
    match mime {
        Some(mime) => format!(r#"{{"b64_json":"{encoded}","mime_type":"{mime}"}}"#),
        None => format!(r#"{{"b64_json":"{encoded}"}}"#),
    }
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
        Arc::new(MapKeys::default().insert(ProviderId::Xai, "xai-test-key"));
    let root = std::env::temp_dir().join(format!(
        "easel-xai-{}-{}",
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
        model_id: "grok-imagine-image-2.0".into(),
        compiled_prompt: "a harbor".into(),
        params: [
            ("aspect_ratio".into(), serde_json::json!("auto")),
            ("resolution".into(), serde_json::json!("1k")),
            ("quality".into(), serde_json::json!("auto")),
        ]
        .into_iter()
        .collect(),
        images: Vec::new(),
        box_snapshot: Vec::new(),
        created_at: "2026-10-18T00:00:00Z".into(),
        conversation: None,
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
    assert_eq!(seen[0].url, "https://api.x.ai/v1/images/generations");
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

#[gpui::test]
fn jpeg_bytes_land_without_a_poll(cx: &mut TestAppContext) {
    let bytes = png();
    let body = format!(
        r#"{{"request_id":"req-77","data":[{}]}}"#,
        data_item(&bytes, Some("image/jpeg"))
    );
    let harness = open(cx, vec![Ok(http(200, body))]);
    submit(cx, &harness);
    assert_one_post(&harness);
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(states[0], JobState::Done { .. }), "{states:?}");
    let listed = harness.history.list().unwrap();
    let id = listed[0].id.as_str();
    assert_eq!(
        std::fs::read(harness.root.join(id).join("result.jpg")).unwrap(),
        bytes
    );
    let meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(harness.root.join(id).join("meta.json")).unwrap())
            .unwrap();
    assert_eq!(meta["provider"], "xai");
    assert_eq!(meta["interaction_id"], "req-77");
    assert!(meta["poll_url"].is_null());
}

#[gpui::test]
fn bytes_without_a_mime_type_write_jpeg(cx: &mut TestAppContext) {
    let bytes = png();
    let body = format!(r#"{{"data":[{}]}}"#, data_item(&bytes, None));
    let harness = open(cx, vec![Ok(http(200, body))]);
    submit(cx, &harness);
    assert_one_post(&harness);
    let listed = harness.history.list().unwrap();
    let id = listed[0].id.as_str();
    assert_eq!(
        std::fs::read(harness.root.join(id).join("result.jpg")).unwrap(),
        bytes
    );
}

#[gpui::test]
fn png_mime_writes_png(cx: &mut TestAppContext) {
    let bytes = png();
    let body = format!(r#"{{"data":[{}]}}"#, data_item(&bytes, Some("image/png")));
    let harness = open(cx, vec![Ok(http(200, body))]);
    submit(cx, &harness);
    assert_one_post(&harness);
    let listed = harness.history.list().unwrap();
    let id = listed[0].id.as_str();
    assert_eq!(
        std::fs::read(harness.root.join(id).join("result.png")).unwrap(),
        bytes
    );
    assert!(!harness.root.join(id).join("result.jpg").exists());
}

#[gpui::test]
fn two_data_items_keep_the_first(cx: &mut TestAppContext) {
    let first = png();
    let second = b"second-picture";
    let body = format!(
        r#"{{"data":[{},{}]}}"#,
        data_item(&first, Some("image/jpeg")),
        data_item(second, Some("image/jpeg"))
    );
    let harness = open(cx, vec![Ok(http(200, body))]);
    submit(cx, &harness);
    assert_one_post(&harness);
    let listed = harness.history.list().unwrap();
    let id = listed[0].id.as_str();
    let stored = std::fs::read(harness.root.join(id).join("result.jpg")).unwrap();
    assert_eq!(stored, first);
    assert_ne!(stored, second);
}

#[gpui::test]
fn a_url_only_body_is_no_image_and_is_not_fetched(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![Ok(http(
            200,
            r#"{"data":[{"url":"https://example.com/out.jpg","mime_type":"image/jpeg"}]}"#,
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
    let error = history_error(&harness);
    assert!(matches!(error, ProviderError::NoImage));
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_no_image(&harness.root, &id);
}

#[gpui::test]
fn respect_moderation_false_blocks_bytes(cx: &mut TestAppContext) {
    let body = format!(
        r#"{{"respect_moderation":false,"data":[{}]}}"#,
        data_item(&png(), Some("image/jpeg"))
    );
    let harness = open(cx, vec![Ok(http(200, body))]);
    submit(cx, &harness);
    assert_one_post(&harness);
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(
        matches!(
            states[0],
            JobState::Failed {
                error: ProviderError::Blocked
            }
        ),
        "{states:?}"
    );
    let error = history_error(&harness);
    assert!(matches!(error, ProviderError::Blocked));
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_no_image(&harness.root, &id);
}

#[gpui::test]
fn empty_data_is_blocked(cx: &mut TestAppContext) {
    let harness = open(cx, vec![Ok(http(200, r#"{"data":[]}"#))]);
    submit(cx, &harness);
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(
        matches!(
            states[0],
            JobState::Failed {
                error: ProviderError::Blocked
            }
        ),
        "{states:?}"
    );
    assert!(matches!(history_error(&harness), ProviderError::Blocked));
}

#[gpui::test]
fn content_moderated_is_blocked(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![Ok(http(
            400,
            r#"{"error":{"code":"imagine:content-moderated","message":"no"}}"#,
        ))],
    );
    submit(cx, &harness);
    assert_eq!(harness.transport.seen().len(), 1);
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(
        matches!(
            states[0],
            JobState::Failed {
                error: ProviderError::Blocked
            }
        ),
        "{states:?}"
    );
    let error = history_error(&harness);
    assert!(matches!(error, ProviderError::Blocked));
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_no_image(&harness.root, &id);
    assert!(harness.root.join(&id).join("request.json").is_file());
    assert!(harness.root.join(&id).join("meta.json").is_file());
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
fn http_401_is_auth_and_writes_a_failed_row(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![Ok(http(401, r#"{"error":{"message":"bad key"}}"#))],
    );
    submit(cx, &harness);
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(
        matches!(
            states[0],
            JobState::Failed {
                error: ProviderError::Auth
            }
        ),
        "{states:?}"
    );
    let error = history_error(&harness);
    assert!(matches!(error, ProviderError::Auth));
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_no_image(&harness.root, &id);
}

#[gpui::test]
fn dismiss_during_the_post_drops_a_late_200(cx: &mut TestAppContext) {
    let body = format!(r#"{{"data":[{}]}}"#, data_item(&png(), Some("image/jpeg")));
    let harness = open(cx, vec![Ok(http(200, body))]);
    harness
        .runner
        .update(cx, |runner, cx| runner.submit(request(), cx));
    harness
        .runner
        .update(cx, |runner, cx| runner.dismiss(0, cx));
    cx.run_until_parked();
    assert!(harness.history.list().unwrap().is_empty());
}
