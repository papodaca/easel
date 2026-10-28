use std::sync::Arc;

use easel_core::error::ProviderError;
use easel_core::history::HistoryStore;
use easel_core::http::{HttpResponse, Method};
use easel_core::job::{JobRequest, JobState};
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
    vec![0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46, 0x49, 0x46]
}

fn picture_body(url: &str) -> String {
    format!(r#"{{"output":{{"choices":[{{"message":{{"content":[{{"image":"{url}"}}]}}}}]}}}}"#)
}

fn two_picture_body(first: &str, second: &str) -> String {
    format!(
        r#"{{"output":{{"choices":[{{"message":{{"content":[{{"image":"{first}"}},{{"image":"{second}"}}]}}}}]}}}}"#
    )
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
        Arc::new(MapKeys::default().insert(ProviderId::Qwen, "qwen-test-key"));
    let root = std::env::temp_dir().join(format!(
        "easel-qwen-{}-{}",
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
        model_id: "qwen-image-3.0-pro".into(),
        compiled_prompt: "a lighthouse".into(),
        params: std::collections::BTreeMap::new(),
        images: Vec::new(),
        box_snapshot: Vec::new(),
        created_at: "2026-10-24T00:00:00Z".into(),
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
    assert_eq!(
        seen[0].url,
        "https://maas.qwencloudapi.com/api/v1/services/aigc/multimodal-generation/generation"
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

const OSS_RESULT: &str = "https://dashscope-result-sz.oss-cn-shenzhen.aliyuncs.com/a.png";

#[gpui::test]
fn a_png_url_is_downloaded_without_a_poll(cx: &mut TestAppContext) {
    let bytes = png();
    let harness = open(
        cx,
        vec![
            Ok(http(200, picture_body(OSS_RESULT))),
            Ok(http(200, bytes.clone())),
        ],
    );
    submit(cx, &harness);
    let seen = harness.transport.seen();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[0].method, Method::Post);
    assert_eq!(seen[1].method, Method::Get);
    assert_eq!(seen[1].url, OSS_RESULT);
    assert_eq!(seen[1].auth, None);
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
    assert_eq!(meta["provider"], "qwen");
    assert_eq!(meta["model_id"], "qwen-image-3.0-pro");
    assert!(meta["poll_url"].is_null());
}

#[gpui::test]
fn two_image_urls_keep_the_first_and_fetch_it_once(cx: &mut TestAppContext) {
    let first = png();
    let harness = open(
        cx,
        vec![
            Ok(http(
                200,
                two_picture_body(
                    OSS_RESULT,
                    "https://dashscope-result-sz.oss-cn-shenzhen.aliyuncs.com/b.png",
                ),
            )),
            Ok(http(200, first.clone())),
        ],
    );
    submit(cx, &harness);
    let seen = harness.transport.seen();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[1].url, OSS_RESULT);
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_eq!(
        std::fs::read(harness.root.join(&id).join("result.png")).unwrap(),
        first
    );
}

#[gpui::test]
fn an_http_result_url_is_no_image_and_is_not_fetched(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![Ok(http(200, picture_body("http://example.com/a.png")))],
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
fn a_result_url_with_userinfo_is_no_image_and_is_not_fetched(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![Ok(http(
            200,
            picture_body("https://user:pass@example.com/a.png"),
        ))],
    );
    submit(cx, &harness);
    assert_one_post(&harness);
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(
        states[0],
        JobState::Failed {
            error: ProviderError::NoImage
        }
    ));
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_no_image(&harness.root, &id);
}

#[gpui::test]
fn a_loopback_result_url_is_no_image_and_is_not_fetched(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![Ok(http(200, picture_body("https://127.0.0.1/a.png")))],
    );
    submit(cx, &harness);
    assert_one_post(&harness);
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(
        states[0],
        JobState::Failed {
            error: ProviderError::NoImage
        }
    ));
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_no_image(&harness.root, &id);
}

#[gpui::test]
fn a_redirecting_picture_get_is_no_image(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![
            Ok(http(200, picture_body(OSS_RESULT))),
            Ok(http(
                302,
                r#"{"location":"https://elsewhere.example/a.png"}"#,
            )),
        ],
    );
    submit(cx, &harness);
    let seen = harness.transport.seen();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[1].url, OSS_RESULT);
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(
        states[0],
        JobState::Failed {
            error: ProviderError::NoImage
        }
    ));
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_no_image(&harness.root, &id);
}

#[gpui::test]
fn a_jpeg_picture_get_is_no_image(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![
            Ok(http(200, picture_body(OSS_RESULT))),
            Ok(http(200, jpeg())),
        ],
    );
    submit(cx, &harness);
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(
        states[0],
        JobState::Failed {
            error: ProviderError::NoImage
        }
    ));
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_no_image(&harness.root, &id);
}

#[gpui::test]
fn a_png_content_type_with_non_png_bytes_is_no_image(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![
            Ok(http(200, picture_body(OSS_RESULT))),
            Ok(http(200, b"sure looks like a picture")),
        ],
    );
    submit(cx, &harness);
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(
        states[0],
        JobState::Failed {
            error: ProviderError::NoImage
        }
    ));
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_no_image(&harness.root, &id);
}

#[gpui::test]
fn a_picture_url_with_a_query_is_requested_with_it(cx: &mut TestAppContext) {
    let url =
        "https://dashscope-result-sz.oss-cn-shenzhen.aliyuncs.com/a.png?Expires=170&Signature=abc";
    let harness = open(
        cx,
        vec![Ok(http(200, picture_body(url))), Ok(http(200, png()))],
    );
    submit(cx, &harness);
    let seen = harness.transport.seen();
    assert_eq!(seen.len(), 2);
    assert_eq!(seen[1].url, url);
    assert!(seen[1].redact_query);
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(states[0], JobState::Done { .. }), "{states:?}");
}

#[gpui::test]
fn a_moderated_body_with_a_picture_url_is_blocked_and_not_fetched(cx: &mut TestAppContext) {
    let body = format!(
        r#"{{"code":"DataInspectionFailed","message":"no","output":{{"choices":[{{"message":{{"content":[{{"image":"{OSS_RESULT}"}}]}}}}]}}}}"#
    );
    let harness = open(cx, vec![Ok(http(400, body))]);
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
    assert!(matches!(history_error(&harness), ProviderError::Blocked));
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_no_image(&harness.root, &id);
}

#[gpui::test]
fn a_lowercase_moderation_code_is_blocked(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![Ok(http(
            400,
            r#"{"code":"data_inspection_failed","message":"no"}"#,
        ))],
    );
    submit(cx, &harness);
    assert_one_post(&harness);
    assert!(matches!(history_error(&harness), ProviderError::Blocked));
}

#[gpui::test]
fn model_not_exist_is_model_not_found(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![Ok(http(
            400,
            r#"{"code":"InvalidApiKey.Expired","message":"Model not exist."}"#,
        ))],
    );
    submit(cx, &harness);
    assert_one_post(&harness);
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
fn model_not_found_code_writes_a_failed_row(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![Ok(http(
            404,
            r#"{"code":"model_not_found","message":"no"}"#,
        ))],
    );
    submit(cx, &harness);
    assert!(matches!(
        history_error(&harness),
        ProviderError::ModelNotFound
    ));
}

#[gpui::test]
fn http_401_is_auth_and_writes_a_failed_row(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![Ok(http(401, r#"{"code":"InvalidApiKey","message":"bad"}"#))],
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
    assert!(matches!(history_error(&harness), ProviderError::Auth));
    let id = harness.history.list().unwrap()[0].id.as_str().to_owned();
    assert_no_image(&harness.root, &id);
}

#[gpui::test]
fn http_429_is_rate_limited_with_no_row_and_no_retry(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![Ok(http(
            429,
            r#"{"code":"Throttling.RateQuota","message":"slow"}"#,
        ))],
    );
    submit(cx, &harness);
    assert_one_post(&harness);
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
fn other_http_failures_write_no_row(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![Ok(http(
            400,
            r#"{"code":"InvalidParameter","message":"bad size"}"#,
        ))],
    );
    submit(cx, &harness);
    assert_one_post(&harness);
    assert!(harness.history.list().unwrap().is_empty());
    let states = harness.runner.read_with(cx, |runner, _| runner.states());
    assert!(matches!(states[0], JobState::Failed { .. }), "{states:?}");
}

#[gpui::test]
fn dismiss_during_the_post_drops_a_late_200(cx: &mut TestAppContext) {
    let harness = open(
        cx,
        vec![
            Ok(http(200, picture_body(OSS_RESULT))),
            Ok(http(200, png())),
        ],
    );
    harness
        .runner
        .update(cx, |runner, cx| runner.submit(request(), cx));
    harness
        .runner
        .update(cx, |runner, cx| runner.dismiss(0, cx));
    cx.run_until_parked();
    assert_eq!(harness.transport.seen().len(), 1);
    assert!(harness.history.list().unwrap().is_empty());
}
