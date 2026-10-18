use crate::model_spec::{AuthScheme, Capabilities, ImageSlots, ModelSpec, ProviderId, SizeControl};
use crate::schema::{FieldDef, FieldKind, ParamSchema};

/// GPT Image 2.5 Flare.
#[derive(Debug, Clone, Copy)]
pub struct GptImage25Flare;

/// GPT Image 2.5 Sunburst.
#[derive(Debug, Clone, Copy)]
pub struct GptImage25Sunburst;

/// GPT Image 2.
#[derive(Debug, Clone, Copy)]
pub struct GptImage2;

/// GPT Image 1.5. Shuts down on 2026-12-01.
#[derive(Debug, Clone, Copy)]
pub struct GptImage15;

/// GPT Image 1 mini. Shuts down on 2026-12-01.
#[derive(Debug, Clone, Copy)]
pub struct GptImage1Mini;

/// GPT Image 1. Shuts down on 2026-10-23.
#[derive(Debug, Clone, Copy)]
pub struct GptImage1;

const GENERATIONS: &str = "https://api.openai.com/v1/images/generations";

const SIZES: &[&str] = &["1024x1024", "1536x1024", "1024x1536"];

const QUALITY: &[&str] = &["low", "medium", "high", "auto"];

const QUALITY_25: &[&str] = &["low", "medium", "high", "xhigh", "max", "auto"];

static FLARE: std::sync::LazyLock<ModelSpec> = std::sync::LazyLock::new(|| {
    image_spec(
        "gpt-image-2.5-flare",
        "GPT Image 2.5 Flare",
        QUALITY_25,
        None,
    )
});

static SUNBURST: std::sync::LazyLock<ModelSpec> = std::sync::LazyLock::new(|| {
    image_spec(
        "gpt-image-2.5-sunburst",
        "GPT Image 2.5 Sunburst",
        QUALITY_25,
        None,
    )
});

static GPT_IMAGE_2: std::sync::LazyLock<ModelSpec> =
    std::sync::LazyLock::new(|| image_spec("gpt-image-2", "GPT Image 2", QUALITY, None));

static GPT_IMAGE_15: std::sync::LazyLock<ModelSpec> = std::sync::LazyLock::new(|| {
    image_spec(
        "gpt-image-1.5",
        "GPT Image 1.5 2026-12-01",
        QUALITY,
        Some(&["high", "low"]),
    )
});

static GPT_IMAGE_1_MINI: std::sync::LazyLock<ModelSpec> = std::sync::LazyLock::new(|| {
    image_spec(
        "gpt-image-1-mini",
        "GPT Image 1 mini 2026-12-01",
        QUALITY,
        Some(&["low"]),
    )
});

static GPT_IMAGE_1: std::sync::LazyLock<ModelSpec> = std::sync::LazyLock::new(|| {
    image_spec(
        "gpt-image-1",
        "GPT Image 1 2026-10-23",
        QUALITY,
        Some(&["high", "low"]),
    )
});

impl GptImage25Flare {
    #[must_use]
    pub fn model_spec() -> &'static ModelSpec {
        &FLARE
    }
}

impl GptImage25Sunburst {
    #[must_use]
    pub fn model_spec() -> &'static ModelSpec {
        &SUNBURST
    }
}

impl GptImage2 {
    #[must_use]
    pub fn model_spec() -> &'static ModelSpec {
        &GPT_IMAGE_2
    }
}

impl GptImage15 {
    #[must_use]
    pub fn model_spec() -> &'static ModelSpec {
        &GPT_IMAGE_15
    }
}

impl GptImage1Mini {
    #[must_use]
    pub fn model_spec() -> &'static ModelSpec {
        &GPT_IMAGE_1_MINI
    }
}

impl GptImage1 {
    #[must_use]
    pub fn model_spec() -> &'static ModelSpec {
        &GPT_IMAGE_1
    }
}

fn image_spec(
    id: &str,
    display_name: &str,
    quality: &[&str],
    fidelity: Option<&[&str]>,
) -> ModelSpec {
    let mut fields = vec![
        select("quality", "Quality", quality, "auto"),
        select("size", "Size", SIZES, "1024x1024"),
        select(
            "background",
            "Background",
            &["transparent", "opaque", "auto"],
            "auto",
        ),
    ];
    if let Some(options) = fidelity {
        fields.push(select("input_fidelity", "Input fidelity", options, "low"));
    }
    ModelSpec {
        id: id.into(),
        display_name: display_name.into(),
        provider: ProviderId::OpenAi,
        auth: AuthScheme::Bearer,
        endpoint: GENERATIONS.into(),
        beta: false,
        capabilities: Capabilities {
            supports_boxes: false,
            image_slots: ImageSlots::List {
                max: 16,
                key: "images".into(),
            },
            size_control: SizeControl::AspectRatioAndResolution,
        },
        schema: ParamSchema {
            fields,
            cross_rules: Vec::new(),
        },
    }
}

fn select(key: &str, label: &str, options: &[&str], default: &str) -> FieldDef {
    FieldDef {
        key: key.into(),
        label: label.into(),
        help: None,
        kind: FieldKind::Select {
            options: options.iter().map(|option| (*option).to_string()).collect(),
        },
        required: true,
        default: Some(serde_json::json!(default)),
        locked_when: None,
    }
}

fn validate_submit(
    spec: &ModelSpec,
    scene: &crate::scene::Scene,
    values: &std::collections::BTreeMap<String, serde_json::Value>,
    image_count: usize,
) -> Vec<crate::error::FieldError> {
    use crate::error::FieldError;
    use crate::schema::RuleContext;

    let ctx = RuleContext {
        has_source_image: scene.source_image.is_some(),
        image_count,
        box_count: scene.boxes.len(),
    };
    let mut errors = spec.schema.validate(values, &ctx);
    if scene.prose.trim().is_empty() {
        errors.push(FieldError {
            key: "prompt".into(),
            message: "Prompt is empty".into(),
            approved_max: None,
        });
    }
    errors
}

fn inline_steps(
    images: &[crate::job::EncodedImage],
    max: usize,
) -> Vec<crate::providers::ReferenceStep> {
    images
        .iter()
        .take(max)
        .map(|image| {
            crate::providers::ReferenceStep::Inline(crate::providers::ImageInput::RawBase64(
                image.base64(),
            ))
        })
        .collect()
}

fn placeholder_submit(spec: &ModelSpec) -> crate::http::HttpRequest {
    crate::http::HttpRequest {
        method: crate::http::Method::Post,
        url: spec.endpoint.clone(),
        headers: Vec::new(),
        auth: Some(ProviderId::OpenAi),
        body: crate::http::Body::Empty,
        max_body_bytes: 64 * 1024 * 1024,
    }
}

macro_rules! impl_openai {
    ($ty:ident, $spec:ident) => {
        impl crate::providers::Provider for $ty {
            fn spec(&self) -> &ModelSpec {
                &$spec
            }

            fn validate_for_submit(
                &self,
                scene: &crate::scene::Scene,
                values: &std::collections::BTreeMap<String, serde_json::Value>,
                image_count: usize,
            ) -> Vec<crate::error::FieldError> {
                validate_submit(&$spec, scene, values, image_count)
            }

            fn reference_steps(
                &self,
                images: &[crate::job::EncodedImage],
            ) -> Vec<crate::providers::ReferenceStep> {
                inline_steps(images, $spec.capabilities.image_slots.max())
            }

            fn build_submit(
                &self,
                _request: &crate::job::JobRequest,
                _images: &[crate::providers::ImageInput],
            ) -> crate::http::HttpRequest {
                placeholder_submit(&$spec)
            }

            fn poll_request(
                &self,
                submitted: &crate::providers::Submitted,
            ) -> crate::http::HttpRequest {
                crate::http::HttpRequest {
                    method: crate::http::Method::Get,
                    url: submitted.poll_url.to_string(),
                    headers: Vec::new(),
                    auth: Some(ProviderId::OpenAi),
                    body: crate::http::Body::Empty,
                    max_body_bytes: 1024,
                }
            }

            fn parse_submit(
                &self,
                _response: &crate::http::HttpResponse,
            ) -> Result<crate::providers::Submitted, crate::error::ProviderError> {
                Err(crate::error::ProviderError::Protocol(
                    "openai returns the image on the submit response".into(),
                ))
            }

            fn parse_poll(
                &self,
                _response: &crate::http::HttpResponse,
            ) -> crate::providers::PollOutcome {
                crate::providers::PollOutcome::Failed(crate::error::ProviderError::Protocol(
                    "openai returns the image on the submit response".into(),
                ))
            }
        }
    };
}

impl_openai!(GptImage25Flare, FLARE);
impl_openai!(GptImage25Sunburst, SUNBURST);
impl_openai!(GptImage2, GPT_IMAGE_2);
impl_openai!(GptImage15, GPT_IMAGE_15);
impl_openai!(GptImage1Mini, GPT_IMAGE_1_MINI);
impl_openai!(GptImage1, GPT_IMAGE_1);

#[cfg(test)]
mod tests {
    use super::{
        GptImage1, GptImage1Mini, GptImage2, GptImage15, GptImage25Flare, GptImage25Sunburst,
    };
    use crate::providers::ModelRegistry;
    use crate::schema::FieldKind;

    fn spec(id: &str) -> &'static crate::model_spec::ModelSpec {
        ModelRegistry::specs()
            .iter()
            .copied()
            .find(|spec| spec.id == id)
            .unwrap_or_else(|| panic!("missing {id}"))
    }

    fn options(id: &str, key: &str) -> Option<Vec<String>> {
        let field = spec(id)
            .schema
            .fields
            .iter()
            .find(|field| field.key == key)?;
        match &field.kind {
            FieldKind::Select { options } => Some(options.clone()),
            _ => None,
        }
    }

    #[test]
    fn quality_size_and_fidelity_follow_the_model() {
        let _structs = (
            GptImage25Flare,
            GptImage25Sunburst,
            GptImage2,
            GptImage15,
            GptImage1Mini,
            GptImage1,
        );
        assert!(
            options("gpt-image-2.5-flare", "quality")
                .unwrap()
                .contains(&"max".to_string())
        );
        assert_eq!(
            options("gpt-image-2", "quality").unwrap(),
            ["low", "medium", "high", "auto"]
        );
        assert!(options("gpt-image-2", "input_fidelity").is_none());
        assert_eq!(
            options("gpt-image-1-mini", "input_fidelity").unwrap(),
            ["low"]
        );
        assert_eq!(
            options("gpt-image-1.5", "input_fidelity").unwrap(),
            ["high", "low"]
        );
        for id in [
            "gpt-image-2.5-flare",
            "gpt-image-2.5-sunburst",
            "gpt-image-2",
            "gpt-image-1.5",
            "gpt-image-1-mini",
            "gpt-image-1",
        ] {
            let spec = spec(id);
            assert!(!spec.capabilities.supports_boxes, "{id}");
            assert_eq!(
                options(id, "size").unwrap(),
                ["1024x1024", "1536x1024", "1024x1536"]
            );
            assert_eq!(
                spec.schema
                    .fields
                    .iter()
                    .find(|field| field.key == "size")
                    .and_then(|field| field.default.as_ref()),
                Some(&serde_json::json!("1024x1024"))
            );
        }
    }
}
