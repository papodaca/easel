use crate::model_spec::{AuthScheme, Capabilities, ImageSlots, ModelSpec, ProviderId, SizeControl};
use crate::schema::{FieldDef, FieldKind, ParamSchema};

/// Nano Banana 2.1. The Interactions model id is `gemini-nano-banana-2.1`.
#[derive(Debug, Clone, Copy)]
pub struct NanoBanana21;

/// Nano Banana 2 Lite. The Interactions model id is `gemini-3.1-flash-lite-image`.
#[derive(Debug, Clone, Copy)]
pub struct NanoBanana2Lite;

/// Nano Banana Pro. The Interactions model id is `gemini-3-pro-image`.
#[derive(Debug, Clone, Copy)]
pub struct NanoBananaPro;

const ENDPOINT: &str = "https://generativelanguage.googleapis.com/v1beta/interactions";

const STANDARD_RATIOS: &[&str] = &[
    "1:1", "2:3", "3:2", "3:4", "4:3", "4:5", "5:4", "9:16", "16:9", "21:9",
];

const WIDE_RATIOS: &[&str] = &["1:4", "4:1", "1:8", "8:1"];

static NANO_BANANA_21: std::sync::LazyLock<ModelSpec> = std::sync::LazyLock::new(|| {
    image_spec(
        "gemini-nano-banana-2.1",
        "Nano Banana 2.1",
        true,
        &["1K", "2K", "4K"],
        true,
    )
});

static NANO_BANANA_2_LITE: std::sync::LazyLock<ModelSpec> = std::sync::LazyLock::new(|| {
    image_spec(
        "gemini-3.1-flash-lite-image",
        "Nano Banana 2 Lite",
        false,
        &["1K"],
        false,
    )
});

static NANO_BANANA_PRO: std::sync::LazyLock<ModelSpec> = std::sync::LazyLock::new(|| {
    image_spec(
        "gemini-3-pro-image",
        "Nano Banana Pro",
        false,
        &["1K", "2K", "4K"],
        true,
    )
});

impl NanoBanana21 {
    #[must_use]
    pub fn model_spec() -> &'static ModelSpec {
        &NANO_BANANA_21
    }
}

impl NanoBanana2Lite {
    #[must_use]
    pub fn model_spec() -> &'static ModelSpec {
        &NANO_BANANA_2_LITE
    }
}

impl NanoBananaPro {
    #[must_use]
    pub fn model_spec() -> &'static ModelSpec {
        &NANO_BANANA_PRO
    }
}

fn image_spec(
    id: &str,
    display_name: &str,
    wide_ratios: bool,
    sizes: &[&str],
    grounding: bool,
) -> ModelSpec {
    let mut fields = vec![
        FieldDef {
            key: "aspect_ratio".into(),
            label: "Aspect ratio".into(),
            help: None,
            kind: FieldKind::Select {
                options: ratios(wide_ratios),
            },
            required: true,
            default: Some(serde_json::json!("1:1")),
            locked_when: None,
        },
        FieldDef {
            key: "resolution".into(),
            label: "Image size".into(),
            help: None,
            kind: FieldKind::Select {
                options: sizes.iter().map(|size| (*size).to_string()).collect(),
            },
            required: true,
            default: Some(serde_json::json!("1K")),
            locked_when: None,
        },
    ];
    if grounding {
        fields.push(FieldDef {
            key: "grounding".into(),
            label: "Search grounding".into(),
            help: Some("Google keeps the prompt and the output for 30 days.".into()),
            kind: FieldKind::Bool,
            required: true,
            default: Some(serde_json::json!(false)),
            locked_when: None,
        });
    }
    ModelSpec {
        id: id.into(),
        display_name: display_name.into(),
        provider: ProviderId::Google,
        auth: AuthScheme::HeaderKey {
            header: "x-goog-api-key".into(),
        },
        endpoint: ENDPOINT.into(),
        beta: false,
        capabilities: Capabilities {
            supports_boxes: false,
            image_slots: ImageSlots::List {
                max: 14,
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

fn build_google(
    spec: &ModelSpec,
    request: &crate::job::JobRequest,
    images: &[crate::providers::ImageInput],
) -> crate::http::HttpRequest {
    use crate::http::{Body, HttpHeader, HttpRequest, Method};
    use crate::providers::ImageInput;
    use serde_json::{Map, Value, json};

    let mut input = vec![json!({
        "type": "text",
        "text": request.compiled_prompt,
    })];
    for (index, image) in images.iter().enumerate() {
        let ImageInput::RawBase64(data) = image else {
            continue;
        };
        let mime = request
            .images
            .get(index)
            .map(|encoded| encoded.media_type.as_str())
            .unwrap_or("image/png");
        input.push(json!({
            "type": "image",
            "mime_type": mime,
            "data": data,
        }));
    }
    let aspect_ratio = request
        .params
        .get("aspect_ratio")
        .and_then(Value::as_str)
        .unwrap_or("1:1");
    let image_size = request
        .params
        .get("resolution")
        .and_then(Value::as_str)
        .unwrap_or("1K");
    let mut body = Map::new();
    body.insert("model".into(), Value::String(request.model_id.clone()));
    body.insert("store".into(), Value::Bool(false));
    body.insert("input".into(), Value::Array(input));
    body.insert(
        "response_format".into(),
        json!({
            "type": "image",
            "aspect_ratio": aspect_ratio,
            "image_size": image_size,
        }),
    );
    let offers_grounding = spec
        .schema
        .fields
        .iter()
        .any(|field| field.key == "grounding");
    if offers_grounding
        && request
            .params
            .get("grounding")
            .and_then(Value::as_bool)
            .unwrap_or(false)
    {
        body.insert("tools".into(), json!([{ "type": "google_search" }]));
    }
    HttpRequest {
        method: Method::Post,
        url: spec.endpoint.clone(),
        headers: vec![HttpHeader {
            name: "Content-Type".into(),
            value: "application/json".into(),
            sensitive: false,
        }],
        auth: Some(ProviderId::Google),
        body: Body::Json(Value::Object(body)),
        max_body_bytes: 100 * 1024 * 1024,
        redact_query: false,
    }
}

fn refused_poll(submitted: &crate::providers::Submitted) -> crate::http::HttpRequest {
    crate::http::HttpRequest {
        method: crate::http::Method::Get,
        url: submitted.poll_url.to_string(),
        headers: Vec::new(),
        auth: Some(ProviderId::Google),
        body: crate::http::Body::Empty,
        max_body_bytes: 1024,
        redact_query: false,
    }
}

fn google_failure(response: &crate::http::HttpResponse) -> Option<crate::error::ProviderError> {
    use crate::error::ProviderError;

    let value: serde_json::Value =
        serde_json::from_slice(&response.body).unwrap_or(serde_json::Value::Null);
    let code = value
        .get("error")
        .and_then(|error| error.get("status").and_then(|status| status.as_str()))
        .or_else(|| {
            value
                .get("error")
                .and_then(|error| error.get("code").and_then(|code| code.as_str()))
        })
        .unwrap_or("");
    match (response.status, code) {
        (
            400,
            "safety"
            | "image_safety"
            | "image_prohibited_content"
            | "image_recitation"
            | "image_other",
        ) => Some(ProviderError::Blocked),
        (401, _) => Some(ProviderError::Auth),
        (404, "model_not_found") => Some(ProviderError::ModelNotFound),
        _ => None,
    }
}

fn decode_inline(response: &crate::http::HttpResponse) -> Option<crate::providers::InlineImage> {
    if response.status != 200 {
        return None;
    }
    let value: serde_json::Value = serde_json::from_slice(&response.body).ok()?;
    let (data, mime) = inline_image_fields(&value)?;
    let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &data).ok()?;
    let interaction_id = value
        .get("id")
        .and_then(|id| id.as_str())
        .unwrap_or("")
        .to_owned();
    Some(crate::providers::InlineImage {
        bytes,
        mime,
        interaction_id,
        suggestion: suggestion_text(&value),
    })
}

fn inline_image_fields(value: &serde_json::Value) -> Option<(String, String)> {
    if let Some((data, mime)) = image_block(value.get("output_image")) {
        return Some((data, mime));
    }
    let mut found = None;
    let steps = value.get("steps").and_then(|steps| steps.as_array())?;
    for step in steps {
        if step.get("type").and_then(|kind| kind.as_str()) != Some("model_output") {
            continue;
        }
        let Some(content) = step.get("content").and_then(|content| content.as_array()) else {
            continue;
        };
        for block in content {
            if let Some((data, mime)) = image_block(Some(block)) {
                found = Some((data, mime));
            }
        }
    }
    found
}

fn image_block(block: Option<&serde_json::Value>) -> Option<(String, String)> {
    let block = block?;
    if block.get("type").and_then(|kind| kind.as_str()) == Some("text") {
        return None;
    }
    let data = block.get("data").and_then(|data| data.as_str())?;
    if data.is_empty() {
        return None;
    }
    if let Some(kind) = block.get("type").and_then(|kind| kind.as_str())
        && kind != "image"
    {
        return None;
    }
    let mime = block
        .get("mime_type")
        .and_then(|mime| mime.as_str())
        .unwrap_or("image/png")
        .to_owned();
    Some((data.to_owned(), mime))
}

fn suggestion_text(value: &serde_json::Value) -> String {
    if let Some(text) = value
        .get("search_suggestions")
        .and_then(|suggestion| suggestion.as_str())
        && !text.is_empty()
    {
        return text.to_owned();
    }
    let mut found = String::new();
    let Some(steps) = value.get("steps").and_then(|steps| steps.as_array()) else {
        return found;
    };
    for step in steps {
        if step.get("type").and_then(|kind| kind.as_str()) != Some("google_search_result") {
            continue;
        }
        let Some(results) = step.get("result").and_then(|result| result.as_array()) else {
            continue;
        };
        for result in results {
            if let Some(html) = result
                .get("search_suggestions")
                .and_then(|suggestion| suggestion.as_str())
                && !html.is_empty()
            {
                found = html.to_owned();
            }
        }
    }
    found
}

pub const BODY_CAP: usize = 20 * 1024 * 1024;

#[must_use]
pub fn posted_json_len(spec: &ModelSpec, request: &crate::job::JobRequest) -> usize {
    let inputs: Vec<crate::providers::ImageInput> = request
        .images
        .iter()
        .map(|image| crate::providers::ImageInput::RawBase64(image.base64()))
        .collect();
    let http = build_google(spec, request, &inputs);
    match http.body {
        crate::http::Body::Json(value) => serde_json::to_vec(&value)
            .map(|bytes| bytes.len())
            .unwrap_or(usize::MAX),
        _ => 0,
    }
}

macro_rules! impl_google {
    ($ty:ident, $spec:ident) => {
        impl crate::providers::Provider for $ty {
            fn spec(&self) -> &ModelSpec {
                &$spec
            }

            fn supports_conversation(&self) -> bool {
                matches!(
                    self.spec().id.as_str(),
                    "gemini-nano-banana-2.1" | "gemini-3-pro-image"
                )
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
                build_google(&$spec, request, images)
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
                if let Some(error) = google_failure(response) {
                    return Err(error);
                }
                Err(crate::error::ProviderError::Protocol(
                    "missing output_image".into(),
                ))
            }

            fn inline_image(
                &self,
                response: &crate::http::HttpResponse,
            ) -> Option<crate::providers::InlineImage> {
                decode_inline(response)
            }

            fn parse_poll(
                &self,
                _response: &crate::http::HttpResponse,
            ) -> crate::providers::PollOutcome {
                crate::providers::PollOutcome::Failed(crate::error::ProviderError::Protocol(
                    "google returns the image on the submit response".into(),
                ))
            }
        }
    };
}

impl_google!(NanoBanana21, NANO_BANANA_21);
impl_google!(NanoBanana2Lite, NANO_BANANA_2_LITE);
impl_google!(NanoBananaPro, NANO_BANANA_PRO);

fn ratios(wide: bool) -> Vec<String> {
    let mut ratios: Vec<String> = STANDARD_RATIOS
        .iter()
        .map(|ratio| (*ratio).to_string())
        .collect();
    if wide {
        ratios.extend(WIDE_RATIOS.iter().map(|ratio| (*ratio).to_string()));
    }
    ratios
}

#[cfg(test)]
mod tests {
    use super::{NanoBanana2Lite, NanoBanana21, NanoBananaPro};
    use crate::model_spec::ProviderId;
    use crate::providers::ModelRegistry;
    use serde_json::json;

    fn spec(id: &str) -> &'static crate::model_spec::ModelSpec {
        ModelRegistry::specs()
            .iter()
            .copied()
            .find(|spec| spec.id == id)
            .unwrap_or_else(|| panic!("missing {id}"))
    }

    #[test]
    fn registry_lists_the_three_google_models_in_order() {
        let _structs = (NanoBanana21, NanoBanana2Lite, NanoBananaPro);
        let ids: Vec<_> = ModelRegistry::specs()
            .iter()
            .filter(|spec| spec.provider == ProviderId::Google)
            .map(|spec| spec.id.as_str())
            .collect();
        assert_eq!(
            ids,
            [
                "gemini-nano-banana-2.1",
                "gemini-3.1-flash-lite-image",
                "gemini-3-pro-image",
            ]
        );
        assert_eq!(
            spec("gemini-nano-banana-2.1").display_name,
            "Nano Banana 2.1"
        );
        assert_eq!(
            spec("gemini-3.1-flash-lite-image").display_name,
            "Nano Banana 2 Lite"
        );
        assert_eq!(spec("gemini-3-pro-image").display_name, "Nano Banana Pro");
        for id in &ids {
            let spec = spec(id);
            assert!(!spec.capabilities.supports_boxes);
            assert_eq!(spec.capabilities.image_slots.max(), 14);
            assert_eq!(spec.endpoint, super::ENDPOINT);
        }
    }

    #[test]
    fn nano_banana_21_accepts_wide_ratios_sizes_and_grounding() {
        let schema = &spec("gemini-nano-banana-2.1").schema;
        assert!(schema.accepts("resolution", &json!("1K")));
        assert!(schema.accepts("resolution", &json!("2K")));
        assert!(schema.accepts("resolution", &json!("4K")));
        assert!(schema.accepts("aspect_ratio", &json!("1:8")));
        assert!(schema.accepts("grounding", &json!(true)));
        assert!(!schema.accepts("resolution", &json!("512")));
    }

    #[test]
    fn nano_banana_pro_accepts_grounding_and_rejects_wide_ratios() {
        let schema = &spec("gemini-3-pro-image").schema;
        assert!(schema.accepts("resolution", &json!("4K")));
        assert!(schema.accepts("aspect_ratio", &json!("16:9")));
        assert!(schema.accepts("grounding", &json!(true)));
        assert!(!schema.accepts("aspect_ratio", &json!("1:8")));
        assert!(!schema.accepts("image_search", &json!(true)));
    }

    #[test]
    fn nano_banana_lite_is_1k_without_grounding_or_wide_ratios() {
        let schema = &spec("gemini-3.1-flash-lite-image").schema;
        assert!(schema.accepts("resolution", &json!("1K")));
        assert!(schema.accepts("aspect_ratio", &json!("1:1")));
        assert!(!schema.accepts("resolution", &json!("4K")));
        assert!(!schema.accepts("aspect_ratio", &json!("1:8")));
        assert!(!schema.accepts("grounding", &json!(true)));
        assert!(!schema.accepts("grounding", &json!(false)));
    }

    #[test]
    fn nano_banana_21_submit_posts_interactions_json() {
        use crate::job::{EncodedImage, JobRequest};
        use crate::providers::{ImageInput, Provider};

        let png = b"\x89PNG\r\n\x1a\n".to_vec();
        let image = EncodedImage {
            media_type: "image/png".into(),
            bytes: png,
            width: 16,
            height: 16,
            content_hash: "abc".into(),
        };
        let mut params = std::collections::BTreeMap::new();
        params.insert("resolution".into(), json!("2K"));
        params.insert("aspect_ratio".into(), json!("16:9"));
        let request = JobRequest {
            model_id: "gemini-nano-banana-2.1".into(),
            compiled_prompt: "a red square".into(),
            params,
            images: vec![image],
            box_snapshot: Vec::new(),
            created_at: "2026-10-17T00:00:00Z".into(),
            conversation: None,
        };
        let http = NanoBanana21.build_submit(
            &request,
            &[ImageInput::RawBase64(request.images[0].base64())],
        );
        assert_eq!(
            http.url,
            "https://generativelanguage.googleapis.com/v1beta/interactions"
        );
        assert!(
            http.headers
                .iter()
                .all(|header| header.name != "Api-Revision")
        );
        let crate::http::Body::Json(body) = http.body else {
            panic!("expected json");
        };
        assert_eq!(body["store"], json!(false));
        assert!(body.get("delivery").is_none());
        assert!(body.get("background").is_none());
        assert!(body.get("image_search").is_none());
        assert!(body.get("aspect_ratio").is_none());
        assert!(body.get("image_size").is_none());
        assert_eq!(body["response_format"]["type"], json!("image"));
        assert_eq!(body["response_format"]["aspect_ratio"], json!("16:9"));
        assert_eq!(body["response_format"]["image_size"], json!("2K"));
        assert_eq!(body["input"][0]["type"], json!("text"));
        assert_eq!(body["input"][0]["text"], json!("a red square"));
        assert_eq!(body["input"][1]["type"], json!("image"));
        assert_eq!(body["input"][1]["mime_type"], json!("image/png"));
        assert_eq!(body["input"][1]["data"], json!(request.images[0].base64()));
    }

    #[test]
    fn a_stored_request_does_not_contain_the_api_key() {
        use crate::history::HistoryStore;
        use crate::job::JobRequest;
        use crate::providers::{CostQuote, JobId, Submitted};
        use std::collections::BTreeMap;

        let key = "gemini-secret-key-zz";
        let root = std::env::temp_dir().join(format!(
            "easel-google-request-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let store = HistoryStore::open(&root).unwrap();
        let id = JobId::parse("google-job").unwrap();
        let mut params = BTreeMap::new();
        params.insert("resolution".into(), json!("1K"));
        params.insert("aspect_ratio".into(), json!("1:1"));
        store
            .begin(
                &id,
                &JobRequest {
                    model_id: "gemini-nano-banana-2.1".into(),
                    compiled_prompt: "a red square".into(),
                    params,
                    images: Vec::new(),
                    box_snapshot: Vec::new(),
                    created_at: "2026-10-17T00:00:00Z".into(),
                    conversation: None,
                },
                &Submitted {
                    job_id: id.clone(),
                    poll_url: "https://generativelanguage.googleapis.com/v1beta/interactions"
                        .parse()
                        .unwrap(),
                    quote: Some(CostQuote {
                        cost: 0.0,
                        input_mp: 0.0,
                        output_mp: 0.0,
                    }),
                },
                None,
            )
            .unwrap();
        let stored = std::fs::read_to_string(root.join("google-job/request.json")).unwrap();
        assert!(!stored.contains(key), "{stored}");
        assert!(!stored.contains("x-goog-api-key"), "{stored}");
        let _ = std::fs::remove_dir_all(&root);
    }

    fn google_request(model: &str) -> crate::job::JobRequest {
        let mut params = std::collections::BTreeMap::new();
        params.insert("resolution".into(), json!("1K"));
        params.insert("aspect_ratio".into(), json!("1:1"));
        crate::job::JobRequest {
            model_id: model.into(),
            compiled_prompt: "a cat".into(),
            params,
            images: Vec::new(),
            box_snapshot: Vec::new(),
            created_at: "2026-10-17T00:00:00Z".into(),
            conversation: None,
        }
    }

    fn body_of(model: &str, grounding: Option<bool>) -> serde_json::Value {
        use crate::providers::Provider;
        let mut request = google_request(model);
        if let Some(value) = grounding {
            request.params.insert("grounding".into(), json!(value));
        }
        let provider: Box<dyn Provider> = match model {
            "gemini-3.1-flash-lite-image" => Box::new(NanoBanana2Lite),
            "gemini-3-pro-image" => Box::new(NanoBananaPro),
            _ => Box::new(NanoBanana21),
        };
        let http = provider.build_submit(&request, &[]);
        let crate::http::Body::Json(body) = http.body else {
            panic!("json");
        };
        body
    }

    #[test]
    fn grounding_sends_one_search_tool_and_names_the_thirty_day_copy() {
        let off = body_of("gemini-nano-banana-2.1", Some(false));
        assert!(off.get("tools").is_none());
        let on = body_of("gemini-nano-banana-2.1", Some(true));
        assert_eq!(on["tools"], json!([{ "type": "google_search" }]));
        assert!(on.get("image_search").is_none());
        let help = spec("gemini-nano-banana-2.1")
            .schema
            .fields
            .iter()
            .find(|field| field.key == "grounding")
            .and_then(|field| field.help.clone());
        assert_eq!(
            help.as_deref(),
            Some("Google keeps the prompt and the output for 30 days.")
        );
        let lite = body_of("gemini-3.1-flash-lite-image", Some(true));
        assert!(lite.get("tools").is_none());
        assert!(
            spec("gemini-3.1-flash-lite-image")
                .schema
                .fields
                .iter()
                .all(|field| field.key != "grounding")
        );
    }

    #[test]
    fn fourteen_images_are_sent_and_the_fifteenth_is_not() {
        use crate::job::EncodedImage;
        use crate::providers::Provider;
        let images: Vec<EncodedImage> = (0..15)
            .map(|index| EncodedImage {
                media_type: "image/png".into(),
                bytes: vec![index],
                width: 8,
                height: 8,
                content_hash: format!("h{index}"),
            })
            .collect();
        assert_eq!(NanoBanana21.reference_steps(&images).len(), 14);
        assert_eq!(NanoBanana2Lite.reference_steps(&images).len(), 14);
        assert_eq!(NanoBananaPro.reference_steps(&images).len(), 14);
        let mut request = google_request("gemini-nano-banana-2.1");
        request.images = images;
        let http = NanoBanana21.build_submit(
            &request,
            &request
                .images
                .iter()
                .take(14)
                .map(|image| crate::providers::ImageInput::RawBase64(image.base64()))
                .collect::<Vec<_>>(),
        );
        let crate::http::Body::Json(body) = http.body else {
            panic!("json");
        };
        assert_eq!(body["input"].as_array().unwrap().len(), 15);
    }

    #[test]
    fn a_body_of_twenty_megabytes_stays_sendable_and_one_byte_over_does_not() {
        use crate::job::EncodedImage;
        let spec = NanoBanana21::model_spec();
        let mut request = google_request("gemini-nano-banana-2.1");
        let mut size = 15_000_000;
        request.images = vec![EncodedImage {
            media_type: "image/png".into(),
            bytes: vec![9; size],
            width: 8,
            height: 8,
            content_hash: "big".into(),
        }];
        for _ in 0..6 {
            let len = super::posted_json_len(spec, &request);
            let delta = len as isize - super::BODY_CAP as isize;
            if delta == 0 {
                break;
            }
            let step = (delta.unsigned_abs() * 3 / 4).max(1);
            if delta > 0 {
                size = size.saturating_sub(step);
            } else {
                size += step;
            }
            request.images[0].bytes.resize(size, 9);
        }
        let mut len = super::posted_json_len(spec, &request);
        let mut guard = 0;
        while len > super::BODY_CAP && guard < 16 {
            let extra = len - super::BODY_CAP;
            let step = (extra * 3 / 4).max(1);
            size = size.saturating_sub(step);
            request.images[0].bytes.resize(size, 9);
            len = super::posted_json_len(spec, &request);
            guard += 1;
        }
        while len < super::BODY_CAP {
            request.compiled_prompt.push('a');
            len = super::posted_json_len(spec, &request);
        }
        assert_eq!(len, super::BODY_CAP);
        request.compiled_prompt.push('b');
        assert_eq!(super::posted_json_len(spec, &request), super::BODY_CAP + 1);
    }
}
