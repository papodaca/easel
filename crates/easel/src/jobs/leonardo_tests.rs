use std::sync::Arc;
use std::time::Duration;

use easel_core::error::ProviderError;
use easel_core::history::HistoryStore;
use easel_core::http::{HttpResponse, Method};
use easel_core::job::{JobRequest, JobState};
use easel_core::model_spec::ProviderId;
use easel_core::providers::Provider;
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

const GENERATION_ID: &str = "11111111-2222-4333-8444-555555555555";
const POLL_URL: &str =
    "https://cloud.leonardo.ai/api/rest/v1/generations/11111111-2222-4333-8444-555555555555";
const CDN_URL: &str = "https://cdn.leonardo.ai/users/u/generations/g/out.jpg";

fn submit_ok() -> Result<HttpResponse, crate::net::TransportError> {
    Ok(http(
        200,
        format!(r#"{{"generationId": "{GENERATION_ID}", "apiCreditCost": 12}}"#),
    ))
}

fn poll_body(status: &str, images: &str) -> String {
    format!(r#"{{"generations_by_pk": {{"status": "{status}", "generated_images": {images}}}}}"#)
}

fn complete_ok(nsfw: bool) -> Result<HttpResponse, crate::net::TransportError> {
    Ok(http(
        200,
        poll_body(
            "COMPLETE",
            &format!(r#"[{{"url": "{CDN_URL}", "nsfw": {nsfw}, "id": "img-1"}}]"#),
        ),
    ))
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
        Arc::new(MapKeys::default().insert(ProviderId::Leonardo, "leonardo-test-key"));
    let root = std::env::temp_dir().join(format!(
        "easel-leonardo-{}-{}",
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
                    |model_id: &str| {
                        easel_core::providers::leonardo::resolve(model_id)
                            .map(|provider| Box::new(provider) as Box<dyn Provider>)
                    },
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
        model_id: "leonardo-phoenix-v1.0".into(),
        compiled_prompt: "ancient observatory".into(),
        params: std::collections::BTreeMap::new(),
        images: Vec::new(),
        box_snapshot: Vec::new(),
        created_at: "2026-10-25T00:00:00Z".into(),
    }
}

fn submit(cx: &mut TestAppContext, harness: &Harness) {
    harness
        .runner
        .update(cx, |runner, cx| runner.submit(request(), cx));
    cx.run_until_parked();
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
    for name in ["result.png", "result.jpg", "result.webp"] {
        assert!(!dir.join(name).exists(), "{name}");
    }
}

#[gpui::test]
fn submit_polls_pending_then_downloads_from_the_cdn(cx: &mut TestAppContext) {
    let bytes = png();
    let harness = open(
        cx,
        vec![
            submit_ok(),
            Ok(http(200, poll_body("PENDING", "[]"))),
            complete_ok(false),
            Ok(http(200, bytes.clone())),
        ],
    );
    submit(cx, &harness);
    let seen = harness.transport.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].method, Method::Post);
    assert_eq!(
        seen[0].url,
        "https://cloud.leonardo.ai/api/rest/v2/generations"
    );
    assert_eq!(seen[0].auth, Some(ProviderId::Leonardo));
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(states[0], JobState::Polling { .. }), "{states:?}");
    let flying = harness.history.in_flight().unwrap();
    assert_eq!(flying.len(), 1);
    assert_eq!(flying[0].provider, "leonardo");
    assert_eq!(flying[0].model_id, "leonardo-phoenix-v1.0");
    assert_eq!(flying[0].poll_url.as_deref(), Some(POLL_URL));

    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    let seen = harness.transport.seen();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[1].method, Method::Get);
    assert_eq!(seen[1].url, POLL_URL);
    assert_eq!(seen[1].auth, Some(ProviderId::Leonardo));
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(states[0], JobState::Polling { .. }), "{states:?}");

    cx.executor().advance_clock(Duration::from_millis(2500));
    cx.run_until_parked();
    let seen = harness.transport.seen();
    assert_eq!(
        seen.len(),
        4,
        "{:?}",
        seen.iter().map(|r| &r.url).collect::<Vec<_>>()
    );
    assert_eq!(seen[2].url, POLL_URL);
    assert_eq!(seen[3].url, CDN_URL);
    assert_eq!(seen[3].auth, None, "the CDN download is unauthenticated");
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(states[0], JobState::Done { .. }), "{states:?}");
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_eq!(
        std::fs::read(harness.root.join(&id).join("result.png")).unwrap(),
        bytes
    );
    assert!(harness.root.join(&id).join("thumb.jpg").is_file());
    let meta: serde_json::Value =
        serde_json::from_slice(&std::fs::read(harness.root.join(&id).join("meta.json")).unwrap())
            .unwrap();
    assert_eq!(meta["provider"], "leonardo");
    assert_eq!(meta["model_id"], "leonardo-phoenix-v1.0");
    assert!(meta["poll_url"].is_null());
}

#[gpui::test]
fn a_jpeg_result_writes_result_jpg(cx: &mut TestAppContext) {
    let bytes = jpeg();
    let harness = open(
        cx,
        vec![
            submit_ok(),
            complete_ok(false),
            Ok(http(200, bytes.clone())),
        ],
    );
    submit(cx, &harness);
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(5));
    cx.run_until_parked();
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_eq!(
        std::fs::read(harness.root.join(&id).join("result.jpg")).unwrap(),
        bytes
    );
}

#[gpui::test]
fn a_webp_result_writes_result_webp(cx: &mut TestAppContext) {
    let bytes = webp();
    let harness = open(
        cx,
        vec![
            submit_ok(),
            complete_ok(false),
            Ok(http(200, bytes.clone())),
        ],
    );
    submit(cx, &harness);
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(5));
    cx.run_until_parked();
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_eq!(
        std::fs::read(harness.root.join(&id).join("result.webp")).unwrap(),
        bytes
    );
}

#[gpui::test]
fn an_nsfw_complete_poll_blocks_without_a_download(cx: &mut TestAppContext) {
    let harness = open(cx, vec![submit_ok(), complete_ok(true)]);
    submit(cx, &harness);
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
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
    assert_eq!(harness.transport.seen().len(), 2, "no CDN download");
    assert!(matches!(history_error(&harness), ProviderError::Blocked));
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_no_image(&harness.root, &id);
}

#[gpui::test]
fn a_moderated_submit_blocks_and_writes_a_failed_row(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![Ok(http(
            400,
            r#"{"error": "content moderation filter: nude", "code": "unexpected"}"#,
        ))],
    );
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
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_no_image(&harness.root, &id);
}

#[gpui::test]
fn http_401_and_403_fail_with_auth(cx: &mut TestAppContext) {
    for status in [401, 403] {
        let harness = open(cx, vec![Ok(http(status, r#"{"error": "access-denied"}"#))]);
        submit(cx, &harness);
        let states = harness.runner.read_with(cx, |runner, _| runner.states());
        assert!(
            matches!(
                states[0],
                JobState::Failed {
                    error: ProviderError::Auth
                }
            ),
            "{status}: {states:?}"
        );
        assert!(matches!(history_error(&harness), ProviderError::Auth));
        let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
        assert_no_image(&harness.root, &id);
    }
}

#[gpui::test]
fn http_404_is_model_not_found(cx: &mut TestAppContext) {
    let harness = open(cx, vec![Ok(http(404, r#"{"error": "unknown model"}"#))]);
    submit(cx, &harness);
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(
        matches!(
            states[0],
            JobState::Failed {
                error: ProviderError::ModelNotFound
            }
        ),
        "{states:?}"
    );
    assert!(matches!(
        history_error(&harness),
        ProviderError::ModelNotFound
    ));
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_no_image(&harness.root, &id);
}

#[gpui::test]
fn validation_402_429_and_500_writes_no_row(cx: &mut TestAppContext) {
    struct Case {
        status: u16,
        body: &'static str,
    }
    for case in [
        Case {
            status: 400,
            body: r#"{"error": "Unexpected variable", "code": "bad-request"}"#,
        },
        Case {
            status: 402,
            body: r#"{"error": "payment required"}"#,
        },
        Case {
            status: 429,
            body: r#"{"error": "too many requests"}"#,
        },
        Case {
            status: 500,
            body: "boom",
        },
    ] {
        let harness = open(cx, vec![Ok(http(case.status, case.body))]);
        submit(cx, &harness);
        assert!(
            harness.history.list().unwrap().is_empty(),
            "{}",
            case.status
        );
        let states = harness.runner.read_with(cx, |runner, _| runner.states());
        assert!(matches!(states[0], JobState::Failed { .. }), "{states:?}");
    }
    let harness = open(
        cx,
        vec![Ok(http(
            400,
            r#"{"error": "Unexpected variable", "code": "bad-request"}"#,
        ))],
    );
    submit(cx, &harness);
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    match &states[0] {
        JobState::Failed {
            error: ProviderError::Validation { message, .. },
        } => assert_eq!(message, "Unexpected variable"),
        other => panic!("{other:?}"),
    }
}

#[gpui::test]
fn dismissing_during_polling_marks_the_row_and_ignores_late_polls(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![
            submit_ok(),
            Ok(http(200, poll_body("PENDING", "[]"))),
            complete_ok(false),
        ],
    );
    submit(cx, &harness);
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(states[0], JobState::Polling { .. }), "{states:?}");
    harness
        .runner
        .update(cx, |runner, cx| runner.dismiss(0, cx));
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(states[0], JobState::Dismissed), "{states:?}");
    let listed = harness.history.list().unwrap();
    assert!(matches!(listed[0].state, JobState::Dismissed), "{listed:?}");

    cx.executor().advance_clock(Duration::from_millis(2500));
    cx.run_until_parked();
    let seen = harness.transport.seen();
    assert_eq!(seen.len(), 2, "the late COMPLETE poll is never downloaded");
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(states[0], JobState::Dismissed), "{states:?}");
    let listed = harness.history.list().unwrap();
    assert!(matches!(listed[0].state, JobState::Dismissed), "{listed:?}");
}

#[gpui::test]
fn restoring_a_completed_history_row_restores_the_editor(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![submit_ok(), complete_ok(false), Ok(http(200, png()))],
    );
    let editor = easel_core::history::EditorSnapshot {
        prose: "ancient observatory".into(),
        model_id: "leonardo-phoenix-v1.0".into(),
        values: {
            let mut values = std::collections::BTreeMap::new();
            values.insert("width".into(), serde_json::json!(1536));
            values.insert("height".into(), serde_json::json!(1536));
            values
        },
        aspect_ratio: String::new(),
    };
    harness.runner.update(cx, |runner, cx| {
        runner.submit_recorded(request(), Some(editor), cx)
    });
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(5));
    cx.run_until_parked();
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(states[0], JobState::Done { .. }), "{states:?}");
    let id = harness.history.list().unwrap()[0].id.clone();
    let loaded = harness.history.load_request(&id).unwrap();
    assert_eq!(loaded.model_id, "leonardo-phoenix-v1.0");
    assert_eq!(loaded.compiled_prompt, "ancient observatory");
    let stored = harness
        .history
        .load_editor(&id)
        .unwrap()
        .expect("editor snapshot");

    // The same steps bottom_strip::restore_selected performs for a history row.
    let state = cx.update(|cx| cx.new(|_| crate::app_state::AppState::new()));
    state.update(cx, |app, cx| {
        app.set_model("leonardo-ideogram-v4.0", cx);
        app.scene.prose = "changed".into();
        app.set_model(&stored.model_id, cx);
        app.values = app.reconcile_values(stored.values.clone());
        app.scene.prose = stored.prose.clone();
        cx.notify();
    });
    let (model, prose, width, height) = state.read_with(cx, |app, _| {
        (
            app.model_id.clone(),
            app.scene.prose.clone(),
            app.values["width"].clone(),
            app.values["height"].clone(),
        )
    });
    assert_eq!(model, "leonardo-phoenix-v1.0");
    assert_eq!(prose, "ancient observatory");
    assert_eq!(width, serde_json::json!(1536));
    assert_eq!(height, serde_json::json!(1536));
}
