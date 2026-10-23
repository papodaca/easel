use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde_json::{Map, Value, json};

use crate::error::{FieldError, ProviderError};
use crate::http::{Body, HttpHeader, HttpRequest, HttpResponse, Method};
use crate::job::{EncodedImage, JobRequest};
use crate::model_spec::{AuthScheme, Capabilities, ImageSlots, ModelSpec, ProviderId, SizeControl};
use crate::providers::bfl::protocol::extract_result_mime;
use crate::providers::{ImageInput, InlineImage, ReferenceStep, Submitted};
use crate::scene::Scene;
use crate::schema::{FieldDef, FieldKind, ParamSchema, RuleContext};

#[derive(Debug, Clone, Copy)]
pub struct FluxSchnell;

#[derive(Debug, Clone, Copy)]
pub struct DreamshaperXl;

/// iLustMix v80 on the image-classic chute.
#[derive(Debug, Clone, Copy)]
pub struct ILustMixV80;

#[derive(Debug, Clone, Copy)]
pub struct JuggernautXl;

#[derive(Debug, Clone, Copy)]
pub struct QwenImageEdit2511;

#[derive(Debug, Clone, Copy)]
pub struct QwenImage2512;

#[derive(Debug, Clone, Copy)]
pub struct ZImageTurbo;

const FLUX_ENDPOINT: &str = "https://vonkaiser-imageclassic.chutes.ai/image/flux";
const DREAMSHAPER_ENDPOINT: &str = "https://vonkaiser-imageclassic.chutes.ai/image/dreamshaper";
const ILUSTMIX_ENDPOINT: &str = "https://vonkaiser-imageclassic.chutes.ai/image/ilustmix";
const JUGGERNAUT_ENDPOINT: &str = "https://vonkaiser-imageclassic.chutes.ai/image/juggernaut";
const QWEN_EDIT_ENDPOINT: &str = "https://vonkaiser-qwen-image-edit-2511.chutes.ai/generate";
const QWEN_ENDPOINT: &str = "https://vonkaiser-qwen-image-2512.chutes.ai/generate";
const Z_IMAGE_ENDPOINT: &str = "https://vonkaiser-z-image-turbo.chutes.ai/generate";

/// The flat keys a Chutes body may carry beyond `prompt`, in a fixed order.
const PARAM_KEYS: &[&str] = &[
    "seed",
    "width",
    "height",
    "guidance_scale",
    "true_cfg_scale",
    "shift",
    "negative_prompt",
    "num_inference_steps",
    "max_sequence_length",
];

static FLUX_SCHNELL: LazyLock<ModelSpec> = LazyLock::new(|| {
    chute_spec(
        "chutes-flux-schnell",
        "FLUX.1 schnell",
        FLUX_ENDPOINT,
        ImageSlots::None,
        128,
        vec![
            float_field("guidance_scale", "Guidance", 1.0, 20.0, 3.5),
            int_field("num_inference_steps", "Steps", 1, 50, Some(4)),
        ],
    )
});

static DREAMSHAPER_XL: LazyLock<ModelSpec> = LazyLock::new(|| {
    chute_spec(
        "chutes-dreamshaper-xl",
        "Dreamshaper XL",
        DREAMSHAPER_ENDPOINT,
        ImageSlots::None,
        128,
        sdxl_fields(),
    )
});

static ILUSTMIX_V80: LazyLock<ModelSpec> = LazyLock::new(|| {
    chute_spec(
        "chutes-ilustmix-v80",
        "iLustMix v80",
        ILUSTMIX_ENDPOINT,
        ImageSlots::None,
        128,
        sdxl_fields(),
    )
});

static JUGGERNAUT_XL: LazyLock<ModelSpec> = LazyLock::new(|| {
    chute_spec(
        "chutes-juggernaut-xl",
        "Juggernaut XL",
        JUGGERNAUT_ENDPOINT,
        ImageSlots::None,
        128,
        sdxl_fields(),
    )
});

static QWEN_IMAGE_EDIT_2511: LazyLock<ModelSpec> = LazyLock::new(|| {
    chute_spec(
        "chutes-qwen-image-edit-2511",
        "Qwen Image Edit 2511",
        QWEN_EDIT_ENDPOINT,
        ImageSlots::List {
            max: 3,
            key: "image_b64s".into(),
        },
        128,
        vec![
            float_field("true_cfg_scale", "True CFG", 0.0, 10.0, 1.0),
            negative_prompt(),
            int_field("num_inference_steps", "Steps", 1, 100, Some(4)),
        ],
    )
});

static QWEN_IMAGE_2512: LazyLock<ModelSpec> = LazyLock::new(|| {
    chute_spec(
        "chutes-qwen-image-2512",
        "Qwen Image 2512",
        QWEN_ENDPOINT,
        ImageSlots::None,
        128,
        vec![
            float_field("true_cfg_scale", "True CFG", 0.0, 10.0, 4.0),
            negative_prompt(),
            int_field("num_inference_steps", "Steps", 5, 75, Some(30)),
        ],
    )
});

static Z_IMAGE_TURBO: LazyLock<ModelSpec> = LazyLock::new(|| {
    chute_spec(
        "chutes-z-image-turbo",
        "Z-Image Turbo",
        Z_IMAGE_ENDPOINT,
        ImageSlots::None,
        576,
        vec![
            float_field("shift", "Shift", 1.0, 10.0, 3.0),
            float_field("guidance_scale", "Guidance", 0.0, 5.0, 0.0),
            int_field(
                "max_sequence_length",
                "Max sequence length",
                256,
                2048,
                Some(512),
            ),
            int_field("num_inference_steps", "Steps", 1, 100, Some(9)),
        ],
    )
});

impl FluxSchnell {
    #[must_use]
    pub fn model_spec() -> &'static ModelSpec {
        &FLUX_SCHNELL
    }
}

impl DreamshaperXl {
    #[must_use]
    pub fn model_spec() -> &'static ModelSpec {
        &DREAMSHAPER_XL
    }
}

impl ILustMixV80 {
    #[must_use]
    pub fn model_spec() -> &'static ModelSpec {
        &ILUSTMIX_V80
    }
}

impl JuggernautXl {
    #[must_use]
    pub fn model_spec() -> &'static ModelSpec {
        &JUGGERNAUT_XL
    }
}

impl QwenImageEdit2511 {
    #[must_use]
    pub fn model_spec() -> &'static ModelSpec {
        &QWEN_IMAGE_EDIT_2511
    }
}

impl QwenImage2512 {
    #[must_use]
    pub fn model_spec() -> &'static ModelSpec {
        &QWEN_IMAGE_2512
    }
}

impl ZImageTurbo {
    #[must_use]
    pub fn model_spec() -> &'static ModelSpec {
        &Z_IMAGE_TURBO
    }
}

fn chute_spec(
    id: &str,
    display_name: &str,
    endpoint: &str,
    slots: ImageSlots,
    min_size: i64,
    fields: Vec<FieldDef>,
) -> ModelSpec {
    let mut schema_fields = vec![seed_field()];
    schema_fields.extend(size_fields(min_size));
    schema_fields.extend(fields);
    ModelSpec {
        id: id.into(),
        display_name: display_name.into(),
        provider: ProviderId::Chutes,
        auth: AuthScheme::Bearer,
        endpoint: endpoint.into(),
        beta: false,
        capabilities: Capabilities {
            supports_boxes: false,
            image_slots: slots,
            size_control: SizeControl::WidthHeight,
        },
        schema: ParamSchema {
            fields: schema_fields,
            cross_rules: Vec::new(),
        },
    }
}

fn seed_field() -> FieldDef {
    int_field("seed", "Seed", 0, 4_294_967_295, None)
}

fn size_fields(min: i64) -> Vec<FieldDef> {
    vec![
        int_field("width", "Width", min, 2048, Some(1024)),
        int_field("height", "Height", min, 2048, Some(1024)),
    ]
}

fn int_field(key: &str, label: &str, min: i64, max: i64, default: Option<i64>) -> FieldDef {
    FieldDef {
        key: key.into(),
        label: label.into(),
        help: None,
        kind: FieldKind::Int { min, max, step: 1 },
        required: false,
        default: default.map(|value| json!(value)),
        locked_when: None,
    }
}

fn float_field(key: &str, label: &str, min: f64, max: f64, default: f64) -> FieldDef {
    FieldDef {
        key: key.into(),
        label: label.into(),
        help: None,
        kind: FieldKind::Float {
            min,
            max,
            step: 0.5,
        },
        required: false,
        default: Some(json!(default)),
        locked_when: None,
    }
}

fn negative_prompt() -> FieldDef {
    FieldDef {
        key: "negative_prompt".into(),
        label: "Negative prompt".into(),
        help: None,
        kind: FieldKind::Text { multiline: false },
        required: false,
        default: None,
        locked_when: None,
    }
}

fn sdxl_fields() -> Vec<FieldDef> {
    vec![
        float_field("guidance_scale", "Guidance", 1.0, 20.0, 7.5),
        negative_prompt(),
        int_field("num_inference_steps", "Steps", 1, 50, Some(25)),
    ]
}

fn validate_chutes(
    spec: &ModelSpec,
    scene: &Scene,
    values: &BTreeMap<String, Value>,
    image_count: usize,
    requires_images: bool,
) -> Vec<FieldError> {
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
    if requires_images && image_count == 0 {
        errors.push(FieldError {
            key: "images".into(),
            message: "Add at least one image.".into(),
            approved_max: None,
        });
    }
    errors
}

fn inline_reference_steps(images: &[EncodedImage], max: usize) -> Vec<ReferenceStep> {
    images
        .iter()
        .take(max)
        .map(|image| ReferenceStep::Inline(ImageInput::RawBase64(image.base64())))
        .collect()
}

fn refused_poll(submitted: &Submitted) -> HttpRequest {
    HttpRequest {
        method: Method::Get,
        url: submitted.poll_url.to_string(),
        headers: Vec::new(),
        auth: Some(ProviderId::Chutes),
        body: Body::Empty,
        max_body_bytes: 1024,
    }
}

fn build_chutes(spec: &ModelSpec, request: &JobRequest, images: &[ImageInput]) -> HttpRequest {
    let mut body = Map::new();
    body.insert(
        "prompt".into(),
        Value::String(request.compiled_prompt.clone()),
    );
    for key in PARAM_KEYS {
        let Some(value) = request.params.get(*key) else {
            continue;
        };
        if value.is_null() {
            continue;
        }
        if value.as_str().is_some_and(str::is_empty) {
            continue;
        }
        body.insert((*key).to_string(), value.clone());
    }
    for (key, value) in crate::providers::map_images(&spec.capabilities.image_slots, images) {
        body.insert(key, value);
    }
    HttpRequest {
        method: Method::Post,
        url: spec.endpoint.clone(),
        headers: vec![HttpHeader {
            name: "Content-Type".into(),
            value: "application/json".into(),
            sensitive: false,
        }],
        auth: Some(ProviderId::Chutes),
        body: Body::Json(Value::Object(body)),
        max_body_bytes: 100 * 1024 * 1024,
    }
}

/// The whole Chutes protocol: one POST, the answer is the image itself.
fn classify(response: &HttpResponse) -> Result<InlineImage, ProviderError> {
    match response.status {
        401 => return Err(ProviderError::Auth),
        402 => return Err(ProviderError::OutOfCredits { hint: None }),
        404 => return Err(ProviderError::ModelNotFound),
        429 => return Err(ProviderError::RateLimited { retry_after: None }),
        200 => {}
        status => {
            return Err(ProviderError::Http {
                status,
                body: String::from_utf8_lossy(&response.body).into_owned(),
            });
        }
    }
    if response.body.is_empty() {
        return Err(ProviderError::NoImage);
    }
    let media = response
        .headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("content-type"))
        .map(|header| header.value.as_str())
        .and_then(|header| {
            let media = header
                .split(';')
                .next()
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase();
            match media.as_str() {
                "image/jpeg" | "image/jpg" => Some("image/jpeg"),
                "image/png" => Some("image/png"),
                _ => None,
            }
        });
    let Some(media) = media else {
        return Err(ProviderError::NoImage);
    };
    let sniffed = extract_result_mime(None, &response.body);
    if sniffed != Some(media) {
        return Err(ProviderError::NoImage);
    }
    Ok(InlineImage {
        bytes: response.body.clone(),
        mime: media.to_string(),
        interaction_id: String::new(),
        suggestion: String::new(),
    })
}

macro_rules! impl_chutes {
    ($ty:ident, $spec:ident) => {
        impl_chutes!($ty, $spec, false);
    };
    ($ty:ident, $spec:ident, $requires_images:expr) => {
        impl crate::providers::Provider for $ty {
            fn spec(&self) -> &ModelSpec {
                &$spec
            }

            fn validate_for_submit(
                &self,
                scene: &Scene,
                values: &std::collections::BTreeMap<String, serde_json::Value>,
                image_count: usize,
            ) -> Vec<crate::error::FieldError> {
                validate_chutes(&$spec, scene, values, image_count, $requires_images)
            }

            fn reference_steps(
                &self,
                images: &[crate::job::EncodedImage],
            ) -> Vec<crate::providers::ReferenceStep> {
                inline_reference_steps(images, $spec.capabilities.image_slots.max())
            }

            fn build_submit(
                &self,
                request: &crate::job::JobRequest,
                images: &[crate::providers::ImageInput],
            ) -> crate::http::HttpRequest {
                build_chutes(&$spec, request, images)
            }

            fn poll_request(
                &self,
                submitted: &crate::providers::Submitted,
            ) -> crate::http::HttpRequest {
                refused_poll(submitted)
            }

            fn parse_submit(
                &self,
                response: &crate::http::HttpResponse,
            ) -> Result<crate::providers::Submitted, crate::error::ProviderError> {
                match classify(response) {
                    Ok(_) => Err(ProviderError::Protocol(
                        "chutes returns the image on the submit response".into(),
                    )),
                    Err(error) => Err(error),
                }
            }

            fn inline_image(
                &self,
                response: &crate::http::HttpResponse,
            ) -> Option<crate::providers::InlineImage> {
                classify(response).ok()
            }

            fn parse_poll(
                &self,
                _response: &crate::http::HttpResponse,
            ) -> crate::providers::PollOutcome {
                crate::providers::PollOutcome::Failed(ProviderError::Protocol(
                    "chutes returns the image on the submit response".into(),
                ))
            }
        }
    };
}

impl_chutes!(FluxSchnell, FLUX_SCHNELL);
impl_chutes!(DreamshaperXl, DREAMSHAPER_XL);
impl_chutes!(ILustMixV80, ILUSTMIX_V80);
impl_chutes!(JuggernautXl, JUGGERNAUT_XL);
impl_chutes!(QwenImageEdit2511, QWEN_IMAGE_EDIT_2511, true);
impl_chutes!(QwenImage2512, QWEN_IMAGE_2512);
impl_chutes!(ZImageTurbo, Z_IMAGE_TURBO);

#[cfg(test)]
mod tests {
    use super::FluxSchnell;
    use crate::error::ProviderError;
    use crate::http::{Body, HttpHeader, HttpResponse, Method};
    use crate::job::{EncodedImage, JobRequest};
    use crate::model_spec::{ImageSlots, ProviderId, SizeControl};
    use crate::providers::{ImageInput, ModelRegistry, PollOutcome, Provider, ReferenceStep};
    use crate::scene::Scene;
    use serde_json::{Value, json};
    use std::collections::BTreeMap;

    const IDS: [&str; 7] = [
        "chutes-flux-schnell",
        "chutes-dreamshaper-xl",
        "chutes-ilustmix-v80",
        "chutes-juggernaut-xl",
        "chutes-qwen-image-edit-2511",
        "chutes-qwen-image-2512",
        "chutes-z-image-turbo",
    ];

    const NAMES: [&str; 7] = [
        "FLUX.1 schnell",
        "Dreamshaper XL",
        "iLustMix v80",
        "Juggernaut XL",
        "Qwen Image Edit 2511",
        "Qwen Image 2512",
        "Z-Image Turbo",
    ];

    const ENDPOINTS: [&str; 7] = [
        "https://vonkaiser-imageclassic.chutes.ai/image/flux",
        "https://vonkaiser-imageclassic.chutes.ai/image/dreamshaper",
        "https://vonkaiser-imageclassic.chutes.ai/image/ilustmix",
        "https://vonkaiser-imageclassic.chutes.ai/image/juggernaut",
        "https://vonkaiser-qwen-image-edit-2511.chutes.ai/generate",
        "https://vonkaiser-qwen-image-2512.chutes.ai/generate",
        "https://vonkaiser-z-image-turbo.chutes.ai/generate",
    ];

    const JPEG_MAGIC: &[u8] = &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x4A, 0x46];
    const PNG_MAGIC: &[u8] = b"\x89PNG\r\n\x1a\n";

    fn provider(id: &str) -> Box<dyn Provider> {
        crate::providers::resolve_model(id).unwrap_or_else(|| panic!("missing {id}"))
    }

    fn spec_of(id: &str) -> &'static crate::model_spec::ModelSpec {
        ModelRegistry::specs()
            .iter()
            .copied()
            .find(|spec| spec.id == id)
            .unwrap_or_else(|| panic!("missing {id}"))
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

    fn inline_inputs(steps: &[ReferenceStep]) -> Vec<ImageInput> {
        steps
            .iter()
            .map(|step| match step {
                ReferenceStep::Inline(input) => input.clone(),
                ReferenceStep::Upload { .. } => panic!("expected inline"),
            })
            .collect()
    }

    fn request(model: &str, params: Value, images: Vec<EncodedImage>) -> JobRequest {
        JobRequest {
            model_id: model.into(),
            compiled_prompt: "a lighthouse".into(),
            params: params.as_object().unwrap().clone().into_iter().collect(),
            images,
            box_snapshot: Vec::new(),
            created_at: "2026-10-22T00:00:00Z".into(),
        }
    }

    fn defaults(model: &str) -> Value {
        let map: BTreeMap<String, Value> = spec_of(model).schema.defaults();
        Value::Object(map.into_iter().collect())
    }

    fn posted(model: &str, params: Value, images: Vec<EncodedImage>) -> (String, Value) {
        let provider = provider(model);
        let steps = provider.reference_steps(&images);
        let inputs = inline_inputs(&steps);
        let http = provider.build_submit(&request(model, params, images), &inputs);
        assert_eq!(http.method, Method::Post);
        assert_eq!(http.auth, Some(ProviderId::Chutes));
        assert!(
            http.headers
                .iter()
                .any(|header| header.name == "Content-Type" && header.value == "application/json")
        );
        let Body::Json(json) = http.body else {
            panic!("expected json");
        };
        (http.url, json)
    }

    fn scene_with(prose: &str) -> Scene {
        let mut scene = Scene::empty();
        scene.prose = prose.into();
        scene
    }

    fn response(status: u16, content_type: Option<&str>, body: &[u8]) -> HttpResponse {
        let headers = content_type
            .map(|value| {
                vec![HttpHeader {
                    name: "Content-Type".into(),
                    value: value.into(),
                    sensitive: false,
                }]
            })
            .unwrap_or_default();
        HttpResponse {
            status,
            headers,
            body: body.to_vec(),
        }
    }

    #[test]
    fn registry_lists_the_seven_chutes_models_in_menu_order() {
        let ids: Vec<_> = ModelRegistry::specs()
            .iter()
            .filter(|spec| spec.provider == ProviderId::Chutes)
            .map(|spec| spec.id.as_str())
            .collect();
        assert_eq!(ids, IDS);
        let names: Vec<_> = ModelRegistry::specs()
            .iter()
            .filter(|spec| spec.provider == ProviderId::Chutes)
            .map(|spec| spec.display_name.as_str())
            .collect();
        assert_eq!(names, NAMES);
        for (index, id) in IDS.iter().enumerate() {
            let spec = spec_of(id);
            assert_eq!(spec.provider, ProviderId::Chutes, "{id}");
            assert_eq!(spec.endpoint, ENDPOINTS[index], "{id}");
            assert!(!spec.beta, "{id}");
            assert!(!spec.capabilities.supports_boxes, "{id}");
            assert_eq!(spec.capabilities.size_control, SizeControl::WidthHeight);
            assert!(matches!(spec.auth, crate::model_spec::AuthScheme::Bearer));
        }
        assert!(ModelRegistry::providers().contains(&ProviderId::Chutes));
        assert_eq!(
            spec_of("chutes-flux-schnell").capabilities.image_slots,
            ImageSlots::None
        );
        match &spec_of("chutes-qwen-image-edit-2511")
            .capabilities
            .image_slots
        {
            ImageSlots::List { max, key } => {
                assert_eq!(*max, 3);
                assert_eq!(key, "image_b64s");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn flux_schnell_posts_its_default_body() {
        let (url, body) = posted(
            "chutes-flux-schnell",
            defaults("chutes-flux-schnell"),
            Vec::new(),
        );
        assert_eq!(url, "https://vonkaiser-imageclassic.chutes.ai/image/flux");
        assert_eq!(body["prompt"], "a lighthouse");
        assert_eq!(body["width"], 1024);
        assert_eq!(body["height"], 1024);
        assert_eq!(body["guidance_scale"], json!(3.5));
        assert_eq!(body["num_inference_steps"], 4);
        for key in ["seed", "negative_prompt", "model", "image_b64s"] {
            assert!(body.get(key).is_none(), "{key}");
        }
    }

    #[test]
    fn the_dreamshaper_family_posts_their_defaults() {
        for (index, id) in [
            "chutes-dreamshaper-xl",
            "chutes-ilustmix-v80",
            "chutes-juggernaut-xl",
        ]
        .into_iter()
        .enumerate()
        {
            let (url, body) = posted(id, defaults(id), Vec::new());
            assert_eq!(url, ENDPOINTS[index + 1], "{id}");
            assert_eq!(body["prompt"], "a lighthouse");
            assert_eq!(body["width"], 1024);
            assert_eq!(body["height"], 1024);
            assert_eq!(body["guidance_scale"], json!(7.5));
            assert_eq!(body["num_inference_steps"], 25);
            assert!(body.get("seed").is_none(), "{id}");
            assert!(body.get("negative_prompt").is_none(), "{id}");
            assert!(body.get("model").is_none(), "{id}");
        }
    }

    #[test]
    fn qwen_edit_posts_raw_base64_canvas_first() {
        let canvas = encoded(1);
        let second = encoded(2);
        let (url, body) = posted(
            "chutes-qwen-image-edit-2511",
            defaults("chutes-qwen-image-edit-2511"),
            vec![canvas.clone(), second.clone()],
        );
        assert_eq!(
            url,
            "https://vonkaiser-qwen-image-edit-2511.chutes.ai/generate"
        );
        let refs = body["image_b64s"].as_array().unwrap();
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].as_str().unwrap(), canvas.base64());
        assert_eq!(refs[1].as_str().unwrap(), second.base64());
        assert_eq!(body["prompt"], "a lighthouse");
        assert_eq!(body["true_cfg_scale"], json!(1.0));
        assert_eq!(body["num_inference_steps"], 4);
        assert!(body.get("model").is_none());
    }

    #[test]
    fn qwen_2512_posts_its_default_body() {
        let (url, body) = posted(
            "chutes-qwen-image-2512",
            defaults("chutes-qwen-image-2512"),
            Vec::new(),
        );
        assert_eq!(url, "https://vonkaiser-qwen-image-2512.chutes.ai/generate");
        assert_eq!(body["true_cfg_scale"], json!(4.0));
        assert_eq!(body["num_inference_steps"], 30);
        assert!(body.get("image_b64s").is_none());
        assert!(body.get("negative_prompt").is_none());
        assert!(body.get("model").is_none());
    }

    #[test]
    fn z_image_turbo_posts_its_default_body() {
        let (url, body) = posted(
            "chutes-z-image-turbo",
            defaults("chutes-z-image-turbo"),
            Vec::new(),
        );
        assert_eq!(url, "https://vonkaiser-z-image-turbo.chutes.ai/generate");
        assert_eq!(body["shift"], json!(3.0));
        assert_eq!(body["guidance_scale"], json!(0.0));
        assert_eq!(body["max_sequence_length"], 512);
        assert_eq!(body["num_inference_steps"], 9);
        assert!(body.get("model").is_none());
    }

    #[test]
    fn no_body_carries_a_model_field_or_a_data_uri() {
        for id in IDS {
            let (_, body) = posted(id, defaults(id), vec![encoded(1)]);
            let text = serde_json::to_string(&body).unwrap();
            assert!(!text.contains("\"model\""), "{id}: {text}");
            assert!(!text.contains("data:"), "{id}: {text}");
        }
    }

    #[test]
    fn field_bounds_follow_the_model_table() {
        for id in [
            "chutes-flux-schnell",
            "chutes-dreamshaper-xl",
            "chutes-ilustmix-v80",
            "chutes-juggernaut-xl",
            "chutes-qwen-image-edit-2511",
            "chutes-qwen-image-2512",
        ] {
            let schema = &spec_of(id).schema;
            assert!(!schema.accepts("width", &json!(127)), "{id}");
            assert!(schema.accepts("width", &json!(128)), "{id}");
            assert!(schema.accepts("width", &json!(2048)), "{id}");
            assert!(!schema.accepts("width", &json!(2049)), "{id}");
            assert!(!schema.accepts("num_inference_steps", &json!(0)), "{id}");
            assert!(!schema.accepts("seed", &json!(-1)), "{id}");
            assert!(schema.accepts("seed", &json!(4_294_967_295u64)), "{id}");
        }
        let flux = &spec_of("chutes-flux-schnell").schema;
        assert!(!flux.accepts("guidance_scale", &json!(20.5)));
        assert!(flux.accepts("guidance_scale", &json!(20.0)));
        let edit = &spec_of("chutes-qwen-image-edit-2511").schema;
        assert!(edit.accepts("num_inference_steps", &json!(100)));
        assert!(edit.accepts("true_cfg_scale", &json!(0.0)));
        let qwen = &spec_of("chutes-qwen-image-2512").schema;
        assert!(!qwen.accepts("num_inference_steps", &json!(4)));
        assert!(qwen.accepts("num_inference_steps", &json!(5)));
        assert!(!qwen.accepts("num_inference_steps", &json!(76)));
        assert!(qwen.accepts("num_inference_steps", &json!(75)));
        assert!(qwen.accepts("true_cfg_scale", &json!(0.0)));
        let z = &spec_of("chutes-z-image-turbo").schema;
        assert!(!z.accepts("width", &json!(575)));
        assert!(z.accepts("width", &json!(576)));
        assert!(z.accepts("width", &json!(2048)));
        assert!(z.accepts("num_inference_steps", &json!(4)));
        assert!(z.accepts("num_inference_steps", &json!(100)));
        assert!(z.accepts("guidance_scale", &json!(0.0)));
        assert!(!z.accepts("guidance_scale", &json!(5.5)));
        assert!(!z.accepts("shift", &json!(0.5)));
        assert!(z.accepts("shift", &json!(3.0)));
        assert!(z.accepts("max_sequence_length", &json!(512)));
        assert!(!z.accepts("max_sequence_length", &json!(128)));
    }

    #[test]
    fn failures_classify_by_status_alone() {
        let provider = provider("chutes-flux-schnell");
        assert!(matches!(
            provider
                .parse_submit(&response(401, None, b"nope"))
                .unwrap_err(),
            ProviderError::Auth
        ));
        assert!(matches!(
            provider
                .parse_submit(&response(402, None, b"nope"))
                .unwrap_err(),
            ProviderError::OutOfCredits { hint: None }
        ));
        assert!(matches!(
            provider
                .parse_submit(&response(404, None, b"nope"))
                .unwrap_err(),
            ProviderError::ModelNotFound
        ));
        assert_eq!(
            provider
                .parse_submit(&response(429, None, b"slow down"))
                .unwrap_err(),
            ProviderError::RateLimited { retry_after: None }
        );
        for status in [403, 422, 500, 503] {
            assert_eq!(
                provider
                    .parse_submit(&response(status, None, b"boom"))
                    .unwrap_err(),
                ProviderError::Http {
                    status,
                    body: "boom".into(),
                },
                "{status}"
            );
        }
        assert!(provider.inline_image(&response(401, None, b"x")).is_none());
    }

    #[test]
    fn a_matching_jpeg_or_png_200_becomes_the_inline_result() {
        let provider = provider("chutes-flux-schnell");
        let jpeg = response(200, Some("image/jpeg"), JPEG_MAGIC);
        let inline = provider.inline_image(&jpeg).unwrap();
        assert_eq!(inline.bytes, JPEG_MAGIC);
        assert_eq!(inline.mime, "image/jpeg");
        assert_eq!(inline.interaction_id, "");
        assert_eq!(inline.suggestion, "");
        let png = provider
            .inline_image(&response(200, Some("image/png"), PNG_MAGIC))
            .unwrap();
        assert_eq!(png.mime, "image/png");
        let normalized = provider
            .inline_image(&response(200, Some("image/jpg; charset=utf-8"), JPEG_MAGIC))
            .unwrap();
        assert_eq!(normalized.mime, "image/jpeg");
        assert_eq!(
            provider.parse_submit(&jpeg).unwrap_err(),
            ProviderError::Protocol("chutes returns the image on the submit response".into())
        );
    }

    #[test]
    fn a_200_that_is_not_a_matching_image_is_no_image() {
        let provider = provider("chutes-flux-schnell");
        for (kind, content_type, body) in [
            ("html", Some("text/html"), b"<html>hi</html>".as_slice()),
            ("json", Some("application/json"), b"{}".as_slice()),
            ("jpeg-header-png-body", Some("image/jpeg"), PNG_MAGIC),
            ("png-header-jpeg-body", Some("image/png"), JPEG_MAGIC),
            ("empty", Some("image/jpeg"), b"".as_slice()),
            ("no-magic", Some("image/jpeg"), b"not-an-image".as_slice()),
            ("missing-header", None, PNG_MAGIC),
            (
                "webp",
                Some("image/webp"),
                b"RIFF\x00\x00\x00\x00WEBPVP8".as_slice(),
            ),
        ] {
            let candidate = response(200, content_type, body);
            assert!(provider.inline_image(&candidate).is_none(), "{kind}");
            assert_eq!(
                provider.parse_submit(&candidate).unwrap_err(),
                ProviderError::NoImage,
                "{kind}"
            );
        }
    }

    #[test]
    fn parse_poll_is_always_the_failed_guard() {
        let provider = provider("chutes-flux-schnell");
        let image = response(200, Some("image/png"), PNG_MAGIC);
        let error = response(500, None, b"boom");
        for candidate in [image, error] {
            assert!(matches!(
                provider.parse_poll(&candidate),
                PollOutcome::Failed(ProviderError::Protocol(_))
            ));
        }
    }

    #[test]
    fn reference_steps_clip_to_the_slot_maximum() {
        let images: Vec<_> = (0..4).map(encoded).collect();
        let flux = provider("chutes-flux-schnell");
        assert!(flux.reference_steps(&images).is_empty());
        assert!(flux.reference_steps(&[]).is_empty());

        let edit = provider("chutes-qwen-image-edit-2511");
        let steps = edit.reference_steps(&images);
        assert_eq!(steps.len(), 3);
        for (step, image) in steps.iter().zip(&images) {
            match step {
                ReferenceStep::Inline(ImageInput::RawBase64(text)) => {
                    assert_eq!(text, &image.base64());
                }
                _ => panic!("expected inline base64"),
            }
        }
    }

    #[test]
    fn an_empty_prompt_is_reported_on_every_spec() {
        for id in IDS {
            let errors = provider(id).validate_for_submit(&scene_with("  "), &BTreeMap::new(), 0);
            assert!(
                errors
                    .iter()
                    .any(|error| error.key == "prompt" && error.message == "Prompt is empty"),
                "{id}"
            );
        }
    }

    #[test]
    fn only_the_edit_model_blocks_zero_images() {
        let errors = provider("chutes-qwen-image-edit-2511").validate_for_submit(
            &scene_with("edit it"),
            &BTreeMap::new(),
            0,
        );
        let image_error = errors
            .iter()
            .find(|error| error.key == "images")
            .unwrap_or_else(|| panic!("no images error: {errors:?}"));
        assert_eq!(image_error.message, "Add at least one image.");
        assert_eq!(image_error.approved_max, None);

        for id in [
            "chutes-flux-schnell",
            "chutes-dreamshaper-xl",
            "chutes-ilustmix-v80",
            "chutes-juggernaut-xl",
            "chutes-qwen-image-2512",
            "chutes-z-image-turbo",
        ] {
            let errors = provider(id).validate_for_submit(&scene_with("go"), &BTreeMap::new(), 0);
            assert!(!errors.iter().any(|error| error.key == "images"), "{id}");
        }
    }

    #[test]
    fn the_clamped_count_reaches_the_validator() {
        let prepared = 0usize;
        let max = spec_of("chutes-qwen-image-edit-2511")
            .capabilities
            .image_slots
            .max();
        let sent = prepared.min(max);
        assert_eq!(sent, 0);
        let errors = provider("chutes-qwen-image-edit-2511").validate_for_submit(
            &scene_with("edit it"),
            &BTreeMap::new(),
            sent,
        );
        assert!(
            errors
                .iter()
                .any(|error| error.message == "Add at least one image.")
        );
    }

    #[test]
    fn schema_violations_surface_from_the_validator() {
        let mut values: BTreeMap<String, Value> = spec_of("chutes-flux-schnell").schema.defaults();
        values.insert("width".into(), json!(64));
        let errors = FluxSchnell.validate_for_submit(&scene_with("go"), &values, 0);
        assert!(
            errors.iter().any(|error| error.key == "width"),
            "{errors:?}"
        );
    }
}
