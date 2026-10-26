use std::sync::Arc;
use std::time::Duration;

use easel_core::error::ProviderError;
use easel_core::history::{EditorSnapshot, HistoryStore};
use easel_core::http::{Body, HttpHeader, HttpResponse, Method};
use easel_core::job::{EncodedImage, JobRequest, JobState};
use easel_core::model_spec::ProviderId;
use gpui::{TestAppContext, prelude::*};
use serde_json::Value;

use crate::app_state::AppState;
use crate::bottom_strip::BottomStrip;
use crate::jobs::JobRunner;
use crate::net::{FakeTransport, HttpTransport, KeyResolver, MapKeys, TransportError};

fn http(status: u16, content_type: Option<&str>, body: impl AsRef<[u8]>) -> HttpResponse {
    let mut headers = Vec::new();
    if let Some(value) = content_type {
        headers.push(HttpHeader {
            name: "Content-Type".into(),
            value: value.into(),
            sensitive: false,
        });
    }
    HttpResponse {
        status,
        headers,
        body: body.as_ref().to_vec(),
    }
}

fn image_ok(status: u16, content_type: &str, body: &[u8], finish_reason: &str) -> HttpResponse {
    let mut response = http(status, Some(content_type), body);
    response.headers.push(HttpHeader {
        name: "finish-reason".into(),
        value: finish_reason.into(),
        sensitive: false,
    });
    response
}

fn png() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/pixel.png"
    ))
    .unwrap()
}

fn jpeg() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/photo.jpg"
    ))
    .unwrap()
}

fn webp() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/picture.webp"
    ))
    .unwrap()
}

fn square_png() -> Vec<u8> {
    let image = image::RgbaImage::from_fn(256, 256, |x, y| {
        image::Rgba([x as u8, y as u8, (x + y) as u8, 255])
    });
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(image)
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .unwrap();
    bytes
}

fn encoded(media_type: &str, bytes: Vec<u8>, width: u32, height: u32) -> EncodedImage {
    EncodedImage {
        media_type: media_type.into(),
        bytes,
        width,
        height,
        content_hash: format!("{media_type}-{width}x{height}"),
    }
}

fn hex_id() -> String {
    "a1".repeat(32)
}

struct Harness {
    runner: gpui::Entity<JobRunner>,
    transport: Arc<FakeTransport>,
    history: Arc<HistoryStore>,
    root: std::path::PathBuf,
}

fn open(cx: &mut TestAppContext, script: Vec<Result<HttpResponse, TransportError>>) -> Harness {
    let transport = Arc::new(FakeTransport::scripted(script));
    let keys: Arc<dyn KeyResolver> =
        Arc::new(MapKeys::default().insert(ProviderId::Stability, "stability-test-key"));
    let root = std::env::temp_dir().join(format!(
        "easel-stability-{}-{}",
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
        created_at: "2026-10-25T00:00:00Z".into(),
    }
}

fn submit(cx: &mut TestAppContext, harness: &Harness, job: JobRequest) {
    harness
        .runner
        .update(cx, |runner, cx| runner.submit(job, cx));
    cx.run_until_parked();
}

fn job_states(cx: &TestAppContext, harness: &Harness) -> Vec<JobState> {
    harness.runner.read_with(cx, |runner, _| runner.states())
}

fn live_status(cx: &TestAppContext, harness: &Harness) -> String {
    harness.runner.read_with(cx, |runner, _| {
        runner
            .live_jobs()
            .first()
            .map(|job| job.status.clone())
            .unwrap_or_default()
    })
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

fn assert_accept_image(headers: &[easel_core::http::HttpHeader]) {
    assert!(
        headers
            .iter()
            .any(|header| header.name == "Accept" && header.value == "image/*"),
        "{headers:?}"
    );
}

fn assert_one_post(harness: &Harness, url: &str) {
    let seen = harness.transport.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].method, Method::Post);
    assert_eq!(seen[0].url, url);
    assert_eq!(seen[0].auth, Some(ProviderId::Stability));
    assert!(
        matches!(seen[0].body, Body::Multipart { .. }),
        "{:?}",
        seen[0].body
    );
    assert_accept_image(&seen[0].headers);
}

fn assert_poll_get(harness: &Harness, index: usize, url: &str) {
    let seen = harness.transport.seen();
    assert!(index < seen.len(), "{} of {}", index, seen.len());
    assert_eq!(seen[index].method, Method::Get);
    assert_eq!(seen[index].url, url);
    assert_eq!(seen[index].auth, Some(ProviderId::Stability));
    assert_accept_image(&seen[index].headers);
}

#[gpui::test]
fn a_200_png_from_stable_image_ultra_writes_result_png_after_one_post(cx: &mut TestAppContext) {
    let bytes = png();
    let harness = open(cx, vec![Ok(image_ok(200, "image/png", &bytes, "SUCCESS"))]);
    submit(
        cx,
        &harness,
        request("stability-ultra", serde_json::json!({}), Vec::new()),
    );
    assert_one_post(
        &harness,
        "https://api.stability.ai/v2beta/stable-image/generate/ultra",
    );
    let states = job_states(cx, &harness);
    assert!(matches!(states[0], JobState::Done { .. }), "{states:?}");
    let listed = harness.history.list().unwrap();
    let id = listed[0].id.as_str();
    assert_eq!(
        std::fs::read(harness.root.join(id).join("result.png")).unwrap(),
        bytes
    );
    let thumb = std::fs::read(harness.root.join(id).join("thumb.jpg")).unwrap();
    assert!(!thumb.is_empty());
    assert!(harness.root.join(id).join("request.json").is_file());
    let meta = meta_of(&harness, id);
    assert_eq!(meta["provider"], "stability");
    assert_eq!(meta["model_id"], "stability-ultra");
    assert!(meta["poll_url"].is_null());
}

#[gpui::test]
fn a_200_jpeg_from_sd35_large_writes_result_jpg_after_one_post(cx: &mut TestAppContext) {
    let bytes = jpeg();
    let harness = open(cx, vec![Ok(image_ok(200, "image/jpeg", &bytes, "SUCCESS"))]);
    submit(
        cx,
        &harness,
        request("sd3.5-large", serde_json::json!({}), Vec::new()),
    );
    assert_one_post(
        &harness,
        "https://api.stability.ai/v2beta/stable-image/generate/sd3",
    );
    let states = job_states(cx, &harness);
    assert!(matches!(states[0], JobState::Done { .. }), "{states:?}");
    let listed = harness.history.list().unwrap();
    let id = listed[0].id.as_str();
    assert_eq!(
        std::fs::read(harness.root.join(id).join("result.jpg")).unwrap(),
        bytes
    );
    assert!(!harness.root.join(id).join("result.png").exists());
    let thumb = std::fs::read(harness.root.join(id).join("thumb.jpg")).unwrap();
    assert!(!thumb.is_empty());
    let meta = meta_of(&harness, id);
    assert_eq!(meta["provider"], "stability");
    assert!(meta["poll_url"].is_null());
}

#[gpui::test]
fn a_200_webp_from_remove_background_writes_result_webp_after_one_post(cx: &mut TestAppContext) {
    let bytes = webp();
    let harness = open(cx, vec![Ok(image_ok(200, "image/webp", &bytes, "SUCCESS"))]);
    submit(
        cx,
        &harness,
        request(
            "stability-remove-background",
            serde_json::json!({}),
            vec![encoded("image/png", square_png(), 256, 256)],
        ),
    );
    assert_one_post(
        &harness,
        "https://api.stability.ai/v2beta/stable-image/edit/remove-background",
    );
    let states = job_states(cx, &harness);
    assert!(matches!(states[0], JobState::Done { .. }), "{states:?}");
    let listed = harness.history.list().unwrap();
    let id = listed[0].id.as_str();
    assert_eq!(
        std::fs::read(harness.root.join(id).join("result.webp")).unwrap(),
        bytes
    );
    let thumb = std::fs::read(harness.root.join(id).join("thumb.jpg")).unwrap();
    assert!(!thumb.is_empty());
    let meta = meta_of(&harness, id);
    assert_eq!(meta["provider"], "stability");
    assert!(meta["poll_url"].is_null());
}

#[gpui::test]
fn creative_upscale_polls_202_then_completes_inline_from_the_200(cx: &mut TestAppContext) {
    let id = hex_id();
    let poll_url = format!("https://api.stability.ai/v2beta/results/{id}");
    let result = webp();
    let harness = open(
        cx,
        vec![
            Ok(http(
                200,
                Some("application/json"),
                format!(r#"{{"id":"{id}"}}"#),
            )),
            Ok(http(202, None, "")),
            Ok(image_ok(200, "image/webp", &result, "SUCCESS")),
        ],
    );
    submit(
        cx,
        &harness,
        request(
            "stability-upscale-creative",
            serde_json::json!({}),
            vec![encoded("image/png", square_png(), 256, 256)],
        ),
    );
    assert_one_post(
        &harness,
        "https://api.stability.ai/v2beta/stable-image/upscale/creative",
    );
    let listed = harness.history.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert!(
        matches!(listed[0].state, JobState::Polling { .. }),
        "{:?}",
        listed[0].state
    );
    let meta = meta_of(&harness, &id);
    assert_eq!(meta["provider"], "stability");
    assert_eq!(meta["model_id"], "stability-upscale-creative");
    assert_eq!(meta["poll_url"], poll_url);
    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    assert_poll_get(&harness, 1, &poll_url);
    let states = job_states(cx, &harness);
    assert!(matches!(states[0], JobState::Polling { .. }), "{states:?}");
    assert_eq!(live_status(cx, &harness), "in-progress");
    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    assert_poll_get(&harness, 2, &poll_url);
    let seen = harness.transport.seen();
    assert_eq!(seen.len(), 3, "1 POST + 2 GETs, no download GET");
    assert!(
        seen.iter()
            .all(|request| request.auth == Some(ProviderId::Stability)),
        "{seen:?}"
    );
    let states = job_states(cx, &harness);
    assert!(matches!(states[0], JobState::Done { .. }), "{states:?}");
    assert_eq!(
        std::fs::read(harness.root.join(&id).join("result.webp")).unwrap(),
        result
    );
    let thumb = std::fs::read(harness.root.join(&id).join("thumb.jpg")).unwrap();
    assert!(!thumb.is_empty());
    let meta = meta_of(&harness, &id);
    assert_eq!(meta["provider"], "stability");
    assert!(meta["poll_url"].is_null());
}

#[gpui::test]
fn replace_background_and_relight_polls_202_then_completes_inline_from_the_200(
    cx: &mut TestAppContext,
) {
    let id = hex_id();
    let poll_url = format!("https://api.stability.ai/v2beta/results/{id}");
    let result = webp();
    let harness = open(
        cx,
        vec![
            Ok(http(
                200,
                Some("application/json"),
                format!(r#"{{"id":"{id}"}}"#),
            )),
            Ok(http(202, None, "")),
            Ok(image_ok(200, "image/webp", &result, "SUCCESS")),
        ],
    );
    submit(
        cx,
        &harness,
        request(
            "stability-replace-background-and-relight",
            serde_json::json!({}),
            vec![
                encoded("image/png", square_png(), 256, 256),
                encoded("image/jpeg", jpeg(), 8, 4),
            ],
        ),
    );
    assert_one_post(
        &harness,
        "https://api.stability.ai/v2beta/stable-image/edit/replace-background-and-relight",
    );
    let listed = harness.history.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert!(
        matches!(listed[0].state, JobState::Polling { .. }),
        "{:?}",
        listed[0].state
    );
    let meta = meta_of(&harness, &id);
    assert_eq!(meta["provider"], "stability");
    assert_eq!(meta["poll_url"], poll_url);
    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    assert_poll_get(&harness, 1, &poll_url);
    assert!(matches!(
        job_states(cx, &harness)[0],
        JobState::Polling { .. }
    ));
    assert_eq!(live_status(cx, &harness), "in-progress");
    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    assert_eq!(harness.transport.seen().len(), 3);
    assert!(
        matches!(job_states(cx, &harness)[0], JobState::Done { .. }),
        "{:?}",
        job_states(cx, &harness)
    );
    assert_eq!(
        std::fs::read(harness.root.join(&id).join("result.webp")).unwrap(),
        result
    );
    assert!(meta_of(&harness, &id)["poll_url"].is_null());
}

#[gpui::test]
fn a_content_filtered_submit_fails_blocked_without_an_image(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![Ok(image_ok(200, "image/png", &png(), "CONTENT_FILTERED"))],
    );
    submit(
        cx,
        &harness,
        request("stability-ultra", serde_json::json!({}), Vec::new()),
    );
    assert_one_post(
        &harness,
        "https://api.stability.ai/v2beta/stable-image/generate/ultra",
    );
    let states = job_states(cx, &harness);
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
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_no_image(&harness, &id);
    assert_eq!(meta_of(&harness, &id)["provider"], "stability");
}

#[gpui::test]
fn a_content_filtered_poll_fails_blocked_without_an_image(cx: &mut TestAppContext) {
    let id = hex_id();
    let harness = open(
        cx,
        vec![
            Ok(http(
                200,
                Some("application/json"),
                format!(r#"{{"id":"{id}"}}"#),
            )),
            Ok(image_ok(200, "image/webp", &webp(), "CONTENT_FILTERED")),
        ],
    );
    submit(
        cx,
        &harness,
        request(
            "stability-upscale-creative",
            serde_json::json!({}),
            vec![encoded("image/png", square_png(), 256, 256)],
        ),
    );
    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    let states = job_states(cx, &harness);
    assert!(
        matches!(
            states[0],
            JobState::Failed {
                error: ProviderError::Blocked
            }
        ),
        "{states:?}"
    );
    let listed = harness.history.list().unwrap();
    let JobState::Failed { error } = &listed[0].state else {
        panic!("{:?}", listed);
    };
    assert!(matches!(error, ProviderError::Blocked));
    assert_no_image(&harness, &id);
    assert_eq!(harness.transport.seen().len(), 2);
}

#[gpui::test]
fn moderation_auth_and_missing_model_submits_write_failed_rows(cx: &mut TestAppContext) {
    for (status, body, expected) in [
        (
            403,
            r#"{"id":"1","name":"content_moderation","errors":["flagged by content moderation"]}"#,
            ProviderError::Blocked,
        ),
        (401, "bad key", ProviderError::Auth),
        (404, "gone", ProviderError::ModelNotFound),
    ] {
        let harness = open(cx, vec![Ok(http(status, None, body))]);
        submit(
            cx,
            &harness,
            request("stability-ultra", serde_json::json!({}), Vec::new()),
        );
        assert_eq!(harness.transport.seen().len(), 1, "{expected:?}");
        assert_eq!(history_error(&harness), expected, "{expected:?}");
        let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
        assert_no_image(&harness, &id);
    }
}

#[gpui::test]
fn a_400_with_errors_fails_validation_and_writes_no_row(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![Ok(http(
            400,
            Some("application/json"),
            r#"{"id":"1","name":"bad_request","errors":["prompt: is required"]}"#,
        ))],
    );
    submit(
        cx,
        &harness,
        request("stability-ultra", serde_json::json!({}), Vec::new()),
    );
    assert_eq!(harness.transport.seen().len(), 1);
    let states = job_states(cx, &harness);
    match &states[0] {
        JobState::Failed {
            error:
                ProviderError::Validation {
                    message,
                    field_errors,
                },
        } => {
            assert_eq!(message, "prompt: is required");
            assert!(field_errors.is_empty());
        }
        other => panic!("{other:?}"),
    }
    assert!(harness.history.list().unwrap().is_empty());
}

#[gpui::test]
fn a_429_fails_rate_limited_and_writes_no_row(cx: &mut TestAppContext) {
    let harness = open(cx, vec![Ok(http(429, None, "slow down"))]);
    submit(
        cx,
        &harness,
        request("stability-ultra", serde_json::json!({}), Vec::new()),
    );
    assert_eq!(harness.transport.seen().len(), 1);
    assert!(harness.history.list().unwrap().is_empty());
    let states = job_states(cx, &harness);
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
fn dismissing_a_polling_creative_upscale_marks_the_row_dismissed_and_ignores_the_late_200(
    cx: &mut TestAppContext,
) {
    let id = hex_id();
    let harness = open(
        cx,
        vec![
            Ok(http(
                200,
                Some("application/json"),
                format!(r#"{{"id":"{id}"}}"#),
            )),
            Ok(http(202, None, "")),
            Ok(image_ok(200, "image/webp", &webp(), "SUCCESS")),
        ],
    );
    submit(
        cx,
        &harness,
        request(
            "stability-upscale-creative",
            serde_json::json!({}),
            vec![encoded("image/png", square_png(), 256, 256)],
        ),
    );
    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    assert!(matches!(
        job_states(cx, &harness)[0],
        JobState::Polling { .. }
    ));
    harness
        .runner
        .update(cx, |runner, cx| runner.dismiss(0, cx));
    let listed = harness.history.list().unwrap();
    assert!(
        matches!(listed[0].state, JobState::Dismissed),
        "{:?}",
        listed[0].state
    );
    assert_eq!(job_states(cx, &harness), vec![JobState::Dismissed]);
    assert!(meta_of(&harness, &id)["poll_url"].is_null());
    assert_no_image(&harness, &id);
    cx.executor().advance_clock(Duration::from_secs(10));
    cx.run_until_parked();
    assert_eq!(harness.transport.seen().len(), 2, "late 200 ignored");
    let listed = harness.history.list().unwrap();
    assert!(matches!(listed[0].state, JobState::Dismissed));
}

#[gpui::test]
fn a_completed_stability_row_restores_the_model_prompt_and_values(cx: &mut TestAppContext) {
    let harness = open(cx, vec![Ok(image_ok(200, "image/png", &png(), "SUCCESS"))]);
    let editor = EditorSnapshot {
        prose: "a lighthouse at dusk".into(),
        model_id: "stability-ultra".into(),
        values: serde_json::from_value(serde_json::json!({
            "aspect_ratio": "16:9",
            "strength": 0.55,
            "seed": 7,
            "output_format": "webp",
        }))
        .unwrap(),
        aspect_ratio: "16:9".into(),
    };
    harness.runner.update(cx, |runner, cx| {
        runner.submit_recorded(
            request("stability-ultra", serde_json::json!({}), Vec::new()),
            Some(editor),
            cx,
        );
    });
    cx.run_until_parked();
    assert!(matches!(job_states(cx, &harness)[0], JobState::Done { .. }));
    let app = cx.update(|cx| cx.new(|_| AppState::new()));
    let strip = cx.update({
        let app = app.clone();
        let runner = harness.runner.clone();
        let history = harness.history.clone();
        move |cx| cx.new(|cx| BottomStrip::new(app, Some(runner), history, cx))
    });
    strip.update(cx, |strip, cx| {
        strip.show(0, cx);
        strip.press("Restore", cx);
    });
    let (model, prose, aspect, values) = app.read_with(cx, |app, _| {
        (
            app.model_id.clone(),
            app.scene.prose.clone(),
            app.scene.aspect_ratio.clone(),
            app.values.clone(),
        )
    });
    assert_eq!(model, "stability-ultra");
    assert_eq!(prose, "a lighthouse at dusk");
    assert_eq!(aspect.as_deref(), Some("16:9"));
    assert_eq!(
        values.get("output_format"),
        Some(&serde_json::json!("webp"))
    );
    assert_eq!(values.get("strength"), Some(&serde_json::json!(0.55)));
    assert_eq!(values.get("seed"), Some(&serde_json::json!(7)));
}
