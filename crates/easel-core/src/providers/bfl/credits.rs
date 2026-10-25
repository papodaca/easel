use crate::error::ProviderError;
use crate::http::{Body, HttpRequest, HttpResponse, Method};
use crate::model_spec::ProviderId;
use serde_json::Value;

/// `GET https://api.bfl.ai/v1/credits`
pub fn credits_request() -> HttpRequest {
    HttpRequest {
        method: Method::Get,
        url: "https://api.bfl.ai/v1/credits".into(),
        headers: Vec::new(),
        auth: Some(ProviderId::Bfl),
        body: Body::Empty,
        max_body_bytes: 64 * 1024,
        redact_query: false,
    }
}

pub fn parse_credits(response: &HttpResponse) -> Result<f64, ProviderError> {
    if response.status != 200 {
        return Err(ProviderError::Http {
            status: response.status,
            body: String::new(),
        });
    }
    let value: Value = serde_json::from_slice(&response.body)
        .map_err(|_| ProviderError::Protocol("credits response was not json".into()))?;
    let credits = value
        .get("credits")
        .and_then(Value::as_f64)
        .filter(|credits| credits.is_finite())
        .ok_or_else(|| ProviderError::Protocol("credits response had no balance".into()))?;
    Ok(credits)
}

#[cfg(test)]
mod tests {
    use super::{credits_request, parse_credits};
    use crate::http::{Body, HttpResponse, Method};
    use crate::model_spec::ProviderId;

    fn response(status: u16, body: &str) -> HttpResponse {
        HttpResponse {
            status,
            headers: Vec::new(),
            body: body.as_bytes().to_vec(),
        }
    }

    #[test]
    fn credits_request_matches_the_documented_endpoint() {
        let request = credits_request();
        assert_eq!(request.method, Method::Get);
        assert_eq!(request.url, "https://api.bfl.ai/v1/credits");
        assert_eq!(request.auth, Some(ProviderId::Bfl));
        assert_eq!(request.body, Body::Empty);
    }

    #[test]
    fn parse_credits_reads_the_balance() {
        assert_eq!(
            parse_credits(&response(200, r#"{"credits": 42}"#)).unwrap(),
            42.0
        );
        assert_eq!(
            parse_credits(&response(200, r#"{"credits": 12.5}"#)).unwrap(),
            12.5
        );
    }

    #[test]
    fn parse_credits_rejects_a_bad_payload() {
        assert!(parse_credits(&response(401, r#"{"credits": 1}"#)).is_err());
        assert!(parse_credits(&response(200, r#"{"balance": 1}"#)).is_err());
        assert!(parse_credits(&response(200, "nope")).is_err());
    }
}
