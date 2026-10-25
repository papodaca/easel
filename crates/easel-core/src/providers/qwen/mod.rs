use std::sync::LazyLock;

use serde_json::{Value, json};

use crate::error::{FieldError, ProviderError};
use crate::http::{Body, HttpHeader, HttpRequest, HttpResponse, Method};
use crate::job::{EncodedImage, JobRequest};
use crate::model_spec::{AuthScheme, Capabilities, ImageSlots, ModelSpec, ProviderId, SizeControl};
use crate::providers::{ImageInput, PollOutcome, Provider, ReferenceStep, Submitted};
use crate::scene::Scene;
use crate::schema::ParamSchema;

/// `qwen-image-3.0-pro`.
#[derive(Debug, Clone, Copy)]
pub struct QwenImage30Pro;

/// `qwen-image-2.1-pro`.
#[derive(Debug, Clone, Copy)]
pub struct QwenImage21Pro;

pub const GENERATION_URL: &str =
    "https://maas.qwencloudapi.com/api/v1/services/aigc/multimodal-generation/generation";

static QWEN_IMAGE_30_PRO: LazyLock<ModelSpec> =
    LazyLock::new(|| qwen_spec("qwen-image-3.0-pro", "Qwen Image 3.0 Pro", 3));

static QWEN_IMAGE_21_PRO: LazyLock<ModelSpec> =
    LazyLock::new(|| qwen_spec("qwen-image-2.1-pro", "Qwen Image 2.1 Pro", 10));

impl QwenImage30Pro {
    #[must_use]
    pub fn model_spec() -> &'static ModelSpec {
        &QWEN_IMAGE_30_PRO
    }
}

impl QwenImage21Pro {
    #[must_use]
    pub fn model_spec() -> &'static ModelSpec {
        &QWEN_IMAGE_21_PRO
    }
}

fn qwen_spec(id: &str, display_name: &str, max_images: usize) -> ModelSpec {
    ModelSpec {
        id: id.into(),
        display_name: display_name.into(),
        provider: ProviderId::Qwen,
        auth: AuthScheme::Bearer,
        endpoint: GENERATION_URL.into(),
        beta: false,
        capabilities: Capabilities {
            supports_boxes: false,
            image_slots: ImageSlots::List {
                max: max_images,
                key: "content".into(),
            },
            size_control: SizeControl::WidthHeight,
        },
        schema: ParamSchema {
            fields: Vec::new(),
            cross_rules: Vec::new(),
        },
    }
}

fn validate_qwen(
    spec: &ModelSpec,
    scene: &Scene,
    values: &std::collections::BTreeMap<String, Value>,
    image_count: usize,
) -> Vec<FieldError> {
    let ctx = crate::schema::RuleContext {
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

fn refused_poll(submitted: &Submitted) -> HttpRequest {
    HttpRequest {
        method: Method::Get,
        url: submitted.poll_url.to_string(),
        headers: Vec::new(),
        auth: Some(ProviderId::Qwen),
        body: Body::Empty,
        max_body_bytes: 1024,
    }
}

/// The whole Qwen protocol: one POST, the answer carries image URLs to download.
fn build_qwen(spec: &ModelSpec, request: &JobRequest, _images: &[ImageInput]) -> HttpRequest {
    let content = vec![json!({ "text": request.compiled_prompt.clone() })];
    let body = json!({
        "model": request.model_id.clone(),
        "input": {
            "messages": [
                {
                    "role": "user",
                    "content": content,
                }
            ]
        },
    });
    HttpRequest {
        method: Method::Post,
        url: spec.endpoint.clone(),
        headers: vec![HttpHeader {
            name: "Content-Type".into(),
            value: "application/json".into(),
            sensitive: false,
        }],
        auth: Some(ProviderId::Qwen),
        body: Body::Json(body),
        max_body_bytes: 100 * 1024 * 1024,
    }
}

macro_rules! impl_qwen {
    ($ty:ident, $spec:ident) => {
        impl Provider for $ty {
            fn spec(&self) -> &ModelSpec {
                &$spec
            }

            fn validate_for_submit(
                &self,
                scene: &Scene,
                values: &std::collections::BTreeMap<String, Value>,
                image_count: usize,
            ) -> Vec<FieldError> {
                validate_qwen(&$spec, scene, values, image_count)
            }

            fn reference_steps(&self, images: &[EncodedImage]) -> Vec<ReferenceStep> {
                images
                    .iter()
                    .take($spec.capabilities.image_slots.max())
                    .map(|image| ReferenceStep::Inline(ImageInput::RawBase64(image.base64())))
                    .collect()
            }

            fn build_submit(&self, request: &JobRequest, images: &[ImageInput]) -> HttpRequest {
                build_qwen(&$spec, request, images)
            }

            fn poll_request(&self, submitted: &Submitted) -> HttpRequest {
                refused_poll(submitted)
            }

            fn parse_submit(&self, _response: &HttpResponse) -> Result<Submitted, ProviderError> {
                Err(ProviderError::Protocol(
                    "qwen returns an image url on the submit response".into(),
                ))
            }

            fn parse_poll(&self, _response: &HttpResponse) -> PollOutcome {
                PollOutcome::Failed(ProviderError::Protocol(
                    "qwen returns an image url on the submit response".into(),
                ))
            }
        }
    };
}

impl_qwen!(QwenImage30Pro, QWEN_IMAGE_30_PRO);
impl_qwen!(QwenImage21Pro, QWEN_IMAGE_21_PRO);

#[cfg(test)]
mod tests {
    use super::{GENERATION_URL, QwenImage21Pro, QwenImage30Pro};
    use crate::model_spec::{ImageSlots, ProviderId, SizeControl};
    use crate::providers::{ModelRegistry, Provider};

    const IDS: [&str; 2] = ["qwen-image-3.0-pro", "qwen-image-2.1-pro"];

    const NAMES: [&str; 2] = ["Qwen Image 3.0 Pro", "Qwen Image 2.1 Pro"];

    fn spec_of(id: &str) -> &'static crate::model_spec::ModelSpec {
        ModelRegistry::specs()
            .iter()
            .copied()
            .find(|spec| spec.id == id)
            .unwrap_or_else(|| panic!("missing {id}"))
    }

    #[test]
    fn registry_lists_the_two_qwen_models_in_menu_order() {
        let ids: Vec<_> = ModelRegistry::specs()
            .iter()
            .filter(|spec| spec.provider == ProviderId::Qwen)
            .map(|spec| spec.id.as_str())
            .collect();
        assert_eq!(ids, IDS);
        let names: Vec<_> = ModelRegistry::specs()
            .iter()
            .filter(|spec| spec.provider == ProviderId::Qwen)
            .map(|spec| spec.display_name.as_str())
            .collect();
        assert_eq!(names, NAMES);
        for (index, id) in IDS.iter().enumerate() {
            let spec = spec_of(id);
            assert_eq!(spec.provider, ProviderId::Qwen, "{id}");
            assert_eq!(spec.endpoint, GENERATION_URL, "{id}");
            assert!(!spec.beta, "{id}");
            assert!(!spec.capabilities.supports_boxes, "{id}");
            assert_eq!(spec.capabilities.size_control, SizeControl::WidthHeight);
            assert!(matches!(spec.auth, crate::model_spec::AuthScheme::Bearer));
            let max = match &spec.capabilities.image_slots {
                ImageSlots::List { max, .. } => *max,
                other => panic!("{other:?}"),
            };
            assert_eq!(max, [3, 10][index], "{id}");
        }
        assert!(ModelRegistry::providers().contains(&ProviderId::Qwen));
    }

    #[test]
    fn both_qwen_ids_resolve_to_the_qwen_provider() {
        for id in IDS {
            let provider =
                crate::providers::resolve_model(id).unwrap_or_else(|| panic!("missing {id}"));
            assert_eq!(provider.spec().id, id, "{id}");
            assert_eq!(provider.spec().provider, ProviderId::Qwen, "{id}");
        }
    }

    #[test]
    fn flux_3_still_resolves_to_black_forest_labs() {
        let provider = crate::providers::resolve_model("flux-3-image").unwrap();
        assert_eq!(provider.spec().id, "flux-3-image");
        assert_eq!(provider.spec().provider, ProviderId::Bfl);
        assert_eq!(provider.spec().provider.display_name(), "Black Forest Labs");
    }

    #[test]
    fn the_slots_cap_the_prepared_images() {
        let images: Vec<crate::job::EncodedImage> = (0..11)
            .map(|seed| crate::job::EncodedImage {
                media_type: "image/png".into(),
                bytes: vec![seed; 8],
                width: 16,
                height: 16,
                content_hash: format!("h{seed}"),
            })
            .collect();
        let pro = QwenImage30Pro.reference_steps(&images);
        assert_eq!(pro.len(), 3);
        let wide = QwenImage21Pro.reference_steps(&images);
        assert_eq!(wide.len(), 10);
        assert!(QwenImage30Pro.reference_steps(&[]).is_empty());
    }
}
