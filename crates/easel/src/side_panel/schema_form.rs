use easel_core::schema::{FieldDef, FieldKind, RuleContext};
use gpui::{
    Context, Entity, IntoElement, ParentElement, Render, Subscription, Window, div, prelude::*, px,
};
use serde_json::{Value, json};

use crate::app_state::AppState;
use crate::ui::muted_text;
use crate::widgets::{self, Checkbox, NumberInput, Select, Slider, TextInput};

const FLUX3_ID: &str = "flux-3-image";

enum FieldControl {
    Select(Entity<Select>),
    Slider(Entity<Slider>),
    Check(Entity<Checkbox>),
    Number {
        input: Entity<NumberInput>,
        clear_blank: bool,
    },
    Text {
        input: Entity<TextInput>,
        integer: bool,
    },
    Repeated,
}

pub struct SchemaForm {
    app_state: Entity<AppState>,
    model_id: String,
    controls: Vec<(String, FieldControl)>,
    #[cfg(test)]
    presented: Vec<(String, Option<String>)>,
    _observe: Subscription,
}

impl SchemaForm {
    pub fn new(app_state: Entity<AppState>, cx: &mut Context<Self>) -> Self {
        let mut form = Self {
            app_state: app_state.clone(),
            model_id: String::new(),
            controls: Vec::new(),
            #[cfg(test)]
            presented: Vec::new(),
            _observe: cx.observe(&app_state, |this, _, cx| {
                let id = this.app_state.read(cx).model_id.clone();
                if id != this.model_id {
                    this.rebuild(cx);
                } else {
                    this.push_values(cx);
                }
                cx.notify();
            }),
        };
        form.rebuild(cx);
        form
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn field_count(&self) -> usize {
        self.controls.len()
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn version_is_read_only(&self, cx: &gpui::App) -> bool {
        self.text_input("version")
            .is_some_and(|input| !input.read(cx).is_enabled())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn value(&self, key: &str, cx: &gpui::App) -> Value {
        let app = self.app_state.read(cx);
        displayed_value(app, key)
    }

    #[cfg(test)]
    pub fn aspect_control(&self) -> &Entity<Select> {
        self.select_input("aspect_ratio")
            .expect("aspect ratio control")
    }

    #[cfg(test)]
    pub fn resolution_is_open(&self, cx: &gpui::App) -> bool {
        self.select_input("resolution")
            .is_some_and(|select| select.read(cx).is_open())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn aspect_enabled(&self, cx: &gpui::App) -> bool {
        self.select_input("aspect_ratio")
            .is_some_and(|select| select.read(cx).is_enabled())
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn safety_max(&self, cx: &gpui::App) -> f64 {
        self.slider_input("safety_tolerance")
            .map(|slider| slider.read(cx).max_value())
            .unwrap_or(0.0)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn safety_value(&self, cx: &gpui::App) -> f64 {
        self.slider_input("safety_tolerance")
            .and_then(|slider| slider.read(cx).value().as_f64())
            .unwrap_or(0.0)
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub fn apply_safety_cap(&mut self, approved_max: i64, cx: &mut Context<Self>) {
        let Some(slider) = self.slider_input("safety_tolerance").cloned() else {
            return;
        };
        let schema_max = self
            .app_state
            .read(cx)
            .spec()
            .schema
            .fields
            .iter()
            .find(|field| field.key == "safety_tolerance")
            .and_then(|field| match field.kind {
                FieldKind::Int { max, .. } => Some(max),
                _ => None,
            })
            .unwrap_or(7);
        let max = (approved_max as f64).clamp(0.0, schema_max as f64);
        let value = slider.update(cx, |slider, cx| {
            slider.set_range(0.0, max, cx);
            slider.value().as_f64().unwrap_or(0.0)
        });
        self.app_state.update(cx, |app, cx| {
            app.values
                .insert("safety_tolerance".into(), json!(value.round() as i64));
            cx.notify();
        });
    }

    fn select_input(&self, key: &str) -> Option<&Entity<Select>> {
        self.controls
            .iter()
            .find_map(|(name, control)| match control {
                FieldControl::Select(input) if name == key => Some(input),
                _ => None,
            })
    }

    fn text_input(&self, key: &str) -> Option<&Entity<TextInput>> {
        self.controls
            .iter()
            .find_map(|(name, control)| match control {
                FieldControl::Text { input, .. } if name == key => Some(input),
                _ => None,
            })
    }

    fn slider_input(&self, key: &str) -> Option<&Entity<Slider>> {
        self.controls
            .iter()
            .find_map(|(name, control)| match control {
                FieldControl::Slider(input) if name == key => Some(input),
                _ => None,
            })
    }

    #[cfg(test)]
    pub fn select(&self, key: &str) -> Option<&Entity<Select>> {
        self.select_input(key)
    }

    #[cfg(test)]
    pub fn text(&self, key: &str) -> Option<&Entity<TextInput>> {
        self.text_input(key)
    }

    #[cfg(test)]
    pub fn check(&self, key: &str) -> Option<&Entity<Checkbox>> {
        self.controls
            .iter()
            .find_map(|(name, control)| match control {
                FieldControl::Check(input) if name == key => Some(input),
                _ => None,
            })
    }

    #[cfg(test)]
    pub fn slider(&self, key: &str) -> Option<&Entity<Slider>> {
        self.slider_input(key)
    }

    #[cfg(test)]
    pub fn number(&self, key: &str) -> Option<&Entity<NumberInput>> {
        self.controls
            .iter()
            .find_map(|(name, control)| match control {
                FieldControl::Number { input, .. } if name == key => Some(input),
                _ => None,
            })
    }

    #[cfg(test)]
    pub fn is_repeated(&self, key: &str) -> bool {
        self.controls
            .iter()
            .any(|(name, control)| name == key && matches!(control, FieldControl::Repeated))
    }

    #[cfg(test)]
    pub fn has_control(&self, key: &str) -> bool {
        self.controls
            .iter()
            .any(|(name, control)| name == key && !matches!(control, FieldControl::Repeated))
    }

    #[cfg(test)]
    pub fn presented(&self) -> &[(String, Option<String>)] {
        &self.presented
    }

    #[cfg(test)]
    pub fn control_caption(&self, key: &str, cx: &gpui::App) -> (Option<String>, Option<String>) {
        let Some((_, control)) = self.controls.iter().find(|(name, _)| name == key) else {
            return (None, None);
        };
        match control {
            FieldControl::Select(input) => caption(input.read(cx).label(), input.read(cx).help()),
            FieldControl::Slider(input) => caption(input.read(cx).label(), input.read(cx).help()),
            FieldControl::Check(input) => caption(input.read(cx).label(), input.read(cx).help()),
            FieldControl::Number { input, .. } => {
                caption(input.read(cx).label(), input.read(cx).help())
            }
            FieldControl::Text { input, .. } => {
                caption(input.read(cx).label(), input.read(cx).help())
            }
            FieldControl::Repeated => (None, None),
        }
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        let (model_id, fields) = {
            let app = self.app_state.read(cx);
            (app.model_id.clone(), app.spec().schema.fields.clone())
        };
        self.model_id = model_id;
        let state = self.app_state.clone();
        self.controls = fields
            .iter()
            .map(|field| (field.key.clone(), build_control(field, &state, cx)))
            .collect();
        self.push_values(cx);
        self.copy_flux_aspect(cx);
    }

    fn push_values(&self, cx: &mut Context<Self>) {
        let app = self.app_state.read(cx);
        let schema = app.spec().schema.clone();
        let ctx = context_of(app);
        let values: Vec<(String, Value)> = schema
            .fields
            .iter()
            .map(|field| {
                let value = schema
                    .locked_value(field, &ctx)
                    .or_else(|| app.values.get(&field.key).cloned())
                    .unwrap_or(Value::Null);
                (field.key.clone(), value)
            })
            .collect();
        for (key, value) in values {
            let Some((_, control)) = self.controls.iter().find(|(name, _)| name == &key) else {
                continue;
            };
            match control {
                FieldControl::Select(input) => {
                    let desired = if value.as_str().is_some_and(|text| !text.is_empty()) {
                        value
                    } else {
                        Value::Null
                    };
                    if input.read(cx).value() != &desired {
                        input.update(cx, |select, cx| select.set_value(desired, cx));
                    }
                }
                FieldControl::Slider(input) => {
                    if let Some(number) = value.as_f64()
                        && input.read(cx).value().as_f64() != Some(number)
                    {
                        input.update(cx, |slider, cx| slider.set_value(&json!(number), cx));
                    }
                }
                FieldControl::Check(input) => {
                    if let Some(checked) = value.as_bool()
                        && input.read(cx).value().as_bool() != Some(checked)
                    {
                        input.update(cx, |check, cx| check.set_value(&json!(checked), cx));
                    }
                }
                FieldControl::Number {
                    input, clear_blank, ..
                } => {
                    if value.is_null() && !*clear_blank {
                        continue;
                    }
                    input.update(cx, |number, cx| number.set_value(&value, cx));
                }
                FieldControl::Text { input, integer, .. } => {
                    let desired = text_of(&value, *integer);
                    if input.read(cx).text() != desired {
                        input.update(cx, |text, cx| text.set_text(&desired, cx));
                    }
                }
                FieldControl::Repeated => {}
            }
        }
    }

    fn copy_flux_aspect(&self, cx: &mut Context<Self>) {
        if self.model_id != FLUX3_ID {
            return;
        }
        let Some(ratio) = self
            .app_state
            .read(cx)
            .values
            .get("aspect_ratio")
            .and_then(Value::as_str)
            .filter(|ratio| !ratio.is_empty())
            .map(str::to_string)
        else {
            return;
        };
        if self.app_state.read(cx).scene.aspect_ratio.as_deref() == Some(ratio.as_str()) {
            return;
        }
        self.app_state.update(cx, |app, cx| {
            app.scene.aspect_ratio = Some(ratio);
            cx.notify();
        });
    }
}

#[cfg(test)]
fn caption(label: Option<&str>, help: Option<&str>) -> (Option<String>, Option<String>) {
    (label.map(str::to_string), help.map(str::to_string))
}

fn build_control(
    field: &FieldDef,
    state: &Entity<AppState>,
    cx: &mut Context<SchemaForm>,
) -> FieldControl {
    match &field.kind {
        FieldKind::Select { options } => {
            let blank_clears = !field.required;
            let mut items: Vec<gpui::SharedString> = options
                .iter()
                .map(|option| gpui::SharedString::from(option.clone()))
                .collect();
            if blank_clears {
                items.insert(0, gpui::SharedString::from(""));
            }
            let input = cx.new(|cx| Select::new(items, cx));
            bind_select(state, &input, field.key.clone(), blank_clears, cx);
            FieldControl::Select(input)
        }
        FieldKind::Bool => {
            let input = cx.new(Checkbox::new);
            bind_check(state, &input, field.key.clone(), cx);
            FieldControl::Check(input)
        }
        FieldKind::Int { max, .. } if *max == i64::MAX => {
            let input = cx.new(TextInput::new);
            bind_text(state, &input, field.key.clone(), true, true, cx);
            FieldControl::Text {
                input,
                integer: true,
            }
        }
        FieldKind::Int { min, max, step }
            if (field.required || field.default.is_some()) && slider_steps(*min, *max, *step) =>
        {
            let input = cx.new(|cx| {
                let mut slider = Slider::new(*min as f64, *max as f64, cx);
                slider.set_step(*step as f64, cx);
                slider
            });
            bind_slider(state, &input, field.key.clone(), cx);
            FieldControl::Slider(input)
        }
        FieldKind::Int { min, max, step } if exact_integer(*min) && exact_integer(*max) => {
            let clear_blank = !field.required && field.default.is_none();
            let input = cx.new(|cx| {
                let mut number = NumberInput::new(*min as f64, *max as f64, *step as f64, cx);
                number.set_blank_allowed(clear_blank, cx);
                number
            });
            bind_number(state, &input, field.key.clone(), true, clear_blank, cx);
            FieldControl::Number { input, clear_blank }
        }
        FieldKind::Int { .. } => {
            let input = cx.new(TextInput::new);
            bind_text(state, &input, field.key.clone(), true, true, cx);
            FieldControl::Text {
                input,
                integer: true,
            }
        }
        FieldKind::Float { min, max, step } => {
            let clear_blank = !field.required && field.default.is_none();
            let input = cx.new(|cx| {
                let mut number = NumberInput::new(*min, *max, *step, cx);
                number.set_blank_allowed(clear_blank, cx);
                number
            });
            bind_number(state, &input, field.key.clone(), false, clear_blank, cx);
            FieldControl::Number { input, clear_blank }
        }
        FieldKind::Text { multiline } => {
            let clear_blank = !field.required && field.default.is_none();
            let input = if *multiline {
                cx.new(TextInput::multiline)
            } else {
                cx.new(TextInput::new)
            };
            bind_text(state, &input, field.key.clone(), false, clear_blank, cx);
            FieldControl::Text {
                input,
                integer: false,
            }
        }
        FieldKind::Uuid => {
            let clear_blank = !field.required && field.default.is_none();
            let input = cx.new(TextInput::new);
            bind_text(state, &input, field.key.clone(), false, clear_blank, cx);
            FieldControl::Text {
                input,
                integer: false,
            }
        }
        FieldKind::ReadOnly => {
            let input = cx.new(|cx| {
                let mut input = TextInput::new(cx);
                input.set_enabled(false, cx);
                input
            });
            FieldControl::Text {
                input,
                integer: false,
            }
        }
        FieldKind::Repeated { .. } => FieldControl::Repeated,
    }
}

fn slider_steps(min: i64, max: i64, step: i64) -> bool {
    if step <= 0 || max == i64::MAX {
        return false;
    }
    max.checked_sub(min).is_some_and(|span| span / step <= 200)
}

fn exact_integer(value: i64) -> bool {
    let float = value as f64;
    float as i64 == value
}

fn bind_select(
    state: &Entity<AppState>,
    select: &Entity<Select>,
    key: String,
    blank_clears: bool,
    cx: &mut Context<SchemaForm>,
) {
    let state = state.clone();
    select.update(cx, |select, cx| {
        select.on_change(
            Box::new(move |value, _window, cx| {
                let value = value.clone();
                state.update(cx, |app, cx| {
                    let blank = value.as_str().is_some_and(str::is_empty);
                    if blank_clears && blank {
                        app.values.remove(&key);
                    } else {
                        if key == "aspect_ratio"
                            && app.model_id == FLUX3_ID
                            && let Some(ratio) = value.as_str()
                            && !ratio.is_empty()
                        {
                            app.scene.aspect_ratio = Some(ratio.to_string());
                        }
                        app.values.insert(key.clone(), value);
                    }
                    cx.notify();
                });
            }),
            cx,
        );
    });
}

fn bind_check(
    state: &Entity<AppState>,
    check: &Entity<Checkbox>,
    key: String,
    cx: &mut Context<SchemaForm>,
) {
    let state = state.clone();
    check.update(cx, |check, cx| {
        check.on_change(
            Box::new(move |value, _window, cx| {
                let value = value.clone();
                state.update(cx, |app, cx| {
                    app.values.insert(key.clone(), value);
                    cx.notify();
                });
            }),
            cx,
        );
    });
}

fn bind_slider(
    state: &Entity<AppState>,
    slider: &Entity<Slider>,
    key: String,
    cx: &mut Context<SchemaForm>,
) {
    let state = state.clone();
    slider.update(cx, |slider, cx| {
        slider.on_change(
            Box::new(move |value, _window, cx| {
                let Some(number) = value.as_f64() else {
                    return;
                };
                state.update(cx, |app, cx| {
                    app.values.insert(key.clone(), json!(number.round() as i64));
                    cx.notify();
                });
            }),
            cx,
        );
    });
}

fn bind_number(
    state: &Entity<AppState>,
    number: &Entity<NumberInput>,
    key: String,
    integer: bool,
    clear_blank: bool,
    cx: &mut Context<SchemaForm>,
) {
    let state = state.clone();
    number.update(cx, |number, cx| {
        number.on_change(
            Box::new(move |value, _window, cx| {
                state.update(cx, |app, cx| {
                    if value.is_null() && clear_blank {
                        app.values.remove(&key);
                    } else if integer {
                        app.values.insert(key.clone(), stored_integer(value));
                    } else if let Some(number) = value.as_f64() {
                        app.values.insert(key.clone(), json!(number));
                    }
                    cx.notify();
                });
            }),
            cx,
        );
    });
}

fn bind_text(
    state: &Entity<AppState>,
    input: &Entity<TextInput>,
    key: String,
    integer: bool,
    clear_blank: bool,
    cx: &mut Context<SchemaForm>,
) {
    let state = state.clone();
    input.update(cx, |input, cx| {
        input.on_change(
            Box::new(move |value, _window, cx| {
                let text = value.as_str().unwrap_or("").trim().to_string();
                state.update(cx, |app, cx| {
                    if integer {
                        if text.is_empty() && clear_blank {
                            app.values.remove(&key);
                        } else if let Ok(number) = text.parse::<i64>() {
                            app.values.insert(key.clone(), json!(number));
                        }
                    } else if text.is_empty() && clear_blank {
                        app.values.remove(&key);
                    } else {
                        app.values.insert(key.clone(), Value::String(text));
                    }
                    cx.notify();
                });
            }),
            cx,
        );
    });
}

fn stored_integer(value: &Value) -> Value {
    let Some(number) = value.as_f64() else {
        return value.clone();
    };
    if number.fract() == 0.0 && (i64::MIN as f64..=i64::MAX as f64).contains(&number) {
        json!(number as i64)
    } else {
        json!(number)
    }
}

fn text_of(value: &Value, integer: bool) -> String {
    if integer {
        if let Some(number) = value.as_i64() {
            return number.to_string();
        }
        if let Some(number) = value.as_f64()
            && number.fract() == 0.0
        {
            return (number as i64).to_string();
        }
        return String::new();
    }
    value.as_str().unwrap_or("").to_string()
}

fn context_of(app: &AppState) -> RuleContext {
    RuleContext {
        has_source_image: app.scene.source_image.is_some(),
        image_count: usize::from(app.scene.source_image.is_some()) + app.scene.references.len(),
        box_count: app.scene.boxes.len(),
    }
}

#[cfg_attr(not(test), allow(dead_code))]
fn displayed_value(app: &AppState, key: &str) -> Value {
    let spec = app.spec();
    if let Some(field) = spec.schema.fields.iter().find(|field| field.key == key)
        && let Some(locked) = spec.schema.locked_value(field, &context_of(app))
    {
        return locked;
    }
    app.values.get(key).cloned().unwrap_or(Value::Null)
}

impl Render for SchemaForm {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let fields = self.app_state.read(cx).spec().schema.fields.clone();
        #[cfg(test)]
        self.presented.clear();
        let mut column = div().flex().flex_col().gap(px(8.));
        for field in &fields {
            let Some((_, control)) = self.controls.iter().find(|(key, _)| key == &field.key) else {
                continue;
            };
            #[cfg(test)]
            self.presented
                .push((field.label.clone(), field.help.clone()));
            let child = match control {
                FieldControl::Select(input) => input.clone().into_any_element(),
                FieldControl::Slider(input) => input.clone().into_any_element(),
                FieldControl::Check(input) => input.clone().into_any_element(),
                FieldControl::Number { input, .. } => input.clone().into_any_element(),
                FieldControl::Text { input, .. } => input.clone().into_any_element(),
                FieldControl::Repeated => {
                    let shown = self
                        .app_state
                        .read(cx)
                        .values
                        .get(&field.key)
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "empty".into());
                    muted_text(shown).into_any_element()
                }
            };
            column = column.child(widgets::field_row(
                Some(field.label.as_str()),
                field.help.as_deref(),
                child,
            ));
        }
        column
    }
}
