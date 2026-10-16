use std::sync::Arc;

use gpui::{Context, EventEmitter, RenderImage};

use crate::image_load::DecodedImage;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SideMode {
    Editor,
    Settings,
}

pub struct AppState {
    pub side_mode: SideMode,
    pub quit_requested: bool,
    pub image: Option<Arc<DecodedImage>>,
    retired_renders: Vec<Arc<RenderImage>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            side_mode: SideMode::Editor,
            quit_requested: false,
            image: None,
            retired_renders: Vec::new(),
        }
    }

    pub fn set_image(&mut self, image: Arc<DecodedImage>, cx: &mut Context<Self>) {
        if let Some(previous) = self.image.replace(image) {
            self.retired_renders.push(Arc::clone(&previous.render));
        }
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
