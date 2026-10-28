use crate::geometry::{Rect, Size};
use crate::prose;
use crate::scene::{BoxElement, BoxKind, Scene};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompiledPrompt {
    pub text: String,
    pub rows_json: String,
    pub warnings: Vec<String>,
}

#[derive(Serialize)]
struct LayoutRow<'a> {
    id: &'a str,
    bbox: [u16; 4],
    desc: &'a str,
}

#[derive(Serialize)]
struct EditRow<'a> {
    id: &'a str,
    from: Option<String>,
    src_bbox: Option<[u16; 4]>,
    tgt_bbox: Option<[u16; 4]>,
    desc: &'a str,
}

#[derive(Serialize)]
#[serde(untagged)]
enum Row<'a> {
    Layout(LayoutRow<'a>),
    Edit(EditRow<'a>),
}

#[must_use]
pub fn compile_prompt(scene: &Scene, frame: Size) -> CompiledPrompt {
    let warnings = warnings_for(scene, frame);
    if scene.boxes.is_empty() {
        return CompiledPrompt {
            text: scene.prose.clone(),
            rows_json: String::new(),
            warnings,
        };
    }
    let rows: Vec<Row<'_>> = scene
        .boxes
        .iter()
        .map(|element| row_for(scene, element, frame))
        .collect();
    let rows_json = serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string());
    CompiledPrompt {
        text: format!("{} {rows_json}", scene.prose),
        rows_json,
        warnings,
    }
}

fn row_for<'a>(scene: &'a Scene, element: &'a BoxElement, fallback: Size) -> Row<'a> {
    let frame = frame_for(scene, element, fallback);
    match element.kind {
        BoxKind::Layout => Row::Layout(LayoutRow {
            id: element.id.as_str(),
            bbox: grid(
                element.src.unwrap_or(Rect {
                    x: 0,
                    y: 0,
                    w: 0,
                    h: 0,
                }),
                frame,
            ),
            desc: &element.desc,
        }),
        BoxKind::New => Row::Edit(EditRow {
            id: element.id.as_str(),
            from: None,
            src_bbox: None,
            tgt_bbox: element.tgt.map(|rect| grid(rect, frame)),
            desc: &element.desc,
        }),
        BoxKind::Keep | BoxKind::Move => Row::Edit(EditRow {
            id: element.id.as_str(),
            from: from_label(element),
            src_bbox: element.src.map(|rect| grid(rect, frame)),
            tgt_bbox: element.tgt.map(|rect| grid(rect, frame)),
            desc: &element.desc,
        }),
        BoxKind::Remove => Row::Edit(EditRow {
            id: element.id.as_str(),
            from: from_label(element),
            src_bbox: element.src.map(|rect| grid(rect, frame)),
            tgt_bbox: None,
            desc: &element.desc,
        }),
    }
}

fn frame_for(scene: &Scene, element: &BoxElement, fallback: Size) -> Size {
    if element.kind == BoxKind::Layout && scene.source_image.is_none() {
        return scene.frame().unwrap_or(fallback);
    }
    if let Some(index) = element.from_image
        && let Some(size) = scene.slots().get(index).and_then(|image| image.size())
    {
        return size;
    }
    if element.kind == BoxKind::Layout {
        return scene.frame().unwrap_or(fallback);
    }
    fallback
}

fn from_label(element: &BoxElement) -> Option<String> {
    element.from_image.map(|index| format!("ref_image_{index}"))
}

fn grid(rect: Rect, frame: Size) -> [u16; 4] {
    rect.to_grid(frame).0
}

fn warnings_for(scene: &Scene, frame: Size) -> Vec<String> {
    let mut warnings = Vec::new();
    for element in &scene.boxes {
        if element.kind != BoxKind::Layout && element.from_image.is_none() {
            warnings.push(format!(
                "{} has no image slot, so it was not sent as ref_image_0",
                element.id.as_str()
            ));
        }
        if element.kind != BoxKind::New {
            continue;
        }
        let Some(rect) = element.tgt.or(element.src) else {
            continue;
        };
        let [top, left, bottom, right] = grid(rect, frame_for(scene, element, frame));
        if right.saturating_sub(left) < 40 || bottom.saturating_sub(top) < 25 {
            warnings.push(format!(
                "{} is under 40×25 grid units; small new elements often fail",
                element.id.as_str()
            ));
        }
    }
    let report = prose::unmatched(&scene.prose, scene);
    for tag in report.tags_without_box {
        warnings.push(format!("<{tag}> has no matching box"));
    }
    for id in report.boxes_without_tag {
        warnings.push(format!("<{}> is not mentioned in the prompt", id.as_str()));
    }
    if !scene.boxes.is_empty()
        && scene.aspect_ratio.as_deref() == Some("auto")
        && mentions_ratio(&scene.prose)
    {
        warnings.push(
            "aspect_ratio is auto and the prompt asks for a ratio, which wins over the source framing and shifts every box"
                .into(),
        );
    }
    warnings
}

fn mentions_ratio(prose: &str) -> bool {
    if prose.to_ascii_lowercase().contains("aspect ratio") {
        return true;
    }
    let bytes = prose.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index].is_ascii_digit() {
            while index < bytes.len() && bytes[index].is_ascii_digit() {
                index += 1;
            }
            if bytes.get(index) == Some(&b':')
                && bytes.get(index + 1).is_some_and(u8::is_ascii_digit)
            {
                return true;
            }
            continue;
        }
        index += 1;
    }
    false
}

#[derive(Debug, Clone, Copy)]
pub struct Flux3;

impl crate::providers::Provider for Flux3 {
    fn spec(&self) -> &crate::model_spec::ModelSpec {
        &SPEC
    }

    fn compile_prompt(
        &self,
        scene: &crate::scene::Scene,
        frame: crate::geometry::Size,
    ) -> CompiledPrompt {
        compile_prompt(scene, frame)
    }

    fn validate_for_submit(
        &self,
        scene: &crate::scene::Scene,
        values: &std::collections::BTreeMap<String, serde_json::Value>,
        image_count: usize,
    ) -> Vec<crate::error::FieldError> {
        validate_scene(scene, values, image_count)
    }

    fn reference_steps(
        &self,
        images: &[crate::job::EncodedImage],
    ) -> Vec<crate::providers::ReferenceStep> {
        images
            .iter()
            .map(|image| {
                crate::providers::ReferenceStep::Inline(crate::providers::ImageInput::RawBase64(
                    image.base64(),
                ))
            })
            .collect()
    }

    fn build_submit(
        &self,
        request: &crate::job::JobRequest,
        images: &[crate::providers::ImageInput],
    ) -> crate::http::HttpRequest {
        build_submit(request, images)
    }

    fn poll_request(&self, submitted: &crate::providers::Submitted) -> crate::http::HttpRequest {
        crate::http::HttpRequest {
            method: crate::http::Method::Get,
            url: submitted.poll_url.to_string(),
            headers: Vec::new(),
            auth: Some(crate::model_spec::ProviderId::Bfl),
            body: crate::http::Body::Empty,
            max_body_bytes: 2 * 1024 * 1024,
            redact_query: false,
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

static SPEC: std::sync::LazyLock<crate::model_spec::ModelSpec> = std::sync::LazyLock::new(spec);

#[must_use]
pub fn model_spec() -> &'static crate::model_spec::ModelSpec {
    &SPEC
}

fn spec() -> crate::model_spec::ModelSpec {
    use crate::model_spec::{
        AuthScheme, Capabilities, ImageSlots, ModelSpec, ProviderId, SizeControl,
    };
    use crate::schema::{FieldDef, FieldKind, LockRule, ParamSchema};
    use serde_json::json;

    let aspects = [
        "21:9", "2:1", "16:9", "3:2", "7:5", "4:3", "5:4", "1:1", "4:5", "3:4", "5:7", "2:3",
        "9:16", "1:2", "9:21", "auto",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    ModelSpec {
        id: "flux-3-image".into(),
        display_name: "FLUX 3".into(),
        provider: ProviderId::Bfl,
        auth: AuthScheme::HeaderKey {
            header: "x-key".into(),
        },
        endpoint: "https://api.bfl.ai/v1/flux-3-image".into(),
        beta: false,
        capabilities: Capabilities {
            supports_boxes: true,
            image_slots: ImageSlots::List {
                max: 10,
                key: "images".into(),
            },
            size_control: SizeControl::AspectRatioAndResolution,
        },
        schema: ParamSchema {
            fields: vec![
                FieldDef {
                    key: "aspect_ratio".into(),
                    label: "Aspect ratio".into(),
                    help: Some(
                        "keeps the source framing unless the prompt asks for a ratio".into(),
                    ),
                    kind: FieldKind::Select { options: aspects },
                    required: true,
                    default: Some(json!("auto")),
                    locked_when: None,
                },
                FieldDef {
                    key: "resolution".into(),
                    label: "Resolution".into(),
                    help: None,
                    kind: FieldKind::Select {
                        options: ["768sq", "1k", "1.5k", "2k", "4k"]
                            .into_iter()
                            .map(str::to_string)
                            .collect(),
                    },
                    required: true,
                    default: Some(json!("1k")),
                    locked_when: None,
                },
                FieldDef {
                    key: "safety_tolerance".into(),
                    label: "Safety tolerance".into(),
                    help: Some("0 is strictest. The account may reject higher levels.".into()),
                    kind: FieldKind::Int {
                        min: 0,
                        max: 7,
                        step: 1,
                    },
                    required: true,
                    default: Some(json!(2)),
                    locked_when: None,
                },
                FieldDef {
                    key: "grounding".into(),
                    label: "Grounding".into(),
                    help: Some(
                        "When on, the prompt may be grounded with web search and image search."
                            .into(),
                    ),
                    kind: FieldKind::Bool,
                    required: true,
                    default: Some(json!(true)),
                    locked_when: None,
                },
                FieldDef {
                    key: "version".into(),
                    label: "Version".into(),
                    help: None,
                    kind: FieldKind::ReadOnly,
                    required: true,
                    default: Some(json!("latest")),
                    locked_when: Some(LockRule::Always {
                        value: json!("latest"),
                    }),
                },
            ],
            cross_rules: Vec::new(),
        },
    }
}

fn validate_scene(
    scene: &crate::scene::Scene,
    values: &std::collections::BTreeMap<String, serde_json::Value>,
    image_count: usize,
) -> Vec<crate::error::FieldError> {
    use crate::error::FieldError;
    use crate::providers::Provider;
    use crate::scene::BoxKind;
    use crate::schema::RuleContext;

    let spec = Flux3.spec();
    let ctx = RuleContext {
        has_source_image: scene.source_image.is_some(),
        image_count,
        box_count: scene.boxes.len(),
    };
    let mut effective = values.clone();
    for field in &spec.schema.fields {
        if let Some(locked) = spec.schema.locked_value(field, &ctx) {
            effective.insert(field.key.clone(), locked);
        }
    }
    let mut errors = spec.schema.validate(&effective, &ctx);
    if let Some(error) = spec.capabilities.validate_image_count(image_count) {
        errors.push(error);
    }
    if scene.prose.trim().is_empty() {
        errors.push(FieldError {
            key: "prompt".into(),
            message: "Prompt is empty".into(),
            approved_max: None,
        });
    }
    let unmatched = crate::prose::unmatched(&scene.prose, scene);
    if let Some(tag) = unmatched.tags_without_box.first() {
        errors.push(FieldError {
            key: "prompt".into(),
            message: format!("<{tag}> has no matching box"),
            approved_max: None,
        });
    }
    let ratio = effective
        .get("aspect_ratio")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("auto");
    if scene.source_image.is_none() && !scene.boxes.is_empty() && ratio == "auto" {
        errors.push(FieldError {
            key: "aspect_ratio".into(),
            message: "Boxes need a fixed aspect ratio".into(),
            approved_max: None,
        });
    }
    let allowed = scene.allowed_kinds();
    for element in &scene.boxes {
        if !allowed.contains(&element.kind) {
            errors.push(FieldError {
                key: "boxes".into(),
                message: format!("{} cannot be used in this scene", kind_name(element.kind)),
                approved_max: None,
            });
        }
        if element.kind == BoxKind::Move && element.tgt.is_none() {
            errors.push(FieldError {
                key: "boxes".into(),
                message: format!("{} needs a target box", element.id.as_str()),
                approved_max: None,
            });
        }
    }
    errors
}

fn kind_name(kind: crate::scene::BoxKind) -> &'static str {
    match kind {
        crate::scene::BoxKind::Layout => "Layout",
        crate::scene::BoxKind::New => "New",
        crate::scene::BoxKind::Keep => "Keep",
        crate::scene::BoxKind::Move => "Move",
        crate::scene::BoxKind::Remove => "Remove",
    }
}

fn build_submit(
    request: &crate::job::JobRequest,
    images: &[crate::providers::ImageInput],
) -> crate::http::HttpRequest {
    use crate::http::{Body, HttpHeader, HttpRequest, Method};
    use crate::model_spec::ProviderId;
    use crate::providers::ImageInput;

    let mut body = serde_json::Map::new();
    body.insert(
        "prompt".into(),
        serde_json::Value::String(request.compiled_prompt.clone()),
    );
    for key in [
        "aspect_ratio",
        "resolution",
        "safety_tolerance",
        "grounding",
        "version",
    ] {
        if let Some(value) = request.params.get(key) {
            body.insert(key.into(), value.clone());
        }
    }
    if !images.is_empty() {
        let encoded = images
            .iter()
            .map(|image| match image {
                ImageInput::RawBase64(text) | ImageInput::Url(text) => {
                    serde_json::Value::String(text.clone())
                }
            })
            .collect();
        body.insert("images".into(), serde_json::Value::Array(encoded));
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
        body: Body::Json(serde_json::Value::Object(body)),
        max_body_bytes: 100 * 1024 * 1024,
        redact_query: false,
    }
}

#[cfg(test)]
mod tests {
    use super::compile_prompt;
    use crate::geometry::{Rect, Size};
    use crate::scene::{BoxKind, ImageRef, Scene};
    use serde_json::Value;

    fn prompt_fixture(name: &str) -> String {
        let raw = match name {
            "layout" => include_str!("../../../tests/fixtures/flux3/layout.json"),
            "edit_boxes" => include_str!("../../../tests/fixtures/flux3/edit_boxes.json"),
            "move" => include_str!("../../../tests/fixtures/flux3/move.json"),
            _ => panic!("unknown fixture {name}"),
        };
        let value: Value = serde_json::from_str(raw).unwrap();
        value["prompt"].as_str().unwrap().to_string()
    }

    fn bbox_rect(value: &Value) -> Rect {
        let [top, left, bottom, right] = [
            value[0].as_i64().unwrap() as i32,
            value[1].as_i64().unwrap() as i32,
            value[2].as_i64().unwrap() as i32,
            value[3].as_i64().unwrap() as i32,
        ];
        Rect {
            x: left,
            y: top,
            w: right - left,
            h: bottom - top,
        }
    }

    fn scene_from_example(prompt: &str, source: bool) -> Scene {
        let (prose, rows_json) = prompt.split_once(" [").expect("json suffix");
        let rows: Vec<Value> = serde_json::from_str(&format!("[{rows_json}")).unwrap();
        let mut scene = Scene::empty();
        scene.prose = prose.to_string();
        if source {
            scene.set_source(ImageRef::new("source.png", 1000, 1000));
            scene.aspect_ratio = Some("auto".into());
        } else {
            scene.aspect_ratio = Some("1:1".into());
        }
        for row in rows {
            let id = row["id"].as_str().unwrap();
            let desc = row["desc"].as_str().unwrap().to_string();
            if let Some(bbox) = row.get("bbox") {
                let created = scene.add_box(bbox_rect(bbox), BoxKind::Layout);
                scene.rename_box(&created, id).unwrap();
                scene.boxes.last_mut().unwrap().desc = desc;
            } else {
                let src = row.get("src_bbox").filter(|value| !value.is_null());
                let tgt = row.get("tgt_bbox").filter(|value| !value.is_null());
                let kind = match (src, tgt) {
                    (None, Some(_)) => BoxKind::New,
                    (Some(_), None) => BoxKind::Remove,
                    (Some(src), Some(tgt)) if src == tgt => BoxKind::Keep,
                    (Some(_), Some(_)) => BoxKind::Move,
                    _ => panic!("row has no boxes"),
                };
                let anchor = src.or(tgt).unwrap();
                let created = scene.add_box(bbox_rect(anchor), kind);
                scene.rename_box(&created, id).unwrap();
                let element = scene
                    .boxes
                    .iter_mut()
                    .find(|element| element.id.as_str() == id)
                    .unwrap();
                element.desc = desc;
                element.src = src.map(bbox_rect);
                element.tgt = tgt.map(bbox_rect);
                element.from_image = row["from"]
                    .as_str()
                    .map(|from| from.trim_start_matches("ref_image_").parse().unwrap());
            }
        }
        scene
    }

    #[test]
    fn layout_example_matches_the_doc_prompt() {
        let prompt = prompt_fixture("layout");
        let scene = scene_from_example(&prompt, false);
        let compiled = compile_prompt(&scene, scene.frame().unwrap());
        assert_eq!(compiled.text, prompt);
    }

    #[test]
    fn edit_example_matches_the_doc_prompt() {
        let prompt = prompt_fixture("edit_boxes");
        let scene = scene_from_example(&prompt, true);
        let compiled = compile_prompt(&scene, scene.frame().unwrap());
        assert_eq!(compiled.text, prompt);
    }

    #[test]
    fn move_example_matches_the_doc_prompt() {
        let prompt = prompt_fixture("move");
        let scene = scene_from_example(&prompt, true);
        let compiled = compile_prompt(&scene, scene.frame().unwrap());
        assert_eq!(compiled.text, prompt);
    }

    #[test]
    fn remove_row_sets_tgt_bbox_null() {
        let mut scene = Scene::empty();
        scene.set_source(ImageRef::new("source.png", 1000, 1000));
        scene.prose = "remove <scrap_1>".into();
        let id = scene.add_box(
            Rect {
                x: 10,
                y: 20,
                w: 30,
                h: 40,
            },
            BoxKind::Remove,
        );
        scene.rename_box(&id, "scrap_1").unwrap();
        scene.update_box(&id, |element| element.desc = "the scrap".into());
        let compiled = compile_prompt(&scene, Size { w: 1000, h: 1000 });
        let rows: Value = serde_json::from_str(&compiled.rows_json).unwrap();
        assert_eq!(rows[0]["from"], "ref_image_0");
        assert_eq!(rows[0]["src_bbox"], serde_json::json!([20, 10, 60, 40]));
        assert!(rows[0]["tgt_bbox"].is_null());
    }

    #[test]
    fn grid_order_is_top_left_bottom_right() {
        let mut scene = Scene::empty();
        scene.set_source(ImageRef::new("frame.png", 1920, 1080));
        scene.prose = "place <mark_1>".into();
        let id = scene.add_box(
            Rect {
                x: 384,
                y: 108,
                w: 1536 - 384,
                h: 972 - 108,
            },
            BoxKind::New,
        );
        scene.rename_box(&id, "mark_1").unwrap();
        let compiled = compile_prompt(&scene, Size { w: 1920, h: 1080 });
        assert!(compiled.text.contains("[100,200,900,800]"));
    }

    #[test]
    fn prose_without_boxes_has_no_json_suffix() {
        let mut scene = Scene::empty();
        scene.prose = "a quiet harbor".into();
        let compiled = compile_prompt(&scene, Size { w: 1000, h: 1000 });
        assert_eq!(compiled.text, "a quiet harbor");
        assert!(compiled.rows_json.is_empty());
    }

    #[test]
    fn a_tiny_new_box_warns() {
        let mut scene = Scene::empty();
        scene.set_source(ImageRef::new("source.png", 1000, 1000));
        scene.prose = "add <speck_1>".into();
        let id = scene.add_box(
            Rect {
                x: 0,
                y: 0,
                w: 20,
                h: 10,
            },
            BoxKind::New,
        );
        scene.rename_box(&id, "speck_1").unwrap();
        let compiled = compile_prompt(&scene, Size { w: 1000, h: 1000 });
        assert!(
            compiled
                .warnings
                .iter()
                .any(|warning| warning.contains("speck_1") && warning.contains("40"))
        );
    }

    #[test]
    fn auto_ratio_with_a_ratio_in_the_prompt_warns_only_when_boxes_exist() {
        let mut scene = Scene::empty();
        scene.set_source(ImageRef::new("source.png", 1000, 1000));
        scene.aspect_ratio = Some("auto".into());
        scene.prose = "make this a wide 16:9 shot".into();
        scene.add_box(
            Rect {
                x: 0,
                y: 0,
                w: 100,
                h: 100,
            },
            BoxKind::Keep,
        );
        let compiled = compile_prompt(&scene, scene.frame().unwrap());
        assert!(
            compiled
                .warnings
                .iter()
                .any(|warning| warning.to_ascii_lowercase().contains("aspect"))
        );

        scene.boxes.clear();
        let compiled = compile_prompt(&scene, scene.frame().unwrap());
        assert!(
            !compiled
                .warnings
                .iter()
                .any(|warning| warning.to_ascii_lowercase().contains("ratio"))
        );
    }

    #[test]
    fn compile_prompt_grids_each_tile_against_its_own_size() {
        let mut scene = Scene::empty();
        scene.set_source(ImageRef::new("a.png", 100, 100));
        scene.add_reference(ImageRef::new("b.png", 400, 200));
        scene.prose = "keep <box_1> and <box_2>".into();
        let spot = Rect {
            x: 10,
            y: 10,
            w: 10,
            h: 10,
        };
        let source_id = scene.add_box(spot, BoxKind::Keep);
        scene.rename_box(&source_id, "box_1").unwrap();
        scene.boxes.push(crate::scene::BoxElement {
            id: crate::scene::BoxId::parse("box_2").unwrap(),
            kind: BoxKind::Keep,
            desc: String::new(),
            src: Some(spot),
            tgt: Some(spot),
            from_image: Some(1),
        });
        let compiled = compile_prompt(&scene, Size { w: 100, h: 100 });
        let rows: Value = serde_json::from_str(&compiled.rows_json).unwrap();
        assert_eq!(rows[0]["src_bbox"], serde_json::json!([100, 100, 200, 200]));
        assert_eq!(rows[1]["src_bbox"], serde_json::json!([50, 25, 100, 50]));
    }

    #[test]
    fn unset_from_image_on_a_new_box_is_not_sent_as_the_source() {
        let mut scene = Scene::empty();
        scene.set_source(ImageRef::new("a.png", 100, 100));
        scene.prose = "add <box_1>".into();
        scene.boxes.push(crate::scene::BoxElement {
            id: crate::scene::BoxId::parse("box_1").unwrap(),
            kind: BoxKind::New,
            desc: String::new(),
            src: None,
            tgt: Some(Rect {
                x: 10,
                y: 10,
                w: 80,
                h: 80,
            }),
            from_image: None,
        });
        let compiled = compile_prompt(&scene, Size { w: 100, h: 100 });
        assert!(!compiled.text.contains("ref_image_0"));
        assert!(
            compiled
                .warnings
                .iter()
                .any(|warning| warning.contains("box_1"))
        );
    }
}

#[cfg(test)]
mod provider_tests {
    use super::Flux3;
    use crate::error::ProviderError;
    use crate::http::{Body, Method};
    use crate::job::{EncodedImage, JobRequest};
    use crate::model_spec::ProviderId;
    use crate::providers::{ImageInput, PollOutcome, Provider, ReferenceStep};
    use crate::scene::{BoxKind, ImageRef, Scene};
    use serde_json::{Value, json};
    use std::collections::BTreeMap;

    fn encoded(bytes: &[u8]) -> EncodedImage {
        EncodedImage {
            media_type: "image/jpeg".into(),
            bytes: bytes.to_vec(),
            width: 300,
            height: 300,
            content_hash: "h".into(),
        }
    }

    fn values() -> BTreeMap<String, Value> {
        Flux3.spec().schema.defaults()
    }

    fn request(prompt: &str, params: BTreeMap<String, Value>) -> JobRequest {
        JobRequest {
            model_id: "flux-3-image".into(),
            compiled_prompt: prompt.into(),
            params,
            images: Vec::new(),
            box_snapshot: Vec::new(),
            created_at: "2026-10-15T00:00:00Z".into(),
            conversation: None,
        }
    }

    #[test]
    fn submit_body_lists_only_schema_keys_and_omits_empty_images() {
        let body = Flux3.build_submit(&request("a harbor", values()), &[]);
        assert_eq!(body.method, Method::Post);
        assert_eq!(body.auth, Some(ProviderId::Bfl));
        assert!(body.headers.iter().any(|header| {
            header.name.eq_ignore_ascii_case("content-type") && header.value == "application/json"
        }));
        let Body::Json(json) = &body.body else {
            panic!("expected json");
        };
        let mut keys: Vec<_> = json.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            vec![
                "aspect_ratio".to_string(),
                "grounding".to_string(),
                "prompt".to_string(),
                "resolution".to_string(),
                "safety_tolerance".to_string(),
                "version".to_string(),
            ]
        );
        assert!(json.get("images").is_none());
    }

    #[test]
    fn images_are_raw_base64_in_reference_order() {
        let inputs = [
            ImageInput::RawBase64("YQ==".into()),
            ImageInput::RawBase64("Yg==".into()),
        ];
        let body = Flux3.build_submit(&request("edit", values()), &inputs);
        let Body::Json(json) = body.body else {
            panic!("json");
        };
        assert_eq!(json["images"], json!(["YQ==", "Yg=="]));
        assert!(!json.to_string().contains("data:"));
    }

    #[test]
    fn validation_covers_ratio_images_prompt_tags_and_box_shape() {
        let mut scene = Scene::empty();
        scene.aspect_ratio = Some("auto".into());
        scene.add_box(
            crate::geometry::Rect {
                x: 0,
                y: 0,
                w: 10,
                h: 10,
            },
            BoxKind::Layout,
        );
        let mut params = values();
        params.insert("aspect_ratio".into(), json!("auto"));
        let errors = Flux3.validate_for_submit(&scene, &params, 0);
        assert!(errors.iter().any(|error| {
            error.key == "aspect_ratio" && error.message.contains("Boxes need a fixed aspect ratio")
        }));

        scene.set_source(ImageRef::new("src.png", 1000, 1000));
        scene.prose.clear();
        let errors = Flux3.validate_for_submit(&scene, &values(), 0);
        assert!(errors.iter().any(|error| error.key == "prompt"));
        assert!(errors.iter().any(|error| error.key == "boxes"));

        scene.boxes.clear();
        scene.prose = "a fox".into();
        assert!(Flux3.validate_for_submit(&scene, &values(), 0).is_empty());

        scene.prose = "see <dog_1>".into();
        let errors = Flux3.validate_for_submit(&scene, &values(), 11);
        assert!(errors.iter().any(|error| error.key == "images"));
        assert!(errors.iter().any(|error| error.key == "prompt"));

        scene.prose = "move <box_1>".into();
        scene.boxes.clear();
        scene.add_box(
            crate::geometry::Rect {
                x: 0,
                y: 0,
                w: 40,
                h: 40,
            },
            BoxKind::Move,
        );
        let errors = Flux3.validate_for_submit(&scene, &values(), 1);
        assert!(errors.iter().any(|error| error.key == "boxes"));
    }

    #[test]
    fn a_layout_box_stays_layout_inside_the_first_source() {
        let mut scene = Scene::empty();
        scene.aspect_ratio = Some("1:1".into());
        scene.prose = "place <box_1>".into();
        scene.add_box(
            crate::geometry::Rect {
                x: 100,
                y: 100,
                w: 200,
                h: 200,
            },
            BoxKind::Layout,
        );
        scene.set_source(ImageRef::new("sq.png", 500, 500));
        assert_eq!(scene.boxes[0].kind, BoxKind::Layout);
        let rect = scene.boxes[0].src.unwrap();
        assert!(rect.x >= 0 && rect.y >= 0);
        assert!(rect.x + rect.w <= 500 && rect.y + rect.h <= 500);
        let errors = Flux3.validate_for_submit(&scene, &values(), 1);
        assert!(
            errors
                .iter()
                .any(|error| error.message.contains("cannot be used"))
        );
    }

    #[test]
    fn reference_steps_are_inline_and_cancel_is_absent() {
        let steps = Flux3.reference_steps(&[encoded(b"a"), encoded(b"b"), encoded(b"c")]);
        assert_eq!(steps.len(), 3);
        assert!(
            steps
                .iter()
                .all(|step| matches!(step, ReferenceStep::Inline(ImageInput::RawBase64(_))))
        );
        assert!(
            Flux3
                .cancel_request(
                    &crate::providers::Submitted {
                        job_id: crate::providers::JobId::parse("abc").unwrap(),
                        poll_url: "https://api.bfl.ai/v1/get_result?id=abc".parse().unwrap(),
                        quote: None,
                    },
                    None,
                )
                .is_none()
        );
        assert_eq!(Flux3.poll_policy(), crate::job::PollPolicy::default_bfl());
    }

    #[test]
    fn poll_uses_the_url_verbatim_and_download_is_anonymous() {
        let submitted = crate::providers::Submitted {
            job_id: crate::providers::JobId::parse("abc").unwrap(),
            poll_url: "https://api.eu.bfl.ai/v1/get_result?id=abc"
                .parse()
                .unwrap(),
            quote: None,
        };
        let poll = Flux3.poll_request(&submitted);
        assert_eq!(poll.auth, Some(ProviderId::Bfl));
        assert_eq!(poll.url, submitted.poll_url.as_str());
        let download = Flux3.download_request(&"https://delivery.bfl.ai/x.png".parse().unwrap());
        assert_eq!(download.auth, None);
        assert_eq!(download.method, Method::Get);
    }

    #[test]
    fn f1_body_snapshot_has_no_credential() {
        let mut params = values();
        params.insert("aspect_ratio".into(), json!("auto"));
        let body = Flux3.build_submit(
            &request("In <ref_image_0>, change <animal_1> to pink. []", params),
            &[ImageInput::RawBase64("aGVsbG8=".into())],
        );
        let Body::Json(json) = &body.body else {
            panic!("json");
        };
        insta::with_settings!({ sort_maps => true }, {
            insta::assert_json_snapshot!(json);
        });
        let rendered = serde_json::to_string(json).unwrap();
        assert!(!rendered.contains("x-key"));
        assert!(!format!("{body:?}").contains("aGVsbG8="));
    }

    #[test]
    fn default_provider_strips_tags() {
        let provider = Plain;
        let mut scene = Scene::empty();
        scene.prose = "In <ref_image_0>, change <animal_1> to pink.".into();
        let compiled = provider.compile_prompt(&scene, crate::geometry::Size { w: 1000, h: 1000 });
        assert!(!compiled.text.contains('<'));
        assert!(!compiled.text.contains("  "));
        assert!(
            compiled
                .warnings
                .iter()
                .any(|warning| warning.contains("ref_image_0"))
        );
        assert!(
            compiled
                .warnings
                .iter()
                .any(|warning| warning.contains("animal_1"))
        );

        scene.prose = "a quiet harbor".into();
        let compiled = provider.compile_prompt(&scene, crate::geometry::Size { w: 1000, h: 1000 });
        assert_eq!(compiled.text, "a quiet harbor");
        assert!(compiled.warnings.is_empty());
    }

    struct Plain;

    impl Provider for Plain {
        fn spec(&self) -> &crate::model_spec::ModelSpec {
            Flux3.spec()
        }

        fn validate_for_submit(
            &self,
            _: &Scene,
            _: &BTreeMap<String, Value>,
            _: usize,
        ) -> Vec<crate::error::FieldError> {
            Vec::new()
        }

        fn reference_steps(&self, _: &[EncodedImage]) -> Vec<ReferenceStep> {
            Vec::new()
        }

        fn build_submit(&self, _: &JobRequest, _: &[ImageInput]) -> crate::http::HttpRequest {
            unreachable!()
        }

        fn poll_request(&self, _: &crate::providers::Submitted) -> crate::http::HttpRequest {
            unreachable!()
        }

        fn parse_submit(
            &self,
            _: &crate::http::HttpResponse,
        ) -> Result<crate::providers::Submitted, ProviderError> {
            Err(ProviderError::Protocol("no".into()))
        }

        fn parse_poll(&self, _: &crate::http::HttpResponse) -> PollOutcome {
            PollOutcome::Cancelled
        }
    }
}
