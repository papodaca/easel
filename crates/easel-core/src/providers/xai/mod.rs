use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::error::{FieldError, ProviderError};
use crate::http::{Body, HttpHeader, HttpRequest, HttpResponse, Method};
use crate::job::{EncodedImage, JobRequest};
use crate::model_spec::{AuthScheme, Capabilities, ImageSlots, ModelSpec, ProviderId, SizeControl};
use crate::providers::{ImageInput, InlineImage, PollOutcome, Provider, ReferenceStep, Submitted};
use crate::scene::Scene;
use crate::schema::{FieldDef, FieldKind, ParamSchema, RuleContext};

/// Grok Imagine image 2.0.
#[derive(Debug, Clone, Copy)]
pub struct GrokImagine;

const GENERATIONS: &str = "https://api.x.ai/v1/images/generations";

const EDITS: &str = "https://api.x.ai/v1/images/edits";

const ASPECTS: &[&str] = &[
    "auto", "1:1", "3:4", "4:3", "9:16", "16:9", "2:3", "3:2", "9:19.5", "19.5:9", "9:20", "20:9",
    "1:2", "2:1", "21:9", "5:2",
];

const RESOLUTIONS: &[&str] = &["1k", "1.5k", "2k"];

const QUALITIES: &[&str] = &["low", "medium", "auto"];

pub const IMAGE_CAP: usize = 5;

static SPEC: std::sync::LazyLock<ModelSpec> = std::sync::LazyLock::new(spec);

impl GrokImagine {
    #[must_use]
    pub fn model_spec() -> &'static ModelSpec {
        &SPEC
    }
}

impl Provider for GrokImagine {
    fn spec(&self) -> &ModelSpec {
        &SPEC
    }

    fn validate_for_submit(
        &self,
        scene: &Scene,
        values: &BTreeMap<String, Value>,
        image_count: usize,
    ) -> Vec<FieldError> {
        let ctx = RuleContext {
            has_source_image: scene.source_image.is_some(),
            image_count,
            box_count: scene.boxes.len(),
        };
        let mut errors = SPEC.schema.validate(values, &ctx);
        if scene.prose.trim().is_empty() {
            errors.push(FieldError {
                key: "prompt".into(),
                message: "Prompt is empty".into(),
                approved_max: None,
            });
        }
        errors
    }

    fn reference_steps(&self, images: &[EncodedImage]) -> Vec<ReferenceStep> {
        images
            .iter()
            .take(SPEC.capabilities.image_slots.max())
            .map(|image| ReferenceStep::Inline(ImageInput::RawBase64(image.base64())))
            .collect()
    }

    fn build_submit(&self, request: &JobRequest, images: &[ImageInput]) -> HttpRequest {
        build_xai(&SPEC, request, images)
    }

    fn poll_request(&self, submitted: &Submitted) -> HttpRequest {
        HttpRequest {
            method: Method::Get,
            url: submitted.poll_url.to_string(),
            headers: Vec::new(),
            auth: Some(ProviderId::Xai),
            body: Body::Empty,
            max_body_bytes: 1024,
            redact_query: false,
        }
    }

    fn parse_submit(&self, response: &HttpResponse) -> Result<Submitted, ProviderError> {
        match imagine(response) {
            Ok(_) => Err(ProviderError::Protocol(
                "xai returns the image on the submit response".into(),
            )),
            Err(error) => Err(error),
        }
    }

    fn inline_image(&self, response: &HttpResponse) -> Option<InlineImage> {
        imagine(response).ok()
    }

    fn parse_poll(&self, _response: &HttpResponse) -> PollOutcome {
        PollOutcome::Failed(ProviderError::Protocol(
            "xai returns the image on the submit response".into(),
        ))
    }
}

fn spec() -> ModelSpec {
    ModelSpec {
        id: "grok-imagine-image-2.0".into(),
        display_name: "Grok Imagine 2.0".into(),
        provider: ProviderId::Xai,
        auth: AuthScheme::Bearer,
        endpoint: GENERATIONS.into(),
        beta: false,
        capabilities: Capabilities {
            supports_boxes: false,
            image_slots: ImageSlots::List {
                max: 5,
                key: "images".into(),
            },
            size_control: SizeControl::AspectRatioAndResolution,
        },
        schema: ParamSchema {
            fields: vec![
                select("aspect_ratio", "Aspect ratio", ASPECTS, "auto"),
                select("resolution", "Resolution", RESOLUTIONS, "1k"),
                select("quality", "Quality", QUALITIES, "auto"),
            ],
            cross_rules: Vec::new(),
        },
    }
}

#[must_use]
pub fn refusal_messages(prepared: usize) -> Vec<String> {
    if prepared > IMAGE_CAP {
        vec![format!("At most {IMAGE_CAP} images.")]
    } else {
        Vec::new()
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
        default: Some(json!(default)),
        locked_when: None,
    }
}

#[must_use]
fn data_url(media_type: &str, encoded: &str) -> String {
    format!("data:{media_type};base64,{encoded}")
}

fn field_string(spec: &ModelSpec, params: &BTreeMap<String, Value>, key: &str) -> Option<String> {
    if let Some(value) = params.get(key)
        && spec.schema.accepts(key, value)
        && let Some(text) = value.as_str()
    {
        return Some(text.to_owned());
    }
    spec.schema
        .fields
        .iter()
        .find(|field| field.key == key)
        .and_then(|field| field.default.as_ref())
        .and_then(|value| value.as_str())
        .map(str::to_owned)
}

fn image_value(request: &JobRequest, images: &[ImageInput], index: usize) -> Value {
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
    json!({
        "url": url,
        "type": "image_url",
    })
}

fn build_xai(spec: &ModelSpec, request: &JobRequest, images: &[ImageInput]) -> HttpRequest {
    let count = images.len().min(spec.capabilities.image_slots.max());
    let mut body = Map::new();
    body.insert("model".into(), Value::String(request.model_id.clone()));
    body.insert(
        "prompt".into(),
        Value::String(request.compiled_prompt.clone()),
    );
    for key in ["aspect_ratio", "resolution", "quality"] {
        if let Some(value) = field_string(spec, &request.params, key) {
            body.insert(key.into(), Value::String(value));
        }
    }
    body.insert("response_format".into(), Value::String("b64_json".into()));
    if count == 1 {
        body.insert("image".into(), image_value(request, images, 0));
    } else if count >= 2 {
        let list = (0..count)
            .map(|index| image_value(request, images, index))
            .collect();
        body.insert("images".into(), Value::Array(list));
    }
    HttpRequest {
        method: Method::Post,
        url: if count == 0 {
            GENERATIONS.into()
        } else {
            EDITS.into()
        },
        headers: vec![HttpHeader {
            name: "Content-Type".into(),
            value: "application/json".into(),
            sensitive: false,
        }],
        auth: Some(ProviderId::Xai),
        body: Body::Json(Value::Object(body)),
        max_body_bytes: 100 * 1024 * 1024,
        redact_query: false,
    }
}

fn imagine(response: &HttpResponse) -> Result<InlineImage, ProviderError> {
    if response.status == 401 {
        return Err(ProviderError::Auth);
    }
    if response.status == 429 {
        return Err(ProviderError::RateLimited { retry_after: None });
    }
    let value: Value = serde_json::from_slice(&response.body).unwrap_or(Value::Null);
    if response.status == 400 && error_code(&value) == "imagine:content-moderated" {
        return Err(ProviderError::Blocked);
    }
    if response.status != 200 {
        return Err(ProviderError::Http {
            status: response.status,
            body: String::from_utf8_lossy(&response.body).into_owned(),
        });
    }
    if moderated(&value) {
        return Err(ProviderError::Blocked);
    }
    let Some(items) = value.get("data").and_then(Value::as_array) else {
        return Err(ProviderError::Protocol("missing data".into()));
    };
    if items.is_empty() {
        return Err(ProviderError::Blocked);
    }
    let item = &items[0];
    match item.get("b64_json") {
        Some(Value::String(encoded)) if !encoded.is_empty() => decode_image(&value, item, encoded),
        Some(_) => Err(ProviderError::Blocked),
        None if has_url(item) => Err(ProviderError::NoImage),
        None => Err(ProviderError::Blocked),
    }
}

fn error_code(value: &Value) -> &str {
    value
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(Value::as_str)
        .or_else(|| value.get("code").and_then(Value::as_str))
        .unwrap_or("")
}

fn moderated(value: &Value) -> bool {
    if respect_moderation_false(value) {
        return true;
    }
    value
        .get("data")
        .and_then(Value::as_array)
        .and_then(|items| items.first())
        .is_some_and(respect_moderation_false)
}

fn respect_moderation_false(value: &Value) -> bool {
    value.get("respect_moderation").and_then(Value::as_bool) == Some(false)
}

fn has_url(item: &Value) -> bool {
    item.get("url")
        .and_then(Value::as_str)
        .is_some_and(|url| !url.is_empty())
}

fn decode_image(root: &Value, item: &Value, encoded: &str) -> Result<InlineImage, ProviderError> {
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
        .map_err(|_| ProviderError::Protocol("invalid image".into()))?;
    let interaction_id = root
        .get("request_id")
        .and_then(Value::as_str)
        .filter(|id| !id.is_empty())
        .unwrap_or("")
        .to_owned();
    Ok(InlineImage {
        bytes,
        mime: mime_type(item),
        interaction_id,
        suggestion: String::new(),
    })
}

fn mime_type(item: &Value) -> String {
    let mime = item.get("mime_type").and_then(Value::as_str).unwrap_or("");
    if mime.is_empty() {
        "image/jpeg".into()
    } else {
        mime.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::GrokImagine;
    use crate::error::ProviderError;
    use crate::http::{Body, HttpResponse, Method};
    use crate::job::{EncodedImage, JobRequest};
    use crate::model_spec::ProviderId;
    use crate::providers::Provider;
    use crate::providers::{ImageInput, ReferenceStep};
    use crate::schema::FieldKind;
    use serde_json::{Value, json};

    fn encoded(bytes: &[u8]) -> EncodedImage {
        EncodedImage {
            media_type: "image/png".into(),
            bytes: bytes.to_vec(),
            width: 256,
            height: 256,
            content_hash: "h".into(),
        }
    }

    fn data_uri(image: &EncodedImage) -> String {
        format!("data:{};base64,{}", image.media_type, image.base64())
    }

    fn request(prompt: &str, params: Value, images: Vec<EncodedImage>) -> JobRequest {
        let params = params.as_object().unwrap().clone().into_iter().collect();
        JobRequest {
            model_id: "grok-imagine-image-2.0".into(),
            compiled_prompt: prompt.into(),
            params,
            images,
            box_snapshot: Vec::new(),
            created_at: "2026-10-18T00:00:00Z".into(),
        }
    }

    fn inputs(images: &[EncodedImage]) -> Vec<ImageInput> {
        images
            .iter()
            .map(|image| ImageInput::RawBase64(image.base64()))
            .collect()
    }

    fn posted(prompt: &str, params: Value, images: Vec<EncodedImage>) -> (String, Value) {
        let provider = crate::providers::resolve_model("grok-imagine-image-2.0").unwrap();
        let request = request(prompt, params, images.clone());
        let http = provider.build_submit(&request, &inputs(&images));
        assert_eq!(http.method, Method::Post);
        assert_eq!(http.auth, Some(ProviderId::Xai));
        let Body::Json(json) = http.body else {
            panic!("expected json");
        };
        (http.url, json)
    }

    fn form_fields() -> Value {
        json!({
            "aspect_ratio": "auto",
            "resolution": "1k",
            "quality": "auto"
        })
    }

    fn assert_no_file_id(value: &Value) {
        match value {
            Value::Object(map) => {
                assert!(map.get("file_id").is_none());
                for nested in map.values() {
                    assert_no_file_id(nested);
                }
            }
            Value::Array(items) => {
                for item in items {
                    assert_no_file_id(item);
                }
            }
            _ => {}
        }
    }

    fn owned(options: &[&str]) -> Vec<String> {
        options.iter().map(|option| (*option).to_string()).collect()
    }

    fn options(key: &str) -> Vec<String> {
        let field = GrokImagine::model_spec()
            .schema
            .fields
            .iter()
            .find(|field| field.key == key)
            .unwrap_or_else(|| panic!("missing {key}"));
        match &field.kind {
            FieldKind::Select { options } => options.clone(),
            _ => panic!("{key} is not a select"),
        }
    }

    #[test]
    fn zero_images_post_generations() {
        let (url, json) = posted("a harbor", form_fields(), Vec::new());
        assert_eq!(url, "https://api.x.ai/v1/images/generations");
        assert_eq!(json["model"], "grok-imagine-image-2.0");
        assert_eq!(json["prompt"], "a harbor");
        assert_eq!(json["response_format"], "b64_json");
        assert_eq!(json["aspect_ratio"], "auto");
        assert_eq!(json["resolution"], "1k");
        assert_eq!(json["quality"], "auto");
        for key in ["image", "images", "n", "deferred"] {
            assert!(json.get(key).is_none(), "{key}");
        }
    }

    #[test]
    fn one_image_posts_a_singular_edit() {
        let image = encoded(b"canvas");
        let (url, json) = posted("make it blue", form_fields(), vec![image.clone()]);
        assert_eq!(url, "https://api.x.ai/v1/images/edits");
        assert_eq!(json["image"]["url"], data_uri(&image));
        assert_eq!(json["image"]["type"], "image_url");
        assert!(json.get("images").is_none());
        assert_no_file_id(&json);
    }

    #[test]
    fn three_images_post_an_images_array() {
        let canvas = encoded(b"canvas");
        let left = encoded(b"left");
        let right = encoded(b"right");
        let (url, json) = posted(
            "combine them",
            form_fields(),
            vec![canvas.clone(), left, right],
        );
        assert_eq!(url, "https://api.x.ai/v1/images/edits");
        assert_eq!(json["images"].as_array().unwrap().len(), 3);
        assert_eq!(json["images"][0]["url"], data_uri(&canvas));
        assert_eq!(json["images"][0]["type"], "image_url");
        assert!(json.get("image").is_none());
        assert_no_file_id(&json);
    }

    #[test]
    fn six_prepared_images_keep_five() {
        let images: Vec<_> = (0..6).map(|n| encoded(&[n])).collect();
        let provider = crate::providers::resolve_model("grok-imagine-image-2.0").unwrap();
        let steps = provider.reference_steps(&images);
        assert_eq!(steps.len(), 5);
        match &steps[0] {
            ReferenceStep::Inline(ImageInput::RawBase64(encoded)) => {
                assert_eq!(encoded, &images[0].base64());
            }
            ReferenceStep::Inline(ImageInput::Url(_)) | ReferenceStep::Upload { .. } => {
                panic!("expected inline base64");
            }
        }
        let (url, json) = posted("six", form_fields(), images.clone());
        assert_eq!(url, "https://api.x.ai/v1/images/edits");
        let list = json["images"].as_array().unwrap();
        assert_eq!(list.len(), 5);
        assert_eq!(list[0]["url"], data_uri(&images[0]));
        assert_eq!(list[4]["url"], data_uri(&images[4]));
        assert!(json.get("image").is_none());
    }

    #[test]
    fn form_fields_copy_onto_generation_and_edit() {
        let params = json!({
            "quality": "auto",
            "resolution": "1.5k",
            "aspect_ratio": "19.5:9"
        });
        let (url, generated) = posted("gen", params.clone(), Vec::new());
        assert_eq!(url, "https://api.x.ai/v1/images/generations");
        assert_eq!(generated["quality"], "auto");
        assert_eq!(generated["resolution"], "1.5k");
        assert_eq!(generated["aspect_ratio"], "19.5:9");
        let image = encoded(b"one");
        let (url, edited) = posted("edit", params, vec![image]);
        assert_eq!(url, "https://api.x.ai/v1/images/edits");
        assert_eq!(edited["quality"], "auto");
        assert_eq!(edited["resolution"], "1.5k");
        assert_eq!(edited["aspect_ratio"], "19.5:9");
    }

    #[test]
    fn six_prepared_images_are_refused() {
        assert!(super::refusal_messages(5).is_empty());
        assert_eq!(
            super::refusal_messages(6),
            vec!["At most 5 images.".to_string()]
        );
    }

    #[test]
    fn spec_lists_the_imagine_enums() {
        let spec = GrokImagine::model_spec();
        assert!(!spec.capabilities.supports_boxes);
        assert_eq!(spec.capabilities.image_slots.max(), 5);
        assert_eq!(spec.provider, ProviderId::Xai);
        assert_eq!(options("aspect_ratio"), owned(super::ASPECTS));
        assert_eq!(options("resolution"), owned(super::RESOLUTIONS));
        assert_eq!(options("quality"), owned(super::QUALITIES));
        let defaults = spec.schema.defaults();
        assert_eq!(defaults.get("aspect_ratio"), Some(&json!("auto")));
        assert_eq!(defaults.get("resolution"), Some(&json!("1k")));
        assert_eq!(defaults.get("quality"), Some(&json!("auto")));
    }

    #[test]
    fn flux_3_still_resolves_to_black_forest_labs() {
        let provider = crate::providers::resolve_model("flux-3-image").unwrap();
        assert_eq!(provider.spec().id, "flux-3-image");
        assert_eq!(provider.spec().provider, ProviderId::Bfl);
        assert_eq!(provider.spec().provider.display_name(), "Black Forest Labs");
    }

    fn response(status: u16, body: &str) -> HttpResponse {
        HttpResponse {
            status,
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn null_or_empty_b64_json_is_blocked_before_any_url() {
        let provider = GrokImagine;
        for body in [
            r#"{"data":[{"b64_json":null}]}"#,
            r#"{"data":[{"b64_json":""}]}"#,
            r#"{"data":[{"b64_json":null,"url":"https://example.com/a.jpg"}]}"#,
        ] {
            let response = response(200, body);
            assert!(provider.inline_image(&response).is_none(), "{body}");
            assert_eq!(
                provider.parse_submit(&response).unwrap_err(),
                ProviderError::Blocked,
                "{body}"
            );
        }
    }

    #[test]
    fn a_request_id_alone_is_not_a_poll_ticket() {
        let provider = GrokImagine;
        let response = response(200, r#"{"request_id":"req-1"}"#);
        assert!(provider.inline_image(&response).is_none());
        let error = provider.parse_submit(&response).unwrap_err();
        assert!(matches!(error, ProviderError::Protocol(_)), "{error:?}");
    }

    #[test]
    fn an_empty_mime_stays_inline_jpeg_and_is_not_submitted() {
        let provider = GrokImagine;
        let image = encoded(b"jpeg-bytes");
        let response = response(
            200,
            &format!(
                r#"{{"request_id":"","data":[{{"b64_json":"{}","mime_type":""}}]}}"#,
                image.base64()
            ),
        );
        let inline = provider.inline_image(&response).unwrap();
        assert_eq!(inline.bytes, b"jpeg-bytes");
        assert_eq!(inline.mime, "image/jpeg");
        assert_eq!(inline.interaction_id, "");
        assert!(matches!(
            provider.parse_submit(&response).unwrap_err(),
            ProviderError::Protocol(_)
        ));
    }

    #[test]
    fn webp_mime_is_passed_through() {
        let provider = GrokImagine;
        let image = encoded(b"webp-bytes");
        let response = response(
            200,
            &format!(
                r#"{{"request_id":"req-webp","data":[{{"b64_json":"{}","mime_type":"image/webp"}},{{"b64_json":"later"}}]}}"#,
                image.base64()
            ),
        );
        let inline = provider.inline_image(&response).unwrap();
        assert_eq!(inline.bytes, b"webp-bytes");
        assert_eq!(inline.mime, "image/webp");
        assert_eq!(inline.interaction_id, "req-webp");
    }
}
