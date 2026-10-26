#![cfg_attr(not(test), allow(dead_code))]

use std::fmt;

use easel_core::{http::HttpRequest, model_spec::ProviderId};
use gpui::{App, Task};
use zeroize::Zeroizing;

mod reqwest_transport;

#[cfg_attr(not(test), allow(unused_imports))]
pub use reqwest_transport::ReqwestTransport;

pub trait KeyResolver: Send + Sync {
    fn key_for(&self, provider: ProviderId) -> Option<Zeroizing<String>>;
}

pub trait HttpTransport: Send + Sync {
    fn request(
        &self,
        request: HttpRequest,
        cx: &mut App,
    ) -> Task<Result<easel_core::http::HttpResponse, TransportError>>;
}

#[derive(Clone, PartialEq, Eq)]
pub enum TransportError {
    InsecureUrl { url: String },
    NoCredential(ProviderId),
    TooLarge { method: String, url: String },
    Timeout { method: String, url: String },
    Network { method: String, url: String },
}

impl fmt::Debug for TransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InsecureUrl { url } => formatter
                .debug_struct("InsecureUrl")
                .field("url", url)
                .finish(),
            Self::NoCredential(provider) => formatter
                .debug_tuple("NoCredential")
                .field(provider)
                .finish(),
            Self::TooLarge { method, url } => formatter
                .debug_struct("TooLarge")
                .field("method", method)
                .field("url", url)
                .finish(),
            Self::Timeout { method, url } => formatter
                .debug_struct("Timeout")
                .field("method", method)
                .field("url", url)
                .finish(),
            Self::Network { method, url } => formatter
                .debug_struct("Network")
                .field("method", method)
                .field("url", url)
                .finish(),
        }
    }
}

impl fmt::Display for TransportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InsecureUrl { url } => write!(formatter, "refusing non-https url {url}"),
            Self::NoCredential(provider) => write!(formatter, "no credential for {provider:?}"),
            Self::TooLarge { method, url } => {
                write!(formatter, "{method} {url} exceeded the size limit")
            }
            Self::Timeout { method, url } => write!(formatter, "{method} {url} timed out"),
            Self::Network { method, url } => write!(formatter, "{method} {url} failed"),
        }
    }
}

#[cfg(test)]
#[derive(Clone, Default)]
pub struct MapKeys {
    bfl: Option<String>,
    krea: Option<String>,
    google: Option<String>,
    openai: Option<String>,
    xai: Option<String>,
    chutes: Option<String>,
    qwen: Option<String>,
    apiframe: Option<String>,
    leonardo: Option<String>,
}

#[cfg(test)]
impl MapKeys {
    pub fn insert(mut self, provider: ProviderId, key: impl Into<String>) -> Self {
        match provider {
            ProviderId::Bfl => self.bfl = Some(key.into()),
            ProviderId::Krea => self.krea = Some(key.into()),
            ProviderId::Google => self.google = Some(key.into()),
            ProviderId::OpenAi => self.openai = Some(key.into()),
            ProviderId::Xai => self.xai = Some(key.into()),
            ProviderId::Chutes => self.chutes = Some(key.into()),
            ProviderId::Qwen => self.qwen = Some(key.into()),
            ProviderId::Apiframe => self.apiframe = Some(key.into()),
            ProviderId::Leonardo => self.leonardo = Some(key.into()),
        }
        self
    }
}

#[cfg(test)]
impl KeyResolver for MapKeys {
    fn key_for(&self, provider: ProviderId) -> Option<Zeroizing<String>> {
        let key = match provider {
            ProviderId::Bfl => self.bfl.clone(),
            ProviderId::Krea => self.krea.clone(),
            ProviderId::Google => self.google.clone(),
            ProviderId::OpenAi => self.openai.clone(),
            ProviderId::Xai => self.xai.clone(),
            ProviderId::Chutes => self.chutes.clone(),
            ProviderId::Qwen => self.qwen.clone(),
            ProviderId::Apiframe => self.apiframe.clone(),
            ProviderId::Leonardo => self.leonardo.clone(),
        };
        key.map(Zeroizing::new)
    }
}

#[cfg(test)]
#[derive(Clone, Default)]
pub struct FakeTransport {
    seen: std::sync::Arc<std::sync::Mutex<Vec<HttpRequest>>>,
    script: std::sync::Arc<
        std::sync::Mutex<
            std::collections::VecDeque<Result<easel_core::http::HttpResponse, TransportError>>,
        >,
    >,
}

#[cfg(test)]
impl FakeTransport {
    pub fn scripted(
        responses: Vec<Result<easel_core::http::HttpResponse, TransportError>>,
    ) -> Self {
        Self {
            seen: std::sync::Arc::default(),
            script: std::sync::Arc::new(std::sync::Mutex::new(responses.into())),
        }
    }

    pub fn seen(&self) -> Vec<HttpRequest> {
        self.seen.lock().expect("seen").clone()
    }
}

#[cfg(test)]
impl HttpTransport for FakeTransport {
    fn request(
        &self,
        request: HttpRequest,
        cx: &mut App,
    ) -> Task<Result<easel_core::http::HttpResponse, TransportError>> {
        self.seen.lock().expect("seen").push(request);
        let next = self
            .script
            .lock()
            .expect("script")
            .pop_front()
            .unwrap_or_else(|| {
                Err(TransportError::Network {
                    method: "GET".into(),
                    url: "fake".into(),
                })
            });
        cx.spawn(async move |_| next)
    }
}
