use std::collections::BTreeMap;
use std::sync::LazyLock;

use serde_json::{Map, Value, json};

use crate::error::{FieldError, ProviderError};
use crate::http::{Body, HttpHeader, HttpRequest, HttpResponse, Method};
use crate::job::{EncodedImage, JobRequest};
use crate::model_spec::{AuthScheme, Capabilities, ImageSlots, ModelSpec, ProviderId, SizeControl};
use crate::providers::{ImageInput, PollOutcome, Provider, ReferenceStep, Submitted};
use crate::scene::Scene;
use crate::schema::{FieldDef, FieldKind, ParamSchema};

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

/// The guard firing when a 200 body still carries a picture URL.
const PICTURE_URL_GUARD: &str = "qwen returns an image url on the submit response";

static QWEN_IMAGE_30_PRO: LazyLock<ModelSpec> =
    LazyLock::new(|| qwen_spec("qwen-image-3.0-pro", "Qwen Image 3.0 Pro", 3, true));

static QWEN_IMAGE_21_PRO: LazyLock<ModelSpec> =
    LazyLock::new(|| qwen_spec("qwen-image-2.1-pro", "Qwen Image 2.1 Pro", 10, false));

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

/// The largest per-side value an R6-valid size can reach, with headroom.
const DIMENSION_MAX: i64 = 8192;

fn qwen_spec(id: &str, display_name: &str, max_images: usize, negative_prompt: bool) -> ModelSpec {
    let mut fields = vec![
        dimension_field("width", "Width"),
        dimension_field("height", "Height"),
    ];
    if negative_prompt {
        fields.push(FieldDef {
            key: "negative_prompt".into(),
            label: "Negative prompt".into(),
            help: None,
            kind: FieldKind::Text { multiline: false },
            required: false,
            default: None,
            locked_when: None,
        });
    }
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
            fields,
            cross_rules: Vec::new(),
        },
    }
}

fn dimension_field(key: &str, label: &str) -> FieldDef {
    FieldDef {
        key: key.into(),
        label: label.into(),
        help: Some(format!(
            "Leave both empty to let the model choose. Both set: the pixel count must be from {MIN_AREA} through {MAX_AREA} and the long side at most {RATIO_CAP} times the short side."
        )),
        kind: FieldKind::Int {
            min: 0,
            max: DIMENSION_MAX,
            step: 1,
        },
        required: false,
        default: None,
        locked_when: None,
    }
}

fn dimension(values: &BTreeMap<String, Value>, key: &str) -> i64 {
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

fn validate_size(values: &BTreeMap<String, Value>) -> Vec<FieldError> {
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
    values: &BTreeMap<String, Value>,
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
        redact_query: false,
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
        redact_query: false,
    }
}

fn body_value(response: &HttpResponse) -> Value {
    serde_json::from_slice(&response.body).unwrap_or(Value::Null)
}

fn body_code(value: &Value) -> &str {
    value
        .get("code")
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("error")
                .and_then(|error| error.get("code"))
                .and_then(Value::as_str)
        })
        .unwrap_or("")
}

fn body_message(value: &Value) -> &str {
    value
        .get("message")
        .and_then(Value::as_str)
        .or_else(|| {
            value
                .get("error")
                .and_then(|error| error.get("message"))
                .and_then(Value::as_str)
        })
        .unwrap_or("")
}

fn is_moderation(code: &str) -> bool {
    code == "DataInspectionFailed" || code == "data_inspection_failed"
}

fn is_throttling(code: &str) -> bool {
    code == "Throttling" || code.starts_with("Throttling.")
}

/// The first image URL in a 200 body, or none when the body carries no picture.
fn first_image_url(value: &Value) -> Option<url::Url> {
    let content = value
        .get("output")?
        .get("choices")?
        .get(0)?
        .get("message")?
        .get("content")?
        .as_array()?;
    for entry in content {
        if let Some(url) = entry.get("image").and_then(Value::as_str) {
            return url::Url::parse(url).ok();
        }
    }
    None
}

/// Classify one synchronous response per the Qwen error catalog.
fn classify(response: &HttpResponse) -> Result<Submitted, ProviderError> {
    let value = body_value(response);
    let code = body_code(&value);
    if is_moderation(code) {
        return Err(ProviderError::Blocked);
    }
    if is_throttling(code) {
        return Err(ProviderError::RateLimited { retry_after: None });
    }
    match response.status {
        401 => Err(ProviderError::Auth),
        404 => Err(ProviderError::ModelNotFound),
        400 if body_message(&value) == "Model not exist." => Err(ProviderError::ModelNotFound),
        429 => Err(ProviderError::RateLimited { retry_after: None }),
        200 if first_image_url(&value).is_some() => {
            Err(ProviderError::Protocol(PICTURE_URL_GUARD.into()))
        }
        200 => Err(ProviderError::NoImage),
        status => Err(ProviderError::Http {
            status,
            body: String::from_utf8_lossy(&response.body).into_owned(),
        }),
    }
}

/// True when a result URL is safe to fetch: HTTPS, no userinfo, and a
/// literal host outside the private, loopback, and link-local ranges.
/// Hostnames resolve at request time without a DNS screen, which the plan
/// defers to follow-up work.
#[must_use]
pub fn is_downloadable(url: &url::Url) -> bool {
    if url.scheme() != "https" || !url.username().is_empty() || url.password().is_some() {
        return false;
    }
    match url.host() {
        Some(url::Host::Ipv4(ip)) => !is_private_v4(&ip),
        Some(url::Host::Ipv6(ip)) => !is_private_v6(&ip),
        Some(url::Host::Domain(_)) => true,
        None => false,
    }
}

fn is_private_v4(ip: &std::net::Ipv4Addr) -> bool {
    let octets = ip.octets();
    ip.is_loopback()
        || ip.is_link_local()
        || ip.is_unspecified()
        || octets[0] == 10
        || (octets[0] == 172 && (16..=31).contains(&octets[1]))
        || (octets[0] == 192 && octets[1] == 168)
}

fn is_private_v6(ip: &std::net::Ipv6Addr) -> bool {
    if let Some(mapped) = ip.to_ipv4_mapped() {
        return is_private_v4(&mapped);
    }
    let segments = ip.segments();
    ip.is_loopback()
        || ip.is_unspecified()
        || segments[0] & 0xffc0 == 0xfe80 // link-local fe80::/10
        || segments[0] & 0xfe00 == 0xfc00 // unique local fc00::/7
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
                values: &BTreeMap<String, Value>,
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

            fn parse_submit(&self, response: &HttpResponse) -> Result<Submitted, ProviderError> {
                classify(response)
            }

            fn picture_url(&self, response: &HttpResponse) -> Option<url::Url> {
                if response.status != 200 {
                    return None;
                }
                first_image_url(&body_value(response))
            }

            fn download_request(&self, url: &url::Url) -> HttpRequest {
                let mut request = crate::providers::plain_download(url);
                request.redact_query = true;
                request
            }

            fn parse_poll(&self, _response: &HttpResponse) -> PollOutcome {
                PollOutcome::Failed(ProviderError::Protocol(PICTURE_URL_GUARD.into()))
            }
        }
    };
}

impl_qwen!(QwenImage30Pro, QWEN_IMAGE_30_PRO, true);
impl_qwen!(QwenImage21Pro, QWEN_IMAGE_21_PRO, false);

/// The largest data URL the API accepts for one input image.
const ENCODED_CAP: usize = 10 * 1024 * 1024;

/// The input aspect limit: long side to short side.
const INPUT_RATIO_CAP: u32 = 10;

/// The data URL length without building it: `data:` plus the media type plus
/// `;base64,` plus standard padded base64 of the bytes.
fn encoded_url_len(media_type: &str, bytes: usize) -> usize {
    5 + media_type.len() + 8 + 4 * bytes.div_ceil(3)
}

/// The lines the form shows for an over-cap or unsuitable image. A capped
/// count never reaches the builder, so these are the only over-cap proof.
#[must_use]
pub fn refusal_messages(spec: &ModelSpec, prepared: usize, images: &[EncodedImage]) -> Vec<String> {
    let max = spec.capabilities.image_slots.max();
    let mut messages = Vec::new();
    if prepared > max {
        messages.push(format!("At most {max} images."));
    }
    let sent = &images[..images.len().min(max)];
    if sent
        .iter()
        .any(|image| encoded_url_len(&image.media_type, image.bytes.len()) > ENCODED_CAP)
    {
        messages.push("An encoded image is over 10 MB.".into());
    }
    if sent.iter().any(|image| {
        let (long, short) = (
            image.width.max(image.height),
            image.width.min(image.height).max(1),
        );
        long > short * INPUT_RATIO_CAP
    }) {
        messages.push("An image's long side is more than 10 times its short side.".into());
    }
    messages
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

    use crate::schema::FieldKind;

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
        let pro = spec_of("qwen-image-3.0-pro");
        let wide = spec_of("qwen-image-2.1-pro");
        assert_eq!(
            super::refusal_messages(pro, 4, &[]),
            vec!["At most 3 images.".to_string()]
        );
        assert!(super::refusal_messages(pro, 3, &[]).is_empty());
        assert_eq!(
            super::refusal_messages(wide, 11, &[]),
            vec!["At most 10 images.".to_string()]
        );
        assert!(super::refusal_messages(wide, 10, &[]).is_empty());

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
    fn refusal_lines_cover_the_input_limits() {
        let pro = spec_of("qwen-image-3.0-pro");
        let huge = EncodedImage {
            media_type: "image/png".into(),
            bytes: vec![0x42; 9_755_648],
            width: 2048,
            height: 2048,
            content_hash: "huge".into(),
        };
        assert_eq!(
            super::refusal_messages(pro, 1, std::slice::from_ref(&huge)),
            vec!["An encoded image is over 10 MB.".to_string()]
        );
        let stretched = EncodedImage {
            media_type: "image/png".into(),
            bytes: vec![1; 16],
            width: 2200,
            height: 200,
            content_hash: "wide".into(),
        };
        assert_eq!(
            super::refusal_messages(pro, 1, std::slice::from_ref(&stretched)),
            vec!["An image's long side is more than 10 times its short side.".to_string()]
        );
        let acceptable = EncodedImage {
            media_type: "image/png".into(),
            bytes: vec![1; 16],
            width: 2048,
            height: 256,
            content_hash: "ok".into(),
        };
        // An image beyond the cap would not be sent, so it is not checked.
        let four = vec![
            acceptable.clone(),
            acceptable.clone(),
            acceptable,
            huge.clone(),
        ];
        assert_eq!(
            super::refusal_messages(pro, 4, &four),
            vec!["At most 3 images.".to_string()]
        );
        let over = vec![stretched, huge];
        assert_eq!(
            super::refusal_messages(pro, 4, &over),
            vec![
                "At most 3 images.".to_string(),
                "An encoded image is over 10 MB.".to_string(),
                "An image's long side is more than 10 times its short side.".to_string(),
            ]
        );
    }

    #[test]
    fn the_form_fields_follow_the_model() {
        let pro = spec_of("qwen-image-3.0-pro");
        let wide = spec_of("qwen-image-2.1-pro");
        for spec in [pro, wide] {
            let width = spec
                .schema
                .fields
                .iter()
                .find(|field| field.key == "width")
                .expect("width");
            let height = spec
                .schema
                .fields
                .iter()
                .find(|field| field.key == "height")
                .expect("height");
            for field in [width, height] {
                assert!(matches!(
                    field.kind,
                    FieldKind::Int {
                        min: 0,
                        max: 8192,
                        step: 1
                    }
                ));
                assert!(field.default.is_none());
                assert!(!field.required);
                let help = field.help.as_deref().unwrap_or_default();
                assert!(help.contains("262144"), "{help}");
                assert!(help.contains("4194304"), "{help}");
                assert!(help.contains("8 times"), "{help}");
            }
        }
        assert!(
            pro.schema
                .fields
                .iter()
                .any(|field| field.key == "negative_prompt")
        );
        assert!(
            !wide
                .schema
                .fields
                .iter()
                .any(|field| field.key == "negative_prompt")
        );
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

    fn picture_body(url: &str, extra: &str) -> String {
        format!(
            r#"{{"output":{{"choices":[{{"message":{{"content":[{{"image":"{url}"}}{extra}]}}}}]}}}}"#
        )
    }

    fn response(status: u16, body: &str) -> HttpResponse {
        HttpResponse {
            status,
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn classification_follows_the_error_catalog() {
        let provider = crate::providers::resolve_model("qwen-image-3.0-pro").unwrap();
        let cases: Vec<(u16, &str, ProviderError)> = vec![
            (
                401,
                r#"{"code":"InvalidApiKey","message":"no"}"#,
                ProviderError::Auth,
            ),
            (
                404,
                r#"{"code":"model_not_found","message":"no"}"#,
                ProviderError::ModelNotFound,
            ),
            (
                400,
                r#"{"code":"InvalidParameter","message":"Model not exist."}"#,
                ProviderError::ModelNotFound,
            ),
            (
                400,
                r#"{"code":"DataInspectionFailed","message":"no","output":{"choices":[{"message":{"content":[{"image":"https://example.com/a.png"}]}}]}}"#,
                ProviderError::Blocked,
            ),
            (
                400,
                r#"{"code":"data_inspection_failed","message":"no"}"#,
                ProviderError::Blocked,
            ),
            (
                429,
                r#"{"code":"SystemBusy","message":"slow"}"#,
                ProviderError::RateLimited { retry_after: None },
            ),
            (
                400,
                r#"{"code":"InvalidParameter","message":"bad size"}"#,
                ProviderError::Http {
                    status: 400,
                    body: r#"{"code":"InvalidParameter","message":"bad size"}"#.into(),
                },
            ),
            (
                500,
                r#"{"code":"InternalError","message":"boom"}"#,
                ProviderError::Http {
                    status: 500,
                    body: r#"{"code":"InternalError","message":"boom"}"#.into(),
                },
            ),
        ];
        for (status, body, expected) in cases {
            let error = provider.parse_submit(&response(status, body)).unwrap_err();
            assert_eq!(error, expected, "{status} {body}");
        }
    }

    #[test]
    fn a_throttling_code_is_rate_limited_on_any_status() {
        let provider = crate::providers::resolve_model("qwen-image-3.0-pro").unwrap();
        for status in [200, 400, 403, 429] {
            for code in ["Throttling", "Throttling.RateQuota"] {
                let body = format!(r#"{{"code":"{code}","message":"slow"}}"#);
                assert_eq!(
                    provider.parse_submit(&response(status, &body)).unwrap_err(),
                    ProviderError::RateLimited { retry_after: None },
                    "{status} {code}"
                );
            }
        }
    }

    #[test]
    fn a_200_with_a_picture_url_reports_it_and_never_polls() {
        let provider = crate::providers::resolve_model("qwen-image-3.0-pro").unwrap();
        let body = picture_body(
            "https://dashscope-result-sz.oss-cn-shenzhen.aliyuncs.com/a.png",
            "",
        );
        let parsed = response(200, &body);
        assert_eq!(
            provider.picture_url(&parsed).map(|url| url.to_string()),
            Some("https://dashscope-result-sz.oss-cn-shenzhen.aliyuncs.com/a.png".into())
        );
        assert!(matches!(
            provider.parse_submit(&parsed),
            Err(ProviderError::Protocol(_))
        ));

        let two = picture_body(
            "https://dashscope-result-sz.oss-cn-shenzhen.aliyuncs.com/first.png",
            r#",{"image":"https://dashscope-result-sz.oss-cn-shenzhen.aliyuncs.com/second.png"}"#,
        );
        assert_eq!(
            provider
                .picture_url(&response(200, &two))
                .map(|url| url.to_string()),
            Some("https://dashscope-result-sz.oss-cn-shenzhen.aliyuncs.com/first.png".into())
        );
    }

    #[test]
    fn a_200_without_a_picture_url_is_no_image() {
        let provider = crate::providers::resolve_model("qwen-image-3.0-pro").unwrap();
        for body in [
            r#"{"output":{"choices":[{"message":{"content":[{"text":"hi"}]}}]}}"#,
            r#"{"output":{}}"#,
            r#"not json"#,
            r#"{}"#,
        ] {
            let parsed = response(200, body);
            assert!(provider.picture_url(&parsed).is_none(), "{body}");
            assert_eq!(
                provider.parse_submit(&parsed).unwrap_err(),
                ProviderError::NoImage,
                "{body}"
            );
        }
    }

    #[test]
    fn a_non_200_body_never_reports_a_picture_url() {
        let provider = crate::providers::resolve_model("qwen-image-3.0-pro").unwrap();
        let body = picture_body(
            "https://dashscope-result-sz.oss-cn-shenzhen.aliyuncs.com/a.png",
            "",
        );
        assert!(provider.picture_url(&response(400, &body)).is_none());
    }

    #[test]
    fn result_urls_are_screened_for_a_public_https_host() {
        let ok = [
            "https://dashscope-result-sz.oss-cn-shenzhen.aliyuncs.com/a.png?Expires=1&Signature=s",
            "https://example.com/a.png",
            "https://8.8.8.8/a.png",
        ];
        let refused = [
            "http://example.com/a.png",
            "https://user:pass@example.com/a.png",
            "https://127.0.0.1/a.png",
            "https://0.0.0.0/a.png",
            "https://10.0.0.9/a.png",
            "https://172.16.0.9/a.png",
            "https://172.31.0.9/a.png",
            "https://192.168.1.9/a.png",
            "https://169.254.1.9/a.png",
            "https://[::1]/a.png",
            "https://[::]/a.png",
            "https://[::ffff:127.0.0.1]/a.png",
            "https://[::ffff:10.0.0.9]/a.png",
            "https://[::ffff:169.254.1.9]/a.png",
            "https://[fe80::1]/a.png",
            "https://[fd00::1]/a.png",
        ];
        for url in ok {
            let parsed: url::Url = url.parse().unwrap();
            assert!(super::is_downloadable(&parsed), "{url}");
        }
        for url in refused {
            let parsed: url::Url = url.parse().unwrap();
            assert!(!super::is_downloadable(&parsed), "{url}");
        }
    }
}
