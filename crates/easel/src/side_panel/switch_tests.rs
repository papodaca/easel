use easel_core::geometry::Rect;
use easel_core::scene::{BoxKind, ImageRef};
use gpui::{TestAppContext, prelude::*};
use serde_json::json;

use crate::app_state::AppState;
use crate::jobs::build_request;

fn app(cx: &mut TestAppContext) -> gpui::Entity<AppState> {
    cx.update(|cx| cx.new(|_| AppState::new()))
}

#[gpui::test]
fn values_carry_only_when_the_target_accepts_them(cx: &mut TestAppContext) {
    let state = app(cx);
    state.update(cx, |app, cx| {
        app.values.insert("aspect_ratio".into(), json!("21:9"));
        app.values.insert("safety_tolerance".into(), json!(3));
        app.set_model("krea-2-large", cx);
    });
    let (aspect, safety) = state.read_with(cx, |app, _| {
        (
            app.values.get("aspect_ratio").cloned(),
            app.values.get("safety_tolerance").cloned(),
        )
    });
    assert_ne!(aspect.as_ref(), Some(&json!("21:9")));
    assert!(safety.is_none());
    state.update(cx, |app, cx| {
        app.set_model("flux-3-image", cx);
        app.values.insert("aspect_ratio".into(), json!("21:9"));
        app.set_model("krea-3", cx);
    });
    let aspect = state.read_with(cx, |app, _| app.values.get("aspect_ratio").cloned());
    assert_eq!(aspect, Some(json!("21:9")));
}

#[gpui::test]
fn switching_away_drops_the_json_suffix_and_back_restores_it(cx: &mut TestAppContext) {
    let state = app(cx);
    state.update(cx, |app, cx| {
        app.scene.prose = "a <layout_1> red square".into();
        app.scene.add_box(
            Rect {
                x: 10,
                y: 10,
                w: 100,
                h: 80,
            },
            BoxKind::Layout,
        );
        app.set_aspect_ratio("1:1", cx);
    });
    let before = state.read_with(cx, |app, _| {
        (
            app.compiled(),
            app.scene.prose.clone(),
            app.scene.boxes.len(),
        )
    });
    assert!(!before.0.rows_json.is_empty(), "{}", before.0.rows_json);
    state.update(cx, |app, cx| app.set_model("krea-3", cx));
    let middle = state.read_with(cx, |app, _| app.compiled());
    assert!(middle.rows_json.is_empty(), "{}", middle.rows_json);
    assert!(!middle.text.contains('<'), "{}", middle.text);
    assert!(!middle.warnings.is_empty(), "{middle:?}");
    assert!(!state.read_with(cx, |app, _| app.box_tool_enabled()));
    state.update(cx, |app, cx| app.set_model("flux-3-image", cx));
    let after = state.read_with(cx, |app, _| {
        (
            app.compiled(),
            app.scene.prose.clone(),
            app.scene.boxes.len(),
        )
    });
    assert_eq!(after.0.rows_json, before.0.rows_json);
    assert_eq!(after.1, before.1);
    assert_eq!(after.2, before.2);
}

#[gpui::test]
fn reference_labels_follow_the_model_capacity(cx: &mut TestAppContext) {
    let state = app(cx);
    state.update(cx, |app, _| {
        for index in 0..9 {
            app.scene
                .add_reference(ImageRef::new("reference", 64, 64 + index as u32));
        }
    });
    state.update(cx, |app, cx| app.set_model("krea-2-large", cx));
    let marks = state.read_with(cx, |app, _| app.reference_marks());
    assert!(marks.iter().all(|(_, sent)| *sent), "{marks:?}");
    assert_eq!(marks[0].0, "image_url");
    assert_eq!(marks[1].0, "style 1");
    assert_eq!(marks[8].0, "style 8");
    state.update(cx, |app, cx| app.set_model("flux-2-pro", cx));
    let marks = state.read_with(cx, |app, _| app.reference_marks());
    assert!(marks[..8].iter().all(|(_, sent)| *sent));
    assert!(!marks[8].1);
    assert!(marks[8].0.contains("not sent"), "{}", marks[8].0);
    assert_eq!(marks[0].0, "input_image");
    assert!(marks[7].0.starts_with("input_image_8"), "{}", marks[7].0);
}

#[gpui::test]
fn a_built_request_keeps_its_model_after_a_switch(cx: &mut TestAppContext) {
    let state = app(cx);
    state.update(cx, |app, cx| {
        app.scene.prose = "a red square".into();
        app.set_aspect_ratio("1:1", cx);
    });
    let request = state.read_with(cx, |app, _| build_request(app).unwrap());
    state.update(cx, |app, cx| app.set_model("krea-3", cx));
    assert_eq!(request.model_id, "flux-3-image");
    let next = state.read_with(cx, |app, _| build_request(app).unwrap());
    assert_eq!(next.model_id, "krea-3");
}

#[gpui::test]
fn google_compile_drops_a_box_tag_and_does_not_require_flux_fields(cx: &mut TestAppContext) {
    let state = app(cx);
    state.update(cx, |app, cx| {
        app.scene.prose = "paint <layout_1> red".into();
        app.set_model("gemini-nano-banana-2.1", cx);
    });
    let (compiled, errors) = state.read_with(cx, |app, _| (app.compiled(), app.field_errors(0)));
    assert!(compiled.rows_json.is_empty(), "{}", compiled.rows_json);
    assert!(!compiled.text.contains('<'), "{}", compiled.text);
    assert!(
        compiled
            .warnings
            .iter()
            .any(|warning| warning.contains("layout_1")),
        "{compiled:?}"
    );
    assert!(
        errors.iter().all(|error| error.key != "safety_tolerance"),
        "{errors:?}"
    );
}
