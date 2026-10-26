use std::sync::Arc;
use std::time::Duration;

use easel_core::error::ProviderError;
use easel_core::history::HistoryStore;
use easel_core::http::{Body, HttpResponse, Method};
use easel_core::job::{EncodedImage, JobRequest, JobState};
use easel_core::model_spec::ProviderId;
use gpui::{Task, TestAppContext, prelude::*};
use serde_json::json;

use crate::app_state::AppState;
use crate::jobs::JobRunner;
use crate::net::{FakeTransport, HttpTransport, KeyResolver, MapKeys, TransportError};

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

fn grid_png(seed: u8) -> Vec<u8> {
    let mut image = image::RgbImage::new(64, 64);
    image.put_pixel(0, 0, image::Rgb([seed, seed, seed]));
    easel_core::image_prep::encode_png(&image::DynamicImage::ImageRgb8(image))
}

fn prepared(seed: &str) -> EncodedImage {
    EncodedImage {
        media_type: "image/png".into(),
        bytes: png(),
        width: 16,
        height: 16,
        content_hash: seed.into(),
    }
}

fn request(model: &str, images: Vec<EncodedImage>) -> JobRequest {
    let defaults = easel_core::providers::resolve_model(model)
        .map(|provider| provider.spec().schema.defaults())
        .unwrap_or_default();
    JobRequest {
        model_id: model.into(),
        compiled_prompt: "a lighthouse".into(),
        params: defaults,
        images,
        box_snapshot: Vec::new(),
        created_at: "2026-10-25T10:00:00Z".into(),
    }
}

struct Harness {
    runner: gpui::Entity<JobRunner>,
    transport: Arc<FakeTransport>,
    history: Arc<HistoryStore>,
    state: Option<gpui::Entity<AppState>>,
    root: std::path::PathBuf,
}

fn open(
    cx: &mut TestAppContext,
    name: &str,
    script: Vec<Result<HttpResponse, TransportError>>,
    watch: bool,
) -> Harness {
    open_with_stall(
        cx,
        name,
        Arc::new(FakeTransport::scripted(script)),
        None,
        watch,
    )
}

/// Wraps a scripted transport so the first CDN download resolves only after
/// `stall` of simulated time, exercising the download-phase deadline.
struct StallingDownloads {
    inner: Arc<FakeTransport>,
    stall: std::sync::Mutex<Option<Duration>>,
}

impl HttpTransport for StallingDownloads {
    fn request(
        &self,
        request: easel_core::http::HttpRequest,
        cx: &mut gpui::App,
    ) -> Task<Result<HttpResponse, TransportError>> {
        let download = request.url.contains("cdn2.apiframe.ai");
        let stall = if download {
            self.stall.lock().expect("stall").take()
        } else {
            None
        };
        let inner_task = self.inner.request(request, cx);
        match stall {
            Some(delay) => cx.spawn(async move |cx| {
                cx.background_executor().timer(delay).await;
                inner_task.await
            }),
            None => inner_task,
        }
    }
}

fn open_with_stall(
    cx: &mut TestAppContext,
    name: &str,
    inner: Arc<FakeTransport>,
    stall: Option<Duration>,
    watch: bool,
) -> Harness {
    let transport: Arc<dyn HttpTransport> = Arc::new(StallingDownloads {
        inner: Arc::clone(&inner),
        stall: std::sync::Mutex::new(stall),
    });
    let keys: Arc<dyn KeyResolver> =
        Arc::new(MapKeys::default().insert(ProviderId::Apiframe, "apiframe-test-key"));
    let root = std::env::temp_dir().join(format!(
        "easel-apiframe-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&root);
    let history = Arc::new(HistoryStore::open(&root).unwrap());
    let (runner, state) = cx.update({
        let transport = transport.clone() as Arc<dyn HttpTransport>;
        let history = Arc::clone(&history);
        move |cx| {
            let state = cx.new(|_| AppState::new());
            let runner = cx.new(|_| {
                JobRunner::new(
                    easel_core::providers::resolve_model,
                    transport,
                    keys,
                    history,
                )
            });
            if watch {
                runner.update(cx, |runner, _| runner.watch_app_state(state.clone()));
            }
            (runner, state)
        }
    });
    Harness {
        runner,
        transport: inner,
        history,
        state: watch.then_some(state),
        root,
    }
}

fn completed_single(id: &str) -> String {
    format!(
        r#"{{"status":"COMPLETED","creditCost":5,"result":{{"images":["https://cdn2.apiframe.ai/images/{id}-1.png"]}}}}"#
    )
}

#[gpui::test]
fn flux2_pro_uploads_references_then_polls_and_caches(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        "flux2pro",
        vec![
            Ok(http(
                201,
                r#"{"url":"https://cdn2.apiframe.ai/uploads/u1.png"}"#,
            )),
            Ok(http(
                201,
                r#"{"url":"https://cdn2.apiframe.ai/uploads/u2.png"}"#,
            )),
            Ok(http(202, r#"{"jobId":"apjob1","status":"QUEUED"}"#)),
            Ok(http(200, r#"{"status":"QUEUED","progress":0}"#)),
            Ok(http(200, r#"{"status":"PROCESSING","progress":50}"#)),
            Ok(http(200, completed_single("apjob1"))),
            Ok(http(200, png())),
            Ok(http(202, r#"{"jobId":"apjob2","status":"QUEUED"}"#)),
            Ok(http(200, completed_single("apjob2"))),
            Ok(http(200, png())),
        ],
        false,
    );
    let first = request(
        "apiframe-flux-2-pro",
        vec![prepared("hash-one"), prepared("hash-two")],
    );
    harness
        .runner
        .update(cx, |runner, cx| runner.submit(first.clone(), cx));
    cx.run_until_parked();
    assert_eq!(
        harness.transport.seen().len(),
        3,
        "{:?}",
        harness.transport.seen()
    );
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(2500));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(3));
    cx.run_until_parked();
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(states[0], JobState::Done { .. }), "{states:?}");
    let seen = harness.transport.seen();
    assert_eq!(seen[0].method, Method::Post);
    assert_eq!(seen[0].url, "https://api.apiframe.ai/v2/uploads");
    assert_eq!(seen[0].auth, Some(ProviderId::Apiframe));
    assert_eq!(seen[1].url, "https://api.apiframe.ai/v2/uploads");
    let submit = &seen[2];
    assert_eq!(submit.url, "https://api.apiframe.ai/v2/images/generate");
    assert_eq!(submit.auth, Some(ProviderId::Apiframe));
    let Body::Json(body) = &submit.body else {
        panic!("expected json");
    };
    assert_eq!(body["model"], "flux-2-pro");
    assert_eq!(body["prompt"], "a lighthouse");
    assert_eq!(
        body["fluxParams"]["input_images"],
        json!([
            "https://cdn2.apiframe.ai/uploads/u1.png",
            "https://cdn2.apiframe.ai/uploads/u2.png"
        ])
    );
    assert!(seen.iter().any(|request| {
        request.method == Method::Get && request.url == "https://api.apiframe.ai/v2/jobs/apjob1"
    }));
    let download = seen.last().unwrap();
    assert_eq!(download.url, "https://cdn2.apiframe.ai/images/apjob1-1.png");
    assert_eq!(download.auth, None);
    let listed = harness.history.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert!(matches!(listed[0].state, JobState::Done { .. }));
    assert!(
        harness
            .root
            .join(listed[0].id.as_str())
            .join("result.png")
            .is_file()
    );

    harness
        .runner
        .update(cx, |runner, cx| runner.submit(first, cx));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(3));
    cx.run_until_parked();
    let seen = harness.transport.seen();
    let upload_count = seen
        .iter()
        .filter(|request| request.url == "https://api.apiframe.ai/v2/uploads")
        .count();
    assert_eq!(upload_count, 2, "cached uploads are not re-posted");
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(
        states
            .iter()
            .all(|state| matches!(state, JobState::Done { .. })),
        "{states:?}"
    );
    assert_eq!(harness.history.list().unwrap().len(), 2);
}

#[gpui::test]
fn a_text_only_model_skips_uploads_and_posts_directly(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        "dalle3",
        vec![
            Ok(http(202, r#"{"jobId":"dalle1","status":"QUEUED"}"#)),
            Ok(http(
                200,
                r#"{"status":"COMPLETED","creditCost":2,"result":{"images":["https://cdn2.apiframe.ai/images/dalle1-1.png"]}}"#,
            )),
            Ok(http(200, png())),
        ],
        false,
    );
    harness.runner.update(cx, |runner, cx| {
        runner.submit(request("apiframe-dall-e-3", vec![prepared("only")]), cx)
    });
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(3));
    cx.run_until_parked();
    let seen = harness.transport.seen();
    assert!(
        seen.iter()
            .all(|request| request.url != "https://api.apiframe.ai/v2/uploads"),
        "{seen:?}"
    );
    assert_eq!(seen.len(), 3, "{seen:?}");
    assert_eq!(seen[0].url, "https://api.apiframe.ai/v2/images/generate");
    let Body::Json(body) = &seen[0].body else {
        panic!("expected json");
    };
    assert_eq!(body["model"], "dall-e-3");
    assert_eq!(body["dalleParams"]["style"], "vivid");
    assert_eq!(body["dalleParams"]["aspect_ratio"], "1:1");
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(states[0], JobState::Done { .. }), "{states:?}");
}

fn mj_completed(id: &str) -> String {
    format!(
        r#"{{"status":"COMPLETED","creditCost":10,"result":{{"images":["https://cdn2.apiframe.ai/images/{id}-1.png","https://cdn2.apiframe.ai/images/{id}-2.png","https://cdn2.apiframe.ai/images/{id}-3.png","https://cdn2.apiframe.ai/images/{id}-4.png"],"gridUrl":"https://cdn2.apiframe.ai/images/{id}-grid.png"}}}}"#
    )
}

fn wait_for_polls(cx: &mut TestAppContext, polls: usize) {
    for _ in 0..polls {
        cx.executor().advance_clock(Duration::from_secs(2));
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(2500));
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_secs(3));
        cx.run_until_parked();
    }
}

#[gpui::test]
fn midjourney_downloads_five_files_and_opens_the_picker(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        "mj",
        vec![
            Ok(http(202, r#"{"jobId":"mjgrid1","status":"QUEUED"}"#)),
            Ok(http(200, r#"{"status":"QUEUED","progress":0}"#)),
            Ok(http(200, mj_completed("mjgrid1"))),
            Ok(http(200, grid_png(9))),
            Ok(http(200, grid_png(1))),
            Ok(http(200, grid_png(2))),
            Ok(http(200, grid_png(3))),
            Ok(http(200, grid_png(4))),
        ],
        true,
    );
    harness.runner.update(cx, |runner, cx| {
        runner.submit(request("apiframe-midjourney", Vec::new()), cx)
    });
    cx.run_until_parked();
    wait_for_polls(cx, 1);
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(states[0], JobState::Done { .. }), "{states:?}");

    let grid_id = easel_core::providers::JobId::parse("mjgrid1").unwrap();
    let meta = harness
        .history
        .grid_meta(&grid_id)
        .expect("grid meta stored");
    assert_eq!(meta.picked, [false, false, false, false]);
    assert_eq!(
        meta.grid_url,
        "https://cdn2.apiframe.ai/images/mjgrid1-grid.png"
    );
    assert_eq!(
        meta.images[0],
        "https://cdn2.apiframe.ai/images/mjgrid1-1.png"
    );
    let quadrants = harness.history.quadrant_bytes(&grid_id, 0..4);
    assert!(quadrants.iter().all(|bytes| bytes.is_some()));
    assert!(harness.root.join("mjgrid1/result.png").is_file());
    assert!(harness.root.join("mjgrid1/thumb.jpg").is_file());
    let listed = harness.history.list().unwrap();
    assert_eq!(listed.len(), 1);

    let opened = harness.state.as_ref().unwrap().read_with(cx, |app, _| {
        app.history_item
            .clone()
            .and_then(|item| item.id)
            .map(|id| id.as_str().to_string())
    });
    assert_eq!(opened.as_deref(), Some("mjgrid1"));
}

#[gpui::test]
fn a_failed_quadrant_download_leaves_no_grid_entry(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        "mj-partial",
        vec![
            Ok(http(202, r#"{"jobId":"mjgrid2","status":"QUEUED"}"#)),
            Ok(http(200, mj_completed("mjgrid2"))),
            Ok(http(200, grid_png(9))),
            Ok(http(200, grid_png(1))),
            Ok(http(200, grid_png(2))),
            Ok(http(200, b"not-an-image")),
        ],
        true,
    );
    harness.runner.update(cx, |runner, cx| {
        runner.submit(request("apiframe-midjourney", Vec::new()), cx)
    });
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(states[0], JobState::Failed { .. }), "{states:?}");
    let listed = harness.history.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert!(matches!(listed[0].state, JobState::Failed { .. }));
    assert!(
        harness
            .history
            .grid_meta(&easel_core::providers::JobId::parse("mjgrid2").unwrap())
            .is_none()
    );
    let opened = harness
        .state
        .as_ref()
        .unwrap()
        .read_with(cx, |app, _| app.history_item.clone());
    assert!(opened.is_none());

    let harness = open(
        cx,
        "mj-expired",
        vec![
            Ok(http(202, r#"{"jobId":"mjgrid3","status":"QUEUED"}"#)),
            Ok(http(200, mj_completed("mjgrid3"))),
            Ok(http(200, grid_png(9))),
            Ok(http(200, grid_png(1))),
            Ok(http(200, grid_png(2))),
            Ok(http(404, "gone")),
        ],
        true,
    );
    harness.runner.update(cx, |runner, cx| {
        runner.submit(request("apiframe-midjourney", Vec::new()), cx)
    });
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(states[0], JobState::Expired), "{states:?}");
    assert!(
        harness
            .history
            .grid_meta(&easel_core::providers::JobId::parse("mjgrid3").unwrap())
            .is_none()
    );
}

#[gpui::test]
fn a_slow_grid_download_outlives_the_poll_budget(cx: &mut TestAppContext) {
    let harness = open_with_stall(
        cx,
        "mj-slow",
        Arc::new(FakeTransport::scripted(vec![
            Ok(http(202, r#"{"jobId":"mjgrid9","status":"QUEUED"}"#)),
            Ok(http(200, r#"{"status":"QUEUED","progress":0}"#)),
            Ok(http(200, mj_completed("mjgrid9"))),
            Ok(http(200, grid_png(9))),
            Ok(http(200, grid_png(1))),
            Ok(http(200, grid_png(2))),
            Ok(http(200, grid_png(3))),
            Ok(http(200, grid_png(4))),
        ])),
        Some(Duration::from_secs(700)),
        true,
    );
    harness.runner.update(cx, |runner, cx| {
        runner.submit(request("apiframe-midjourney", Vec::new()), cx)
    });
    cx.run_until_parked();
    wait_for_polls(cx, 1);

    // The stall holds the first CDN fetch past the 600s poll budget; the
    // download phase owns its own deadline and the completed job must survive.
    cx.executor().advance_clock(Duration::from_secs(700));
    cx.run_until_parked();
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(states[0], JobState::Done { .. }), "{states:?}");
    assert!(harness.root.join("mjgrid9/result.png").is_file());
    assert!(
        harness
            .history
            .grid_meta(&easel_core::providers::JobId::parse("mjgrid9").unwrap())
            .is_some()
    );
}

#[gpui::test]
fn a_content_policy_failure_blocks_and_a_402_writes_no_row(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        "failed",
        vec![
            Ok(http(202, r#"{"jobId":"apjob3","status":"QUEUED"}"#)),
            Ok(http(
                200,
                r#"{"status":"FAILED","error":"Provider returned an error: content policy violation"}"#,
            )),
        ],
        false,
    );
    harness.runner.update(cx, |runner, cx| {
        runner.submit(request("apiframe-seedream-5-pro", Vec::new()), cx)
    });
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    match &states[0] {
        JobState::Failed {
            error: ProviderError::Blocked,
        } => {}
        other => panic!("{other:?}"),
    }
    let listed = harness.history.list().unwrap();
    assert_eq!(listed.len(), 1);
    assert!(matches!(listed[0].state, JobState::Failed { .. }));
    assert!(!harness.root.join("apjob3/result.bin").exists());
    assert!(!harness.root.join("apjob3/result.png").exists());

    let harness = open(
        cx,
        "out-of-credits",
        vec![Ok(http(402, "no credits"))],
        false,
    );
    harness.runner.update(cx, |runner, cx| {
        runner.submit(request("apiframe-flux-2-pro", Vec::new()), cx)
    });
    cx.run_until_parked();
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(
        matches!(
            states[0],
            JobState::Failed {
                error: ProviderError::OutOfCredits { .. }
            }
        ),
        "{states:?}"
    );
    assert!(harness.history.list().unwrap().is_empty());
}
