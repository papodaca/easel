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
use crate::side_panel::prompt_editor::PromptEditor;
use crate::side_panel::references::References;
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

fn source_image() -> Arc<crate::image_load::DecodedImage> {
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
    let validation = openai_validation(&mut cx, state);
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
