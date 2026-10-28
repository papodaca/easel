use std::io::Cursor;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use easel_core::geometry::Size;
use easel_core::image_prep::{PrepError, PrepOptions, prepare_for_upload};
use easel_core::job::EncodedImage;
use gpui::{App, Context, Entity, RenderImage};
use image::{DynamicImage, ImageDecoder, ImageReader, Limits};

use crate::app_state::AppState;
use crate::notifications::{Notifications, Toast, ToastLevel};

const MAX_SIDE: u32 = 16_384;
const MAX_PIXELS: u64 = 64_000_000;
const MAX_ALLOC: u64 = 1024 * 1024 * 1024;

/// How many times an image was actually resized and encoded for upload.
/// The frame-budget test reads this to prove `render` never triggers it.
#[cfg(test)]
pub static PREPARE_MISSES: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
pub struct DecodedImage {
    pub size: Size,
    pub render: Arc<RenderImage>,
    /// Kept so a later step can re-encode without reading the file again.
    #[allow(dead_code)]
    pub rgba_for_export: Arc<DynamicImage>,
    /// Upload encodings already produced for this image, one per option set.
    /// Resizing and encoding a large image takes hundreds of milliseconds and
    /// the validation panel asks for it on every frame, so it must not repeat.
    prepared: Mutex<Vec<(PrepOptions, EncodedImage)>>,
}

impl DecodedImage {
    pub fn new(size: Size, render: Arc<RenderImage>, rgba_for_export: Arc<DynamicImage>) -> Self {
        Self {
            size,
            render,
            rgba_for_export,
            prepared: Mutex::new(Vec::new()),
        }
    }

    /// The upload encoding for `options`, computed once and reused after.
    pub fn prepare_for_upload(&self, options: PrepOptions) -> Result<EncodedImage, PrepError> {
        if let Some((_, encoded)) = self
            .prepared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .iter()
            .find(|(cached, _)| *cached == options)
        {
            return Ok(encoded.clone());
        }
        #[cfg(test)]
        PREPARE_MISSES.fetch_add(1, Ordering::SeqCst);
        // The one place the raw encoder may run; everything else goes
        // through this memo.
        #[allow(clippy::disallowed_methods)]
        let encoded = prepare_for_upload(&self.rgba_for_export, options)?;
        self.prepared
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push((options, encoded.clone()));
        Ok(encoded)
    }
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
    rgba_to_bgra(&mut rgba);
    let render = Arc::new(RenderImage::new([image::Frame::new(rgba)]));
    Ok(DecodedImage::new(size, render, rgba_for_export))
}

fn rgba_to_bgra(pixels: &mut [u8]) {
    #[cfg(target_arch = "x86_64")]
    {
        if std::arch::is_x86_feature_detected!("ssse3") {
            // SAFETY: the CPU supports SSSE3, which this routine requires.
            unsafe { swizzle_ssse3(pixels) }
            return;
        }
    }
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
}

#[cfg(target_arch = "x86_64")]
#[target_feature(enable = "ssse3")]
unsafe fn swizzle_ssse3(pixels: &mut [u8]) {
    use std::arch::x86_64::{_mm_loadu_si128, _mm_setr_epi8, _mm_shuffle_epi8, _mm_storeu_si128};

    let mask = _mm_setr_epi8(2, 1, 0, 3, 6, 5, 4, 7, 10, 9, 8, 11, 14, 13, 12, 15);
    let (chunks, rest) = pixels.as_chunks_mut::<16>();
    for chunk in chunks {
        // Each chunk is 16 bytes. These are the unaligned load and store.
        unsafe {
            let loaded = _mm_loadu_si128(chunk.as_ptr().cast());
            let swapped = _mm_shuffle_epi8(loaded, mask);
            _mm_storeu_si128(chunk.as_mut_ptr().cast(), swapped);
        }
    }
    for pixel in rest.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
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
    _target: LoadTarget,
    cx: &mut App,
) {
    if paths.is_empty() {
        return;
    }
    notifications.update(cx, |notes, cx| notes.set_status("Loading…", cx));
    let skipped = Arc::new(AtomicUsize::new(0));
    let max_images = Arc::new(AtomicUsize::new(0));
    cx.spawn(async move |cx| {
        for path in paths {
            let label = path.display().to_string();
            let read = cx
                .background_executor()
                .spawn(async move { std::fs::read(path) })
                .await;
            let bytes = match read {
                Ok(bytes) => bytes,
                Err(err) => {
                    let detail = err.to_string();
                    let skipped_note = notifications.clone();
                    cx.update(|cx| {
                        push_toast(
                            &skipped_note,
                            ToastLevel::Error,
                            ImageError::Decode {
                                label: label.clone(),
                                detail,
                            }
                            .to_string(),
                            cx,
                        );
                    });
                    continue;
                }
            };
            let origin = easel_core::session::SourceRef {
                path: label.clone(),
                hash: easel_core::session::content_hash(&bytes),
                owned: false,
            };
            let decoded = cx
                .background_executor()
                .spawn(async move { decode_image(&bytes, &label) })
                .await;
            match decoded {
                Ok(image) => {
                    let skipped = skipped.clone();
                    let max_images = max_images.clone();
                    cx.update(|cx| {
                        state.update(cx, |app, cx| {
                            max_images.store(
                                app.spec().capabilities.image_slots.max(),
                                Ordering::Relaxed,
                            );
                            if place_image(app, Arc::new(image), Some(origin), cx) == Place::Skipped
                            {
                                skipped.fetch_add(1, Ordering::Relaxed);
                            }
                        });
                    });
                }
                Err(error) => {
                    let notifications = notifications.clone();
                    cx.update(|cx| {
                        push_toast(&notifications, ToastLevel::Error, error.to_string(), cx);
                    });
                }
            }
        }
        let skipped = skipped.load(Ordering::Relaxed);
        let max = max_images.load(Ordering::Relaxed);
        cx.update(|cx| {
            state.update(cx, |app, cx| {
                app.sheet_note = if skipped == 0 {
                    None
                } else {
                    Some(format!("accepts at most {max} images, {skipped} not added"))
                };
                cx.notify();
            });
            notifications.update(cx, |notes, cx| notes.set_status("Ready", cx));
        });
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
            cx.update(|cx| apply_load_result(&state, &notifications, result, target, None, cx));
        })
        .detach();
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Source,
    Extra,
    Skipped,
}

pub fn place_image(
    state: &mut AppState,
    image: Arc<DecodedImage>,
    origin: Option<easel_core::session::SourceRef>,
    cx: &mut Context<AppState>,
) -> Place {
    let count = usize::from(state.scene.source_image.is_some()) + state.scene.references.len();
    let max = state.spec().capabilities.image_slots.max();
    if count >= max {
        return Place::Skipped;
    }
    if state.scene.source_image.is_none() {
        state.set_image(image, cx);
        state.source_ref = origin;
        Place::Source
    } else {
        let _ = state.add_extra(image, cx);
        state.extra_refs.push(origin);
        Place::Extra
    }
}

#[derive(Clone, Copy)]
pub enum LoadTarget {
    #[cfg_attr(not(test), allow(dead_code))]
    ReplaceSource,
    Paste,
}

pub fn apply_load_result(
    state: &Entity<AppState>,
    notifications: &Entity<Notifications>,
    result: Result<DecodedImage, ImageError>,
    target: LoadTarget,
    origin: Option<easel_core::session::SourceRef>,
    cx: &mut App,
) {
    match result {
        Ok(image) => {
            let width = image.size.w;
            let height = image.size.h;
            state.update(cx, |state, cx| {
                let max = state.spec().capabilities.image_slots.max();
                let placed = match target {
                    LoadTarget::ReplaceSource => {
                        state.set_image(Arc::new(image), cx);
                        state.source_ref = origin;
                        Place::Source
                    }
                    LoadTarget::Paste => place_image(state, Arc::new(image), origin, cx),
                };
                state.sheet_note = if matches!(placed, Place::Skipped) {
                    Some(format!("accepts at most {max} images, 1 not added"))
                } else {
                    None
                };
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
    fn swizzle_covers_a_simd_chunk_and_the_tail() {
        let mut pixels: Vec<u8> = (0..17u8)
            .flat_map(|i| [i, i.wrapping_add(1), i.wrapping_add(2), 255])
            .collect();
        let expected: Vec<u8> = (0..17u8)
            .flat_map(|i| [i.wrapping_add(2), i.wrapping_add(1), i, 255])
            .collect();
        super::rgba_to_bgra(&mut pixels);
        assert_eq!(pixels, expected);
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
                None,
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

    fn color_png(width: u32, height: u32, pixel: [u8; 4]) -> Vec<u8> {
        let image = image::RgbaImage::from_fn(width, height, |_, _| image::Rgba(pixel));
        let dynamic = image::DynamicImage::ImageRgba8(image);
        let mut bytes = Vec::new();
        dynamic
            .write_to(
                &mut std::io::Cursor::new(&mut bytes),
                image::ImageFormat::Png,
            )
            .unwrap();
        bytes
    }

    fn decoded(label: &str, bytes: &[u8]) -> Arc<super::DecodedImage> {
        Arc::new(decode_image(bytes, label).unwrap())
    }

    fn origin(path: &str) -> easel_core::session::SourceRef {
        easel_core::session::SourceRef {
            path: path.into(),
            hash: path.into(),
            owned: false,
        }
    }

    #[gpui::test]
    fn files_land_in_slot_order_and_the_request_follows(cx: &mut TestAppContext) {
        let state = cx.new(|_| AppState::new());
        let a = decoded("a.png", &color_png(256, 256, [1, 0, 0, 255]));
        let b = decoded("b.png", &color_png(256, 256, [0, 1, 0, 255]));
        state.update(cx, |app, cx| {
            assert_eq!(
                super::place_image(app, a, Some(origin("a.png")), cx),
                super::Place::Source
            );
            assert_eq!(
                super::place_image(app, b, Some(origin("b.png")), cx),
                super::Place::Extra
            );
        });
        let (source, extra) = state.read_with(cx, |app, _| {
            (
                app.scene.source_image.as_ref().unwrap().path.clone(),
                app.scene.references[0].path.clone(),
            )
        });
        assert_eq!(source, "source");
        assert_eq!(extra, "reference");
        let alone = cx.new(|_| AppState::new());
        let a = decoded("a.png", &color_png(256, 256, [1, 0, 0, 255]));
        alone.update(cx, |app, cx| {
            super::place_image(app, a, None, cx);
        });
        let hash_a = alone.read_with(cx, |app, _| {
            crate::jobs::build_request(app).unwrap().images[0]
                .content_hash
                .clone()
        });
        let hashes = state.read_with(cx, |app, _| {
            crate::jobs::build_request(app)
                .unwrap()
                .images
                .into_iter()
                .map(|image| image.content_hash)
                .collect::<Vec<_>>()
        });
        assert_eq!(hashes[0], hash_a);
        assert_ne!(hashes[1], hash_a);

        let c = decoded("c.png", &color_png(4, 4, [0, 0, 1, 255]));
        state.update(cx, |app, cx| {
            super::place_image(app, c, Some(origin("c.png")), cx);
        });
        let paths = state.read_with(cx, |app, _| {
            (
                app.scene.source_image.as_ref().unwrap().path.clone(),
                app.scene.references.len(),
            )
        });
        assert_eq!(paths.0, "source");
        assert_eq!(paths.1, 2);
    }

    #[gpui::test]
    fn a_load_past_the_max_names_the_skip_and_remove_clears_it(cx: &mut TestAppContext) {
        let state = cx.new(|_| AppState::new());
        let max = state.read_with(cx, |app, _| app.spec().capabilities.image_slots.max());
        state.update(cx, |app, cx| {
            for index in 0..max {
                let image = decoded("slot.png", &color_png(2, 2, [index as u8, 0, 0, 255]));
                super::place_image(app, image, None, cx);
            }
            let extra = decoded("past.png", &color_png(2, 2, [9, 9, 9, 255]));
            assert_eq!(
                super::place_image(app, extra, None, cx),
                super::Place::Skipped
            );
            app.sheet_note = Some(format!("accepts at most {max} images, 1 not added"));
        });
        let (count, note) = state.read_with(cx, |app, _| {
            (
                usize::from(app.scene.source_image.is_some()) + app.scene.references.len(),
                app.sheet_note.clone().unwrap(),
            )
        });
        assert_eq!(count, max);
        assert!(
            note.contains(&format!("accepts at most {max} images")),
            "{note}"
        );
        assert!(note.contains('1'), "{note}");
        state.update(cx, |app, cx| app.remove_reference_slot(0, cx));
        assert!(state.read_with(cx, |app, _| app.sheet_note.is_none()));
    }

    #[gpui::test]
    fn a_bad_file_between_two_good_ones_keeps_both(cx: &mut TestAppContext) {
        let state = cx.new(|_| AppState::new());
        let notes = cx.new(|_| Notifications::new());
        let a = decoded("a.png", &color_png(2, 2, [1, 0, 0, 255]));
        let b = decoded("b.png", &color_png(3, 3, [0, 1, 0, 255]));
        state.update(cx, |app, cx| {
            super::place_image(app, a, None, cx);
        });
        let error = decode_image(b"not an image", "bad.png").unwrap_err();
        cx.update(|cx| {
            apply_load_result(
                &state,
                &notes,
                Err(error),
                super::LoadTarget::Paste,
                None,
                cx,
            );
        });
        state.update(cx, |app, cx| {
            super::place_image(app, b, None, cx);
        });
        assert_eq!(notes.read_with(cx, |notes, _| notes.toast_count()), 1);
        let count = state.read_with(cx, |app, _| {
            usize::from(app.scene.source_image.is_some()) + app.scene.references.len()
        });
        assert_eq!(count, 2);
    }

    #[gpui::test]
    fn paste_sets_the_source_ref_only_for_the_first_image(cx: &mut TestAppContext) {
        let state = cx.new(|_| AppState::new());
        let first = decoded("a.png", &color_png(2, 2, [1, 0, 0, 255]));
        let extra = decoded("b.png", &color_png(3, 3, [0, 1, 0, 255]));
        state.update(cx, |app, cx| {
            super::place_image(app, first, Some(origin("a.png")), cx);
            super::place_image(app, extra, Some(origin("b.png")), cx);
        });
        let (source, extra_ref) = state.read_with(cx, |app, _| {
            (
                app.source_ref.as_ref().unwrap().path.clone(),
                app.extra_refs[0].as_ref().unwrap().path.clone(),
            )
        });
        assert_eq!(source, "a.png");
        assert_eq!(extra_ref, "b.png");
    }

    #[gpui::test]
    fn remove_on_the_only_tile_clears_the_source(cx: &mut TestAppContext) {
        let state = cx.new(|_| AppState::new());
        let image = decoded("a.png", &color_png(2, 2, [1, 0, 0, 255]));
        state.update(cx, |app, cx| {
            super::place_image(app, image, Some(origin("a.png")), cx);
            app.remove_reference_slot(0, cx);
        });
        let cleared = state.read_with(cx, |app, _| {
            app.image.is_none() && app.scene.source_image.is_none() && app.source_ref.is_none()
        });
        assert!(cleared);
    }

    #[gpui::test]
    fn move_right_swaps_pixels_refs_and_keeps_rectangles(cx: &mut TestAppContext) {
        let state = cx.new(|_| AppState::new());
        let left = decoded("a.png", &color_png(2, 2, [1, 0, 0, 255]));
        let right = decoded("b.png", &color_png(4, 2, [0, 0, 1, 255]));
        state.update(cx, |app, cx| {
            super::place_image(app, left, Some(origin("a.png")), cx);
            super::place_image(app, right, Some(origin("b.png")), cx);
            let spot = easel_core::geometry::Rect {
                x: 10,
                y: 10,
                w: 10,
                h: 10,
            };
            app.scene.add_box(spot, easel_core::scene::BoxKind::Keep);
            app.scene
                .add_box_on(spot, easel_core::scene::BoxKind::Keep, 1);
            app.scene.prose = "<ref_image_0> <ref_image_1>".into();
            app.move_slot(0, 1, cx);
        });
        let (source_ref, extra_ref, prose, rects) = state.read_with(cx, |app, _| {
            (
                app.source_ref.as_ref().unwrap().path.clone(),
                app.extra_refs[0].as_ref().unwrap().path.clone(),
                app.scene.prose.clone(),
                app.scene
                    .boxes
                    .iter()
                    .map(|element| element.src.unwrap())
                    .collect::<Vec<_>>(),
            )
        });
        assert_eq!(source_ref, "b.png");
        assert_eq!(extra_ref, "a.png");
        assert_eq!(prose, "<ref_image_1> <ref_image_0>");
        assert!(rects.iter().all(|rect| rect.x == 10 && rect.y == 10));
        let width = state.read_with(cx, |app, _| app.image.as_ref().unwrap().size.w);
        assert_eq!(width, 4);
    }
}
