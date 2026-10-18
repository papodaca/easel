use std::sync::Arc;

use easel_core::geometry::Rect;
use easel_core::model_spec::ProviderId;
use easel_core::scene::{BoxKind, ImageRef};
use easel_core::session::{SessionFile, SessionStore};
use gpui::{TestAppContext, prelude::*};
use serde_json::json;

use crate::app_state::AppState;
use crate::jobs::build_request;
use crate::net::{FakeTransport, HttpTransport};
use crate::notifications::Notifications;
use crate::session::restore;
use crate::settings::{SettingsPanel, SharedKeys};
use crate::side_panel::SidePanel;

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

fn editor(cx: &mut TestAppContext) -> (gpui::Entity<AppState>, gpui::Entity<SidePanel>) {
    editor_with(cx, Arc::new(SharedKeys::default()))
}

fn editor_with(
    cx: &mut TestAppContext,
    keys: Arc<SharedKeys>,
) -> (gpui::Entity<AppState>, gpui::Entity<SidePanel>) {
    let state = app(cx);
    let panel = cx.update({
        let state = state.clone();
        move |cx| cx.new(|cx| SidePanel::new(state, keys, None, None, cx))
    });
    (state, panel)
}

fn labels(cx: &TestAppContext, select: gpui::Entity<crate::widgets::Select>) -> Vec<String> {
    select.read_with(cx, |select, _| {
        select
            .options()
            .iter()
            .map(|option| option.to_string())
            .collect()
    })
}

fn selected(cx: &TestAppContext, select: gpui::Entity<crate::widgets::Select>) -> String {
    select.read_with(cx, |select, _| {
        select.value().as_str().unwrap_or("").to_owned()
    })
}

#[gpui::test]
fn the_header_lists_providers_and_the_body_lists_their_models(cx: &mut TestAppContext) {
    let _env = ProviderEnvGuard::clear();
    let keys = Arc::new(SharedKeys::default());
    keys.set_session(ProviderId::Bfl, "bfl-key");
    keys.set_session(ProviderId::Google, "google-key");
    keys.set_session(ProviderId::Krea, "krea-key");
    let (_state, panel) = editor_with(cx, keys);
    let providers = labels(
        cx,
        panel.read_with(cx, |panel, _| panel.provider_menu().clone()),
    );
    assert!(providers.contains(&"Google".to_string()));
    assert!(providers.contains(&"Black Forest Labs".to_string()));
    assert!(providers.contains(&"Krea".to_string()));
    assert!(!providers.iter().any(|name| name == "OpenAI"));
    assert!(!providers.iter().any(|name| name == "FLUX 3"));
    assert_eq!(
        selected(
            cx,
            panel.read_with(cx, |panel, _| panel.provider_menu().clone())
        ),
        "Black Forest Labs"
    );

    panel.update(cx, |panel, cx| panel.choose_provider("Google", cx));
    let models = labels(
        cx,
        panel.read_with(cx, |panel, _| panel.model_menu().clone()),
    );
    assert_eq!(
        models,
        ["Nano Banana 2.1", "Nano Banana 2 Lite", "Nano Banana Pro",]
    );
    assert!(!models.iter().any(|name| name == "FLUX 3"));
    assert_eq!(
        selected(
            cx,
            panel.read_with(cx, |panel, _| panel.model_menu().clone())
        ),
        "Nano Banana 2.1"
    );

    panel.update(cx, |panel, cx| panel.choose_provider("Krea", cx));
    let models = labels(
        cx,
        panel.read_with(cx, |panel, _| panel.model_menu().clone()),
    );
    assert_eq!(models, ["Krea 3", "Krea 2 Large"]);
    assert_eq!(
        selected(
            cx,
            panel.read_with(cx, |panel, _| panel.model_menu().clone())
        ),
        "Krea 3"
    );

    panel.update(cx, |panel, cx| panel.choose_provider("Google", cx));
    panel.update(cx, |panel, cx| panel.choose_model("Nano Banana Pro", cx));
    panel.update(cx, |panel, cx| panel.choose_provider("Krea", cx));
    panel.update(cx, |panel, cx| panel.choose_provider("Google", cx));
    assert_eq!(
        selected(
            cx,
            panel.read_with(cx, |panel, _| panel.model_menu().clone())
        ),
        "Nano Banana Pro"
    );
}

struct ProviderEnvGuard {
    saved: Vec<(&'static str, Option<String>)>,
}

impl ProviderEnvGuard {
    fn clear() -> Self {
        let names = [
            "BFL_API_KEY",
            "KREA_API_KEY",
            "GEMINI_API_KEY",
            "OPENAI_API_KEY",
        ];
        let saved = names
            .iter()
            .map(|name| (*name, std::env::var(name).ok()))
            .collect();
        for name in names {
            unsafe { std::env::remove_var(name) };
        }
        Self { saved }
    }
}

impl Drop for ProviderEnvGuard {
    fn drop(&mut self) {
        for (name, value) in self.saved.drain(..) {
            if let Some(value) = value {
                unsafe { std::env::set_var(name, value) };
            }
        }
    }
}

fn without_provider_env<T>(body: impl FnOnce() -> T) -> T {
    let _guard = ProviderEnvGuard::clear();
    body()
}

#[gpui::test]
fn a_provider_appears_after_its_key_is_saved(cx: &mut TestAppContext) {
    without_provider_env(|| a_provider_appears_after_its_key_is_saved_inner(cx));
}

fn a_provider_appears_after_its_key_is_saved_inner(cx: &mut TestAppContext) {
    let keys = Arc::new(SharedKeys::default());
    let (panel, settings) = cx.update({
        let keys = Arc::clone(&keys);
        move |cx| {
            let state = cx.new(|_| AppState::new());
            let notes = cx.new(|_| Notifications::new());
            let transport: Arc<dyn HttpTransport> = Arc::new(FakeTransport::scripted(Vec::new()));
            let settings = cx.new({
                let keys = Arc::clone(&keys);
                move |cx| SettingsPanel::open(notes, transport, keys, cx)
            });
            let panel = cx.new({
                let settings = settings.clone();
                move |cx| SidePanel::new(state, keys, None, Some(settings), cx)
            });
            (panel, settings)
        }
    });
    cx.run_until_parked();
    let providers = labels(
        cx,
        panel.read_with(cx, |panel, _| panel.provider_menu().clone()),
    );
    assert!(providers.is_empty(), "{providers:?}");
    keys.set_session(ProviderId::Google, "google-key");
    settings.update(cx, |_, cx| cx.notify());
    let providers = labels(
        cx,
        panel.read_with(cx, |panel, _| panel.provider_menu().clone()),
    );
    assert_eq!(providers, ["Google".to_string()]);
}

#[gpui::test]
fn switching_to_lite_keeps_the_prompt_and_drops_wide_ratio_and_grounding(cx: &mut TestAppContext) {
    let (state, panel) = editor(cx);
    state.update(cx, |app, cx| {
        app.scene.prose = "keep this prompt".into();
        app.set_model("gemini-nano-banana-2.1", cx);
        app.values.insert("grounding".into(), json!(true));
        app.values.insert("aspect_ratio".into(), json!("1:8"));
        app.values.insert("resolution".into(), json!("2K"));
    });
    panel.update(cx, |panel, cx| panel.choose_model("Nano Banana 2 Lite", cx));
    let (prose, values, fields) = state.read_with(cx, |app, _| {
        (
            app.scene.prose.clone(),
            app.values.clone(),
            app.spec()
                .schema
                .fields
                .iter()
                .map(|field| field.key.clone())
                .collect::<Vec<_>>(),
        )
    });
    assert_eq!(prose, "keep this prompt");
    assert!(!fields.iter().any(|key| key == "grounding"));
    assert!(!values.contains_key("grounding"));
    assert_ne!(values.get("aspect_ratio"), Some(&json!("1:8")));
    assert_eq!(values.get("resolution"), Some(&json!("1K")));
    let ratios = state.read_with(cx, |app, _| {
        app.spec()
            .schema
            .fields
            .iter()
            .find(|field| field.key == "aspect_ratio")
            .and_then(|field| match &field.kind {
                easel_core::schema::FieldKind::Select { options } => Some(options.clone()),
                _ => None,
            })
    });
    assert!(!ratios.unwrap().iter().any(|ratio| ratio == "1:8"));
}

#[gpui::test]
fn a_restored_pro_session_shows_google_and_nano_banana_pro(cx: &mut TestAppContext) {
    let root = std::env::temp_dir().join(format!(
        "easel-restore-pro-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let file = SessionFile {
        version: 1,
        scene: easel_core::scene::Scene::empty(),
        values: std::collections::BTreeMap::new(),
        model_id: "gemini-3-pro-image".into(),
        source: None,
        references: Vec::new(),
    };
    SessionStore::open(root.join("session.json"))
        .write(&file)
        .unwrap();
    let (state, panel) = editor(cx);
    cx.update(|cx| restore(state, root.join("session.json"), cx));
    cx.run_until_parked();
    assert_eq!(
        selected(
            cx,
            panel.read_with(cx, |panel, _| panel.provider_menu().clone())
        ),
        "Google"
    );
    assert_eq!(
        selected(
            cx,
            panel.read_with(cx, |panel, _| panel.model_menu().clone())
        ),
        "Nano Banana Pro"
    );
    let _ = std::fs::remove_dir_all(&root);
}

#[gpui::test]
fn flare_max_becomes_a_quality_gpt_image_2_allows(cx: &mut TestAppContext) {
    let (state, panel) = editor(cx);
    state.update(cx, |app, cx| {
        app.scene.prose = "a harbor".into();
        app.set_model("gpt-image-2.5-flare", cx);
        app.values.insert("quality".into(), json!("max"));
    });
    panel.update(cx, |panel, cx| panel.choose_model("GPT Image 2", cx));
    let (quality, fields) = state.read_with(cx, |app, _| {
        (
            app.values.get("quality").cloned(),
            app.spec()
                .schema
                .fields
                .iter()
                .map(|field| field.key.clone())
                .collect::<Vec<_>>(),
        )
    });
    let quality = quality.and_then(|value| value.as_str().map(str::to_owned));
    assert!(
        matches!(quality.as_deref(), Some("low" | "medium" | "high" | "auto")),
        "{quality:?}"
    );
    assert!(!fields.iter().any(|key| key == "input_fidelity"));
}

#[gpui::test]
fn mini_drops_high_fidelity_to_low(cx: &mut TestAppContext) {
    let (state, panel) = editor(cx);
    state.update(cx, |app, cx| {
        app.set_model("gpt-image-1.5", cx);
        app.values.insert("input_fidelity".into(), json!("high"));
    });
    panel.update(cx, |panel, cx| {
        panel.choose_model("GPT Image 1 mini 2026-12-01", cx)
    });
    assert_eq!(
        state.read_with(cx, |app, _| app.values.get("input_fidelity").cloned()),
        Some(json!("low"))
    );
}

#[gpui::test]
fn switching_back_to_flux_keeps_the_prompt_and_posts_to_bfl(cx: &mut TestAppContext) {
    use easel_core::providers::Provider;

    let state = app(cx);
    state.update(cx, |app, cx| {
        app.scene.prose = "keep this prompt".into();
        app.scene
            .set_source(ImageRef::new("canvas.png", 1024, 1024));
        app.set_model("gpt-image-2.5-flare", cx);
        app.set_model("flux-3-image", cx);
    });
    let (prose, canvas) = state.read_with(cx, |app, _| {
        (app.scene.prose.clone(), app.scene.source_image.is_some())
    });
    assert_eq!(prose, "keep this prompt");
    assert!(canvas);
    let url = state.read_with(cx, |app, _| {
        let request = build_request(app).unwrap();
        easel_core::providers::bfl::flux3::Flux3
            .build_submit(&request, &[])
            .url
    });
    assert!(url.starts_with("https://api.bfl.ai/"), "{url}");
}
