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
        .map(|element| row_for(element, frame))
        .collect();
    let rows_json = serde_json::to_string(&rows).unwrap_or_else(|_| "[]".to_string());
    CompiledPrompt {
        text: format!("{} {rows_json}", scene.prose),
        rows_json,
        warnings,
    }
}

fn row_for(element: &BoxElement, frame: Size) -> Row<'_> {
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
            from: Some(from_label(element)),
            src_bbox: element.src.map(|rect| grid(rect, frame)),
            tgt_bbox: element.tgt.map(|rect| grid(rect, frame)),
            desc: &element.desc,
        }),
        BoxKind::Remove => Row::Edit(EditRow {
            id: element.id.as_str(),
            from: Some(from_label(element)),
            src_bbox: element.src.map(|rect| grid(rect, frame)),
            tgt_bbox: None,
            desc: &element.desc,
        }),
    }
}

fn from_label(element: &BoxElement) -> String {
    format!("ref_image_{}", element.from_image.unwrap_or(0))
}

fn grid(rect: Rect, frame: Size) -> [u16; 4] {
    rect.to_grid(frame).0
}

fn warnings_for(scene: &Scene, frame: Size) -> Vec<String> {
    let mut warnings = Vec::new();
    for element in &scene.boxes {
        if element.kind != BoxKind::New {
            continue;
        }
        let Some(rect) = element.tgt.or(element.src) else {
            continue;
        };
        let [top, left, bottom, right] = grid(rect, frame);
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
}
