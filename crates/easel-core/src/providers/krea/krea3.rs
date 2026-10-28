use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::error::FieldError;
use crate::http::{Body, HttpHeader, HttpRequest, Method};
use crate::job::JobRequest;
use crate::model_spec::{AuthScheme, Capabilities, ImageSlots, ModelSpec, ProviderId, SizeControl};
use crate::providers::krea::{assets, protocol};
use crate::providers::{ImageInput, PollOutcome, Provider, ReferenceStep, Submitted};
use crate::scene::Scene;
use crate::schema::{CrossRule, FieldDef, FieldKind, ParamSchema, RuleContext};

#[derive(Debug, Clone, Copy)]
pub struct Krea3;

static SPEC: std::sync::LazyLock<ModelSpec> = std::sync::LazyLock::new(spec);

#[must_use]
pub fn model_spec() -> &'static ModelSpec {
    &SPEC
}

pub const BETA_MESSAGE: &str = "Krea 3 is in beta; your workspace needs access";

impl Provider for Krea3 {
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

    fn reference_steps(&self, images: &[crate::job::EncodedImage]) -> Vec<ReferenceStep> {
        assets::reference_steps(images)
    }

    fn parse_upload(
        &self,
        response: &crate::http::HttpResponse,
    ) -> Result<ImageInput, crate::error::ProviderError> {
        assets::parse_upload(response)
    }

    fn build_submit(&self, request: &JobRequest, images: &[ImageInput]) -> HttpRequest {
        let mut body = Map::new();
        body.insert(
            "prompt".into(),
            Value::String(request.compiled_prompt.clone()),
        );
        for key in ["thinking_mode", "resolution", "seed", "regional_edit"] {
            if let Some(value) = request.params.get(key).filter(|value| !value.is_null()) {
                body.insert(key.into(), value.clone());
            }
        }
        if let Some(value) = request
            .params
            .get("aspect_ratio")
            .and_then(Value::as_str)
            .filter(|text| !text.is_empty())
        {
            body.insert("aspect_ratio".into(), Value::String(value.to_owned()));
        }
        for (key, value) in crate::providers::map_images(&SPEC.capabilities.image_slots, images) {
            body.insert(key, value);
        }
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
            redact_query: false,
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

fn spec() -> ModelSpec {
    let aspects = [
        "1:1", "4:5", "3:4", "2:3", "9:16", "5:4", "4:3", "3:2", "16:9", "21:9",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    ModelSpec {
        id: "krea-3".into(),
        display_name: "Krea 3".into(),
        provider: ProviderId::Krea,
        auth: AuthScheme::Bearer,
        endpoint: "https://api.krea.ai/generate/image/krea/krea-3".into(),
        beta: true,
        capabilities: Capabilities {
            supports_boxes: false,
            image_slots: ImageSlots::List {
                max: 8,
                key: "image_urls".into(),
            },
            size_control: SizeControl::AspectRatioAndResolution,
        },
        schema: ParamSchema {
            fields: vec![
                FieldDef {
                    key: "thinking_mode".into(),
                    label: "Thinking mode".into(),
                    help: Some("high costs more than fast.".into()),
                    kind: FieldKind::Select {
                        options: vec!["fast".into(), "high".into()],
                    },
                    required: true,
                    default: Some(Value::String("fast".into())),
                    locked_when: None,
                },
                FieldDef {
                    key: "resolution".into(),
                    label: "Resolution".into(),
                    help: Some("Uploaded reference images may persist on Krea's side.".into()),
                    kind: FieldKind::Select {
                        options: vec!["1K".into(), "2K".into()],
                    },
                    required: true,
                    default: Some(Value::String("1K".into())),
                    locked_when: None,
                },
                FieldDef {
                    key: "aspect_ratio".into(),
                    label: "Aspect ratio".into(),
                    help: Some("Leave empty and Krea chooses.".into()),
                    kind: FieldKind::Select { options: aspects },
                    required: false,
                    default: None,
                    locked_when: None,
                },
                FieldDef {
                    key: "seed".into(),
                    label: "Seed".into(),
                    help: None,
                    kind: FieldKind::Int {
                        min: 0,
                        max: i64::MAX,
                        step: 1,
                    },
                    required: false,
                    default: None,
                    locked_when: None,
                },
                FieldDef {
                    key: "regional_edit".into(),
                    label: "Regional edit".into(),
                    help: Some("Needs at least one reference image.".into()),
                    kind: FieldKind::Bool,
                    required: true,
                    default: Some(Value::Bool(false)),
                    locked_when: None,
                },
            ],
            cross_rules: vec![CrossRule::RequiresImagesWhen {
                key: "regional_edit".into(),
                min: 1,
            }],
        },
    }
}

#[cfg(test)]
mod tests {
    use super::Krea3;
    use crate::http::Body;
    use crate::job::JobRequest;
    use crate::providers::{ImageInput, Provider};
    use crate::scene::Scene;
    use crate::schema::RuleContext;
    use serde_json::{Value, json};
    use std::collections::BTreeMap;

    fn request(prompt: &str, params: BTreeMap<String, Value>) -> JobRequest {
        JobRequest {
            model_id: "krea-3".into(),
            compiled_prompt: prompt.into(),
            params,
            images: Vec::new(),
            box_snapshot: Vec::new(),
            created_at: "2026-10-16T00:00:00Z".into(),
            conversation: None,
        }
    }

    #[test]
    fn edit_body_snapshot_omits_an_empty_aspect_ratio() {
        let mut params = Krea3.spec().schema.defaults();
        params.insert("thinking_mode".into(), json!("high"));
        params.insert("resolution".into(), json!("2K"));
        params.insert("regional_edit".into(), json!(true));
        params.insert("seed".into(), json!(7));
        let images = [
            ImageInput::Url("https://api.krea.ai/assets/a".into()),
            ImageInput::Url("https://api.krea.ai/assets/b".into()),
        ];
        let body = Krea3.build_submit(&request("a moonlit lake", params), &images);
        let Body::Json(json) = &body.body else {
            panic!("json");
        };
        assert!(json.get("aspect_ratio").is_none());
        insta::with_settings!({ sort_maps => true }, {
            insta::assert_json_snapshot!(json);
        });
    }

    #[test]
    fn regional_edit_requires_a_reference() {
        let mut values = Krea3.spec().schema.defaults();
        values.insert("regional_edit".into(), json!(true));
        let ctx = RuleContext {
            has_source_image: false,
            image_count: 0,
            box_count: 0,
        };
        let errors = Krea3.spec().schema.validate(&values, &ctx);
        assert!(errors.iter().any(|error| {
            error.key == "regional_edit" && error.message.contains("reference image")
        }));
        let ctx = RuleContext {
            image_count: 1,
            ..ctx
        };
        assert!(Krea3.spec().schema.validate(&values, &ctx).is_empty());
        let mut scene = Scene::empty();
        scene.prose = "a lake".into();
        assert!(
            Krea3
                .validate_for_submit(&scene, &values, 0)
                .iter()
                .any(|error| error.key == "regional_edit")
        );
    }
}
