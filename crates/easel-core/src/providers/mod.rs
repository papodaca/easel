pub mod apiframe;
pub mod bfl;
pub mod chutes;
pub mod google;
pub mod krea;
pub mod leonardo;
pub mod meta;
pub mod openai;
pub mod qwen;
pub mod stability;
pub mod xai;

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
                let mut specs = vec![
                    bfl::flux3::model_spec(),
                    bfl::flux2_pro::model_spec(),
                    krea::krea3::model_spec(),
                    krea::krea2_large::model_spec(),
                    google::NanoBanana21::model_spec(),
                    google::NanoBanana2Lite::model_spec(),
                    google::NanoBananaPro::model_spec(),
                    openai::GptImage25Flare::model_spec(),
                    openai::GptImage25Sunburst::model_spec(),
                    openai::GptImage2::model_spec(),
                    openai::GptImage15::model_spec(),
                    openai::GptImage1Mini::model_spec(),
                    openai::GptImage1::model_spec(),
                    xai::GrokImagine::model_spec(),
                    meta::MuseImage::model_spec(),
                    chutes::FluxSchnell::model_spec(),
                    chutes::DreamshaperXl::model_spec(),
                    chutes::ILustMixV80::model_spec(),
                    chutes::JuggernautXl::model_spec(),
                    chutes::QwenImageEdit2511::model_spec(),
                    chutes::QwenImage2512::model_spec(),
                    chutes::ZImageTurbo::model_spec(),
                    qwen::QwenImage30Pro::model_spec(),
                    qwen::QwenImage21Pro::model_spec(),
                    stability::StableImageUltra::model_spec(),
                    stability::StableImageCore::model_spec(),
                    stability::Sd35Large::model_spec(),
                    stability::Sd35LargeTurbo::model_spec(),
                    stability::Sd35Medium::model_spec(),
                    stability::Sd35Flash::model_spec(),
                    stability::UpscaleFast::model_spec(),
                    stability::UpscaleConservative::model_spec(),
                    stability::UpscaleCreative::model_spec(),
                    stability::Outpaint::model_spec(),
                    stability::Erase::model_spec(),
                    stability::Inpaint::model_spec(),
                    stability::SearchAndReplace::model_spec(),
                    stability::SearchAndRecolor::model_spec(),
                    stability::RemoveBackground::model_spec(),
                    stability::ReplaceBackgroundAndRelight::model_spec(),
                ];
                specs.extend(
                    apiframe::descriptors()
                        .iter()
                        .map(|descriptor| &descriptor.spec),
                );
                specs.extend(leonardo::specs());
                specs
            })
            .as_slice()
    }

    /// Providers in alphabetical order, as shown in the pickers.
    #[must_use]
    pub fn providers() -> &'static [ProviderId] {
        static PROVIDERS: OnceLock<Vec<ProviderId>> = OnceLock::new();
        PROVIDERS
            .get_or_init(|| {
                let mut providers = vec![
                    ProviderId::Bfl,
                    ProviderId::Krea,
                    ProviderId::Google,
                    ProviderId::OpenAi,
                    ProviderId::Xai,
                    ProviderId::Meta,
                    ProviderId::Chutes,
                    ProviderId::Qwen,
                    ProviderId::Apiframe,
                    ProviderId::Leonardo,
                    ProviderId::Stability,
                ];
                providers.sort_by(|a, b| {
                    a.display_name()
                        .to_lowercase()
                        .cmp(&b.display_name().to_lowercase())
                });
                providers
            })
            .as_slice()
    }

    #[must_use]
    pub fn auth_scheme(provider: ProviderId) -> crate::model_spec::AuthScheme {
        provider.auth_scheme()
    }
}

#[cfg(test)]
mod tests {
    use super::{ModelRegistry, ProviderId};

    #[test]
    fn providers_are_listed_alphabetically() {
        let names: Vec<&str> = ModelRegistry::providers()
            .iter()
            .map(|provider| provider.display_name())
            .collect();
        let mut sorted = names.clone();
        sorted.sort_by_key(|name| name.to_lowercase());
        assert_eq!(names, sorted);
    }

    #[test]
    fn every_provider_is_registered_once() {
        let all = ModelRegistry::providers();
        assert_eq!(all.len(), 11);
        for expected in [
            ProviderId::Bfl,
            ProviderId::Krea,
            ProviderId::Google,
            ProviderId::OpenAi,
            ProviderId::Xai,
            ProviderId::Meta,
            ProviderId::Chutes,
            ProviderId::Qwen,
            ProviderId::Apiframe,
            ProviderId::Leonardo,
            ProviderId::Stability,
        ] {
            assert_eq!(all.iter().filter(|p| **p == expected).count(), 1);
        }
    }
}

/// The provider that owns a registered model id.
#[must_use]
pub fn resolve_model(model_id: &str) -> Option<Box<dyn Provider>> {
    match model_id {
        "flux-3-image" => Some(Box::new(bfl::flux3::Flux3)),
        "flux-2-pro" => Some(Box::new(bfl::flux2_pro::Flux2Pro)),
        "krea-3" => Some(Box::new(krea::krea3::Krea3)),
        "krea-2-large" => Some(Box::new(krea::krea2_large::Krea2Large)),
        "gemini-nano-banana-2.1" => Some(Box::new(google::NanoBanana21)),
        "gemini-3.1-flash-lite-image" => Some(Box::new(google::NanoBanana2Lite)),
        "gemini-3-pro-image" => Some(Box::new(google::NanoBananaPro)),
        "gpt-image-2.5-flare" => Some(Box::new(openai::GptImage25Flare)),
        "gpt-image-2.5-sunburst" => Some(Box::new(openai::GptImage25Sunburst)),
        "gpt-image-2" => Some(Box::new(openai::GptImage2)),
        "gpt-image-1.5" => Some(Box::new(openai::GptImage15)),
        "gpt-image-1-mini" => Some(Box::new(openai::GptImage1Mini)),
        "gpt-image-1" => Some(Box::new(openai::GptImage1)),
        "grok-imagine-image-2.0" => Some(Box::new(xai::GrokImagine)),
        "muse-image-1.0" => Some(Box::new(meta::MuseImage)),
        "chutes-flux-schnell" => Some(Box::new(chutes::FluxSchnell)),
        "chutes-dreamshaper-xl" => Some(Box::new(chutes::DreamshaperXl)),
        "chutes-ilustmix-v80" => Some(Box::new(chutes::ILustMixV80)),
        "chutes-juggernaut-xl" => Some(Box::new(chutes::JuggernautXl)),
        "chutes-qwen-image-edit-2511" => Some(Box::new(chutes::QwenImageEdit2511)),
        "chutes-qwen-image-2512" => Some(Box::new(chutes::QwenImage2512)),
        "chutes-z-image-turbo" => Some(Box::new(chutes::ZImageTurbo)),
        "qwen-image-3.0-pro" => Some(Box::new(qwen::QwenImage30Pro)),
        "qwen-image-2.1-pro" => Some(Box::new(qwen::QwenImage21Pro)),
        "stability-ultra" => Some(Box::new(stability::StableImageUltra)),
        "stability-core" => Some(Box::new(stability::StableImageCore)),
        "sd3.5-large" => Some(Box::new(stability::Sd35Large)),
        "sd3.5-large-turbo" => Some(Box::new(stability::Sd35LargeTurbo)),
        "sd3.5-medium" => Some(Box::new(stability::Sd35Medium)),
        "sd3.5-flash" => Some(Box::new(stability::Sd35Flash)),
        "stability-upscale-fast" => Some(Box::new(stability::UpscaleFast)),
        "stability-upscale-conservative" => Some(Box::new(stability::UpscaleConservative)),
        "stability-upscale-creative" => Some(Box::new(stability::UpscaleCreative)),
        "stability-outpaint" => Some(Box::new(stability::Outpaint)),
        "stability-erase" => Some(Box::new(stability::Erase)),
        "stability-inpaint" => Some(Box::new(stability::Inpaint)),
        "stability-search-and-replace" => Some(Box::new(stability::SearchAndReplace)),
        "stability-search-and-recolor" => Some(Box::new(stability::SearchAndRecolor)),
        "stability-remove-background" => Some(Box::new(stability::RemoveBackground)),
        "stability-replace-background-and-relight" => {
            Some(Box::new(stability::ReplaceBackgroundAndRelight))
        }
        other => match apiframe::resolve(other) {
            Some(provider) => Some(Box::new(provider)),
            None => {
                leonardo::resolve(other).map(|provider| Box::new(provider) as Box<dyn Provider>)
            }
        },
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InlineImage {
    pub bytes: Vec<u8>,
    pub mime: String,
    pub interaction_id: String,
    pub suggestion: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Submitted {
    pub job_id: JobId,
    pub poll_url: url::Url,
    pub quote: Option<CostQuote>,
}

#[derive(Debug, Clone, PartialEq)]
#[allow(clippy::large_enum_variant)]
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
        /// Midjourney only: the four quadrant URLs beside the 2x2 composite.
        grid_urls: Option<[url::Url; 4]>,
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
        ImageSlots::None => {}
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

    /// The picture URL a 200 submit response carries, when it does.
    fn picture_url(&self, _response: &HttpResponse) -> Option<url::Url> {
        None
    }

    fn inline_image(&self, _response: &HttpResponse) -> Option<InlineImage> {
        None
    }

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

    /// The pixel cap upload preparation should apply for this model's input
    /// images; `u64::MAX` leaves the resolution tier's cap in charge.
    fn prep_pixel_cap(&self) -> u64 {
        u64::MAX
    }

    fn download_request(&self, url: &url::Url) -> HttpRequest {
        plain_download(url)
    }
}

/// The unauthenticated GET every provider uses to fetch picture bytes.
#[must_use]
pub fn plain_download(url: &url::Url) -> HttpRequest {
    HttpRequest {
        method: Method::Get,
        url: url.to_string(),
        headers: Vec::new(),
        auth: None,
        body: Body::Empty,
        max_body_bytes: 100 * 1024 * 1024,
        redact_query: false,
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
