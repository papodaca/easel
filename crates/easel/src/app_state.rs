use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use easel_core::geometry::Rect;
use easel_core::providers::bfl::flux3::Flux3;
use easel_core::providers::{JobId, Provider};
use easel_core::scene::{BoxId, BoxKind, ImageRef, Scene};
use gpui::{Context, EventEmitter, RenderImage};
use serde_json::Value;

use crate::image_load::DecodedImage;
use crate::notifications::{Notifications, Toast, ToastLevel};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Box,
}

/// The history row the modal is showing. A queued job has a token and no id yet.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HistoryItem {
    pub token: Option<u64>,
    pub id: Option<JobId>,
}

/// The finished turn the next Generate should refine.
#[derive(Clone)]
pub struct ContinueFrom {
    pub job_id: JobId,
    #[allow(dead_code)]
    pub model_id: String,
    pub provider_id: String,
    pub image: Arc<DecodedImage>,
    pub prompt: String,
}

pub struct AppState {
    pub settings_open: bool,
    pub history_item: Option<HistoryItem>,
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
    pub extra_refs: Vec<Option<easel_core::session::SourceRef>>,
    pub sheet_note: Option<String>,
    retired_renders: Vec<Arc<RenderImage>>,
    krea_sunset_noted: bool,
    pub model_id: String,
    pub conversation_mode: bool,
    pub continuing: Option<ContinueFrom>,
    last_by_provider: HashMap<easel_core::model_spec::ProviderId, String>,
}

impl AppState {
    pub fn new() -> Self {
        let mut app = Self {
            settings_open: false,
            history_item: None,
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
            extra_refs: Vec::new(),
            sheet_note: None,
            retired_renders: Vec::new(),
            krea_sunset_noted: false,
            model_id: easel_core::providers::bfl::flux3::Flux3.spec().id.clone(),
            conversation_mode: false,
            continuing: None,
            last_by_provider: HashMap::new(),
        };
        app.remember_model();
        app
    }

    pub fn remember_model(&mut self) {
        let Some(spec) = easel_core::providers::ModelRegistry::specs()
            .iter()
            .copied()
            .find(|spec| spec.id == self.model_id)
        else {
            return;
        };
        self.last_by_provider.insert(spec.provider, spec.id.clone());
    }

    pub fn model_for_provider(
        &self,
        provider: easel_core::model_spec::ProviderId,
    ) -> Option<String> {
        self.last_by_provider.get(&provider).cloned().or_else(|| {
            easel_core::providers::ModelRegistry::specs()
                .iter()
                .find(|spec| spec.provider == provider)
                .map(|spec| spec.id.clone())
        })
    }

    pub fn spec(&self) -> &'static easel_core::model_spec::ModelSpec {
        easel_core::providers::ModelRegistry::specs()
            .iter()
            .copied()
            .find(|spec| spec.id == self.model_id)
            .unwrap_or_else(|| easel_core::providers::bfl::flux3::model_spec())
    }

    pub fn set_model(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(spec) = easel_core::providers::ModelRegistry::specs()
            .iter()
            .copied()
            .find(|spec| spec.id == id)
        else {
            return;
        };
        if spec.id == self.model_id {
            return;
        }
        let outgoing_defaults = self.spec().schema.defaults();
        let incoming_defaults = spec.schema.defaults();
        let mut values = incoming_defaults.clone();
        for (key, value) in &self.values {
            if outgoing_defaults.get(key) == Some(value) && incoming_defaults.contains_key(key) {
                continue;
            }
            if spec.schema.accepts(key, value) {
                values.insert(key.clone(), value.clone());
            }
        }
        self.model_id = spec.id.clone();
        self.continuing = None;
        self.remember_model();
        self.values = values;
        if !spec.capabilities.supports_boxes && self.tool == Tool::Box {
            self.tool = Tool::Select;
        }
        cx.notify();
    }

    /// How many loaded images this model would actually send.
    /// Leonardo multi-guidance models with a one-image slot selected send only
    /// the first prepared image; every other model sends up to its slot maximum.
    pub fn max_sent_images(&self) -> usize {
        let max = self.spec().capabilities.image_slots.max();
        if self.spec().provider != easel_core::model_spec::ProviderId::Leonardo {
            return max;
        }
        // On multi-guidance models only the style slot takes more than one
        // image; every other selected slot sends just the first.
        let style_or_unset = matches!(
            self.values.get("guidance_type").and_then(Value::as_str),
            None | Some("style")
        );
        if style_or_unset { max } else { 1 }
    }

    pub fn reconcile_values(
        &self,
        stored: std::collections::BTreeMap<String, Value>,
    ) -> std::collections::BTreeMap<String, Value> {
        let mut values = self.spec().schema.defaults();
        for (key, value) in stored {
            if self.spec().schema.accepts(&key, &value) {
                values.insert(key, value);
            }
        }
        values
    }

    pub fn field_errors(&self, sent_images: usize) -> Vec<easel_core::error::FieldError> {
        let Some(provider) = easel_core::providers::resolve_model(&self.model_id) else {
            return vec![easel_core::error::FieldError {
                key: "model".into(),
                message: format!("unknown model {}", self.model_id),
                approved_max: None,
            }];
        };
        provider.validate_for_submit(&self.scene, &self.values, sent_images)
    }

    pub fn compiled(&self) -> easel_core::providers::bfl::flux3::CompiledPrompt {
        let frame = self
            .scene
            .frame()
            .unwrap_or(easel_core::geometry::Size { w: 1000, h: 1000 });
        let Some(provider) = easel_core::providers::resolve_model(&self.model_id) else {
            return easel_core::providers::bfl::flux3::CompiledPrompt {
                text: self.scene.prose.clone(),
                rows_json: String::new(),
                warnings: Vec::new(),
            };
        };
        provider.compile_prompt(&self.scene, frame)
    }

    pub fn reference_marks(&self) -> Vec<(String, bool)> {
        use easel_core::model_spec::ImageSlots;

        let count = usize::from(self.scene.source_image.is_some()) + self.scene.references.len();
        let slots = &self.spec().capabilities.image_slots;
        let max = self.max_sent_images();
        (0..count)
            .map(|index| {
                let label = match slots {
                    ImageSlots::Numbered { .. } => {
                        if index == 0 {
                            "input_image".to_owned()
                        } else {
                            format!("input_image_{}", index + 1)
                        }
                    }
                    ImageSlots::SingleWithStyleRefs { .. } => {
                        if index == 0 {
                            "image_url".to_owned()
                        } else {
                            format!("style {index}")
                        }
                    }
                    ImageSlots::List { key, .. } if key == "image_urls" => {
                        format!("image_urls[{index}]")
                    }
                    ImageSlots::List { .. } | ImageSlots::Single | ImageSlots::None => {
                        format!("ref_image_{index}")
                    }
                };
                let sent = index < max;
                let label = if sent {
                    label
                } else {
                    format!("{label} not sent")
                };
                (label, sent)
            })
            .collect()
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
        self.scene.frame().is_some() && self.spec().capabilities.supports_boxes
    }

    pub fn set_conversation_mode(&mut self, on: bool, cx: &mut Context<Self>) {
        self.conversation_mode = on;
        if !on {
            self.continuing = None;
        }
        cx.notify();
    }

    pub fn supports_conversation(&self) -> bool {
        easel_core::providers::resolve_model(&self.model_id)
            .is_some_and(|provider| provider.supports_conversation())
    }

    pub fn conversation_help(&self) -> String {
        if let Some(continued) = &self.continuing {
            let prompt: String = continued.prompt.chars().take(40).collect();
            return format!("Continues from: {prompt}");
        }
        format!(
            "{} keeps this turn's prompt and images so it can be continued. Easel can't delete them.",
            self.spec().provider.display_name()
        )
    }

    pub fn set_image(&mut self, image: Arc<DecodedImage>, cx: &mut Context<Self>) {
        if self
            .continuing
            .as_ref()
            .is_some_and(|continued| !Arc::ptr_eq(&continued.image, &image))
        {
            self.continuing = None;
        }
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
    ) -> Result<(), String> {
        let count = usize::from(self.scene.source_image.is_some()) + self.scene.references.len();
        let max = self.spec().capabilities.image_slots.max();
        if count >= max {
            return Err(format!("accepts at most {max} images"));
        }
        self.scene
            .add_reference(ImageRef::new("reference", image.size.w, image.size.h));
        self.extras.push(image);
        cx.notify();
        Ok(())
    }

    pub fn remove_reference_slot(&mut self, index: usize, cx: &mut Context<Self>) {
        let (mut pixels, mut refs) = self.slot_buffers();
        if index < pixels.len() {
            let removed = pixels.remove(index);
            self.retire(removed);
            refs.remove(index);
        }
        self.scene.remove_reference(index);
        self.write_slots(pixels, refs);
        self.sheet_note = None;
        cx.notify();
    }

    pub fn move_slot(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
        let (mut pixels, mut refs) = self.slot_buffers();
        if from >= pixels.len() || to >= pixels.len() || from == to {
            return;
        }
        let image = pixels.remove(from);
        pixels.insert(to, image);
        let reference = refs.remove(from);
        refs.insert(to, reference);
        self.scene.move_reference(from, to);
        self.write_slots(pixels, refs);
        cx.notify();
    }

    fn slot_buffers(
        &self,
    ) -> (
        Vec<Option<Arc<DecodedImage>>>,
        Vec<Option<easel_core::session::SourceRef>>,
    ) {
        let mut pixels = Vec::new();
        let mut refs = Vec::new();
        if self.scene.source_image.is_some() {
            pixels.push(self.image.clone());
            refs.push(self.source_ref.clone());
        }
        pixels.extend(self.extras.iter().cloned().map(Some));
        refs.extend(self.extra_refs.iter().cloned());
        while refs.len() < pixels.len() {
            refs.push(None);
        }
        refs.truncate(pixels.len());
        (pixels, refs)
    }

    fn write_slots(
        &mut self,
        pixels: Vec<Option<Arc<DecodedImage>>>,
        refs: Vec<Option<easel_core::session::SourceRef>>,
    ) {
        let mut pixels = pixels.into_iter();
        let mut refs = refs.into_iter();
        if self.scene.source_image.is_some() {
            self.image = pixels.next().flatten();
            self.source_ref = refs.next().flatten();
        } else {
            let previous = self.image.take();
            self.retire(previous);
            self.source_ref = None;
        }
        self.extras = pixels.flatten().collect();
        self.extra_refs = refs.collect();
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

    pub fn set_history_item(&mut self, item: Option<HistoryItem>, cx: &mut Context<Self>) {
        if self.history_item == item {
            return;
        }
        self.history_item = item;
        cx.notify();
    }

    pub fn set_settings_open(&mut self, open: bool, cx: &mut Context<Self>) {
        if self.settings_open == open {
            return;
        }
        self.settings_open = open;
        cx.notify();
    }

    pub fn toggle_settings(&mut self, cx: &mut Context<Self>) {
        self.set_settings_open(!self.settings_open, cx);
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
    fn switching_models_adopts_defaults_for_untouched_fields(cx: &mut TestAppContext) {
        let app = cx.new(|_| AppState::new());
        app.update(cx, |app, cx| {
            app.set_model("chutes-flux-schnell", cx);
            assert_eq!(app.values["num_inference_steps"], serde_json::json!(4));
            app.set_model("chutes-qwen-image-2512", cx);
            assert_eq!(app.values["num_inference_steps"], serde_json::json!(30));
            app.values
                .insert("num_inference_steps".into(), serde_json::json!(20));
            app.set_model("chutes-flux-schnell", cx);
            assert_eq!(app.values["num_inference_steps"], serde_json::json!(20));
        });
    }

    #[gpui::test]
    fn max_sent_images_defaults_to_the_slot_maximum(cx: &mut TestAppContext) {
        let app = cx.new(|_| AppState::new());
        app.read_with(cx, |app, _| {
            let max = app.spec().capabilities.image_slots.max();
            assert!(max > 1, "{}", max);
            assert_eq!(app.max_sent_images(), max);
        });
    }

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
