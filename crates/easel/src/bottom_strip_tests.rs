use std::sync::Arc;

use easel_core::geometry::Rect;
use easel_core::history::{Completion, HistoryStore};
use easel_core::job::{EncodedImage, JobRequest};
use easel_core::providers::{CostQuote, JobId, Submitted};
use easel_core::scene::BoxKind;
use gpui::{
    Focusable as _, MouseButton, MouseDownEvent, Modifiers, TestAppContext, VisualTestContext,
    WindowOptions, prelude::*, px,
};

use crate::app_state::AppState;
use crate::bottom_strip::BottomStrip;
use crate::root::Root;

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
    history.begin(&job, &request(), &submitted(id, 1.25), None).unwrap();
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

fn draw(cx: &mut VisualTestContext) {
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
}

fn down(button: MouseButton, position: gpui::Point<gpui::Pixels>, click_count: usize) -> MouseDownEvent {
    MouseDownEvent {
        button,
        position,
        modifiers: Modifiers::default(),
        click_count,
        first_mouse: false,
    }
}

fn state_has_image(root: &gpui::Entity<Root>, cx: &VisualTestContext) -> bool {
    root.read_with(cx, |root, cx| root.app_state.read(cx).image.is_some())
}

fn open_root(
    cx: &mut TestAppContext,
) -> (
    gpui::Entity<Root>,
    gpui::Entity<BottomStrip>,
    Arc<HistoryStore>,
    VisualTestContext,
) {
    let root_dir = temp_dir("root");
    let history = Arc::new(HistoryStore::open(root_dir.join("history")).unwrap());
    let window = cx.update({
        let history = Arc::clone(&history);
        move |cx| {
            crate::actions::bind_keys(cx);
            cx.open_window(WindowOptions::default(), |window, cx| {
                let root = cx.new(|cx| Root::test_open(history, cx));
                root.focus_handle(cx).focus(window, cx);
                root
            })
            .unwrap()
        }
    });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let root = window.root(&mut visual).unwrap();
    let strip = root.read_with(&visual, |root, _| root.strip());
    (root, strip, history, visual)
}

fn open_modal(cx: &mut VisualTestContext, strip: &gpui::Entity<BottomStrip>) {
    draw(cx);
    let point = strip.read_with(cx, |strip, _| strip.entry_point(0));
    cx.simulate_event(down(MouseButton::Left, point, 1));
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(400));
    cx.run_until_parked();
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
    let (root, strip, history, mut cx) = open_root(cx);
    seed_done(&history, "job1");
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    draw(&mut cx);
    let point = strip.read_with(&cx, |strip, _| strip.entry_point(0));
    cx.simulate_event(down(MouseButton::Left, point, 1));
    draw(&mut cx);
    assert!(root.read_with(&cx, |root, cx| root.app_state.read(cx).history_item.is_none()));
    cx.simulate_event(down(MouseButton::Left, point, 1));
    assert!(state_has_image(&root, &cx));
    assert!(root.read_with(&cx, |root, cx| root.app_state.read(cx).history_item.is_none()));
}

#[gpui::test]
fn a_failed_entry_strips_controls_and_truncates(cx: &mut TestAppContext) {
    let (_state, strip, history, mut cx) = open(cx);
    let job = JobId::parse("job1").unwrap();
    history.begin(&job, &request(), &submitted("job1", 1.0), None).unwrap();
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
        None)
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
    assert_eq!(
        strip.read_with(&cx, |strip, _| strip.spinners()),
        vec![true]
    );
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
    assert_eq!(
        strip.read_with(&cx, |strip, _| strip.spinners()),
        vec![false]
    );
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

#[gpui::test]
fn a_single_click_opens_the_modal_after_the_interval(cx: &mut TestAppContext) {
    let (_root, strip, history, mut cx) = open_root(cx);
    seed_done(&history, "job1");
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    open_modal(&mut cx, &strip);
    draw(&mut cx);
    let dialog = strip.update(&mut cx, |strip, cx| strip.dialog(cx).expect("modal"));
    assert!(dialog.image.is_some());
    assert!(dialog.pixel_size.is_some());
    assert_eq!(dialog.cost, Some(1.25));
    assert_eq!(dialog.model, "flux-3-image");
    assert_eq!(dialog.prompt, "a red square");
    assert_eq!(
        dialog.actions,
        vec!["Promote", "Save As", "Reveal", "Compare", "Discard"]
    );
    assert!(cx.debug_bounds("history-action-Promote").is_some());
    assert!(cx.debug_bounds("strip-Promote").is_none());
}

#[gpui::test]
fn a_right_click_opens_the_modal_without_promoting(cx: &mut TestAppContext) {
    let (root, strip, history, mut cx) = open_root(cx);
    seed_done(&history, "job1");
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    draw(&mut cx);
    let point = strip.read_with(&cx, |strip, _| strip.entry_point(0));
    cx.simulate_event(down(MouseButton::Right, point, 1));
    assert!(root.read_with(&cx, |root, cx| root.app_state.read(cx).history_item.is_some()));
    assert!(!state_has_image(&root, &cx));
}

#[gpui::test]
fn escape_closes_settings_before_the_history_modal(cx: &mut TestAppContext) {
    let (root, strip, history, mut cx) = open_root(cx);
    seed_done(&history, "job1");
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    open_modal(&mut cx, &strip);
    cx.update(|window, cx| {
        window.dispatch_action(Box::new(crate::actions::Settings), cx);
    });
    draw(&mut cx);
    cx.simulate_click(gpui::point(px(8.), px(8.)), Modifiers::default());
    assert!(root.read_with(&cx, |root, cx| {
        root.app_state.read(cx).settings_open && root.app_state.read(cx).history_item.is_some()
    }));
    cx.update(|window, cx| {
        window.dispatch_action(Box::new(crate::actions::CloseSettings), cx);
    });
    assert!(!root.read_with(&cx, |root, cx| root.app_state.read(cx).settings_open));
    assert!(root.read_with(&cx, |root, cx| root.app_state.read(cx).history_item.is_some()));
    cx.update(|window, cx| {
        window.dispatch_action(Box::new(crate::actions::CloseSettings), cx);
    });
    assert!(root.read_with(&cx, |root, cx| root.app_state.read(cx).history_item.is_none()));

    open_modal(&mut cx, &strip);
    draw(&mut cx);
    cx.simulate_click(gpui::point(px(8.), px(8.)), Modifiers::default());
    assert!(root.read_with(&cx, |root, cx| root.app_state.read(cx).history_item.is_some()));
    cx.simulate_click(
        gpui::point(px(8.), px(crate::theme::TITLE_BAR_HEIGHT + 12.)),
        Modifiers::default(),
    );
    assert!(root.read_with(&cx, |root, cx| root.app_state.read(cx).history_item.is_none()));
}

#[gpui::test]
fn modal_actions_follow_the_opened_job(cx: &mut TestAppContext) {
    let (strip, runner, mut cx) = open_runner(cx, "retarget", Vec::new());
    let history = strip.read_with(&cx, |strip, _| strip.history());
    seed_done(&history, "job1");
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    open_modal(&mut cx, &strip);
    let opened = strip.read_with(&cx, |strip, cx| {
        strip.app_state().read(cx).history_item.clone()
    });
    runner.update(&mut cx, |runner, cx| runner.submit(request(), cx));
    cx.run_until_parked();
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    let still = strip.read_with(&cx, |strip, cx| {
        strip.app_state().read(cx).history_item.clone()
    });
    assert_eq!(still.as_ref().and_then(|item| item.id.as_ref().map(|id| id.as_str().to_string())), Some("job1".into()));
    assert_eq!(opened.unwrap().id.unwrap().as_str(), "job1");
    strip.update(&mut cx, |strip, cx| strip.press("Promote", cx));
    assert!(strip.read_with(&cx, |strip, cx| strip.app_state().read(cx).image.is_some()));
    assert!(strip.read_with(&cx, |strip, cx| {
        strip.app_state().read(cx).history_item.is_none()
    }));
}

#[gpui::test]
fn a_failed_row_opens_without_an_image_or_reveal(cx: &mut TestAppContext) {
    let (_state, strip, history, mut cx) = open(cx);
    let job = JobId::parse("job1").unwrap();
    history
        .begin(&job, &request(), &submitted("job1", 1.0), None)
        .unwrap();
    history
        .fail(
            &job,
            &easel_core::error::ProviderError::Protocol("provider said no".into()),
        )
        .unwrap();
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    open_modal(&mut cx, &strip);
    let dialog = strip.update(&mut cx, |strip, cx| strip.dialog(cx).expect("modal"));
    assert!(dialog.image.is_none());
    assert_eq!(dialog.actions, vec!["Discard".to_string()]);
    assert!(dialog.line.contains("provider said no"));
}

#[gpui::test]
fn an_unsupported_result_keeps_the_failed_line_and_an_empty_image(cx: &mut TestAppContext) {
    let (_state, strip, history, mut cx) = open(cx);
    let job = JobId::parse("gifjob").unwrap();
    history
        .begin(&job, &request(), &submitted("gifjob", 1.0), None)
        .unwrap();
    history
        .complete(
            &job,
            b"GIF89a-not-really",
            "image/png",
            b"t",
            &Completion::default(),
        )
        .unwrap();
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    open_modal(&mut cx, &strip);
    let dialog = strip.update(&mut cx, |strip, cx| strip.dialog(cx).expect("modal"));
    assert!(dialog.image.is_none());
    assert!(dialog.line.contains("unsupported"));
    assert!(dialog.actions.contains(&"Reveal".to_string()));
}

#[gpui::test]
fn an_in_flight_row_opens_with_dismiss_only(cx: &mut TestAppContext) {
    let (strip, runner, mut cx) = open_runner(cx, "inflight-modal", Vec::new());
    runner.update(&mut cx, |runner, cx| runner.submit(request(), cx));
    strip.update(&mut cx, |strip, cx| {
        strip.refresh(cx);
        strip.show(0, cx);
    });
    let dialog = strip.update(&mut cx, |strip, cx| strip.dialog(cx).expect("modal"));
    assert_eq!(dialog.actions, vec!["Dismiss".to_string()]);
    assert!(dialog.image.is_none());
    assert!(dialog.spinning);
}

#[gpui::test]
fn a_queued_dismiss_closes_the_modal(cx: &mut TestAppContext) {
    let (strip, runner, mut cx) = open_runner(cx, "queued-dismiss", Vec::new());
    runner.update(&mut cx, |runner, cx| runner.submit(request(), cx));
    strip.update(&mut cx, |strip, cx| {
        strip.refresh(cx);
        strip.show(0, cx);
        strip.press("Dismiss", cx);
    });
    assert!(strip.read_with(&cx, |strip, cx| {
        strip.app_state().read(cx).history_item.is_none()
    }));
}

#[gpui::test]
fn dismiss_that_leaves_a_history_row_keeps_the_modal(cx: &mut TestAppContext) {
    let (strip, runner, mut cx) = open_runner(
        cx,
        "dismiss-keeps",
        vec![Ok(http(200, submit_body("kept")))],
    );
    runner.update(&mut cx, |runner, cx| runner.submit(request(), cx));
    cx.run_until_parked();
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    open_modal(&mut cx, &strip);
    let before = strip.read_with(&cx, |strip, cx| {
        strip.app_state().read(cx).history_item.clone().unwrap().id
    });
    strip.update(&mut cx, |strip, cx| strip.press("Dismiss", cx));
    let after = strip.read_with(&cx, |strip, cx| {
        strip.app_state().read(cx).history_item.clone().and_then(|item| item.id)
    });
    assert_eq!(before, after);
    assert!(after.is_some());
    let history = strip.read_with(&cx, |strip, _| strip.history());
    assert_eq!(history.list().unwrap().len(), 1);
}

#[gpui::test]
fn discard_confirm_removes_the_entry_and_cancel_leaves_the_modal(cx: &mut TestAppContext) {
    let (_root, strip, history, mut cx) = open_root(cx);
    seed_done(&history, "job1");
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    open_modal(&mut cx, &strip);
    strip.update(&mut cx, |strip, cx| strip.press("Discard", cx));
    assert!(strip.read_with(&cx, |strip, _| strip.confirm_visible()));
    strip.update(&mut cx, |strip, cx| strip.press("Cancel", cx));
    assert_eq!(history.list().unwrap().len(), 1);
    assert!(strip.read_with(&cx, |strip, cx| strip.app_state().read(cx).history_item.is_some()));
    strip.update(&mut cx, |strip, cx| {
        strip.press("Discard", cx);
        strip.press("Confirm", cx);
    });
    assert!(history.list().unwrap().is_empty());
    assert!(strip.read_with(&cx, |strip, cx| strip.app_state().read(cx).history_item.is_none()));
}

#[gpui::test]
fn promote_and_compare_close_the_modal_and_save_as_does_not(cx: &mut TestAppContext) {
    let (_root, strip, history, mut cx) = open_root(cx);
    seed_done(&history, "job1");
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    open_modal(&mut cx, &strip);
    strip.update(&mut cx, |strip, cx| strip.press("Save As", cx));
    assert!(strip.read_with(&cx, |strip, cx| strip.app_state().read(cx).history_item.is_some()));
    strip.update(&mut cx, |strip, cx| strip.press("Compare", cx));
    assert!(strip.read_with(&cx, |strip, cx| {
        strip.app_state().read(cx).inspection.is_some()
            && strip.app_state().read(cx).history_item.is_none()
    }));

    open_modal(&mut cx, &strip);
    strip.update(&mut cx, |strip, cx| strip.press("Promote", cx));
    assert!(strip.read_with(&cx, |strip, cx| {
        strip.app_state().read(cx).image.is_some()
            && strip.app_state().read(cx).history_item.is_none()
    }));
}

#[gpui::test]
fn the_modal_shows_the_typed_prompt_when_a_snapshot_exists(cx: &mut TestAppContext) {
    let (_state, strip, history, mut cx) = open(cx);
    let job = JobId::parse("snap").unwrap();
    let editor = easel_core::history::EditorSnapshot {
        prose: "make <cat_1> orange".into(),
        model_id: "flux-3".into(),
        values: std::collections::BTreeMap::new(),
        aspect_ratio: "16:9".into(),
    };
    history
        .begin(&job, &request(), &submitted("snap", 1.0), Some(&editor))
        .unwrap();
    history
        .complete(&job, &png(), "image/png", &png(), &Completion::default())
        .unwrap();
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    open_modal(&mut cx, &strip);
    let dialog = strip.update(&mut cx, |strip, cx| strip.dialog(cx).expect("modal"));
    assert_eq!(dialog.prompt, "make <cat_1> orange");
    assert_eq!(dialog.model, "flux-3");
}

fn editor_snapshot(prose: &str, model: &str, tolerance: i64, aspect: &str) -> easel_core::history::EditorSnapshot {
    easel_core::history::EditorSnapshot {
        prose: prose.into(),
        model_id: model.into(),
        values: serde_json::from_value(serde_json::json!({"safety_tolerance": tolerance})).unwrap(),
        aspect_ratio: aspect.into(),
    }
}

fn seed_snapshot(history: &HistoryStore, id: &str, editor: &easel_core::history::EditorSnapshot) {
    let job = JobId::parse(id).unwrap();
    history
        .begin(&job, &request(), &submitted(id, 1.0), Some(editor))
        .unwrap();
    history
        .complete(&job, &png(), "image/png", &png(), &Completion {
            cost: Some(1.0),
            prompt: None,
            duration: None,
        })
        .unwrap();
}

#[gpui::test]
fn restore_puts_the_editor_back_and_leaves_the_canvas(cx: &mut TestAppContext) {
    let (root, strip, history, mut cx) = open_root(cx);
    let editor = editor_snapshot("use <ref_image_1>", "flux-3-image", 4, "16:9");
    seed_snapshot(&history, "job1", &editor);
    let state = strip.read_with(&cx, |strip, _| strip.app_state());
    let image = Arc::new(crate::image_load::decode_image(&png(), "canvas").unwrap());
    let image_ptr = Arc::as_ptr(&image);
    state.update(&mut cx, |app, cx| {
        app.set_image(image, cx);
        app.set_model("flux-2-pro", cx);
        app.scene.prose = "changed".into();
        app.set_aspect_ratio("1:1", cx);
        app.inspection = Some(crate::jobs::Inspection {
            prompt: "compare".into(),
            suggestion: String::new(),
            rows: Vec::new(),
            boxes: Vec::new(),
            frame: None,
        });
        cx.notify();
    });
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    open_modal(&mut cx, &strip);
    strip.update(&mut cx, |strip, cx| strip.press("Restore", cx));
    let (prose, model, tolerance, aspect, inspection, same_image) =
        state.read_with(&cx, |app, _| {
            (
                app.scene.prose.clone(),
                app.model_id.clone(),
                app.values.get("safety_tolerance").cloned(),
                app.scene.aspect_ratio.clone(),
                app.inspection.clone(),
                app.image.as_ref().map(Arc::as_ptr) == Some(image_ptr),
            )
        });
    assert_eq!(prose, "use <ref_image_1>");
    assert_eq!(model, "flux-3-image");
    assert_eq!(tolerance, Some(serde_json::json!(4)));
    assert_eq!(aspect.as_deref(), Some("16:9"));
    assert!(inspection.is_none());
    assert!(same_image);
    assert!(root.read_with(&cx, |root, cx| root.app_state.read(cx).history_item.is_none()));
}

#[gpui::test]
fn restore_replaces_form_values_when_the_model_is_unchanged(cx: &mut TestAppContext) {
    let (_root, strip, history, mut cx) = open_root(cx);
    let editor = editor_snapshot("same model", "flux-3-image", 4, "");
    seed_snapshot(&history, "job1", &editor);
    let state = strip.read_with(&cx, |strip, _| strip.app_state());
    state.update(&mut cx, |app, cx| {
        app.values.insert("safety_tolerance".into(), serde_json::json!(1));
        cx.notify();
    });
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    open_modal(&mut cx, &strip);
    strip.update(&mut cx, |strip, cx| strip.press("Restore", cx));
    let tolerance = state.read_with(&cx, |app, _| app.values.get("safety_tolerance").cloned());
    assert_eq!(tolerance, Some(serde_json::json!(4)));
    assert_eq!(state.read_with(&cx, |app, _| app.model_id.clone()), "flux-3-image");
}

#[gpui::test]
fn restore_of_an_unknown_model_leaves_the_editor(cx: &mut TestAppContext) {
    let (root, strip, history, mut cx) = open_root(cx);
    let editor = editor_snapshot("from the snapshot", "missing-model", 4, "16:9");
    seed_snapshot(&history, "job1", &editor);
    let state = strip.read_with(&cx, |strip, _| strip.app_state());
    state.update(&mut cx, |app, cx| {
        app.scene.prose = "keep this".into();
        cx.notify();
    });
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    open_modal(&mut cx, &strip);
    strip.update(&mut cx, |strip, cx| strip.press("Restore", cx));
    assert_eq!(state.read_with(&cx, |app, _| app.scene.prose.clone()), "keep this");
    assert_eq!(state.read_with(&cx, |app, _| app.model_id.clone()), "flux-3-image");
    assert!(root.read_with(&cx, |root, cx| root.app_state.read(cx).history_item.is_some()));
    assert!(root.read_with(&cx, |root, cx| root.status_line(cx).contains("missing-model")));
}

#[gpui::test]
fn a_chutes_row_restores_and_rearms_the_edit_block(cx: &mut TestAppContext) {
    let (_root, strip, history, mut cx) = open_root(cx);
    let editor = easel_core::history::EditorSnapshot {
        prose: "make it blue".into(),
        model_id: "chutes-qwen-image-edit-2511".into(),
        values: serde_json::from_value(serde_json::json!({"true_cfg_scale": 1.0})).unwrap(),
        aspect_ratio: String::new(),
    };
    seed_snapshot(&history, "job1", &editor);
    let state = strip.read_with(&cx, |strip, _| strip.app_state());
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    open_modal(&mut cx, &strip);
    strip.update(&mut cx, |strip, cx| strip.press("Restore", cx));
    let (model, prose, cfg) = state.read_with(&cx, |app, _| {
        (
            app.model_id.clone(),
            app.scene.prose.clone(),
            app.values.get("true_cfg_scale").cloned(),
        )
    });
    assert_eq!(model, "chutes-qwen-image-edit-2511");
    assert_eq!(prose, "make it blue");
    assert_eq!(cfg, Some(serde_json::json!(1.0)));
    let errors = state.read_with(&cx, |app, _| app.field_errors(0));
    assert!(
        errors
            .iter()
            .any(|error| error.message == "Add at least one image."),
        "{errors:?}"
    );
}

#[gpui::test]
fn restore_while_compare_is_open_shows_the_editor_again(cx: &mut TestAppContext) {
    let (_root, strip, history, mut cx) = open_root(cx);
    let editor = editor_snapshot("back to the editor", "flux-3-image", 4, "16:9");
    seed_snapshot(&history, "job1", &editor);
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    open_modal(&mut cx, &strip);
    strip.update(&mut cx, |strip, cx| strip.press("Compare", cx));
    assert!(strip.read_with(&cx, |strip, cx| {
        strip.app_state().read(cx).inspection.is_some()
    }));
    strip.update(&mut cx, |strip, cx| {
        strip.show(0, cx);
        strip.press("Restore", cx);
    });
    assert!(strip.read_with(&cx, |strip, cx| {
        strip.app_state().read(cx).inspection.is_none()
    }));
}

#[gpui::test]
fn reveal_opens_the_result_folder_and_stays_open(cx: &mut TestAppContext) {
    let (root, strip, history, mut cx) = open_root(cx);
    seed_done(&history, "job1");
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let record = Arc::clone(&seen);
    strip.update(&mut cx, |strip, _| {
        strip.set_reveal(Arc::new(move |folder| {
            record
                .lock()
                .expect("calls")
                .push(folder.to_path_buf());
            Ok(())
        }));
    });
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    open_modal(&mut cx, &strip);
    strip.update(&mut cx, |strip, cx| strip.press("Reveal", cx));
    let calls = seen.lock().expect("calls").clone();
    let result = history
        .result_path(&JobId::parse("job1").unwrap())
        .unwrap();
    assert_eq!(calls, vec![result.parent().unwrap().to_path_buf()]);
    assert!(root.read_with(&cx, |root, cx| root.app_state.read(cx).history_item.is_some()));
}

#[gpui::test]
fn a_failed_reveal_is_reported_and_starts_nothing_else(cx: &mut TestAppContext) {
    let (root, strip, history, mut cx) = open_root(cx);
    seed_done(&history, "job1");
    let seen = Arc::new(std::sync::Mutex::new(0u32));
    let record = Arc::clone(&seen);
    strip.update(&mut cx, |strip, _| {
        strip.set_reveal(Arc::new(move |_folder| {
            *record.lock().expect("calls") += 1;
            Err("Could not open the folder".into())
        }));
    });
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    open_modal(&mut cx, &strip);
    strip.update(&mut cx, |strip, cx| strip.press("Reveal", cx));
    assert_eq!(*seen.lock().expect("calls"), 1);
    assert!(root.read_with(&cx, |root, cx| {
        root.status_line(cx).contains("Could not open the folder")
            && root.app_state.read(cx).history_item.is_some()
    }));
}

#[test]
fn reveal_passes_a_folder_with_a_space_as_one_argument() {
    let dir = temp_dir("space dir");
    let history = Arc::new(HistoryStore::open(dir.join("history")).unwrap());
    seed_done(&history, "job1");
    let path = history
        .result_path(&JobId::parse("job1").unwrap())
        .unwrap();
    let folder = path.parent().unwrap();
    assert!(folder.to_string_lossy().contains(' '));
    let mut command = std::process::Command::new("xdg-open");
    command.arg(folder);
    let args: Vec<_> = command.get_args().collect();
    assert_eq!(args, [folder.as_os_str()]);
}

fn grid_png(seed: u8) -> Vec<u8> {
    let mut image = image::RgbImage::new(64, 64);
    image.put_pixel(0, 0, image::Rgb([seed, seed, seed]));
    easel_core::image_prep::encode_png(&image::DynamicImage::ImageRgb8(image))
}

fn apiframe_request() -> JobRequest {
    let mut request = request();
    request.model_id = "apiframe-midjourney".into();
    request.compiled_prompt = "a lighthouse maze".into();
    request.created_at = "2026-10-25T10:00:00Z".into();
    request
}

fn apiframe_submit_body(id: &str) -> String {
    format!(r#"{{"jobId":"{id}","status":"QUEUED"}}"#)
}

fn mj_completed_body(id: &str) -> String {
    format!(
        r#"{{"status":"COMPLETED","creditCost":10,"result":{{"images":["https://cdn2.apiframe.ai/images/{id}-1.png","https://cdn2.apiframe.ai/images/{id}-2.png","https://cdn2.apiframe.ai/images/{id}-3.png","https://cdn2.apiframe.ai/images/{id}-4.png"],"gridUrl":"https://cdn2.apiframe.ai/images/{id}-grid.png"}}}}"#
    )
}

fn grid_script(id: &str) -> Vec<Result<easel_core::http::HttpResponse, crate::net::TransportError>> {
    vec![
        Ok(http(202, apiframe_submit_body(id))),
        Ok(http(200, r#"{"status":"QUEUED","progress":0}"#)),
        Ok(http(200, r#"{"status":"PROCESSING","progress":50}"#)),
        Ok(http(200, mj_completed_body(id))),
        Ok(http(200, grid_png(9))),
        Ok(http(200, grid_png(1))),
        Ok(http(200, grid_png(2))),
        Ok(http(200, grid_png(3))),
        Ok(http(200, grid_png(4))),
    ]
}

fn grid_meta_of(history: &HistoryStore, id: &str) -> easel_core::history::GridMeta {
    history
        .grid_meta(&JobId::parse(id).unwrap())
        .unwrap_or_else(|| panic!("no grid meta for {id}"))
}

fn seed_grid(history: &HistoryStore, id: &str) -> Vec<Vec<u8>> {
    let grid = JobId::parse(id).unwrap();
    let submitted = Submitted {
        job_id: grid.clone(),
        poll_url: format!("https://api.apiframe.ai/v2/jobs/{id}")
            .parse()
            .unwrap(),
        quote: None,
    };
    history.begin(&grid, &apiframe_request(), &submitted, None).unwrap();
    let quadrants: Vec<Vec<u8>> = (0..4).map(grid_png).collect();
    let meta = easel_core::history::GridMeta {
        grid_url: format!("https://cdn2.apiframe.ai/images/{id}-grid.png"),
        images: std::array::from_fn(|index| {
            format!("https://cdn2.apiframe.ai/images/{id}-{}.png", index + 1)
        }),
        picked: [false; 4],
    };
    history
        .complete_grid(
            &grid,
            &grid_png(9),
            &quadrants,
            &grid_png(8),
            meta,
            &Completion::default(),
        )
        .unwrap();
    quadrants
}

fn grid_runner_open(
    cx: &mut TestAppContext,
    name: &str,
    script: Vec<Result<easel_core::http::HttpResponse, crate::net::TransportError>>,
) -> (
    gpui::Entity<AppState>,
    gpui::Entity<BottomStrip>,
    gpui::Entity<crate::jobs::JobRunner>,
    Arc<HistoryStore>,
    VisualTestContext,
) {
    let root = temp_dir(name);
    let history = Arc::new(HistoryStore::open(root.join("history")).unwrap());
    let transport: Arc<dyn crate::net::HttpTransport> =
        Arc::new(crate::net::FakeTransport::scripted(script));
    let keys = Arc::new(crate::settings::SharedKeys::default());
    keys.set_session(
        easel_core::model_spec::ProviderId::Apiframe,
        "apiframe-test-key",
    );
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
            cx.open_window(WindowOptions::default(), |_window, cx| {
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
    let state = strip.read_with(&visual, |strip, _| strip.app_state());
    (state, strip, runner, history, visual)
}

fn wait_for_grid_job(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.executor().advance_clock(std::time::Duration::from_secs(2));
    cx.run_until_parked();
    cx.executor()
        .advance_clock(std::time::Duration::from_millis(2500));
    cx.run_until_parked();
    cx.executor().advance_clock(std::time::Duration::from_secs(3));
    cx.run_until_parked();
}

#[gpui::test]
fn a_completed_midjourney_job_downloads_the_grid_and_opens_the_picker(cx: &mut TestAppContext) {
    let (state, strip, _runner, history, mut cx) =
        grid_runner_open(cx, "grid-auto", grid_script("mjgrid1"));
    runner_submit(&strip, &mut cx);
    wait_for_grid_job(&mut cx);

    let meta = grid_meta_of(&history, "mjgrid1");
    assert_eq!(meta.picked, [false, false, false, false]);
    let quadrants = history
        .quadrant_bytes(&JobId::parse("mjgrid1").unwrap(), 0..4)
        .into_iter()
        .map(|bytes| bytes.is_some())
        .collect::<Vec<_>>();
    assert_eq!(quadrants, [true, true, true, true]);
    let opened = state.read_with(&cx, |app, _| {
        app.history_item
            .clone()
            .and_then(|item| item.id)
            .map(|id| id.as_str().to_string())
    });
    assert_eq!(opened.as_deref(), Some("mjgrid1"));

    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    let dialog = strip.update(&mut cx, |strip, cx| {
        strip.grid_dialog(cx).expect("grid picker open")
    });
    assert_eq!(dialog.picked, [false, false, false, false]);
    assert_eq!(dialog.images.iter().filter(|image| image.is_some()).count(), 4);

    strip.update(&mut cx, |strip, cx| {
        strip.toggle_grid_quadrant(0, cx);
        strip.toggle_grid_quadrant(2, cx);
        strip.confirm_grid_pick(cx);
    });
    let ids: Vec<String> = history
        .list()
        .unwrap()
        .iter()
        .map(|entry| entry.id.as_str().to_string())
        .collect();
    assert_eq!(ids, ["mjgrid1-q1", "mjgrid1-q3", "mjgrid1"]);
    assert_eq!(
        grid_meta_of(&history, "mjgrid1").picked,
        [true, false, true, false]
    );
    let lines = strip.update(&mut cx, |strip, cx| {
        strip.refresh(cx);
        strip.lines()
    });
    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(
        state.read_with(&cx, |app, _| app.history_item.is_none()),
        "modal closed after the pick"
    );
}

#[gpui::test]
fn a_failed_pick_reports_the_error_and_keeps_the_picker_open(cx: &mut TestAppContext) {
    let (_state, strip, _runner, history, mut cx) =
        grid_runner_open(cx, "grid-pick-fail", Vec::new());
    seed_grid(&history, "mjgrid7");
    let notes =
        cx.update(|_window, cx| cx.new(|_| crate::notifications::Notifications::new()));
    strip.update(&mut cx, |strip, _| strip.set_notes(notes.clone()));
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    open_modal(&mut cx, &strip);
    assert!(strip.update(&mut cx, |strip, cx| strip.grid_dialog(cx).is_some()));
    strip.update(&mut cx, |strip, cx| {
        strip.toggle_grid_quadrant(0, cx);
        strip.toggle_grid_quadrant(2, cx);
    });
    let entry = history
        .result_path(&JobId::parse("mjgrid7").unwrap())
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf();
    std::fs::remove_file(entry.join("quadrant_0.png")).unwrap();

    strip.update(&mut cx, |strip, cx| strip.confirm_grid_pick(cx));
    assert!(
        strip.update(&mut cx, |strip, cx| strip.grid_dialog(cx).is_some()),
        "a failed pick keeps the picker open"
    );
    assert_eq!(
        strip.read_with(&cx, |strip, _| strip.grid_selection),
        [true, false, true, false],
        "the selection survives the failure"
    );
    assert!(
        notes.read_with(&cx, |notes, _| notes.status().contains("quadrant file is missing")),
        "the store error reaches the status bar"
    );
    let ids: Vec<String> = history
        .list()
        .unwrap()
        .iter()
        .map(|entry| entry.id.as_str().to_string())
        .collect();
    assert_eq!(ids, ["mjgrid7"], "{ids:?}");

    strip.update(&mut cx, |strip, cx| strip.toggle_grid_quadrant(0, cx));
    strip.update(&mut cx, |strip, cx| strip.confirm_grid_pick(cx));
    let ids: Vec<String> = history
        .list()
        .unwrap()
        .iter()
        .map(|entry| entry.id.as_str().to_string())
        .collect();
    assert_eq!(ids, ["mjgrid7-q3", "mjgrid7"], "{ids:?}");
    assert_eq!(
        grid_meta_of(&history, "mjgrid7").picked,
        [false, false, true, false]
    );
    assert!(
        strip.update(&mut cx, |strip, cx| strip.grid_dialog(cx).is_none()),
        "a successful retry closes the picker"
    );
}

fn runner_submit(strip: &gpui::Entity<BottomStrip>, cx: &mut VisualTestContext) {
    let runner = strip
        .read_with(cx, |strip, _| strip.runner.clone())
        .expect("runner");
    runner.update(cx, |runner, cx| runner.submit(apiframe_request(), cx));
}

#[gpui::test]
fn reopening_a_grid_marks_picked_quadrants_and_mints_the_rest(cx: &mut TestAppContext) {
    let (state, strip, _runner, history, mut cx) = grid_runner_open(cx, "grid-reopen", Vec::new());
    seed_grid(&history, "mjgrid2");
    history
        .pick_quadrants(&JobId::parse("mjgrid2").unwrap(), &[0, 2], "2026-10-25T10:05:00Z")
        .unwrap();
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    let grid_index = strip.read_with(&cx, |strip, _| {
        strip
            .rows
            .iter()
            .position(|row| row.grid.is_some())
            .expect("grid row in strip")
    });
    strip.update(&mut cx, |strip, cx| strip.show(grid_index, cx));

    let dialog = strip.update(&mut cx, |strip, cx| {
        strip.grid_dialog(cx).expect("grid picker open")
    });
    assert_eq!(dialog.picked, [true, false, true, false]);
    assert_eq!(dialog.selected, [false, false, false, false]);
    strip.update(&mut cx, |strip, cx| {
        strip.toggle_grid_quadrant(0, cx);
    });
    let selected = strip.read_with(&cx, |strip, _| strip.grid_selection);
    assert_eq!(selected, [false, false, false, false], "picked stays blocked");
    strip.update(&mut cx, |strip, cx| {
        strip.toggle_grid_quadrant(3, cx);
        strip.confirm_grid_pick(cx);
    });
    let ids: Vec<String> = history
        .list()
        .unwrap()
        .iter()
        .map(|entry| entry.id.as_str().to_string())
        .collect();
    assert_eq!(ids, ["mjgrid2-q4", "mjgrid2-q1", "mjgrid2-q3", "mjgrid2"]);
    assert_eq!(
        grid_meta_of(&history, "mjgrid2").picked,
        [true, false, true, true]
    );
    assert!(
        state.read_with(&cx, |app, _| app.history_item.is_none()),
        "modal closed after the pick"
    );
}

#[gpui::test]
fn closing_the_picker_without_picking_keeps_every_quadrant(cx: &mut TestAppContext) {
    let (_state, strip, _runner, history, mut cx) =
        grid_runner_open(cx, "grid-close", Vec::new());
    seed_grid(&history, "mjgrid3");
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    open_modal(&mut cx, &strip);
    strip.update(&mut cx, |strip, cx| {
        strip.toggle_grid_quadrant(1, cx);
        strip.dismiss_modal(cx);
    });
    let ids: Vec<String> = history
        .list()
        .unwrap()
        .iter()
        .map(|entry| entry.id.as_str().to_string())
        .collect();
    assert_eq!(ids, ["mjgrid3"], "{ids:?}");

    open_modal(&mut cx, &strip);
    let dialog = strip.update(&mut cx, |strip, cx| {
        strip.grid_dialog(cx).expect("reopened")
    });
    assert_eq!(dialog.picked, [false, false, false, false]);
    assert_eq!(dialog.selected, [false, false, false, false]);
}

#[gpui::test]
fn double_clicking_a_grid_row_never_promotes_the_composite(cx: &mut TestAppContext) {
    let (_state, strip, _runner, history, mut cx) =
        grid_runner_open(cx, "grid-double", Vec::new());
    seed_grid(&history, "mjgrid4");
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    draw(&mut cx);
    let point = strip.read_with(&cx, |strip, _| strip.entry_point(0));
    cx.simulate_event(down(MouseButton::Left, point, 2));
    let (dialog_open, image) = strip.update(&mut cx, |strip, cx| {
        (
            strip.grid_dialog(cx).is_some(),
            strip.app_state().read(cx).image.clone(),
        )
    });
    assert!(dialog_open);
    assert!(image.is_none());
}

#[gpui::test]
fn a_busy_modal_blocks_the_grid_auto_open(cx: &mut TestAppContext) {
    let (state, strip, _runner, history, mut cx) =
        grid_runner_open(cx, "grid-busy", grid_script("mjgrid5"));
    state.update(&mut cx, |app, cx| app.set_settings_open(true, cx));
    runner_submit(&strip, &mut cx);
    wait_for_grid_job(&mut cx);
    let opened = state.read_with(&cx, |app, _| app.history_item.clone());
    assert!(opened.is_none(), "settings modal keeps its place");
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    let lines = strip.read_with(&cx, |strip, _| strip.lines());
    assert_eq!(lines.len(), 1, "{lines:?}");
    let meta = grid_meta_of(&history, "mjgrid5");
    assert_eq!(meta.picked, [false, false, false, false]);
}

#[gpui::test]
fn picked_quadrants_behave_like_single_image_rows(cx: &mut TestAppContext) {
    let (_state, strip, _runner, history, mut cx) =
        grid_runner_open(cx, "grid-standalone", Vec::new());
    seed_grid(&history, "mjgrid6");
    history
        .pick_quadrants(&JobId::parse("mjgrid6").unwrap(), &[0], "2026-10-25T10:05:00Z")
        .unwrap();
    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    let lines = strip.read_with(&cx, |strip, _| strip.lines());
    assert_eq!(lines.len(), 2, "{lines:?}");

    open_modal(&mut cx, &strip);
    let dialog = strip.update(&mut cx, |strip, cx| strip.dialog(cx).expect("single dialog"));
    assert!(dialog.image.is_some());
    assert_eq!(
        dialog.actions,
        vec![
            "Promote".to_string(),
            "Save As".to_string(),
            "Reveal".to_string(),
            "Compare".to_string(),
            "Discard".to_string()
        ]
    );
    strip.update(&mut cx, |strip, cx| strip.dismiss_modal(cx));

    draw(&mut cx);
    let point = strip.read_with(&cx, |strip, _| strip.entry_point(0));
    cx.simulate_event(down(MouseButton::Left, point, 2));
    assert!(strip.read_with(&cx, |strip, cx| {
        strip.app_state().read(cx).image.is_some()
    }));

    strip.update(&mut cx, |strip, cx| strip.refresh(cx));
    // Discarding the grid row removes only the grid entry.
    let grid_index = strip.read_with(&cx, |strip, _| {
        strip
            .rows
            .iter()
            .position(|row| row.grid.is_some())
            .expect("grid row in strip")
    });
    strip.update(&mut cx, |strip, cx| {
        strip.select(grid_index, cx);
        strip.press("Discard", cx);
        strip.press("Confirm", cx);
    });
    let ids: Vec<String> = history
        .list()
        .unwrap()
        .iter()
        .map(|entry| entry.id.as_str().to_string())
        .collect();
    assert_eq!(ids, ["mjgrid6-q1"], "{ids:?}");
}
