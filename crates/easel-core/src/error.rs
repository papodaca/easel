use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldError {
    pub key: String,
    pub message: String,
    #[serde(default)]
    pub approved_max: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Error)]
#[serde(rename_all = "snake_case")]
pub enum ProviderError {
    #[error("authentication failed")]
    Auth,
    #[error("{}", credits_message(hint))]
    OutOfCredits {
        #[serde(default)]
        hint: Option<String>,
    },
    #[error("rate limited")]
    RateLimited {
        #[serde(with = "duration_option")]
        retry_after: Option<Duration>,
    },
    #[error("{message}")]
    Validation {
        #[serde(default)]
        message: String,
        field_errors: Vec<FieldError>,
    },
    #[error("moderated")]
    Moderated { reasons: Vec<String> },
    #[error("{}", gone_message(replacement, sunset))]
    Gone {
        replacement: Option<String>,
        sunset: Option<String>,
    },
    #[error("http {status}")]
    Http { status: u16, body: String },
    #[error("protocol error: {0}")]
    Protocol(String),
    #[error("network error: {0}")]
    Network(String),
}

fn credits_message(hint: &Option<String>) -> String {
    match hint {
        Some(hint) => format!("out of API balance ({hint})"),
        None => "out of credits".into(),
    }
}

fn gone_message(replacement: &Option<String>, sunset: &Option<String>) -> String {
    match (replacement, sunset) {
        (Some(name), Some(date)) => format!("replaced by {name} (sunset {date})"),
        (Some(name), None) => format!("replaced by {name}"),
        (None, Some(date)) => format!("model sunset {date}"),
        (None, None) => "model is gone".into(),
    }
}

mod duration_option {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::time::Duration;

    pub fn serialize<S>(value: &Option<Duration>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let encoded = value.map(|duration| (duration.as_secs(), duration.subsec_nanos()));
        Option::<(u64, u32)>::serialize(&encoded, serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<Duration>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let encoded = Option::<(u64, u32)>::deserialize(deserializer)?;
        Ok(encoded.map(|(secs, nanos)| Duration::new(secs, nanos)))
    }
}

#[cfg(test)]
mod tests {
    use super::{FieldError, ProviderError};
    use std::time::Duration;

    #[test]
    fn provider_error_round_trip() {
        let error = ProviderError::Validation {
            message: "too high".into(),
            field_errors: vec![FieldError {
                key: "safety_tolerance".into(),
                message: "too high".into(),
                approved_max: Some(5),
            }],
        };
        let json = serde_json::to_value(&error).unwrap();
        let back: ProviderError = serde_json::from_value(json).unwrap();
        assert_eq!(error, back);

        let limited = ProviderError::RateLimited {
            retry_after: Some(Duration::from_secs(3)),
        };
        let json = serde_json::to_value(&limited).unwrap();
        let back: ProviderError = serde_json::from_value(json).unwrap();
        assert_eq!(limited, back);
    }
}
