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
}

/// Step 1 stand-in. Later steps add scene-driven variants.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LockRule {
    Always { value: Value },
    WhenSourceImageAndBoxes { value: Value },
}

/// Step 1 stand-in. Later steps add numeric cross-field rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum CrossRule {
    Never,
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
            match rule {
                CrossRule::Never => {}
            }
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
    }
    None
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
