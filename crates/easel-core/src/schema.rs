use crate::error::FieldError;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldDef {
    pub key: String,
    pub label: String,
    #[serde(default)]
    pub help: Option<String>,
    pub kind: FieldKind,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default)]
    pub locked_when: Option<LockRule>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum FieldKind {
    Text { multiline: bool },
    Select { options: Vec<String> },
    Int { min: i64, max: i64, step: i64 },
    Float { min: f64, max: f64, step: f64 },
    Bool,
    ReadOnly,
    Uuid,
    Repeated { item: Vec<FieldDef>, max: usize },
}

/// Step 1 stand-in. Later steps add scene-driven variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LockRule {
    Always { value: Value },
    WhenSourceImageAndBoxes { value: Value },
}

/// Rules that look at more than one field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CrossRule {
    Never,
    /// Each listed field is a multiple of `n`. Zero is skipped when `exempt_zero`.
    MultipleOf {
        keys: Vec<String>,
        n: i64,
        exempt_zero: bool,
    },
    /// Each listed field is 0 or at least `min`.
    ZeroOrAtLeast {
        keys: Vec<String>,
        min: i64,
    },
    /// `values[w] * values[h]` is at most `max`. A zero dimension counts as zero area.
    MaxArea {
        w: String,
        h: String,
        max: i64,
    },
    /// When `key` is true, at least `min` images must be attached.
    RequiresImagesWhen {
        key: String,
        min: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ParamSchema {
    pub fields: Vec<FieldDef>,
    #[serde(default)]
    pub cross_rules: Vec<CrossRule>,
}

/// The only scene facts the schema layer is allowed to see.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleContext {
    pub has_source_image: bool,
    pub image_count: usize,
    pub box_count: usize,
}

impl ParamSchema {
    #[must_use]
    pub fn defaults(&self) -> BTreeMap<String, Value> {
        self.fields
            .iter()
            .filter_map(|field| {
                field
                    .default
                    .clone()
                    .map(|value| (field.key.clone(), value))
            })
            .collect()
    }

    #[must_use]
    pub fn validate(&self, values: &BTreeMap<String, Value>, ctx: &RuleContext) -> Vec<FieldError> {
        let mut errors = Vec::new();
        for field in &self.fields {
            let locked = self.locked_value(field, ctx);
            let value = locked.as_ref().or_else(|| values.get(&field.key));
            match value {
                None | Some(Value::Null) if field.required => {
                    errors.push(FieldError {
                        key: field.key.clone(),
                        message: format!("{} is required", field.label),
                        approved_max: None,
                    });
                }
                Some(value) => {
                    if let Some(error) = check_kind(&field.key, &field.kind, value) {
                        errors.push(error);
                    }
                }
                None => {}
            }
        }
        for rule in &self.cross_rules {
            errors.extend(eval_cross(rule, values, ctx));
        }
        errors
    }

    #[must_use]
    pub fn locked_value(&self, field: &FieldDef, ctx: &RuleContext) -> Option<Value> {
        let field = self
            .fields
            .iter()
            .find(|candidate| candidate.key == field.key)
            .unwrap_or(field);
        match field.locked_when.as_ref()? {
            LockRule::Always { value } => Some(value.clone()),
            LockRule::WhenSourceImageAndBoxes { value } => {
                (ctx.has_source_image && ctx.box_count > 0).then(|| value.clone())
            }
        }
    }
}

fn eval_cross(
    rule: &CrossRule,
    values: &BTreeMap<String, Value>,
    ctx: &RuleContext,
) -> Vec<FieldError> {
    match rule {
        CrossRule::Never => Vec::new(),
        CrossRule::MultipleOf {
            keys,
            n,
            exempt_zero,
        } => {
            if *n == 0 {
                return Vec::new();
            }
            keys.iter()
                .filter_map(|key| {
                    let number = values.get(key)?.as_i64()?;
                    if *exempt_zero && number == 0 {
                        return None;
                    }
                    (number % n != 0).then(|| FieldError {
                        key: key.clone(),
                        message: format!("must be a multiple of {n}"),
                        approved_max: None,
                    })
                })
                .collect()
        }
        CrossRule::ZeroOrAtLeast { keys, min } => keys
            .iter()
            .filter_map(|key| {
                let number = values.get(key)?.as_i64()?;
                (number != 0 && number < *min).then(|| FieldError {
                    key: key.clone(),
                    message: format!("must be 0 or at least {min}"),
                    approved_max: None,
                })
            })
            .collect(),
        CrossRule::MaxArea { w, h, max } => {
            let Some(width) = values.get(w).and_then(Value::as_i64) else {
                return Vec::new();
            };
            let Some(height) = values.get(h).and_then(Value::as_i64) else {
                return Vec::new();
            };
            let Some(area) = width.checked_mul(height) else {
                return vec![area_error(w), area_error(h)];
            };
            if area > *max {
                vec![area_error(w), area_error(h)]
            } else {
                Vec::new()
            }
        }
        CrossRule::RequiresImagesWhen { key, min } => {
            let enabled = values.get(key).and_then(Value::as_bool).unwrap_or(false);
            if enabled && ctx.image_count < *min {
                vec![FieldError {
                    key: key.clone(),
                    message: "Regional edit needs a reference image".into(),
                    approved_max: None,
                }]
            } else {
                Vec::new()
            }
        }
    }
}

fn area_error(key: &str) -> FieldError {
    FieldError {
        key: key.to_string(),
        message: "must be at most 4 MP".into(),
        approved_max: None,
    }
}

fn check_kind(key: &str, kind: &FieldKind, value: &Value) -> Option<FieldError> {
    match kind {
        FieldKind::Text { .. } => {
            if value.as_str().is_none() {
                return Some(type_error(key, "expected text"));
            }
        }
        FieldKind::Select { options } => {
            let Some(text) = value.as_str() else {
                return Some(type_error(key, "expected one of the options"));
            };
            if !options.iter().any(|option| option == text) {
                return Some(FieldError {
                    key: key.to_string(),
                    message: "not one of the options".into(),
                    approved_max: None,
                });
            }
        }
        FieldKind::Int { min, max, .. } => {
            let Some(number) = value.as_i64() else {
                return Some(type_error(key, "expected an integer"));
            };
            if number < *min || number > *max {
                return Some(FieldError {
                    key: key.to_string(),
                    message: format!("must be between {min} and {max}"),
                    approved_max: Some(*max),
                });
            }
        }
        FieldKind::Float { min, max, .. } => {
            let Some(number) = value.as_f64() else {
                return Some(type_error(key, "expected a number"));
            };
            if number < *min || number > *max {
                return Some(FieldError {
                    key: key.to_string(),
                    message: format!("must be between {min} and {max}"),
                    approved_max: None,
                });
            }
        }
        FieldKind::Bool => {
            if !value.is_boolean() {
                return Some(type_error(key, "expected a boolean"));
            }
        }
        FieldKind::ReadOnly => {}
        FieldKind::Uuid => {
            let Some(text) = value.as_str() else {
                return Some(type_error(key, "expected a UUID"));
            };
            if !is_uuid(text) {
                return Some(FieldError {
                    key: key.to_string(),
                    message: "must be a UUID".into(),
                    approved_max: None,
                });
            }
        }
        FieldKind::Repeated { item, max } => {
            let Some(entries) = value.as_array() else {
                return Some(type_error(key, "expected a list"));
            };
            if entries.len() > *max {
                return Some(FieldError {
                    key: key.to_string(),
                    message: format!("accepts at most {max}"),
                    approved_max: i64::try_from(*max).ok(),
                });
            }
            for entry in entries {
                let Some(object) = entry.as_object() else {
                    return Some(type_error(key, "expected an object"));
                };
                for field in item {
                    match object.get(&field.key) {
                        None | Some(Value::Null) if field.required => {
                            return Some(FieldError {
                                key: key.to_string(),
                                message: format!("{} is required", field.label),
                                approved_max: None,
                            });
                        }
                        Some(child) => {
                            if let Some(mut error) = check_kind(&field.key, &field.kind, child) {
                                error.key = key.to_string();
                                return Some(error);
                            }
                        }
                        None => {}
                    }
                }
            }
        }
    }
    None
}

fn is_uuid(text: &str) -> bool {
    if text == "00000000-0000-0000-0000-000000000000"
        || text.eq_ignore_ascii_case("ffffffff-ffff-ffff-ffff-ffffffffffff")
    {
        return true;
    }
    let parts: Vec<&str> = text.split('-').collect();
    if parts.len() != 5 {
        return false;
    }
    let lengths = [8, 4, 4, 4, 12];
    if parts
        .iter()
        .zip(lengths)
        .any(|(part, length)| part.len() != length || !part.chars().all(|c| c.is_ascii_hexdigit()))
    {
        return false;
    }
    let version = parts[2].as_bytes()[0];
    let variant = parts[3].as_bytes()[0];
    (b'1'..=b'8').contains(&version) && matches!(variant, b'8' | b'9' | b'a' | b'b' | b'A' | b'B')
}

fn type_error(key: &str, message: &str) -> FieldError {
    FieldError {
        key: key.to_string(),
        message: message.to_string(),
        approved_max: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{FieldDef, FieldKind, ParamSchema, RuleContext};
    use crate::error::FieldError;
    use serde_json::json;
    use std::collections::BTreeMap;

    fn schema() -> ParamSchema {
        ParamSchema {
            fields: vec![
                FieldDef {
                    key: "prompt".into(),
                    label: "Prompt".into(),
                    help: None,
                    kind: FieldKind::Text { multiline: true },
                    required: true,
                    default: Some(json!("a fox")),
                    locked_when: None,
                },
                FieldDef {
                    key: "steps".into(),
                    label: "Steps".into(),
                    help: None,
                    kind: FieldKind::Int {
                        min: 1,
                        max: 50,
                        step: 1,
                    },
                    required: false,
                    default: Some(json!(20)),
                    locked_when: None,
                },
                FieldDef {
                    key: "mode".into(),
                    label: "Mode".into(),
                    help: None,
                    kind: FieldKind::Select {
                        options: vec!["fast".into(), "quality".into()],
                    },
                    required: false,
                    default: Some(json!("fast")),
                    locked_when: None,
                },
            ],
            cross_rules: vec![],
        }
    }

    fn context() -> RuleContext {
        RuleContext {
            has_source_image: false,
            image_count: 0,
            box_count: 0,
        }
    }

    fn keys(errors: &[FieldError]) -> Vec<&str> {
        errors.iter().map(|error| error.key.as_str()).collect()
    }

    #[test]
    fn validate_names_the_key_for_each_kind_of_failure() {
        let schema = schema();
        let missing = schema.validate(&BTreeMap::new(), &context());
        assert_eq!(keys(&missing), ["prompt"]);

        let mut values = schema.defaults();
        values.insert("steps".into(), json!(99));
        let range = schema.validate(&values, &context());
        assert_eq!(keys(&range), ["steps"]);

        let mut values = schema.defaults();
        values.insert("mode".into(), json!("nope"));
        let select = schema.validate(&values, &context());
        assert_eq!(keys(&select), ["mode"]);
    }

    #[test]
    fn defaults_satisfy_required_fields() {
        let schema = schema();
        assert!(schema.validate(&schema.defaults(), &context()).is_empty());
    }
}
