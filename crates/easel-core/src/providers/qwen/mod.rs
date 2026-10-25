use std::sync::LazyLock;

use serde_json::{Map, Value, json};

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

/// Both dimensions set: the smallest and largest pixel count `parameters.size` may ask for.
const MIN_AREA: i64 = 262_144;
const MAX_AREA: i64 = 4_194_304;

/// The long side may be at most this many times the short side.
const RATIO_CAP: i64 = 8;

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

fn dimension(values: &std::collections::BTreeMap<String, Value>, key: &str) -> i64 {
    values
        .get(key)
        .and_then(Value::as_i64)
        .filter(|number| *number >= 0)
        .unwrap_or(0)
}

fn size_parameter(width: i64, height: i64) -> Option<String> {
    if width == 0 || height == 0 {
        return None;
    }
    let area = width.checked_mul(height)?;
    if !(MIN_AREA..=MAX_AREA).contains(&area) {
        return None;
    }
    let (long, short) = (width.max(height), width.min(height));
    if long > short.saturating_mul(RATIO_CAP) {
        return None;
    }
    Some(format!("{width}*{height}"))
}

fn validate_size(values: &std::collections::BTreeMap<String, Value>) -> Vec<FieldError> {
    let width = dimension(values, "width");
    let height = dimension(values, "height");
    match (width, height) {
        (0, 0) => Vec::new(),
        (0, _) => vec![pair_error("width")],
        (_, 0) => vec![pair_error("height")],
        (width, height) => {
            let area = width.checked_mul(height);
            if area.is_none_or(|area| !(MIN_AREA..=MAX_AREA).contains(&area)) {
                vec![FieldError {
                    key: "width".into(),
                    message: format!(
                        "Width times height must be from {MIN_AREA} through {MAX_AREA}."
                    ),
                    approved_max: None,
                }]
            } else if width.max(height) > width.min(height).saturating_mul(RATIO_CAP) {
                vec![FieldError {
                    key: "width".into(),
                    message: "The long side must be at most 8 times the short side.".into(),
                    approved_max: None,
                }]
            } else {
                Vec::new()
            }
        }
    }
}

fn pair_error(key: &str) -> FieldError {
    FieldError {
        key: key.into(),
        message: "Set both width and height, or leave both at 0.".into(),
        approved_max: None,
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
    errors.extend(validate_size(values));
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

fn data_url(media_type: &str, encoded: &str) -> String {
    format!("data:{media_type};base64,{encoded}")
}

fn image_entry(request: &JobRequest, images: &[ImageInput], index: usize) -> Value {
    let url = match &images[index] {
        ImageInput::Url(url) => url.clone(),
        ImageInput::RawBase64(encoded) => {
            let media_type = request
                .images
                .get(index)
                .map(|image| image.media_type.as_str())
                .unwrap_or("image/png");
            data_url(media_type, encoded)
        }
    };
    json!({ "image": url })
}

/// The whole Qwen protocol: one POST, the answer carries image URLs to download.
fn build_qwen(
    spec: &ModelSpec,
    request: &JobRequest,
    images: &[ImageInput],
    negative_prompt: bool,
) -> HttpRequest {
    let count = images.len().min(spec.capabilities.image_slots.max());
    let mut content = Vec::new();
    for index in 0..count {
        content.push(image_entry(request, images, index));
    }
    content.push(json!({ "text": request.compiled_prompt.clone() }));

    let width = dimension(&request.params, "width");
    let height = dimension(&request.params, "height");
    let mut parameters = Map::new();
    if let Some(size) = size_parameter(width, height) {
        parameters.insert("size".into(), Value::String(size));
    }
    if negative_prompt
        && let Some(text) = request
            .params
            .get("negative_prompt")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
    {
        parameters.insert("negative_prompt".into(), Value::String(text.to_owned()));
    }

    let mut body = Map::new();
    body.insert("model".into(), Value::String(request.model_id.clone()));
    body.insert(
        "input".into(),
        json!({
            "messages": [
                {
                    "role": "user",
                    "content": content,
                }
            ]
        }),
    );
    if !parameters.is_empty() {
        body.insert("parameters".into(), Value::Object(parameters));
    }
    HttpRequest {
        method: Method::Post,
        url: spec.endpoint.clone(),
        headers: vec![HttpHeader {
            name: "Content-Type".into(),
            value: "application/json".into(),
            sensitive: false,
        }],
        auth: Some(ProviderId::Qwen),
        body: Body::Json(Value::Object(body)),
        max_body_bytes: 100 * 1024 * 1024,
    }
}

macro_rules! impl_qwen {
    ($ty:ident, $spec:ident, $negative_prompt:expr) => {
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
                build_qwen(&$spec, request, images, $negative_prompt)
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

impl_qwen!(QwenImage30Pro, QWEN_IMAGE_30_PRO, true);
impl_qwen!(QwenImage21Pro, QWEN_IMAGE_21_PRO, false);

/// The over-cap line for the form. A capped count never reaches the builder.
#[must_use]
pub fn refusal_messages(spec: &ModelSpec, prepared: usize) -> Vec<String> {
    let max = spec.capabilities.image_slots.max();
    if prepared > max {
        vec![format!("At most {max} images.")]
    } else {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::{GENERATION_URL, QwenImage21Pro, QwenImage30Pro};
    use crate::error::ProviderError;
    use crate::http::{Body, HttpResponse, Method};
    use crate::job::{EncodedImage, JobRequest};
    use crate::model_spec::{ImageSlots, ProviderId, SizeControl};
    use crate::providers::{ModelRegistry, Provider};
    use crate::scene::Scene;
    use serde_json::{Value, json};
    use std::collections::BTreeMap;

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
        let images: Vec<EncodedImage> = (0..11)
            .map(|seed| EncodedImage {
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

    fn encoded(seed: u8) -> EncodedImage {
        EncodedImage {
            media_type: "image/png".into(),
            bytes: vec![seed; 8],
            width: 16,
            height: 16,
            content_hash: format!("h{seed}"),
        }
    }

    fn data_uri(image: &EncodedImage) -> String {
        format!("data:{};base64,{}", image.media_type, image.base64())
    }

    fn scene_with(prose: &str) -> Scene {
        let mut scene = Scene::empty();
        scene.prose = prose.into();
        scene
    }

    fn request(model: &str, params: Value, images: Vec<EncodedImage>) -> JobRequest {
        JobRequest {
            model_id: model.into(),
            compiled_prompt: "a lighthouse".into(),
            params: params.as_object().unwrap().clone().into_iter().collect(),
            images,
            box_snapshot: Vec::new(),
            created_at: "2026-10-24T00:00:00Z".into(),
        }
    }

    fn posted(model: &str, params: Value, images: Vec<EncodedImage>) -> (String, Value) {
        let provider = crate::providers::resolve_model(model).unwrap();
        let steps = provider.reference_steps(&images);
        let inline = steps
            .iter()
            .map(|step| match step {
                crate::providers::ReferenceStep::Inline(input) => input.clone(),
                crate::providers::ReferenceStep::Upload { .. } => panic!("expected inline"),
            })
            .collect::<Vec<_>>();
        let http = provider.build_submit(&request(model, params, images), &inline);
        assert_eq!(http.method, Method::Post);
        assert_eq!(http.url, GENERATION_URL);
        assert_eq!(http.auth, Some(ProviderId::Qwen));
        assert!(
            http.headers
                .iter()
                .any(|header| header.name == "Content-Type" && header.value == "application/json")
        );
        assert!(
            !http
                .headers
                .iter()
                .any(|header| header.name == "X-DashScope-Async")
        );
        let Body::Json(json) = http.body else {
            panic!("expected json");
        };
        (http.url, json)
    }

    fn content_of(model: &str, params: Value, images: Vec<EncodedImage>) -> Vec<Value> {
        let (_, body) = posted(model, params, images);
        body["input"]["messages"][0]["content"]
            .as_array()
            .expect("content")
            .clone()
    }

    #[test]
    fn zero_images_post_one_text_object_and_no_optional_fields() {
        let (_, body) = posted("qwen-image-3.0-pro", json!({}), Vec::new());
        assert_eq!(body["model"], "qwen-image-3.0-pro");
        assert_eq!(body["input"]["messages"][0]["role"], "user");
        let content = body["input"]["messages"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 1);
        assert_eq!(content[0]["text"], "a lighthouse");
        assert!(content[0].get("image").is_none());
        for key in [
            "n",
            "prompt_extend",
            "enable_thinking",
            "watermark",
            "seed",
            "prompt_extend_mode",
            "parameters",
        ] {
            assert!(body.get(key).is_none(), "{key}");
        }
    }

    #[test]
    fn width_and_height_send_size_on_generation_and_edit() {
        let params = json!({"width": 1024, "height": 1536});
        let (_, generated) = posted("qwen-image-3.0-pro", params.clone(), Vec::new());
        assert_eq!(generated["parameters"]["size"], "1024*1536");
        let (_, edited) = posted("qwen-image-2.1-pro", params, vec![encoded(1)]);
        assert_eq!(edited["parameters"]["size"], "1024*1536");
    }

    #[test]
    fn zero_dimensions_omit_size_and_one_dimension_is_a_field_error() {
        let (_, body) = posted(
            "qwen-image-3.0-pro",
            json!({"width": 0, "height": 0}),
            Vec::new(),
        );
        assert!(body.get("parameters").is_none(), "{}", body);
        let provider = crate::providers::resolve_model("qwen-image-3.0-pro").unwrap();
        let errors = provider.validate_for_submit(
            &scene_with("go"),
            &BTreeMap::from([("width".into(), json!(1024)), ("height".into(), json!(0))]),
            0,
        );
        assert!(
            errors
                .iter()
                .any(|error| error.message == "Set both width and height, or leave both at 0."),
            "{errors:?}"
        );
    }

    #[test]
    fn the_area_and_ratio_bounds_hold() {
        let provider = crate::providers::resolve_model("qwen-image-3.0-pro").unwrap();
        let sized = |width: i64, height: i64| {
            BTreeMap::from([
                ("width".into(), json!(width)),
                ("height".into(), json!(height)),
            ])
        };
        let accepted = provider.validate_for_submit(&scene_with("go"), &sized(2048, 256), 0);
        assert!(
            !accepted
                .iter()
                .any(|error| error.message.contains("262144") || error.message.contains("8 times")),
            "{accepted:?}"
        );
        let (_, body) = posted(
            "qwen-image-3.0-pro",
            json!({"width": 2048, "height": 256}),
            Vec::new(),
        );
        assert_eq!(body["parameters"]["size"], "2048*256");

        for (width, height, message) in [
            (
                4096,
                4096,
                "Width times height must be from 262144 through 4194304.",
            ),
            (
                2048,
                200,
                "The long side must be at most 8 times the short side.",
            ),
        ] {
            let errors = provider.validate_for_submit(&scene_with("go"), &sized(width, height), 0);
            assert!(
                errors.iter().any(|error| error.message == message),
                "{width}x{height}: {errors:?}"
            );
            let (_, body) = posted(
                "qwen-image-3.0-pro",
                json!({"width": width, "height": height}),
                Vec::new(),
            );
            assert!(
                body["parameters"].get("size").is_none(),
                "{width}x{height}: {}",
                body["parameters"]
            );
        }
    }

    #[test]
    fn one_image_sends_the_data_uri_then_the_text() {
        let image = encoded(7);
        let content = content_of("qwen-image-3.0-pro", json!({}), vec![image.clone()]);
        assert_eq!(content.len(), 2);
        assert_eq!(content[0]["image"], data_uri(&image));
        assert_eq!(content[1]["text"], "a lighthouse");
    }

    #[test]
    fn three_images_on_the_wide_model_send_canvas_first() {
        let canvas = encoded(1);
        let second = encoded(2);
        let third = encoded(3);
        let content = content_of(
            "qwen-image-2.1-pro",
            json!({}),
            vec![canvas.clone(), second.clone(), third.clone()],
        );
        assert_eq!(content.len(), 4);
        assert_eq!(content[0]["image"], data_uri(&canvas));
        assert_eq!(content[1]["image"], data_uri(&second));
        assert_eq!(content[2]["image"], data_uri(&third));
        assert_eq!(content[3]["text"], "a lighthouse");
    }

    #[test]
    fn negative_prompt_follows_the_model() {
        let (_, pro) = posted(
            "qwen-image-3.0-pro",
            json!({"negative_prompt": "blurry"}),
            Vec::new(),
        );
        assert_eq!(pro["parameters"]["negative_prompt"], "blurry");
        let (_, wide) = posted(
            "qwen-image-2.1-pro",
            json!({"negative_prompt": "blurry"}),
            Vec::new(),
        );
        assert!(
            wide["parameters"].get("negative_prompt").is_none(),
            "{}",
            wide["parameters"]
        );
        for model in IDS {
            let (_, body) = posted(model, json!({"negative_prompt": ""}), Vec::new());
            assert!(
                body["parameters"].get("negative_prompt").is_none(),
                "{model}: {}",
                body["parameters"]
            );
        }
    }

    #[test]
    fn four_prepared_images_are_refused_on_the_smaller_cap() {
        assert_eq!(
            super::refusal_messages(spec_of("qwen-image-3.0-pro"), 4),
            vec!["At most 3 images.".to_string()]
        );
        assert!(super::refusal_messages(spec_of("qwen-image-3.0-pro"), 3).is_empty());
        assert_eq!(
            super::refusal_messages(spec_of("qwen-image-2.1-pro"), 11),
            vec!["At most 10 images.".to_string()]
        );
        assert!(super::refusal_messages(spec_of("qwen-image-2.1-pro"), 10).is_empty());

        let images: Vec<_> = (0..4).map(encoded).collect();
        let content = content_of(
            "qwen-image-3.0-pro",
            json!({}),
            images.into_iter().take(3).collect(),
        );
        let images_in_body = content
            .iter()
            .filter(|entry| entry.get("image").is_some())
            .count();
        assert_eq!(images_in_body, 3);
    }

    #[test]
    fn an_empty_prompt_is_reported_on_every_spec() {
        for id in IDS {
            let errors = crate::providers::resolve_model(id)
                .unwrap()
                .validate_for_submit(&scene_with("  "), &BTreeMap::new(), 0);
            assert!(
                errors
                    .iter()
                    .any(|error| error.key == "prompt" && error.message == "Prompt is empty"),
                "{id}"
            );
        }
    }

    #[test]
    fn parse_submit_never_polls() {
        let provider = crate::providers::resolve_model("qwen-image-3.0-pro").unwrap();
        let response = HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: b"{}".to_vec(),
        };
        assert!(matches!(
            provider.parse_submit(&response),
            Err(ProviderError::Protocol(_))
        ));
    }
}
