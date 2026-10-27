use std::sync::Arc;

use easel_core::geometry::{Rect, Size};
use easel_core::model_spec::ProviderId;
use easel_core::scene::BoxKind;
use easel_core::schema::FieldKind;
use gpui::{
    ClipboardItem, Focusable as _, Image, ImageFormat, TestAppContext, VisualTestContext,
    prelude::*,
};
use serde_json::json;

use crate::app_state::AppState;
use crate::net::MapKeys;
use crate::side_panel::References;
use crate::side_panel::prompt_editor::PromptEditor;
use crate::side_panel::schema_form::SchemaForm;
use crate::side_panel::validation::Validation;

fn png_bytes() -> Vec<u8> {
    std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/pixel.png"
    ))
    .unwrap()
}

struct Host {
    state: gpui::Entity<AppState>,
}

impl gpui::Render for Host {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        gpui::div()
    }
}

fn open(cx: &mut TestAppContext) -> (gpui::Entity<AppState>, VisualTestContext) {
    let window = cx.update(|cx| {
        crate::widgets::bind_keys(cx);
        cx.open_window(gpui::WindowOptions::default(), |_window, cx| {
            let state = cx.new(|_| AppState::new());
            cx.new(|_| Host { state })
        })
        .unwrap()
    });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let state = window
        .root(&mut visual)
        .unwrap()
        .read_with(&visual, |host, _| host.state.clone());
    (state, visual)
}

#[gpui::test]
fn form_renders_each_schema_field_and_version_is_read_only(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    let form = cx.new(|cx| SchemaForm::new(state, cx));
    let (count, read_only, version) = form.read_with(&cx, |form, cx| {
        (
            form.field_count(),
            form.version_is_read_only(cx),
            form.value("version", cx),
        )
    });
    assert_eq!(count, 5);
    assert!(read_only);
    assert_eq!(version, json!("latest"));
}

#[gpui::test]
fn auto_with_boxes_and_no_source_disables_generate(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.scene.prose = "a red square".into();
        app.scene.add_box(
            Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 100,
            },
            BoxKind::Layout,
        );
        cx.notify();
    });
    let keys: Arc<dyn crate::net::KeyResolver> =
        Arc::new(MapKeys::default().insert(ProviderId::Bfl, "test-key"));
    let validation = cx.new(|cx| Validation::new(state.clone(), keys, cx));
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    assert!(validation.read_with(&cx, |validation, cx| {
        validation
            .messages(cx)
            .iter()
            .any(|message| message.contains("aspect"))
    }));
    state.update(&mut cx, |app, cx| app.set_aspect_ratio("1:1", cx));
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
}

#[gpui::test]
fn picking_an_aspect_ratio_updates_the_scene(cx: &mut TestAppContext) {
    let window = cx.update(|cx| {
        crate::widgets::bind_keys(cx);
        cx.open_window(gpui::WindowOptions::default(), |_window, cx| {
            let state = cx.new(|_| AppState::new());
            let form = cx.new(|cx| SchemaForm::new(state.clone(), cx));
            cx.new(|_| Shown { state, form })
        })
        .unwrap()
    });
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    let shown = window.root(&mut cx).unwrap();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let form = shown.read_with(&cx, |shown, _| shown.form.clone());
    let point = form.read_with(&cx, |form, cx| form.aspect_control().read(cx).click_point());
    cx.simulate_click(point, gpui::Modifiers::default());
    assert!(form.read_with(&cx, |form, cx| form.aspect_control().read(cx).is_open()));
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let option = form.read_with(&cx, |form, cx| form.aspect_control().read(cx).menu_point(0));
    cx.simulate_click(option, gpui::Modifiers::default());
    let ratio = shown.read_with(&cx, |shown, cx| {
        shown.state.read(cx).scene.aspect_ratio.clone()
    });
    assert_eq!(ratio.as_deref(), Some("21:9"));
    assert!(!form.read_with(&cx, |form, cx| form.resolution_is_open(cx)));
}

struct Shown {
    state: gpui::Entity<AppState>,
    form: gpui::Entity<SchemaForm>,
}

impl gpui::Render for Shown {
    fn render(
        &mut self,
        _: &mut gpui::Window,
        _: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        gpui::div().size_full().child(self.form.clone())
    }
}

#[gpui::test]
fn aspect_stays_editable_with_a_source_and_boxes(cx: &mut TestAppContext) {
    let window = cx.update(|cx| {
        crate::widgets::bind_keys(cx);
        cx.open_window(gpui::WindowOptions::default(), |_window, cx| {
            let state = cx.new(|cx| {
                let mut app = AppState::new();
                app.set_image(source_image(), cx);
                app.scene.add_box(
                    Rect {
                        x: 0,
                        y: 0,
                        w: 40,
                        h: 40,
                    },
                    BoxKind::New,
                );
                app
            });
            let form = cx.new(|cx| SchemaForm::new(state.clone(), cx));
            cx.new(|_| Shown { state, form })
        })
        .unwrap()
    });
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    let shown = window.root(&mut cx).unwrap();
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let form = shown.read_with(&cx, |shown, _| shown.form.clone());
    assert!(form.read_with(&cx, |form, cx| form.aspect_enabled(cx)));
    let point = form.read_with(&cx, |form, cx| form.aspect_control().read(cx).click_point());
    cx.simulate_click(point, gpui::Modifiers::default());
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let option = form.read_with(&cx, |form, cx| form.aspect_control().read(cx).menu_point(0));
    cx.simulate_click(option, gpui::Modifiers::default());
    let ratio = shown.read_with(&cx, |shown, cx| {
        shown.state.read(cx).scene.aspect_ratio.clone()
    });
    assert_eq!(ratio.as_deref(), Some("21:9"));
}

#[gpui::test]
fn a_missing_key_disables_generate_until_a_session_key_exists(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.scene.prose = "a red square".into();
        app.set_aspect_ratio("1:1", cx);
    });
    let keys: Arc<dyn crate::net::KeyResolver> = Arc::new(MapKeys::default());
    let validation = cx.new(|cx| Validation::new(state.clone(), keys, cx));
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages
            .iter()
            .any(|message| message.contains("No BFL API key"))
    );
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    let keys: Arc<dyn crate::net::KeyResolver> =
        Arc::new(MapKeys::default().insert(ProviderId::Bfl, "session-key"));
    let validation = cx.new(|cx| Validation::new(state, keys, cx));
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    assert!(validation.read_with(&cx, |validation, cx| {
        !validation
            .messages(cx)
            .iter()
            .any(|message| message.contains("No BFL API key"))
    }));
}

#[gpui::test]
fn a_missing_xai_key_disables_generate_until_a_key_exists(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.scene.prose = "a red square".into();
        app.set_aspect_ratio("1:1", cx);
    });
    assert_eq!(
        crate::side_panel::validation::missing_key_line(ProviderId::Xai),
        "No xAI API key — open Settings"
    );
    let keys: Arc<dyn crate::net::KeyResolver> = Arc::new(MapKeys::default());
    let validation = cx.new(|cx| Validation::new(state.clone(), keys, cx));
    let provider = state.read_with(&cx, |app, _| app.spec().provider);
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages
            .iter()
            .any(|message| message == &crate::side_panel::validation::missing_key_line(provider)),
        "{messages:?}"
    );
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    let keys: Arc<dyn crate::net::KeyResolver> =
        Arc::new(MapKeys::default().insert(ProviderId::Xai, "xai-key"));
    let validation = cx.new(|cx| Validation::new(state.clone(), keys, cx));
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    let keys: Arc<dyn crate::net::KeyResolver> = Arc::new(
        MapKeys::default()
            .insert(provider, "session-key")
            .insert(ProviderId::Xai, "xai-key"),
    );
    let validation = cx.new(|cx| Validation::new(state, keys, cx));
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    assert!(validation.read_with(&cx, |validation, cx| {
        !validation
            .messages(cx)
            .iter()
            .any(|message| message.contains("API key"))
    }));
}

#[gpui::test]
fn insert_tag_writes_the_bracketed_id_at_the_cursor(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, _| {
        app.scene.prose = "ab".into();
        let id = app.scene.add_box_with_prefix(
            Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
            },
            BoxKind::Layout,
            "animal",
        );
        assert_eq!(id.as_str(), "animal_1");
    });
    let editor = cx.new(|cx| PromptEditor::new(state, cx));
    editor.update(&mut cx, |editor, cx| {
        editor.set_cursor(1, cx);
        editor.insert_tag("animal_1", cx);
    });
    assert_eq!(
        editor.read_with(&cx, |editor, cx| editor.prose(cx)),
        "a<animal_1>b"
    );
}

#[gpui::test]
fn a_tenth_extra_reference_is_refused(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| app.set_image(source_image(), cx));
    let refs = cx.new(|cx| References::new(state, cx));
    for _ in 0..9 {
        refs.update(&mut cx, |refs, cx| refs.add_decoded(source_image(), cx));
    }
    refs.update(&mut cx, |refs, cx| refs.add_decoded(source_image(), cx));
    let message = refs.read_with(&cx, |refs, _| refs.refusal().unwrap_or_default());
    assert!(message.contains("10"), "{message}");
}

#[gpui::test]
fn removing_the_source_rewrites_the_next_reference_tag(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_image(source_image(), cx);
        app.scene.prose = "keep <ref_image_1>".into();
    });
    let refs = cx.new(|cx| References::new(state.clone(), cx));
    refs.update(&mut cx, |refs, cx| refs.add_decoded(source_image(), cx));
    refs.update(&mut cx, |refs, cx| refs.remove_slot(0, cx));
    let prose = state.read_with(&cx, |app, _| app.scene.prose.clone());
    assert_eq!(prose, "keep <ref_image_0>");
}

#[gpui::test]
fn a_safety_cap_shrinks_the_slider(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    let form = cx.new(|cx| SchemaForm::new(state, cx));
    form.update(&mut cx, |form, cx| form.apply_safety_cap(4, cx));
    let (max, value) = form.read_with(&cx, |form, cx| (form.safety_max(cx), form.safety_value(cx)));
    assert_eq!(max, 4.0);
    assert!(value <= 4.0);
}

fn mount(
    cx: &mut TestAppContext,
) -> (
    gpui::Entity<AppState>,
    gpui::Entity<SchemaForm>,
    VisualTestContext,
) {
    let window = cx.update(|cx| {
        crate::widgets::bind_keys(cx);
        cx.open_window(gpui::WindowOptions::default(), |_window, cx| {
            let state = cx.new(|_| AppState::new());
            let form = cx.new(|cx| SchemaForm::new(state.clone(), cx));
            cx.new(|_| Shown { state, form })
        })
        .unwrap()
    });
    let mut visual = VisualTestContext::from_window(window.into(), cx);
    let shown = window.root(&mut visual).unwrap();
    visual.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let (state, form) = shown.read_with(&visual, |shown, _| {
        (shown.state.clone(), shown.form.clone())
    });
    (state, form, visual)
}

fn choose(cx: &mut VisualTestContext, form: &gpui::Entity<SchemaForm>, key: &str, label: &str) {
    let select = form.read_with(cx, |form, _| form.select(key).expect(key).clone());
    let (start, target) = select.read_with(cx, |select, _| {
        let options = select.options();
        let start = options
            .iter()
            .position(|option| select.value().as_str() == Some(option.as_ref()))
            .unwrap_or(0);
        let target = options
            .iter()
            .position(|option| option.as_ref() == label)
            .unwrap_or_else(|| panic!("missing option {label}"));
        (start, target)
    });
    cx.update(|window, cx| select.focus_handle(cx).focus(window, cx));
    let mut keys = String::from("enter ");
    let motion = if target >= start { "down " } else { "up " };
    for _ in 0..start.abs_diff(target) {
        keys.push_str(motion);
    }
    keys.push_str("enter");
    cx.simulate_keystrokes(keys.trim());
}

fn replace_text(
    cx: &mut VisualTestContext,
    form: &gpui::Entity<SchemaForm>,
    key: &str,
    text: &str,
) {
    let input = form.read_with(cx, |form, _| form.text(key).expect(key).clone());
    cx.update(|window, cx| input.focus_handle(cx).focus(window, cx));
    cx.simulate_keystrokes("ctrl-a backspace");
    if !text.is_empty() {
        cx.simulate_input(text);
    }
}

fn replace_number(
    cx: &mut VisualTestContext,
    form: &gpui::Entity<SchemaForm>,
    key: &str,
    text: &str,
) {
    let input = form.read_with(cx, |form, _| form.number(key).expect(key).clone());
    cx.update(|window, cx| input.focus_handle(cx).focus(window, cx));
    cx.simulate_keystrokes("ctrl-a backspace");
    if !text.is_empty() {
        cx.simulate_input(text);
    }
    cx.update(|window, cx| {
        window.blur(cx);
        window.draw(cx).clear(cx);
    });
}

fn click_check(cx: &mut VisualTestContext, form: &gpui::Entity<SchemaForm>, key: &str) {
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let point = form.read_with(cx, |form, cx| {
        form.check(key).expect(key).read(cx).click_point()
    });
    cx.simulate_click(point, gpui::Modifiers::default());
}

fn drag_slider(cx: &mut VisualTestContext, form: &gpui::Entity<SchemaForm>, key: &str, at: f32) {
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let bounds = form.read_with(cx, |form, cx| {
        form.slider(key).expect(key).read(cx).track_bounds()
    });
    let y = bounds.origin.y + bounds.size.height / 2.0;
    let x = if at >= 1.0 {
        bounds.origin.x + bounds.size.width - gpui::px(1.0)
    } else {
        bounds.origin.x + bounds.size.width * at
    };
    let point = gpui::point(x, y);
    cx.simulate_mouse_down(point, gpui::MouseButton::Left, gpui::Modifiers::default());
    cx.simulate_mouse_up(point, gpui::MouseButton::Left, gpui::Modifiers::default());
}

fn stored(
    state: &gpui::Entity<AppState>,
    cx: &VisualTestContext,
    key: &str,
) -> Option<serde_json::Value> {
    state.read_with(cx, |app, _| app.values.get(key).cloned())
}

#[gpui::test]
fn krea3_edits_thinking_seed_and_regional_edit(cx: &mut TestAppContext) {
    let (state, form, mut cx) = mount(cx);
    state.update(&mut cx, |app, cx| {
        app.scene.prose = "a red square".into();
        app.set_model("krea-3", cx);
    });
    choose(&mut cx, &form, "thinking_mode", "high");
    replace_text(&mut cx, &form, "seed", "7");
    click_check(&mut cx, &form, "regional_edit");
    assert_eq!(stored(&state, &cx, "thinking_mode"), Some(json!("high")));
    assert_eq!(stored(&state, &cx, "seed"), Some(json!(7)));
    assert!(stored(&state, &cx, "seed").unwrap().is_i64());
    assert_eq!(stored(&state, &cx, "regional_edit"), Some(json!(true)));
    let keys: Arc<dyn crate::net::KeyResolver> =
        Arc::new(MapKeys::default().insert(ProviderId::Krea, "krea-key"));
    let validation = cx.new(|cx| Validation::new(state.clone(), keys, cx));
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    assert!(validation.read_with(&cx, |validation, cx| {
        validation
            .messages(cx)
            .iter()
            .any(|message| message.contains("reference image"))
    }));
    state.update(&mut cx, |app, cx| app.set_image(source_image(), cx));
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));

    replace_text(&mut cx, &form, "seed", "");
    assert_eq!(stored(&state, &cx, "seed"), None);
    replace_text(&mut cx, &form, "seed", "9007199254740993");
    let seed = stored(&state, &cx, "seed").unwrap();
    assert_eq!(seed.as_i64(), Some(9007199254740993));
    assert!(seed.is_i64());
}

#[gpui::test]
fn krea3_blank_aspect_removes_the_key(cx: &mut TestAppContext) {
    let (state, form, mut cx) = mount(cx);
    state.update(&mut cx, |app, cx| app.set_model("krea-3", cx));
    choose(&mut cx, &form, "aspect_ratio", "4:3");
    assert_eq!(stored(&state, &cx, "aspect_ratio"), Some(json!("4:3")));
    choose(&mut cx, &form, "aspect_ratio", "");
    assert_eq!(stored(&state, &cx, "aspect_ratio"), None);
    let options = form.read_with(&cx, |form, cx| {
        form.select("resolution")
            .unwrap()
            .read(cx)
            .options()
            .iter()
            .map(|option| option.to_string())
            .collect::<Vec<_>>()
    });
    assert_eq!(options, ["1K", "2K"]);
}

#[gpui::test]
fn flux2_width_seed_and_safety_follow_the_schema(cx: &mut TestAppContext) {
    let (state, form, mut cx) = mount(cx);
    state.update(&mut cx, |app, cx| {
        app.scene.prose = "a red square".into();
        app.set_model("flux-2-pro", cx);
    });
    assert_eq!(form.read_with(&cx, |form, cx| form.safety_max(cx)), 5.0);
    let width = stored(&state, &cx, "width").unwrap();
    assert_eq!(width.as_i64(), Some(0));
    assert!(width.is_i64());
    assert_eq!(
        form.read_with(&cx, |form, cx| form
            .number("width")
            .unwrap()
            .read(cx)
            .text(cx)),
        "0"
    );
    replace_text(&mut cx, &form, "seed", "7");
    replace_text(&mut cx, &form, "seed", "");
    assert_eq!(stored(&state, &cx, "seed"), None);
    assert_eq!(stored(&state, &cx, "width").unwrap().as_i64(), Some(0));

    replace_number(&mut cx, &form, "width", "100");
    let width = stored(&state, &cx, "width").unwrap();
    assert_eq!(width.as_i64(), Some(100));
    assert!(width.is_i64());
    let keys: Arc<dyn crate::net::KeyResolver> =
        Arc::new(MapKeys::default().insert(ProviderId::Bfl, "bfl-key"));
    let validation = cx.new(|cx| Validation::new(state, keys, cx));
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    assert!(validation.read_with(&cx, |validation, cx| {
        validation
            .messages(cx)
            .iter()
            .any(|message| message.contains("multiple of 16"))
    }));
}

#[gpui::test]
fn flux2_user_clears_and_stores_text(cx: &mut TestAppContext) {
    let (state, form, mut cx) = mount(cx);
    state.update(&mut cx, |app, cx| app.set_model("flux-2-pro", cx));
    assert_eq!(stored(&state, &cx, "user"), None);
    replace_text(&mut cx, &form, "user", "ada");
    assert_eq!(stored(&state, &cx, "user"), Some(json!("ada")));
    replace_text(&mut cx, &form, "user", "");
    assert_eq!(stored(&state, &cx, "user"), None);
}

#[gpui::test]
fn switching_models_keeps_seed_and_restores_flux3(cx: &mut TestAppContext) {
    let (state, form, mut cx) = mount(cx);
    state.update(&mut cx, |app, cx| app.set_model("krea-3", cx));
    replace_text(&mut cx, &form, "seed", "7");
    state.update(&mut cx, |app, cx| app.set_model("flux-2-pro", cx));
    assert_eq!(stored(&state, &cx, "seed"), Some(json!(7)));
    assert_eq!(stored(&state, &cx, "width").unwrap().as_i64(), Some(0));
    assert!(form.read_with(&cx, |form, _| !form.has_control("thinking_mode")));
    state.update(&mut cx, |app, cx| app.set_model("flux-3-image", cx));
    let (read_only, version, aspect_enabled) = form.read_with(&cx, |form, cx| {
        (
            form.version_is_read_only(cx),
            form.value("version", cx),
            form.aspect_enabled(cx),
        )
    });
    assert!(read_only);
    assert_eq!(version, json!("latest"));
    assert!(aspect_enabled);
}

#[gpui::test]
fn krea2_aspect_does_not_move_the_scene_until_flux3(cx: &mut TestAppContext) {
    let (state, form, mut cx) = mount(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("krea-2-large", cx);
        app.scene.aspect_ratio = Some("1:1".into());
    });
    choose(&mut cx, &form, "aspect_ratio", "4:3");
    assert_eq!(stored(&state, &cx, "aspect_ratio"), Some(json!("4:3")));
    assert_eq!(
        state.read_with(&cx, |app, _| app.scene.aspect_ratio.clone()),
        Some("1:1".into())
    );
    state.update(&mut cx, |app, cx| app.set_model("flux-3-image", cx));
    assert_eq!(
        state.read_with(&cx, |app, _| app.scene.aspect_ratio.clone()),
        Some("4:3".into())
    );
}

#[gpui::test]
fn krea2_wide_aspect_stays_off_the_scene_and_sliders_both_move(cx: &mut TestAppContext) {
    let (state, form, mut cx) = mount(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("krea-2-large", cx);
        app.scene.aspect_ratio = Some("1:1".into());
    });
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    choose(&mut cx, &form, "aspect_ratio", "2.35:1");
    assert_eq!(stored(&state, &cx, "aspect_ratio"), Some(json!("2.35:1")));
    assert_eq!(
        state.read_with(&cx, |app, _| app.scene.aspect_ratio.clone()),
        Some("1:1".into())
    );
    drag_slider(&mut cx, &form, "intensity", 1.0);
    drag_slider(&mut cx, &form, "complexity", 0.0);
    let intensity = stored(&state, &cx, "intensity").unwrap();
    let complexity = stored(&state, &cx, "complexity").unwrap();
    assert!(intensity.is_i64());
    assert!(complexity.is_i64());
    assert_ne!(intensity, complexity);
}

#[gpui::test]
fn flux3_aspect_writes_the_scene_frame(cx: &mut TestAppContext) {
    let (state, form, mut cx) = mount(cx);
    choose(&mut cx, &form, "aspect_ratio", "16:9");
    assert_eq!(
        state.read_with(&cx, |app, _| app.scene.aspect_ratio.clone()),
        Some("16:9".into())
    );
    assert_eq!(
        state.read_with(&cx, |app, _| app.scene.frame()),
        Some(Size { w: 1000, h: 562 })
    );
}

#[gpui::test]
fn field_help_is_shown_once(cx: &mut TestAppContext) {
    let (_state, form, cx) = mount(cx);
    let (rows, caption) = form.read_with(&cx, |form, cx| {
        (
            form.presented().to_vec(),
            form.control_caption("grounding", cx),
        )
    });
    assert_eq!(
        rows.iter()
            .filter(|(label, _)| label == "Grounding")
            .count(),
        1
    );
    assert!(rows.iter().any(|(label, help)| {
        label == "Grounding"
            && help
                .as_deref()
                .is_some_and(|text| text.contains("web search"))
    }));
    assert_eq!(caption, (None, None));
}

#[gpui::test]
fn repeated_fields_stay_plain_and_scalars_do_not(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    let form = cx.new(|cx| SchemaForm::new(state.clone(), cx));
    for id in ["flux-3-image", "flux-2-pro", "krea-3", "krea-2-large"] {
        state.update(&mut cx, |app, cx| {
            if app.model_id != id {
                app.set_model(id, cx);
            }
        });
        let fields = state.read_with(&cx, |app, _| app.spec().schema.fields.clone());
        form.read_with(&cx, |form, _| {
            for field in &fields {
                let repeated = matches!(field.kind, FieldKind::Repeated { .. });
                assert_eq!(form.is_repeated(&field.key), repeated, "{}", field.key);
                assert!(form.has_control(&field.key), "{}", field.key);
            }
        });
    }
}

fn replace_row_text(
    cx: &mut VisualTestContext,
    form: &gpui::Entity<SchemaForm>,
    key: &str,
    index: usize,
    child: &str,
    text: &str,
) {
    let input = form.read_with(cx, |form, _| {
        form.row_text(key, index, child).expect(child).clone()
    });
    cx.update(|window, cx| input.focus_handle(cx).focus(window, cx));
    cx.simulate_keystrokes("ctrl-a backspace");
    if !text.is_empty() {
        cx.simulate_input(text);
    }
}

fn replace_row_number(
    cx: &mut VisualTestContext,
    form: &gpui::Entity<SchemaForm>,
    key: &str,
    index: usize,
    child: &str,
    text: &str,
) {
    let input = form.read_with(cx, |form, _| {
        form.row_number(key, index, child).expect(child).clone()
    });
    cx.update(|window, cx| input.focus_handle(cx).focus(window, cx));
    cx.simulate_keystrokes("ctrl-a backspace");
    if !text.is_empty() {
        cx.simulate_input(text);
    }
    cx.update(|window, cx| {
        window.blur(cx);
        window.draw(cx).clear(cx);
    });
}

#[gpui::test]
fn krea2_styles_can_be_added_edited_and_removed(cx: &mut TestAppContext) {
    let (state, form, mut cx) = mount(cx);
    state.update(&mut cx, |app, cx| app.set_model("krea-2-large", cx));
    form.update(&mut cx, |form, cx| {
        form.add_repeated("styles", cx);
        form.add_repeated("styles", cx);
    });
    replace_row_text(&mut cx, &form, "styles", 0, "id", "style-a");
    replace_row_number(&mut cx, &form, "styles", 0, "strength", "0.8");
    replace_row_text(&mut cx, &form, "styles", 1, "id", "style-b");
    replace_row_number(&mut cx, &form, "styles", 1, "strength", "-0.4");
    let styles = stored(&state, &cx, "styles").unwrap();
    let rows = styles.as_array().unwrap();
    assert_eq!(rows[0]["id"], json!("style-a"));
    assert_eq!(rows[1]["id"], json!("style-b"));
    assert!((rows[0]["strength"].as_f64().unwrap() - 0.8).abs() < 1e-6);
    assert!((rows[1]["strength"].as_f64().unwrap() - -0.4).abs() < 1e-6);
    form.update(&mut cx, |form, cx| form.remove_repeated("styles", 0, cx));
    let styles = stored(&state, &cx, "styles").unwrap();
    let rows = styles.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], json!("style-b"));
    assert!((rows[0]["strength"].as_f64().unwrap() - -0.4).abs() < 1e-6);
}

#[gpui::test]
fn repeated_add_stops_at_the_schema_maximum(cx: &mut TestAppContext) {
    let (state, form, mut cx) = mount(cx);
    state.update(&mut cx, |app, cx| app.set_model("krea-2-large", cx));
    let max = state.read_with(&cx, |app, _| {
        app.spec()
            .schema
            .fields
            .iter()
            .find(|field| field.key == "styles")
            .and_then(|field| match field.kind {
                FieldKind::Repeated { max, .. } => Some(max),
                _ => None,
            })
            .unwrap()
    });
    form.update(&mut cx, |form, cx| {
        for _ in 0..max {
            form.add_repeated("styles", cx);
        }
        form.add_repeated("styles", cx);
    });
    assert_eq!(
        form.read_with(&cx, |form, _| form.repeated_len("styles")),
        max
    );
    assert_eq!(
        stored(&state, &cx, "styles")
            .unwrap()
            .as_array()
            .unwrap()
            .len(),
        max
    );
}

#[gpui::test]
fn a_bad_moodboard_id_blocks_generate(cx: &mut TestAppContext) {
    let (state, form, mut cx) = mount(cx);
    state.update(&mut cx, |app, cx| {
        app.scene.prose = "a red square".into();
        app.set_model("krea-2-large", cx);
    });
    choose(&mut cx, &form, "aspect_ratio", "1:1");
    form.update(&mut cx, |form, cx| form.add_repeated("moodboards", cx));
    replace_row_text(&mut cx, &form, "moodboards", 0, "id", "not-a-uuid");
    let keys: Arc<dyn crate::net::KeyResolver> =
        Arc::new(MapKeys::default().insert(ProviderId::Krea, "krea-key"));
    let validation = cx.new(|cx| Validation::new(state.clone(), keys, cx));
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    assert!(validation.read_with(&cx, |validation, cx| {
        validation
            .messages(cx)
            .iter()
            .any(|message| message.contains("UUID"))
    }));
    replace_row_text(
        &mut cx,
        &form,
        "moodboards",
        0,
        "id",
        "11111111-1111-4111-8111-111111111111",
    );
    assert!(validation.read_with(&cx, |validation, cx| {
        !validation
            .messages(cx)
            .iter()
            .any(|message| message.contains("UUID"))
    }));
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
}

#[gpui::test]
fn empty_style_rows_do_not_block_generate(cx: &mut TestAppContext) {
    let (state, form, mut cx) = mount(cx);
    state.update(&mut cx, |app, cx| {
        app.scene.prose = "a red square".into();
        app.set_model("krea-2-large", cx);
    });
    let styles = stored(&state, &cx, "styles").unwrap();
    assert_eq!(styles.as_array().map(Vec::len), Some(0));
    let keys: Arc<dyn crate::net::KeyResolver> =
        Arc::new(MapKeys::default().insert(ProviderId::Krea, "krea-key"));
    let validation = cx.new(|cx| Validation::new(state.clone(), keys, cx));
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    choose(&mut cx, &form, "aspect_ratio", "3:2");
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
}

#[gpui::test]
fn source_strength_stays_visible_without_an_image(cx: &mut TestAppContext) {
    let (state, form, mut cx) = mount(cx);
    state.update(&mut cx, |app, cx| app.set_model("krea-2-large", cx));
    assert!(state.read_with(&cx, |app, _| app.scene.source_image.is_none()));
    let (shown, text) = form.read_with(&cx, |form, cx| {
        (
            form.has_control("strength"),
            form.number("strength").unwrap().read(cx).text(cx),
        )
    });
    assert!(shown);
    assert_eq!(text, "0.99");
}

pub(crate) fn source_image() -> Arc<crate::image_load::DecodedImage> {
    let mut image = crate::image_load::decode_image(&png_bytes(), "pixel.png").unwrap();
    image.size = easel_core::geometry::Size { w: 64, h: 64 };
    Arc::new(image)
}

#[gpui::test]
fn paste_sets_the_source_then_adds_a_reference(cx: &mut TestAppContext) {
    let window = cx.update(|cx| {
        crate::actions::bind_keys(cx);
        cx.open_window(gpui::WindowOptions::default(), |window, cx| {
            let root = cx.new(crate::root::Root::new);
            root.focus_handle(cx).focus(window, cx);
            root
        })
        .unwrap()
    });
    let mut cx = VisualTestContext::from_window(window.into(), cx);
    let root = window.root(&mut cx).unwrap();
    cx.update(|_window, cx| {
        cx.write_to_clipboard(ClipboardItem::new_image(&Image {
            format: ImageFormat::Png,
            bytes: png_bytes(),
            id: 1,
        }));
    });
    cx.simulate_keystrokes("ctrl-v");
    cx.run_until_parked();
    assert!(root.read_with(&cx, |root, cx| {
        root.app_state.read(cx).scene.source_image.is_some()
    }));
    cx.simulate_keystrokes("ctrl-v");
    cx.run_until_parked();
    assert_eq!(
        root.read_with(&cx, |root, cx| root
            .app_state
            .read(cx)
            .scene
            .references
            .len()),
        1
    );
}

#[gpui::test]
fn the_editor_does_not_list_the_images(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    let keys = std::sync::Arc::new(crate::settings::SharedKeys::default());
    let panel = cx.new(|cx| crate::side_panel::SidePanel::new(state, keys, None, None, cx));
    assert!(!panel.read_with(&cx, |panel, _| panel.shows_reference_section()));
}

fn openai_validation(
    cx: &mut VisualTestContext,
    state: gpui::Entity<AppState>,
) -> gpui::Entity<Validation> {
    let keys: Arc<dyn crate::net::KeyResolver> =
        Arc::new(MapKeys::default().insert(ProviderId::OpenAi, "openai-key"));
    cx.new(|cx| Validation::new(state, keys, cx))
}

fn decoded(buffer: image::RgbaImage) -> Arc<crate::image_load::DecodedImage> {
    let size = easel_core::geometry::Size {
        w: buffer.width(),
        h: buffer.height(),
    };
    let rgba_for_export = Arc::new(image::DynamicImage::ImageRgba8(buffer.clone()));
    let mut rgba = buffer;
    for pixel in rgba.pixels_mut() {
        pixel.0.swap(0, 2);
    }
    Arc::new(crate::image_load::DecodedImage {
        size,
        render: Arc::new(gpui::RenderImage::new([image::Frame::new(rgba)])),
        rgba_for_export,
    })
}

#[gpui::test]
fn openai_caps_disable_generate(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("gpt-image-2", cx);
        app.scene.prose = "a".repeat(32_001);
    });
    let validation = openai_validation(&mut cx, state.clone());
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    assert!(validation.read_with(&cx, |validation, cx| {
        validation
            .messages(cx)
            .iter()
            .any(|message| message.contains("32000"))
    }));

    state.update(&mut cx, |app, cx| {
        app.scene.prose = "a harbor".into();
        app.set_image(source_image(), cx);
        for _ in 0..16 {
            app.scene
                .add_reference(easel_core::scene::ImageRef::new("ref", 64, 64));
            app.extras.push(source_image());
        }
        cx.notify();
    });
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages.iter().any(|message| message.contains("16")),
        "{messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|message| message.contains("will be sent"))
    );
    assert!(validation.read_with(&cx, |validation, cx| validation.send_notice(cx).is_none()));
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
}

#[gpui::test]
fn an_oversized_data_url_disables_generate(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    let mut buffer = image::RgbaImage::new(2000, 2000);
    let mut noise = 0x1234_5678u32;
    for pixel in buffer.pixels_mut() {
        noise = noise.wrapping_mul(1664525).wrapping_add(1013904223);
        *pixel = image::Rgba([noise as u8, (noise >> 8) as u8, (noise >> 16) as u8, 128]);
    }
    let image = decoded(buffer);
    state.update(&mut cx, |app, cx| {
        app.set_model("gpt-image-2.5-flare", cx);
        app.scene.prose = "a harbor".into();
        app.set_image(image, cx);
    });
    let validation = openai_validation(&mut cx, state.clone());
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages.iter().any(|message| message.contains("20971520")),
        "{messages:?}"
    );
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    state.update(&mut cx, |app, cx| app.set_model("gpt-image-2", cx));
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages.iter().any(|message| message.contains("20971520")),
        "{messages:?}"
    );
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
}

#[gpui::test]
fn openai_size_stays_on_the_four_megapixel_prep(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    let image = decoded(image::RgbaImage::from_pixel(
        2100,
        2100,
        image::Rgba([20, 40, 60, 255]),
    ));
    state.update(&mut cx, |app, cx| {
        app.set_model("gpt-image-2.5-flare", cx);
        app.scene.prose = "a harbor".into();
        app.values.insert("size".into(), json!("1536x1024"));
        app.set_image(image, cx);
    });
    let pixels = state.read_with(&cx, |app, _| {
        let request = crate::jobs::build_request(app).expect("request");
        let image = &request.images[0];
        u64::from(image.width) * u64::from(image.height)
    });
    assert!(pixels <= 4_000_000, "{pixels}");
}

#[gpui::test]
fn fidelity_stays_hidden_without_an_image_and_keeps_its_value(cx: &mut TestAppContext) {
    let (state, form, mut cx) = mount(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("gpt-image-1.5", cx);
        app.values.insert("input_fidelity".into(), json!("high"));
    });
    assert!(!form.read_with(&cx, |form, _| form.has_control("input_fidelity")));
    state.update(&mut cx, |app, cx| app.set_image(source_image(), cx));
    assert!(form.read_with(&cx, |form, _| form.has_control("input_fidelity")));
    assert_eq!(
        form.read_with(&cx, |form, cx| {
            form.select("input_fidelity")
                .unwrap()
                .read(cx)
                .value()
                .clone()
        }),
        json!("high")
    );
    state.update(&mut cx, |app, cx| app.remove_reference_slot(0, cx));
    assert!(!form.read_with(&cx, |form, _| form.has_control("input_fidelity")));
    assert_eq!(
        state.read_with(&cx, |app, _| app.values.get("input_fidelity").cloned()),
        Some(json!("high"))
    );

    state.update(&mut cx, |app, cx| {
        app.values.remove("input_fidelity");
        app.set_image(source_image(), cx);
    });
    assert_eq!(
        form.read_with(&cx, |form, cx| {
            form.select("input_fidelity")
                .unwrap()
                .read(cx)
                .value()
                .clone()
        }),
        json!("low")
    );
}

fn xai_validation(
    cx: &mut VisualTestContext,
    state: gpui::Entity<AppState>,
) -> gpui::Entity<Validation> {
    let keys: Arc<dyn crate::net::KeyResolver> =
        Arc::new(MapKeys::default().insert(ProviderId::Xai, "xai-key"));
    cx.new(|cx| Validation::new(state, keys, cx))
}

fn chutes_validation(
    cx: &mut VisualTestContext,
    state: gpui::Entity<AppState>,
) -> gpui::Entity<Validation> {
    let keys: Arc<dyn crate::net::KeyResolver> =
        Arc::new(MapKeys::default().insert(ProviderId::Chutes, "chutes-key"));
    cx.new(|cx| Validation::new(state, keys, cx))
}

fn chutes_submit(app: &AppState) -> easel_core::http::HttpRequest {
    use easel_core::providers::ReferenceStep;

    let provider = easel_core::providers::resolve_model(&app.model_id).expect("provider");
    let request = crate::jobs::build_request(app).expect("request");
    let images = provider
        .reference_steps(&request.images)
        .into_iter()
        .filter_map(|step| match step {
            ReferenceStep::Inline(image) => Some(image),
            ReferenceStep::Upload { .. } => None,
        })
        .collect::<Vec<_>>();
    provider.build_submit(&request, &images)
}

fn chutes_image() -> Arc<crate::image_load::DecodedImage> {
    decoded(image::RgbaImage::from_pixel(
        256,
        256,
        image::Rgba([20, 40, 60, 255]),
    ))
}

fn attach_chutes_prepared(app: &mut AppState, cx: &mut gpui::Context<AppState>, extras: usize) {
    app.set_image(chutes_image(), cx);
    for _ in 0..extras {
        app.scene
            .add_reference(easel_core::scene::ImageRef::new("ref", 256, 256));
        app.extras.push(chutes_image());
    }
}

fn select_view(
    form: &gpui::Entity<SchemaForm>,
    cx: &VisualTestContext,
    key: &str,
) -> (Vec<String>, String) {
    form.read_with(cx, |form, cx| {
        let select = form.select(key).expect(key).read(cx);
        let options = select
            .options()
            .iter()
            .map(|option| option.to_string())
            .collect();
        let value = select.value().as_str().unwrap_or("").to_owned();
        (options, value)
    })
}

fn flux_submit(app: &AppState) -> easel_core::http::HttpRequest {
    use easel_core::providers::{Provider, ReferenceStep};

    let request = crate::jobs::build_request(app).expect("request");
    let images = easel_core::providers::bfl::flux3::Flux3
        .reference_steps(&request.images)
        .into_iter()
        .filter_map(|step| match step {
            ReferenceStep::Inline(image) => Some(image),
            ReferenceStep::Upload { .. } => None,
        })
        .collect::<Vec<_>>();
    easel_core::providers::bfl::flux3::Flux3.build_submit(&request, &images)
}

fn attach_prepared(app: &mut AppState, cx: &mut gpui::Context<AppState>, extras: usize) {
    app.set_image(source_image(), cx);
    for _ in 0..extras {
        app.scene
            .add_reference(easel_core::scene::ImageRef::new("ref", 64, 64));
        app.extras.push(source_image());
    }
}

#[gpui::test]
fn xai_form_starts_at_auto_1k_and_auto(cx: &mut TestAppContext) {
    let (state, form, mut cx) = mount(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("grok-imagine-image-2.0", cx);
    });
    let (aspects, aspect) = select_view(&form, &cx, "aspect_ratio");
    let (resolutions, resolution) = select_view(&form, &cx, "resolution");
    let (qualities, quality) = select_view(&form, &cx, "quality");
    assert_eq!(
        aspects,
        [
            "auto", "1:1", "3:4", "4:3", "9:16", "16:9", "2:3", "3:2", "9:19.5", "19.5:9", "9:20",
            "20:9", "1:2", "2:1", "21:9", "5:2",
        ]
    );
    assert_eq!(resolutions, ["1k", "1.5k", "2k"]);
    assert_eq!(qualities, ["low", "medium", "auto"]);
    assert_eq!(aspect, "auto");
    assert_eq!(resolution, "1k");
    assert_eq!(quality, "auto");
}

#[gpui::test]
fn xai_five_images_generate_and_six_refuse(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("grok-imagine-image-2.0", cx);
        app.scene.prose = "a harbor".into();
        attach_prepared(app, cx, 4);
    });
    let validation = xai_validation(&mut cx, state.clone());
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));

    state.update(&mut cx, |app, cx| {
        app.scene
            .add_reference(easel_core::scene::ImageRef::new("ref", 64, 64));
        app.extras.push(source_image());
        cx.notify();
    });
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages
            .iter()
            .any(|message| message == "At most 5 images."),
        "{messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|message| message.contains("will be sent"))
    );
    assert!(validation.read_with(&cx, |validation, cx| validation.send_notice(cx).is_none()));
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
}

#[gpui::test]
fn xai_empty_prompt_disables_generate(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("grok-imagine-image-2.0", cx);
        app.scene.prose.clear();
    });
    let validation = xai_validation(&mut cx, state);
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages.iter().any(|message| message == "Prompt is empty"),
        "{messages:?}"
    );
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
}

#[gpui::test]
fn switching_to_xai_keeps_only_listed_values(cx: &mut TestAppContext) {
    let (state, form, mut cx) = mount(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("gemini-nano-banana-2.1", cx);
        app.values.insert("resolution".into(), json!("2K"));
        app.values.insert("quality".into(), json!("low"));
        app.values.insert("aspect_ratio".into(), json!("16:9"));
        app.set_model("grok-imagine-image-2.0", cx);
    });
    assert_eq!(select_view(&form, &cx, "resolution").1, "1k");
    assert_eq!(select_view(&form, &cx, "quality").1, "low");
    assert_eq!(select_view(&form, &cx, "aspect_ratio").1, "16:9");
    assert_eq!(stored(&state, &cx, "resolution"), Some(json!("1k")));
    assert_eq!(stored(&state, &cx, "quality"), Some(json!("low")));
    assert_eq!(stored(&state, &cx, "aspect_ratio"), Some(json!("16:9")));

    state.update(&mut cx, |app, cx| {
        app.set_model("gpt-image-2.5-flare", cx);
        app.values.insert("quality".into(), json!("max"));
        app.set_model("grok-imagine-image-2.0", cx);
    });
    assert_eq!(select_view(&form, &cx, "quality").1, "auto");
    assert_eq!(stored(&state, &cx, "quality"), Some(json!("auto")));
}

#[gpui::test]
fn switching_back_to_flux_keeps_the_prompt_canvas_and_request(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    let image = decoded(image::RgbaImage::from_pixel(
        256,
        256,
        image::Rgba([20, 40, 60, 255]),
    ));
    state.update(&mut cx, |app, cx| {
        app.scene.prose = "keep this prompt".into();
        app.set_image(image, cx);
    });
    let before = state.read_with(&cx, |app, _| flux_submit(app));
    state.update(&mut cx, |app, cx| {
        app.set_model("grok-imagine-image-2.0", cx);
        app.set_model("flux-3-image", cx);
    });
    let (prose, canvas, after) = state.read_with(&cx, |app, _| {
        (
            app.scene.prose.clone(),
            app.image.is_some() && app.scene.source_image.is_some(),
            flux_submit(app),
        )
    });
    assert_eq!(prose, "keep this prompt");
    assert!(canvas);
    assert_eq!(after, before);
}

fn meta_validation(
    cx: &mut VisualTestContext,
    state: gpui::Entity<AppState>,
) -> gpui::Entity<Validation> {
    let keys: Arc<dyn crate::net::KeyResolver> =
        Arc::new(MapKeys::default().insert(ProviderId::Meta, "meta-key"));
    cx.new(|cx| Validation::new(state, keys, cx))
}

fn check_value(form: &gpui::Entity<SchemaForm>, cx: &VisualTestContext, key: &str) -> bool {
    form.read_with(cx, |form, cx| {
        form.check(key)
            .expect(key)
            .read(cx)
            .value()
            .as_bool()
            .unwrap()
    })
}

#[gpui::test]
fn meta_form_starts_at_auto_webp_high_and_three_switches(cx: &mut TestAppContext) {
    let (state, form, mut cx) = mount(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("muse-image-1.0", cx);
    });
    cx.update(|window, cx| {
        window.draw(cx).clear(cx);
    });
    let (sizes, size) = select_view(&form, &cx, "size");
    let (formats, format) = select_view(&form, &cx, "output_format");
    let (strengths, strength) = select_view(&form, &cx, "reasoning_strength");
    assert_eq!(
        sizes,
        [
            "auto",
            "1024x1024",
            "1536x1024",
            "1024x1536",
            "1792x1024",
            "1024x1792",
        ]
    );
    assert_eq!(formats, ["webp", "png", "jpeg"]);
    assert_eq!(strengths, ["high", "low"]);
    assert_eq!(size, "auto");
    assert_eq!(format, "webp");
    assert_eq!(strength, "high");
    assert!(check_value(&form, &cx, "enable_image_search"));
    assert!(check_value(&form, &cx, "enable_web_search"));
    assert!(check_value(&form, &cx, "enable_shell"));
    let labels = form.read_with(&cx, |form, _| {
        form.presented()
            .iter()
            .map(|(label, _)| label.clone())
            .collect::<Vec<_>>()
    });
    assert_eq!(
        labels,
        [
            "Size".to_string(),
            "Output format".to_string(),
            "Reasoning strength".to_string(),
            "Image search".to_string(),
            "Web search".to_string(),
            "Code execution".to_string(),
        ]
    );
}

#[gpui::test]
fn meta_ten_images_generate_and_eleven_refuse(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("muse-image-1.0", cx);
        app.scene.prose = "a harbor".into();
        attach_prepared(app, cx, 9);
    });
    let validation = meta_validation(&mut cx, state.clone());
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));

    state.update(&mut cx, |app, cx| {
        app.scene
            .add_reference(easel_core::scene::ImageRef::new("ref", 64, 64));
        app.extras.push(source_image());
        cx.notify();
    });
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages
            .iter()
            .any(|message| message == "At most 10 images."),
        "{messages:?}"
    );
    assert!(
        !messages
            .iter()
            .any(|message| message.contains("will be sent"))
    );
    assert!(validation.read_with(&cx, |validation, cx| validation.send_notice(cx).is_none()));
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
}

#[gpui::test]
fn meta_empty_prompt_disables_generate(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("muse-image-1.0", cx);
        app.scene.prose.clear();
    });
    let validation = meta_validation(&mut cx, state);
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages.iter().any(|message| message == "Prompt is empty"),
        "{messages:?}"
    );
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
}

#[gpui::test]
fn switching_to_meta_keeps_listed_sizes_and_formats(cx: &mut TestAppContext) {
    let (state, _form, mut cx) = mount(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("gpt-image-2.5-flare", cx);
        app.values.insert("size".into(), json!("1536x1024"));
        app.set_model("muse-image-1.0", cx);
    });
    assert_eq!(stored(&state, &cx, "size"), Some(json!("1536x1024")));

    state.update(&mut cx, |app, cx| {
        app.set_model("gpt-image-2.5-flare", cx);
        app.values.insert("size".into(), json!("1024x1024"));
        app.set_model("muse-image-1.0", cx);
    });
    assert_eq!(stored(&state, &cx, "size"), Some(json!("auto")));

    state.update(&mut cx, |app, cx| {
        app.set_model("apiframe-seedream-4", cx);
        app.values.insert("size".into(), json!("4K"));
        app.set_model("muse-image-1.0", cx);
    });
    assert_eq!(stored(&state, &cx, "size"), Some(json!("auto")));

    state.update(&mut cx, |app, cx| {
        app.set_model("leonardo-seedream-5.0-pro", cx);
        app.values.insert("output_format".into(), json!("png"));
        app.set_model("muse-image-1.0", cx);
    });
    assert_eq!(stored(&state, &cx, "output_format"), Some(json!("png")));

    state.update(&mut cx, |app, cx| {
        app.set_model("leonardo-seedream-5.0-pro", cx);
        app.values.insert("output_format".into(), json!("jpeg"));
        app.set_model("muse-image-1.0", cx);
    });
    assert_eq!(stored(&state, &cx, "output_format"), Some(json!("webp")));
}

#[gpui::test]
fn switching_back_to_flux_from_meta_keeps_the_prompt_canvas_and_request(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    let image = decoded(image::RgbaImage::from_pixel(
        256,
        256,
        image::Rgba([20, 40, 60, 255]),
    ));
    state.update(&mut cx, |app, cx| {
        app.scene.prose = "keep this prompt".into();
        app.set_image(image, cx);
    });
    let before = state.read_with(&cx, |app, _| flux_submit(app));
    state.update(&mut cx, |app, cx| {
        app.set_model("muse-image-1.0", cx);
        app.set_model("flux-3-image", cx);
    });
    let (prose, canvas, after) = state.read_with(&cx, |app, _| {
        (
            app.scene.prose.clone(),
            app.image.is_some() && app.scene.source_image.is_some(),
            flux_submit(app),
        )
    });
    assert_eq!(prose, "keep this prompt");
    assert!(canvas);
    assert_eq!(after, before);
}

#[gpui::test]
fn meta_switches_reach_the_request_body(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("muse-image-1.0", cx);
        app.scene.prose = "a harbor".into();
        app.values.insert("enable_web_search".into(), json!(false));
        app.values.insert("enable_shell".into(), json!(false));
    });
    let http = state.read_with(&cx, |app, _| {
        let provider = easel_core::providers::resolve_model(&app.model_id).expect("provider");
        let request = crate::jobs::build_request(app).expect("request");
        provider.build_submit(&request, &[])
    });
    let easel_core::http::Body::Json(json) = http.body else {
        panic!("expected json");
    };
    assert_eq!(
        json["tool_enablement"],
        json!({
            "enable_image_search": true,
            "enable_web_search": false,
            "enable_shell": false
        })
    );
}

#[gpui::test]
fn a_loaded_picture_on_flux_schnell_generates_with_a_count_notice(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("chutes-flux-schnell", cx);
        app.scene.prose = "a harbor".into();
        app.set_image(chutes_image(), cx);
    });
    let validation = chutes_validation(&mut cx, state.clone());
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    assert_eq!(
        validation.read_with(&cx, |validation, cx| validation.send_notice(cx)),
        Some("0 of 1 images will be sent".to_string())
    );
    let body = state.read_with(&cx, |app, _| chutes_submit(app));
    let easel_core::http::Body::Json(json) = body.body else {
        panic!("expected json");
    };
    assert!(json.get("image_b64s").is_none(), "{json}");
    assert!(json.get("image_url").is_none(), "{json}");
    assert!(json.get("images").is_none(), "{json}");
}

#[gpui::test]
fn four_pictures_under_qwen_edit_keep_generate_and_send_three(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("gemini-nano-banana-2.1", cx);
        app.scene.prose = "combine them".into();
        attach_chutes_prepared(app, cx, 3);
    });
    state.update(&mut cx, |app, cx| {
        app.set_model("chutes-qwen-image-edit-2511", cx)
    });
    let validation = chutes_validation(&mut cx, state.clone());
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    assert_eq!(
        validation.read_with(&cx, |validation, cx| validation.send_notice(cx)),
        Some("3 of 4 images will be sent".to_string())
    );
    let body = state.read_with(&cx, |app, _| chutes_submit(app));
    let easel_core::http::Body::Json(json) = body.body else {
        panic!("expected json");
    };
    let refs = json["image_b64s"].as_array().expect("image_b64s");
    assert_eq!(refs.len(), 3);
}

#[gpui::test]
fn qwen_edit_with_nothing_loaded_blocks_generate(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("chutes-qwen-image-edit-2511", cx);
        app.scene.prose = "make it blue".into();
    });
    let validation = chutes_validation(&mut cx, state);
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages
            .iter()
            .any(|message| message == "Add at least one image."),
        "{messages:?}"
    );
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
}

#[gpui::test]
fn a_missing_chutes_key_disables_generate(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("chutes-flux-schnell", cx);
        app.scene.prose = "a harbor".into();
    });
    let keys: Arc<dyn crate::net::KeyResolver> = Arc::new(MapKeys::default());
    let validation = cx.new(|cx| Validation::new(state.clone(), keys, cx));
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages
            .iter()
            .any(|message| message == "No Chutes API key — open Settings"),
        "{messages:?}"
    );
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    let validation = chutes_validation(&mut cx, state);
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
}

#[gpui::test]
fn switching_z_image_to_flux_schnell_carries_and_drops_the_right_values(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("chutes-z-image-turbo", cx);
        app.scene.prose = "a lighthouse".into();
        app.values.insert("guidance_scale".into(), json!(0.0));
        app.values.insert("width".into(), json!(1024));
        app.set_image(source_image(), cx);
    });
    assert_eq!(stored(&state, &cx, "guidance_scale"), Some(json!(0.0)));
    state.update(&mut cx, |app, cx| app.set_model("chutes-flux-schnell", cx));
    let (guidance, width, prose, has_image) = state.read_with(&cx, |app, _| {
        (
            app.values.get("guidance_scale").cloned(),
            app.values.get("width").cloned(),
            app.scene.prose.clone(),
            app.image.is_some(),
        )
    });
    assert_eq!(guidance, Some(json!(3.5)));
    assert_eq!(width, Some(json!(1024)));
    assert_eq!(prose, "a lighthouse");
    assert!(has_image);
    let validation = chutes_validation(&mut cx, state);
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    assert_eq!(
        validation.read_with(&cx, |validation, cx| validation.send_notice(cx)),
        Some("0 of 1 images will be sent".to_string())
    );
}

#[gpui::test]
fn a_none_slot_model_refuses_further_pictures(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("chutes-flux-schnell", cx);
        app.scene.prose = "a harbor".into();
        app.set_image(source_image(), cx);
    });
    let refs = cx.new(|cx| References::new(state.clone(), cx));
    refs.update(&mut cx, |refs, cx| refs.add_decoded(source_image(), cx));
    let message = refs.read_with(&cx, |refs, _| refs.refusal().unwrap_or_default());
    assert!(message.contains("at most 0 images"), "{message}");
    assert!(state.read_with(&cx, |app, _| app.image.is_some()));
    assert!(state.read_with(&cx, |app, _| app.scene.source_image.is_some()));
}

#[gpui::test]
fn flux3_still_posts_to_bfl_after_the_chutes_additions(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.scene.prose = "a red square".into();
        app.set_model("chutes-flux-schnell", cx);
        app.set_model("flux-3-image", cx);
    });
    let url = state.read_with(&cx, |app, _| flux_submit(app).url);
    assert!(url.starts_with("https://api.bfl.ai"), "{url}");
}

fn qwen_validation(
    cx: &mut VisualTestContext,
    state: gpui::Entity<AppState>,
) -> gpui::Entity<Validation> {
    let keys: Arc<dyn crate::net::KeyResolver> =
        Arc::new(MapKeys::default().insert(ProviderId::Qwen, "qwen-key"));
    cx.new(|cx| Validation::new(state, keys, cx))
}

fn qwen_validation_without_key(
    cx: &mut VisualTestContext,
    state: gpui::Entity<AppState>,
) -> gpui::Entity<Validation> {
    let keys: Arc<dyn crate::net::KeyResolver> = Arc::new(MapKeys::default());
    cx.new(|cx| Validation::new(state, keys, cx))
}

fn qwen_image() -> Arc<crate::image_load::DecodedImage> {
    decoded(image::RgbaImage::from_pixel(
        256,
        256,
        image::Rgba([20, 40, 60, 255]),
    ))
}

fn attach_qwen_prepared(app: &mut AppState, cx: &mut gpui::Context<AppState>, extras: usize) {
    app.set_image(qwen_image(), cx);
    for _ in 0..extras {
        app.scene
            .add_reference(easel_core::scene::ImageRef::new("ref", 256, 256));
        app.extras.push(qwen_image());
    }
}

fn qwen_submit(app: &AppState) -> easel_core::http::HttpRequest {
    use easel_core::providers::ReferenceStep;

    let provider = easel_core::providers::resolve_model(&app.model_id).expect("provider");
    let request = crate::jobs::build_request(app).expect("request");
    let images = provider
        .reference_steps(&request.images)
        .into_iter()
        .filter_map(|step| match step {
            ReferenceStep::Inline(image) => Some(image),
            ReferenceStep::Upload { .. } => None,
        })
        .collect::<Vec<_>>();
    provider.build_submit(&request, &images)
}

#[gpui::test]
fn the_qwen_form_starts_empty_and_the_negative_prompt_follows_the_model(cx: &mut TestAppContext) {
    let (state, form, mut cx) = mount(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("qwen-image-3.0-pro", cx);
    });
    assert!(form.read_with(&cx, |form, _| form.has_control("width")));
    assert!(form.read_with(&cx, |form, _| form.has_control("height")));
    assert!(form.read_with(&cx, |form, _| form.has_control("negative_prompt")));
    assert_eq!(stored(&state, &cx, "width"), None);
    assert_eq!(stored(&state, &cx, "height"), None);

    state.update(&mut cx, |app, cx| {
        app.set_model("qwen-image-2.1-pro", cx);
    });
    assert!(form.read_with(&cx, |form, _| form.has_control("width")));
    assert!(form.read_with(&cx, |form, _| form.has_control("height")));
    assert!(!form.read_with(&cx, |form, _| form.has_control("negative_prompt")));
}

#[gpui::test]
fn qwen_sizes_gate_generate(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("qwen-image-3.0-pro", cx);
        app.scene.prose = "a harbor".into();
    });
    let validation = qwen_validation(&mut cx, state.clone());
    let messages = |cx: &VisualTestContext, validation: &gpui::Entity<Validation>| {
        validation.read_with(cx, |validation, cx| validation.messages(cx))
    };
    let enabled = |cx: &VisualTestContext, validation: &gpui::Entity<Validation>| {
        validation.read_with(cx, |validation, cx| validation.generate_enabled(cx))
    };

    state.update(&mut cx, |app, _| {
        app.values.insert("width".into(), json!(1024));
    });
    assert!(
        !enabled(&cx, &validation),
        "{:?}",
        messages(&cx, &validation)
    );
    assert!(
        messages(&cx, &validation)
            .iter()
            .any(|message| *message == "Set both width and height, or leave both at 0.")
    );

    state.update(&mut cx, |app, _| {
        app.values.insert("height".into(), json!(256));
    });
    assert!(
        enabled(&cx, &validation),
        "{:?}",
        messages(&cx, &validation)
    );

    state.update(&mut cx, |app, _| {
        app.values.insert("width".into(), json!(100));
        app.values.insert("height".into(), json!(100));
    });
    assert!(!enabled(&cx, &validation));
    assert!(
        messages(&cx, &validation)
            .iter()
            .any(|message| *message == "Width times height must be from 262144 through 4194304.")
    );

    state.update(&mut cx, |app, _| {
        app.values.insert("width".into(), json!(2048));
        app.values.insert("height".into(), json!(200));
    });
    assert!(!enabled(&cx, &validation));
    assert!(
        messages(&cx, &validation)
            .iter()
            .any(|message| *message == "The long side must be at most 8 times the short side.")
    );

    state.update(&mut cx, |app, _| {
        app.values.insert("height".into(), json!(256));
    });
    assert!(
        enabled(&cx, &validation),
        "{:?}",
        messages(&cx, &validation)
    );
}

#[gpui::test]
fn four_pictures_on_the_small_qwen_cap_disable_generate(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("qwen-image-3.0-pro", cx);
        app.scene.prose = "a harbor".into();
        attach_qwen_prepared(app, cx, 3);
    });
    let validation = qwen_validation(&mut cx, state.clone());
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages
            .iter()
            .any(|message| message == "At most 3 images."),
        "{messages:?}"
    );
    assert!(validation.read_with(&cx, |validation, cx| validation.send_notice(cx).is_none()));

    state.update(&mut cx, |app, cx| {
        app.set_model("qwen-image-2.1-pro", cx);
    });
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        !messages.iter().any(|message| message.contains("images.")),
        "{messages:?}"
    );
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
}

#[gpui::test]
fn eleven_pictures_on_the_wide_qwen_cap_disable_generate(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("qwen-image-2.1-pro", cx);
        app.scene.prose = "a harbor".into();
        attach_qwen_prepared(app, cx, 9);
    });
    let validation = qwen_validation(&mut cx, state.clone());
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));

    state.update(&mut cx, |app, cx| {
        app.scene
            .add_reference(easel_core::scene::ImageRef::new("ref", 256, 256));
        app.extras.push(qwen_image());
        cx.notify();
    });
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages
            .iter()
            .any(|message| message == "At most 10 images."),
        "{messages:?}"
    );
    assert!(validation.read_with(&cx, |validation, cx| validation.send_notice(cx).is_none()));
}

#[gpui::test]
fn a_negative_prompt_stays_only_on_qwen_image_3_0_pro(cx: &mut TestAppContext) {
    let (state, form, mut cx) = mount(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("qwen-image-3.0-pro", cx);
        app.scene.prose = "keep this prompt".into();
        app.set_image(qwen_image(), cx);
    });
    replace_text(&mut cx, &form, "negative_prompt", "blurry");
    assert_eq!(
        stored(&state, &cx, "negative_prompt"),
        Some(json!("blurry"))
    );

    state.update(&mut cx, |app, cx| {
        app.set_model("qwen-image-2.1-pro", cx);
    });
    assert_eq!(stored(&state, &cx, "negative_prompt"), None);
    let (prose, canvas) =
        state.read_with(&cx, |app, _| (app.scene.prose.clone(), app.image.is_some()));
    assert_eq!(prose, "keep this prompt");
    assert!(canvas);
}

#[gpui::test]
fn qwen_image_gates_cover_the_encoded_size_ratio_and_missing_preparation(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    let mut buffer = image::RgbaImage::new(2000, 2000);
    let mut noise = 0x1234_5678u32;
    for pixel in buffer.pixels_mut() {
        noise = noise.wrapping_mul(1664525).wrapping_add(1013904223);
        *pixel = image::Rgba([noise as u8, (noise >> 8) as u8, (noise >> 16) as u8, 128]);
    }
    let oversized = decoded(buffer);
    state.update(&mut cx, |app, cx| {
        app.set_model("qwen-image-3.0-pro", cx);
        app.scene.prose = "a harbor".into();
        app.set_image(oversized, cx);
    });
    let validation = qwen_validation(&mut cx, state.clone());
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages
            .iter()
            .any(|message| message == "An encoded image is over 10 MB."),
        "{messages:?}"
    );
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));

    let stretched = decoded(image::RgbaImage::from_pixel(
        3000,
        256,
        image::Rgba([20, 40, 60, 255]),
    ));
    state.update(&mut cx, |app, cx| {
        app.set_image(stretched, cx);
        app.extras.clear();
    });
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages.iter().any(|message| *message
            == "An image's long side is more than 10 times its short side."),
        "{messages:?}"
    );
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));

    state.update(&mut cx, |app, cx| {
        app.set_image(qwen_image(), cx);
        app.scene
            .add_reference(easel_core::scene::ImageRef::new("ref", 256, 256));
    });
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages
            .iter()
            .any(|message| message == "A prepared image is missing."),
        "{messages:?}"
    );
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
}

#[gpui::test]
fn an_empty_prompt_and_a_missing_key_disable_qwen_generate(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("qwen-image-3.0-pro", cx);
        app.scene.prose = "   ".into();
    });
    let validation = qwen_validation_without_key(&mut cx, state.clone());
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(messages.iter().any(|message| message == "Prompt is empty"));
    assert!(
        messages
            .iter()
            .any(|message| *message == "No Qwen API key — open Settings"),
        "{messages:?}"
    );
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
}

#[gpui::test]
fn switching_from_qwen_back_to_flux_keeps_the_prompt_canvas_and_request(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.scene.prose = "keep this prompt".into();
        app.set_image(qwen_image(), cx);
    });
    let before = state.read_with(&cx, |app, _| flux_submit(app));
    state.update(&mut cx, |app, cx| {
        app.set_model("qwen-image-3.0-pro", cx);
        app.set_model("qwen-image-2.1-pro", cx);
        app.set_model("flux-3-image", cx);
    });
    let (prose, canvas, after) = state.read_with(&cx, |app, _| {
        (
            app.scene.prose.clone(),
            app.image.is_some() && app.scene.source_image.is_some(),
            flux_submit(app),
        )
    });
    assert_eq!(prose, "keep this prompt");
    assert!(canvas);
    assert_eq!(after, before);
}

#[gpui::test]
fn a_qwen_edit_sends_data_uris_canvas_first(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("qwen-image-3.0-pro", cx);
        app.scene.prose = "make it blue".into();
        attach_qwen_prepared(app, cx, 1);
    });
    let http = state.read_with(&cx, |app, _| qwen_submit(app));
    assert_eq!(http.auth, Some(ProviderId::Qwen));
    let easel_core::http::Body::Json(body) = http.body else {
        panic!("expected json");
    };
    let content = body["input"]["messages"][0]["content"].as_array().unwrap();
    assert_eq!(content.len(), 3);
    assert!(
        content[0]["image"]
            .as_str()
            .unwrap()
            .starts_with("data:image/")
    );
    assert!(
        content[1]["image"]
            .as_str()
            .unwrap()
            .starts_with("data:image/")
    );
    assert_eq!(content[2]["text"], "make it blue");
}

fn apiframe_validation(
    cx: &mut VisualTestContext,
    state: gpui::Entity<AppState>,
) -> gpui::Entity<Validation> {
    let keys: Arc<dyn crate::net::KeyResolver> =
        Arc::new(MapKeys::default().insert(ProviderId::Apiframe, "apiframe-key"));
    cx.new(|cx| Validation::new(state, keys, cx))
}

#[gpui::test]
fn a_missing_apiframe_key_disables_generate_until_a_key_exists(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("apiframe-midjourney", cx);
        app.scene.prose = "a lighthouse".into();
    });
    let keys: Arc<dyn crate::net::KeyResolver> = Arc::new(MapKeys::default());
    let validation = cx.new(|cx| Validation::new(state.clone(), keys, cx));
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages
            .iter()
            .any(|message| *message == "No Apiframe API key — open Settings"),
        "{messages:?}"
    );
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    let validation = apiframe_validation(&mut cx, state);
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
}

#[gpui::test]
fn apiframe_reference_notices_follow_each_model_slot_cap(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.scene.prose = "a harbor".into();
        attach_prepared(app, cx, 1);
    });
    let enabled = |cx: &VisualTestContext, validation: &gpui::Entity<Validation>| {
        validation.read_with(cx, |validation, cx| validation.generate_enabled(cx))
    };
    let notice = |cx: &VisualTestContext, validation: &gpui::Entity<Validation>| {
        validation.read_with(cx, |validation, cx| validation.send_notice(cx))
    };

    state.update(&mut cx, |app, cx| app.set_model("apiframe-flux-2-pro", cx));
    let validation = apiframe_validation(&mut cx, state.clone());
    assert!(enabled(&cx, &validation));
    assert_eq!(notice(&cx, &validation), None);

    state.update(&mut cx, |app, cx| {
        app.set_model("apiframe-stable-diffusion-3.5-large", cx)
    });
    assert!(enabled(&cx, &validation));
    assert_eq!(
        notice(&cx, &validation),
        Some("1 of 2 images will be sent".to_string())
    );

    state.update(&mut cx, |app, cx| app.set_model("apiframe-dall-e-3", cx));
    assert!(enabled(&cx, &validation));
    assert_eq!(
        notice(&cx, &validation),
        Some("0 of 2 images will be sent".to_string())
    );
}

#[gpui::test]
fn apiframe_qwen_image_task_type_gates_generate(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("apiframe-qwen-image", cx);
        app.scene.prose = "make it blue".into();
    });
    let validation = apiframe_validation(&mut cx, state.clone());
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    state.update(&mut cx, |app, _| {
        app.values.insert("task_type".into(), json!("image-edit"));
    });
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages
            .iter()
            .any(|message| *message == "Add at least one image."),
        "{messages:?}"
    );
    state.update(&mut cx, |app, _| {
        app.values
            .insert("task_type".into(), json!("text-to-image"));
    });
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    state.update(&mut cx, |app, _| {
        app.values.insert("task_type".into(), json!("image-edit"));
    });
    state.update(&mut cx, |app, cx| {
        attach_prepared(app, cx, 0);
    });
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
}

fn leonardo_validation(
    cx: &mut VisualTestContext,
    state: gpui::Entity<AppState>,
) -> gpui::Entity<Validation> {
    let keys: Arc<dyn crate::net::KeyResolver> =
        Arc::new(MapKeys::default().insert(ProviderId::Leonardo, "leonardo-key"));
    cx.new(|cx| Validation::new(state, keys, cx))
}

fn leonardo_image() -> Arc<crate::image_load::DecodedImage> {
    decoded(image::RgbaImage::from_pixel(
        128,
        128,
        image::Rgba([20, 40, 60, 255]),
    ))
}

fn tiny_image() -> Arc<crate::image_load::DecodedImage> {
    decoded(image::RgbaImage::from_pixel(
        100,
        100,
        image::Rgba([20, 40, 60, 255]),
    ))
}

fn attach_leonardo(app: &mut AppState, cx: &mut gpui::Context<AppState>, extras: usize) {
    app.set_image(leonardo_image(), cx);
    for _ in 0..extras {
        app.scene
            .add_reference(easel_core::scene::ImageRef::new("ref", 128, 128));
        app.extras.push(leonardo_image());
    }
}

#[gpui::test]
fn a_missing_leonardo_key_disables_generate(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("leonardo-phoenix-v1.0", cx);
        app.scene.prose = "ancient observatory".into();
    });
    let keys: Arc<dyn crate::net::KeyResolver> = Arc::new(MapKeys::default());
    let validation = cx.new(|cx| Validation::new(state.clone(), keys, cx));
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages
            .iter()
            .any(|message| message == "No Leonardo.Ai API key — open Settings"),
        "{messages:?}"
    );
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    let validation = leonardo_validation(&mut cx, state);
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    assert!(validation.read_with(&cx, |validation, cx| validation.send_notice(cx).is_none()));
}

#[gpui::test]
fn promptless_leonardo_models_need_one_image_and_generate_with_a_128_canvas(
    cx: &mut TestAppContext,
) {
    for id in [
        "leonardo-aurora-upscaler-creative",
        "leonardo-aurora-upscaler-precise",
        "leonardo-remove-bg",
    ] {
        let (state, mut cx) = open(cx);
        state.update(&mut cx, |app, cx| app.set_model(id, cx));
        let validation = leonardo_validation(&mut cx, state.clone());
        let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
        assert!(
            messages
                .iter()
                .any(|message| message == "Add at least one image."),
            "{id}: {messages:?}"
        );
        assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));

        state.update(&mut cx, |app, cx| app.set_image(leonardo_image(), cx));
        assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
        state.update(&mut cx, |app, _| {
            assert!(crate::jobs::build_request(app).is_ok(), "{id}");
        });
    }
}

#[gpui::test]
fn phoenix_content_guidance_max_blocks_until_high(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("leonardo-phoenix-v1.0", cx);
        app.scene.prose = "ancient observatory".into();
        app.set_image(leonardo_image(), cx);
        app.values.insert("guidance_type".into(), json!("content"));
        app.values.insert("guidance_strength".into(), json!("MAX"));
    });
    let validation = leonardo_validation(&mut cx, state.clone());
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages
            .iter()
            .any(|message| message == "Content guidance strength must be LOW, MID, HIGH, or ULTRA."),
        "{messages:?}"
    );
    state.update(&mut cx, |app, _| {
        app.values.insert("guidance_strength".into(), json!("HIGH"));
    });
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
}

#[gpui::test]
fn phoenix_clamps_by_guidance_slot_and_lucid_by_capacity(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("leonardo-phoenix-v1.0", cx);
        app.scene.prose = "ancient observatory".into();
        attach_leonardo(app, cx, 2);
        app.values.insert("guidance_type".into(), json!("style"));
    });
    let validation = leonardo_validation(&mut cx, state.clone());
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    assert!(validation.read_with(&cx, |validation, cx| validation.send_notice(cx).is_none()));
    let sent = state.update(&mut cx, |app, _| {
        crate::jobs::build_request(app).unwrap().images.len()
    });
    assert_eq!(sent, 3);

    state.update(&mut cx, |app, _| {
        app.values
            .insert("guidance_type".into(), json!("image_to_image"));
    });
    assert_eq!(
        validation.read_with(&cx, |validation, cx| validation.send_notice(cx)),
        Some("1 of 3 images will be sent".to_string())
    );
    let (sent, marks) = state.read_with(&cx, |app, _| {
        (
            crate::jobs::build_request(app).unwrap().images.len(),
            app.reference_marks(),
        )
    });
    assert_eq!(sent, 1);
    assert!(!marks[1].1 && marks[1].0.contains("not sent"), "{marks:?}");
    assert!(!marks[2].1 && marks[2].0.contains("not sent"), "{marks:?}");

    state.update(&mut cx, |app, cx| {
        app.set_model("leonardo-lucid-origin", cx)
    });
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    assert_eq!(
        validation.read_with(&cx, |validation, cx| validation.send_notice(cx)),
        Some("1 of 3 images will be sent".to_string())
    );
}

#[gpui::test]
fn a_text_only_model_omits_undersized_images(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("leonardo-ideogram-v4.0", cx);
        app.scene.prose = "a harbor".into();
        app.set_image(tiny_image(), cx);
        app.scene
            .add_reference(easel_core::scene::ImageRef::new("ref", 128, 128));
        app.extras.push(leonardo_image());
        app.scene
            .add_reference(easel_core::scene::ImageRef::new("ref", 128, 128));
        app.extras.push(leonardo_image());
    });
    let validation = leonardo_validation(&mut cx, state.clone());
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    assert_eq!(
        validation.read_with(&cx, |validation, cx| validation.send_notice(cx)),
        Some("0 of 3 images will be sent".to_string())
    );
    let images = state.update(&mut cx, |app, _| {
        crate::jobs::build_request(app).unwrap().images.len()
    });
    assert_eq!(images, 0);
}

#[gpui::test]
fn remove_bg_sends_both_of_its_slot_images(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("leonardo-remove-bg", cx);
        attach_leonardo(app, cx, 1);
    });
    let validation = leonardo_validation(&mut cx, state.clone());
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    assert!(validation.read_with(&cx, |validation, cx| validation.send_notice(cx).is_none()));
    state.read_with(&cx, |app, _| {
        let max = app.spec().capabilities.image_slots.max();
        assert!(max > 1, "{}", max);
        assert_eq!(app.max_sent_images(), max);
        assert_eq!(crate::jobs::build_request(app).unwrap().images.len(), 2);
    });
}

#[gpui::test]
fn krea2_sends_every_attached_image_without_a_guidance_slot(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("leonardo-krea-2", cx);
        app.scene.prose = "a harbor".into();
        attach_leonardo(app, cx, 2);
    });
    let validation = leonardo_validation(&mut cx, state.clone());
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    assert!(validation.read_with(&cx, |validation, cx| validation.send_notice(cx).is_none()));
    state.read_with(&cx, |app, _| {
        let max = app.spec().capabilities.image_slots.max();
        assert!(max > 1, "{}", max);
        assert_eq!(app.max_sent_images(), max);
        assert_eq!(crate::jobs::build_request(app).unwrap().images.len(), 3);
    });
}

#[gpui::test]
fn an_undersized_image_blocks_generate_on_leonardo_image_models(cx: &mut TestAppContext) {
    for id in ["leonardo-remove-bg", "leonardo-phoenix-v1.0"] {
        let (state, mut cx) = open(cx);
        state.update(&mut cx, |app, cx| {
            app.set_model(id, cx);
            app.scene.prose = "a harbor".into();
            app.set_image(tiny_image(), cx);
        });
        let validation = leonardo_validation(&mut cx, state.clone());
        let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
        assert!(
            messages
                .iter()
                .any(|message| *message == "Input image must be at least 128×128."),
            "{id}: {messages:?}"
        );
        assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));

        state.update(&mut cx, |app, cx| app.set_image(leonardo_image(), cx));
        assert!(validation.read_with(&cx, |validation, cx| {
            !validation
                .messages(cx)
                .iter()
                .any(|message| message.contains("128×128"))
        }));
        assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
        state.update(&mut cx, |app, _| {
            assert!(crate::jobs::build_request(app).is_ok(), "{id}");
        });
    }
}

#[gpui::test]
fn leonardo_width_and_height_render_number_inputs(cx: &mut TestAppContext) {
    let (state, form, mut cx) = mount(cx);
    for id in [
        "leonardo-kino-xl",
        "leonardo-flux-dev",
        "leonardo-gpt-image-1.5",
    ] {
        state.update(&mut cx, |app, cx| app.set_model(id, cx));
        assert!(
            form.read_with(&cx, |form, _| form.number("width").is_some()),
            "{id}"
        );
        assert!(
            form.read_with(&cx, |form, _| form.number("height").is_some()),
            "{id}"
        );
        assert!(
            form.read_with(&cx, |form, _| form.slider("width").is_none()),
            "{id}"
        );
        assert!(
            form.read_with(&cx, |form, _| form.slider("height").is_none()),
            "{id}"
        );
    }
}

fn stability_validation(
    cx: &mut VisualTestContext,
    state: gpui::Entity<AppState>,
) -> gpui::Entity<Validation> {
    let keys: Arc<dyn crate::net::KeyResolver> =
        Arc::new(MapKeys::default().insert(ProviderId::Stability, "stability-key"));
    cx.new(|cx| Validation::new(state, keys, cx))
}

fn small_image() -> Arc<crate::image_load::DecodedImage> {
    decoded(image::RgbaImage::from_pixel(
        64,
        64,
        image::Rgba([20, 40, 60, 255]),
    ))
}

fn stability_submit(app: &AppState) -> easel_core::http::HttpRequest {
    let provider = easel_core::providers::resolve_model(&app.model_id).expect("provider");
    let request = crate::jobs::build_request(app).expect("request");
    provider.build_submit(&request, &[])
}

fn parts_of(http: &easel_core::http::HttpRequest) -> &[easel_core::http::Part] {
    match &http.body {
        easel_core::http::Body::Multipart { parts } => parts,
        easel_core::http::Body::Json(_) => panic!("expected multipart, got json"),
        easel_core::http::Body::Bytes(_, _) => panic!("expected multipart, got bytes"),
        easel_core::http::Body::Empty => panic!("expected multipart, got empty"),
    }
}

#[gpui::test]
fn a_missing_stability_key_disables_generate(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("stability-ultra", cx);
        app.scene.prose = "a harbor".into();
    });
    let keys: Arc<dyn crate::net::KeyResolver> = Arc::new(MapKeys::default());
    let validation = cx.new(|cx| Validation::new(state.clone(), keys, cx));
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages
            .iter()
            .any(|message| *message == "No Stability AI API key — open Settings"),
        "{messages:?}"
    );
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    let validation = stability_validation(&mut cx, state);
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
}

#[gpui::test]
fn promptless_stability_edits_run_on_a_64_pixel_image(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("stability-upscale-fast", cx);
        app.scene.prose.clear();
        app.set_image(small_image(), cx);
    });
    let validation = stability_validation(&mut cx, state.clone());
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    let http = state.read_with(&cx, |app, _| stability_submit(app));
    assert_eq!(http.auth, Some(ProviderId::Stability));
    let parts = parts_of(&http);
    assert!(parts.iter().any(|part| part.name == "image"));
    assert!(
        parts
            .iter()
            .any(|part| part.name == "output_format" && part.bytes.as_slice() == b"png")
    );
    assert!(!parts.iter().any(|part| part.name == "prompt"));

    state.update(&mut cx, |app, cx| {
        app.set_model("stability-remove-background", cx)
    });
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    let http = state.read_with(&cx, |app, _| stability_submit(app));
    assert!(!parts_of(&http).iter().any(|part| part.name == "prompt"));

    state.update(&mut cx, |app, cx| app.set_model("stability-erase", cx));
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    let http = state.read_with(&cx, |app, _| stability_submit(app));
    let parts = parts_of(&http);
    let image = parts
        .iter()
        .find(|part| part.name == "image")
        .expect("image part");
    assert_eq!(image.content_type, "image/png");
    assert!(image.bytes.starts_with(b"\x89PNG"));
    assert!(!parts.iter().any(|part| part.name == "mask"));
    assert!(!parts.iter().any(|part| part.name == "prompt"));

    state.update(&mut cx, |app, cx| app.set_model("stability-inpaint", cx));
    let http = state.read_with(&cx, |app, _| stability_submit(app));
    let parts = parts_of(&http);
    let image = parts
        .iter()
        .find(|part| part.name == "image")
        .expect("image part");
    assert_eq!(image.content_type, "image/png");
    assert!(image.bytes.starts_with(b"\x89PNG"));
    assert!(!parts.iter().any(|part| part.name == "mask"));
}

#[gpui::test]
fn relight_needs_a_second_image_when_the_prompt_is_empty(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("stability-replace-background-and-relight", cx);
        app.scene.prose.clear();
        app.set_image(small_image(), cx);
    });
    let validation = stability_validation(&mut cx, state.clone());
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages.iter().any(|message| message == "Prompt is empty"),
        "{messages:?}"
    );
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    state.update(&mut cx, |app, cx| {
        app.scene
            .add_reference(easel_core::scene::ImageRef::new("ref", 64, 64));
        app.extras.push(small_image());
        cx.notify();
    });
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
}

#[gpui::test]
fn outpaint_directions_and_search_prompt_gate_generate(cx: &mut TestAppContext) {
    let (state, form, mut cx) = mount(cx);
    state.update(&mut cx, |app, cx| {
        app.set_model("stability-outpaint", cx);
        app.set_image(source_image(), cx);
    });
    let validation = stability_validation(&mut cx, state.clone());
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages
            .iter()
            .any(|message| *message == "Set at least one outpaint direction above 0."),
        "{messages:?}"
    );
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    replace_number(&mut cx, &form, "left", "200");
    assert_eq!(stored(&state, &cx, "left"), Some(json!(200)));
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)),
        "{messages:?}"
    );

    state.update(&mut cx, |app, cx| {
        app.scene.prose = "replace the car".into();
        app.set_model("stability-search-and-replace", cx)
    });
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages
            .iter()
            .any(|message| *message == "Search prompt is required"),
        "{messages:?}"
    );
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    replace_text(&mut cx, &form, "search_prompt", "the red car");
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    replace_text(&mut cx, &form, "search_prompt", "");
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages
            .iter()
            .any(|message| *message == "Search prompt is required"),
        "{messages:?}"
    );
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
}

#[gpui::test]
fn over_cap_stability_switches_clamp_with_a_send_notice(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    state.update(&mut cx, |app, cx| {
        app.scene.prose = "a harbor".into();
        app.set_model("stability-inpaint", cx);
    });
    let validation = stability_validation(&mut cx, state.clone());
    let messages = validation.read_with(&cx, |validation, cx| validation.messages(cx));
    assert!(
        messages
            .iter()
            .any(|message| message == "Add at least one image."),
        "{messages:?}"
    );
    assert!(!validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));

    state.update(&mut cx, |app, cx| {
        app.set_model("stability-replace-background-and-relight", cx);
        app.set_image(small_image(), cx);
        for _ in 0..2 {
            app.scene
                .add_reference(easel_core::scene::ImageRef::new("ref", 64, 64));
            app.extras.push(small_image());
        }
        cx.notify();
    });
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));

    state.update(&mut cx, |app, cx| app.set_model("stability-ultra", cx));
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    assert_eq!(
        validation.read_with(&cx, |validation, cx| validation.send_notice(cx)),
        Some("1 of 3 images will be sent".to_string())
    );

    state.update(&mut cx, |app, cx| app.set_model("stability-core", cx));
    assert!(validation.read_with(&cx, |validation, cx| validation.generate_enabled(cx)));
    assert_eq!(
        validation.read_with(&cx, |validation, cx| validation.send_notice(cx)),
        Some("0 of 3 images will be sent".to_string())
    );
}
