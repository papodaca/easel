pub mod bfl;
pub mod krea;

use std::sync::OnceLock;

use crate::error::ProviderError;
use crate::model_spec::{ModelSpec, ProviderId};

/// Registered models and the providers that own their keys.
pub struct ModelRegistry;

impl ModelRegistry {
    #[must_use]
    pub fn specs() -> &'static [&'static ModelSpec] {
        static SPECS: OnceLock<Vec<&'static ModelSpec>> = OnceLock::new();
        SPECS
            .get_or_init(|| {
                vec![
                    bfl::flux3::model_spec(),
                    bfl::flux2_pro::model_spec(),
                    krea::krea3::model_spec(),
                    krea::krea2_large::model_spec(),
                ]
            })
            .as_slice()
    }

    #[must_use]
    pub fn providers() -> &'static [ProviderId] {
        &[ProviderId::Bfl, ProviderId::Krea]
    }

    #[must_use]
    pub fn auth_scheme(provider: ProviderId) -> crate::model_spec::AuthScheme {
        provider.auth_scheme()
    }
}
use serde::{Deserialize, Serialize};

/// Server job id. Only values matching `^[A-Za-z0-9_-]{1,64}$` can name a file.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct JobId(String);

impl JobId {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let valid = (1..=64).contains(&raw.len())
            && raw
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if valid {
            Ok(Self(raw.to_string()))
        } else {
            Err(format!("invalid job id `{raw}`"))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for JobId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CostQuote {
    pub cost: f64,
    pub input_mp: f64,
    pub output_mp: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Submitted {
    pub job_id: JobId,
    pub poll_url: url::Url,
    pub quote: Option<CostQuote>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PollOutcome {
    InFlight {
        status: String,
        progress: Option<f32>,
    },
    Ready {
        url: url::Url,
        settled_cost: Option<f64>,
        prompt: Option<String>,
        duration: Option<f64>,
    },
    Failed(ProviderError),
    Cancelled,
}

use crate::error::FieldError;
use crate::geometry::Size;
use crate::http::{Body, HttpRequest, HttpResponse, Method};
use crate::job::{EncodedImage, JobRequest, PollPolicy};
use crate::scene::Scene;

use self::bfl::flux3::CompiledPrompt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImageInput {
    RawBase64(String),
    Url(String),
}

impl ImageInput {
    #[must_use]
    pub fn as_str(&self) -> &str {
        match self {
            Self::RawBase64(text) | Self::Url(text) => text,
        }
    }
}

/// Place prepared images into the request fields the slot kind names.
#[must_use]
pub fn map_images(
    slots: &crate::model_spec::ImageSlots,
    inputs: &[ImageInput],
) -> std::collections::BTreeMap<String, serde_json::Value> {
    use crate::model_spec::ImageSlots;
    let mut fields = std::collections::BTreeMap::new();
    match slots {
        ImageSlots::List { max, key } => {
            let key = key.clone();
            let values = inputs
                .iter()
                .take(*max)
                .map(|input| serde_json::Value::String(input.as_str().to_owned()))
                .collect();
            if !inputs.is_empty() {
                fields.insert(key, serde_json::Value::Array(values));
            }
        }
        ImageSlots::Numbered { max } => {
            for (index, input) in inputs.iter().take(*max).enumerate() {
                let key = if index == 0 {
                    "input_image".to_owned()
                } else {
                    format!("input_image_{}", index + 1)
                };
                fields.insert(key, serde_json::Value::String(input.as_str().to_owned()));
            }
        }
        ImageSlots::SingleWithStyleRefs { max_style_refs } => {
            if let Some(input) = inputs.first() {
                fields.insert(
                    "image_url".into(),
                    serde_json::Value::String(input.as_str().to_owned()),
                );
            }
            let refs: Vec<_> = inputs
                .iter()
                .skip(1)
                .take(*max_style_refs)
                .map(|input| {
                    serde_json::json!({
                        "url": input.as_str(),
                        "strength": 0.5,
                    })
                })
                .collect();
            if !refs.is_empty() {
                fields.insert(
                    "image_style_references".into(),
                    serde_json::Value::Array(refs),
                );
            }
        }
        ImageSlots::Single => {
            if let Some(input) = inputs.first() {
                fields.insert(
                    "image_url".into(),
                    serde_json::Value::String(input.as_str().to_owned()),
                );
            }
        }
    }
    fields
}

#[derive(Clone)]
pub enum ReferenceStep {
    Inline(ImageInput),
    Upload {
        request: HttpRequest,
        content_hash: String,
    },
}

pub trait Provider {
    fn spec(&self) -> &ModelSpec;

    fn compile_prompt(&self, scene: &Scene, _frame: Size) -> CompiledPrompt {
        let found = crate::prose::tags(&scene.prose);
        if found.is_empty() {
            return CompiledPrompt {
                text: scene.prose.clone(),
                rows_json: String::new(),
                warnings: Vec::new(),
            };
        }
        let mut text = String::new();
        let mut cursor = 0;
        let mut warnings = Vec::new();
        for tag in &found {
            text.push_str(&scene.prose[cursor..tag.span.0]);
            warnings.push(format!("removed <{}>", tag.name));
            cursor = tag.span.1;
        }
        text.push_str(&scene.prose[cursor..]);
        CompiledPrompt {
            text: collapse_whitespace(&text),
            rows_json: String::new(),
            warnings,
        }
    }

    fn validate_for_submit(
        &self,
        scene: &Scene,
        values: &std::collections::BTreeMap<String, serde_json::Value>,
        image_count: usize,
    ) -> Vec<FieldError>;

    fn reference_steps(&self, images: &[EncodedImage]) -> Vec<ReferenceStep>;

    fn parse_upload(&self, _response: &HttpResponse) -> Result<ImageInput, ProviderError> {
        Err(ProviderError::Protocol("provider does not upload".into()))
    }

    fn build_submit(&self, request: &JobRequest, images: &[ImageInput]) -> HttpRequest;

    fn poll_request(&self, submitted: &Submitted) -> HttpRequest;

    fn parse_submit(&self, response: &HttpResponse) -> Result<Submitted, ProviderError>;

    fn parse_poll(&self, response: &HttpResponse) -> PollOutcome;

    fn cancel_request(
        &self,
        _submitted: &Submitted,
        _last: Option<&PollOutcome>,
    ) -> Option<HttpRequest> {
        None
    }

    fn poll_policy(&self) -> PollPolicy {
        PollPolicy::default_bfl()
    }

    fn download_request(&self, url: &url::Url) -> HttpRequest {
        HttpRequest {
            method: Method::Get,
            url: url.to_string(),
            headers: Vec::new(),
            auth: None,
            body: Body::Empty,
            max_body_bytes: 100 * 1024 * 1024,
        }
    }
}

fn collapse_whitespace(text: &str) -> String {
    let mut collapsed = String::new();
    let mut pending_space = false;
    for character in text.chars() {
        if character.is_whitespace() {
            pending_space = true;
            continue;
        }
        if pending_space && !collapsed.is_empty() {
            collapsed.push(' ');
        }
        pending_space = false;
        collapsed.push(character);
    }
    collapsed
}
