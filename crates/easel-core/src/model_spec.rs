use crate::error::FieldError;
use crate::schema::ParamSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderId {
    Bfl,
    Krea,
    Google,
    #[serde(rename = "openai")]
    OpenAi,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostRule {
    Exact(&'static str),
    Suffix(&'static str),
}

const BFL_HOSTS: &[HostRule] = &[HostRule::Exact("api.bfl.ai"), HostRule::Suffix(".bfl.ai")];

const KREA_HOSTS: &[HostRule] = &[HostRule::Exact("api.krea.ai"), HostRule::Suffix(".krea.ai")];

const GOOGLE_HOSTS: &[HostRule] = &[HostRule::Exact("generativelanguage.googleapis.com")];

const OPENAI_HOSTS: &[HostRule] = &[HostRule::Exact("api.openai.com")];

impl ProviderId {
    #[must_use]
    pub fn base_url(self) -> &'static str {
        match self {
            Self::Bfl => "https://api.bfl.ai",
            Self::Krea => "https://api.krea.ai",
            Self::Google => "https://generativelanguage.googleapis.com",
            Self::OpenAi => "https://api.openai.com",
        }
    }

    #[must_use]
    pub fn env_var(self) -> &'static str {
        match self {
            Self::Bfl => "BFL_API_KEY",
            Self::Krea => "KREA_API_KEY",
            Self::Google => "GEMINI_API_KEY",
            Self::OpenAi => "OPENAI_API_KEY",
        }
    }

    #[must_use]
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Bfl => "Black Forest Labs",
            Self::Krea => "Krea",
            Self::Google => "Google",
            Self::OpenAi => "OpenAI",
        }
    }

    #[must_use]
    pub fn auth_scheme(self) -> AuthScheme {
        match self {
            Self::Bfl => AuthScheme::HeaderKey {
                header: "x-key".into(),
            },
            Self::Krea => AuthScheme::Bearer,
            Self::Google => AuthScheme::HeaderKey {
                header: "x-goog-api-key".into(),
            },
            Self::OpenAi => AuthScheme::Bearer,
        }
    }

    #[must_use]
    pub fn allowed_hosts(self) -> &'static [HostRule] {
        match self {
            Self::Bfl => BFL_HOSTS,
            Self::Krea => KREA_HOSTS,
            Self::Google => GOOGLE_HOSTS,
            Self::OpenAi => OPENAI_HOSTS,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ImageSlots {
    List {
        max: usize,
        #[serde(default = "default_image_key")]
        key: String,
    },
    Numbered {
        max: usize,
    },
    Single,
    SingleWithStyleRefs {
        max_style_refs: usize,
    },
}

fn default_image_key() -> String {
    "images".into()
}

impl ImageSlots {
    #[must_use]
    pub fn max(&self) -> usize {
        match self {
            Self::List { max, .. } | Self::Numbered { max } => *max,
            Self::Single => 1,
            Self::SingleWithStyleRefs { max_style_refs } => max_style_refs.saturating_add(1),
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
                image_slots: ImageSlots::List {
                    max: 10,
                    key: "images".into(),
                },
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
            image_slots: ImageSlots::List {
                max: 10,
                key: "images".into(),
            },
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
        assert!(ProviderId::Krea.allows_url("https://api.krea.ai/assets/fixed"));
        assert!(ProviderId::Krea.allows_url("https://cdn.krea.ai/r.png"));
        assert!(!ProviderId::Bfl.allows_url("https://bfl.ai.evil.example/x"));
        assert!(!ProviderId::Bfl.allows_url("http://api.bfl.ai/x"));
        assert!(!ProviderId::Bfl.allows_url("https://api.krea.ai/x"));
    }

    #[test]
    fn google_allows_only_the_generative_language_host() {
        use crate::providers::ModelRegistry;

        assert_eq!(ProviderId::Google.env_var(), "GEMINI_API_KEY");
        assert_eq!(ProviderId::Google.display_name(), "Google");
        assert!(matches!(
            ProviderId::Google.auth_scheme(),
            AuthScheme::HeaderKey { ref header } if header == "x-goog-api-key"
        ));
        assert!(
            ProviderId::Google
                .allows_url("https://generativelanguage.googleapis.com/v1beta/interactions")
        );
        assert!(!ProviderId::Google.allows_url("https://evil.example/x"));
        assert!(
            !ProviderId::Google
                .allows_url("http://generativelanguage.googleapis.com/v1beta/interactions")
        );
        assert!(ModelRegistry::providers().contains(&ProviderId::Google));
        let ids: Vec<_> = ModelRegistry::specs()
            .iter()
            .filter(|spec| spec.provider == ProviderId::Google)
            .map(|spec| spec.id.as_str())
            .collect();
        assert_eq!(
            ids,
            [
                "gemini-nano-banana-2.1",
                "gemini-3.1-flash-lite-image",
                "gemini-3-pro-image",
            ]
        );
    }

    #[test]
    fn registry_lists_the_six_openai_models_in_menu_order() {
        use crate::providers::ModelRegistry;

        let specs: Vec<_> = ModelRegistry::specs()
            .iter()
            .filter(|spec| spec.id.starts_with("gpt-image"))
            .collect();
        let ids: Vec<_> = specs.iter().map(|spec| spec.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "gpt-image-2.5-flare",
                "gpt-image-2.5-sunburst",
                "gpt-image-2",
                "gpt-image-1.5",
                "gpt-image-1-mini",
                "gpt-image-1",
            ]
        );
        let names: Vec<_> = specs
            .iter()
            .map(|spec| spec.display_name.as_str())
            .collect();
        assert!(names[3].contains("2026-12-01"), "{}", names[3]);
        assert!(names[4].contains("2026-12-01"), "{}", names[4]);
        assert!(names[5].contains("2026-10-23"), "{}", names[5]);
        assert!(!names[0].contains("2026-"));
    }

    #[test]
    fn openai_allows_only_api_openai_com() {
        assert_eq!(ProviderId::OpenAi.env_var(), "OPENAI_API_KEY");
        assert_eq!(ProviderId::OpenAi.display_name(), "OpenAI");
        assert!(matches!(
            ProviderId::OpenAi.auth_scheme(),
            AuthScheme::Bearer
        ));
        assert!(ProviderId::OpenAi.allows_url("https://api.openai.com/v1/images/generations"));
        assert!(!ProviderId::OpenAi.allows_url("https://evil.example/v1/images/generations"));
        assert!(!ProviderId::OpenAi.allows_url("https://api.openai.com.evil.example/x"));
        assert!(!ProviderId::OpenAi.allows_url("http://api.openai.com/v1/models"));
        assert_eq!(
            serde_json::to_value(ProviderId::OpenAi).unwrap(),
            serde_json::json!("openai")
        );
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
