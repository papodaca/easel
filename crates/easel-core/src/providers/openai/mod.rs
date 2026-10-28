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

const EDITS: &str = "https://api.openai.com/v1/images/edits";

pub const PROMPT_CAP: usize = 32_000;

pub const DATA_URL_CAP: usize = 20_971_520;

pub const IMAGE_CAP: usize = 16;

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

#[must_use]
pub fn data_url(media_type: &str, encoded: &str) -> String {
    format!("data:{media_type};base64,{encoded}")
}

#[must_use]
pub fn refusal_messages(prompt: &str, data_urls: &[String], prepared: usize) -> Vec<String> {
    let mut messages = Vec::new();
    if prompt.chars().count() > PROMPT_CAP {
        messages.push(format!("The prompt is over {PROMPT_CAP} characters."));
    }
    if prepared > IMAGE_CAP {
        messages.push(format!("At most {IMAGE_CAP} images."));
    }
    if data_urls
        .iter()
        .any(|url| url.chars().count() > DATA_URL_CAP)
    {
        messages.push(format!(
            "An image data URL is over {DATA_URL_CAP} characters."
        ));
    }
    messages
}

fn field_string(
    spec: &ModelSpec,
    params: &std::collections::BTreeMap<String, serde_json::Value>,
    key: &str,
) -> Option<String> {
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

fn response_json(response: &crate::http::HttpResponse) -> serde_json::Value {
    serde_json::from_slice(&response.body).unwrap_or(serde_json::Value::Null)
}

fn error_code(value: &serde_json::Value) -> &str {
    value
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(|code| code.as_str())
        .unwrap_or("")
}

fn decode_png(response: &crate::http::HttpResponse) -> Option<Vec<u8>> {
    if response.status != 200 {
        return None;
    }
    let value = response_json(response);
    if error_code(&value) == "moderation_blocked" {
        return None;
    }
    let encoded = value
        .get("data")?
        .as_array()?
        .first()?
        .get("b64_json")?
        .as_str()?;
    if encoded.is_empty() {
        return None;
    }
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded).ok()
}

fn parse_openai(
    response: &crate::http::HttpResponse,
) -> Result<crate::providers::Submitted, crate::error::ProviderError> {
    use crate::error::ProviderError;

    let value = response_json(response);
    if error_code(&value) == "moderation_blocked" {
        return Err(ProviderError::Blocked);
    }
    if response.status == 401 {
        return Err(ProviderError::Auth);
    }
    if decode_png(response).is_some() {
        return Err(ProviderError::Protocol(
            "openai returns the image on the submit response".into(),
        ));
    }
    if response.status == 200 {
        return Err(ProviderError::NoImage);
    }
    Err(ProviderError::Http {
        status: response.status,
        body: String::from_utf8_lossy(&response.body).into_owned(),
    })
}

fn build_openai(
    spec: &ModelSpec,
    request: &crate::job::JobRequest,
    images: &[crate::providers::ImageInput],
) -> crate::http::HttpRequest {
    use crate::http::{Body, HttpHeader, HttpRequest, Method};
    use crate::providers::ImageInput;
    use serde_json::{Map, Value, json};

    let edits = !images.is_empty();
    let mut body = Map::new();
    body.insert("model".into(), Value::String(request.model_id.clone()));
    body.insert(
        "prompt".into(),
        Value::String(request.compiled_prompt.clone()),
    );
    for key in ["quality", "size", "background"] {
        if let Some(value) = field_string(spec, &request.params, key) {
            body.insert(key.into(), Value::String(value));
        }
    }
    body.insert("output_format".into(), Value::String("png".into()));
    if edits {
        let urls = images
            .iter()
            .enumerate()
            .map(|(index, image)| {
                let image_url = match image {
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
                json!({ "image_url": image_url })
            })
            .collect();
        body.insert("images".into(), Value::Array(urls));
        if spec
            .schema
            .fields
            .iter()
            .any(|field| field.key == "input_fidelity")
            && let Some(value) = field_string(spec, &request.params, "input_fidelity")
        {
            body.insert("input_fidelity".into(), Value::String(value));
        }
    }
    HttpRequest {
        method: Method::Post,
        url: if edits {
            EDITS.into()
        } else {
            spec.endpoint.clone()
        },
        headers: vec![HttpHeader {
            name: "Content-Type".into(),
            value: "application/json".into(),
            sensitive: false,
        }],
        auth: Some(ProviderId::OpenAi),
        body: Body::Json(Value::Object(body)),
        max_body_bytes: 100 * 1024 * 1024,
        redact_query: false,
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
                request: &crate::job::JobRequest,
                images: &[crate::providers::ImageInput],
            ) -> crate::http::HttpRequest {
                build_openai(&$spec, request, images)
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
                    redact_query: false,
                }
            }

            fn parse_submit(
                &self,
                response: &crate::http::HttpResponse,
            ) -> Result<crate::providers::Submitted, crate::error::ProviderError> {
                parse_openai(response)
            }

            fn inline_image(
                &self,
                response: &crate::http::HttpResponse,
            ) -> Option<crate::providers::InlineImage> {
                decode_png(response).map(|bytes| crate::providers::InlineImage {
                    bytes,
                    mime: "image/png".into(),
                    interaction_id: String::new(),
                    suggestion: String::new(),
                })
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

    use crate::http::{Body, Method};
    use crate::job::{EncodedImage, JobRequest};
    use crate::model_spec::ProviderId;
    use crate::providers::{ImageInput, Provider};
    use crate::scene::{BoxKind, ImageRef, Scene};
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

    fn request(model: &str, prompt: &str, params: Value, images: Vec<EncodedImage>) -> JobRequest {
        let params = params.as_object().unwrap().clone().into_iter().collect();
        JobRequest {
            model_id: model.into(),
            compiled_prompt: prompt.into(),
            params,
            images,
            box_snapshot: Vec::new(),
            created_at: "2026-10-17T00:00:00Z".into(),
            conversation: None,
        }
    }

    fn inputs(images: &[EncodedImage]) -> Vec<ImageInput> {
        images
            .iter()
            .map(|image| ImageInput::RawBase64(image.base64()))
            .collect()
    }

    fn posted(
        model: &str,
        prompt: &str,
        params: Value,
        images: Vec<EncodedImage>,
    ) -> (String, Value) {
        let provider = crate::providers::resolve_model(model).unwrap();
        let request = request(model, prompt, params, images.clone());
        let http = provider.build_submit(&request, &inputs(&images));
        assert_eq!(http.method, Method::Post);
        assert_eq!(http.auth, Some(ProviderId::OpenAi));
        assert!(!request.params.contains_key("images"));
        let Body::Json(json) = http.body else {
            panic!("expected json");
        };
        (http.url, json)
    }

    #[test]
    fn generations_body_omits_edit_only_fields() {
        let (url, json) = posted(
            "gpt-image-2.5-flare",
            "a harbor",
            json!({
                "quality": "auto",
                "size": "1024x1024",
                "background": "transparent"
            }),
            Vec::new(),
        );
        assert_eq!(url, "https://api.openai.com/v1/images/generations");
        assert_eq!(json["model"], "gpt-image-2.5-flare");
        assert_eq!(json["prompt"], "a harbor");
        assert_eq!(json["quality"], "auto");
        assert_eq!(json["size"], "1024x1024");
        assert_eq!(json["background"], "transparent");
        assert_eq!(json["output_format"], "png");
        for key in [
            "input_fidelity",
            "n",
            "stream",
            "mask",
            "moderation",
            "output_compression",
            "response_format",
            "style",
            "safety_identifier",
            "user",
            "images",
        ] {
            assert!(json.get(key).is_none(), "{key}");
        }
    }

    #[test]
    fn edits_put_references_on_the_body_and_keep_them_out_of_params() {
        let first = encoded(b"one");
        let second = encoded(b"two");
        let first_url = super::data_url("image/png", &first.base64());
        let second_url = super::data_url("image/png", &second.base64());
        let (url, json) = posted(
            "gpt-image-1.5",
            "make it blue",
            json!({
                "quality": "auto",
                "size": "1024x1024",
                "background": "auto",
                "input_fidelity": "high"
            }),
            vec![first, second],
        );
        assert_eq!(url, "https://api.openai.com/v1/images/edits");
        assert_eq!(json["input_fidelity"], "high");
        assert_eq!(json["output_format"], "png");
        assert_eq!(json["images"][0]["image_url"], first_url);
        assert_eq!(json["images"][1]["image_url"], second_url);
        assert!(json.get("mask").is_none());
    }

    #[test]
    fn canvas_data_url_is_first() {
        let canvas = encoded(b"canvas");
        let reference = encoded(b"ref");
        let canvas_url = super::data_url("image/png", &canvas.base64());
        let (_, json) = posted(
            "gpt-image-2",
            "edit",
            json!({
                "quality": "high",
                "size": "1536x1024",
                "background": "opaque",
                "input_fidelity": "high"
            }),
            vec![canvas, reference],
        );
        assert_eq!(json["images"][0]["image_url"], canvas_url);
        assert!(json.get("input_fidelity").is_none());
    }

    #[test]
    fn mini_edit_sends_low_after_reconcile() {
        let mini = spec("gpt-image-1-mini");
        let mut stored = spec("gpt-image-1.5").schema.defaults();
        stored.insert("input_fidelity".into(), json!("high"));
        let mut values = mini.schema.defaults();
        for (key, value) in stored {
            if mini.schema.accepts(&key, &value) {
                values.insert(key, value);
            }
        }
        assert_eq!(values.get("input_fidelity"), Some(&json!("low")));
        let image = encoded(b"mini");
        let params = Value::Object(values.into_iter().collect());
        let (_, json) = posted("gpt-image-1-mini", "edit", params, vec![image]);
        assert_eq!(json["input_fidelity"], "low");
    }

    #[test]
    fn flare_compile_leaves_boxes_out_of_the_prompt() {
        let mut scene = Scene::empty();
        scene.set_source(ImageRef::new("canvas.png", 1024, 1024));
        scene.prose = "a harbor".into();
        scene.add_box(
            crate::geometry::Rect {
                x: 10,
                y: 20,
                w: 30,
                h: 40,
            },
            BoxKind::Layout,
        );
        let compiled =
            GptImage25Flare.compile_prompt(&scene, crate::geometry::Size { w: 1024, h: 1024 });
        assert_eq!(compiled.text, "a harbor");
        assert!(compiled.rows_json.is_empty());
        assert!(!compiled.text.contains("[10"));
    }

    #[test]
    fn caps_name_the_limits_and_the_image_line_is_not_a_partial_send() {
        let prompt = "a".repeat(super::PROMPT_CAP + 1);
        let messages = super::refusal_messages(&prompt, &[], 0);
        assert!(messages.iter().any(|message| message.contains("32000")));

        let url = "x".repeat(super::DATA_URL_CAP + 1);
        let messages = super::refusal_messages("ok", std::slice::from_ref(&url), 1);
        assert!(messages.iter().any(|message| message.contains("20971520")));

        let messages = super::refusal_messages("ok", &[], 17);
        assert_eq!(messages.len(), 1);
        assert!(messages[0].contains("16"));
        assert!(!messages[0].contains("will be sent"));
    }
}
