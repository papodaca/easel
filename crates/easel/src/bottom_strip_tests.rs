use std::sync::Arc;

use easel_core::geometry::Rect;
use easel_core::history::{Completion, HistoryStore};
use easel_core::job::{EncodedImage, JobRequest};
use easel_core::providers::{CostQuote, JobId, Submitted};
use easel_core::scene::BoxKind;
use gpui::{MouseButton, MouseDownEvent, Modifiers, TestAppContext, VisualTestContext, prelude::*};

use crate::app_state::AppState;
use crate::bottom_strip::BottomStrip;

fn png() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/pixel.png"
    ))
    .unwrap()
}

fn temp_dir(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "easel-strip-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&path);
    path
}

fn request() -> JobRequest {
    JobRequest {
        model_id: "flux-3-image".into(),
        compiled_prompt: "a red square".into(),
        params: std::collections::BTreeMap::new(),
        images: vec![EncodedImage {
            media_type: "image/png".into(),
            bytes: Vec::new(),
            width: 8,
            height: 8,
            content_hash: "abc".into(),
        }],
        box_snapshot: Vec::new(),
        created_at: "2026-10-15T00:00:00Z".into(),
    }
}

fn submitted(id: &str, cost: f64) -> Submitted {
    Submitted {
        job_id: JobId::parse(id).unwrap(),
        poll_url: format!("https://api.bfl.ai/v1/get_result?id={id}")
            .parse()
            .unwrap(),
        quote: Some(CostQuote {
            cost,
            input_mp: 1.0,
            output_mp: 1.0,
        }),
    }
}

fn seed_done(history: &HistoryStore, id: &str) {
    let job = JobId::parse(id).unwrap();
    history.begin(&job, &request(), &submitted(id, 1.25)).unwrap();
    history
        .complete(
            &job,
            &png(),
            "image/png",
            &png(),
            &Completion {
                cost: Some(1.25),
                prompt: None,
                duration: None,
            },
        )
        .unwrap();
}

struct Host {
    strip: gpui::Entity<BottomStrip>,
}

impl gpui::Render for Host {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl gpui::IntoElement {
        gpui::div().size_full().child(self.strip.clone())
    }
}

fn open(cx: &mut TestAppContext) -> (gpui::Entity<AppState>, gpui::Entity<BottomStrip>, Arc<HistoryStore>, VisualTestContext) {
    let root = temp_dir("open");
    let history = Arc::new(HistoryStore::open(root.join("history")).unwrap());
    let window = cx.update({
        let history = Arc::clone(&history);
        move |cx| {
            cx.open_window(gpui::WindowOptions::default(), |_window, cx| {
                let state = cx.new(|_| AppState::new());
                let strip = cx.new(|cx| BottomStrip::new(state.clone(), None, history, cx));
                cx.new(|_| Host { strip })
            })
            .unwrap()
        }
    });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let strip = window
        .root(&mut visual)
        .unwrap()
        .read_with(&visual, |host, _| host.strip.clone());
    let state = strip.read_with(&visual, |strip, _| strip.app_state());
    (state, strip, history, visual)
}

#[gpui::test]
fn promote_keeps_the_box_count(cx: &mut TestAppContext) {
    let (state, strip, history, mut cx) = open(cx);
    seed_done(&history, "job1");
    state.update(&mut cx, |app, cx| {
        app.scene.add_box(
            Rect {
                x: 10,
                y: 10,
                w: 20,
                h: 20,
            },
            BoxKind::Layout,
        );
        cx.notify();
    });
    strip.update(&mut cx, |strip, cx| {
        strip.refresh(cx);
        strip.select(0, cx);
        strip.press("Promote", cx);
    });
    let (boxes, has_image) = state.read_with(&cx, |app, _| (app.scene.boxes.len(), app.image.is_some()));
    assert_eq!(boxes, 1);
    assert!(has_image);
}

#[gpui::test]
fn discard_confirms_before_removing_the_entry(cx: &mut TestAppContext) {
    let (_state, strip, history, mut cx) = open(cx);
    seed_done(&history, "job1");
    strip.update(&mut cx, |strip, cx| {
        strip.refresh(cx);
        strip.select(0, cx);
        strip.press("Discard", cx);
    });
    assert!(strip.read_with(&cx, |strip, _| strip.confirm_visible()));
    strip.update(&mut cx, |strip, cx| strip.press("Cancel", cx));
    assert_eq!(history.list().unwrap().len(), 1);
    assert!(!strip.read_with(&cx, |strip, _| strip.confirm_visible()));
    strip.update(&mut cx, |strip, cx| {
        strip.press("Discard", cx);
        strip.press("Confirm", cx);
    });
    assert!(history.list().unwrap().is_empty());
    assert!(strip.read_with(&cx, |strip, _| strip.lines().is_empty()));
}

#[gpui::test]
fn click_shows_actions_and_double_click_promotes(cx: &mut TestAppContext) {
    let (state, strip, history, mut cx) = open(cx);
    seed_done(&history, "job1");
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    cx.run_until_parked();
    let point = strip.read_with(&cx, |strip, _| strip.entry_point(0));
    cx.simulate_click(point, Modifiers::default());
    let actions = strip.read_with(&cx, |strip, _| strip.actions());
    assert_eq!(
        actions,
        vec!["Promote", "Save As", "Compare", "Discard"]
    );
    cx.simulate_event(MouseDownEvent {
        button: MouseButton::Left,
        position: point,
        modifiers: Modifiers::default(),
        click_count: 2,
        first_mouse: false,
    });
    assert!(state.read_with(&cx, |app, _| app.image.is_some()));
}

#[gpui::test]
fn a_failed_entry_strips_controls_and_truncates(cx: &mut TestAppContext) {
    let (_state, strip, history, mut cx) = open(cx);
    let job = JobId::parse("job1").unwrap();
    history.begin(&job, &request(), &submitted("job1", 1.0)).unwrap();
    let mut message = "\u{202E}\u{0001}".to_string();
    message.push_str(&"x".repeat(10 * 1024));
    history
        .fail(
            &job,
            &easel_core::error::ProviderError::Protocol(message),
        )
        .unwrap();
    strip.update(&mut cx, |strip, cx| {
        strip.refresh(cx);
        strip.select(0, cx);
    });
    let (line, detail) = strip.read_with(&cx, |strip, _| (strip.lines()[0].clone(), strip.detail()));
    assert!(line.len() <= 2 * 1024, "{}", line.len());
    assert!(detail.len() <= 16 * 1024, "{}", detail.len());
    assert!(detail.len() > line.len());
    assert!(!line.contains('\u{202E}') && !detail.contains('\u{202E}'));
    assert!(!line.contains('\u{0001}') && !detail.contains('\u{0001}'));
}

#[gpui::test]
fn compare_shows_grounding_text_and_keeps_a_wide_ratio_on_nano_banana(
    cx: &mut TestAppContext,
) {
    use serde_json::json;

    let (state, strip, history, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| app.set_model("gemini-3-pro-image", cx));
    let mut job = request();
    job.model_id = "gemini-nano-banana-2.1".into();
    job.compiled_prompt = "a cat".into();
    job.params.insert("aspect_ratio".into(), json!("1:8"));
    job.params.insert("resolution".into(), json!("1K"));
    let suggestion = "<div class=\"search\">cats</div>";
    history
        .complete_inline(
            &JobId::parse("gcmp").unwrap(),
            &job,
            "interaction",
            suggestion,
            &png(),
            "image/png",
            &png(),
        )
        .unwrap();
    strip.update(&mut cx, |strip, cx| {
        strip.refresh(cx);
        strip.select(0, cx);
        strip.press("Compare", cx);
    });
    let (model, ratio, inspection) = state.read_with(&cx, |app, _| {
        (
            app.model_id.clone(),
            app.values.get("aspect_ratio").cloned(),
            app.inspection.clone(),
        )
    });
    assert_eq!(model, "gemini-nano-banana-2.1");
    assert_eq!(ratio, Some(json!("1:8")));
    let inspection = inspection.expect("compare");
    assert_eq!(inspection.prompt, "a cat");
    assert_eq!(inspection.suggestion, suggestion);
    assert!(
        inspection
            .rows
            .iter()
            .any(|(key, _)| key == "aspect_ratio")
    );
    state.update(&mut cx, |app, cx| {
        let params = app.values.clone();
        app.set_model("gemini-3-pro-image", cx);
        app.values = app.reconcile_values(params);
    });
    assert_ne!(
        state.read_with(&cx, |app, _| app.values.get("aspect_ratio").cloned()),
        Some(json!("1:8"))
    );
}

fn http(status: u16, body: impl AsRef<[u8]>) -> easel_core::http::HttpResponse {
    easel_core::http::HttpResponse {
        status,
        headers: Vec::new(),
        body: body.as_ref().to_vec(),
    }
}

fn submit_body(id: &str) -> String {
    format!(
        r#"{{"id":"{id}","polling_url":"https://api.bfl.ai/v1/get_result?id={id}","cost":1.25,"input_mp":1.0,"output_mp":1.0}}"#
    )
}

struct RunnerHost {
    strip: gpui::Entity<BottomStrip>,
}

impl gpui::Render for RunnerHost {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        gpui::div().size_full().child(self.strip.clone())
    }
}

fn open_runner(
    cx: &mut TestAppContext,
    name: &str,
    script: Vec<Result<easel_core::http::HttpResponse, crate::net::TransportError>>,
) -> (
    gpui::Entity<BottomStrip>,
    gpui::Entity<crate::jobs::JobRunner>,
    VisualTestContext,
) {
    let root = temp_dir(name);
    let history = Arc::new(HistoryStore::open(root.join("history")).unwrap());
    let transport: Arc<dyn crate::net::HttpTransport> =
        Arc::new(crate::net::FakeTransport::scripted(script));
    let keys = Arc::new(crate::settings::SharedKeys::default());
    keys.set_session(easel_core::model_spec::ProviderId::Bfl, "test-key");
    let runner = cx.update({
        let history = Arc::clone(&history);
        let keys = Arc::clone(&keys);
        move |cx| {
            cx.new(|_| {
                crate::jobs::JobRunner::new(
                    easel_core::providers::resolve_model,
                    transport,
                    Arc::new(crate::settings::Resolver(keys)),
                    history,
                )
            })
        }
    });
    let window = cx.update({
        let history = Arc::clone(&history);
        let runner = runner.clone();
        move |cx| {
            cx.open_window(gpui::WindowOptions::default(), |_window, cx| {
                let state = cx.new(|_| AppState::new());
                let strip = cx.new(|cx| BottomStrip::new(state, Some(runner), history, cx));
                cx.new(|_| RunnerHost { strip })
            })
            .unwrap()
        }
    });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let strip = window
        .root(&mut visual)
        .unwrap()
        .read_with(&visual, |host, _| host.strip.clone());
    (strip, runner, visual)
}

#[gpui::test]
fn in_flight_entries_show_progress(cx: &mut TestAppContext) {
    let (strip, runner, mut cx) = open_runner(
        cx,
        "progress",
        vec![
            Ok(http(200, submit_body("job1"))),
            Ok(http(200, r#"{"status":"Generating","progress":0.4}"#)),
        ],
    );
    runner.update(&mut cx, |runner, cx| runner.submit(request(), cx));
    cx.run_until_parked();
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    let line = strip.read_with(&cx, |strip, _| strip.lines()[0].clone());
    assert!(line.contains("Generating"), "{line}");
    assert!(line.contains("40"), "{line}");
    strip.update(&mut cx, |strip, cx| strip.select(0, cx));
    let actions = strip.read_with(&cx, |strip, _| strip.actions());
    assert_eq!(actions, vec!["Dismiss".to_string()]);
}

#[gpui::test]
fn the_strip_replaces_the_quote_with_the_settled_cost(cx: &mut TestAppContext) {
    let ready = r#"{"status":"Ready","result":{"sample":"https://delivery.bfl.ai/img.png"},"cost":4.5}"#;
    let (strip, runner, mut cx) = open_runner(
        cx,
        "cost",
        vec![
            Ok(http(200, submit_body("job1"))),
            Ok(http(200, r#"{"status":"Pending"}"#)),
            Ok(http(200, ready)),
            Ok(http(200, png())),
        ],
    );
    runner.update(&mut cx, |runner, cx| runner.submit(request(), cx));
    cx.run_until_parked();
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    let quoted = strip.read_with(&cx, |strip, _| strip.costs());
    assert!((quoted[0].unwrap() - 1.25).abs() < 1e-9, "{quoted:?}");
    cx.executor().advance_clock(std::time::Duration::from_secs(1));
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(1500));
    cx.run_until_parked();
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    let settled = strip.read_with(&cx, |strip, _| strip.costs());
    assert!((settled[0].unwrap() - 4.5).abs() < 1e-9, "{settled:?}");
}

#[gpui::test]
fn promote_replaces_the_source_and_leaves_the_other_tile(cx: &mut TestAppContext) {
    let state = cx.new(|_| AppState::new());
    let bytes = png();
    state.update(cx, |app, cx| {
        let source = crate::image_load::decode_image(&bytes, "source").unwrap();
        let mut extra = crate::image_load::decode_image(&bytes, "extra").unwrap();
        extra.size = easel_core::geometry::Size { w: 40, h: 40 };
        app.set_image(Arc::new(source), cx);
        app.add_extra(Arc::new(extra), cx).unwrap();
        let src = Rect {
            x: 10,
            y: 10,
            w: 20,
            h: 20,
        };
        let tgt = Rect {
            x: 40,
            y: 40,
            w: 20,
            h: 20,
        };
        let first = app.scene.add_box(src, BoxKind::Move);
        app.scene
            .update_box(&first, |element| element.tgt = Some(tgt));
        let second = app.scene.add_box_on(src, BoxKind::Move, 1);
        app.scene
            .update_box(&second, |element| element.tgt = Some(tgt));
        crate::jobs::promote(app, &bytes, cx).unwrap();
    });
    let (src0, src1, extra) = state.read_with(cx, |app, _| {
        (
            app.scene.boxes[0].src.unwrap(),
            app.scene.boxes[1].src.unwrap(),
            app.extras[0].size,
        )
    });
    assert_eq!(
        src0,
        Rect {
            x: 40,
            y: 40,
            w: 20,
            h: 20,
        }
    );
    assert_eq!(
        src1,
        Rect {
            x: 10,
            y: 10,
            w: 20,
            h: 20,
        }
    );
    assert_eq!(extra, easel_core::geometry::Size { w: 40, h: 40 });
}
