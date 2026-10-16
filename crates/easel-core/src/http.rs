use std::fmt;

use crate::model_spec::ProviderId;
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
    Delete,
}

#[derive(Clone, PartialEq, Eq)]
pub struct HttpHeader {
    pub name: String,
    pub value: String,
    pub sensitive: bool,
}

impl fmt::Debug for HttpHeader {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let value = if self.sensitive {
            "<redacted>"
        } else {
            self.value.as_str()
        };
        formatter
            .debug_struct("HttpHeader")
            .field("name", &self.name)
            .field("value", &value)
            .field("sensitive", &self.sensitive)
            .finish()
    }
}

#[derive(Clone, PartialEq, Eq)]
pub struct Part {
    pub name: String,
    pub filename: Option<String>,
    pub content_type: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, PartialEq)]
pub enum Body {
    Empty,
    Json(Value),
    Bytes(Vec<u8>, String),
    Multipart { parts: Vec<Part> },
}

impl Body {
    fn kind_name(&self) -> &'static str {
        match self {
            Self::Empty => "Empty",
            Self::Json(_) => "Json",
            Self::Bytes(_, _) => "Bytes",
            Self::Multipart { .. } => "Multipart",
        }
    }
}

impl fmt::Debug for Body {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.kind_name())
    }
}

#[derive(Clone, PartialEq)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub headers: Vec<HttpHeader>,
    pub auth: Option<ProviderId>,
    pub body: Body,
    pub max_body_bytes: u64,
}

impl fmt::Debug for HttpRequest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HttpRequest")
            .field("method", &self.method)
            .field("url", &self.url)
            .field("headers", &self.headers)
            .field("auth", &self.auth)
            .field("body", &self.body)
            .field("max_body_bytes", &self.max_body_bytes)
            .finish()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HttpResponse {
    pub status: u16,
    pub headers: Vec<HttpHeader>,
    pub body: Vec<u8>,
}

#[cfg(test)]
mod tests {
    use super::{Body, HttpHeader, HttpRequest, Method};
    use crate::model_spec::ProviderId;
    use serde_json::json;

    #[test]
    fn debug_redacts_sensitive_headers_and_omits_bodies() {
        let header = HttpHeader {
            name: "x-key".into(),
            value: "super-secret".into(),
            sensitive: true,
        };
        let header_debug = format!("{header:?}");
        assert!(header_debug.contains("<redacted>"));
        assert!(!header_debug.contains("super-secret"));

        let request = HttpRequest {
            method: Method::Post,
            url: "https://api.bfl.ai/v1/flux-3-image".into(),
            headers: vec![header],
            auth: Some(ProviderId::Bfl),
            body: Body::Json(json!({"prompt": "super-secret-body"})),
            max_body_bytes: 1024,
        };
        let request_debug = format!("{request:?}");
        assert!(!request_debug.contains("super-secret"));
        assert!(!request_debug.contains("super-secret-body"));
        assert!(request_debug.contains("Json"));
    }
}
