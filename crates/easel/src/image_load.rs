use std::io::Cursor;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use easel_core::geometry::Size;
use gpui::{App, Entity, RenderImage};
use image::{DynamicImage, ImageDecoder, ImageReader, Limits};

use crate::app_state::AppState;
use crate::notifications::{Notifications, Toast, ToastLevel};

const MAX_SIDE: u32 = 16_384;
const MAX_PIXELS: u64 = 64_000_000;
const MAX_ALLOC: u64 = 1024 * 1024 * 1024;

#[derive(Debug)]
pub struct DecodedImage {
    pub size: Size,
    pub render: Arc<RenderImage>,
    /// Kept so a later step can re-encode without reading the file again.
    #[allow(dead_code)]
    pub rgba_for_export: Arc<DynamicImage>,
}

#[derive(Debug)]
pub enum ImageError {
    Unsupported {
        label: String,
    },
    Dimensions {
        label: String,
        width: u32,
        height: u32,
    },
    Decode {
        label: String,
        detail: String,
    },
}

impl std::fmt::Display for ImageError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Unsupported { label } => write!(formatter, "{label}: unsupported image"),
            Self::Dimensions {
                label,
                width,
                height,
            } => write!(
                formatter,
                "{label}: image is {width}x{height}, over the size limit"
            ),
            Self::Decode { label, detail } => write!(formatter, "{label}: {detail}"),
        }
    }
}

impl std::error::Error for ImageError {}

pub fn decode_image(bytes: &[u8], label: &str) -> Result<DecodedImage, ImageError> {
    let Some(format) = sniff_format(bytes) else {
        return Err(ImageError::Unsupported {
            label: label.to_owned(),
        });
    };
    if let Some((width, height)) = header_dimensions(bytes)
        && over_limit(width, height)
    {
        return Err(ImageError::Dimensions {
            label: label.to_owned(),
            width,
            height,
        });
    }

    let reader = ImageReader::with_format(Cursor::new(bytes), format);
    let mut decoder = reader.into_decoder().map_err(|err| ImageError::Decode {
        label: label.to_owned(),
        detail: err.to_string(),
    })?;
    let (width, height) = decoder.dimensions();
    if over_limit(width, height) {
        return Err(ImageError::Dimensions {
            label: label.to_owned(),
            width,
            height,
        });
    }

    let mut limits = Limits::default();
    limits.max_alloc = Some(MAX_ALLOC);
    decoder
        .set_limits(limits)
        .map_err(|err| ImageError::Decode {
            label: label.to_owned(),
            detail: err.to_string(),
        })?;
    let orientation = decoder.orientation().map_err(|err| ImageError::Decode {
        label: label.to_owned(),
        detail: err.to_string(),
    })?;
    let mut image = DynamicImage::from_decoder(decoder).map_err(|err| ImageError::Decode {
        label: label.to_owned(),
        detail: err.to_string(),
    })?;
    image.apply_orientation(orientation);

    let mut rgba = image.to_rgba8();
    let size = Size {
        w: rgba.width(),
        h: rgba.height(),
    };
    let rgba_for_export = Arc::new(DynamicImage::ImageRgba8(rgba.clone()));
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let render = Arc::new(RenderImage::new([image::Frame::new(rgba)]));
    Ok(DecodedImage {
        size,
        render,
        rgba_for_export,
    })
}

fn over_limit(width: u32, height: u32) -> bool {
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    width > MAX_SIDE || height > MAX_SIDE || pixels > MAX_PIXELS
}

fn header_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() >= 24 && bytes.starts_with(b"\x89PNG\r\n\x1a\n") && &bytes[12..16] == b"IHDR" {
        let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?);
        let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?);
        return Some((width, height));
    }
    None
}

fn sniff_format(bytes: &[u8]) -> Option<image::ImageFormat> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some(image::ImageFormat::Png)
    } else if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        Some(image::ImageFormat::Jpeg)
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        Some(image::ImageFormat::WebP)
    } else {
        None
    }
}

pub fn load_paths(
    state: Entity<AppState>,
    notifications: Entity<Notifications>,
    paths: Vec<PathBuf>,
    target: LoadTarget,
    cx: &mut App,
) {
    let mut paths = paths.into_iter();
    let Some(first) = paths.next() else {
        return;
    };
    let ignored = paths.count();
    if ignored > 0 {
        let noun = if ignored == 1 { "file" } else { "files" };
        push_toast(
            &notifications,
            ToastLevel::Info,
            format!("Ignored {ignored} {noun}"),
            cx,
        );
    }
    let label = first.display().to_string();
    notifications.update(cx, |notes, cx| notes.set_status("Loading…", cx));
    cx.spawn(async move |cx| {
        let read = cx
            .background_executor()
            .spawn(async move { std::fs::read(first) })
            .await;
        let result = match read {
            Ok(bytes) => {
                cx.background_executor()
                    .spawn(async move { decode_image(&bytes, &label) })
                    .await
            }
            Err(err) => Err(ImageError::Decode {
                label,
                detail: err.to_string(),
            }),
        };
        cx.update(|cx| apply_load_result(&state, &notifications, result, target, cx));
    })
    .detach();
}

pub fn load_bytes(
    state: Entity<AppState>,
    notifications: Entity<Notifications>,
    bytes: Vec<u8>,
    label: String,
    target: LoadTarget,
    cx: &mut App,
) {
    notifications.update(cx, |notes, cx| notes.set_status("Loading…", cx));
    let read: Result<Vec<u8>, std::io::Error> = Ok(bytes);
    decode_task(read, label, target).detach_and_report(state, notifications, cx);
}

fn decode_task(
    read: Result<Vec<u8>, std::io::Error>,
    label: String,
    target: LoadTarget,
) -> DecodeJob {
    DecodeJob {
        read,
        label,
        target,
    }
}

struct DecodeJob {
    read: Result<Vec<u8>, std::io::Error>,
    label: String,
    target: LoadTarget,
}

impl DecodeJob {
    fn detach_and_report(
        self,
        state: Entity<AppState>,
        notifications: Entity<Notifications>,
        cx: &mut App,
    ) {
        let DecodeJob {
            read,
            label,
            target,
        } = self;
        cx.spawn(async move |cx| {
            let result = match read {
                Ok(bytes) => {
                    cx.background_executor()
                        .spawn(async move { decode_image(&bytes, &label) })
                        .await
                }
                Err(err) => Err(ImageError::Decode {
                    label,
                    detail: err.to_string(),
                }),
            };
            cx.update(|cx| apply_load_result(&state, &notifications, result, target, cx));
        })
        .detach();
    }
}

#[derive(Clone, Copy)]
pub enum LoadTarget {
    ReplaceSource,
    Paste,
}

pub fn apply_load_result(
    state: &Entity<AppState>,
    notifications: &Entity<Notifications>,
    result: Result<DecodedImage, ImageError>,
    target: LoadTarget,
    cx: &mut App,
) {
    match result {
        Ok(image) => {
            let width = image.size.w;
            let height = image.size.h;
            state.update(cx, |state, cx| match target {
                LoadTarget::ReplaceSource => state.set_image(Arc::new(image), cx),
                LoadTarget::Paste if state.scene.source_image.is_none() => {
                    state.set_image(Arc::new(image), cx);
                }
                LoadTarget::Paste => {
                    let _ = state.add_extra(Arc::new(image), cx);
                }
            });
            notifications.update(cx, |notes, cx| {
                notes.set_status(format!("{width}x{height}"), cx);
            });
        }
        Err(error) => {
            push_toast(notifications, ToastLevel::Error, error.to_string(), cx);
            notifications.update(cx, |notes, cx| notes.set_status("Ready", cx));
        }
    }
}

fn push_toast(
    notifications: &Entity<Notifications>,
    level: ToastLevel,
    text: String,
    cx: &mut App,
) {
    notifications.update(cx, |notes, cx| {
        notes.push(
            Toast {
                level,
                text: text.into(),
                ttl: Duration::from_secs(6),
                action: None,
            },
            cx,
        );
    });
}

pub fn prompt_failure_text(error: &impl std::fmt::Display) -> String {
    let text = error.to_string();
    if text.contains("xdg-desktop-portal") {
        text
    } else {
        format!("Couldn't open the file dialog ({text}). xdg-desktop-portal is required.")
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use super::{ImageError, apply_load_result, decode_image};
    use crate::app_state::AppState;
    use crate::notifications::Notifications;
    use gpui::{TestAppContext, prelude::*};

    fn fixture(name: &str) -> Vec<u8> {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name);
        std::fs::read(&path).unwrap_or_else(|err| panic!("read {}: {err}", path.display()))
    }

    #[test]
    fn decodes_png_jpeg_and_webp_dimensions() {
        let png = decode_image(&fixture("pixel.png"), "pixel.png").unwrap();
        assert_eq!((png.size.w, png.size.h), (1, 1));

        let jpeg = decode_image(&fixture("photo.jpg"), "photo.jpg").unwrap();
        assert_eq!((jpeg.size.w, jpeg.size.h), (8, 4));

        let webp = decode_image(&fixture("picture.webp"), "picture.webp").unwrap();
        assert_eq!((webp.size.w, webp.size.h), (6, 4));
    }

    #[test]
    fn exif_orientation_6_swaps_width_and_height() {
        let image = decode_image(&fixture("orient6.jpg"), "orient6.jpg").unwrap();
        assert_eq!((image.size.w, image.size.h), (1, 2));
    }

    #[test]
    fn text_file_error_names_the_file() {
        let error = decode_image(&fixture("notes.txt"), "notes.txt").unwrap_err();
        let message = error.to_string();
        assert!(
            message.contains("notes.txt"),
            "error should name the file, got {message}"
        );
    }

    #[test]
    fn huge_png_header_fails_the_dimension_check() {
        let error = decode_image(&fixture("huge.png"), "huge.png").unwrap_err();
        assert!(
            matches!(
                error,
                ImageError::Dimensions {
                    width: 100_000,
                    height: 100_000,
                    ..
                }
            ),
            "expected the dimension check, got {error:?}"
        );
    }

    #[test]
    fn gif_and_bmp_are_unsupported() {
        let gif = decode_image(&fixture("tiny.gif"), "tiny.gif").unwrap_err();
        assert!(matches!(gif, ImageError::Unsupported { .. }), "{gif:?}");
        let bmp = decode_image(&fixture("tiny.bmp"), "tiny.bmp").unwrap_err();
        assert!(matches!(bmp, ImageError::Unsupported { .. }), "{bmp:?}");
    }

    #[test]
    fn swizzle_moves_red_and_blue() {
        let image = decode_image(&fixture("pixel.png"), "pixel.png").unwrap();
        let frame = image.render.as_bytes(0).unwrap();
        assert_eq!(frame, &[3, 2, 1, 4]);
    }

    #[gpui::test]
    fn failed_load_leaves_the_image_and_pushes_one_error_toast(cx: &mut TestAppContext) {
        let state = cx.new(|_| AppState::new());
        let notes = cx.new(|_| Notifications::new());
        let good = decode_image(&fixture("pixel.png"), "pixel.png").unwrap();
        let previous = Arc::new(good);
        state.update(cx, |state, cx| {
            state.set_image(Arc::clone(&previous), cx);
        });

        let error = decode_image(b"not an image", "notes.txt").unwrap_err();
        cx.update(|cx| {
            apply_load_result(
                &state,
                &notes,
                Err(error),
                super::LoadTarget::ReplaceSource,
                cx,
            )
        });

        let unchanged = state.read_with(cx, |state, _| {
            state
                .image
                .as_ref()
                .is_some_and(|image| Arc::ptr_eq(image, &previous))
        });
        assert!(unchanged);
        assert_eq!(notes.read_with(cx, |notes, _| notes.toast_count()), 1);
        let level_is_error = notes.read_with(cx, |notes, _| notes.last_toast_is_error());
        assert!(level_is_error);
    }
}
