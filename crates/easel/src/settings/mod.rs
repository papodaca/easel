#[cfg(test)]
mod credentials;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use easel_core::http::{Body, HttpRequest, Method};
use easel_core::model_spec::ProviderId;
use gpui::{
    Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div, prelude::*, px, rgb,
};
use zeroize::Zeroizing;

use crate::net::{HttpTransport, KeyResolver};
use crate::notifications::{Notifications, Toast, ToastLevel};
use crate::theme;
use crate::ui::{Button, muted_text, section_label};
use crate::widgets::{TextInput, field_row};

const TEST_URL: &str = "https://api.bfl.ai/v1/get_result?id=nonexistent";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Source {
    Env,
    Store,
    Session,
}

#[cfg_attr(not(test), allow(dead_code))]
pub enum SaveOutcome {
    Stored,
    KeptInSession { message: &'static str },
    Rejected { message: String },
}

pub trait CredentialStore {
    fn read(&self, provider: ProviderId) -> Result<Option<Zeroizing<String>>, ()>;
    fn write(&self, provider: ProviderId, key: &str) -> Result<(), ()>;
    fn delete(&self, provider: ProviderId) -> Result<(), ()>;
}

#[derive(Default)]
pub struct MemoryStore {
    keys: Mutex<HashMap<ProviderId, String>>,
}

impl CredentialStore for MemoryStore {
    fn read(&self, provider: ProviderId) -> Result<Option<Zeroizing<String>>, ()> {
        Ok(self
            .keys
            .lock()
            .expect("memory store")
            .get(&provider)
            .cloned()
            .map(Zeroizing::new))
    }

    fn write(&self, provider: ProviderId, key: &str) -> Result<(), ()> {
        self.keys
            .lock()
            .expect("memory store")
            .insert(provider, key.to_owned());
        Ok(())
    }

    fn delete(&self, provider: ProviderId) -> Result<(), ()> {
        self.keys.lock().expect("memory store").remove(&provider);
        Ok(())
    }
}

#[cfg(test)]
pub struct FailingStore;

#[cfg(test)]
impl CredentialStore for FailingStore {
    fn read(&self, _: ProviderId) -> Result<Option<Zeroizing<String>>, ()> {
        Err(())
    }

    fn write(&self, _: ProviderId, _: &str) -> Result<(), ()> {
        Err(())
    }

    fn delete(&self, _: ProviderId) -> Result<(), ()> {
        Err(())
    }
}

#[derive(Default)]
pub struct SessionKeys {
    bfl: Option<Zeroizing<String>>,
    krea: Option<Zeroizing<String>>,
}

impl SessionKeys {
    pub fn set(&mut self, provider: ProviderId, key: impl Into<String>) {
        let key = Zeroizing::new(key.into());
        match provider {
            ProviderId::Bfl => self.bfl = Some(key),
            ProviderId::Krea => self.krea = Some(key),
        }
    }

    pub fn clear(&mut self, provider: ProviderId) {
        match provider {
            ProviderId::Bfl => self.bfl = None,
            ProviderId::Krea => self.krea = None,
        }
    }

    fn get(&self, provider: ProviderId) -> Option<Zeroizing<String>> {
        match provider {
            ProviderId::Bfl => self.bfl.clone(),
            ProviderId::Krea => self.krea.clone(),
        }
    }
}

pub struct SharedKeys {
    memory: MemoryStore,
    session: Mutex<SessionKeys>,
}

impl Default for SharedKeys {
    fn default() -> Self {
        Self {
            memory: MemoryStore::default(),
            session: Mutex::new(SessionKeys::default()),
        }
    }
}

impl SharedKeys {
    fn lock_session(&self) -> std::sync::MutexGuard<'_, SessionKeys> {
        self.session.lock().expect("session keys")
    }

    pub fn set_session(&self, provider: ProviderId, key: impl Into<String>) {
        self.lock_session().set(provider, key);
    }
}

pub struct Resolver(pub Arc<SharedKeys>);

impl KeyResolver for Resolver {
    fn key_for(&self, provider: ProviderId) -> Option<Zeroizing<String>> {
        let mut env = HashMap::new();
        if let Ok(value) = std::env::var(provider.env_var()) {
            env.insert(provider.env_var().to_owned(), value);
        }
        let session = self.0.lock_session();
        resolve_key(provider, &env, &self.0.memory, &session).map(|(key, _)| key)
    }
}

pub fn resolve_key(
    provider: ProviderId,
    env: &HashMap<String, String>,
    store: &dyn CredentialStore,
    session: &SessionKeys,
) -> Option<(Zeroizing<String>, Source)> {
    if let Some(value) = env.get(provider.env_var()) {
        let value = value.trim();
        if !value.is_empty() {
            return Some((Zeroizing::new(value.to_owned()), Source::Env));
        }
    }
    match store.read(provider) {
        Ok(Some(key)) if !key.is_empty() => Some((key, Source::Store)),
        Ok(_) | Err(_) => session.get(provider).map(|key| (key, Source::Session)),
    }
}

#[cfg_attr(not(test), allow(dead_code))]
pub fn save_key(
    provider: ProviderId,
    raw: &str,
    store: &dyn CredentialStore,
    session: &mut SessionKeys,
) -> SaveOutcome {
    let key = match normalize_key(raw) {
        Ok(key) => key,
        Err(message) => return SaveOutcome::Rejected { message },
    };
    match store.write(provider, &key) {
        Ok(()) => {
            session.set(provider, key);
            SaveOutcome::Stored
        }
        Err(()) => {
            session.set(provider, key);
            SaveOutcome::KeptInSession {
                message: "Credential store is unavailable. The key is kept for this session.",
            }
        }
    }
}

fn normalize_key(raw: &str) -> Result<String, String> {
    let trimmed = raw.trim();
    let visible = !trimmed.is_empty()
        && trimmed
            .chars()
            .all(|ch| ch.is_ascii() && (0x20..=0x7E).contains(&(ch as u32)));
    if visible {
        Ok(trimmed.to_owned())
    } else {
        Err("Use visible ASCII characters only.".into())
    }
}

pub struct SettingsPanel {
    notifications: Entity<Notifications>,
    transport: Arc<dyn HttpTransport>,
    keys: Arc<SharedKeys>,
    draft: Entity<TextInput>,
    note: gpui::SharedString,
    field_message: gpui::SharedString,
    test_log: Vec<String>,
    revealed: bool,
    data_directory: gpui::SharedString,
}

impl SettingsPanel {
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn new(
        notifications: Entity<Notifications>,
        transport: Arc<dyn HttpTransport>,
        cx: &mut Context<Self>,
    ) -> Self {
        Self::open(
            notifications,
            transport,
            Arc::new(SharedKeys::default()),
            cx,
        )
    }

    pub fn open(
        notifications: Entity<Notifications>,
        transport: Arc<dyn HttpTransport>,
        keys: Arc<SharedKeys>,
        cx: &mut Context<Self>,
    ) -> Self {
        let draft = cx.new(|cx| {
            let mut input = TextInput::new(cx);
            input.set_masked(true, cx);
            input.set_label("BFL API key", cx);
            input
        });
        let url = ProviderId::Bfl.base_url().to_owned();
        let keys_for_load = Arc::clone(&keys);
        let read = cx.read_credentials(&url);
        cx.spawn(async move |this, cx| {
            let result = read.await;
            this.update(cx, |this, cx| {
                match result {
                    Ok(Some((_, bytes))) => {
                        if let Ok(text) = String::from_utf8(bytes)
                            && normalize_key(&text).is_ok()
                        {
                            this.keys.memory.write(ProviderId::Bfl, &text).ok();
                            this.note = "Source: credential store".into();
                        }
                    }
                    Ok(None) => {}
                    Err(_) => {
                        this.note = "Credential store is unavailable. Keys stay in memory for this session.".into();
                    }
                }
                let _ = keys_for_load;
                cx.notify();
            })
            .ok();
        })
        .detach();
        Self {
            notifications,
            transport,
            keys,
            draft,
            note: "Source: none".into(),
            field_message: gpui::SharedString::default(),
            test_log: Vec::new(),
            revealed: false,
            data_directory: gpui::SharedString::default(),
        }
    }

    pub fn set_data_directory(&mut self, path: impl Into<String>, cx: &mut Context<Self>) {
        self.data_directory = path.into().into();
        cx.notify();
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_draft(&mut self, text: &str, cx: &mut Context<Self>) {
        self.draft.update(cx, |input, cx| input.set_text(text, cx));
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn logged_test_lines(&self) -> Vec<String> {
        self.test_log.clone()
    }

    pub fn test_key(&mut self, cx: &mut Context<Self>) -> gpui::Task<()> {
        let draft = self.draft.read(cx).text().to_owned();
        if let Ok(key) = normalize_key(&draft) {
            self.keys.lock_session().set(ProviderId::Bfl, key);
        }
        self.test_log.push("BFL key test".into());
        log::debug!("BFL key test");
        let request = HttpRequest {
            method: Method::Get,
            url: TEST_URL.to_owned(),
            headers: Vec::new(),
            auth: Some(ProviderId::Bfl),
            body: Body::Empty,
            max_body_bytes: 64 * 1024,
        };
        let response = self.transport.request(request, cx);
        let notes = self.notifications.clone();
        cx.spawn(async move |this, cx| {
            let result = response.await;
            let (text, level) = match result {
                Ok(response) if response.status == 401 || response.status == 403 => {
                    ("invalid key", ToastLevel::Error)
                }
                Ok(_) => ("key accepted", ToastLevel::Info),
                Err(_) => ("couldn't reach the API", ToastLevel::Error),
            };
            notes.update(cx, |notes, cx| {
                notes.push(
                    Toast {
                        level,
                        text: text.into(),
                        ttl: std::time::Duration::from_secs(6),
                        action: None,
                    },
                    cx,
                );
            });
            this.update(cx, |this, cx| {
                this.note = text.into();
                cx.notify();
            })
            .ok();
        })
    }

    fn save(&mut self, cx: &mut Context<Self>) {
        let draft = self.draft.read(cx).text().to_owned();
        let key = match normalize_key(&draft) {
            Ok(key) => key,
            Err(message) => {
                self.field_message = message.into();
                cx.notify();
                return;
            }
        };
        self.field_message = gpui::SharedString::default();
        let url = ProviderId::Bfl.base_url().to_owned();
        let secret = key.clone();
        let write = cx.write_credentials(&url, "api", secret.as_bytes());
        let keys = Arc::clone(&self.keys);
        cx.spawn(async move |this, cx| {
            let stored = write.await.is_ok();
            this.update(cx, |this, cx| {
                if stored {
                    keys.memory.write(ProviderId::Bfl, &key).ok();
                    this.note = "Source: credential store".into();
                } else {
                    keys.memory.delete(ProviderId::Bfl).ok();
                    keys.lock_session().set(ProviderId::Bfl, key);
                    this.note =
                        "Credential store is unavailable. The key is kept for this session.".into();
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn clear(&mut self, cx: &mut Context<Self>) {
        let url = ProviderId::Bfl.base_url().to_owned();
        let delete = cx.delete_credentials(&url);
        self.keys.memory.delete(ProviderId::Bfl).ok();
        self.keys.lock_session().clear(ProviderId::Bfl);
        self.draft.update(cx, |input, cx| input.set_text("", cx));
        self.note = "Source: none".into();
        cx.spawn(async move |_, _| {
            delete.await.ok();
        })
        .detach();
        cx.notify();
    }

    fn reveal(&mut self, cx: &mut Context<Self>) {
        self.revealed = !self.revealed;
        let revealed = self.revealed;
        self.draft
            .update(cx, |input, cx| input.set_masked(!revealed, cx));
    }
}

impl Render for SettingsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let note = self.note.clone();
        let field_message = self.field_message.clone();
        let reveal_label = if self.revealed { "Hide" } else { "Reveal" };
        div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .w_full()
            .text_color(rgb(theme::TEXT))
            .child(section_label("Black Forest Labs"))
            .child(self.draft.clone())
            .child(muted_text(note))
            .when(!field_message.is_empty(), |column| {
                column.child(
                    div()
                        .text_sm()
                        .text_color(rgb(theme::ERROR))
                        .child(field_message),
                )
            })
            .child(
                div()
                    .flex()
                    .flex_row()
                    .items_center()
                    .gap(px(6.0))
                    .child(
                        Button::filled("settings-save", "Save")
                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                    )
                    .child(
                        Button::filled("settings-test", "Test").on_click(cx.listener(
                            |this, _, _, cx| {
                                this.test_key(cx).detach();
                            },
                        )),
                    )
                    .child(div().flex_1())
                    .child(
                        Button::ghost("settings-reveal", reveal_label)
                            .on_click(cx.listener(|this, _, _, cx| this.reveal(cx))),
                    )
                    .child(
                        Button::ghost("settings-clear", "Clear")
                            .on_click(cx.listener(|this, _, _, cx| this.clear(cx))),
                    ),
            )
            .child(field_row(
                Some("Data directory"),
                None,
                div().text_sm().child(self.data_directory.clone()),
            ))
    }
}
