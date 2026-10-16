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
    panel.update(cx, |panel, cx| panel.set_draft("sekrit-key-zz", cx));
    panel.update(cx, |panel, cx| panel.test_key(cx)).await;
    assert!(notes.read_with(cx, |notes, _| {
        notes.last_toast_text().contains("invalid key")
    }));
    panel.update(cx, |panel, cx| panel.test_key(cx)).await;
    assert!(notes.read_with(cx, |notes, _| {
        notes.last_toast_text().contains("key accepted")
    }));
    let logged = panel.read_with(cx, |panel, _| panel.logged_test_lines());
    assert!(!logged.iter().any(|line| line.contains("sekrit-key-zz")));
}
