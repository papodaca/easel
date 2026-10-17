mod assets;
pub mod protocol;

use std::collections::BTreeMap;

use serde_json::Value;

use crate::error::FieldError;
use crate::http::{Body, HttpHeader, HttpRequest, Method};
use crate::job::JobRequest;
use crate::model_spec::{AuthScheme, Capabilities, ImageSlots, ModelSpec, ProviderId, SizeControl};
use crate::providers::{ImageInput, PollOutcome, Provider, ReferenceStep, Submitted};
use crate::scene::Scene;
use crate::schema::{FieldDef, FieldKind, ParamSchema};

pub use assets::{parse_upload, reference_steps};
pub use protocol::{Sunset, deprecation_headers, sunset_toast};

/// Shared Krea job behavior. Model specs in later units call the same protocol.
#[derive(Debug, Clone, Copy)]
pub struct Krea;

static SPEC: std::sync::LazyLock<ModelSpec> = std::sync::LazyLock::new(|| ModelSpec {
    id: "krea".into(),
    display_name: "Krea".into(),
    provider: ProviderId::Krea,
    auth: AuthScheme::Bearer,
    endpoint: "https://api.krea.ai/generate/image/krea/test".into(),
    beta: false,
    capabilities: Capabilities {
        supports_boxes: false,
        image_slots: ImageSlots::List { max: 8 },
        size_control: SizeControl::AspectRatioAndResolution,
    },
    schema: ParamSchema {
        fields: vec![FieldDef {
            key: "prompt".into(),
            label: "Prompt".into(),
            help: Some("Uploaded images may persist on Krea's side.".into()),
            kind: FieldKind::Text { multiline: true },
            required: true,
            default: Some(Value::String(String::new())),
            locked_when: None,
        }],
        cross_rules: Vec::new(),
    },
});

impl Provider for Krea {
    fn spec(&self) -> &ModelSpec {
        &SPEC
    }

    fn validate_for_submit(
        &self,
        scene: &Scene,
        _values: &BTreeMap<String, Value>,
        _image_count: usize,
    ) -> Vec<FieldError> {
        if scene.prose.trim().is_empty() {
            vec![FieldError {
                key: "prompt".into(),
                message: "Prompt is empty".into(),
                approved_max: None,
            }]
        } else {
            Vec::new()
        }
    }

    fn reference_steps(&self, images: &[crate::job::EncodedImage]) -> Vec<ReferenceStep> {
        reference_steps(images)
    }

    fn parse_upload(
        &self,
        response: &crate::http::HttpResponse,
    ) -> Result<ImageInput, crate::error::ProviderError> {
        parse_upload(response)
    }

    fn build_submit(&self, request: &JobRequest, _images: &[ImageInput]) -> HttpRequest {
        let mut body = serde_json::Map::new();
        body.insert(
            "prompt".into(),
            Value::String(request.compiled_prompt.clone()),
        );
        HttpRequest {
            method: Method::Post,
            url: SPEC.endpoint.clone(),
            headers: vec![HttpHeader {
                name: "Content-Type".into(),
                value: "application/json".into(),
                sensitive: false,
            }],
            auth: Some(ProviderId::Krea),
            body: Body::Json(Value::Object(body)),
            max_body_bytes: 2 * 1024 * 1024,
        }
    }

    fn poll_request(&self, submitted: &Submitted) -> HttpRequest {
        protocol::poll_request(submitted)
    }

    fn parse_submit(
        &self,
        response: &crate::http::HttpResponse,
    ) -> Result<Submitted, crate::error::ProviderError> {
        protocol::parse_submit(response)
    }

    fn parse_poll(&self, response: &crate::http::HttpResponse) -> PollOutcome {
        protocol::parse_poll(response)
    }

    fn cancel_request(
        &self,
        submitted: &Submitted,
        last: Option<&PollOutcome>,
    ) -> Option<HttpRequest> {
        protocol::cancel_request(submitted, last)
    }

    fn poll_policy(&self) -> crate::job::PollPolicy {
        protocol::poll_policy()
    }
}
