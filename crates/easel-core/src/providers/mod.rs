pub mod bfl;

use crate::error::ProviderError;
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
use crate::model_spec::ModelSpec;
use crate::scene::Scene;

use self::bfl::flux3::CompiledPrompt;

#[derive(Clone)]
pub enum ImageInput {
    RawBase64(String),
    Url(String),
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
