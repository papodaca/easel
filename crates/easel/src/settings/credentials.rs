use std::sync::Arc;

use easel_core::model_spec::ProviderId;
use gpui::{AppContext as _, TestAppContext};
use zeroize::Zeroizing;

use super::{
    CredentialStore, FailingStore, MemoryStore, SaveOutcome, SessionKeys, Source, resolve_key,
    save_key,
};

fn env_with(value: &str) -> std::collections::HashMap<String, String> {
    std::collections::HashMap::from([(ProviderId::Bfl.env_var().to_owned(), value.to_owned())])
}

struct EnvVarGuard {
    name: &'static str,
    saved: Option<String>,
}

impl EnvVarGuard {
    fn set(name: &'static str, value: &str) -> Self {
        let saved = std::env::var(name).ok();
        unsafe { std::env::set_var(name, value) };
        Self { name, saved }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match self.saved.take() {
            Some(value) => unsafe { std::env::set_var(self.name, value) },
            None => unsafe { std::env::remove_var(self.name) },
        }
    }
}

#[gpui::test]
fn an_env_key_shows_source_environment(cx: &mut TestAppContext) {
    use gpui::prelude::*;

    use crate::net::FakeTransport;
    use crate::notifications::Notifications;
    use crate::settings::SettingsPanel;

    let _guard = EnvVarGuard::set(ProviderId::Google.env_var(), "env-google-key");
    let notes = cx.new(|_| Notifications::new());
    let transport = Arc::new(FakeTransport::scripted(Vec::new()));
    let panel = cx.new(move |cx| SettingsPanel::new(notes, transport, cx));
    assert_eq!(
        panel.read_with(cx, |panel, _| panel.row_note(ProviderId::Google).to_owned()),
        "Source: environment"
    );
    cx.run_until_parked();
    assert_eq!(
        panel.read_with(cx, |panel, _| panel.row_note(ProviderId::Google).to_owned()),
        "Source: environment"
    );
}

#[test]
fn openai_env_beats_the_store() {
    let store = MemoryStore::default();
    store.write(ProviderId::OpenAi, "store-key").expect("write");
    assert!(store.stores_under(ProviderId::OpenAi.base_url()));
    let mut session = SessionKeys::default();
    session.set(ProviderId::OpenAi, "session-key");
    let env = std::collections::HashMap::from([(
        ProviderId::OpenAi.env_var().to_owned(),
        "env-key".to_owned(),
    )]);
    let resolved = resolve_key(ProviderId::OpenAi, &env, &store, &session).expect("env");
    assert_eq!(resolved.0.as_str(), "env-key");
    assert_eq!(resolved.1, Source::Env);
    assert_eq!(ProviderId::OpenAi.env_var(), "OPENAI_API_KEY");
}

#[test]
fn xai_env_beats_the_store_keyed_by_base_url() {
    let store = MemoryStore::default();
    store.write(ProviderId::Xai, "store-key").expect("write");
    assert!(store.stores_under(ProviderId::Xai.base_url()));
    assert_eq!(ProviderId::Xai.base_url(), "https://api.x.ai");
    let mut session = SessionKeys::default();
    session.set(ProviderId::Xai, "session-key");
    let env = std::collections::HashMap::from([(
        ProviderId::Xai.env_var().to_owned(),
        "env-key".to_owned(),
    )]);
    let resolved = resolve_key(ProviderId::Xai, &env, &store, &session).expect("env");
    assert_eq!(resolved.0.as_str(), "env-key");
    assert_eq!(resolved.1, Source::Env);
    assert_eq!(ProviderId::Xai.env_var(), "XAI_API_KEY");
}

#[test]
fn chutes_env_beats_the_store_keyed_by_base_url() {
    let store = MemoryStore::default();
    store.write(ProviderId::Chutes, "store-key").expect("write");
    assert!(store.stores_under(ProviderId::Chutes.base_url()));
    assert_eq!(ProviderId::Chutes.base_url(), "https://api.chutes.ai");
    let mut session = SessionKeys::default();
    session.set(ProviderId::Chutes, "session-key");
    let env = std::collections::HashMap::from([(
        ProviderId::Chutes.env_var().to_owned(),
        "env-key".to_owned(),
    )]);
    let resolved = resolve_key(ProviderId::Chutes, &env, &store, &session).expect("env");
    assert_eq!(resolved.0.as_str(), "env-key");
    assert_eq!(resolved.1, Source::Env);
    assert_eq!(ProviderId::Chutes.env_var(), "CHUTES_API_KEY");

    let store_read = resolve_key(
        ProviderId::Chutes,
        &std::collections::HashMap::new(),
        &store,
        &session,
    )
    .expect("store");
    assert_eq!(store_read.0.as_str(), "store-key");
    assert_eq!(store_read.1, Source::Store);
}

#[test]
fn krea_env_beats_the_store_keyed_by_base_url() {
    let store = MemoryStore::default();
    store.write(ProviderId::Krea, "store-key").expect("write");
    assert!(store.stores_under(ProviderId::Krea.base_url()));
    let mut session = SessionKeys::default();
    session.set(ProviderId::Krea, "session-key");
    let env = std::collections::HashMap::from([(
        ProviderId::Krea.env_var().to_owned(),
        "env-key".to_owned(),
    )]);
    let resolved = resolve_key(ProviderId::Krea, &env, &store, &session).expect("env");
    assert_eq!(resolved.0.as_str(), "env-key");
    assert_eq!(resolved.1, Source::Env);

    let resolved = resolve_key(
        ProviderId::Krea,
        &std::collections::HashMap::new(),
        &store,
        &session,
    )
    .expect("store");
    assert_eq!(resolved.0.as_str(), "store-key");
    assert_eq!(resolved.1, Source::Store);
}

#[test]
fn env_beats_store_and_session() {
    let store = MemoryStore::default();
    store.write(ProviderId::Bfl, "store-key").expect("write");
    let mut session = SessionKeys::default();
    session.set(ProviderId::Bfl, "session-key");
    let resolved =
        resolve_key(ProviderId::Bfl, &env_with("env-key"), &store, &session).expect("key");
    assert_eq!(resolved.0.as_str(), "env-key");
    assert_eq!(resolved.1, Source::Env);
}

#[test]
fn store_then_session_then_none() {
    let store = MemoryStore::default();
    store.write(ProviderId::Bfl, "store-key").expect("write");
    let session = SessionKeys::default();
    let resolved = resolve_key(
        ProviderId::Bfl,
        &std::collections::HashMap::new(),
        &store,
        &session,
    )
    .expect("store");
    assert_eq!(resolved.0.as_str(), "store-key");
    assert_eq!(resolved.1, Source::Store);

    store.delete(ProviderId::Bfl).expect("delete");
    let mut session = SessionKeys::default();
    session.set(ProviderId::Bfl, "session-key");
    let resolved = resolve_key(
        ProviderId::Bfl,
        &std::collections::HashMap::new(),
        &store,
        &session,
    )
    .expect("session");
    assert_eq!(resolved.0.as_str(), "session-key");
    assert_eq!(resolved.1, Source::Session);

    session.clear(ProviderId::Bfl);
    assert!(
        resolve_key(
            ProviderId::Bfl,
            &std::collections::HashMap::new(),
            &store,
            &session,
        )
        .is_none()
    );
}

#[test]
fn memory_store_round_trips_and_deletes() {
    let store = MemoryStore::default();
    assert!(store.read(ProviderId::Bfl).expect("read").is_none());
    store.write(ProviderId::Bfl, "abc").expect("write");
    let stored = store.read(ProviderId::Bfl).expect("read").expect("present");
    assert_eq!(stored.as_str(), "abc");
    store.delete(ProviderId::Bfl).expect("delete");
    assert!(store.read(ProviderId::Bfl).expect("read").is_none());
}

#[test]
fn store_failures_keep_the_session_usable() {
    let store = FailingStore;
    let mut session = SessionKeys::default();
    let outcome = save_key(ProviderId::Bfl, "  abc\n", &store, &mut session);
    assert!(matches!(
        outcome,
        SaveOutcome::KeptInSession { message } if !message.is_empty()
    ));
    let resolved = resolve_key(
        ProviderId::Bfl,
        &std::collections::HashMap::new(),
        &store,
        &session,
    )
    .expect("session");
    assert_eq!(resolved.0.as_str(), "abc");
    assert_eq!(resolved.1, Source::Session);
}

#[test]
fn keys_are_trimmed_and_non_ascii_is_rejected() {
    let store = MemoryStore::default();
    let mut session = SessionKeys::default();
    let outcome = save_key(ProviderId::Bfl, " abc\n", &store, &mut session);
    assert!(matches!(outcome, SaveOutcome::Stored));
    assert_eq!(
        store
            .read(ProviderId::Bfl)
            .expect("read")
            .expect("stored")
            .as_str(),
        "abc"
    );

    let before = store.read(ProviderId::Bfl).expect("read");
    let rejected = save_key(ProviderId::Bfl, "bad\u{0}key", &store, &mut session);
    assert!(matches!(rejected, SaveOutcome::Rejected { message } if !message.is_empty()));
    let rejected = save_key(ProviderId::Bfl, "käse", &store, &mut session);
    assert!(matches!(rejected, SaveOutcome::Rejected { message } if !message.is_empty()));
    assert_eq!(store.read(ProviderId::Bfl).expect("read"), before);
    let _ = Zeroizing::new(String::new());
}

#[gpui::test]
fn masked_input_debug_hides_the_key(cx: &mut TestAppContext) {
    let window = cx.update(|cx| {
        cx.open_window(gpui::WindowOptions::default(), |_window, cx| {
            cx.new(|cx| {
                let mut input = crate::widgets::TextInput::new(cx);
                input.set_masked(true, cx);
                input
            })
        })
        .unwrap()
    });
    let input = window.root(cx).unwrap();
    input.update(cx, |input, cx| input.set_text("sekrit-key-zz", cx));
    let debug = input.read_with(cx, |input, _| format!("{input:?}"));
    assert!(!debug.contains("sekrit-key-zz"), "{debug}");
    assert!(debug.contains("<redacted>"), "{debug}");
}

#[gpui::test]
async fn test_action_classifies_the_bfl_response(cx: &mut TestAppContext) {
    use easel_core::http::HttpResponse;
    use gpui::prelude::*;

    use crate::net::{FakeTransport, TransportError};
    use crate::notifications::Notifications;
    use crate::settings::SettingsPanel;

    let transport = Arc::new(FakeTransport::scripted(vec![
        Ok(HttpResponse {
            status: 401,
            headers: Vec::new(),
            body: b"nope".to_vec(),
        }),
        Ok(HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: b"Task not found".to_vec(),
        }),
        Err(TransportError::Network {
            method: "GET".into(),
            url: "https://api.bfl.ai/v1/get_result?id=nonexistent".into(),
        }),
    ]));
    let notes = cx.new(|_| Notifications::new());
    let panel = cx.new({
        let notes = notes.clone();
        let transport = Arc::clone(&transport);
        move |cx| SettingsPanel::new(notes, transport, cx)
    });
    panel.update(cx, |panel, cx| {
        panel.set_draft(ProviderId::Bfl, "sekrit-key-zz", cx)
    });
    panel
        .update(cx, |panel, cx| panel.test_key(ProviderId::Bfl, cx))
        .await;
    assert!(notes.read_with(cx, |notes, _| {
        notes.last_toast_text().contains("invalid key")
    }));
    panel
        .update(cx, |panel, cx| panel.test_key(ProviderId::Bfl, cx))
        .await;
    assert!(notes.read_with(cx, |notes, _| {
        notes.last_toast_text().contains("key accepted")
    }));
    let logged = panel.read_with(cx, |panel, _| panel.logged_test_lines());
    assert!(!logged.iter().any(|line| line.contains("sekrit-key-zz")));
}

#[gpui::test]
async fn each_provider_row_keeps_its_own_test_result(cx: &mut TestAppContext) {
    use easel_core::http::HttpResponse;
    use easel_core::providers::ModelRegistry;
    use gpui::prelude::*;

    use crate::net::FakeTransport;
    use crate::notifications::Notifications;
    use crate::settings::SettingsPanel;

    let transport = Arc::new(FakeTransport::scripted(vec![
        Ok(HttpResponse {
            status: 401,
            headers: Vec::new(),
            body: b"no".to_vec(),
        }),
        Ok(HttpResponse {
            status: 404,
            headers: Vec::new(),
            body: b"missing".to_vec(),
        }),
    ]));
    let notes = cx.new(|_| Notifications::new());
    let panel = cx.new({
        let notes = notes.clone();
        let transport = Arc::clone(&transport);
        move |cx| SettingsPanel::new(notes, transport, cx)
    });
    let titles = panel.read_with(cx, |panel, _| panel.provider_titles());
    assert_eq!(titles.len(), ModelRegistry::providers().len());
    assert!(titles.iter().any(|title| title == "Black Forest Labs"));
    assert!(titles.iter().any(|title| title == "Krea"));
    assert!(titles.iter().any(|title| title == "OpenAI"));

    panel.update(cx, |panel, cx| {
        panel.set_draft(ProviderId::Krea, "krea-secret", cx);
        panel.set_draft(ProviderId::Bfl, "bfl-secret", cx);
    });
    panel
        .update(cx, |panel, cx| panel.test_key(ProviderId::Krea, cx))
        .await;
    panel
        .update(cx, |panel, cx| panel.test_key(ProviderId::Bfl, cx))
        .await;
    let (krea, bfl) = panel.read_with(cx, |panel, _| {
        (
            panel.row_note(ProviderId::Krea).to_owned(),
            panel.row_note(ProviderId::Bfl).to_owned(),
        )
    });
    assert!(krea.contains("invalid key"), "{krea}");
    assert!(bfl.contains("key accepted"), "{bfl}");
    let seen = transport.seen();
    assert!(seen[0].url.starts_with("https://api.krea.ai/jobs/"));
    assert!(seen[1].url.starts_with("https://api.bfl.ai/"));
    assert_eq!(seen[0].auth, Some(ProviderId::Krea));
    assert_eq!(seen[1].auth, Some(ProviderId::Bfl));
}

#[gpui::test]
async fn google_settings_row_probes_models_with_a_get(cx: &mut TestAppContext) {
    use easel_core::http::{HttpResponse, Method};
    use easel_core::model_spec::AuthScheme;
    use gpui::prelude::*;

    use crate::net::FakeTransport;
    use crate::notifications::Notifications;
    use crate::settings::{SettingsPanel, key_probe};

    let probe = key_probe(ProviderId::Google);
    assert_eq!(probe.method, Method::Get);
    assert_eq!(
        probe.url,
        "https://generativelanguage.googleapis.com/v1beta/models"
    );
    assert!(!probe.url.contains("key="));
    assert_eq!(probe.auth, Some(ProviderId::Google));
    assert!(matches!(
        ProviderId::Google.auth_scheme(),
        AuthScheme::HeaderKey { ref header } if header == "x-goog-api-key"
    ));

    let transport = Arc::new(FakeTransport::scripted(vec![Ok(HttpResponse {
        status: 200,
        headers: Vec::new(),
        body: b"{}".to_vec(),
    })]));
    let notes = cx.new(|_| Notifications::new());
    let panel = cx.new({
        let notes = notes.clone();
        let transport = Arc::clone(&transport);
        move |cx| SettingsPanel::new(notes, transport, cx)
    });
    let titles = panel.read_with(cx, |panel, _| panel.provider_titles());
    assert!(titles.iter().any(|title| title == "Google"));
    panel
        .update(cx, |panel, cx| panel.test_key(ProviderId::Google, cx))
        .await;
    let seen = transport.seen();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].method, Method::Get);
    assert_eq!(seen[0].url, probe.url);
    assert_eq!(
        panel.read_with(cx, |panel, _| panel.row_note(ProviderId::Google).to_owned()),
        "key accepted"
    );
}

#[gpui::test]
async fn openai_settings_probe_uses_get_models(cx: &mut TestAppContext) {
    use easel_core::http::{HttpResponse, Method};
    use gpui::prelude::*;

    use crate::net::FakeTransport;
    use crate::notifications::Notifications;
    use crate::settings::{SettingsPanel, key_probe};

    let probe = key_probe(ProviderId::OpenAi);
    assert_eq!(probe.method, Method::Get);
    assert_eq!(probe.url, "https://api.openai.com/v1/models");
    assert_eq!(probe.auth, Some(ProviderId::OpenAi));

    for (status, note) in [(401, "invalid key"), (200, "key accepted")] {
        let transport = Arc::new(FakeTransport::scripted(vec![Ok(HttpResponse {
            status,
            headers: Vec::new(),
            body: b"{}".to_vec(),
        })]));
        let notes = cx.new(|_| Notifications::new());
        let panel = cx.new({
            let notes = notes.clone();
            let transport = Arc::clone(&transport);
            move |cx| SettingsPanel::new(notes, transport, cx)
        });
        let titles = panel.read_with(cx, |panel, _| panel.provider_titles());
        assert!(titles.iter().any(|title| title == "OpenAI"));
        panel
            .update(cx, |panel, cx| panel.test_key(ProviderId::OpenAi, cx))
            .await;
        let seen = transport.seen();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].method, Method::Get);
        assert_eq!(seen[0].url, probe.url);
        assert!(
            !seen.iter().any(|request| request.url.contains("/images/")),
            "{seen:?}"
        );
        assert_eq!(
            panel.read_with(cx, |panel, _| panel.row_note(ProviderId::OpenAi).to_owned()),
            note
        );
    }
}

#[gpui::test]
async fn xai_settings_probe_uses_get_api_key(cx: &mut TestAppContext) {
    use easel_core::http::{HttpResponse, Method};
    use easel_core::model_spec::AuthScheme;
    use gpui::prelude::*;

    use crate::net::FakeTransport;
    use crate::notifications::Notifications;
    use crate::settings::{SettingsPanel, key_probe};

    let probe = key_probe(ProviderId::Xai);
    assert_eq!(probe.method, Method::Get);
    assert_eq!(probe.url, "https://api.x.ai/v1/api-key");
    assert_eq!(probe.auth, Some(ProviderId::Xai));
    assert!(matches!(ProviderId::Xai.auth_scheme(), AuthScheme::Bearer));

    for (status, note) in [
        (401, "invalid key"),
        (200, "key accepted"),
        (404, "key accepted"),
    ] {
        let transport = Arc::new(FakeTransport::scripted(vec![Ok(HttpResponse {
            status,
            headers: Vec::new(),
            body: b"{}".to_vec(),
        })]));
        let notes = cx.new(|_| Notifications::new());
        let panel = cx.new({
            let notes = notes.clone();
            let transport = Arc::clone(&transport);
            move |cx| SettingsPanel::new(notes, transport, cx)
        });
        let titles = panel.read_with(cx, |panel, _| panel.provider_titles());
        assert!(titles.iter().any(|title| title == "xAI"));
        panel
            .update(cx, |panel, cx| panel.test_key(ProviderId::Xai, cx))
            .await;
        let seen = transport.seen();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].method, Method::Get);
        assert_eq!(seen[0].url, probe.url);
        assert_eq!(seen[0].auth, Some(ProviderId::Xai));
        assert_eq!(
            panel.read_with(cx, |panel, _| panel.row_note(ProviderId::Xai).to_owned()),
            note
        );
    }
}

#[test]
fn chutes_probe_is_a_get_of_the_quotas_endpoint() {
    use crate::settings::key_probe;
    use easel_core::http::{Body, Method};
    use easel_core::model_spec::AuthScheme;

    let probe = key_probe(ProviderId::Chutes);
    assert_eq!(probe.method, Method::Get);
    assert_eq!(probe.url, "https://api.chutes.ai/users/me/quotas");
    assert_eq!(probe.auth, Some(ProviderId::Chutes));
    assert_eq!(probe.body, Body::Empty);
    assert!(matches!(
        ProviderId::Chutes.auth_scheme(),
        AuthScheme::Bearer
    ));
}

#[gpui::test]
fn the_provider_menu_selects_which_key_form_is_active(cx: &mut TestAppContext) {
    use gpui::prelude::*;

    use crate::net::FakeTransport;
    use crate::notifications::Notifications;
    use crate::settings::SettingsPanel;

    let notes = cx.new(|_| Notifications::new());
    let transport = Arc::new(FakeTransport::scripted(Vec::new()));
    let panel = cx.new(move |cx| SettingsPanel::new(notes, transport, cx));
    assert_eq!(
        panel.read_with(cx, |panel, cx| panel.selected_provider(cx)),
        ProviderId::Bfl
    );
    assert_eq!(
        panel.read_with(cx, |panel, cx| {
            panel.provider_menu().read(cx).label().map(str::to_owned)
        }),
        Some("Provider".to_owned())
    );
    panel.update(cx, |panel, cx| {
        panel.provider_menu().update(cx, |select, cx| {
            select.set_value(serde_json::Value::String("OpenAI".into()), cx);
        });
    });
    assert_eq!(
        panel.read_with(cx, |panel, cx| panel.selected_provider(cx)),
        ProviderId::OpenAi
    );
}
