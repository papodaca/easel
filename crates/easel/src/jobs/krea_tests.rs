use std::sync::Arc;
use std::time::Duration;

use easel_core::error::ProviderError;
use easel_core::history::HistoryStore;
use easel_core::http::{HttpResponse, Method};
use easel_core::job::{EncodedImage, JobRequest, JobState};
use easel_core::model_spec::ProviderId;
use easel_core::providers::Provider;
use easel_core::providers::bfl::flux3::Flux3;
use easel_core::providers::krea::Krea;
use easel_core::providers::krea::krea3::Krea3;
use gpui::{TestAppContext, prelude::*};

use crate::jobs::JobRunner;
use crate::net::{FakeTransport, HttpTransport, KeyResolver, MapKeys, TransportError};

fn http(status: u16, body: &str) -> HttpResponse {
    HttpResponse {
        status,
        headers: Vec::new(),
        body: body.as_bytes().to_vec(),
    }
}

fn png() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/pixel.png"
    ))
    .unwrap()
}

fn image() -> EncodedImage {
    EncodedImage {
        media_type: "image/png".into(),
        bytes: png(),
        width: 16,
        height: 16,
        content_hash: "same-hash".into(),
    }
}

fn bare_request() -> JobRequest {
    let mut request = request_with_image();
    request.images.clear();
    request
}

fn request_with_image() -> JobRequest {
    JobRequest {
        model_id: "krea".into(),
        compiled_prompt: "a red square".into(),
        params: std::collections::BTreeMap::new(),
        images: vec![image()],
        box_snapshot: Vec::new(),
        created_at: "2026-10-16T00:00:00Z".into(),
    }
}

struct Harness<P> {
    runner: gpui::Entity<JobRunner<P>>,
    transport: Arc<FakeTransport>,
}

fn open<P: Provider + 'static>(
    cx: &mut TestAppContext,
    provider: P,
    script: Vec<Result<HttpResponse, TransportError>>,
) -> Harness<P> {
    let transport = Arc::new(FakeTransport::scripted(script));
    let keys: Arc<dyn KeyResolver> =
        Arc::new(MapKeys::default().insert(ProviderId::Krea, "krea-test-key"));
    let root = std::env::temp_dir().join(format!(
        "easel-krea-{}-{}",
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
        move |cx| cx.new(|_| JobRunner::new(provider, transport, keys, history))
    });
    Harness { runner, transport }
}

fn completed_script() -> Vec<Result<HttpResponse, TransportError>> {
    vec![
        Ok(http(
            200,
            r#"{"image_url":"https://api.krea.ai/assets/fixed"}"#,
        )),
        Ok(http(200, r#"{"job_id":"job-1","status":"queued"}"#)),
        Ok(http(200, r#"{"status":"queued","progress":0.1}"#)),
        Ok(http(200, r#"{"status":"processing","progress":0.4}"#)),
        Ok(http(200, r#"{"status":"sampling","progress":0.8}"#)),
        Ok(http(
            200,
            r#"{"status":"completed","result":{"urls":["https://cdn.krea.ai/r.png"]}}"#,
        )),
        Ok(HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: png(),
        }),
    ]
}

#[gpui::test]
fn upload_then_poll_then_a_cached_second_job(cx: &mut TestAppContext) {
    let mut script = completed_script();
    script.extend([
        Ok(http(200, r#"{"job_id":"job-2","status":"queued"}"#)),
        Ok(http(
            200,
            r#"{"status":"completed","result":{"urls":["https://cdn.krea.ai/r.png"]}}"#,
        )),
        Ok(HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: png(),
        }),
    ]);
    let harness = open(cx, Krea, script);
    harness
        .runner
        .update(cx, |runner, cx| runner.submit(request_with_image(), cx));
    cx.run_until_parked();
    assert_eq!(harness.transport.seen().len(), 2);
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    assert_eq!(harness.transport.seen().len(), 3);
    cx.executor().advance_clock(Duration::from_secs(3));
    cx.run_until_parked();
    assert_eq!(harness.transport.seen().len(), 4);
    cx.executor().advance_clock(Duration::from_millis(4500));
    cx.run_until_parked();
    assert_eq!(harness.transport.seen().len(), 5);
    cx.executor().advance_clock(Duration::from_secs(5));
    cx.run_until_parked();
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(states[0], JobState::Done { .. }), "{states:?}");
    let seen = harness.transport.seen();
    assert_eq!(seen[0].method, Method::Post);
    assert!(seen[0].url.ends_with("/assets"));
    assert_eq!(seen.last().unwrap().auth, None);
    let uploads = seen
        .iter()
        .filter(|request| request.url.ends_with("/assets"))
        .count();
    harness
        .runner
        .update(cx, |runner, cx| runner.submit(request_with_image(), cx));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(5));
    cx.run_until_parked();
    let seen = harness.transport.seen();
    assert_eq!(
        seen.iter()
            .filter(|request| request.url.ends_with("/assets"))
            .count(),
        uploads
    );
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(
        states
            .iter()
            .any(|state| matches!(state, JobState::Done { .. })),
        "{states:?}"
    );
}

#[gpui::test]
fn dismiss_during_processing_deletes_once(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        Krea,
        vec![
            Ok(http(
                200,
                r#"{"image_url":"https://api.krea.ai/assets/fixed"}"#,
            )),
            Ok(http(200, r#"{"job_id":"job-1","status":"queued"}"#)),
            Ok(http(200, r#"{"status":"queued","progress":0.1}"#)),
            Ok(http(200, r#"{"status":"processing","progress":0.4}"#)),
            Ok(http(404, "missing")),
        ],
    );
    harness
        .runner
        .update(cx, |runner, cx| runner.submit(request_with_image(), cx));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(3));
    cx.run_until_parked();
    harness
        .runner
        .update(cx, |runner, cx| runner.dismiss(0, cx));
    let deletes = harness
        .transport
        .seen()
        .iter()
        .filter(|request| request.method == Method::Delete)
        .count();
    assert_eq!(deletes, 1);
    assert!(harness.transport.seen().iter().any(|request| {
        request.method == Method::Delete && request.url == "https://api.krea.ai/jobs/job-1"
    }));
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(states[0], JobState::Dismissed), "{states:?}");
}

#[gpui::test]
fn dismiss_after_completed_sends_no_delete(cx: &mut TestAppContext) {
    let harness = open(cx, Krea, completed_script());
    harness
        .runner
        .update(cx, |runner, cx| runner.submit(request_with_image(), cx));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(2));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(3));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_millis(4500));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(5));
    cx.run_until_parked();
    let before = harness.transport.seen().len();
    harness
        .runner
        .update(cx, |runner, cx| runner.dismiss(0, cx));
    assert_eq!(harness.transport.seen().len(), before);
    assert!(
        harness
            .transport
            .seen()
            .iter()
            .all(|request| request.method != Method::Delete)
    );
}

#[gpui::test]
fn krea_deadline_is_ten_minutes(cx: &mut TestAppContext) {
    let mut script = vec![
        Ok(http(
            200,
            r#"{"image_url":"https://api.krea.ai/assets/fixed"}"#,
        )),
        Ok(http(200, r#"{"job_id":"job-1","status":"queued"}"#)),
    ];
    script.extend(std::iter::repeat_n(
        Ok(http(200, r#"{"status":"backlogged","progress":0}"#)),
        80,
    ));
    let harness = open(cx, Krea, script);
    harness
        .runner
        .update(cx, |runner, cx| runner.submit(request_with_image(), cx));
    cx.run_until_parked();
    cx.executor().advance_clock(Duration::from_secs(5 * 60));
    cx.run_until_parked();
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(states[0], JobState::Polling { .. }), "{states:?}");
    cx.executor().advance_clock(Duration::from_secs(5 * 60));
    cx.run_until_parked();
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    let JobState::Failed { error } = &states[0] else {
        panic!("{states:?}");
    };
    assert!(
        error.to_string().to_ascii_lowercase().contains("timed out"),
        "{error}"
    );
    let _ = Flux3.poll_policy();
}

#[gpui::test]
fn a_beta_401_is_access_when_the_key_works_elsewhere(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        Krea3,
        vec![Ok(http(401, "no")), Ok(http(404, "missing"))],
    );
    harness
        .runner
        .update(cx, |runner, cx| runner.submit(bare_request(), cx));
    cx.run_until_parked();
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    match &states[0] {
        JobState::Failed {
            error: ProviderError::Protocol(text),
        } => assert!(text.contains("beta"), "{text}"),
        other => panic!("{other:?}"),
    }
    assert!(
        harness
            .transport
            .seen()
            .iter()
            .any(|request| request.url.starts_with("https://api.krea.ai/jobs/"))
    );

    let harness = open(cx, Krea3, vec![Ok(http(401, "no")), Ok(http(401, "no"))]);
    harness
        .runner
        .update(cx, |runner, cx| runner.submit(bare_request(), cx));
    cx.run_until_parked();
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
    let text = format!("{states:?}");
    assert!(!text.contains("beta"), "{text}");
}
