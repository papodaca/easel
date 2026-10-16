use std::collections::BTreeMap;
use std::sync::Arc;

use easel_core::geometry::Rect;
use easel_core::providers::Provider;
use easel_core::providers::bfl::flux3::Flux3;
use easel_core::scene::{BoxId, BoxKind, ImageRef, Scene};
use gpui::{Context, EventEmitter, RenderImage};
use serde_json::Value;

use crate::image_load::DecodedImage;

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
    retired_renders: Vec<Arc<RenderImage>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            side_mode: SideMode::Editor,
            quit_requested: false,
            image: None,
            scene: Scene::empty(),
            values: Flux3.spec().schema.defaults(),
            tool: Tool::Select,
            selected_box: None,
            focus_box_field: false,
            draw_target: None,
            revert_kind: None,
            saved_src: None,
            saved_tgt: None,
            retired_renders: Vec::new(),
        }
    }

    pub fn box_tool_enabled(&self) -> bool {
        self.scene.frame().is_some()
    }

    pub fn set_image(&mut self, image: Arc<DecodedImage>, cx: &mut Context<Self>) {
        self.scene
            .set_source(ImageRef::new("source", image.size.w, image.size.h));
        if let Some(previous) = self.image.replace(image) {
            self.retired_renders.push(Arc::clone(&previous.render));
        }
        cx.notify();
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
