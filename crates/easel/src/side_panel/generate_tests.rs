use std::sync::Arc;

use easel_core::history::HistoryStore;
use easel_core::model_spec::ProviderId;
use gpui::{TestAppContext, prelude::*};

use crate::app_state::AppState;
use crate::jobs::JobRunner;
use crate::net::{FakeTransport, HttpTransport};
use crate::settings::{Resolver, SharedKeys};
use crate::side_panel::SidePanel;

fn temp_dir() -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!(
        "easel-generate-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&path);
    path
}

#[gpui::test]
fn generate_submits_the_scene_to_the_runner(cx: &mut TestAppContext) {
    let root = temp_dir();
    let history = Arc::new(HistoryStore::open(root.join("history")).unwrap());
    let transport: Arc<dyn HttpTransport> = Arc::new(FakeTransport::scripted(Vec::new()));
    let keys = Arc::new(SharedKeys::default());
    keys.set_session(ProviderId::Bfl, "test-key");
    let runner = cx.update({
        let transport = Arc::clone(&transport);
        let history = Arc::clone(&history);
        let keys = Arc::clone(&keys);
        move |cx| {
            cx.new(|_| {
                JobRunner::new(
                    easel_core::providers::resolve_model,
                    transport,
                    Arc::new(Resolver(keys)),
                    history,
                )
            })
        }
    });
    let (state, panel) = cx.update(|cx| {
        let state = cx.new(|_| AppState::new());
        let panel = cx.new({
            let state = state.clone();
            let runner = runner.clone();
            move |cx| SidePanel::new(state, keys, Some(runner), None, cx)
        });
        (state, panel)
    });
    state.update(cx, |app, cx| {
        app.scene.prose = "a red square".into();
        app.set_aspect_ratio("1:1", cx);
    });
    panel.update(cx, |panel, cx| panel.generate(cx));
    let states = runner.read_with(cx, |runner, _| runner.states());
    assert_eq!(states.len(), 1, "{states:?}");
    let _ = std::fs::remove_dir_all(root);
}
