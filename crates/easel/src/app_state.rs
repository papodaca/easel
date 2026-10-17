use std::collections::BTreeMap;
use std::sync::Arc;

use easel_core::geometry::Rect;
use easel_core::providers::Provider;
use easel_core::providers::bfl::flux3::Flux3;
use easel_core::scene::{BoxId, BoxKind, ImageRef, Scene};
use gpui::{Context, EventEmitter, RenderImage};
use serde_json::Value;

use crate::image_load::DecodedImage;
use crate::notifications::{Notifications, Toast, ToastLevel};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideMode {
    Editor,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Box,
}

pub struct AppState {
    pub side_mode: SideMode,
    pub quit_requested: bool,
    pub image: Option<Arc<DecodedImage>>,
    pub extras: Vec<Arc<DecodedImage>>,
    pub scene: Scene,
    #[cfg_attr(not(test), allow(dead_code))]
    pub values: BTreeMap<String, Value>,
    pub tool: Tool,
    pub selected_box: Option<BoxId>,
    pub focus_box_field: bool,
    pub draw_target: Option<BoxId>,
    pub revert_kind: Option<BoxKind>,
    pub saved_src: Option<Rect>,
    pub saved_tgt: Option<Rect>,
    pub inspection: Option<crate::jobs::Inspection>,
    pub source_ref: Option<easel_core::session::SourceRef>,
    retired_renders: Vec<Arc<RenderImage>>,
    krea_sunset_noted: bool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            side_mode: SideMode::Editor,
            quit_requested: false,
            image: None,
            extras: Vec::new(),
            scene: Scene::empty(),
            values: Flux3.spec().schema.defaults(),
            tool: Tool::Select,
            selected_box: None,
            focus_box_field: false,
            draw_target: None,
            revert_kind: None,
            saved_src: None,
            saved_tgt: None,
            inspection: None,
            source_ref: None,
            retired_renders: Vec::new(),
            krea_sunset_noted: false,
        }
    }

    pub fn note_krea_sunset(
        &mut self,
        sunset: easel_core::providers::krea::Sunset,
        notes: &gpui::Entity<Notifications>,
        cx: &mut Context<Self>,
    ) {
        if self.krea_sunset_noted {
            return;
        }
        self.krea_sunset_noted = true;
        let text = easel_core::providers::krea::sunset_toast(&sunset);
        notes.update(cx, |notes, cx| {
            notes.push(
                Toast {
                    level: ToastLevel::Info,
                    text: text.into(),
                    ttl: std::time::Duration::from_secs(8),
                    action: None,
                },
                cx,
            );
        });
    }

    pub fn box_tool_enabled(&self) -> bool {
        self.scene.frame().is_some()
    }

    pub fn set_image(&mut self, image: Arc<DecodedImage>, cx: &mut Context<Self>) {
        self.source_ref = None;
        self.scene
            .set_source(ImageRef::new("source", image.size.w, image.size.h));
        if let Some(previous) = self.image.replace(image) {
            self.retired_renders.push(Arc::clone(&previous.render));
        }
        cx.notify();
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn add_extra(
        &mut self,
        image: Arc<DecodedImage>,
        cx: &mut Context<Self>,
    ) -> Result<(), &'static str> {
        let count = usize::from(self.scene.source_image.is_some()) + self.scene.references.len();
        if count >= 10 {
            return Err("FLUX 3 accepts at most 10 images");
        }
        self.scene
            .add_reference(ImageRef::new("reference", image.size.w, image.size.h));
        self.extras.push(image);
        cx.notify();
        Ok(())
    }

    pub fn remove_reference_slot(&mut self, index: usize, cx: &mut Context<Self>) {
        let mut pixels = Vec::new();
        if self.scene.source_image.is_some() {
            pixels.push(self.image.clone());
        }
        pixels.extend(self.extras.iter().cloned().map(Some));
        let removed = if index < pixels.len() {
            pixels.remove(index)
        } else {
            None
        };
        self.retire(removed);
        self.scene.remove_reference(index);
        let mut pixels = pixels.into_iter();
        if self.scene.source_image.is_some() {
            self.image = pixels.next().flatten();
        } else {
            let previous = self.image.take();
            self.retire(previous);
        }
        self.extras = pixels.flatten().collect();
        cx.notify();
    }

    fn retire(&mut self, image: Option<Arc<DecodedImage>>) {
        if let Some(image) = image {
            self.retired_renders.push(Arc::clone(&image.render));
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn set_aspect_ratio(&mut self, ratio: &str, cx: &mut Context<Self>) {
        self.scene.aspect_ratio = Some(ratio.to_string());
        self.values
            .insert("aspect_ratio".into(), Value::String(ratio.to_string()));
        cx.notify();
    }

    pub fn take_retired_renders(&mut self) -> Vec<Arc<RenderImage>> {
        std::mem::take(&mut self.retired_renders)
    }

    pub fn toggle_side_mode(&mut self, cx: &mut Context<Self>) {
        self.side_mode = match self.side_mode {
            SideMode::Editor => SideMode::Settings,
            SideMode::Settings => SideMode::Editor,
        };
        cx.notify();
    }

    pub fn request_quit(&mut self, cx: &mut Context<Self>) {
        self.quit_requested = true;
        cx.notify();
    }
}

impl EventEmitter<()> for AppState {}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use easel_core::http::{HttpHeader, HttpResponse};
    use easel_core::providers::krea::{self, Sunset};
    use gpui::{TestAppContext, prelude::*};

    use crate::notifications::Notifications;

    use super::AppState;

    #[gpui::test]
    fn a_sunset_header_toasts_once_per_session(cx: &mut TestAppContext) {
        let response = HttpResponse {
            status: 200,
            headers: vec![
                HttpHeader {
                    name: "Deprecation".into(),
                    value: "true".into(),
                    sensitive: false,
                },
                HttpHeader {
                    name: "Sunset".into(),
                    value: "Sun, 06 Nov 1994 08:49:37 GMT".into(),
                    sensitive: false,
                },
            ],
            body: Vec::new(),
        };
        let sunset = krea::deprecation_headers(&response).expect("sunset");
        assert_eq!(sunset, Sunset::On("06 Nov 1994".into()));
        let notes = cx.new(|_| Notifications::new());
        let app = cx.new(|_| AppState::new());
        app.update(cx, |app, cx| {
            app.note_krea_sunset(sunset.clone(), &notes, cx)
        });
        app.update(cx, |app, cx| app.note_krea_sunset(sunset, &notes, cx));
        assert_eq!(notes.read_with(cx, |notes, _| notes.toast_count()), 1);
        let text = notes.read_with(cx, |notes, _| notes.last_toast_text());
        assert!(text.contains("06 Nov 1994"), "{text}");
        assert!(!text.contains("GMT"), "{text}");
    }
}
