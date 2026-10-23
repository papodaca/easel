use std::sync::Arc;

use easel_core::error::ProviderError;
use easel_core::history::HistoryStore;
use easel_core::http::{Body, HttpHeader, HttpResponse, Method};
use easel_core::job::{EncodedImage, JobRequest, JobState};
use easel_core::model_spec::ProviderId;
use gpui::{TestAppContext, prelude::*};
use serde_json::Value;

use crate::jobs::JobRunner;
use crate::net::{FakeTransport, HttpTransport, KeyResolver, MapKeys};

fn http(status: u16, content_type: Option<&str>, body: impl AsRef<[u8]>) -> HttpResponse {
    let headers = content_type
        .map(|value| {
            vec![HttpHeader {
                name: "Content-Type".into(),
                value: value.into(),
                sensitive: false,
            }]
        })
        .unwrap_or_default();
    HttpResponse {
        status,
        headers,
        body: body.as_ref().to_vec(),
    }
}

fn jpeg() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/photo.jpg"
    ))
    .unwrap()
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
        width: 64,
        height: 64,
        content_hash: String::new(),
    }
    .base64()
}

fn encoded(bytes: &[u8]) -> EncodedImage {
    EncodedImage {
        media_type: "image/png".into(),
        bytes: bytes.to_vec(),
        width: 64,
        height: 64,
        content_hash: format!("h-{}", bytes.len()),
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
        Arc::new(MapKeys::default().insert(ProviderId::Chutes, "chutes-test-key"));
    let root = std::env::temp_dir().join(format!(
        "easel-chutes-{}-{}",
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

fn request(model: &str, params: Value, images: Vec<EncodedImage>) -> JobRequest {
    JobRequest {
        model_id: model.into(),
        compiled_prompt: "a harbor".into(),
        params: params.as_object().unwrap().clone().into_iter().collect(),
        images,
        box_snapshot: Vec::new(),
        created_at: "2026-10-22T00:00:00Z".into(),
    }
}

fn flux_request() -> JobRequest {
    request(
        "chutes-flux-schnell",
        serde_json::json!({
            "width": 1024,
            "height": 1024,
            "guidance_scale": 3.5,
            "num_inference_steps": 4,
        }),
        Vec::new(),
    )
}

fn submit(cx: &mut TestAppContext, harness: &Harness, job: JobRequest) {
    harness
        .runner
        .update(cx, |runner, cx| runner.submit(job, cx));
    cx.run_until_parked();
}

fn posted_body(harness: &Harness) -> Value {
    let seen = harness.transport.seen();
    assert_eq!(seen.len(), 1, "{:?}", seen.len());
    let Body::Json(json) = &seen[0].body else {
        panic!("expected json: {:?}", seen[0].body);
    };
    json.clone()
}

fn assert_one_post(harness: &Harness, url: &str) {
    let seen = harness.transport.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].method, Method::Post);
    assert_eq!(seen[0].url, url);
    assert_eq!(seen[0].auth, Some(ProviderId::Chutes));
}

fn history_error(harness: &Harness) -> ProviderError {
    let listed = harness.history.list().unwrap();
    let JobState::Failed { error } = &listed[0].state else {
        panic!("{:?}", listed);
    };
    error.clone()
}

fn meta_of(harness: &Harness, id: &str) -> Value {
    serde_json::from_slice(&std::fs::read(harness.root.join(id).join("meta.json")).unwrap())
        .unwrap()
}

fn assert_no_image(harness: &Harness, id: &str) {
    let dir = harness.root.join(id);
    for name in ["result.jpg", "result.png", "result.webp"] {
        assert!(!dir.join(name).exists(), "{name}");
    }
}

#[gpui::test]
fn a_200_jpeg_from_flux_schnell_writes_result_jpg_after_one_post(cx: &mut TestAppContext) {
    let bytes = jpeg();
    let harness = open(cx, vec![Ok(http(200, Some("image/jpeg"), &bytes))]);
    submit(cx, &harness, flux_request());
    assert_one_post(
        &harness,
        "https://vonkaiser-imageclassic.chutes.ai/image/flux",
    );
    let body = posted_body(&harness);
    assert_eq!(body["prompt"], "a harbor");
    assert_eq!(body["width"], 1024);
    assert_eq!(body["guidance_scale"], serde_json::json!(3.5));
    assert!(body.get("model").is_none());
    assert!(body.get("image_b64s").is_none());
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(states[0], JobState::Done { .. }), "{states:?}");
    let listed = harness.history.list().unwrap();
    let id = listed[0].id.as_str();
    assert_eq!(
        std::fs::read(harness.root.join(id).join("result.jpg")).unwrap(),
        bytes
    );
    assert!(harness.root.join(id).join("thumb.jpg").is_file());
    assert!(harness.root.join(id).join("request.json").is_file());
    let meta = meta_of(&harness, id);
    assert_eq!(meta["provider"], "chutes");
    assert_eq!(meta["interaction_id"], "");
    assert!(meta["poll_url"].is_null());
}

#[gpui::test]
fn a_200_png_from_z_image_turbo_writes_result_png(cx: &mut TestAppContext) {
    let bytes = png();
    let harness = open(cx, vec![Ok(http(200, Some("image/png"), &bytes))]);
    submit(
        cx,
        &harness,
        request(
            "chutes-z-image-turbo",
            serde_json::json!({
                "width": 1024,
                "height": 1024,
                "shift": 3.0,
                "guidance_scale": 0.0,
                "max_sequence_length": 512,
                "num_inference_steps": 9,
            }),
            Vec::new(),
        ),
    );
    assert_one_post(
        &harness,
        "https://vonkaiser-z-image-turbo.chutes.ai/generate",
    );
    let listed = harness.history.list().unwrap();
    let id = listed[0].id.as_str();
    assert_eq!(
        std::fs::read(harness.root.join(id).join("result.png")).unwrap(),
        bytes
    );
    assert!(!harness.root.join(id).join("result.jpg").exists());
}

#[gpui::test]
fn qwen_edit_posts_both_references_canvas_first(cx: &mut TestAppContext) {
    let canvas = b"canvas-picture-bytes";
    let second = b"second-picture-bytes";
    let harness = open(cx, vec![Ok(http(200, Some("image/jpeg"), jpeg()))]);
    submit(
        cx,
        &harness,
        request(
            "chutes-qwen-image-edit-2511",
            serde_json::json!({
                "width": 1024,
                "height": 1024,
                "true_cfg_scale": 1.0,
                "num_inference_steps": 4,
            }),
            vec![encoded(canvas), encoded(second)],
        ),
    );
    assert_one_post(
        &harness,
        "https://vonkaiser-qwen-image-edit-2511.chutes.ai/generate",
    );
    let body = posted_body(&harness);
    let refs = body["image_b64s"].as_array().unwrap();
    assert_eq!(refs.len(), 2);
    assert_eq!(refs[0].as_str().unwrap(), b64(canvas));
    assert_eq!(refs[1].as_str().unwrap(), b64(second));
    let listed = harness.history.list().unwrap();
    let id = listed[0].id.as_str();
    assert!(harness.root.join(id).join("result.jpg").is_file());
}

#[gpui::test]
fn four_prepared_pictures_clip_to_three_in_image_b64s(cx: &mut TestAppContext) {
    let images: Vec<_> = (0..4)
        .map(|index| encoded(format!("picture-{index}").as_bytes()))
        .collect();
    let harness = open(cx, vec![Ok(http(200, Some("image/jpeg"), jpeg()))]);
    submit(
        cx,
        &harness,
        request(
            "chutes-qwen-image-edit-2511",
            serde_json::json!({ "width": 1024, "height": 1024 }),
            images,
        ),
    );
    assert_one_post(
        &harness,
        "https://vonkaiser-qwen-image-edit-2511.chutes.ai/generate",
    );
    let body = posted_body(&harness);
    let refs = body["image_b64s"].as_array().unwrap();
    assert_eq!(refs.len(), 3);
    assert_eq!(refs[0].as_str().unwrap(), b64(b"picture-0"));
    assert_eq!(refs[2].as_str().unwrap(), b64(b"picture-2"));
}

#[gpui::test]
fn a_401_writes_a_failed_auth_row(cx: &mut TestAppContext) {
    let harness = open(cx, vec![Ok(http(401, None, b"bad key"))]);
    submit(cx, &harness, flux_request());
    assert_one_post(
        &harness,
        "https://vonkaiser-imageclassic.chutes.ai/image/flux",
    );
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
    assert!(matches!(history_error(&harness), ProviderError::Auth));
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_no_image(&harness, &id);
}

#[gpui::test]
fn a_429_writes_no_row(cx: &mut TestAppContext) {
    let harness = open(cx, vec![Ok(http(429, None, b"slow down"))]);
    submit(cx, &harness, flux_request());
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
fn a_404_writes_a_model_not_found_row(cx: &mut TestAppContext) {
    let harness = open(cx, vec![Ok(http(404, None, b"gone"))]);
    submit(cx, &harness, flux_request());
    assert!(matches!(
        history_error(&harness),
        ProviderError::ModelNotFound
    ));
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_no_image(&harness, &id);
}

#[gpui::test]
fn a_402_writes_no_row(cx: &mut TestAppContext) {
    let harness = open(cx, vec![Ok(http(402, None, b"funding"))]);
    submit(cx, &harness, flux_request());
    assert_eq!(harness.transport.seen().len(), 1);
    assert!(harness.history.list().unwrap().is_empty());
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(
        matches!(
            states[0],
            JobState::Failed {
                error: ProviderError::OutOfCredits { hint: None }
            }
        ),
        "{states:?}"
    );
}

#[gpui::test]
fn a_403_writes_no_row(cx: &mut TestAppContext) {
    let harness = open(cx, vec![Ok(http(403, None, b"forbidden"))]);
    submit(cx, &harness, flux_request());
    assert!(harness.history.list().unwrap().is_empty());
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(
        matches!(
            states[0],
            JobState::Failed {
                error: ProviderError::Http { status: 403, .. }
            }
        ),
        "{states:?}"
    );
}

#[gpui::test]
fn a_json_200_writes_a_failed_no_image_row(cx: &mut TestAppContext) {
    let harness = open(cx, vec![Ok(http(200, Some("application/json"), b"{}"))]);
    submit(cx, &harness, flux_request());
    assert_one_post(
        &harness,
        "https://vonkaiser-imageclassic.chutes.ai/image/flux",
    );
    assert!(matches!(history_error(&harness), ProviderError::NoImage));
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_no_image(&harness, &id);
    assert!(harness.root.join(&id).join("request.json").is_file());
    assert!(harness.root.join(&id).join("meta.json").is_file());
}

#[gpui::test]
fn dismissing_during_the_post_drops_the_late_200(cx: &mut TestAppContext) {
    let harness = open(cx, vec![Ok(http(200, Some("image/jpeg"), jpeg()))]);
    harness
        .runner
        .update(cx, |runner, cx| runner.submit(flux_request(), cx));
    harness
        .runner
        .update(cx, |runner, cx| runner.dismiss(0, cx));
    cx.run_until_parked();
    assert!(harness.history.list().unwrap().is_empty());
    assert!(
        harness
            .transport
            .seen()
            .iter()
            .all(|request| request.method == Method::Post)
    );
}
