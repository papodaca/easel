use crate::error::ProviderError;
use crate::scene::BoxElement;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EncodedImage {
    pub media_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobRequest {
    pub model_id: String,
    pub compiled_prompt: String,
    pub params: BTreeMap<String, Value>,
    pub images: Vec<EncodedImage>,
    pub box_snapshot: Vec<BoxElement>,
    pub created_at: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Submitting,
    Polling {
        attempt: u32,
        progress: Option<f32>,
    },
    Downloading,
    Done {
        result_path: String,
        cost: Option<f64>,
    },
    Failed {
        error: ProviderError,
    },
    Dismissed,
    Expired,
}

impl JobState {
    #[must_use]
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Done { .. } | Self::Failed { .. } | Self::Dismissed | Self::Expired
        )
    }

    #[must_use]
    pub fn is_in_flight(&self) -> bool {
        matches!(
            self,
            Self::Queued | Self::Submitting | Self::Polling { .. } | Self::Downloading
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{EncodedImage, JobRequest, JobState};
    use crate::error::ProviderError;
    use crate::scene::{BoxElement, BoxId, BoxKind};
    use serde_json::json;
    use std::collections::BTreeMap;
    use std::time::Duration;

    fn request() -> JobRequest {
        JobRequest {
            model_id: "flux-3".into(),
            compiled_prompt: "a fox".into(),
            params: BTreeMap::from([("steps".into(), json!(20))]),
            images: vec![EncodedImage {
                media_type: "image/png".into(),
                bytes: vec![1, 2, 3],
            }],
            box_snapshot: vec![BoxElement {
                id: BoxId::parse("animal_1").unwrap(),
                kind: BoxKind::Keep,
                desc: "fox".into(),
                src: None,
                tgt: None,
                from_image: None,
            }],
            created_at: "2026-10-15T00:00:00Z".into(),
        }
    }

    fn states() -> Vec<JobState> {
        vec![
            JobState::Queued,
            JobState::Submitting,
            JobState::Polling {
                attempt: 2,
                progress: Some(0.5),
            },
            JobState::Downloading,
            JobState::Done {
                result_path: "out.png".into(),
                cost: Some(1.5),
            },
            JobState::Failed {
                error: ProviderError::RateLimited {
                    retry_after: Some(Duration::from_secs(3)),
                },
            },
            JobState::Dismissed,
            JobState::Expired,
        ]
    }

    #[test]
    fn job_request_and_every_state_round_trip() {
        let request = request();
        let json = serde_json::to_value(&request).unwrap();
        let back: JobRequest = serde_json::from_value(json).unwrap();
        assert_eq!(request, back);

        for state in states() {
            let json = serde_json::to_value(&state).unwrap();
            let back: JobState = serde_json::from_value(json.clone()).unwrap();
            assert_eq!(state, back);
        }
    }

    #[test]
    fn downloading_state_has_no_url() {
        let json = serde_json::to_value(JobState::Downloading)
            .unwrap()
            .to_string();
        assert!(!json.to_lowercase().contains("url"));
        assert!(!json.contains("http"));
    }

    #[test]
    fn terminal_and_in_flight_match_the_lifecycle() {
        for state in states() {
            let terminal = matches!(
                state,
                JobState::Done { .. }
                    | JobState::Failed { .. }
                    | JobState::Dismissed
                    | JobState::Expired
            );
            let in_flight = matches!(
                state,
                JobState::Queued
                    | JobState::Submitting
                    | JobState::Polling { .. }
                    | JobState::Downloading
            );
            assert_eq!(state.is_terminal(), terminal);
            assert_eq!(state.is_in_flight(), in_flight);
        }
    }
}
