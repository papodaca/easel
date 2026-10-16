pub mod bfl;

use crate::error::ProviderError;
use serde::{Deserialize, Serialize};

/// Server job id. Only values matching `^[A-Za-z0-9_-]{1,64}$` can name a file.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct JobId(String);

impl JobId {
    pub fn parse(raw: &str) -> Result<Self, String> {
        let valid = (1..=64).contains(&raw.len())
            && raw
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-');
        if valid {
            Ok(Self(raw.to_string()))
        } else {
            Err(format!("invalid job id `{raw}`"))
        }
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl<'de> Deserialize<'de> for JobId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let raw = String::deserialize(deserializer)?;
        Self::parse(&raw).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CostQuote {
    pub cost: f64,
    pub input_mp: f64,
    pub output_mp: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Submitted {
    pub job_id: JobId,
    pub poll_url: url::Url,
    pub quote: Option<CostQuote>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum PollOutcome {
    InFlight {
        status: String,
        progress: Option<f32>,
    },
    Ready {
        url: url::Url,
        settled_cost: Option<f64>,
        prompt: Option<String>,
        duration: Option<f64>,
    },
    Failed(ProviderError),
    Cancelled,
}
