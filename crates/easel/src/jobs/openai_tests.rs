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

fn image_body(first: &[u8], second: Option<&[u8]>) -> String {
    let first = b64(first);
    match second {
        Some(second) => {
            let second = b64(second);
            format!(r#"{{"data":[{{"b64_json":"{first}"}},{{"b64_json":"{second}"}}]}}"#)
        }
        None => format!(r#"{{"data":[{{"b64_json":"{first}"}}]}}"#),
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
        Arc::new(MapKeys::default().insert(ProviderId::OpenAi, "openai-test-key"));
    let root = std::env::temp_dir().join(format!(
        "easel-openai-{}-{}",
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

fn request(model: &str) -> JobRequest {
    JobRequest {
        model_id: model.into(),
        compiled_prompt: "a harbor".into(),
        params: [
            ("quality".into(), serde_json::json!("auto")),
            ("size".into(), serde_json::json!("1024x1024")),
            ("background".into(), serde_json::json!("auto")),
        ]
        .into_iter()
        .collect(),
        images: Vec::new(),
        box_snapshot: Vec::new(),
        created_at: "2026-10-17T00:00:00Z".into(),
        conversation: None,
    }
}

fn submit(cx: &mut TestAppContext, harness: &Harness, model: &str) {
    harness
        .runner
        .update(cx, |runner, cx| runner.submit(request(model), cx));
    cx.run_until_parked();
}

fn strip_lines(cx: &mut TestAppContext, harness: &Harness) -> Vec<String> {
    let app = cx.update(|cx| cx.new(|_| crate::app_state::AppState::new()));
    let strip = cx.update({
        let runner = harness.runner.clone();
        let history = harness.history.clone();
        move |cx| cx.new(|cx| crate::bottom_strip::BottomStrip::new(app, Some(runner), history, cx))
    });
    strip.update(cx, |strip, cx| strip.refresh(cx));
    strip.read_with(cx, |strip, _| strip.lines())
}

#[gpui::test]
fn a_generations_png_lands_without_a_poll(cx: &mut TestAppContext) {
    let bytes = png();
    let harness = open(cx, vec![Ok(http(200, image_body(&bytes, Some(b"later"))))]);
    submit(cx, &harness, "gpt-image-2.5-flare");
    let seen = harness.transport.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].method, Method::Post);
    assert_eq!(seen[0].url, "https://api.openai.com/v1/images/generations");
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(states[0], JobState::Done { .. }), "{states:?}");
    let listed = harness.history.list().unwrap();
    let id = listed[0].id.as_str();
    assert_eq!(
        std::fs::read(harness.root.join(id).join("result.png")).unwrap(),
        bytes
    );
    let meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(harness.root.join(id).join("meta.json")).unwrap())
            .unwrap();
    assert_eq!(meta["provider"], "openai");
    assert_eq!(meta["interaction_id"], "");
    assert!(meta["poll_url"].is_null());
}

#[gpui::test]
fn a_url_only_body_fails_without_a_get(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![Ok(http(
            200,
            r#"{"data":[{"url":"https://example.com/out.png"}]}"#,
        ))],
    );
    submit(cx, &harness, "gpt-image-2");
    let seen = harness.transport.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].method, Method::Post);
    assert!(!seen[0].url.contains("example.com"));
    let listed = harness.history.list().unwrap();
    let JobState::Failed { error } = &listed[0].state else {
        panic!("{:?}", listed);
    };
    assert_eq!(error.to_string(), "no usable image");
    assert!(matches!(error, ProviderError::NoImage));
    let id = listed[0].id.as_str();
    assert!(!harness.root.join(id).join("result.png").exists());
    let lines = strip_lines(cx, &harness);
    assert!(
        lines.iter().any(|text| text == "no usable image"),
        "{lines:?}"
    );
}

#[gpui::test]
fn moderation_blocked_writes_a_failed_row_and_no_file(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![Ok(http(
            400,
            r#"{"error":{"code":"moderation_blocked","message":"no"}}"#,
        ))],
    );
    submit(cx, &harness, "gpt-image-2.5-flare");
    assert_eq!(harness.transport.seen().len(), 1);
    let listed = harness.history.list().unwrap();
    let JobState::Failed { error } = &listed[0].state else {
        panic!("{:?}", listed);
    };
    assert!(matches!(error, ProviderError::Blocked));
    assert_eq!(error.to_string(), "blocked by safety");
    let id = listed[0].id.as_str();
    assert!(!harness.root.join(id).join("result.png").exists());
    assert!(harness.root.join(id).join("request.json").is_file());
    assert!(harness.root.join(id).join("meta.json").is_file());
    let lines = strip_lines(cx, &harness);
    assert!(
        lines.iter().any(|text| text == "blocked by safety"),
        "{lines:?}"
    );
}

#[gpui::test]
fn http_401_is_an_auth_failure(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![Ok(http(401, r#"{"error":{"message":"bad key"}}"#))],
    );
    submit(cx, &harness, "gpt-image-2");
    let listed = harness.history.list().unwrap();
    let JobState::Failed { error } = &listed[0].state else {
        panic!("{:?}", listed);
    };
    assert!(matches!(error, ProviderError::Auth));
    let lines = strip_lines(cx, &harness);
    assert!(
        lines.iter().any(|text| text == "authentication failed"),
        "{lines:?}"
    );
}

#[gpui::test]
fn empty_b64_json_is_no_usable_image(cx: &mut TestAppContext) {
    let harness = open(cx, vec![Ok(http(200, r#"{"data":[{"b64_json":""}]}"#))]);
    submit(cx, &harness, "gpt-image-2");
    let listed = harness.history.list().unwrap();
    let JobState::Failed { error } = &listed[0].state else {
        panic!("{:?}", listed);
    };
    assert_eq!(error.to_string(), "no usable image");
    let id = listed[0].id.as_str();
    assert!(!harness.root.join(id).join("result.png").exists());
}

#[gpui::test]
fn a_rate_limit_stays_http_and_writes_no_row(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![Ok(http(
            429,
            r#"{"error":{"code":"rate_limit_exceeded","message":"slow"}}"#,
        ))],
    );
    submit(cx, &harness, "gpt-image-1");
    assert!(harness.history.list().unwrap().is_empty());
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    let JobState::Failed { error } = &states[0] else {
        panic!("{states:?}");
    };
    assert!(matches!(error, ProviderError::Http { status: 429, .. }));
}

#[gpui::test]
fn dismiss_during_the_post_writes_nothing(cx: &mut TestAppContext) {
    let harness = open(cx, vec![Ok(http(200, image_body(&png(), None)))]);
    harness
        .runner
        .update(cx, |runner, cx| runner.submit(request("gpt-image-2"), cx));
    harness
        .runner
        .update(cx, |runner, cx| runner.dismiss(0, cx));
    cx.run_until_parked();
    assert!(harness.history.list().unwrap().is_empty());
}
