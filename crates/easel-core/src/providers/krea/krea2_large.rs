use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::error::FieldError;
use crate::http::{Body, HttpHeader, HttpRequest, Method};
use crate::job::JobRequest;
use crate::model_spec::{
    AuthScheme, Capabilities, ImageSlots, ModelSpec, ProviderId, SizeControl,
};
use crate::providers::krea::{assets, protocol};
use crate::providers::{ImageInput, PollOutcome, Provider, ReferenceStep, Submitted};
use crate::schema::{FieldDef, FieldKind, ParamSchema, RuleContext};
use crate::scene::Scene;

#[derive(Debug, Clone, Copy)]
pub struct Krea2Large;

static SPEC: std::sync::LazyLock<ModelSpec> = std::sync::LazyLock::new(spec);

#[must_use]
pub fn model_spec() -> &'static ModelSpec {
    &SPEC
}

impl Provider for Krea2Large {
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
            image_count: image_count.min(SPEC.capabilities.image_slots.max()),
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
        for key in [
            "aspect_ratio",
            "resolution",
            "seed",
            "creativity",
            "intensity",
            "complexity",
            "movement",
        ] {
            if let Some(value) = request.params.get(key).filter(|value| !value.is_null()) {
                body.insert(key.into(), value.clone());
            }
        }
        for key in ["styles", "moodboards"] {
            if let Some(Value::Array(items)) = request.params.get(key)
                && !items.is_empty()
            {
                body.insert(key.into(), Value::Array(items.clone()));
            }
        }
        let mapped = crate::providers::map_images(&SPEC.capabilities.image_slots, images);
        let has_image = mapped.contains_key("image_url");
        for (key, value) in mapped {
            body.insert(key, value);
        }
        if has_image {
            let strength = request
                .params
                .get("strength")
                .cloned()
                .unwrap_or_else(|| Value::from(0.99));
            body.insert("strength".into(), strength);
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
        "1:1", "4:3", "3:2", "16:9", "2.35:1", "4:5", "3:4", "2:3", "9:16",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    ModelSpec {
        id: "krea-2-large".into(),
        display_name: "Krea 2 Large".into(),
        provider: ProviderId::Krea,
        auth: AuthScheme::Bearer,
        endpoint: "https://api.krea.ai/generate/image/krea/krea-2/large".into(),
        beta: false,
        capabilities: Capabilities {
            supports_boxes: false,
            image_slots: ImageSlots::SingleWithStyleRefs { max_style_refs: 10 },
            size_control: SizeControl::AspectRatioAndResolution,
        },
        schema: ParamSchema {
            fields: vec![
                FieldDef {
                    key: "aspect_ratio".into(),
                    label: "Aspect ratio".into(),
                    help: None,
                    kind: FieldKind::Select { options: aspects },
                    required: true,
                    default: None,
                    locked_when: None,
                },
                FieldDef {
                    key: "resolution".into(),
                    label: "Resolution".into(),
                    help: Some("Uploaded reference images may persist on Krea's side.".into()),
                    kind: FieldKind::Select {
                        options: vec!["1K".into()],
                    },
                    required: true,
                    default: Some(Value::String("1K".into())),
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
                    key: "creativity".into(),
                    label: "Creativity".into(),
                    help: Some("raw sends the prompt as written.".into()),
                    kind: FieldKind::Select {
                        options: ["raw", "low", "medium", "high"]
                            .into_iter()
                            .map(str::to_string)
                            .collect(),
                    },
                    required: true,
                    default: Some(Value::String("low".into())),
                    locked_when: None,
                },
                slider("intensity", "Intensity"),
                slider("complexity", "Complexity"),
                slider("movement", "Movement"),
                FieldDef {
                    key: "strength".into(),
                    label: "Strength".into(),
                    help: Some("How far an image-to-image run moves from the source. 0.99 is the default.".into()),
                    kind: FieldKind::Float {
                        min: 0.0,
                        max: 1.0,
                        step: 0.01,
                    },
                    required: false,
                    default: Some(Value::from(0.99)),
                    locked_when: None,
                },
                FieldDef {
                    key: "styles".into(),
                    label: "Styles".into(),
                    help: Some("Style ids come from the Krea web app or GET /styles.".into()),
                    kind: FieldKind::Repeated {
                        max: 16,
                        item: vec![
                            FieldDef {
                                key: "id".into(),
                                label: "Id".into(),
                                help: None,
                                kind: FieldKind::Text { multiline: false },
                                required: true,
                                default: None,
                                locked_when: None,
                            },
                            FieldDef {
                                key: "strength".into(),
                                label: "Strength".into(),
                                help: None,
                                kind: FieldKind::Float {
                                    min: -2.0,
                                    max: 2.0,
                                    step: 0.01,
                                },
                                required: true,
                                default: Some(Value::from(1.0)),
                                locked_when: None,
                            },
                        ],
                    },
                    required: false,
                    default: None,
                    locked_when: None,
                },
                FieldDef {
                    key: "moodboards".into(),
                    label: "Moodboards".into(),
                    help: Some("Moodboard ids come from the Krea web app.".into()),
                    kind: FieldKind::Repeated {
                        max: 1,
                        item: vec![
                            FieldDef {
                                key: "id".into(),
                                label: "Id".into(),
                                help: None,
                                kind: FieldKind::Uuid,
                                required: true,
                                default: None,
                                locked_when: None,
                            },
                            FieldDef {
                                key: "strength".into(),
                                label: "Strength".into(),
                                help: None,
                                kind: FieldKind::Float {
                                    min: 0.0,
                                    max: 1.0,
                                    step: 0.01,
                                },
                                required: false,
                                default: Some(Value::from(0.23)),
                                locked_when: None,
                            },
                        ],
                    },
                    required: false,
                    default: None,
                    locked_when: None,
                },
            ],
            cross_rules: Vec::new(),
        },
    }
}

fn slider(key: &str, label: &str) -> FieldDef {
    FieldDef {
        key: key.into(),
        label: label.into(),
        help: Some("From -100 to 100. 0 leaves the slider off.".into()),
        kind: FieldKind::Int {
            min: -100,
            max: 100,
            step: 1,
        },
        required: true,
        default: Some(Value::from(0)),
        locked_when: None,
    }
}

#[cfg(test)]
mod tests {
    use super::Krea2Large;
    use crate::http::Body;
    use crate::job::JobRequest;
    use crate::providers::{ImageInput, Provider};
    use crate::schema::RuleContext;
    use serde_json::{Value, json};
    use std::collections::BTreeMap;

    fn ctx() -> RuleContext {
        RuleContext {
            has_source_image: false,
            image_count: 0,
            box_count: 0,
        }
    }

    fn request(prompt: &str, params: BTreeMap<String, Value>) -> JobRequest {
        JobRequest {
            model_id: "krea-2-large".into(),
            compiled_prompt: prompt.into(),
            params,
            images: Vec::new(),
            box_snapshot: Vec::new(),
            created_at: "2026-10-16T00:00:00Z".into(),
        }
    }

    fn ready() -> BTreeMap<String, Value> {
        let mut values = Krea2Large.spec().schema.defaults();
        values.insert("aspect_ratio".into(), json!("3:2"));
        values.insert("resolution".into(), json!("1K"));
        values
    }

    #[test]
    fn text_body_snapshot_includes_styles_and_a_moodboard() {
        let mut params = ready();
        params.insert("creativity".into(), json!("medium"));
        params.insert("intensity".into(), json!(40));
        params.insert(
            "styles".into(),
            json!([
                {"id": "style-a", "strength": 0.8},
                {"id": "style-b", "strength": -0.4}
            ]),
        );
        params.insert(
            "moodboards".into(),
            json!([{"id": "11111111-1111-4111-8111-111111111111", "strength": 0.23}]),
        );
        let body = Krea2Large.build_submit(&request("adobe towers", params), &[]);
        let Body::Json(json) = &body.body else {
            panic!("json");
        };
        assert!(json.get("strength").is_none());
        assert!(json.get("image_url").is_none());
        insta::with_settings!({ sort_maps => true }, {
            insta::assert_json_snapshot!(json);
        });
    }

    #[test]
    fn aspect_ratio_and_resolution_are_required() {
        let schema = &Krea2Large.spec().schema;
        let mut values = ready();
        values.remove("aspect_ratio");
        assert!(schema.validate(&values, &ctx()).iter().any(|error| error.key == "aspect_ratio"));
        let mut values = ready();
        values.remove("resolution");
        assert!(schema.validate(&values, &ctx()).iter().any(|error| error.key == "resolution"));
    }

    #[test]
    fn references_split_into_the_source_and_style_refs() {
        let inputs = ["https://api.krea.ai/a", "https://api.krea.ai/b", "https://api.krea.ai/c"]
            .into_iter()
            .map(|url| ImageInput::Url(url.into()))
            .collect::<Vec<_>>();
        let body = Krea2Large.build_submit(&request("a street", ready()), &inputs);
        let Body::Json(json) = body.body else {
            panic!("json");
        };
        assert_eq!(json["image_url"], json!("https://api.krea.ai/a"));
        assert_eq!(json["strength"], json!(0.99));
        assert_eq!(
            json["image_style_references"],
            json!([
                {"url": "https://api.krea.ai/b", "strength": 0.5},
                {"url": "https://api.krea.ai/c", "strength": 0.5}
            ])
        );
        let empty = Krea2Large.build_submit(&request("a street", ready()), &[]);
        let Body::Json(json) = empty.body else {
            panic!("json");
        };
        assert!(json.get("image_url").is_none());
        assert!(json.get("strength").is_none());
        assert!(json.get("image_style_references").is_none());
        assert!(
            Krea2Large
                .spec()
                .capabilities
                .validate_image_count(11)
                .is_none()
        );
        assert!(
            Krea2Large
                .spec()
                .capabilities
                .validate_image_count(12)
                .is_some()
        );
    }

    #[test]
    fn moodboards_styles_and_sliders_reject_bad_values() {
        let schema = &Krea2Large.spec().schema;
        let mut values = ready();
        values.insert(
            "moodboards".into(),
            json!([{"id": "not-a-uuid", "strength": 0.23}]),
        );
        assert!(schema.validate(&values, &ctx()).iter().any(|error| {
            error.key == "moodboards" && error.message.contains("UUID")
        }));
        values.insert(
            "moodboards".into(),
            json!([
                {"id": "11111111-1111-4111-8111-111111111111", "strength": 0.23},
                {"id": "22222222-2222-4222-8222-222222222222", "strength": 0.23}
            ]),
        );
        assert!(schema.validate(&values, &ctx()).iter().any(|error| {
            error.key == "moodboards" && error.message.contains("at most 1")
        }));
        let mut values = ready();
        values.insert("movement".into(), json!(101));
        assert!(schema.validate(&values, &ctx()).iter().any(|error| error.key == "movement"));
        values.insert("movement".into(), json!(0));
        values.insert("strength".into(), json!(1.5));
        assert!(schema.validate(&values, &ctx()).iter().any(|error| error.key == "strength"));
    }
}
