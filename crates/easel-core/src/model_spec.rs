use crate::error::FieldError;
use crate::schema::ParamSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    Bfl,
    Krea,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostRule {
    Exact(&'static str),
    Suffix(&'static str),
}

const BFL_HOSTS: &[HostRule] = &[HostRule::Exact("api.bfl.ai"), HostRule::Suffix(".bfl.ai")];

const KREA_HOSTS: &[HostRule] = &[HostRule::Exact("api.krea.ai"), HostRule::Suffix(".krea.ai")];

impl ProviderId {
    #[must_use]
    pub fn base_url(self) -> &'static str {
        match self {
            Self::Bfl => "https://api.bfl.ai",
            Self::Krea => "https://api.krea.ai",
        }
    }

    #[must_use]
    pub fn env_var(self) -> &'static str {
        match self {
            Self::Bfl => "BFL_API_KEY",
            Self::Krea => "KREA_API_KEY",
        }
    }

    #[must_use]
    pub fn allowed_hosts(self) -> &'static [HostRule] {
        match self {
            Self::Bfl => BFL_HOSTS,
            Self::Krea => KREA_HOSTS,
        }
    }

    #[must_use]
    pub fn allows_url(self, url: &str) -> bool {
        let Some(rest) = url.strip_prefix("https://") else {
            return false;
        };
        let host_and_port = rest.split(['/', '?', '#']).next().unwrap_or("");
        if host_and_port.is_empty() || host_and_port.contains('@') {
            return false;
        }
        let host = host_and_port
            .rsplit_once(':')
            .map(|(host, _port)| host)
            .unwrap_or(host_and_port)
            .to_ascii_lowercase();
        self.allowed_hosts().iter().any(|rule| rule.matches(&host))
    }
}

impl HostRule {
    fn matches(self, host: &str) -> bool {
        match self {
            Self::Exact(exact) => host == exact,
            Self::Suffix(suffix) => host.len() > suffix.len() && host.ends_with(suffix),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthScheme {
    HeaderKey { header: String },
    Bearer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSlots {
    List { max: usize },
    Numbered { max: usize },
    Single,
}

impl ImageSlots {
    #[must_use]
    pub fn max(self) -> usize {
        match self {
            Self::List { max } | Self::Numbered { max } => max,
            Self::Single => 1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SizeControl {
    AspectRatioAndResolution,
    WidthHeight,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capabilities {
    pub supports_boxes: bool,
    pub image_slots: ImageSlots,
    pub size_control: SizeControl,
}

impl Capabilities {
    #[must_use]
    pub fn validate_image_count(&self, count: usize) -> Option<FieldError> {
        let max = self.image_slots.max();
        if count > max {
            Some(FieldError {
                key: "images".into(),
                message: format!("accepts at most {max} images"),
                approved_max: i64::try_from(max).ok(),
            })
        } else {
            None
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModelSpec {
    pub id: String,
    pub display_name: String,
    pub provider: ProviderId,
    pub auth: AuthScheme,
    pub endpoint: String,
    #[serde(default)]
    pub beta: bool,
    pub capabilities: Capabilities,
    pub schema: ParamSchema,
}

#[cfg(test)]
mod tests {
    use super::{AuthScheme, Capabilities, ImageSlots, ModelSpec, ProviderId, SizeControl};
    use crate::schema::{CrossRule, FieldDef, FieldKind, LockRule, ParamSchema, RuleContext};
    use serde_json::json;

    fn spec() -> ModelSpec {
        ModelSpec {
            id: "flux-3".into(),
            display_name: "FLUX 3".into(),
            provider: ProviderId::Bfl,
            auth: AuthScheme::HeaderKey {
                header: "x-key".into(),
            },
            endpoint: "/v1/flux-3-image".into(),
            beta: false,
            capabilities: Capabilities {
                supports_boxes: true,
                image_slots: ImageSlots::List { max: 10 },
                size_control: SizeControl::AspectRatioAndResolution,
            },
            schema: ParamSchema {
                fields: vec![FieldDef {
                    key: "prompt".into(),
                    label: "Prompt".into(),
                    help: None,
                    kind: FieldKind::Text { multiline: true },
                    required: true,
                    default: Some(json!("")),
                    locked_when: Some(LockRule::Always {
                        value: json!("locked"),
                    }),
                }],
                cross_rules: vec![CrossRule::Never],
            },
        }
    }

    #[test]
    fn image_count_honors_the_slot_maximum() {
        let capabilities = Capabilities {
            supports_boxes: true,
            image_slots: ImageSlots::List { max: 10 },
            size_control: SizeControl::AspectRatioAndResolution,
        };
        assert!(capabilities.validate_image_count(10).is_none());
        let error = capabilities.validate_image_count(11).unwrap();
        assert_eq!(error.key, "images");
    }

    #[test]
    fn provider_identity_and_host_allowance() {
        assert_eq!(ProviderId::Krea.env_var(), "KREA_API_KEY");
        assert_eq!(
            serde_json::to_value(ProviderId::Bfl).unwrap(),
            serde_json::json!("bfl")
        );
        assert_eq!(
            serde_json::to_value(ProviderId::Krea).unwrap(),
            serde_json::json!("krea")
        );
        assert!(ProviderId::Bfl.allows_url("https://delivery-eu.bfl.ai/x"));
        assert!(!ProviderId::Bfl.allows_url("https://bfl.ai.evil.example/x"));
        assert!(!ProviderId::Bfl.allows_url("http://api.bfl.ai/x"));
        assert!(!ProviderId::Bfl.allows_url("https://api.krea.ai/x"));
    }

    #[test]
    fn model_spec_round_trip_keeps_lock_and_cross_rules() {
        let spec = spec();
        let ctx = RuleContext {
            has_source_image: true,
            image_count: 1,
            box_count: 2,
        };
        assert_eq!(
            spec.schema
                .locked_value(&spec.schema.fields[0], &ctx)
                .as_ref(),
            Some(&json!("locked"))
        );
        let json = serde_json::to_value(&spec).unwrap();
        let back: ModelSpec = serde_json::from_value(json).unwrap();
        assert_eq!(spec, back);
    }
}
