use crate::error::{FieldError, ProviderError};
use crate::http::HttpResponse;
use crate::model_spec::ProviderId;
use crate::providers::{CostQuote, JobId, PollOutcome, Submitted};
use serde_json::Value;
use std::time::Duration;

const BODY_CAP: usize = 64 * 1024;
const RETRY_CAP_SECS: f64 = 300.0;
const SAFETY_MAX: i64 = 7;

pub const READY_POLL_BODY: &str = r#"{"status":"Ready","result":{"sample":"https://delivery.bfl.ai/img.png","prompt":"expanded","duration":3.5},"cost":4.5}"#;

const SCHEMA_KEYS: &[&str] = &[
    "prompt",
    "images",
    "aspect_ratio",
    "resolution",
    "safety_tolerance",
    "grounding",
    "version",
];

pub fn parse_submit(response: &HttpResponse) -> Result<Submitted, ProviderError> {
    match response.status {
        401 | 403 => return Err(ProviderError::Auth),
        402 => return Err(ProviderError::OutOfCredits),
        429 => return Err(rate_limited(response)),
        400 => return Err(images_rejected(response)),
        422 => return Err(validation_error(response)),
        200 => {}
        status if (500..600).contains(&status) => {
            return Err(ProviderError::Network(lossy_prefix(&response.body)));
        }
        status => {
            return Err(ProviderError::Http {
                status,
                body: lossy_prefix(&response.body),
            });
        }
    }
    let value = json_body(&response.body)?;
    let id = value
        .get("id")
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderError::Protocol("submit response had no id".into()))?;
    let job_id = JobId::parse(id).map_err(ProviderError::Protocol)?;
    let poll = value
        .get("polling_url")
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderError::Protocol("submit response had no polling_url".into()))?;
    if !ProviderId::Bfl.allows_url(poll) {
        return Err(ProviderError::Protocol(
            "polling url is not an allowed https host".into(),
        ));
    }
    let poll_url = url::Url::parse(poll)
        .map_err(|_| ProviderError::Protocol("polling url was not valid".into()))?;
    Ok(Submitted {
        job_id,
        poll_url,
        quote: quote_from(&value),
    })
}

pub fn parse_poll(response: &HttpResponse) -> PollOutcome {
    if (500..600).contains(&response.status) {
        if let Ok(value) = serde_json::from_slice::<Value>(&response.body)
            && value.get("status").and_then(Value::as_str).is_some()
        {
            return outcome_from_body(&value);
        }
        return PollOutcome::Failed(ProviderError::Network(lossy_prefix(&response.body)));
    }
    match response.status {
        401 | 403 => return PollOutcome::Failed(ProviderError::Auth),
        402 => return PollOutcome::Failed(ProviderError::OutOfCredits),
        429 => return PollOutcome::Failed(rate_limited(response)),
        _ => {}
    }
    match serde_json::from_slice::<Value>(&response.body) {
        Ok(value) => outcome_from_body(&value),
        Err(_) if response.status == 200 => {
            PollOutcome::Failed(ProviderError::Protocol("poll response was not json".into()))
        }
        Err(_) => PollOutcome::Failed(ProviderError::Http {
            status: response.status,
            body: lossy_prefix(&response.body),
        }),
    }
}

#[must_use]
pub fn extract_result_mime(content_type: Option<&str>, magic: &[u8]) -> Option<&'static str> {
    if magic.starts_with(b"GIF87a") || magic.starts_with(b"GIF89a") {
        return None;
    }
    if magic.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Some("image/png");
    }
    if magic.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if magic.len() >= 12 && magic.starts_with(b"RIFF") && &magic[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    let media = content_type
        .unwrap_or("")
        .split(';')
        .next()
        .unwrap_or("")
        .trim();
    match media {
        "image/png" | "image/jpeg" | "image/webp" => Some(match media {
            "image/png" => "image/png",
            "image/jpeg" => "image/jpeg",
            _ => "image/webp",
        }),
        _ => None,
    }
}

fn outcome_from_body(value: &Value) -> PollOutcome {
    let status = value.get("status").and_then(Value::as_str).unwrap_or("");
    match status {
        "Pending" | "Reasoning" | "Generating" => PollOutcome::InFlight {
            status: status.to_string(),
            progress: progress_of(value),
        },
        "Ready" => ready_outcome(value),
        "Request Moderated" | "Content Moderated" => {
            PollOutcome::Failed(ProviderError::Moderated {
                reasons: moderation_reasons(value),
            })
        }
        "Error" => PollOutcome::Failed(ProviderError::Protocol(format!(
            "Error: {}",
            detail_text(value)
        ))),
        "Task not found" => PollOutcome::Failed(ProviderError::Protocol("Task not found".into())),
        "No Card Generated" => {
            PollOutcome::Failed(ProviderError::Protocol("No Card Generated".into()))
        }
        "" => PollOutcome::Failed(ProviderError::Protocol(
            "poll response had no status".into(),
        )),
        other => PollOutcome::InFlight {
            status: other.to_string(),
            progress: progress_of(value),
        },
    }
}

fn ready_outcome(value: &Value) -> PollOutcome {
    let sample = value
        .pointer("/result/sample")
        .and_then(Value::as_str)
        .unwrap_or("");
    if !ProviderId::Bfl.allows_url(sample) {
        return PollOutcome::Failed(ProviderError::Protocol("unexpected result host".into()));
    }
    let Ok(url) = url::Url::parse(sample) else {
        return PollOutcome::Failed(ProviderError::Protocol("result url was not valid".into()));
    };
    PollOutcome::Ready {
        url,
        settled_cost: value.get("cost").and_then(Value::as_f64),
        prompt: value
            .pointer("/result/prompt")
            .and_then(Value::as_str)
            .map(str::to_string),
        duration: value.pointer("/result/duration").and_then(Value::as_f64),
    }
}

fn moderation_reasons(value: &Value) -> Vec<String> {
    value
        .pointer("/details/Moderation Reasons")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn detail_text(value: &Value) -> String {
    match value.get("details") {
        Some(Value::String(text)) => text.clone(),
        Some(other) => other.to_string(),
        None => String::new(),
    }
}

fn progress_of(value: &Value) -> Option<f32> {
    value
        .get("progress")
        .and_then(Value::as_f64)
        .map(|number| number as f32)
}

fn quote_from(value: &Value) -> Option<CostQuote> {
    let cost = value.get("cost").and_then(Value::as_f64)?;
    Some(CostQuote {
        cost,
        input_mp: value.get("input_mp").and_then(Value::as_f64).unwrap_or(0.0),
        output_mp: value
            .get("output_mp")
            .and_then(Value::as_f64)
            .unwrap_or(0.0),
    })
}

fn json_body(body: &[u8]) -> Result<Value, ProviderError> {
    serde_json::from_slice(body)
        .map_err(|_| ProviderError::Protocol("response was not json".into()))
}

fn rate_limited(response: &HttpResponse) -> ProviderError {
    ProviderError::RateLimited {
        retry_after: retry_after(response),
    }
}

fn retry_after(response: &HttpResponse) -> Option<Duration> {
    let from_body = serde_json::from_slice::<Value>(&response.body)
        .ok()
        .and_then(|value| value.get("retry_after").and_then(Value::as_f64));
    let from_header = response
        .headers
        .iter()
        .find(|header| header.name.eq_ignore_ascii_case("retry-after"))
        .and_then(|header| header.value.parse::<f64>().ok());
    let seconds = from_body.or(from_header)?;
    Some(Duration::from_secs_f64(seconds.clamp(0.0, RETRY_CAP_SECS)))
}

fn images_rejected(response: &HttpResponse) -> ProviderError {
    let message = serde_json::from_slice::<Value>(&response.body)
        .ok()
        .and_then(|value| {
            value
                .get("detail")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_else(|| "reference image was rejected".into());
    ProviderError::Validation {
        message: message.clone(),
        field_errors: vec![FieldError {
            key: "images".into(),
            message,
            approved_max: None,
        }],
    }
}

fn validation_error(response: &HttpResponse) -> ProviderError {
    let Ok(value) = serde_json::from_slice::<Value>(&response.body) else {
        return ProviderError::Validation {
            message: lossy_prefix(&response.body),
            field_errors: Vec::new(),
        };
    };
    let Some(details) = value.get("detail").and_then(Value::as_array) else {
        let message = value
            .get("detail")
            .and_then(Value::as_str)
            .unwrap_or("validation failed")
            .to_string();
        return ProviderError::Validation {
            message,
            field_errors: Vec::new(),
        };
    };
    let mut field_errors = Vec::new();
    let mut messages = Vec::new();
    for detail in details {
        let msg = detail
            .get("msg")
            .and_then(Value::as_str)
            .unwrap_or("invalid")
            .to_string();
        let key = loc_key(detail.get("loc"));
        if key.is_empty() {
            messages.push(msg.clone());
        } else {
            messages.push(format!("{key}: {msg}"));
        }
        if SCHEMA_KEYS.contains(&key.as_str()) {
            field_errors.push(FieldError {
                approved_max: (key == "safety_tolerance")
                    .then(|| largest_int(&msg))
                    .flatten(),
                key,
                message: msg,
            });
        }
    }
    ProviderError::Validation {
        message: messages.join("; "),
        field_errors,
    }
}

fn loc_key(loc: Option<&Value>) -> String {
    loc.and_then(Value::as_array)
        .and_then(|parts| {
            parts.iter().rev().find_map(|part| {
                let text = part.as_str()?;
                (text != "body").then(|| text.to_string())
            })
        })
        .unwrap_or_default()
}

fn largest_int(message: &str) -> Option<i64> {
    let mut best: Option<i64> = None;
    let mut current: Option<i64> = None;
    for byte in message.bytes().chain(std::iter::once(b' ')) {
        if byte.is_ascii_digit() {
            let digit = i64::from(byte - b'0');
            current = Some(
                current
                    .unwrap_or(0)
                    .saturating_mul(10)
                    .saturating_add(digit),
            );
        } else if let Some(number) = current.take() {
            best = Some(best.map_or(number, |prev| prev.max(number)));
        }
    }
    best.map(|number| number.clamp(0, SAFETY_MAX))
}

fn lossy_prefix(body: &[u8]) -> String {
    let end = body.len().min(BODY_CAP);
    let mut end = end;
    while end > 0 && std::str::from_utf8(&body[..end]).is_err() {
        end -= 1;
    }
    String::from_utf8_lossy(&body[..end]).into_owned()
}

#[cfg(test)]
mod tests {
    use super::{READY_POLL_BODY, extract_result_mime, parse_poll, parse_submit};
    use crate::error::ProviderError;
    use crate::http::{HttpHeader, HttpResponse};
    use crate::providers::PollOutcome;
    use std::time::Duration;

    fn response(status: u16, body: &str) -> HttpResponse {
        HttpResponse {
            status,
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    fn header_response(status: u16, body: &str, name: &str, value: &str) -> HttpResponse {
        HttpResponse {
            status,
            headers: vec![HttpHeader {
                name: name.into(),
                value: value.into(),
                sensitive: false,
            }],
            body: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn submit_200_carries_id_poll_url_and_quote() {
        let submitted = parse_submit(&response(
            200,
            r#"{"id":"abc-123_XYZ","polling_url":"https://api.bfl.ai/v1/get_result?id=abc-123_XYZ","cost":1.25,"input_mp":2.5,"output_mp":1.0}"#,
        ))
        .unwrap();
        assert_eq!(submitted.job_id.as_str(), "abc-123_XYZ");
        assert_eq!(
            submitted.poll_url.as_str(),
            "https://api.bfl.ai/v1/get_result?id=abc-123_XYZ"
        );
        let quote = submitted.quote.unwrap();
        assert!((quote.cost - 1.25).abs() < 1e-9);
        assert!((quote.input_mp - 2.5).abs() < 1e-9);
        assert!((quote.output_mp - 1.0).abs() < 1e-9);
    }

    #[test]
    fn submit_auth_credit_rate_and_image_errors() {
        assert!(matches!(
            parse_submit(&response(401, "nope")).unwrap_err(),
            ProviderError::Auth
        ));
        assert!(matches!(
            parse_submit(&response(403, "nope")).unwrap_err(),
            ProviderError::Auth
        ));
        assert!(matches!(
            parse_submit(&response(402, "nope")).unwrap_err(),
            ProviderError::OutOfCredits
        ));
        let limited = parse_submit(&response(429, r#"{"retry_after":12}"#)).unwrap_err();
        assert_eq!(
            limited,
            ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(12))
            }
        );
        let images =
            parse_submit(&response(400, r#"{"detail":"reference too large"}"#)).unwrap_err();
        match images {
            ProviderError::Validation { field_errors, .. } => {
                assert_eq!(field_errors[0].key, "images");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn submit_422_maps_safety_tolerance_and_clamps_the_cap() {
        let error = parse_submit(&response(
            422,
            r#"{"detail":[{"loc":["body","safety_tolerance"],"msg":"Approved levels: 0, 1, 2, 3, 4","type":"value_error"}]}"#,
        ))
        .unwrap_err();
        match error {
            ProviderError::Validation { field_errors, .. } => {
                assert_eq!(field_errors[0].key, "safety_tolerance");
                assert_eq!(field_errors[0].approved_max, Some(4));
            }
            other => panic!("{other:?}"),
        }

        let clamped = parse_submit(&response(
            422,
            r#"{"detail":[{"loc":["body","safety_tolerance"],"msg":"approved maximum 999","type":"value_error"}]}"#,
        ))
        .unwrap_err();
        match clamped {
            ProviderError::Validation { field_errors, .. } => {
                assert_eq!(field_errors[0].approved_max, Some(7));
            }
            other => panic!("{other:?}"),
        }

        let unknown = parse_submit(&response(
            422,
            r#"{"detail":[{"loc":["body","nonexistent"],"msg":"no such field","type":"value_error"}]}"#,
        ))
        .unwrap_err();
        match unknown {
            ProviderError::Validation {
                message,
                field_errors,
            } => {
                assert!(field_errors.is_empty());
                assert!(message.contains("nonexistent") || message.contains("no such field"));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn submit_rejects_bad_ids_and_poll_urls() {
        for id in ["../x", "a/b", &"a".repeat(65)] {
            let body =
                format!(r#"{{"id":"{id}","polling_url":"https://api.bfl.ai/v1/get_result?id=1"}}"#);
            assert!(
                matches!(
                    parse_submit(&response(200, &body)).unwrap_err(),
                    ProviderError::Protocol(_)
                ),
                "{id}"
            );
        }
        assert!(
            parse_submit(&response(
                200,
                r#"{"id":"abc-123_XYZ","polling_url":"https://api.bfl.ai/v1/get_result"}"#
            ))
            .is_ok()
        );

        for url in [
            "https://evil.example/v1/get_result",
            "http://api.bfl.ai/v1/get_result",
        ] {
            let body = format!(r#"{{"id":"abc","polling_url":"{url}"}}"#);
            assert!(matches!(
                parse_submit(&response(200, &body)).unwrap_err(),
                ProviderError::Protocol(_)
            ));
        }
        let regional = parse_submit(&response(
            200,
            r#"{"id":"abc","polling_url":"https://api.eu.bfl.ai/v1/get_result?id=abc"}"#,
        ))
        .unwrap();
        assert!(regional.poll_url.as_str().contains("api.eu.bfl.ai"));
    }

    #[test]
    fn retry_after_is_clamped_to_five_minutes() {
        let error = parse_submit(&response(429, r#"{"retry_after":999999}"#)).unwrap_err();
        assert_eq!(
            error,
            ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(300))
            }
        );
    }

    #[test]
    fn poll_statuses_map_to_outcomes() {
        for status in ["Pending", "Reasoning", "Generating"] {
            let body = format!(r#"{{"status":"{status}","progress":0.4}}"#);
            match parse_poll(&response(200, &body)) {
                PollOutcome::InFlight {
                    status: got,
                    progress,
                } => {
                    assert_eq!(got, status);
                    assert_eq!(progress, Some(0.4));
                }
                other => panic!("{other:?}"),
            }
        }
        match parse_poll(&response(200, READY_POLL_BODY)) {
            PollOutcome::Ready {
                url,
                settled_cost,
                prompt,
                duration,
            } => {
                assert_eq!(url.as_str(), "https://delivery.bfl.ai/img.png");
                assert_eq!(settled_cost, Some(4.5));
                assert_eq!(prompt.as_deref(), Some("expanded"));
                assert_eq!(duration, Some(3.5));
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn moderation_error_and_missing_task_fail() {
        match parse_poll(&response(
            200,
            r#"{"status":"Request Moderated","details":{"Moderation Reasons":["Sexual","Violence"]}}"#,
        )) {
            PollOutcome::Failed(ProviderError::Moderated { reasons }) => {
                assert_eq!(reasons, vec!["Sexual".to_string(), "Violence".to_string()]);
            }
            other => panic!("{other:?}"),
        }
        match parse_poll(&response(
            200,
            r#"{"status":"Content Moderated","details":{"Moderation Reasons":["Hate"]}}"#,
        )) {
            PollOutcome::Failed(ProviderError::Moderated { reasons }) => {
                assert_eq!(reasons, vec!["Hate".to_string()]);
            }
            other => panic!("{other:?}"),
        }
        match parse_poll(&response(200, r#"{"status":"Error","details":"boom"}"#)) {
            PollOutcome::Failed(ProviderError::Protocol(message)) => {
                assert!(message.contains("Error"))
            }
            other => panic!("{other:?}"),
        }
        match parse_poll(&response(200, r#"{"status":"Task not found"}"#)) {
            PollOutcome::Failed(ProviderError::Protocol(message)) => {
                assert!(message.contains("Task not found"));
                assert_ne!(message, "Error");
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn service_unavailable_json_stays_in_flight_and_html_is_network() {
        match parse_poll(&response(503, r#"{"status":"Pending"}"#)) {
            PollOutcome::InFlight { status, .. } => assert_eq!(status, "Pending"),
            other => panic!("{other:?}"),
        }
        match parse_poll(&response(503, "<html>down</html>")) {
            PollOutcome::Failed(ProviderError::Network(_)) => {}
            other => panic!("{other:?}"),
        }
        match parse_poll(&response(200, r#"{"status":"Baking"}"#)) {
            PollOutcome::InFlight { status, .. } => assert_eq!(status, "Baking"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn oversized_html_503_keeps_at_most_64_kib() {
        let body = vec![b'a'; 5 * 1024 * 1024];
        let response = HttpResponse {
            status: 503,
            headers: Vec::new(),
            body,
        };
        match parse_poll(&response) {
            PollOutcome::Failed(ProviderError::Network(message)) => {
                assert!(message.len() <= 64 * 1024);
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn mime_sniff_prefers_magic_bytes() {
        assert_eq!(
            extract_result_mime(Some("image/jpeg"), b"\x89PNG\r\n\x1a\nrest"),
            Some("image/png")
        );
        assert_eq!(extract_result_mime(Some("image/png"), b"GIF89a"), None);
    }

    #[test]
    fn retry_after_header_is_read() {
        let error = parse_submit(&header_response(429, "{}", "retry-after", "9")).unwrap_err();
        assert_eq!(
            error,
            ProviderError::RateLimited {
                retry_after: Some(Duration::from_secs(9))
            }
        );
    }
}
