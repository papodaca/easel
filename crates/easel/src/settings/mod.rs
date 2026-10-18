#[cfg(test)]
mod credentials;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use easel_core::http::{Body, HttpRequest, Method};
use easel_core::model_spec::ProviderId;
use easel_core::providers::ModelRegistry;
use gpui::{
    Context, Entity, IntoElement, ParentElement, Render, Styled, Window, div, prelude::*, px, rgb,
};
use zeroize::Zeroizing;

use crate::net::{HttpTransport, KeyResolver};
use crate::notifications::{Notifications, Toast, ToastLevel};
use crate::theme;
use crate::ui::{Button, muted_text, section_label};
use crate::widgets::{TextInput, field_row};

/// Probe the provider uses to decide whether a key is accepted.
/// Krea asks for a fresh job id; a valid key gets 404, an invalid key gets 401.
pub fn key_probe(provider: ProviderId) -> HttpRequest {
    let url = match provider {
        ProviderId::Bfl => "https://api.bfl.ai/v1/get_result?id=nonexistent".to_owned(),
        ProviderId::Krea => format!("https://api.krea.ai/jobs/{}", fresh_uuid()),
        ProviderId::Google => "https://generativelanguage.googleapis.com/v1beta/models".to_owned(),
        ProviderId::OpenAi => "https://api.openai.com/v1/models".to_owned(),
    };
    HttpRequest {
        method: Method::Get,
        url,
        headers: Vec::new(),
        auth: Some(provider),
        body: Body::Empty,
        max_body_bytes: 64 * 1024,
    }
}

fn fresh_uuid() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let mut hasher = RandomState::new().build_hasher();
    hasher.write_u64(COUNTER.fetch_add(1, Ordering::Relaxed));
    let mixed = hasher.finish();
    hasher.write_u64(mixed);
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&hasher.finish().to_le_bytes());
    hasher.write_u8(1);
    bytes[8..].copy_from_slice(&hasher.finish().to_le_bytes());
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0],
        bytes[1],
        bytes[2],
        bytes[3],
        bytes[4],
        bytes[5],
        bytes[6],
        bytes[7],
        bytes[8],
        bytes[9],
        bytes[10],
        bytes[11],
        bytes[12],
        bytes[13],
        bytes[14],
        bytes[15],
    )
}

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
    keys: Mutex<HashMap<String, String>>,
}

impl MemoryStore {
    #[cfg(test)]
    pub fn stores_under(&self, service: &str) -> bool {
        self.keys
            .lock()
            .expect("memory store")
            .contains_key(service)
    }
}

impl CredentialStore for MemoryStore {
    fn read(&self, provider: ProviderId) -> Result<Option<Zeroizing<String>>, ()> {
        Ok(self
            .keys
            .lock()
            .expect("memory store")
            .get(provider.base_url())
            .cloned()
            .map(Zeroizing::new))
    }

    fn write(&self, provider: ProviderId, key: &str) -> Result<(), ()> {
        self.keys
            .lock()
            .expect("memory store")
            .insert(provider.base_url().to_owned(), key.to_owned());
        Ok(())
    }

    fn delete(&self, provider: ProviderId) -> Result<(), ()> {
        self.keys
            .lock()
            .expect("memory store")
            .remove(provider.base_url());
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
    google: Option<Zeroizing<String>>,
    openai: Option<Zeroizing<String>>,
}

impl SessionKeys {
    pub fn set(&mut self, provider: ProviderId, key: impl Into<String>) {
        let key = Zeroizing::new(key.into());
        match provider {
            ProviderId::Bfl => self.bfl = Some(key),
            ProviderId::Krea => self.krea = Some(key),
            ProviderId::Google => self.google = Some(key),
            ProviderId::OpenAi => self.openai = Some(key),
        }
    }

    pub fn clear(&mut self, provider: ProviderId) {
        match provider {
            ProviderId::Bfl => self.bfl = None,
            ProviderId::Krea => self.krea = None,
            ProviderId::Google => self.google = None,
            ProviderId::OpenAi => self.openai = None,
        }
    }

    fn get(&self, provider: ProviderId) -> Option<Zeroizing<String>> {
        match provider {
            ProviderId::Bfl => self.bfl.clone(),
            ProviderId::Krea => self.krea.clone(),
            ProviderId::Google => self.google.clone(),
            ProviderId::OpenAi => self.openai.clone(),
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

    #[cfg_attr(not(test), allow(dead_code))]
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

struct ProviderRow {
    provider: ProviderId,
    draft: Entity<TextInput>,
    note: gpui::SharedString,
    field_message: gpui::SharedString,
    revealed: bool,
}

pub struct SettingsPanel {
    notifications: Entity<Notifications>,
    transport: Arc<dyn HttpTransport>,
    keys: Arc<SharedKeys>,
    rows: Vec<ProviderRow>,
    test_log: Vec<String>,
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
        let rows = ModelRegistry::providers()
            .iter()
            .copied()
            .map(|provider| {
                let label = format!("{} API key", provider.display_name());
                let draft = cx.new(|cx| {
                    let mut input = TextInput::new(cx);
                    input.set_masked(true, cx);
                    input.set_label(label, cx);
                    input
                });
                ProviderRow {
                    provider,
                    draft,
                    note: "Source: none".into(),
                    field_message: gpui::SharedString::default(),
                    revealed: false,
                }
            })
            .collect();
        let reads: Vec<_> = ModelRegistry::providers()
            .iter()
            .copied()
            .map(|provider| {
                let url = provider.base_url().to_owned();
                (provider, cx.read_credentials(&url))
            })
            .collect();
        cx.spawn(async move |this, cx| {
            let mut loaded = Vec::new();
            for (provider, read) in reads {
                loaded.push((provider, read.await));
            }
            this.update(cx, |this, cx| {
                for (provider, result) in loaded {
                    match result {
                        Ok(Some((_, bytes))) => {
                            if let Ok(text) = String::from_utf8(bytes)
                                && normalize_key(&text).is_ok()
                            {
                                this.keys.memory.write(provider, &text).ok();
                                this.row_mut(provider).note = "Source: credential store".into();
                            }
                        }
                        Ok(None) => {}
                        Err(_) => {
                            this.row_mut(provider).note =
                                "Credential store is unavailable. Keys stay in memory for this session."
                                    .into();
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        Self {
            notifications,
            transport,
            keys,
            rows,
            test_log: Vec::new(),
            data_directory: gpui::SharedString::default(),
        }
    }

    pub fn set_data_directory(&mut self, path: impl Into<String>, cx: &mut Context<Self>) {
        self.data_directory = path.into().into();
        cx.notify();
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_draft(&mut self, provider: ProviderId, text: &str, cx: &mut Context<Self>) {
        self.row(provider)
            .draft
            .update(cx, |input, cx| input.set_text(text, cx));
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn logged_test_lines(&self) -> Vec<String> {
        self.test_log.clone()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn provider_titles(&self) -> Vec<String> {
        self.rows
            .iter()
            .map(|row| row.provider.display_name().to_owned())
            .collect()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn row_note(&self, provider: ProviderId) -> &str {
        self.row(provider).note.as_ref()
    }

    pub fn test_key(&mut self, provider: ProviderId, cx: &mut Context<Self>) -> gpui::Task<()> {
        let draft = self.row(provider).draft.read(cx).text().to_owned();
        if let Ok(key) = normalize_key(&draft) {
            self.keys.lock_session().set(provider, key);
        }
        let label = format!("{} key test", provider.display_name());
        log::debug!("{label}");
        self.test_log.push(label);
        let request = key_probe(provider);
        let response = self.transport.request(request, cx);
        let notes = self.notifications.clone();
        cx.spawn(async move |this, cx| {
            let result = response.await;
            let (text, level) = match result {
                Ok(response)
                    if response.status == 401
                        || (provider != ProviderId::Google && response.status == 403) =>
                {
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
                this.row_mut(provider).note = text.into();
                cx.notify();
            })
            .ok();
        })
    }

    fn row(&self, provider: ProviderId) -> &ProviderRow {
        self.rows
            .iter()
            .find(|row| row.provider == provider)
            .expect("registered provider")
    }

    fn row_mut(&mut self, provider: ProviderId) -> &mut ProviderRow {
        self.rows
            .iter_mut()
            .find(|row| row.provider == provider)
            .expect("registered provider")
    }

    fn save(&mut self, provider: ProviderId, cx: &mut Context<Self>) {
        let draft = self.row(provider).draft.read(cx).text().to_owned();
        let key = match normalize_key(&draft) {
            Ok(key) => key,
            Err(message) => {
                self.row_mut(provider).field_message = message.into();
                cx.notify();
                return;
            }
        };
        self.row_mut(provider).field_message = gpui::SharedString::default();
        let url = provider.base_url().to_owned();
        let secret = key.clone();
        let write = cx.write_credentials(&url, "api", secret.as_bytes());
        let keys = Arc::clone(&self.keys);
        cx.spawn(async move |this, cx| {
            let stored = write.await.is_ok();
            this.update(cx, |this, cx| {
                if stored {
                    keys.memory.write(provider, &key).ok();
                    this.row_mut(provider).note = "Source: credential store".into();
                } else {
                    keys.memory.delete(provider).ok();
                    keys.lock_session().set(provider, key);
                    this.row_mut(provider).note =
                        "Credential store is unavailable. The key is kept for this session.".into();
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn clear(&mut self, provider: ProviderId, cx: &mut Context<Self>) {
        let url = provider.base_url().to_owned();
        let delete = cx.delete_credentials(&url);
        self.keys.memory.delete(provider).ok();
        self.keys.lock_session().clear(provider);
        let row = self.row_mut(provider);
        row.draft.update(cx, |input, cx| input.set_text("", cx));
        row.note = "Source: none".into();
        cx.spawn(async move |_, _| {
            delete.await.ok();
        })
        .detach();
        cx.notify();
    }

    fn reveal(&mut self, provider: ProviderId, cx: &mut Context<Self>) {
        let revealed = !self.row(provider).revealed;
        self.row_mut(provider).revealed = revealed;
        self.row(provider)
            .draft
            .update(cx, |input, cx| input.set_masked(!revealed, cx));
    }
}

impl Render for SettingsPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let rows: Vec<_> = self
            .rows
            .iter()
            .map(|row| {
                (
                    row.provider,
                    row.draft.clone(),
                    row.note.clone(),
                    row.field_message.clone(),
                    row.revealed,
                )
            })
            .collect();
        let mut column = div()
            .flex()
            .flex_col()
            .gap(px(10.0))
            .w_full()
            .text_color(rgb(theme::TEXT));
        for (provider, draft, note, field_message, revealed) in rows {
            let slug = match provider {
                ProviderId::Bfl => "bfl",
                ProviderId::Krea => "krea",
                ProviderId::Google => "google",
                ProviderId::OpenAi => "openai",
            };
            let reveal_label = if revealed { "Hide" } else { "Reveal" };
            column = column
                .child(section_label(provider.display_name()))
                .child(draft)
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
                            Button::filled(format!("settings-save-{slug}"), "Save").on_click(
                                cx.listener(move |this, _, _, cx| this.save(provider, cx)),
                            ),
                        )
                        .child(
                            Button::filled(format!("settings-test-{slug}"), "Test").on_click(
                                cx.listener(move |this, _, _, cx| {
                                    this.test_key(provider, cx).detach();
                                }),
                            ),
                        )
                        .child(div().flex_1())
                        .child(
                            Button::ghost(format!("settings-reveal-{slug}"), reveal_label)
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.reveal(provider, cx)),
                                ),
                        )
                        .child(
                            Button::ghost(format!("settings-clear-{slug}"), "Clear").on_click(
                                cx.listener(move |this, _, _, cx| this.clear(provider, cx)),
                            ),
                        ),
                );
        }
        column.child(field_row(
            Some("Data directory"),
            None,
            div().text_sm().child(self.data_directory.clone()),
        ))
    }
}
