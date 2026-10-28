use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::error::{FieldError, ProviderError};
use crate::http::{Body, HttpHeader, HttpRequest, HttpResponse, Method};
use crate::job::{EncodedImage, JobRequest};
use crate::model_spec::{AuthScheme, Capabilities, ImageSlots, ModelSpec, ProviderId, SizeControl};
use crate::providers::bfl::protocol::extract_result_mime;
use crate::providers::{ImageInput, InlineImage, PollOutcome, Provider, ReferenceStep, Submitted};
use crate::scene::Scene;
use crate::schema::{FieldDef, FieldKind, ParamSchema, RuleContext};

/// Muse Image 1.0.
#[derive(Debug, Clone, Copy)]
pub struct MuseImage;

const GENERATIONS: &str = "https://api.meta.ai/v1/images/generations";

const EDITS: &str = "https://api.meta.ai/v1/images/edits";

const RESPONSES: &str = "https://api.meta.ai/v1/responses";

const SIZES: &[&str] = &[
    "auto",
    "1024x1024",
    "1536x1024",
    "1024x1536",
    "1792x1024",
    "1024x1792",
];

const OUTPUT_FORMATS: &[&str] = &["webp", "png", "jpeg"];

const REASONING: &[&str] = &["high", "low"];

pub const IMAGE_CAP: usize = 10;

static SPEC: std::sync::LazyLock<ModelSpec> = std::sync::LazyLock::new(spec);

impl MuseImage {
    #[must_use]
    pub fn model_spec() -> &'static ModelSpec {
        &SPEC
    }
}

impl Provider for MuseImage {
    fn spec(&self) -> &ModelSpec {
        &SPEC
    }

    fn supports_conversation(&self) -> bool {
        true
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
        build_muse(&SPEC, request, images)
    }

    fn poll_request(&self, submitted: &Submitted) -> HttpRequest {
        HttpRequest {
            method: Method::Get,
            url: submitted.poll_url.to_string(),
            headers: Vec::new(),
            auth: Some(ProviderId::Meta),
            body: Body::Empty,
            max_body_bytes: 1024,
            redact_query: false,
        }
    }

    fn parse_submit(&self, response: &HttpResponse) -> Result<Submitted, ProviderError> {
        match muse(response) {
            Ok(_) => Err(ProviderError::Protocol(
                "muse returns the image on the submit response".into(),
            )),
            Err(error) => Err(error),
        }
    }

    fn inline_image(&self, response: &HttpResponse) -> Option<InlineImage> {
        muse(response).ok()
    }

    fn parse_poll(&self, _response: &HttpResponse) -> PollOutcome {
        PollOutcome::Failed(ProviderError::Protocol(
            "muse returns the image on the submit response".into(),
        ))
    }
}

fn spec() -> ModelSpec {
    ModelSpec {
        id: "muse-image-1.0".into(),
        display_name: "Muse Image 1.0".into(),
        provider: ProviderId::Meta,
        auth: AuthScheme::Bearer,
        endpoint: GENERATIONS.into(),
        beta: false,
        capabilities: Capabilities {
            supports_boxes: false,
            image_slots: ImageSlots::List {
                max: IMAGE_CAP,
                key: "images".into(),
            },
            size_control: SizeControl::AspectRatioAndResolution,
        },
        schema: ParamSchema {
            fields: vec![
                select("size", "Size", SIZES, "auto"),
                select("output_format", "Output format", OUTPUT_FORMATS, "webp"),
                select(
                    "reasoning_strength",
                    "Reasoning strength",
                    REASONING,
                    "high",
                ),
                switch("enable_image_search", "Image search"),
                switch("enable_web_search", "Web search"),
                switch("enable_shell", "Code execution"),
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

fn switch(key: &str, label: &str) -> FieldDef {
    FieldDef {
        key: key.into(),
        label: label.into(),
        help: None,
        kind: FieldKind::Bool,
        required: true,
        default: Some(json!(true)),
        locked_when: None,
    }
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

fn field_bool(spec: &ModelSpec, params: &BTreeMap<String, Value>, key: &str) -> bool {
    if let Some(value) = params.get(key)
        && spec.schema.accepts(key, value)
        && let Some(flag) = value.as_bool()
    {
        return flag;
    }
    spec.schema
        .fields
        .iter()
        .find(|field| field.key == key)
        .and_then(|field| field.default.as_ref())
        .and_then(Value::as_bool)
        .unwrap_or(true)
}

fn data_url(media_type: &str, encoded: &str) -> String {
    format!("data:{media_type};base64,{encoded}")
}

fn image_url(request: &JobRequest, images: &[ImageInput], index: usize) -> Value {
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
    json!({ "image_url": url })
}

fn build_muse(spec: &ModelSpec, request: &JobRequest, images: &[ImageInput]) -> HttpRequest {
    if request.conversation.is_some() {
        return build_responses(spec, request, images);
    }
    let count = images.len().min(spec.capabilities.image_slots.max());
    let mut body = Map::new();
    body.insert("model".into(), Value::String(request.model_id.clone()));
    body.insert(
        "prompt".into(),
        Value::String(request.compiled_prompt.clone()),
    );
    body.insert("response_format".into(), Value::String("b64_json".into()));
    if let Some(format) = field_string(spec, &request.params, "output_format") {
        body.insert("output_format".into(), Value::String(format));
    }
    if let Some(strength) = field_string(spec, &request.params, "reasoning_strength") {
        body.insert("reasoning_strength".into(), Value::String(strength));
    }
    if let Some(size) = field_string(spec, &request.params, "size")
        && size != "auto"
    {
        body.insert("size".into(), Value::String(size));
    }
    body.insert(
        "tool_enablement".into(),
        json!({
            "enable_image_search": field_bool(spec, &request.params, "enable_image_search"),
            "enable_web_search": field_bool(spec, &request.params, "enable_web_search"),
            "enable_shell": field_bool(spec, &request.params, "enable_shell"),
        }),
    );
    if count > 0 {
        let list = (0..count)
            .map(|index| image_url(request, images, index))
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
        auth: Some(ProviderId::Meta),
        body: Body::Json(Value::Object(body)),
        max_body_bytes: 100 * 1024 * 1024,
        redact_query: false,
    }
}

fn build_responses(spec: &ModelSpec, request: &JobRequest, images: &[ImageInput]) -> HttpRequest {
    let count = images.len().min(spec.capabilities.image_slots.max());
    let mut content = vec![json!({
        "type": "input_text",
        "text": request.compiled_prompt,
    })];
    for index in 0..count {
        content.push(json!({
            "type": "input_image",
            "image_url": image_url(request, images, index)["image_url"],
        }));
    }
    let mut tool = Map::new();
    tool.insert("type".into(), Value::String("image_generation".into()));
    for key in ["enable_image_search", "enable_web_search", "enable_shell"] {
        tool.insert(
            key.into(),
            Value::Bool(field_bool(spec, &request.params, key)),
        );
    }
    if let Some(strength) = field_string(spec, &request.params, "reasoning_strength") {
        tool.insert("reasoning_strength".into(), Value::String(strength));
    }
    if let Some(format) = field_string(spec, &request.params, "output_format") {
        tool.insert("output_format".into(), Value::String(format));
    }
    if let Some(size) = field_string(spec, &request.params, "size")
        && size != "auto"
    {
        tool.insert("size".into(), Value::String(size));
    }
    let mut body = Map::new();
    body.insert("model".into(), Value::String(request.model_id.clone()));
    body.insert("store".into(), Value::Bool(true));
    body.insert(
        "input".into(),
        json!([{ "role": "user", "content": content }]),
    );
    if let Some(previous) = request
        .conversation
        .as_ref()
        .and_then(|turn| turn.parent_provider_id.clone())
    {
        body.insert("previous_response_id".into(), Value::String(previous));
    }
    body.insert("tools".into(), Value::Array(vec![Value::Object(tool)]));
    HttpRequest {
        method: Method::Post,
        url: RESPONSES.into(),
        headers: vec![HttpHeader {
            name: "Content-Type".into(),
            value: "application/json".into(),
            sensitive: false,
        }],
        auth: Some(ProviderId::Meta),
        body: Body::Json(Value::Object(body)),
        max_body_bytes: 100 * 1024 * 1024,
        redact_query: false,
    }
}

fn muse(response: &HttpResponse) -> Result<InlineImage, ProviderError> {
    if response.status == 401 {
        return Err(ProviderError::Auth);
    }
    if response.status == 429 {
        return Err(ProviderError::RateLimited { retry_after: None });
    }
    let value: Value = serde_json::from_slice(&response.body).unwrap_or(Value::Null);
    if matches!(response.status, 400 | 404) && names_previous_response(&value) {
        return Err(ProviderError::ConversationEnded);
    }
    if response.status == 400 && error_code(&value) == "content_policy_violation" {
        return Err(ProviderError::Blocked);
    }
    if response.status != 200 {
        return Err(ProviderError::Http {
            status: response.status,
            body: String::from_utf8_lossy(&response.body).into_owned(),
        });
    }
    if value.get("output").is_some() {
        return decode_responses(&value);
    }
    let Some(items) = value.get("data").and_then(Value::as_array) else {
        return Err(ProviderError::Protocol("missing data".into()));
    };
    if items.is_empty() {
        return Err(ProviderError::Blocked);
    }
    let item = &items[0];
    match item.get("b64_json") {
        Some(Value::String(encoded)) if !encoded.is_empty() => decode_image(&value, encoded),
        _ => Err(ProviderError::NoImage),
    }
}

fn names_previous_response(value: &Value) -> bool {
    let error = value.get("error");
    let message = error
        .and_then(|error| error.get("message"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let param = error
        .and_then(|error| error.get("param"))
        .and_then(Value::as_str)
        .unwrap_or("");
    message.contains("previous_response_id") || param.contains("previous_response_id")
}

fn decode_responses(value: &Value) -> Result<InlineImage, ProviderError> {
    let items = value
        .get("output")
        .and_then(Value::as_array)
        .ok_or_else(|| ProviderError::NoImage)?;
    let call = items.iter().find(|item| {
        item.get("type").and_then(Value::as_str) == Some("image_generation_call")
    });
    let Some(encoded) = call
        .and_then(|item| item.get("result"))
        .and_then(Value::as_str)
        .filter(|encoded| !encoded.is_empty())
    else {
        return Err(ProviderError::NoImage);
    };
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
        .map_err(|_| ProviderError::Protocol("invalid image".into()))?;
    let mime = mime_type(value, &bytes);
    let interaction_id = value
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    Ok(InlineImage {
        bytes,
        mime,
        interaction_id,
        suggestion: reasoning_summary(items),
    })
}

fn reasoning_summary(items: &[Value]) -> String {
    items
        .iter()
        .filter(|item| item.get("type").and_then(Value::as_str) == Some("reasoning"))
        .filter_map(|item| item.get("summary").and_then(Value::as_array))
        .flatten()
        .filter_map(|part| part.get("text").and_then(Value::as_str))
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

fn error_code(value: &Value) -> &str {
    value
        .get("error")
        .and_then(|error| error.get("code"))
        .and_then(Value::as_str)
        .unwrap_or("")
}

fn decode_image(root: &Value, encoded: &str) -> Result<InlineImage, ProviderError> {
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, encoded)
        .map_err(|_| ProviderError::Protocol("invalid image".into()))?;
    let mime = mime_type(root, &bytes);
    Ok(InlineImage {
        bytes,
        mime,
        interaction_id: String::new(),
        suggestion: String::new(),
    })
}

fn mime_type(root: &Value, bytes: &[u8]) -> String {
    let hinted = match root.get("output_format").and_then(Value::as_str) {
        Some("png") => Some("image/png"),
        Some("jpeg") => Some("image/jpeg"),
        Some("webp") => Some("image/webp"),
        _ => None,
    };
    extract_result_mime(hinted, bytes)
        .unwrap_or("image/webp")
        .to_owned()
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::{Value, json};

    use crate::http::{Body, Method};
    use crate::job::{EncodedImage, JobRequest};
    use crate::model_spec::ProviderId;
    use crate::providers::{ImageInput, ReferenceStep};

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
            model_id: "muse-image-1.0".into(),
            compiled_prompt: prompt.into(),
            params,
            images,
            box_snapshot: Vec::new(),
            created_at: "2026-10-27T00:00:00Z".into(),
            conversation: None,
        }
    }

    fn inputs(images: &[EncodedImage]) -> Vec<ImageInput> {
        images
            .iter()
            .map(|image| ImageInput::RawBase64(image.base64()))
            .collect()
    }

    fn posted(prompt: &str, params: Value, images: Vec<EncodedImage>) -> (String, Value) {
        let provider = crate::providers::resolve_model("muse-image-1.0").unwrap();
        let request = request(prompt, params, images.clone());
        let http = provider.build_submit(&request, &inputs(&images));
        assert_eq!(http.method, Method::Post);
        assert_eq!(http.auth, Some(ProviderId::Meta));
        let Body::Json(json) = http.body else {
            panic!("expected json");
        };
        (http.url, json)
    }

    fn defaults() -> Value {
        json!({
            "size": "auto",
            "output_format": "webp",
            "reasoning_strength": "high",
            "enable_image_search": true,
            "enable_web_search": true,
            "enable_shell": true
        })
    }

    fn top_keys(json: &Value) -> BTreeSet<&str> {
        json.as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect()
    }

    fn assert_image_item(item: &Value, uri: &str) {
        let map = item.as_object().unwrap();
        assert_eq!(map.len(), 1);
        assert_eq!(item["image_url"], uri);
    }

    #[test]
    fn zero_images_post_generations_without_a_size() {
        let (url, json) = posted("a harbor", defaults(), Vec::new());
        assert_eq!(url, "https://api.meta.ai/v1/images/generations");
        assert_eq!(json["model"], "muse-image-1.0");
        assert_eq!(json["prompt"], "a harbor");
        assert_eq!(json["response_format"], "b64_json");
        assert_eq!(json["output_format"], "webp");
        assert_eq!(json["reasoning_strength"], "high");
        assert_eq!(
            json["tool_enablement"],
            json!({
                "enable_image_search": true,
                "enable_web_search": true,
                "enable_shell": true
            })
        );
        assert_eq!(
            top_keys(&json),
            BTreeSet::from([
                "model",
                "output_format",
                "prompt",
                "reasoning_strength",
                "response_format",
                "tool_enablement",
            ])
        );
    }

    #[test]
    fn one_image_posts_an_edit_with_a_single_image_url() {
        let image = encoded(b"canvas");
        let (url, json) = posted("make it blue", defaults(), vec![image.clone()]);
        assert_eq!(url, "https://api.meta.ai/v1/images/edits");
        let images = json["images"].as_array().unwrap();
        assert_eq!(images.len(), 1);
        assert_image_item(&images[0], &data_uri(&image));
        assert!(json.get("image").is_none());
        assert!(json.get("size").is_none());
    }

    #[test]
    fn three_images_post_the_canvas_first() {
        let canvas = encoded(b"canvas");
        let left = encoded(b"left");
        let right = encoded(b"right");
        let (url, json) = posted(
            "combine them",
            defaults(),
            vec![canvas.clone(), left, right],
        );
        assert_eq!(url, "https://api.meta.ai/v1/images/edits");
        let images = json["images"].as_array().unwrap();
        assert_eq!(images.len(), 3);
        assert_image_item(&images[0], &data_uri(&canvas));
    }

    #[test]
    fn eleven_prepared_images_keep_ten() {
        let images: Vec<_> = (0..11).map(|n| encoded(&[n])).collect();
        let provider = crate::providers::resolve_model("muse-image-1.0").unwrap();
        let steps = provider.reference_steps(&images);
        assert_eq!(steps.len(), 10);
        match &steps[0] {
            ReferenceStep::Inline(ImageInput::RawBase64(encoded)) => {
                assert_eq!(encoded, &images[0].base64());
            }
            ReferenceStep::Inline(ImageInput::Url(_)) | ReferenceStep::Upload { .. } => {
                panic!("expected inline base64");
            }
        }
        let (url, json) = posted("eleven", defaults(), images.clone());
        assert_eq!(url, "https://api.meta.ai/v1/images/edits");
        let list = json["images"].as_array().unwrap();
        assert_eq!(list.len(), 10);
        assert_image_item(&list[0], &data_uri(&images[0]));
        assert_image_item(&list[9], &data_uri(&images[9]));
    }

    #[test]
    fn size_is_sent_unless_it_is_auto() {
        let sized = json!({
            "size": "1536x1024",
            "output_format": "webp",
            "reasoning_strength": "high",
            "enable_image_search": true,
            "enable_web_search": true,
            "enable_shell": true
        });
        let (url, generated) = posted("gen", sized.clone(), Vec::new());
        assert_eq!(url, "https://api.meta.ai/v1/images/generations");
        assert_eq!(generated["size"], "1536x1024");
        let image = encoded(b"one");
        let (url, edited) = posted("edit", sized, vec![image]);
        assert_eq!(url, "https://api.meta.ai/v1/images/edits");
        assert_eq!(edited["size"], "1536x1024");

        let (url, generated) = posted("gen", defaults(), Vec::new());
        assert_eq!(url, "https://api.meta.ai/v1/images/generations");
        assert!(generated.get("size").is_none());
        let image = encoded(b"one");
        let (url, edited) = posted("edit", defaults(), vec![image]);
        assert_eq!(url, "https://api.meta.ai/v1/images/edits");
        assert!(edited.get("size").is_none());
    }

    #[test]
    fn grounding_switches_copy_into_tool_enablement() {
        let params = json!({
            "size": "auto",
            "output_format": "webp",
            "reasoning_strength": "high",
            "enable_image_search": true,
            "enable_web_search": false,
            "enable_shell": false
        });
        let (_, json) = posted("search", params, Vec::new());
        assert_eq!(
            json["tool_enablement"],
            json!({
                "enable_image_search": true,
                "enable_web_search": false,
                "enable_shell": false
            })
        );
    }

    #[test]
    fn output_format_and_reasoning_copy_onto_both_urls() {
        let params = json!({
            "size": "auto",
            "output_format": "png",
            "reasoning_strength": "low",
            "enable_image_search": true,
            "enable_web_search": true,
            "enable_shell": true
        });
        let (url, generated) = posted("gen", params.clone(), Vec::new());
        assert_eq!(url, "https://api.meta.ai/v1/images/generations");
        assert_eq!(generated["output_format"], "png");
        assert_eq!(generated["reasoning_strength"], "low");
        let image = encoded(b"one");
        let (url, edited) = posted("edit", params, vec![image]);
        assert_eq!(url, "https://api.meta.ai/v1/images/edits");
        assert_eq!(edited["output_format"], "png");
        assert_eq!(edited["reasoning_strength"], "low");
    }

    fn conversation_post(request: JobRequest) -> (String, Value) {
        let provider = crate::providers::resolve_model("muse-image-1.0").unwrap();
        let images = inputs(&request.images);
        let http = provider.build_submit(&request, &images);
        let Body::Json(json) = http.body else {
            panic!("expected json");
        };
        (http.url, json)
    }

    #[test]
    fn a_first_conversation_turn_posts_responses() {
        use crate::job::Conversation;
        let mut request = request("a harbor", defaults(), Vec::new());
        request.conversation = Some(Conversation {
            parent_job_id: None,
            parent_provider_id: None,
        });
        let (url, json) = conversation_post(request);
        assert_eq!(url, "https://api.meta.ai/v1/responses");
        assert_eq!(json["store"], json!(true));
        assert!(json.get("previous_response_id").is_none());
        assert_eq!(json["input"][0]["role"], json!("user"));
        assert_eq!(json["input"][0]["content"][0]["type"], json!("input_text"));
        assert_eq!(json["input"][0]["content"][0]["text"], json!("a harbor"));
        assert!(json.get("tool_enablement").is_none());
        assert!(json.get("response_format").is_none());
        assert!(json.get("n").is_none());
        let tool = &json["tools"][0];
        assert_eq!(tool["type"], json!("image_generation"));
        assert_eq!(tool["reasoning_strength"], json!("high"));
        assert_eq!(tool["output_format"], json!("webp"));
        assert!(tool.get("size").is_none());
        assert_eq!(json["tools"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn references_and_a_continued_turn_shape_the_responses_body() {
        use crate::job::Conversation;
        let canvas = encoded(b"canvas");
        let extra = encoded(b"extra");
        let mut first = request("combine", defaults(), vec![canvas.clone(), extra.clone()]);
        first.conversation = Some(Conversation {
            parent_job_id: None,
            parent_provider_id: None,
        });
        let (_, json) = conversation_post(first);
        let content = json["input"][0]["content"].as_array().unwrap();
        assert_eq!(content.len(), 3);
        assert_eq!(content[1]["type"], json!("input_image"));
        assert_eq!(content[1]["image_url"], json!(data_uri(&canvas)));
        assert_eq!(content[2]["image_url"], json!(data_uri(&extra)));

        let mut continued = request("warmer", defaults(), Vec::new());
        continued.conversation = Some(Conversation {
            parent_job_id: Some("turn-1".into()),
            parent_provider_id: Some("resp_1".into()),
        });
        continued.params.insert("size".into(), json!("1536x1024"));
        let (_, json) = conversation_post(continued);
        assert_eq!(json["previous_response_id"], json!("resp_1"));
        assert_eq!(json["input"][0]["content"].as_array().unwrap().len(), 1);
        assert_eq!(json["tools"][0]["size"], json!("1536x1024"));
    }

    #[test]
    fn a_responses_body_decodes_the_image_call_and_the_summary() {
        use crate::http::HttpResponse;
        use crate::providers::Provider;
        let png = b"\x89PNG\r\n\x1a\nrest";
        let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, png);
        let body = format!(
            r#"{{"id":"resp_abc","output":[{{"type":"message","content":[]}},{{"type":"image_generation_call","result":"{encoded}"}},{{"type":"reasoning","summary":[{{"text":"looked up the harbor"}},{{"text":"kept the boats"}}]}}]}}"#
        );
        let response = HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: body.into_bytes(),
        };
        let image = super::MuseImage.inline_image(&response).unwrap();
        assert_eq!(image.bytes, png);
        assert_eq!(image.interaction_id, "resp_abc");
        assert_eq!(image.suggestion, "looked up the harbor\nkept the boats");
        assert_eq!(image.mime, "image/png");

        let missing = HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: br#"{"id":"resp_none","output":[{"type":"reasoning","summary":[]}]}"#.to_vec(),
        };
        assert_eq!(
            super::MuseImage.parse_submit(&missing).unwrap_err(),
            crate::error::ProviderError::NoImage
        );
        let gone = HttpResponse {
            status: 400,
            headers: Vec::new(),
            body: br#"{"error":{"code":"invalid_request","message":"unknown previous_response_id","param":"previous_response_id"}}"#.to_vec(),
        };
        assert_eq!(
            super::MuseImage.parse_submit(&gone).unwrap_err(),
            crate::error::ProviderError::ConversationEnded
        );
    }

    #[test]
    fn flux_3_still_resolves_to_black_forest_labs() {
        let provider = crate::providers::resolve_model("flux-3-image").unwrap();
        assert_eq!(provider.spec().id, "flux-3-image");
        assert_eq!(provider.spec().provider, ProviderId::Bfl);
        assert_eq!(provider.spec().provider.display_name(), "Black Forest Labs");
    }
}
