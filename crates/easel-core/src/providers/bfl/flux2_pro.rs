use std::collections::BTreeMap;

use serde_json::{Map, Value};

use crate::error::FieldError;
use crate::http::{Body, HttpHeader, HttpRequest, Method};
use crate::job::JobRequest;
use crate::model_spec::{AuthScheme, Capabilities, ImageSlots, ModelSpec, ProviderId, SizeControl};
use crate::providers::{ImageInput, Provider, ReferenceStep, map_images};
use crate::scene::Scene;
use crate::schema::{CrossRule, FieldDef, FieldKind, ParamSchema, RuleContext};

const SAFETY_CAP: i64 = 5;

#[derive(Debug, Clone, Copy)]
pub struct Flux2Pro;

static SPEC: std::sync::LazyLock<ModelSpec> = std::sync::LazyLock::new(spec);

#[must_use]
pub fn model_spec() -> &'static ModelSpec {
    &SPEC
}

/// Clamp FLUX.2 safety values above 5 and say so.
#[must_use]
pub fn prepare_params(params: &BTreeMap<String, Value>) -> (BTreeMap<String, Value>, Vec<String>) {
    let mut params = params.clone();
    let mut notes = Vec::new();
    if let Some(value) = params.get("safety_tolerance").and_then(Value::as_i64)
        && value > SAFETY_CAP
    {
        params.insert("safety_tolerance".into(), Value::from(SAFETY_CAP));
        notes.push(format!(
            "Safety tolerance {value} was clamped to {SAFETY_CAP}."
        ));
    }
    (params, notes)
}

impl Provider for Flux2Pro {
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
        images
            .iter()
            .map(|image| ReferenceStep::Inline(ImageInput::RawBase64(image.base64())))
            .collect()
    }

    fn build_submit(&self, request: &JobRequest, images: &[ImageInput]) -> HttpRequest {
        let (params, _) = prepare_params(&request.params);
        let mut body = Map::new();
        body.insert(
            "prompt".into(),
            Value::String(request.compiled_prompt.clone()),
        );
        for key in [
            "seed",
            "width",
            "height",
            "safety_tolerance",
            "output_format",
            "disable_pup",
            "user",
        ] {
            if let Some(value) = params.get(key).filter(|value| !value.is_null()) {
                body.insert(key.into(), value.clone());
            }
        }
        for (key, value) in map_images(&SPEC.capabilities.image_slots, images) {
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
            auth: Some(ProviderId::Bfl),
            body: Body::Json(Value::Object(body)),
            max_body_bytes: 100 * 1024 * 1024,
        }
    }

    fn poll_request(&self, submitted: &crate::providers::Submitted) -> HttpRequest {
        HttpRequest {
            method: Method::Get,
            url: submitted.poll_url.to_string(),
            headers: Vec::new(),
            auth: Some(ProviderId::Bfl),
            body: Body::Empty,
            max_body_bytes: 2 * 1024 * 1024,
        }
    }

    fn parse_submit(
        &self,
        response: &crate::http::HttpResponse,
    ) -> Result<crate::providers::Submitted, crate::error::ProviderError> {
        crate::providers::bfl::protocol::parse_submit(response)
    }

    fn parse_poll(&self, response: &crate::http::HttpResponse) -> crate::providers::PollOutcome {
        crate::providers::bfl::protocol::parse_poll(response)
    }
}

fn spec() -> ModelSpec {
    let formats = ["jpeg", "png", "webp"]
        .into_iter()
        .map(str::to_string)
        .collect();
    ModelSpec {
        id: "flux-2-pro".into(),
        display_name: "FLUX.2 [pro]".into(),
        provider: ProviderId::Bfl,
        auth: AuthScheme::HeaderKey {
            header: "x-key".into(),
        },
        endpoint: "https://api.bfl.ai/v1/flux-2-pro".into(),
        beta: false,
        capabilities: Capabilities {
            supports_boxes: false,
            image_slots: ImageSlots::Numbered { max: 8 },
            size_control: SizeControl::WidthHeight,
        },
        schema: ParamSchema {
            fields: vec![
                FieldDef {
                    key: "seed".into(),
                    label: "Seed".into(),
                    help: Some("Optional. Leave empty for a new seed.".into()),
                    kind: FieldKind::Int {
                        min: 0,
                        max: i64::MAX,
                        step: 1,
                    },
                    required: false,
                    default: None,
                    locked_when: None,
                },
                dimension("width", "Width"),
                dimension("height", "Height"),
                FieldDef {
                    key: "safety_tolerance".into(),
                    label: "Safety tolerance".into(),
                    help: Some("0 is strictest. FLUX.2 [pro] accepts at most 5.".into()),
                    kind: FieldKind::Int {
                        min: 0,
                        max: SAFETY_CAP,
                        step: 1,
                    },
                    required: true,
                    default: Some(Value::from(2)),
                    locked_when: None,
                },
                FieldDef {
                    key: "output_format".into(),
                    label: "Output format".into(),
                    help: None,
                    kind: FieldKind::Select { options: formats },
                    required: true,
                    default: Some(Value::String("jpeg".into())),
                    locked_when: None,
                },
                FieldDef {
                    key: "disable_pup".into(),
                    label: "Disable prompt upsampling".into(),
                    help: Some("Generate from the prompt as written.".into()),
                    kind: FieldKind::Bool,
                    required: true,
                    default: Some(Value::Bool(false)),
                    locked_when: None,
                },
                FieldDef {
                    key: "user".into(),
                    label: "User".into(),
                    help: Some("Optional id for the end user.".into()),
                    kind: FieldKind::Text { multiline: false },
                    required: false,
                    default: None,
                    locked_when: None,
                },
            ],
            cross_rules: vec![
                CrossRule::MultipleOf {
                    keys: vec!["width".into(), "height".into()],
                    n: 16,
                    exempt_zero: true,
                },
                CrossRule::ZeroOrAtLeast {
                    keys: vec!["width".into(), "height".into()],
                    min: 64,
                },
                CrossRule::MaxArea {
                    w: "width".into(),
                    h: "height".into(),
                    max: 4_000_000,
                },
            ],
        },
    }
}

fn dimension(key: &str, label: &str) -> FieldDef {
    FieldDef {
        key: key.into(),
        label: label.into(),
        help: Some("0 lets the model choose the size. Otherwise a multiple of 16.".into()),
        kind: FieldKind::Int {
            min: 0,
            max: 4096,
            step: 16,
        },
        required: true,
        default: Some(Value::from(0)),
        locked_when: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Flux2Pro, prepare_params};
    use crate::http::{Body, HttpResponse};
    use crate::job::JobRequest;
    use crate::model_spec::ImageSlots;
    use crate::providers::bfl::protocol::{self, READY_POLL_BODY};
    use crate::providers::{ImageInput, PollOutcome, Provider, map_images};
    use crate::scene::Scene;
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

    fn sized(width: i64, height: i64) -> BTreeMap<String, Value> {
        let mut values = Flux2Pro.spec().schema.defaults();
        values.insert("width".into(), json!(width));
        values.insert("height".into(), json!(height));
        values
    }

    fn request(prompt: &str, params: BTreeMap<String, Value>) -> JobRequest {
        JobRequest {
            model_id: "flux-2-pro".into(),
            compiled_prompt: prompt.into(),
            params,
            images: Vec::new(),
            box_snapshot: Vec::new(),
            created_at: "2026-10-15T00:00:00Z".into(),
        }
    }

    #[test]
    fn text_to_image_body_snapshot() {
        let mut params = Flux2Pro.spec().schema.defaults();
        params.insert("seed".into(), json!(42));
        params.insert("width".into(), json!(1024));
        params.insert("height".into(), json!(768));
        params.insert("output_format".into(), json!("png"));
        params.insert("disable_pup".into(), json!(true));
        let body = Flux2Pro.build_submit(&request("a harbor at dusk", params), &[]);
        let Body::Json(json) = &body.body else {
            panic!("json");
        };
        insta::with_settings!({ sort_maps => true }, {
            insta::assert_json_snapshot!(json);
        });
    }

    #[test]
    fn three_references_fill_numbered_slots_and_the_ninth_is_not_sent() {
        let inputs = ["YQ==", "Yg==", "Yw=="]
            .into_iter()
            .map(|text| ImageInput::RawBase64(text.into()))
            .collect::<Vec<_>>();
        let mapped = map_images(&ImageSlots::Numbered { max: 8 }, &inputs);
        assert_eq!(mapped["input_image"], json!("YQ=="));
        assert_eq!(mapped["input_image_2"], json!("Yg=="));
        assert_eq!(mapped["input_image_3"], json!("Yw=="));
        assert!(!mapped.contains_key("input_image_4"));

        let slots = &Flux2Pro.spec().capabilities.image_slots;
        let error = Flux2Pro
            .spec()
            .capabilities
            .validate_image_count(9)
            .expect("nine is over the cap");
        assert_eq!(error.key, "images");
        assert_eq!(slots.max(), 8);
        let mut scene = Scene::empty();
        scene.prose = "a harbor".into();
        let errors = Flux2Pro.validate_for_submit(&scene, &sized(1024, 768), 9);
        assert!(
            errors.iter().all(|error| error.key != "images"),
            "{errors:?}"
        );
    }

    #[test]
    fn width_and_height_follow_the_flux2_limits() {
        let schema = &Flux2Pro.spec().schema;
        let both = schema.validate(&sized(1000, 1000), &ctx());
        assert!(both.iter().any(|error| error.key == "width"));
        assert!(both.iter().any(|error| error.key == "height"));
        assert!(
            both.iter()
                .all(|error| error.message.contains("multiple of 16"))
        );

        let area = schema.validate(&sized(2048, 2048), &ctx());
        assert!(area.iter().any(|error| error.message.contains("4 MP")));

        assert!(schema.validate(&sized(0, 0), &ctx()).is_empty());
        assert!(schema.validate(&sized(1024, 0), &ctx()).is_empty());

        let narrow = schema.validate(&sized(48, 1024), &ctx());
        assert!(
            narrow
                .iter()
                .any(|error| error.key == "width" && error.message.contains("64"))
        );
        assert!(narrow.iter().all(|error| error.key != "height"));
    }

    #[test]
    fn safety_tolerance_above_five_is_clamped_with_a_note() {
        let mut params = sized(0, 0);
        params.insert("safety_tolerance".into(), json!(6));
        let (prepared, notes) = prepare_params(&params);
        assert_eq!(prepared["safety_tolerance"], json!(5));
        assert!(notes.iter().any(|note| note.contains('5')), "{notes:?}");
        let body = Flux2Pro.build_submit(&request("a harbor", params), &[]);
        let Body::Json(json) = body.body else {
            panic!("json");
        };
        assert_eq!(json["safety_tolerance"], json!(5));
    }

    #[test]
    fn ready_poll_reuses_the_bfl_fixture() {
        let response = HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: READY_POLL_BODY.as_bytes().to_vec(),
        };
        assert_eq!(
            Flux2Pro.parse_poll(&response),
            protocol::parse_poll(&response)
        );
        assert!(matches!(
            Flux2Pro.parse_poll(&response),
            PollOutcome::Ready { settled_cost: Some(cost), .. } if cost == 4.5
        ));
    }

    #[test]
    fn spec_round_trips_with_the_size_rules() {
        let spec = Flux2Pro.spec().clone();
        let back: crate::model_spec::ModelSpec =
            serde_json::from_value(serde_json::to_value(&spec).unwrap()).unwrap();
        assert_eq!(spec, back);
        assert_eq!(back.schema.cross_rules.len(), 3);
        assert!(!back.capabilities.supports_boxes);
    }
}
