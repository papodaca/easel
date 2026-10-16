use std::sync::Arc;

use easel_core::geometry::Rect;
use easel_core::model_spec::ProviderId;
use easel_core::scene::BoxKind;
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
fn aspect_locks_only_when_a_source_and_boxes_are_both_present(cx: &mut TestAppContext) {
    let (state, mut cx) = open(cx);
    let form = cx.new(|cx| SchemaForm::new(state.clone(), cx));
    state.update(&mut cx, |app, cx| app.set_image(source_image(), cx));
    assert!(form.read_with(&cx, |form, cx| form.aspect_enabled(cx)));
    state.update(&mut cx, |app, cx| {
        app.scene.add_box(
            Rect {
                x: 0,
                y: 0,
                w: 40,
                h: 40,
            },
            BoxKind::New,
        );
        cx.notify();
    });
    let (enabled, value) = form.read_with(&cx, |form, cx| {
        (form.aspect_enabled(cx), form.value("aspect_ratio", cx))
    });
    assert!(!enabled);
    assert_eq!(value, json!("auto"));
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
