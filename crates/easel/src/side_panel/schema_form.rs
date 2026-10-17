use easel_core::providers::Provider;
use easel_core::providers::bfl::flux3::Flux3;
use easel_core::schema::{FieldKind, RuleContext};
use gpui::{
    Context, Entity, IntoElement, ParentElement, Render, Subscription, Window, div, prelude::*, px,
};
use serde_json::{Value, json};

use crate::app_state::AppState;
use crate::ui::section_label;
use crate::widgets::{Checkbox, Select, Slider, TextInput};

pub struct SchemaForm {
    app_state: Entity<AppState>,
    aspect: Entity<Select>,
    resolution: Entity<Select>,
    safety: Entity<Slider>,
    grounding: Entity<Checkbox>,
    version: Entity<TextInput>,
    _observe: Subscription,
}

impl SchemaForm {
    pub fn new(app_state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let spec = Flux3.spec();
        let aspect_options = select_options(&spec.schema, "aspect_ratio");
        let resolution_options = select_options(&spec.schema, "resolution");
        let aspect = cx.new(|cx| Select::new(aspect_options, cx));
        let resolution = cx.new(|cx| Select::new(resolution_options, cx));
        let safety = cx.new(|cx| Slider::new(0.0, 7.0, cx));
        let grounding = cx.new(Checkbox::new);
        let version = cx.new(|cx| {
            let mut input = TextInput::new(cx);
            input.set_enabled(false, cx);
            input.set_text("latest", cx);
            input
        });
        let form = Self {
            app_state: app_state.clone(),
            aspect,
            resolution,
            safety,
            grounding,
            version,
            _observe: cx.observe(&app_state, |_, _, cx| cx.notify()),
        };
        form.apply_defaults(cx);
        form.wire(cx);
        form
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn field_count(&self) -> usize {
        Flux3.spec().schema.fields.len()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn version_is_read_only(&self, cx: &gpui::App) -> bool {
        !self.version.read(cx).is_enabled()
    }

    pub fn value(&self, key: &str, cx: &gpui::App) -> Value {
        let app = self.app_state.read(cx);
        locked_or_stored(app, key)
    }

    #[cfg(test)]
    pub fn aspect_control(&self) -> &Entity<Select> {
        &self.aspect
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn aspect_enabled(&self, cx: &gpui::App) -> bool {
        self.aspect.read(cx).is_enabled()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn safety_max(&self, cx: &gpui::App) -> f64 {
        self.safety.read(cx).max_value()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn safety_value(&self, cx: &gpui::App) -> f64 {
        self.safety.read(cx).value().as_f64().unwrap_or(0.0)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn apply_safety_cap(&mut self, approved_max: i64, cx: &mut Context<Self>) {
        let max = approved_max.clamp(0, 7) as f64;
        let value = self.safety.update(cx, |slider, cx| {
            slider.set_range(0.0, max, cx);
            slider.value().as_f64().unwrap_or(0.0)
        });
        self.app_state.update(cx, |app, cx| {
            app.values.insert("safety_tolerance".into(), json!(value));
            cx.notify();
        });
    }

    fn apply_defaults(&self, cx: &mut Context<Self>) {
        let values = self.app_state.read(cx).values.clone();
        if let Some(value) = values.get("aspect_ratio") {
            self.aspect.update(cx, |select, cx| {
                select.set_value(value.clone(), cx);
            });
        }
        if let Some(value) = values.get("resolution") {
            self.resolution.update(cx, |select, cx| {
                select.set_value(value.clone(), cx);
            });
        }
        if let Some(value) = values.get("safety_tolerance") {
            self.safety
                .update(cx, |slider, cx| slider.set_value(value, cx));
        }
        if let Some(value) = values.get("grounding") {
            self.grounding
                .update(cx, |check, cx| check.set_value(value, cx));
        }
    }

    fn wire(&self, cx: &mut Context<Self>) {
        bind_select(&self.app_state, &self.aspect, "aspect_ratio", cx);
        bind_select(&self.app_state, &self.resolution, "resolution", cx);
        let state = self.app_state.clone();
        self.safety.update(cx, |slider, cx| {
            slider.on_change(
                Box::new(move |value, _window, cx| {
                    let value = value.clone();
                    state.update(cx, |app, cx| {
                        app.values.insert("safety_tolerance".into(), value);
                        cx.notify();
                    });
                }),
                cx,
            );
        });
        let state = self.app_state.clone();
        self.grounding.update(cx, |check, cx| {
            check.on_change(
                Box::new(move |value, _window, cx| {
                    let value = value.clone();
                    state.update(cx, |app, cx| {
                        app.values.insert("grounding".into(), value);
                        cx.notify();
                    });
                }),
                cx,
            );
        });
    }

    fn sync(&mut self, cx: &mut Context<Self>) {
        let value = self.value("aspect_ratio", cx);
        self.aspect.update(cx, |select, cx| {
            select.set_enabled(true, cx);
            select.set_value(value, cx);
        });
    }
}

fn bind_select(
    state: &Entity<AppState>,
    select: &Entity<Select>,
    key: &'static str,
    cx: &mut Context<SchemaForm>,
) {
    let state = state.clone();
    select.update(cx, |select, cx| {
        select.on_change(
            Box::new(move |value, _window, cx| {
                let value = value.clone();
                state.update(cx, |app, cx| {
                    if key == "aspect_ratio"
                        && let Some(ratio) = value.as_str()
                    {
                        app.scene.aspect_ratio = Some(ratio.to_string());
                    }
                    app.values.insert(key.to_string(), value);
                    cx.notify();
                });
            }),
            cx,
        );
    });
}

fn select_options(schema: &easel_core::schema::ParamSchema, key: &str) -> Vec<gpui::SharedString> {
    schema
        .fields
        .iter()
        .find(|field| field.key == key)
        .and_then(|field| match &field.kind {
            FieldKind::Select { options } => Some(
                options
                    .iter()
                    .map(|option| gpui::SharedString::from(option.clone()))
                    .collect(),
            ),
            _ => None,
        })
        .unwrap_or_default()
}

fn context_of(app: &AppState) -> RuleContext {
    RuleContext {
        has_source_image: app.scene.source_image.is_some(),
        image_count: usize::from(app.scene.source_image.is_some()) + app.scene.references.len(),
        box_count: app.scene.boxes.len(),
    }
}

fn locked_or_stored(app: &AppState, key: &str) -> Value {
    let spec = Flux3.spec();
    if let Some(field) = spec.schema.fields.iter().find(|field| field.key == key)
        && let Some(locked) = spec.schema.locked_value(field, &context_of(app))
    {
        return locked;
    }
    app.values.get(key).cloned().unwrap_or(Value::Null)
}

impl Render for SchemaForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync(cx);
        div()
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(section_label("Aspect ratio"))
            .child(self.aspect.clone())
            .child(section_label("Resolution"))
            .child(self.resolution.clone())
            .child(section_label("Safety tolerance"))
            .child(self.safety.clone())
            .child(section_label("Grounding"))
            .child(self.grounding.clone())
            .child(section_label("Version"))
            .child(self.version.clone())
    }
}
