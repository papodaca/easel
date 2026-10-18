use std::sync::Arc;

use easel_core::geometry::Size;
use easel_core::history::LoadedRequest;
use easel_core::scene::BoxElement;
use gpui::Context;

use crate::app_state::AppState;
use crate::image_load::decode_image;

#[derive(Clone, Debug, PartialEq)]
pub struct Inspection {
    pub prompt: String,
    pub suggestion: String,
    pub rows: Vec<(String, String)>,
    pub boxes: Vec<BoxElement>,
    pub frame: Option<Size>,
}

pub fn promote(app: &mut AppState, bytes: &[u8], cx: &mut Context<AppState>) -> Result<(), String> {
    let image = decode_image(bytes, "result").map_err(|err| err.to_string())?;
    app.set_image(Arc::new(image), cx);
    app.scene.collapse_move_boxes();
    Ok(())
}

pub fn inspection_from(loaded: &LoadedRequest) -> Inspection {
    let mut rows = Vec::new();
    let mut keys: Vec<_> = loaded.params.keys().cloned().collect();
    keys.sort();
    for key in keys {
        if let Some(value) = loaded.params.get(&key) {
            rows.push((key, value.to_string()));
        }
    }
    for (index, image) in loaded.images.iter().enumerate() {
        rows.push((
            format!("ref_image_{index}"),
            format!(
                "{}x{} {} {}",
                image.width, image.height, image.mime, image.content_hash
            ),
        ));
    }
    Inspection {
        prompt: loaded.compiled_prompt.clone(),
        suggestion: String::new(),
        rows,
        boxes: loaded.box_snapshot.clone(),
        frame: loaded
            .images
            .first()
            .map(|image| Size {
                w: image.width,
                h: image.height,
            })
            .filter(|size| size.w > 0 && size.h > 0),
    }
}
