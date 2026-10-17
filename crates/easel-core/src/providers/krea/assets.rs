use crate::error::ProviderError;
use crate::http::{Body, HttpRequest, HttpResponse, Method, Part};
use crate::job::EncodedImage;
use crate::model_spec::ProviderId;
use crate::providers::{ImageInput, ReferenceStep};
use serde_json::Value;

pub fn reference_steps(images: &[EncodedImage]) -> Vec<ReferenceStep> {
    images
        .iter()
        .map(|image| ReferenceStep::Upload {
            request: upload_request(image),
            content_hash: image.content_hash.clone(),
        })
        .collect()
}

pub fn parse_upload(response: &HttpResponse) -> Result<ImageInput, ProviderError> {
    if response.status != 200 {
        return Err(crate::providers::krea::protocol::parse_error(response));
    }
    let value = serde_json::from_slice::<Value>(&response.body)
        .map_err(|_| ProviderError::Protocol("asset response was not json".into()))?;
    let url = value
        .get("image_url")
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderError::Protocol("asset response had no image_url".into()))?;
    if !ProviderId::Krea.allows_url(url) {
        return Err(ProviderError::Protocol(
            "asset url is not an allowed https host".into(),
        ));
    }
    Ok(ImageInput::Url(url.to_owned()))
}

fn upload_request(image: &EncodedImage) -> HttpRequest {
    let ext = match image.media_type.as_str() {
        "image/jpeg" | "image/jpg" => "jpg",
        "image/png" => "png",
        "image/webp" => "webp",
        _ => "bin",
    };
    HttpRequest {
        method: Method::Post,
        url: format!("{}/assets", ProviderId::Krea.base_url()),
        headers: Vec::new(),
        auth: Some(ProviderId::Krea),
        body: Body::Multipart {
            parts: vec![Part {
                name: "file".into(),
                filename: Some(format!("image.{ext}")),
                content_type: image.media_type.clone(),
                bytes: image.bytes.clone(),
            }],
        },
        max_body_bytes: 2 * 1024 * 1024,
    }
}

#[cfg(test)]
mod tests {
    use super::{parse_upload, reference_steps};
    use crate::error::ProviderError;
    use crate::http::{Body, HttpResponse, Method};
    use crate::job::EncodedImage;
    use crate::model_spec::ProviderId;
    use crate::providers::{ImageInput, ReferenceStep};

    fn encoded(media_type: &str, bytes: Vec<u8>) -> EncodedImage {
        EncodedImage {
            media_type: media_type.into(),
            bytes,
            width: 512,
            height: 512,
            content_hash: "hash".into(),
        }
    }

    #[test]
    fn each_image_uploads_once_under_a_fixed_filename() {
        let jpeg = encoded("image/jpeg", vec![0; 5 * 1024 * 1024]);
        let png = encoded("image/png", vec![1; 100 * 1024]);
        let steps = reference_steps(&[jpeg, png]);
        assert_eq!(steps.len(), 2);
        assert_part(&steps[0], "image.jpg", "image/jpeg");
        assert_part(&steps[1], "image.png", "image/png");
    }

    fn assert_part(step: &ReferenceStep, filename: &str, content_type: &str) {
        let ReferenceStep::Upload { request, .. } = step else {
            panic!("expected upload");
        };
        assert_eq!(request.method, Method::Post);
        assert_eq!(request.url, "https://api.krea.ai/assets");
        assert_eq!(request.auth, Some(ProviderId::Krea));
        let Body::Multipart { parts } = &request.body else {
            panic!("multipart");
        };
        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0].name, "file");
        assert_eq!(parts[0].filename.as_deref(), Some(filename));
        assert_eq!(parts[0].content_type, content_type);
        assert!(!filename.contains('/'));
        assert!(!filename.contains('\\'));
    }

    #[test]
    fn asset_urls_must_stay_on_the_krea_allowlist() {
        let ok = parse_upload(&HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: include_bytes!("../../../tests/fixtures/krea/asset.json").to_vec(),
        })
        .unwrap();
        assert_eq!(
            ok,
            ImageInput::Url("https://api.krea.ai/assets/fixed".into())
        );
        let err = parse_upload(&HttpResponse {
            status: 200,
            headers: Vec::new(),
            body: br#"{"image_url":"https://evil.example/a.png"}"#.to_vec(),
        });
        assert!(matches!(err, Err(ProviderError::Protocol(_))));
    }
}
