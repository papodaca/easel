use std::io::Cursor;

use base64::Engine;
use image::imageops::FilterType;
use image::{DynamicImage, GenericImageView};

use crate::job::EncodedImage;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PrepOptions {
    pub max_pixels: u64,
    pub prefer_jpeg: bool,
    pub min_side: u32,
}

impl PrepOptions {
    #[must_use]
    pub fn for_resolution(tier: &str) -> Self {
        Self {
            max_pixels: if tier == "4k" { 16_000_000 } else { 4_000_000 },
            prefer_jpeg: true,
            min_side: 256,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageSummary {
    pub content_hash: String,
    pub width: u32,
    pub height: u32,
    pub mime: String,
}

#[derive(Debug, Error)]
pub enum PrepError {
    #[error("image is {width}x{height}, below the {min_side} minimum")]
    TooSmall {
        width: u32,
        height: u32,
        min_side: u32,
    },
    #[error("{0}")]
    Encode(String),
}

pub fn prepare_for_upload(
    image: &DynamicImage,
    options: PrepOptions,
) -> Result<EncodedImage, PrepError> {
    let (width, height) = image.dimensions();
    if width < options.min_side || height < options.min_side {
        return Err(PrepError::TooSmall {
            width,
            height,
            min_side: options.min_side,
        });
    }
    let (target_w, target_h) = fit_pixels(width, height, options.max_pixels);
    let resized = if (target_w, target_h) == (width, height) {
        image.clone()
    } else {
        image.resize(target_w, target_h, FilterType::Triangle)
    };
    let png = !options.prefer_jpeg || has_transparency(&resized);
    let (bytes, media_type) = if png {
        (encode_png(&resized), "image/png")
    } else {
        (encode_jpeg(&resized)?, "image/jpeg")
    };
    Ok(EncodedImage {
        media_type: media_type.into(),
        width: resized.width(),
        height: resized.height(),
        content_hash: blake3::hash(&bytes).to_hex().to_string(),
        bytes,
    })
}

#[must_use]
pub fn encode_png(image: &DynamicImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .expect("png encode");
    bytes
}

pub fn thumbnail(bytes: &[u8], long_edge: u32) -> Result<Vec<u8>, PrepError> {
    let image = image::load_from_memory(bytes).map_err(|err| PrepError::Encode(err.to_string()))?;
    let (width, height) = image.dimensions();
    if width == 0 || height == 0 {
        return Err(PrepError::Encode("empty image".into()));
    }
    let (target_w, target_h) = if width >= height {
        let height = (u64::from(height) * u64::from(long_edge) / u64::from(width)).max(1) as u32;
        (long_edge, height)
    } else {
        let width = (u64::from(width) * u64::from(long_edge) / u64::from(height)).max(1) as u32;
        (width, long_edge)
    };
    let resized = image.resize_exact(target_w, target_h, FilterType::Triangle);
    encode_jpeg(&resized)
}

impl EncodedImage {
    #[must_use]
    pub fn base64(&self) -> String {
        base64::engine::general_purpose::STANDARD.encode(&self.bytes)
    }

    #[must_use]
    pub fn summary(&self) -> ImageSummary {
        ImageSummary {
            content_hash: self.content_hash.clone(),
            width: self.width,
            height: self.height,
            mime: self.media_type.clone(),
        }
    }
}

fn encode_jpeg(image: &DynamicImage) -> Result<Vec<u8>, PrepError> {
    let rgb = image.to_rgb8();
    let mut bytes = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 92);
    encoder
        .encode(
            rgb.as_raw(),
            rgb.width(),
            rgb.height(),
            image::ExtendedColorType::Rgb8,
        )
        .map_err(|err| PrepError::Encode(err.to_string()))?;
    Ok(bytes)
}

fn has_transparency(image: &DynamicImage) -> bool {
    match image {
        DynamicImage::ImageRgba8(buffer) => buffer.pixels().any(|pixel| pixel.0[3] < 255),
        DynamicImage::ImageLumaA8(buffer) => buffer.pixels().any(|pixel| pixel.0[1] < 255),
        _ => false,
    }
}

pub(crate) fn fit_pixels(width: u32, height: u32, max_pixels: u64) -> (u32, u32) {
    let pixels = u64::from(width) * u64::from(height);
    if pixels <= max_pixels || width == 0 || height == 0 {
        return (width, height);
    }
    let scale = (max_pixels as f64 / pixels as f64).sqrt();
    let mut next_w = ((f64::from(width) * scale).floor() as u32).max(1);
    let mut next_h = ((f64::from(height) * scale).floor() as u32).max(1);
    while u64::from(next_w) * u64::from(next_h) > max_pixels {
        if next_w >= next_h && next_w > 1 {
            next_w -= 1;
        } else if next_h > 1 {
            next_h -= 1;
        } else {
            break;
        }
    }
    (next_w, next_h)
}

#[cfg(test)]
mod tests {
    use super::{PrepOptions, encode_png, prepare_for_upload, thumbnail};
    use base64::Engine;
    use image::{DynamicImage, RgbaImage};

    fn rgb(w: u32, h: u32, pixel: [u8; 3]) -> DynamicImage {
        DynamicImage::ImageRgb8(image::RgbImage::from_fn(w, h, |_, _| image::Rgb(pixel)))
    }

    #[test]
    fn four_megapixel_cap_resizes_and_encodes_jpeg() {
        let image = rgb(6000, 4000, [20, 40, 60]);
        let encoded = prepare_for_upload(&image, PrepOptions::for_resolution("1k")).unwrap();
        assert!(u64::from(encoded.width) * u64::from(encoded.height) <= 4_000_000);
        let ratio = f64::from(encoded.width) / f64::from(encoded.height);
        assert!((ratio - 1.5).abs() < 0.02);
        assert_eq!(encoded.media_type, "image/jpeg");
        assert!(encoded.bytes.starts_with(&[0xFF, 0xD8, 0xFF]));
    }

    #[test]
    fn transparent_rgba_encodes_as_png() {
        let mut image = RgbaImage::from_pixel(1000, 1000, image::Rgba([1, 2, 3, 255]));
        image.put_pixel(0, 0, image::Rgba([1, 2, 3, 0]));
        let encoded = prepare_for_upload(
            &DynamicImage::ImageRgba8(image),
            PrepOptions::for_resolution("1k"),
        )
        .unwrap();
        assert_eq!(encoded.media_type, "image/png");
        assert!(encoded.bytes.starts_with(b"\x89PNG"));
    }

    #[test]
    fn below_256_is_rejected() {
        let error =
            prepare_for_upload(&rgb(200, 200, [1, 2, 3]), PrepOptions::for_resolution("1k"))
                .unwrap_err();
        assert!(error.to_string().contains("256"));
    }

    #[test]
    fn a_lowered_min_side_accepts_128_and_still_rejects_64() {
        let options = PrepOptions {
            min_side: 128,
            ..PrepOptions::for_resolution("1k")
        };
        let accepted = prepare_for_upload(&rgb(128, 128, [1, 2, 3]), options).unwrap();
        assert_eq!((accepted.width, accepted.height), (128, 128));
        let rejected = prepare_for_upload(&rgb(64, 64, [1, 2, 3]), options).unwrap_err();
        assert!(rejected.to_string().contains("128"), "{rejected}");
        assert!(
            prepare_for_upload(&rgb(128, 128, [1, 2, 3]), PrepOptions::for_resolution("1k"))
                .is_err()
        );
    }

    #[test]
    fn an_image_under_the_cap_keeps_its_dimensions() {
        let encoded = prepare_for_upload(
            &rgb(1920, 1080, [9, 9, 9]),
            PrepOptions::for_resolution("2k"),
        )
        .unwrap();
        assert_eq!((encoded.width, encoded.height), (1920, 1080));
    }

    #[test]
    fn base64_is_raw_and_round_trips() {
        let encoded =
            prepare_for_upload(&rgb(300, 300, [4, 5, 6]), PrepOptions::for_resolution("1k"))
                .unwrap();
        let text = encoded.base64();
        assert!(!text.contains("data:"));
        let back = base64::engine::general_purpose::STANDARD
            .decode(text)
            .unwrap();
        assert_eq!(back, encoded.bytes);
    }

    #[test]
    fn thumbnail_uses_a_256_long_edge() {
        let png = encode_png(&rgb(4000, 2000, [7, 8, 9]));
        let thumb = thumbnail(&png, 256).unwrap();
        let decoded = image::load_from_memory(&thumb).unwrap();
        assert_eq!(decoded.width(), 256);
        assert_eq!(decoded.height(), 128);
    }

    #[test]
    fn content_hash_tracks_encoded_bytes() {
        let options = PrepOptions::for_resolution("1k");
        let base = rgb(300, 300, [1, 2, 3]);
        let mut edited = base.clone();
        edited
            .as_mut_rgb8()
            .unwrap()
            .put_pixel(10, 10, image::Rgb([255, 0, 0]));
        let first = prepare_for_upload(&base, options).unwrap();
        let second = prepare_for_upload(&base, options).unwrap();
        let changed = prepare_for_upload(&edited, options).unwrap();
        assert_eq!(first.content_hash, second.content_hash);
        assert_ne!(first.content_hash, changed.content_hash);
        let summary = first.summary();
        assert_eq!(summary.content_hash, first.content_hash);
        assert_eq!(summary.mime, first.media_type);
    }

    #[test]
    fn four_k_resolution_allows_16_megapixels() {
        assert_eq!(PrepOptions::for_resolution("4k").max_pixels, 16_000_000);
        assert_eq!(PrepOptions::for_resolution("1k").max_pixels, 4_000_000);
    }
}
